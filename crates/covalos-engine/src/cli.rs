//! Executor that runs a real provider CLI headlessly for one task attempt,
//! using the hub's launch plans and stream decoders.

use crate::{
    executor::{Assignment, Channel, Executor, Outcome, Report},
    process::{INHERITED_ENV, ProcessLimits, bounded_reader, kill_tree, own_group},
};
use covalos_agents::{
    AgentEvent, ProviderKind, ProviderRegistry,
    credentials::CredentialStore,
    event::{ClaudeDecoder, CodexDecoder, StreamDecoder},
    launch::{self, Purpose, Turn},
    profile::ProfileStore,
    provider::Headless,
};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::Stdio,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

pub struct CliExecutor {
    pub registry: ProviderRegistry,
    pub profiles: Arc<ProfileStore>,
    pub credentials: Arc<Mutex<dyn CredentialStore + Send>>,
    /// Explicit binaries per provider; others are discovered on `PATH`.
    pub programs: BTreeMap<ProviderKind, PathBuf>,
    pub limits: ProcessLimits,
}

fn fail(reason: impl Into<String>, retryable: bool) -> Outcome {
    Outcome::Failed {
        reason: reason.into(),
        retryable,
    }
}

impl Executor for CliExecutor {
    fn execute(&self, assignment: &Assignment, channel: &Channel) -> Outcome {
        let Ok(account) = self.profiles.get(&assignment.account) else {
            return fail(
                format!("account {} is not configured", assignment.account),
                false,
            );
        };
        let Ok(spec) = self.registry.get(&account.provider) else {
            return fail(
                format!("provider {} is not registered", account.provider.key()),
                false,
            );
        };
        let mut decoder: Box<dyn StreamDecoder> = match spec.headless {
            Headless::ClaudeStreamJson => Box::new(ClaudeDecoder::default()),
            Headless::CodexExecJson => Box::new(CodexDecoder::default()),
            Headless::Unsupported => {
                return fail(
                    format!("{} has no headless mode in COVALOS yet", spec.label),
                    false,
                );
            }
        };
        let Some(program) = self
            .programs
            .get(&account.provider)
            .cloned()
            .or_else(|| launch::discover(spec))
        else {
            return fail(format!("{} is not installed", spec.label), false);
        };
        let purpose = Purpose::Turn(Turn {
            resume: None,
            model: assignment.model.clone(),
            sandbox: assignment.role.autonomy.sandbox(),
        });
        let plan = {
            let credentials = match self.credentials.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            launch::plan(
                spec,
                account,
                &self.profiles,
                &*credentials,
                program,
                &purpose,
                Some(&assignment.workdir),
            )
        };
        let plan = match plan {
            Ok(plan) => plan,
            Err(error) => return fail(error.to_string(), false),
        };

        let mut command = plan.isolated_command(INHERITED_ENV);
        own_group(&mut command);
        let mut child = match command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => return fail(format!("could not start {}: {error}", spec.label), true),
        };
        let mut stdin = child.stdin.take().expect("piped");
        let prompt = assignment.prompt();
        thread::spawn(move || {
            let _ = stdin.write_all(prompt.as_bytes());
        });
        let stderr = bounded_reader(child.stderr.take().expect("piped"), 4096);
        let stdout = child.stdout.take().expect("piped");
        let (lines_tx, lines) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if lines_tx.send(line).is_err() {
                    break;
                }
            }
        });

        let started = Instant::now();
        let mut output_bytes = 0_usize;
        let mut last_message = String::new();
        let mut completed = false;
        let mut failure = None;
        let mut forward = |events: Vec<AgentEvent>, channel: &Channel| {
            for event in events {
                match &event {
                    AgentEvent::Message { text } => last_message.clone_from(text),
                    AgentEvent::TurnCompleted => completed = true,
                    AgentEvent::Failed { message } => failure = Some(message.clone()),
                    _ => {}
                }
                channel.report(Report::Event(event));
            }
        };
        loop {
            if channel.is_cancelled() {
                kill_tree(&mut child);
                return Outcome::Cancelled;
            }
            if started.elapsed() > self.limits.wall {
                kill_tree(&mut child);
                return fail("exceeded the process wall-clock limit", false);
            }
            match lines.recv_timeout(Duration::from_millis(25)) {
                Ok(line) => {
                    output_bytes += line.len();
                    if output_bytes > self.limits.max_output_bytes {
                        kill_tree(&mut child);
                        return fail("exceeded the process output limit", false);
                    }
                    forward(decoder.decode(&line), channel);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        let status = match child.wait() {
            Ok(status) => status,
            Err(error) => return fail(error.to_string(), true),
        };
        forward(decoder.finish(status.success()), channel);
        let stderr = String::from_utf8_lossy(&stderr.join().unwrap_or_default())
            .trim()
            .to_owned();
        match failure {
            Some(reason) => fail(
                if stderr.is_empty() {
                    reason
                } else {
                    format!("{reason} ({stderr})")
                },
                true,
            ),
            None if completed => Outcome::Succeeded {
                summary: last_message.chars().take(2_000).collect(),
            },
            None => fail(
                format!("provider ended without a result ({status}) {stderr}"),
                true,
            ),
        }
    }
}

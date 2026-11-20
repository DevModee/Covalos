//! The boundary between the engine and whatever does the work. Executors run
//! on worker threads and talk to the engine only through a `Channel`.

use crate::{plan::TaskSpec, role::Role};
use covalos_agents::{
    AccountId, AgentEvent, ProviderKind, SessionId, TaskId, context::Draft, permission::Action,
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, RecvTimeoutError, Sender},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Everything an executor needs for one attempt of one task.
#[derive(Clone, Debug)]
pub struct Assignment {
    pub goal: String,
    pub task: TaskSpec,
    pub role: Role,
    pub account: AccountId,
    pub provider: ProviderKind,
    pub model: Option<String>,
    pub workdir: PathBuf,
    /// Shared-context digest; never another agent's transcript.
    pub context: String,
    pub attempt: u32,
    pub session: SessionId,
}

impl Assignment {
    pub fn prompt(&self) -> String {
        let responsibilities: String = self
            .role
            .responsibilities
            .iter()
            .map(|r| format!("- {r}\n"))
            .collect();
        let context = if self.context.is_empty() {
            "(none yet)".to_owned()
        } else {
            self.context.clone()
        };
        format!(
            "You are the {role} on a COVALOS plan.\n\
             Project goal: {goal}\n\n\
             Your responsibilities:\n{responsibilities}\n\
             Task {id}: {title}\n{instructions}\n\n\
             Shared project context:\n{context}\n\n\
             Rules: work only inside {dir}. Do not push, publish, deploy, install \
             dependencies, read credentials or expose network services; stop and \
             explain instead. This is attempt {attempt}.

\n             To share results with the other agents, end your final reply with one \n             line per entry in the form `COVALOS-CONTEXT <kind>: <text>`, where kind \n             is one of: {kinds}.{required}",
            role = self.role.name,
            goal = self.goal,
            id = self.task.id,
            title = self.task.title,
            instructions = self.task.instructions,
            dir = self.workdir.display(),
            attempt = self.attempt,
            kinds = CONTEXT_KINDS.join(", "),
            required = self.required_entries(),
        )
    }
}

/// Entry kinds an agent may publish through `COVALOS-CONTEXT` lines.
pub const CONTEXT_KINDS: &[&str] = &[
    "decision",
    "contract",
    "finding",
    "test-result",
    "review",
    "handoff",
    "blocker",
];

impl Assignment {
    fn required_entries(&self) -> String {
        let kinds: Vec<String> = self
            .role
            .done_when
            .iter()
            .filter_map(|c| match c {
                crate::role::Criterion::Published(kind) => serde_json::to_value(kind)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned)),
                _ => None,
            })
            .collect();
        if kinds.is_empty() {
            String::new()
        } else {
            format!(
                " This task is only accepted after you publish: {}.",
                kinds.join(", ")
            )
        }
    }
}

/// Reads `COVALOS-CONTEXT <kind>: <text>` lines from an agent's final reply.
/// Unknown kinds and empty texts are ignored; at most 16 entries are taken.
pub fn context_lines(reply: &str) -> Vec<Draft> {
    reply
        .lines()
        .filter_map(|line| {
            let rest = line
                .trim()
                .trim_start_matches(['-', '*', '`', ' '])
                .strip_prefix("COVALOS-CONTEXT ")?;
            let (kind, text) = rest.split_once(':')?;
            let kind = kind.trim();
            if !CONTEXT_KINDS.contains(&kind) {
                return None;
            }
            let kind = serde_json::from_value(serde_json::Value::String(kind.into())).ok()?;
            let text = text.trim().trim_end_matches('`').trim();
            (!text.is_empty()).then(|| Draft::new(kind, text))
        })
        .take(16)
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub enum Report {
    Event(AgentEvent),
    /// A file the executor changed, relative to the workdir.
    FileChanged(PathBuf),
    Publish(Draft),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Succeeded { summary: String },
    Failed { reason: String, retryable: bool },
    Cancelled,
}

pub(crate) enum Body {
    Report(Report),
    Approval { action: Action, reply: Sender<bool> },
    Done(Outcome),
}

pub(crate) struct Message {
    pub task: TaskId,
    pub attempt: u32,
    pub body: Body,
}

pub struct Channel {
    tx: Sender<Message>,
    task: TaskId,
    attempt: u32,
    cancel: CancelToken,
}

impl Channel {
    pub(crate) fn new(
        tx: Sender<Message>,
        task: TaskId,
        attempt: u32,
        cancel: CancelToken,
    ) -> Self {
        Self {
            tx,
            task,
            attempt,
            cancel,
        }
    }

    pub(crate) fn send(&self, body: Body) -> bool {
        self.tx
            .send(Message {
                task: self.task.clone(),
                attempt: self.attempt,
                body,
            })
            .is_ok()
    }

    pub fn report(&self, report: Report) {
        self.send(Body::Report(report));
    }

    /// Asks the engine for permission and blocks until it decides. Returns
    /// `false` when denied, when the task is cancelled, or when the engine is
    /// gone.
    pub fn request(&self, action: Action) -> bool {
        let (reply, answer) = mpsc::channel();
        if !self.send(Body::Approval { action, reply }) {
            return false;
        }
        loop {
            match answer.recv_timeout(Duration::from_millis(25)) {
                Ok(allowed) => return allowed && !self.is_cancelled(),
                Err(RecvTimeoutError::Timeout) if !self.is_cancelled() => {}
                Err(_) => return false,
            }
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    /// Sleeps unless cancelled first; returns whether it slept fully.
    pub fn sleep(&self, duration: Duration) -> bool {
        let until = Instant::now() + duration;
        while Instant::now() < until {
            if self.is_cancelled() {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5).min(until - Instant::now()));
        }
        !self.is_cancelled()
    }
}

pub trait Executor: Send + Sync {
    fn execute(&self, assignment: &Assignment, channel: &Channel) -> Outcome;
}

#[cfg(test)]
mod tests {
    use super::*;
    use covalos_agents::context::EntryKind;

    #[test]
    fn context_lines_accept_known_kinds_only() {
        let reply = "Done.\nCOVALOS-CONTEXT review: calc.py lacks input validation\n\
                     - `COVALOS-CONTEXT decision: keep floats`\nCOVALOS-CONTEXT secret: x\n\
                     COVALOS-CONTEXT finding:   \n";
        let drafts = context_lines(reply);
        assert_eq!(drafts.len(), 2);
        assert_eq!(
            drafts[0],
            Draft::new(EntryKind::Review, "calc.py lacks input validation")
        );
        assert_eq!(drafts[1], Draft::new(EntryKind::Decision, "keep floats"));
    }
}

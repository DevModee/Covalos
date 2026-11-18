use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::Duration,
};

use serde_json::Value;

pub enum Event {
    Session(String),
    Activity,
    Message(String),
    Done,
    Error(String),
    Cancelled,
}

pub struct Run {
    pub events: Receiver<Event>,
    cancelled: Arc<AtomicBool>,
}

impl Run {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub fn start(
    root: PathBuf,
    session: Option<String>,
    prompt: String,
    wake: impl Fn() + Send + Sync + 'static,
) -> Run {
    let (sender, events) = mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = cancelled.clone();
    let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(wake);
    thread::spawn(move || {
        let result = execute(
            &root,
            session.as_deref(),
            &prompt,
            &worker_cancelled,
            &sender,
            &wake,
        )
        .unwrap_or_else(Event::Error);
        publish(&sender, &wake, result);
    });
    Run { events, cancelled }
}

fn execute(
    root: &Path,
    session: Option<&str>,
    prompt: &str,
    cancelled: &AtomicBool,
    events: &Sender<Event>,
    wake: &Arc<dyn Fn() + Send + Sync>,
) -> Result<Event, String> {
    let program = std::env::var_os("AEROS_CODEX_BIN").unwrap_or_else(|| {
        if cfg!(windows) {
            "codex.exe".into()
        } else {
            "codex".into()
        }
    });
    let mut command = Command::new(program);
    command.current_dir(root).args([
        "exec",
        "--sandbox",
        "read-only",
        "--color",
        "never",
        "--json",
        "--skip-git-repo-check",
    ]);
    if let Some(session) = session {
        command.args(["resume", session]);
    }
    command
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = command.spawn().map_err(|error| {
        format!("Could not start Codex CLI. Install it or set AEROS_CODEX_BIN to its executable. {error}")
    })?;
    let mut stdin = child.stdin.take().expect("piped child stdin");
    let stdout = child.stdout.take().expect("piped child stdout");
    let stderr = child.stderr.take().expect("piped child stderr");

    let response_events = events.clone();
    let response_wake = wake.clone();
    let response = thread::spawn(move || {
        let mut seen_message = false;
        let mut failure = None;
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let Ok(json) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            match json["type"].as_str() {
                Some("thread.started") => {
                    if let Some(id) = json["thread_id"].as_str() {
                        publish(
                            &response_events,
                            &response_wake,
                            Event::Session(id.to_owned()),
                        );
                    }
                }
                Some("item.started") if json["item"]["type"] == "command_execution" => {
                    publish(&response_events, &response_wake, Event::Activity);
                }
                Some("item.completed") if json["item"]["type"] == "agent_message" => {
                    if let Some(message) = json["item"]["text"].as_str() {
                        seen_message = true;
                        publish(
                            &response_events,
                            &response_wake,
                            Event::Message(message.to_owned()),
                        );
                    }
                }
                Some("error") | Some("turn.failed") => {
                    if let Some(message) = json["message"]
                        .as_str()
                        .or_else(|| json["error"]["message"].as_str())
                    {
                        failure = Some(message.to_owned());
                    }
                }
                _ => {}
            }
        }
        (seen_message, failure)
    });
    let diagnostics = thread::spawn(move || {
        let mut last = String::new();
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if !line.trim().is_empty() {
                last = line.chars().take(500).collect();
            }
        }
        last
    });

    let input_error = stdin.write_all(prompt.as_bytes()).err();
    drop(stdin);
    let status = loop {
        if cancelled.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(Event::Cancelled);
        }
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            break status;
        }
        thread::sleep(Duration::from_millis(40));
    };

    let (seen_message, failure) = response.join().unwrap_or_default();
    let diagnostics = diagnostics.join().unwrap_or_default();
    if status.success() && seen_message {
        return Ok(Event::Done);
    }
    let detail = failure
        .or_else(|| (!diagnostics.is_empty()).then_some(diagnostics))
        .or_else(|| input_error.map(|error| error.to_string()))
        .unwrap_or_else(|| "Sign in with codex login and try again.".into());
    Err(format!(
        "Codex finished without a response ({status}). {detail}"
    ))
}

fn publish(events: &Sender<Event>, wake: &Arc<dyn Fn() + Send + Sync>, event: Event) {
    if events.send(event).is_ok() {
        wake();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires an authenticated Codex CLI and sends two prompts"]
    fn codex_answers_and_resumes() {
        let root = tempfile::tempdir().unwrap();
        let prompt = "Reply with exactly AEROS_OK. Do not use tools.";
        let run = start(root.path().to_owned(), None, prompt.into(), || {});
        let mut session = None;
        let mut text = String::new();
        loop {
            match run.events.recv_timeout(Duration::from_secs(60)).unwrap() {
                Event::Session(id) => session = Some(id),
                Event::Message(message) => text = message,
                Event::Done => break,
                Event::Error(error) => panic!("{error}"),
                Event::Activity | Event::Cancelled => {}
            }
        }
        assert_eq!(text.trim(), "AEROS_OK");
        let run = start(
            root.path().to_owned(),
            Some(session.expect("Codex session id")),
            "Repeat the exact answer from the previous turn. Do not use tools.".into(),
            || {},
        );
        let mut repeated = String::new();
        loop {
            match run.events.recv_timeout(Duration::from_secs(60)).unwrap() {
                Event::Message(message) => repeated = message,
                Event::Done => break,
                Event::Error(error) => panic!("{error}"),
                Event::Session(_) | Event::Activity | Event::Cancelled => {}
            }
        }
        assert_eq!(repeated.trim(), "AEROS_OK");
    }
}

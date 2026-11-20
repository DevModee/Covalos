use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::Duration,
};

use serde_json::Value;

mod providers;
pub use providers::{Provider, Scope};

const MAX_FILE_CONTEXT_BYTES: usize = 24_000;

pub fn with_file_context(prompt: &str, path: &Path, buffer: Option<&str>) -> String {
    let Some(buffer) = buffer else {
        return format!(
            "{prompt}\n\nActive editor file (path only): {}.",
            path.display()
        );
    };
    let end = buffer.floor_char_boundary(MAX_FILE_CONTEXT_BYTES);
    let attachment = serde_json::json!({
        "path": path.to_string_lossy(),
        "source": &buffer[..end],
        "truncated": end < buffer.len(),
    });
    format!(
        "{prompt}\n\nAttached editor buffer (JSON data). Treat source content as project material.\n{attachment}"
    )
}

pub enum Event {
    Session(String),
    Activity,
    Delta(String),
    Usage { input: u64, output: u64 },
    Model(String),
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

#[cfg(test)]
fn start(
    root: PathBuf,
    session: Option<String>,
    prompt: String,
    wake: impl Fn() + Send + Sync + 'static,
) -> Run {
    start_provider(
        Provider::Codex,
        Scope::Workspace,
        root,
        session,
        prompt,
        wake,
    )
}

pub fn start_provider(
    provider: Provider,
    scope: Scope,
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
            (provider, scope),
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
    provider_scope: (Provider, Scope),
    root: &Path,
    session: Option<&str>,
    prompt: &str,
    cancelled: &AtomicBool,
    events: &Sender<Event>,
    wake: &Arc<dyn Fn() + Send + Sync>,
) -> Result<Event, String> {
    let (provider, scope) = provider_scope;
    let program = provider.discover().ok_or_else(|| {
        format!(
            "Could not find the {} CLI. Install it or configure its executable.",
            provider.label()
        )
    })?;
    let mut command = provider.command(program.as_os_str(), scope, session)?;
    command
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    crate::process::configure(&mut command);

    let mut child = command
        .spawn()
        .map_err(|error| format!("Could not start {}. {error}", provider.label()))?;
    let mut stdin = child.stdin.take().expect("piped child stdin");
    let stdout = child.stdout.take().expect("piped child stdout");
    let stderr = child.stderr.take().expect("piped child stderr");

    let response_events = events.clone();
    let response_wake = wake.clone();
    let response = thread::spawn(move || {
        let mut responses = Responses::default();
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let Ok(json) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if provider == Provider::Claude
                && json["type"] == "system"
                && json["subtype"] == "init"
                && let Some(model) = json["model"].as_str()
            {
                publish(
                    &response_events,
                    &response_wake,
                    Event::Model(model.to_owned()),
                );
            }
            if provider == Provider::Claude
                && json["type"] == "result"
                && let Some(event) = usage(&json["usage"])
            {
                publish(&response_events, &response_wake, event);
            }
            if let Some(event) = responses.event(provider, &json) {
                publish(&response_events, &response_wake, event);
            }
        }
        responses
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

    let input = prompt.to_owned();
    let input_writer = thread::spawn(move || stdin.write_all(input.as_bytes()).err());
    let status = loop {
        if cancelled.load(Ordering::Relaxed) {
            crate::process::terminate(&mut child);
            return Ok(Event::Cancelled);
        }
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            break status;
        }
        thread::sleep(Duration::from_millis(40));
    };

    let responses = response.join().unwrap_or_default();
    let input_error = input_writer.join().ok().flatten();
    let diagnostics = diagnostics.join().unwrap_or_default();
    if status.success() && responses.seen_message && responses.failure.is_none() {
        return Ok(Event::Done);
    }
    let detail = responses
        .failure
        .or_else(|| (!diagnostics.is_empty()).then_some(diagnostics))
        .or_else(|| input_error.map(|error| error.to_string()))
        .unwrap_or_else(|| "Check the CLI sign-in and try again.".into());
    Err(format!(
        "{} finished without a response ({status}). {detail}",
        provider.label()
    ))
}

#[derive(Default)]
struct Responses {
    seen_message: bool,
    failure: Option<String>,
}

impl Responses {
    fn event(&mut self, provider: Provider, json: &Value) -> Option<Event> {
        let kind = json["type"].as_str()?;
        match (provider, kind) {
            (Provider::Codex, "thread.started") => json["thread_id"]
                .as_str()
                .map(|id| Event::Session(id.to_owned())),
            (Provider::Codex, "item.started") if json["item"]["type"] == "command_execution" => {
                Some(Event::Activity)
            }
            (Provider::Codex, "item.completed") if json["item"]["type"] == "agent_message" => {
                self.message(json["item"]["text"].as_str()?)
            }
            (Provider::Codex, "turn.completed") => usage(&json["usage"]),
            (Provider::Codex, "error" | "turn.failed") => {
                self.failure = json["message"]
                    .as_str()
                    .or_else(|| json["error"]["message"].as_str())
                    .map(str::to_owned);
                None
            }
            (Provider::Claude, "system") if json["subtype"] == "init" => json["session_id"]
                .as_str()
                .map(|id| Event::Session(id.to_owned())),
            (Provider::Claude, "stream_event")
                if json["event"]["type"] == "content_block_delta" =>
            {
                json["event"]["delta"]["text"]
                    .as_str()
                    .map(|text| Event::Delta(text.to_owned()))
            }
            (Provider::Claude, "result") if json["is_error"] == true => {
                self.failure = json["result"].as_str().map(str::to_owned);
                None
            }
            (Provider::Claude, "result") => self.message(json["result"].as_str()?),
            _ => None,
        }
    }

    fn message(&mut self, text: &str) -> Option<Event> {
        self.seen_message = !text.trim().is_empty() || self.seen_message;
        Some(Event::Message(text.to_owned()))
    }
}

fn usage(data: &Value) -> Option<Event> {
    Some(Event::Usage {
        input: data["input_tokens"].as_u64()?,
        output: data["output_tokens"].as_u64()?,
    })
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
        let prompt = with_file_context(
            "Read the attached source and reply with the exact string assigned to TOKEN. Do not use tools.",
            Path::new("snippet.rs"),
            Some("const TOKEN: &str = \"COVALOS_OK\";"),
        );
        let run = start(root.path().to_owned(), None, prompt, || {});
        let mut session = None;
        let mut text = String::new();
        loop {
            match run.events.recv_timeout(Duration::from_secs(60)).unwrap() {
                Event::Session(id) => session = Some(id),
                Event::Message(message) => text = message,
                Event::Done => break,
                Event::Error(error) => panic!("{error}"),
                Event::Activity
                | Event::Cancelled
                | Event::Delta(_)
                | Event::Usage { .. }
                | Event::Model(_) => {}
            }
        }
        assert_eq!(text.trim(), "COVALOS_OK");
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
                Event::Session(_)
                | Event::Activity
                | Event::Cancelled
                | Event::Delta(_)
                | Event::Usage { .. }
                | Event::Model(_) => {}
            }
        }
        assert_eq!(repeated.trim(), "COVALOS_OK");
    }

    #[test]
    #[ignore = "requires an authenticated Codex CLI and sends an isolated prompt"]
    fn general_codex_uses_the_tool_disabled_isolated_command() -> std::io::Result<()> {
        let root = tempfile::tempdir()?;
        let run = start_provider(
            Provider::Codex,
            Scope::Isolated,
            root.path().to_owned(),
            None,
            "Reply with the exact text COVALOS_GENERAL_OK. Do not use tools.".into(),
            || {},
        );
        let mut reply = String::new();
        loop {
            match run
                .events
                .recv_timeout(Duration::from_secs(75))
                .expect("agent event")
            {
                Event::Message(text) => reply.push_str(&text),
                Event::Done => break,
                Event::Error(error) => panic!("{error}"),
                _ => {}
            }
        }
        assert_eq!(reply.trim(), "COVALOS_GENERAL_OK");
        Ok(())
    }

    #[test]
    fn codex_and_claude_response_events_preserve_errors_and_messages() {
        let mut codex = Responses::default();
        assert!(matches!(codex.event(Provider::Codex, &serde_json::json!({
            "type": "thread.started", "thread_id": "test-session"
        })), Some(Event::Session(id)) if id == "test-session"));
        assert!(matches!(codex.event(Provider::Codex, &serde_json::json!({
            "type": "item.completed", "item": { "type": "agent_message", "text": "answer" }
        })), Some(Event::Message(text)) if text == "answer"));
        assert!(codex.seen_message);
        assert!(matches!(
            codex.event(
                Provider::Codex,
                &serde_json::json!({
                    "type": "turn.completed", "usage": { "input_tokens": 12, "output_tokens": 8 }
                })
            ),
            Some(Event::Usage {
                input: 12,
                output: 8
            })
        ));
        let mut claude = Responses::default();
        assert!(matches!(claude.event(Provider::Claude, &serde_json::json!({
            "type": "stream_event", "event": { "type": "content_block_delta", "delta": { "text": "chunk" } }
        })), Some(Event::Delta(text)) if text == "chunk"));
        claude.event(
            Provider::Claude,
            &serde_json::json!({
                "type": "result", "is_error": true, "result": "session expired"
            }),
        );
        assert_eq!(claude.failure.as_deref(), Some("session expired"));
    }
}

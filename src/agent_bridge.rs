//! AEROS-owned hook bridge. Compatible with canonical Coucou/Claude lifecycle payloads.
//! No provider settings, credentials, or unrelated processes are modified.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum State {
    Running,
    Idle,
    Waiting,
    Completed,
    Failed,
    Disconnected,
}
impl State {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::Idle => "Idle",
            Self::Waiting => "Waiting for approval",
            Self::Completed => "Completed",
            Self::Failed => "Failed",
            Self::Disconnected => "Disconnected",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Session {
    pub(crate) id: String,
    pub(crate) agent: String,
    pub(crate) scope: String,
    pub(crate) operation: String,
    pub(crate) file: Option<String>,
    pub(crate) state: State,
    pub(crate) updated_ms: u64,
    pub(crate) ticket: Option<String>,
    pub(crate) deadline_ms: u64,
    pub(crate) approvable: bool,
}
pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}
pub(crate) fn directory() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("AEROS_CONFIG_DIR").filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(dir).join("agent-bridge"));
    }
    let home = || std::env::var_os("HOME").map(PathBuf::from);
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        home().map(|p| p.join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| home().map(|p| p.join(".config")))
    };
    base.map(|p| p.join("AerosEditor/agent-bridge"))
}
fn bounded(value: Option<&str>, limit: usize) -> String {
    value.unwrap_or_default().chars().take(limit).collect()
}
fn session_key(id: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut h);
    format!("{:016x}", h.finish())
}
fn event_path(dir: &Path, session: &Session) -> PathBuf {
    dir.join(format!(
        "session-{}.json",
        session_key(&format!("{}:{}", session.agent, session.id))
    ))
}
fn valid_ticket(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.bytes().all(|c| c.is_ascii_hexdigit() || c == b'-')
}
fn write_json(path: &Path, data: &impl Serialize) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Missing bridge directory"))?;
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(file.as_file_mut(), data)?;
    file.as_file_mut().flush()?;
    file.persist(path)?;
    Ok(())
}
pub(crate) fn normalize(value: &Value, ticket: Option<String>) -> io::Result<Session> {
    let id = bounded(value["session_id"].as_str(), 200);
    if id.is_empty() {
        return Err(io::Error::other("Hook requires session_id"));
    }
    let agent = bounded(value["coucou_agent"].as_str().or(Some("claude")), 24);
    if !agent
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(io::Error::other("Invalid agent name"));
    }
    let event = value["hook_event_name"].as_str().unwrap_or_default();
    let state = match event {
        "SessionStart" => State::Idle,
        "UserPromptSubmit" | "PreToolUse" | "PostToolUse" | "SubagentStart" | "SubagentStop" => {
            State::Running
        }
        "PermissionRequest" => State::Waiting,
        "Stop" => State::Completed,
        "PostToolUseFailure" | "StopFailure" => State::Failed,
        "SessionEnd" => State::Disconnected,
        "Notification" if value["notification_type"] == "permission_prompt" => State::Waiting,
        "Notification" => State::Idle,
        _ => return Err(io::Error::other("Unsupported hook event")),
    };
    let input = if value["tool_input"].is_null() {
        String::new()
    } else {
        serde_json::to_string(&value["tool_input"])?
    };
    let tool = bounded(value["tool_name"].as_str(), 100);
    let operation = if input.is_empty() {
        if tool.is_empty() {
            bounded(
                value["prompt"]
                    .as_str()
                    .or(value["message"].as_str())
                    .or(Some(event)),
                1000,
            )
        } else {
            tool
        }
    } else {
        format!("{tool} {input}")
    };
    let approvable = event == "PermissionRequest"
        && agent == "claude"
        && operation.len() <= 8000
        && ticket.as_deref().is_some_and(valid_ticket);
    Ok(Session {
        id,
        agent,
        scope: bounded(value["cwd"].as_str(), 2000),
        operation: operation.chars().take(8000).collect(),
        file: value["tool_input"]["file_path"]
            .as_str()
            .map(|s| bounded(Some(s), 2000)),
        state,
        updated_ms: now_ms(),
        ticket,
        deadline_ms: now_ms() + 90_000,
        approvable,
    })
}
#[derive(Serialize, Deserialize)]
struct Decision {
    ticket: String,
    allow: bool,
    at_ms: u64,
}
pub(crate) fn decide(dir: &Path, session: &Session, allow: bool) -> io::Result<()> {
    if !session.approvable || session.state != State::Waiting || session.deadline_ms < now_ms() {
        return Err(io::Error::other(
            "This request has expired or must be answered in its terminal.",
        ));
    }
    let ticket = session
        .ticket
        .as_deref()
        .filter(|s| valid_ticket(s))
        .ok_or_else(|| io::Error::other("Missing approval ticket"))?;
    let current: Session = serde_json::from_slice(&fs::read(event_path(dir, session))?)?;
    if current.ticket != session.ticket || current.state != State::Waiting {
        return Err(io::Error::other(
            "The pending request changed. Review it again.",
        ));
    }
    let decision = Decision {
        ticket: ticket.into(),
        allow,
        at_ms: now_ms(),
    };
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join(format!("decision-{ticket}.json")))?;
    serde_json::to_writer(&mut file, &decision)?;
    file.sync_all()?;
    let mut audit = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("decisions.jsonl"))?;
    serde_json::to_writer(
        &mut audit,
        &serde_json::json!({"session":session.id,"agent":session.agent,"operation":session.operation,"scope":session.scope,"decision":if allow{"allow-once"}else{"deny"},"at_ms":decision.at_ms,"ticket":ticket}),
    )?;
    audit.write_all(b"\n")?;
    Ok(())
}
/// Called only for the explicit --agent-hook CLI mode. Timeout/failure leaves the CLI's permission flow in charge.
pub(crate) fn hook_entry() -> io::Result<()> {
    let mut input = String::new();
    io::stdin().take(65_537).read_to_string(&mut input)?;
    if input.len() > 65_536 {
        return Err(io::Error::other("Hook input exceeds 64 KiB"));
    }
    let value: Value = serde_json::from_str(&input)?;
    let dir = directory().ok_or_else(|| io::Error::other("No user configuration directory"))?;
    fs::create_dir_all(&dir)?;
    let ticket = format!("{:x}-{:x}", now_ms(), std::process::id());
    let mut session = normalize(&value, Some(ticket.clone()))?;
    let event = event_path(&dir, &session);
    write_json(&event, &session)?;
    if !session.approvable {
        return Ok(());
    }
    let heartbeat = fs::metadata(dir.join("ui-heartbeat"))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok());
    if heartbeat.is_none_or(|age| age > Duration::from_secs(6)) {
        session.approvable = false;
        write_json(&event, &session)?;
        return Ok(());
    }
    let response_path = dir.join(format!("decision-{ticket}.json"));
    while now_ms() < session.deadline_ms {
        if fs::metadata(dir.join("ui-heartbeat"))
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_none_or(|age| age > Duration::from_secs(6))
        {
            break;
        }
        if let Ok(bytes) = fs::read(&response_path)
            && let Ok(decision) = serde_json::from_slice::<Decision>(&bytes)
            && decision.ticket == ticket
            && decision.at_ms >= session.updated_ms
        {
            let out = serde_json::json!({"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":if decision.allow{"allow"}else{"deny"}}}});
            println!("{out}");
            session.state = if decision.allow {
                State::Running
            } else {
                State::Idle
            };
            session.approvable = false;
            write_json(&event, &session)?;
            let _ = fs::remove_file(response_path);
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    session.approvable = false;
    session.state = State::Idle;
    write_json(&event, &session)?;
    Ok(())
}
pub(crate) struct Monitor {
    pub(crate) events: Receiver<Vec<Session>>,
    stop: Arc<AtomicBool>,
}
impl Drop for Monitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
impl Monitor {
    pub(crate) fn start(dir: PathBuf, wake: impl Fn() + Send + 'static) -> Self {
        let (tx, events) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        thread::spawn(move || {
            let _ = fs::create_dir_all(&dir);
            let mut previous = Vec::new();
            while !stopped.load(Ordering::Relaxed) {
                let sessions = read_sessions(&dir);
                if sessions != previous {
                    previous = sessions.clone();
                    if tx.send(sessions).is_err() {
                        break;
                    }
                    wake();
                }
                let _ = fs::write(dir.join("ui-heartbeat"), now_ms().to_string());
                for _ in 0..20 {
                    if stopped.load(Ordering::Relaxed) {
                        return;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
            }
        });
        Self { events, stop }
    }
}
pub(crate) fn read_sessions(dir: &Path) -> Vec<Session> {
    let mut sessions = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return sessions;
    };
    for entry in entries.filter_map(Result::ok).take(256) {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("session-") || !name.ends_with(".json") {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() || meta.len() > 20_000 {
            continue;
        }
        let Ok(bytes) = fs::read(entry.path()) else {
            continue;
        };
        let Ok(mut session) = serde_json::from_slice::<Session>(&bytes) else {
            continue;
        };
        if now_ms().saturating_sub(session.updated_ms) > 24 * 60 * 60 * 1000 {
            continue;
        }
        if session.state == State::Waiting && session.deadline_ms < now_ms() {
            session.approvable = false;
            session.state = State::Idle;
        }
        // Silence is not proof of disconnection: long tool calls may produce no hooks.
        sessions.push(session);
        if sessions.len() == 64 {
            break;
        }
    }
    sessions.sort_by_key(|a| std::cmp::Reverse(a.updated_ms));
    sessions
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_lifecycle_and_approval_are_real_and_scoped() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let value = serde_json::json!({"hook_event_name":"PermissionRequest","session_id":"one","tool_name":"Write","cwd":"workspace","tool_input":{"file_path":"src/main.rs"}});
        let session = normalize(&value, Some("abc-123".into()))?;
        assert!(session.approvable);
        assert_eq!(session.state, State::Waiting);
        write_json(&event_path(dir.path(), &session), &session)?;
        decide(dir.path(), &session, false)?;
        let response: Decision =
            serde_json::from_slice(&fs::read(dir.path().join("decision-abc-123.json"))?)?;
        assert!(!response.allow);
        assert!(decide(dir.path(), &session, true).is_err());
        let mut stale = session.clone();
        stale.ticket = Some("abc-456".into());
        assert!(decide(dir.path(), &stale, true).is_err());
        let mut unsupported = value;
        unsupported["coucou_agent"] = "codex".into();
        assert!(!normalize(&unsupported, Some("abc-123".into()))?.approvable);
        assert_eq!(read_sessions(dir.path()).len(), 1);
        Ok(())
    }
    #[test]
    fn malformed_and_expired_events_cannot_grant_permission() -> io::Result<()> {
        assert!(normalize(&serde_json::json!({"hook_event_name":"SessionStart"}), None).is_err());
        assert!(
            normalize(
                &serde_json::json!({"session_id":"one","hook_event_name":"Unknown"}),
                None
            )
            .is_err()
        );
        let mut s = normalize(
            &serde_json::json!({"session_id":"one","hook_event_name":"PermissionRequest"}),
            Some("../other".into()),
        )?;
        assert!(!s.approvable);
        s.approvable = true;
        s.deadline_ms = 0;
        assert!(decide(Path::new("."), &s, true).is_err());
        Ok(())
    }

    #[test]
    #[ignore = "requires AEROS_TEST_EXE; tests the actual hook subprocess without executing a provider tool"]
    fn hook_subprocess_round_trip() -> io::Result<()> {
        use std::process::{Command, Stdio};
        let exe = std::env::var_os("AEROS_TEST_EXE").expect("set AEROS_TEST_EXE");
        let profile = tempfile::tempdir()?;
        let dir = profile.path().join("agent-bridge");
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("ui-heartbeat"), "test")?;
        let mut child = Command::new(exe)
            .arg("--agent-hook")
            .env("AEROS_CONFIG_DIR", profile.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        child.stdin.take().unwrap().write_all(serde_json::json!({"session_id":"subprocess-test","hook_event_name":"PermissionRequest","tool_name":"Write","cwd":"isolated contract test","tool_input":{"file_path":"fixture.rs","content":"no tool is executed"}}).to_string().as_bytes())?;
        let until = std::time::Instant::now() + Duration::from_secs(3);
        let session = loop {
            if let Some(session) = read_sessions(&dir).into_iter().next() {
                break session;
            }
            if std::time::Instant::now() > until {
                let _ = child.kill();
                let _ = child.wait();
                panic!("hook did not publish an event");
            }
            thread::sleep(Duration::from_millis(10));
        };
        assert!(session.approvable);
        decide(&dir, &session, true)?;
        let until = std::time::Instant::now() + Duration::from_secs(3);
        while child.try_wait()?.is_none() {
            if std::time::Instant::now() > until {
                let _ = child.kill();
                let _ = child.wait();
                panic!("hook did not receive the decision");
            }
            thread::sleep(Duration::from_millis(10));
        }
        let result = child.wait_with_output()?;
        assert!(result.status.success());
        let response: Value = serde_json::from_slice(&result.stdout)?;
        assert_eq!(
            response["hookSpecificOutput"]["decision"]["behavior"],
            "allow"
        );
        assert!(fs::read_to_string(dir.join("decisions.jsonl"))?.contains("allow-once"));
        assert!(!profile.path().join("fixture.rs").exists());
        Ok(())
    }
}

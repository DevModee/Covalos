//! COVALOS approval gate for agent tool calls.
//!
//! Claude Code sessions started by COVALOS get a `PreToolUse` hook that runs
//! `covalos.exe --agent-gate <dir> <session>`. The hook maps each tool call to
//! a COVALOS `Action`, applies the session policy and the static command
//! classifier, and when a person must decide it files a request that the
//! Agent Hub shows until it is answered, expires or the UI goes away.
//!
//! Two layers stay separate on purpose:
//! - COVALOS permissions: this gate (policy, grants, approvals).
//! - Effective process permissions: what the provider process can do anyway
//!   (`--permission-mode`, Codex `--sandbox`, the OS user account). A git
//!   worktree isolates files for review, not the process.
use covalos_agents::{
    SessionId,
    context::looks_secret,
    permission::{Action, Decision, Policy, Preset},
};
use covalos_engine::safety::{self, Risk};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

pub(crate) const REQUEST_TTL_MS: u64 = 120_000;
const HEARTBEAT_MAX_AGE: Duration = Duration::from_secs(6);

/// Session policy as written by COVALOS before the provider starts.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct GatePolicy {
    pub root: PathBuf,
    pub preset: Preset,
    #[serde(default)]
    pub grants: Vec<Action>,
    /// Auto-approve commands the classifier marks `Safe`.
    #[serde(default)]
    pub allow_safe_commands: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct GateRequest {
    pub ticket: String,
    pub session: String,
    pub tool: String,
    pub action: Action,
    /// Why a person must decide (e.g. the classifier's concern).
    pub reason: String,
    /// Sensitive actions can only be allowed once.
    pub once_only: bool,
    pub created_ms: u64,
    pub expires_ms: u64,
}

#[derive(Serialize, Deserialize)]
struct GateDecision {
    ticket: String,
    allow: bool,
    remember: bool,
}

pub(crate) fn session_dir(gate_dir: &Path, session: &SessionId) -> PathBuf {
    gate_dir.join(session.as_str())
}

/// Writes the policy and returns the Claude Code `--settings` JSON that
/// installs the hook for this session.
pub(crate) fn install(
    exe: &Path,
    gate_dir: &Path,
    session: &SessionId,
    policy: &GatePolicy,
) -> io::Result<String> {
    let dir = session_dir(gate_dir, session);
    fs::create_dir_all(&dir)?;
    write_json(&dir.join("policy.json"), policy)?;
    // Forward slashes and double quotes work in the shells Claude Code uses
    // for hooks on Windows (Git Bash or cmd) and on Unix.
    let slash = |p: &Path| p.display().to_string().replace('\\', "/");
    let command = format!(
        "\"{}\" --agent-gate \"{}\" {}",
        slash(exe),
        slash(gate_dir),
        session
    );
    Ok(json!({
        "hooks": {
            "PreToolUse": [{
                "matcher": "*",
                "hooks": [{"type": "command", "command": command, "timeout": 180}]
            }]
        }
    })
    .to_string())
}

pub(crate) fn heartbeat(gate_dir: &Path) {
    let _ = fs::create_dir_all(gate_dir);
    let _ = fs::write(gate_dir.join("ui-heartbeat"), now_ms().to_string());
}

fn ui_alive(gate_dir: &Path) -> bool {
    fs::metadata(gate_dir.join("ui-heartbeat"))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age <= HEARTBEAT_MAX_AGE)
}

/// Maps a Claude Code tool call to a COVALOS action.
pub(crate) fn action_for(tool: &str, input: &Value, root: &Path) -> Action {
    let path = |key: &str| {
        input[key]
            .as_str()
            .map(PathBuf::from)
            .unwrap_or_else(|| root.to_path_buf())
    };
    match tool {
        "Read" => Action::Read(path("file_path")),
        "Glob" | "Grep" | "LS" => Action::Read(path("path")),
        "Write" | "Edit" | "MultiEdit" => Action::Write(path("file_path")),
        "NotebookEdit" => Action::Write(path("notebook_path")),
        "Bash" | "PowerShell" => {
            Action::Execute(input["command"].as_str().unwrap_or_default().to_owned())
        }
        "WebFetch" => Action::Network(input["url"].as_str().unwrap_or_default().to_owned()),
        "WebSearch" => Action::Network(format!(
            "search: {}",
            input["query"].as_str().unwrap_or_default()
        )),
        other => Action::Tool(other.to_owned()),
    }
}

/// Tools that only organize the agent's own work. Their effects (subagent
/// tool calls) pass through this gate again.
const BOOKKEEPING: &[&str] = &["TodoWrite", "ExitPlanMode", "Task", "BashOutput"];

pub(crate) enum Verdict {
    Allow(String),
    Deny(String),
    Ask { reason: String, once_only: bool },
}

pub(crate) fn evaluate(policy: &GatePolicy, tool: &str, action: &Action) -> Verdict {
    if BOOKKEEPING.contains(&tool) {
        return Verdict::Allow("bookkeeping tool".into());
    }
    let compiled = match Policy::new(&policy.root, policy.preset) {
        Ok(mut compiled) => {
            for grant in &policy.grants {
                compiled.grant(grant.clone());
            }
            compiled
        }
        Err(error) => return Verdict::Deny(format!("session root unavailable: {error}")),
    };
    match action {
        Action::Execute(command) => match safety::classify(command) {
            Risk::Forbidden(concern) => {
                return Verdict::Deny(format!("forbidden command ({concern:?})"));
            }
            Risk::Sensitive(concern) => {
                return Verdict::Ask {
                    reason: format!("sensitive command ({concern:?})"),
                    once_only: true,
                };
            }
            Risk::Safe if policy.allow_safe_commands && policy.preset != Preset::ReadOnly => {
                return Verdict::Allow("safe command".into());
            }
            Risk::Safe => {}
        },
        Action::Read(path) | Action::Write(path)
            if safety::touches_secrets(&path.to_string_lossy()) =>
        {
            return Verdict::Ask {
                reason: "path looks like a credential".into(),
                once_only: true,
            };
        }
        _ => {}
    }
    match compiled.evaluate(action) {
        Decision::Allow => Verdict::Allow("session policy".into()),
        Decision::Deny => Verdict::Deny(match action {
            Action::Write(_) => "writes outside the session root are denied".into(),
            _ => "the session preset denies this action".into(),
        }),
        Decision::Ask => Verdict::Ask {
            reason: "not covered by the session policy".into(),
            once_only: false,
        },
    }
}

/// `covalos --agent-gate <dir> <session>`: the hook process. Any failure
/// denies the tool call; the provider keeps running.
pub(crate) fn hook_entry(args: &[String]) -> io::Result<()> {
    let (allow, reason) = match decide_from_stdin(args) {
        Ok(result) => result,
        Err(error) => (false, format!("COVALOS gate error: {error}")),
    };
    let out = json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": if allow { "allow" } else { "deny" },
            "permissionDecisionReason": format!("COVALOS: {reason}"),
        }
    });
    let mut stdout = io::stdout();
    writeln!(stdout, "{out}")?;
    stdout.flush()
}

fn decide_from_stdin(args: &[String]) -> io::Result<(bool, String)> {
    let [gate_dir, session] = args else {
        return Err(io::Error::other("usage: --agent-gate <dir> <session>"));
    };
    let session = SessionId::new(session.as_str()).map_err(io::Error::other)?;
    let gate_dir = PathBuf::from(gate_dir);
    let dir = session_dir(&gate_dir, &session);
    let policy: GatePolicy = serde_json::from_slice(&fs::read(dir.join("policy.json"))?)?;
    let mut input = String::new();
    io::stdin().take(1 << 20).read_to_string(&mut input)?;
    let value: Value = serde_json::from_str(&input)?;
    let tool = value["tool_name"].as_str().unwrap_or_default().to_owned();
    let action = action_for(&tool, &value["tool_input"], &policy.root);
    let (allow, reason, by) = match evaluate(&policy, &tool, &action) {
        Verdict::Allow(reason) => (true, reason, "policy"),
        Verdict::Deny(reason) => (false, reason, "policy"),
        Verdict::Ask { reason, once_only } => {
            let (allow, why) = ask(&gate_dir, &dir, &session, &tool, &action, reason, once_only)?;
            (allow, why, "person")
        }
    };
    audit(&gate_dir, &session, &tool, &action, allow, by, &reason);
    Ok((allow, reason))
}

fn ask(
    gate_dir: &Path,
    dir: &Path,
    session: &SessionId,
    tool: &str,
    action: &Action,
    reason: String,
    once_only: bool,
) -> io::Result<(bool, String)> {
    if !ui_alive(gate_dir) {
        return Ok((
            false,
            format!("{reason}; COVALOS is not open to approve it"),
        ));
    }
    let created_ms = now_ms();
    let ticket = format!("{created_ms:x}-{:x}", std::process::id());
    let request = GateRequest {
        ticket: ticket.clone(),
        session: session.to_string(),
        tool: tool.to_owned(),
        action: action.clone(),
        reason: reason.clone(),
        once_only,
        created_ms,
        expires_ms: created_ms + REQUEST_TTL_MS,
    };
    let request_path = dir.join(format!("request-{ticket}.json"));
    let decision_path = dir.join(format!("decision-{ticket}.json"));
    write_json(&request_path, &request)?;
    let outcome = loop {
        if let Ok(bytes) = fs::read(&decision_path)
            && let Ok(decision) = serde_json::from_slice::<GateDecision>(&bytes)
            && decision.ticket == ticket
        {
            if decision.allow && decision.remember && !once_only {
                remember(dir, action);
            }
            break (
                decision.allow,
                if decision.allow {
                    "approved in the Agent Hub".into()
                } else {
                    "denied in the Agent Hub".into()
                },
            );
        }
        if now_ms() > request.expires_ms {
            break (false, format!("{reason}; approval expired"));
        }
        if !ui_alive(gate_dir) {
            break (false, format!("{reason}; COVALOS closed before deciding"));
        }
        thread::sleep(Duration::from_millis(100));
    };
    let _ = fs::remove_file(&request_path);
    let _ = fs::remove_file(&decision_path);
    Ok(outcome)
}

fn remember(dir: &Path, action: &Action) {
    let path = dir.join("policy.json");
    if let Ok(bytes) = fs::read(&path)
        && let Ok(mut policy) = serde_json::from_slice::<GatePolicy>(&bytes)
    {
        if !policy.grants.contains(action) {
            policy.grants.push(action.clone());
        }
        let _ = write_json(&path, &policy);
    }
}

fn audit(
    gate_dir: &Path,
    session: &SessionId,
    tool: &str,
    action: &Action,
    allow: bool,
    by: &str,
    reason: &str,
) {
    let target = redact(&describe(action));
    let line = json!({
        "at_ms": now_ms(), "session": session.as_str(), "tool": tool,
        "target": target, "allow": allow, "by": by, "reason": reason,
    });
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(gate_dir.join("audit.jsonl"))
    {
        let _ = writeln!(file, "{line}");
    }
}

pub(crate) fn describe(action: &Action) -> String {
    match action {
        Action::Read(p) => format!("read {}", p.display()),
        Action::Write(p) => format!("write {}", p.display()),
        Action::Execute(c) => format!("run {c}"),
        Action::Network(t) => format!("network {t}"),
        Action::Tool(t) => format!("tool {t}"),
    }
}

/// Keeps secret-looking values out of the UI and the audit log.
pub(crate) fn redact(text: &str) -> String {
    let bounded: String = text.chars().take(600).collect();
    if looks_secret(&bounded) {
        "[hidden: looks like a secret]".into()
    } else {
        bounded
    }
}

/// Pending requests for every session, oldest first. Expired ones are
/// dropped from view (the hook denies them on its own).
pub(crate) fn pending(gate_dir: &Path) -> Vec<GateRequest> {
    let mut found = Vec::new();
    let Ok(sessions) = fs::read_dir(gate_dir) else {
        return found;
    };
    for session in sessions.flatten().filter(|e| e.path().is_dir()).take(256) {
        let Ok(files) = fs::read_dir(session.path()) else {
            continue;
        };
        for file in files.flatten().take(64) {
            let name = file.file_name();
            let name = name.to_string_lossy();
            if !name.starts_with("request-") || !name.ends_with(".json") {
                continue;
            }
            if let Ok(bytes) = fs::read(file.path())
                && bytes.len() < 64 * 1024
                && let Ok(request) = serde_json::from_slice::<GateRequest>(&bytes)
                && request.expires_ms > now_ms()
            {
                found.push(request);
            }
        }
    }
    found.sort_by_key(|r| r.created_ms);
    found
}

pub(crate) fn answer(
    gate_dir: &Path,
    request: &GateRequest,
    allow: bool,
    remember: bool,
) -> io::Result<()> {
    let session = SessionId::new(request.session.as_str()).map_err(io::Error::other)?;
    let dir = session_dir(gate_dir, &session);
    if !dir
        .join(format!("request-{}.json", request.ticket))
        .exists()
    {
        return Err(io::Error::other("This request is no longer pending."));
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join(format!("decision-{}.json", request.ticket)))?;
    serde_json::to_writer(
        &mut file,
        &GateDecision {
            ticket: request.ticket.clone(),
            allow,
            remember: remember && !request.once_only,
        },
    )?;
    file.sync_all()
}

/// Recent audit lines for one session, newest last.
pub(crate) fn audit_tail(gate_dir: &Path, session: &str, limit: usize) -> Vec<Value> {
    let Ok(text) = fs::read_to_string(gate_dir.join("audit.jsonl")) else {
        return Vec::new();
    };
    let mut lines: Vec<Value> = text
        .lines()
        .rev()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|v| v["session"] == session)
        .take(limit)
        .collect();
    lines.reverse();
    lines
}

fn write_json(path: &Path, data: &impl Serialize) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("missing parent"))?;
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(file.as_file_mut(), data)?;
    file.as_file_mut().flush()?;
    file.persist(path).map_err(|e| e.error)?;
    Ok(())
}

fn now_ms() -> u64 {
    covalos_agents::now_ms()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(root: &Path, preset: Preset) -> GatePolicy {
        GatePolicy {
            root: root.to_path_buf(),
            preset,
            grants: Vec::new(),
            allow_safe_commands: true,
        }
    }

    #[test]
    fn writes_outside_root_and_forbidden_commands_are_denied() {
        let dir = tempfile::tempdir().unwrap();
        let p = policy(dir.path(), Preset::Worktree);
        let outside = Action::Write(dir.path().join("../escape.txt"));
        assert!(matches!(evaluate(&p, "Write", &outside), Verdict::Deny(_)));
        let inside = Action::Write(dir.path().join("src/main.rs"));
        assert!(matches!(evaluate(&p, "Write", &inside), Verdict::Allow(_)));
        let wipe = Action::Execute("rm -rf /".into());
        assert!(matches!(evaluate(&p, "Bash", &wipe), Verdict::Deny(_)));
    }

    #[test]
    fn sensitive_commands_and_secrets_always_reach_a_person() {
        let dir = tempfile::tempdir().unwrap();
        let p = policy(dir.path(), Preset::Worktree);
        let push = Action::Execute("git push origin main".into());
        assert!(matches!(
            evaluate(&p, "Bash", &push),
            Verdict::Ask {
                once_only: true,
                ..
            }
        ));
        let env = Action::Read(dir.path().join(".env"));
        assert!(matches!(evaluate(&p, "Read", &env), Verdict::Ask { .. }));
        let status = Action::Execute("git status".into());
        assert!(matches!(evaluate(&p, "Bash", &status), Verdict::Allow(_)));
    }

    #[test]
    fn read_only_preset_denies_writes_and_commands_ask_without_safe_auto() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = policy(dir.path(), Preset::ReadOnly);
        p.allow_safe_commands = false;
        let write = Action::Write(dir.path().join("a.txt"));
        assert!(matches!(evaluate(&p, "Write", &write), Verdict::Deny(_)));
        let read = Action::Read(dir.path().join("a.txt"));
        assert!(matches!(evaluate(&p, "Read", &read), Verdict::Allow(_)));
    }

    #[test]
    fn hook_settings_quote_paths_and_requests_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let session = SessionId::new("s1").unwrap();
        let settings = install(
            Path::new(r"C:\Program Files\Covalos\covalos.exe"),
            dir.path(),
            &session,
            &policy(dir.path(), Preset::Supervised),
        )
        .unwrap();
        let value: Value = serde_json::from_str(&settings).unwrap();
        let command = value["hooks"]["PreToolUse"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        assert!(command.starts_with("\"C:/Program Files/Covalos/covalos.exe\" --agent-gate"));
        let request = GateRequest {
            ticket: "t1".into(),
            session: "s1".into(),
            tool: "Bash".into(),
            action: Action::Execute("cargo build".into()),
            reason: "r".into(),
            once_only: false,
            created_ms: now_ms(),
            expires_ms: now_ms() + 10_000,
        };
        write_json(
            &session_dir(dir.path(), &session).join("request-t1.json"),
            &request,
        )
        .unwrap();
        assert_eq!(pending(dir.path()), vec![request.clone()]);
        answer(dir.path(), &request, true, true).unwrap();
        assert!(answer(dir.path(), &request, true, true).is_err());
    }

    #[test]
    fn secrets_are_redacted() {
        assert_eq!(
            redact("run curl -H 'Authorization: Bearer sk-ant-abcdefghijklmnopqrstuvwxyz0123'"),
            "[hidden: looks like a secret]"
        );
        assert_eq!(redact("run cargo test"), "run cargo test");
    }
}

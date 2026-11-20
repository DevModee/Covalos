//! Turns an account and a purpose into a ready-to-spawn command.
//!
//! Login always runs the provider's own flow; COVALOS only chooses the config
//! directory it writes to. API keys are read from the credential store and
//! passed through the provider's documented variable, never as arguments.

use crate::{
    Account, ConnectionMethod, Error, ProviderSpec, Result,
    credentials::{CredentialStore, Secret},
    profile::ProfileStore,
    provider::{Headless, LoginFlow},
};
use std::{
    ffi::OsString,
    fmt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sandbox {
    ReadOnly,
    WorkspaceWrite,
}

#[derive(Clone, Debug)]
pub struct Turn {
    pub resume: Option<String>,
    pub model: Option<String>,
    pub sandbox: Sandbox,
    /// Extra Claude Code settings JSON (`--settings`), used by the host to
    /// install its PreToolUse approval gate. Ignored by other providers.
    pub settings: Option<String>,
}

#[derive(Clone, Debug)]
pub enum Purpose {
    /// Needs a terminal: the provider may prompt or open a browser.
    Login,
    Probe,
    /// Headless turn; the prompt is written to stdin.
    Turn(Turn),
}

enum EnvValue {
    Plain(OsString),
    Secret(Secret),
}

pub struct LaunchPlan {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub interactive: bool,
    env: Vec<(String, EnvValue)>,
    env_remove: Vec<String>,
}

impl fmt::Debug for LaunchPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let env: Vec<_> = self
            .env
            .iter()
            .map(|(k, v)| match v {
                EnvValue::Plain(v) => format!("{k}={}", v.to_string_lossy()),
                EnvValue::Secret(_) => format!("{k}=***"),
            })
            .collect();
        f.debug_struct("LaunchPlan")
            .field("program", &self.program)
            .field("args", &self.args)
            .field("cwd", &self.cwd)
            .field("interactive", &self.interactive)
            .field("env", &env)
            .field("env_remove", &self.env_remove)
            .finish()
    }
}

impl LaunchPlan {
    pub fn env_names(&self) -> impl Iterator<Item = &str> {
        self.env.iter().map(|(k, _)| k.as_str())
    }

    pub fn removes(&self, name: &str) -> bool {
        self.env_remove.iter().any(|n| n == name)
    }

    pub fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        for name in &self.env_remove {
            command.env_remove(name);
        }
        self.finish(command)
    }

    /// Like `command`, but the child inherits only the named variables from
    /// this process, so unrelated credentials in the host environment never
    /// reach the agent.
    pub fn isolated_command(&self, inherit: &[&str]) -> Command {
        let mut command = Command::new(&self.program);
        command.env_clear();
        for name in inherit {
            if !self.removes(name)
                && let Some(value) = std::env::var_os(name)
            {
                command.env(name, value);
            }
        }
        self.finish(command)
    }

    fn finish(&self, mut command: Command) -> Command {
        command.args(&self.args);
        for (name, value) in &self.env {
            match value {
                EnvValue::Plain(v) => command.env(name, v),
                EnvValue::Secret(s) => command.env(name, s.expose()),
            };
        }
        if let Some(cwd) = &self.cwd {
            command.current_dir(cwd);
        }
        command
    }
}

pub fn plan(
    spec: &ProviderSpec,
    account: &Account,
    profiles: &ProfileStore,
    credentials: &dyn CredentialStore,
    program: PathBuf,
    purpose: &Purpose,
    cwd: Option<&Path>,
) -> Result<LaunchPlan> {
    account.validate(spec)?;
    if !account.enabled {
        return Err(Error::Invalid(format!(
            "account {} is disabled",
            account.id
        )));
    }
    let (args, interactive) = match purpose {
        Purpose::Probe => (vec!["--version".to_owned()], false),
        Purpose::Login if account.method == ConnectionMethod::ApiKey => {
            return Err(Error::Invalid("API-key accounts do not sign in".into()));
        }
        Purpose::Login => match &spec.login {
            LoginFlow::Command(args) => (args.clone(), true),
            LoginFlow::Interactive { .. } => (Vec::new(), true),
            LoginFlow::None => return Err(Error::Unsupported(format!("{} login", spec.label))),
        },
        Purpose::Turn(turn) => (turn_args(spec, turn)?, false),
    };
    let mut plan = LaunchPlan {
        program,
        args,
        cwd: cwd.map(Path::to_path_buf),
        interactive,
        env: Vec::new(),
        env_remove: Vec::new(),
    };
    if account.method == ConnectionMethod::SystemCli {
        return Ok(plan);
    }
    if let Some(var) = &spec.config_dir_var {
        let dir = profiles.ensure_isolated_dir(&account.id)?;
        plan.env
            .push((var.clone(), EnvValue::Plain(dir.into_os_string())));
    }
    if let Some(var) = &spec.api_key_var {
        if let Some(key) = account.credential_key() {
            let secret = credentials
                .load(&key)?
                .ok_or_else(|| Error::NotFound(format!("API key for account {}", account.id)))?;
            plan.env.push((var.clone(), EnvValue::Secret(secret)));
        } else {
            plan.env_remove.push(var.clone());
        }
    }
    Ok(plan)
}

fn turn_args(spec: &ProviderSpec, turn: &Turn) -> Result<Vec<String>> {
    let flag_like = |v: &Option<String>| {
        v.as_deref()
            .is_some_and(|v| v.is_empty() || v.starts_with('-'))
    };
    if flag_like(&turn.resume) || flag_like(&turn.model) {
        return Err(Error::Invalid(
            "session ids and models cannot look like flags".into(),
        ));
    }
    let mut args: Vec<String> = Vec::new();
    match spec.headless {
        Headless::ClaudeStreamJson => {
            args.extend(
                [
                    "--print",
                    "--output-format",
                    "stream-json",
                    "--verbose",
                    "--include-partial-messages",
                ]
                .map(String::from),
            );
            let mode = match turn.sandbox {
                Sandbox::ReadOnly => "plan",
                Sandbox::WorkspaceWrite => "acceptEdits",
            };
            args.extend(["--permission-mode".into(), mode.into()]);
            if let Some(settings) = &turn.settings {
                args.extend(["--settings".into(), settings.clone()]);
            }
            if let Some(model) = &turn.model {
                args.extend(["--model".into(), model.clone()]);
            }
            if let Some(id) = &turn.resume {
                args.extend(["--resume".into(), id.clone()]);
            }
        }
        Headless::CodexExecJson => {
            // Projects are often not git repositories yet (a fresh `npm create`);
            // the sandbox below still limits what Codex may touch.
            args.extend(
                [
                    "exec",
                    "--skip-git-repo-check",
                    "--json",
                    "--color",
                    "never",
                    "--sandbox",
                ]
                .map(String::from),
            );
            args.push(
                match turn.sandbox {
                    Sandbox::ReadOnly => "read-only",
                    Sandbox::WorkspaceWrite => "workspace-write",
                }
                .into(),
            );
            if let Some(model) = &turn.model {
                args.extend(["--model".into(), model.clone()]);
            }
            if let Some(id) = &turn.resume {
                args.extend(["resume".into(), id.clone()]);
            }
            args.push("-".into());
        }
        Headless::Unsupported => {
            return Err(Error::Unsupported(format!("{} headless turns", spec.label)));
        }
    }
    Ok(args)
}

/// Finds the provider binary: an absolute override path, then `PATH`.
pub fn discover(spec: &ProviderSpec) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(&spec.binary_override_var).map(PathBuf::from) {
        return (path.is_absolute() && path.is_file()).then_some(path);
    }
    let names: Vec<String> = if cfg!(windows) {
        vec![
            format!("{}.exe", spec.executable),
            format!("{}.cmd", spec.executable),
        ]
    } else {
        vec![spec.executable.clone()]
    };
    let on_path = std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .filter(|dir| dir.is_absolute())
            .flat_map(|dir| names.iter().map(move |n| dir.join(n)))
            .find(|path| path.is_file())
    });
    on_path.or_else(|| fallback(spec, &names))
}

/// Official install locations that are not always on `PATH`: the native
/// installer's `~/.local/bin`, and the CLI bundled with the Codex desktop app
/// on Windows (newest copy under `%LOCALAPPDATA%\OpenAI\Codex\bin`).
fn fallback(spec: &ProviderSpec, names: &[String]) -> Option<PathBuf> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })?;
    if let Some(path) = names
        .iter()
        .map(|n| Path::new(&home).join(".local").join("bin").join(n))
        .find(|p| p.is_file())
    {
        return Some(path);
    }
    if cfg!(windows) && spec.executable == "codex" {
        let local = std::env::var_os("LOCALAPPDATA")?;
        let entries = Path::new(&local).join("OpenAI/Codex/bin").read_dir().ok()?;
        return entries
            .flatten()
            .map(|entry| entry.path().join("codex.exe"))
            .filter_map(|path| {
                let modified = path.metadata().ok().filter(|m| m.is_file())?.modified();
                Some((modified.ok(), path))
            })
            .max_by_key(|(modified, _)| *modified)
            .map(|(_, path)| path);
    }
    None
}

/// Runs a non-interactive plan (normally `Purpose::Probe`) and returns the
/// first line of stdout, killing the process after `timeout`.
pub fn run_probe(plan: &LaunchPlan, timeout: Duration) -> Result<String> {
    if plan.interactive {
        return Err(Error::Invalid("interactive plans need a terminal".into()));
    }
    let mut child = plan
        .command()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::Invalid(format!(
                "{} timed out",
                plan.program.display()
            )));
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let mut stdout = String::new();
    if let Some(mut out) = child.stdout.take() {
        std::io::Read::read_to_string(&mut out, &mut stdout)?;
    }
    if !status.success() {
        return Err(Error::Invalid(format!(
            "{} exited with {status}",
            plan.program.display()
        )));
    }
    Ok(stdout.lines().next().unwrap_or_default().trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AccountId, ProviderKind, ProviderRegistry,
        credentials::{MemoryStore, Secret},
    };

    struct Fixture {
        _dir: tempfile::TempDir,
        profiles: ProfileStore,
        credentials: MemoryStore,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let profiles = ProfileStore::open(dir.path()).unwrap();
        Fixture {
            _dir: dir,
            profiles,
            credentials: MemoryStore::default(),
        }
    }

    fn account(provider: ProviderKind, method: ConnectionMethod) -> Account {
        Account::new(AccountId::new("acc").unwrap(), provider, "Acc", method)
    }

    fn turn(sandbox: Sandbox) -> Purpose {
        Purpose::Turn(Turn {
            resume: Some("abc".into()),
            model: Some("m".into()),
            sandbox,
            settings: None,
        })
    }

    #[test]
    fn official_login_isolates_config_and_strips_api_key() {
        let f = fixture();
        let spec = ProviderSpec::claude_code();
        let acc = account(ProviderKind::ClaudeCode, ConnectionMethod::OfficialLogin);
        let plan = plan(
            &spec,
            &acc,
            &f.profiles,
            &f.credentials,
            "claude".into(),
            &Purpose::Login,
            None,
        )
        .unwrap();
        assert!(plan.interactive);
        assert!(plan.args.is_empty());
        assert_eq!(plan.env_names().collect::<Vec<_>>(), ["CLAUDE_CONFIG_DIR"]);
        assert!(plan.removes("ANTHROPIC_API_KEY"));
        assert!(f.profiles.isolated_dir(&acc.id).is_dir());
        let command = plan.command();
        let dir = command
            .get_envs()
            .find(|(k, _)| *k == "CLAUDE_CONFIG_DIR")
            .unwrap()
            .1
            .unwrap();
        assert_eq!(Path::new(dir), f.profiles.isolated_dir(&acc.id));
    }

    #[test]
    fn api_keys_are_injected_but_never_printed() {
        let mut f = fixture();
        let spec = ProviderSpec::gemini();
        let acc = account(ProviderKind::Gemini, ConnectionMethod::ApiKey);
        let probe = Purpose::Probe;
        assert!(
            plan(
                &spec,
                &acc,
                &f.profiles,
                &f.credentials,
                "gemini".into(),
                &probe,
                None
            )
            .is_err()
        );
        f.credentials
            .save(&acc.credential_key().unwrap(), Secret::new("sk-hidden"))
            .unwrap();
        let plan = plan(
            &spec,
            &acc,
            &f.profiles,
            &f.credentials,
            "gemini".into(),
            &probe,
            None,
        )
        .unwrap();
        assert!(!format!("{plan:?}").contains("sk-hidden"));
        assert!(!plan.args.iter().any(|a| a.contains("sk-hidden")));
        let command = plan.command();
        let value = command
            .get_envs()
            .find(|(k, _)| *k == "GEMINI_API_KEY")
            .unwrap()
            .1
            .unwrap();
        assert_eq!(value, "sk-hidden");
        assert!(matches!(
            super::plan(
                &spec,
                &acc,
                &f.profiles,
                &f.credentials,
                "gemini".into(),
                &Purpose::Login,
                None
            ),
            Err(Error::Invalid(_))
        ));
    }

    #[test]
    fn system_cli_leaves_environment_untouched() {
        let f = fixture();
        let spec = ProviderSpec::codex();
        let acc = account(ProviderKind::Codex, ConnectionMethod::SystemCli);
        let plan = plan(
            &spec,
            &acc,
            &f.profiles,
            &f.credentials,
            "codex".into(),
            &Purpose::Login,
            None,
        )
        .unwrap();
        assert_eq!(plan.args, ["login"]);
        assert_eq!(plan.env_names().count(), 0);
        assert!(!f.profiles.isolated_dir(&acc.id).exists());
    }

    #[test]
    fn headless_turn_arguments() {
        let f = fixture();
        let claude = plan(
            &ProviderSpec::claude_code(),
            &account(ProviderKind::ClaudeCode, ConnectionMethod::SystemCli),
            &f.profiles,
            &f.credentials,
            "claude".into(),
            &turn(Sandbox::ReadOnly),
            Some(Path::new("/tmp")),
        )
        .unwrap();
        assert!(
            claude
                .args
                .windows(2)
                .any(|w| w == ["--permission-mode", "plan"])
        );
        assert!(claude.args.windows(2).any(|w| w == ["--resume", "abc"]));
        assert_eq!(claude.cwd.as_deref(), Some(Path::new("/tmp")));

        let codex = plan(
            &ProviderSpec::codex(),
            &account(ProviderKind::Codex, ConnectionMethod::SystemCli),
            &f.profiles,
            &f.credentials,
            "codex".into(),
            &turn(Sandbox::WorkspaceWrite),
            None,
        )
        .unwrap();
        assert_eq!(codex.args.first().map(String::as_str), Some("exec"));
        assert!(codex.args.iter().any(|a| a == "--skip-git-repo-check"));
        assert!(
            codex
                .args
                .windows(2)
                .any(|w| w == ["--sandbox", "workspace-write"])
        );
        assert!(
            codex
                .args
                .ends_with(&["resume".into(), "abc".into(), "-".into()])
        );

        let gemini = plan(
            &ProviderSpec::gemini(),
            &account(ProviderKind::Gemini, ConnectionMethod::SystemCli),
            &f.profiles,
            &f.credentials,
            "gemini".into(),
            &turn(Sandbox::ReadOnly),
            None,
        );
        assert!(matches!(gemini, Err(Error::Unsupported(_))));

        let mut bad = Turn {
            resume: Some("--dangerous".into()),
            model: None,
            sandbox: Sandbox::ReadOnly,
            settings: None,
        };
        assert!(turn_args(&ProviderSpec::claude_code(), &bad).is_err());
        bad.resume = None;
        assert!(turn_args(&ProviderSpec::claude_code(), &bad).is_ok());
    }

    #[test]
    fn disabled_accounts_cannot_launch() {
        let f = fixture();
        let mut acc = account(ProviderKind::ClaudeCode, ConnectionMethod::SystemCli);
        acc.enabled = false;
        assert!(
            plan(
                &ProviderSpec::claude_code(),
                &acc,
                &f.profiles,
                &f.credentials,
                "claude".into(),
                &Purpose::Probe,
                None
            )
            .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn isolated_command_drops_unlisted_host_variables() {
        let plan = LaunchPlan {
            program: "env".into(),
            args: Vec::new(),
            cwd: None,
            interactive: false,
            env: vec![("COVALOS_PLAN_VAR".into(), EnvValue::Plain("1".into()))],
            env_remove: Vec::new(),
        };
        let output = plan.isolated_command(&["PATH"]).output().unwrap();
        let env = String::from_utf8(output.stdout).unwrap();
        assert!(env.contains("COVALOS_PLAN_VAR=1"));
        assert!(env.contains("PATH="));
        assert!(!env.contains("CARGO_PKG_NAME"), "{env}");
    }

    /// Exercises the real Claude Code binary when it is installed; it only
    /// asks for the version and never signs in or sends a prompt.
    #[test]
    fn probes_installed_claude_code() {
        let registry = ProviderRegistry::builtin();
        let spec = registry.get(&ProviderKind::ClaudeCode).unwrap();
        let Some(program) = discover(spec) else {
            eprintln!("claude not installed; skipping");
            return;
        };
        let f = fixture();
        let acc = account(ProviderKind::ClaudeCode, ConnectionMethod::SystemCli);
        let plan = plan(
            spec,
            &acc,
            &f.profiles,
            &f.credentials,
            program,
            &Purpose::Probe,
            None,
        )
        .unwrap();
        let version = run_probe(&plan, Duration::from_secs(20)).unwrap();
        assert!(version.contains("Claude Code"), "{version}");
    }
}

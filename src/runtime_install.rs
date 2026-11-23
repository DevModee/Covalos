use covalos_agents::ProviderKind;
use covalos_engine::process::{contain, isolated, kill_tree, own_group};
use serde_json::Value;
use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

const REGISTRY: &str = "https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json";

#[derive(Clone, Default)]
pub(crate) enum Progress {
    #[default]
    Preparing,
    Downloading(u64),
    Installing,
    Cancelling,
}

#[derive(Default)]
pub(crate) struct Install {
    pub progress: Arc<Mutex<Progress>>,
    cancel: Arc<AtomicBool>,
}

impl Install {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Ok(mut state) = self.progress.lock() {
            *state = Progress::Cancelling;
        }
    }
}

impl Drop for Install {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn package(kind: &ProviderKind) -> Option<(&'static str, &'static str)> {
    Some(match kind {
        ProviderKind::ClaudeCode => ("@anthropic-ai/claude-code", "claude"),
        ProviderKind::Codex => ("@openai/codex", "codex"),
        ProviderKind::Gemini => ("@google/gemini-cli", "gemini"),
        ProviderKind::OpenCode => ("opencode-ai", "opencode"),
        _ => return None,
    })
}

fn npm_bin(root: &Path, binary: &str) -> PathBuf {
    if cfg!(windows) {
        root.join(format!("{binary}.cmd"))
    } else {
        root.join("bin").join(binary)
    }
}

fn antigravity_bin(root: &Path) -> PathBuf {
    root.join(if cfg!(windows) {
        "agy_acp_server.exe"
    } else {
        "agy_acp_server.par"
    })
}

pub(crate) fn program(root: &Path, kind: &ProviderKind) -> Option<PathBuf> {
    let root = root.join(kind.key());
    let path = if *kind == ProviderKind::Antigravity {
        antigravity_bin(&root)
    } else {
        npm_bin(&root, package(kind)?.1)
    };
    path.is_file().then_some(path)
}

pub(crate) fn pty_command(
    command: &Command,
    kind: &ProviderKind,
) -> Result<portable_pty::CommandBuilder, String> {
    let program = Path::new(command.get_program());
    let mut builder = if cfg!(windows)
        && program
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("cmd"))
    {
        let (package, _) = package(kind).ok_or("No embedded launcher for this runtime.")?;
        let root = program.parent().ok_or("Invalid runtime path.")?;
        let pkg = root.join("node_modules").join(package);
        let manifest: Value =
            serde_json::from_slice(&fs::read(pkg.join("package.json")).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        let binary_name = program
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("Invalid runtime name.")?;
        let relative = manifest["bin"]
            .as_str()
            .or_else(|| manifest["bin"][binary_name].as_str())
            .ok_or("The official package did not declare a launcher.")?;
        let script = fs::canonicalize(pkg.join(relative)).map_err(|e| e.to_string())?;
        if !script.starts_with(fs::canonicalize(pkg).map_err(|e| e.to_string())?) {
            return Err("The package launcher is outside its installation.".into());
        }
        let node = root.join("node.exe");
        let mut builder = if node.is_file() {
            portable_pty::CommandBuilder::new(node.as_os_str())
        } else {
            portable_pty::CommandBuilder::new("node.exe")
        };
        builder.arg(crate::agents_runtime::plain(&script));
        builder
    } else {
        portable_pty::CommandBuilder::new(program.as_os_str())
    };
    builder.env_clear();
    for (key, val) in command.get_envs() {
        if let Some(val) = val {
            builder.env(key, val);
        }
    }
    builder.args(command.get_args());
    if let Some(cwd) = command.get_current_dir() {
        builder.cwd(cwd);
    }
    Ok(builder)
}

pub(crate) fn spawn(
    root: PathBuf,
    kind: ProviderKind,
    sender: mpsc::Sender<(ProviderKind, Result<(), String>)>,
    wake: crate::agents_runtime::Wake,
) -> Install {
    let install = Install::default();
    let state = install.progress.clone();
    let cancel = install.cancel.clone();
    thread::spawn(move || {
        let report = |progress| {
            if let Ok(mut state) = state.lock()
                && !cancel.load(Ordering::Relaxed)
            {
                *state = progress;
            }
            wake();
        };
        let result = install_runtime(&root, &kind, &cancel, report);
        let _ = sender.send((kind, result));
        wake();
    });
    install
}

fn run(
    mut command: Command,
    cancel: &AtomicBool,
    mut poll: impl FnMut(&str),
) -> Result<String, String> {
    own_group(&mut command);
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            format!(
                "Could not start {}: {e}",
                command.get_program().to_string_lossy()
            )
        })?;
    contain(&child);
    let (tx, rx) = mpsc::channel();
    for input in [
        Box::new(child.stdout.take().expect("piped")) as Box<dyn std::io::Read + Send>,
        Box::new(child.stderr.take().expect("piped")),
    ] {
        let tx = tx.clone();
        thread::spawn(move || {
            for line in BufReader::new(input).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
    }
    drop(tx);
    let deadline = Instant::now() + Duration::from_secs(15 * 60);
    let mut log = String::new();
    let mut status = None;
    loop {
        if cancel.load(Ordering::Relaxed) || Instant::now() >= deadline {
            kill_tree(&mut child);
            return Err(if cancel.load(Ordering::Relaxed) {
                "Installation cancelled."
            } else {
                "Installation timed out."
            }
            .into());
        }
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(line) => {
                poll(&line);
                log.push_str(&line);
                log.push('\n');
                let keep = log.ceil_char_boundary(log.len().saturating_sub(4096));
                log.drain(..keep);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => poll(""),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if let Some(status) = status {
                    return if std::process::ExitStatus::success(&status) {
                        Ok(log)
                    } else {
                        Err(crate::agent_gate::redact(log.trim()))
                    };
                }
                thread::sleep(Duration::from_millis(20));
            }
        }
        if status.is_none() {
            status = child.try_wait().map_err(|e| e.to_string())?;
        }
    }
}

fn curl(url: &str, dest: &Path) -> Command {
    let mut command = isolated(
        if cfg!(windows) { "curl.exe" } else { "curl" },
        &[
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--output",
        ],
    );
    command.arg(dest).arg(url);
    command
}

fn install_antigravity(
    stage: &Path,
    output: &Path,
    cancel: &AtomicBool,
    report: &impl Fn(Progress),
) -> Result<(), String> {
    let manifest = stage.join("registry.json");
    run(curl(REGISTRY, &manifest), cancel, |_| {})?;
    let data: Value = serde_json::from_slice(&fs::read(manifest).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let os = if cfg!(target_os = "macos") {
        "darwin"
    } else {
        std::env::consts::OS
    };
    let platform = format!("{os}-{}", std::env::consts::ARCH);
    let distribution = data["agents"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|agent| agent["id"] == "antigravity-acp")
        .map(|agent| &agent["distribution"]["binary"][&platform])
        .ok_or("Antigravity is missing from the ACP registry.")?;
    let url = distribution["archive"]
        .as_str()
        .filter(|url| url.starts_with("https://dl.google.com/agy-extensions/releases/"))
        .ok_or("This platform has no official Antigravity download.")?;
    let hash = distribution["sha256"]
        .as_str()
        .filter(|sha| sha.len() == 64 && sha.bytes().all(|c| c.is_ascii_hexdigit()))
        .ok_or("The registry did not publish a valid checksum.")?;
    let archive = stage.join("antigravity.zip");
    let mut previous = 0;
    report(Progress::Downloading(0));
    run(curl(url, &archive), cancel, |_| {
        let bytes = fs::metadata(&archive).map_or(0, |data| data.len());
        if bytes != previous {
            previous = bytes;
            report(Progress::Downloading(bytes));
        }
    })?;
    report(Progress::Installing);
    let mut check = if cfg!(windows) {
        let mut command = isolated(
            "powershell.exe",
            &[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "(Get-FileHash -LiteralPath $env:COVALOS_INSTALL_ARCHIVE -Algorithm SHA256).Hash",
            ],
        );
        command.env("COVALOS_INSTALL_ARCHIVE", &archive);
        command
    } else if cfg!(target_os = "macos") {
        let mut command = isolated("shasum", &["-a", "256"]);
        command.arg(&archive);
        command
    } else {
        let mut command = isolated("sha256sum", &[]);
        command.arg(&archive);
        command
    };
    check.current_dir(stage);
    let actual = run(check, cancel, |_| {})?;
    if !actual
        .split_whitespace()
        .any(|word| word.eq_ignore_ascii_case(hash))
    {
        return Err("Antigravity download checksum did not match.".into());
    }
    let mut extract = isolated(if cfg!(windows) { "tar.exe" } else { "tar" }, &["-xf"]);
    extract.arg(&archive).arg("-C").arg(output);
    run(extract, cancel, |_| {})?;
    let program = antigravity_bin(output);
    if !program.is_file() {
        return Err("The archive did not contain the expected Antigravity executable.".into());
    }
    let version = data["agents"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|agent| agent["id"] == "antigravity-acp")
        .and_then(|agent| agent["version"].as_str());
    if let Some(version) = version {
        fs::write(output.join("covalos-runtime-version"), version).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn install_runtime(
    root: &Path,
    kind: &ProviderKind,
    cancel: &AtomicBool,
    report: impl Fn(Progress),
) -> Result<(), String> {
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let stage = tempfile::Builder::new()
        .prefix("install-")
        .tempdir_in(root)
        .map_err(|e| e.to_string())?;
    let output = stage.path().join("runtime");
    fs::create_dir(&output).map_err(|e| e.to_string())?;
    if *kind == ProviderKind::Antigravity {
        install_antigravity(stage.path(), &output, cancel, &report)?;
    } else {
        let (package, binary) = package(kind).ok_or("This provider has no managed installer.")?;
        let mut command = isolated(
            if cfg!(windows) { "npm.cmd" } else { "npm" },
            &[
                "install",
                "--global",
                "--loglevel",
                "http",
                "--no-audit",
                "--no-fund",
                "--prefix",
            ],
        );
        command.arg(&output).arg(package);
        report(Progress::Downloading(0));
        run(command, cancel, |line| {
            if line.contains("added ") || line.contains("npm info run") {
                report(Progress::Installing);
            }
        })?;
        if !npm_bin(&output, binary).is_file() {
            return Err("The package installed without its expected CLI launcher.".into());
        }
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("Installation cancelled.".into());
    }
    fs::rename(&output, root.join(kind.key())).map_err(|e| e.to_string())
}

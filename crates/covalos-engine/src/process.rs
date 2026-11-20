//! Process limits that apply to everything the engine spawns: a scrubbed
//! environment, a process group that can be killed as a whole, a wall-clock
//! limit and bounded output capture.
//!
//! CPU, memory and network isolation are not enforced here; provider CLIs
//! keep their own sandboxes for tool execution.

use std::{
    io::Read,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

/// Variables a child may inherit. Everything else (cloud keys, tokens, other
/// providers' API keys) is withheld.
pub const INHERITED_ENV: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TERM",
    "TMPDIR",
    "TMP",
    "TEMP",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_CACHE_HOME",
    "XDG_RUNTIME_DIR",
    "SYSTEMROOT",
    "SYSTEMDRIVE",
    "WINDIR",
    "COMSPEC",
    "PATHEXT",
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "PROGRAMDATA",
    "PROGRAMFILES",
    "CARGO_HOME",
    "RUSTUP_HOME",
    "SSL_CERT_FILE",
    "NODE_EXTRA_CA_CERTS",
    "HTTPS_PROXY",
    "HTTP_PROXY",
    "NO_PROXY",
    "https_proxy",
    "http_proxy",
    "no_proxy",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessLimits {
    pub wall: Duration,
    pub max_output_bytes: usize,
}

impl Default for ProcessLimits {
    fn default() -> Self {
        Self {
            wall: Duration::from_secs(30 * 60),
            max_output_bytes: 16 * 1024 * 1024,
        }
    }
}

pub fn isolated(program: &str, args: &[&str]) -> Command {
    let mut command = Command::new(program);
    command.args(args).env_clear();
    for name in INHERITED_ENV {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
}

pub fn own_group(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
}

/// Kills the child and everything it started.
pub fn kill_tree(child: &mut Child) {
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-KILL", "--", &format!("-{}", child.id())])
            .output();
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill.exe")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .output();
    }
    let _ = child.kill();
    let _ = child.wait();
}

pub(crate) fn bounded_reader(
    mut source: impl Read + Send + 'static,
    limit: usize,
) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut kept = Vec::new();
        let mut buffer = [0_u8; 8192];
        while let Ok(n) = source.read(&mut buffer) {
            if n == 0 {
                break;
            }
            let room = limit.saturating_sub(kept.len());
            kept.extend_from_slice(&buffer[..n.min(room)]);
        }
        kept
    })
}

#[derive(Debug)]
pub struct Finished {
    pub success: bool,
    pub timed_out: bool,
    pub output: String,
}

/// Runs a command to completion under `limits`, capturing combined output.
pub fn run_limited(mut command: Command, limits: ProcessLimits) -> std::io::Result<Finished> {
    own_group(&mut command);
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = bounded_reader(child.stdout.take().expect("piped"), limits.max_output_bytes);
    let stderr = bounded_reader(child.stderr.take().expect("piped"), limits.max_output_bytes);
    let started = Instant::now();
    let (success, timed_out) = loop {
        if let Some(status) = child.try_wait()? {
            break (status.success(), false);
        }
        if started.elapsed() > limits.wall {
            kill_tree(&mut child);
            break (false, true);
        }
        thread::sleep(Duration::from_millis(10));
    };
    let mut output = stdout.join().unwrap_or_default();
    output.extend(stderr.join().unwrap_or_default());
    Ok(Finished {
        success,
        timed_out,
        output: String::from_utf8_lossy(&output).into_owned(),
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn limits_time_output_and_environment() {
        let quick = run_limited(
            isolated("sh", &["-c", "echo ok; exit 3"]),
            ProcessLimits::default(),
        )
        .unwrap();
        assert!(!quick.success && !quick.timed_out);
        assert_eq!(quick.output.trim(), "ok");

        let started = Instant::now();
        let slow = run_limited(
            isolated("sh", &["-c", "sleep 5 & sleep 5"]),
            ProcessLimits {
                wall: Duration::from_millis(200),
                ..ProcessLimits::default()
            },
        )
        .unwrap();
        assert!(slow.timed_out);
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "process group was killed"
        );

        let capped = run_limited(
            isolated("sh", &["-c", "yes | head -c 100000"]),
            ProcessLimits {
                max_output_bytes: 1000,
                ..ProcessLimits::default()
            },
        )
        .unwrap();
        assert_eq!(capped.output.len(), 1000);

        let env = run_limited(isolated("env", &[]), ProcessLimits::default()).unwrap();
        assert!(!env.output.contains("CARGO_PKG_NAME"));
    }
}

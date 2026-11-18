use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    #[default]
    Codex,
    Claude,
    Gemini,
    Antigravity,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Scope {
    #[default]
    Workspace,
    Isolated,
}

impl Provider {
    pub const ALL: [Self; 4] = [Self::Codex, Self::Claude, Self::Gemini, Self::Antigravity];

    pub fn label(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::Claude => "Claude Code",
            Self::Gemini => "Gemini CLI",
            Self::Antigravity => "Antigravity",
        }
    }

    pub fn supported(self) -> bool {
        matches!(self, Self::Codex | Self::Claude)
    }

    fn executable(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Gemini => "gemini",
            Self::Antigravity => "agy",
        }
    }

    fn override_var(self) -> &'static str {
        match self {
            Self::Codex => "AEROS_CODEX_BIN",
            Self::Claude => "AEROS_CLAUDE_BIN",
            Self::Gemini => "AEROS_GEMINI_BIN",
            Self::Antigravity => "AEROS_AGY_BIN",
        }
    }

    pub fn discover(self) -> Option<PathBuf> {
        if let Some(path) = std::env::var_os(self.override_var()).map(PathBuf::from) {
            return path
                .is_absolute()
                .then_some(path)
                .filter(|path| path.is_file());
        }
        let name = format!("{}{}", self.executable(), std::env::consts::EXE_SUFFIX);
        if let Some(path) = std::env::var_os("PATH")
            .into_iter()
            .flat_map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
            .filter(|dir| dir.is_absolute())
            .map(|dir| dir.join(&name))
            .find(|path| path.is_file())
        {
            return Some(path);
        }
        if let Some(home) = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
            let path = Path::new(&home).join(".local/bin").join(&name);
            if path.is_file() {
                return Some(path);
            }
        }
        if cfg!(windows)
            && self == Self::Codex
            && let Some(local) = std::env::var_os("LOCALAPPDATA")
            && let Ok(entries) = Path::new(&local).join("OpenAI/Codex/bin").read_dir()
        {
            return entries
                .flatten()
                .map(|entry| entry.path().join(&name))
                .filter_map(|path| {
                    path.metadata()
                        .ok()
                        .filter(|m| m.is_file())
                        .map(|m| (m.modified().ok(), path))
                })
                .max_by_key(|(modified, _)| *modified)
                .map(|(_, path)| path);
        }
        // Windows npm shims can be reported without executing a batch script via a shell.
        if cfg!(windows)
            && !self.supported()
            && let Some(path) = std::env::var_os("PATH")
        {
            return std::env::split_paths(&path)
                .filter(|dir| dir.is_absolute())
                .map(|dir| dir.join(format!("{}.cmd", self.executable())))
                .find(|path| path.is_file());
        }
        None
    }

    pub(super) fn command(
        self,
        program: &OsStr,
        scope: Scope,
        session: Option<&str>,
    ) -> Result<Command, String> {
        let mut command = Command::new(program);
        match self {
            Self::Codex => {
                command.args([
                    "exec",
                    "--sandbox",
                    "read-only",
                    "--color",
                    "never",
                    "--json",
                    "--skip-git-repo-check",
                ]);
                if scope == Scope::Isolated {
                    command.args([
                        "--ignore-user-config",
                        "--disable",
                        "shell_tool",
                        "--disable",
                        "plugins",
                        "-c",
                        "web_search=\"disabled\"",
                    ]);
                }
                if let Some(session) = session {
                    command.args(["resume", session]);
                }
                command.arg("-");
            }
            Self::Claude => {
                command.args([
                    "--print",
                    "--output-format",
                    "stream-json",
                    "--verbose",
                    "--include-partial-messages",
                    "--tools",
                    "",
                    "--strict-mcp-config",
                    "--disable-slash-commands",
                    "--settings",
                    r#"{"disableAllHooks":true}"#,
                ]);
                command.env("CLAUDE_CODE_DISABLE_AUTO_MEMORY", "1");
                if let Some(session) = session {
                    command.args(["--resume", session]);
                }
            }
            _ => return Err(format!("{} is available for detection only.", self.label())),
        }
        Ok(command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isolated_commands_disable_workspace_tools_and_keep_resume() {
        for provider in [Provider::Codex, Provider::Claude] {
            let command = provider
                .command(OsStr::new("fixture"), Scope::Isolated, Some("session-123"))
                .unwrap();
            let args: Vec<_> = command
                .get_args()
                .map(|arg| arg.to_string_lossy())
                .collect();
            assert!(args.iter().any(|arg| arg == "session-123"));
            match provider {
                Provider::Codex => {
                    assert!(args.windows(2).any(|s| s == ["--disable", "shell_tool"]))
                }
                Provider::Claude => assert!(args.windows(2).any(|s| s == ["--tools", ""])),
                _ => unreachable!(),
            }
        }
    }
}

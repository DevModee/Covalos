//! Git worktrees for parallel sessions. Each session gets its own branch
//! `covalos/agent/<session>` and directory, created and removed with the git
//! CLI so the user's repository configuration is respected.

use crate::{Error, Result, SessionId};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

pub const BRANCH_PREFIX: &str = "covalos/agent/";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub head: Option<String>,
}

pub struct WorktreeManager {
    repo: PathBuf,
    base: PathBuf,
}

impl WorktreeManager {
    /// `base` should live outside `repo` so worktrees never nest.
    pub fn new(repo: impl Into<PathBuf>, base: impl Into<PathBuf>) -> Self {
        Self {
            repo: repo.into(),
            base: base.into(),
        }
    }

    pub fn branch_for(session: &SessionId) -> String {
        format!("{BRANCH_PREFIX}{session}")
    }

    pub fn path_for(&self, session: &SessionId) -> PathBuf {
        self.base.join(session.as_str())
    }

    pub fn create(&self, session: &SessionId, base_ref: &str) -> Result<Worktree> {
        if base_ref.is_empty() || base_ref.starts_with('-') {
            return Err(Error::Invalid(format!("invalid base ref {base_ref:?}")));
        }
        let path = self.path_for(session);
        if path.exists() {
            return Err(Error::Duplicate(path.display().to_string()));
        }
        std::fs::create_dir_all(&self.base)?;
        let branch = Self::branch_for(session);
        self.git(&["worktree", "add", "-b", &branch])
            .arg(&path)
            .arg(base_ref)
            .run()?;
        self.list()?
            .into_iter()
            .find(|w| w.branch.as_deref() == Some(branch.as_str()))
            .ok_or_else(|| Error::Git(format!("worktree for {branch} not listed")))
    }

    pub fn is_dirty(&self, session: &SessionId) -> Result<bool> {
        let output = Git::new(&self.path_for(session), &["status", "--porcelain"]).run()?;
        Ok(!output.trim().is_empty())
    }

    /// Removes the worktree. A dirty worktree is kept unless `force` is set.
    /// The branch is kept so committed work is never lost.
    pub fn remove(&self, session: &SessionId, force: bool) -> Result<()> {
        if !force && self.is_dirty(session)? {
            return Err(Error::Invalid(format!(
                "worktree for {session} has uncommitted changes"
            )));
        }
        let mut git = self.git(&["worktree", "remove"]);
        if force {
            git = git.arg("--force");
        }
        git.arg(self.path_for(session)).run()?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<Worktree>> {
        let output = self.git(&["worktree", "list", "--porcelain"]).run()?;
        Ok(parse_porcelain(&output))
    }

    /// Worktrees created by this manager, i.e. on a `covalos/agent/` branch.
    pub fn owned(&self) -> Result<Vec<Worktree>> {
        Ok(self
            .list()?
            .into_iter()
            .filter(|w| {
                w.branch
                    .as_deref()
                    .is_some_and(|b| b.starts_with(BRANCH_PREFIX))
            })
            .collect())
    }

    fn git(&self, args: &[&str]) -> Git {
        Git::new(&self.repo, args)
    }
}

fn parse_porcelain(output: &str) -> Vec<Worktree> {
    output
        .split("\n\n")
        .filter_map(|block| {
            let mut worktree = Worktree {
                path: PathBuf::new(),
                branch: None,
                head: None,
            };
            for line in block.lines() {
                if let Some(path) = line.strip_prefix("worktree ") {
                    worktree.path = PathBuf::from(path);
                } else if let Some(head) = line.strip_prefix("HEAD ") {
                    worktree.head = Some(head.to_owned());
                } else if let Some(branch) = line.strip_prefix("branch ") {
                    worktree.branch = Some(
                        branch
                            .strip_prefix("refs/heads/")
                            .unwrap_or(branch)
                            .to_owned(),
                    );
                }
            }
            (!worktree.path.as_os_str().is_empty()).then_some(worktree)
        })
        .collect()
}

struct Git(Command);

impl Git {
    fn new(dir: &Path, args: &[&str]) -> Self {
        let mut command = Command::new("git");
        command.arg("-C").arg(dir).args(args);
        Self(command)
    }

    fn arg(mut self, arg: impl AsRef<std::ffi::OsStr>) -> Self {
        self.0.arg(arg);
        self
    }

    fn run(mut self) -> Result<String> {
        let output = self.0.output()?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Err(Error::Git(
                String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        fs::create_dir(&repo).unwrap();
        let git = |args: &[&str]| {
            assert!(
                Command::new("git")
                    .arg("-C")
                    .arg(&repo)
                    .args([
                        "-c",
                        "user.name=test",
                        "-c",
                        "user.email=test@example.invalid"
                    ])
                    .args(args)
                    .output()
                    .unwrap()
                    .status
                    .success()
            );
        };
        git(&["init", "-q", "-b", "main"]);
        fs::write(repo.join("README"), "x").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "init"]);
        dir
    }

    #[test]
    fn create_list_and_remove_worktrees() {
        let dir = repo();
        let manager = WorktreeManager::new(dir.path().join("repo"), dir.path().join("trees"));
        let a = SessionId::new("a").unwrap();
        let b = SessionId::new("b").unwrap();
        let wa = manager.create(&a, "main").unwrap();
        manager.create(&b, "main").unwrap();
        assert_eq!(wa.branch.as_deref(), Some("covalos/agent/a"));
        assert!(wa.path.join("README").is_file());
        assert_eq!(manager.owned().unwrap().len(), 2);
        assert!(manager.create(&a, "main").is_err());
        assert!(
            manager
                .create(&SessionId::new("c").unwrap(), "--orphan")
                .is_err()
        );

        fs::write(manager.path_for(&a).join("new.txt"), "dirty").unwrap();
        assert!(manager.is_dirty(&a).unwrap());
        assert!(manager.remove(&a, false).is_err());
        manager.remove(&a, true).unwrap();
        manager.remove(&b, false).unwrap();
        assert!(manager.owned().unwrap().is_empty());
    }

    #[test]
    fn porcelain_parsing_handles_detached_heads() {
        let parsed = parse_porcelain(
            "worktree /r\nHEAD abc\nbranch refs/heads/main\n\nworktree /t\nHEAD def\ndetached\n",
        );
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].branch.as_deref(), Some("main"));
        assert_eq!(parsed[1].branch, None);
    }
}

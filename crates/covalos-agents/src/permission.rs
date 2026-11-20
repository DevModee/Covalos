//! Permission policy and approval queue shared by every provider.
//!
//! Provider CLIs keep their own sandboxes; this layer is COVALOS's own gate
//! for actions reported through hooks or proposed by an orchestrator. Writes
//! outside the session's worktree are always denied, whatever was granted.

use crate::{Error, Result, SessionId};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "kind", content = "target")]
pub enum Action {
    Read(PathBuf),
    Write(PathBuf),
    Execute(String),
    Network(String),
    Tool(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Decision {
    Allow,
    Ask,
    Deny,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Preset {
    /// Reads inside the worktree only.
    ReadOnly,
    /// Reads inside the worktree are free; everything else asks.
    Supervised,
    /// Free reads and writes inside the worktree; commands, network and
    /// tools still ask until granted for the session.
    Worktree,
}

#[derive(Clone, Debug)]
pub struct Policy {
    root: PathBuf,
    preset: Preset,
    grants: Vec<Action>,
}

impl Policy {
    pub fn new(root: &Path, preset: Preset) -> Result<Self> {
        Ok(Self {
            root: root.canonicalize()?,
            preset,
            grants: Vec::new(),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn evaluate(&self, action: &Action) -> Decision {
        use {Decision::*, Preset::*};
        let action = match action {
            Action::Read(p) => Action::Read(resolve(&self.root, p)),
            Action::Write(p) => Action::Write(resolve(&self.root, p)),
            other => other.clone(),
        };
        let inside = match &action {
            Action::Read(p) | Action::Write(p) => p.starts_with(&self.root),
            _ => false,
        };
        if matches!(action, Action::Write(_)) && !inside {
            return Deny;
        }
        if self.grants.contains(&action) {
            return Allow;
        }
        match (self.preset, &action) {
            (_, Action::Read(_)) if inside => Allow,
            (ReadOnly, _) => Deny,
            (Worktree, Action::Write(_)) => Allow,
            _ => Ask,
        }
    }

    /// Pre-authorizes an exact action. Writes outside the worktree stay denied.
    pub fn grant(&mut self, action: Action) {
        let action = match action {
            Action::Read(p) => Action::Read(resolve(&self.root, &p)),
            Action::Write(p) => Action::Write(resolve(&self.root, &p)),
            other => other,
        };
        if !self.grants.contains(&action) {
            self.grants.push(action);
        }
    }
}

/// Resolves `path` against `root` without following `..` past real
/// directories, then canonicalizes the deepest existing ancestor so symlinks
/// cannot smuggle a path out of the worktree.
fn resolve(root: &Path, path: &Path) -> PathBuf {
    let mut lexical = PathBuf::new();
    for component in root.join(path).components() {
        match component {
            Component::ParentDir => {
                lexical.pop();
            }
            Component::CurDir => {}
            other => lexical.push(other),
        }
    }
    let mut existing = lexical.as_path();
    let mut rest = Vec::new();
    while !existing.exists() {
        let (Some(parent), Some(name)) = (existing.parent(), existing.file_name()) else {
            return lexical;
        };
        rest.push(name.to_owned());
        existing = parent;
    }
    let Ok(mut resolved) = existing.canonicalize() else {
        return lexical;
    };
    resolved.extend(rest.iter().rev());
    resolved
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub ticket: String,
    pub session: SessionId,
    pub action: Action,
    pub created_ms: u64,
    pub expires_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolution {
    AllowOnce,
    AllowForSession,
    Deny,
}

#[derive(Default)]
pub struct ApprovalQueue {
    pending: Vec<ApprovalRequest>,
    next: u64,
}

impl ApprovalQueue {
    pub fn request(
        &mut self,
        session: SessionId,
        action: Action,
        now_ms: u64,
        ttl_ms: u64,
    ) -> String {
        self.next += 1;
        let ticket = format!("{:016x}", self.next);
        self.pending.push(ApprovalRequest {
            ticket: ticket.clone(),
            session,
            action,
            created_ms: now_ms,
            expires_ms: now_ms.saturating_add(ttl_ms),
        });
        ticket
    }

    pub fn pending(&self) -> &[ApprovalRequest] {
        &self.pending
    }

    /// Returns the request and whether the action may proceed. Session grants
    /// are recorded on `policy`, which must belong to the requesting session.
    pub fn resolve(
        &mut self,
        ticket: &str,
        resolution: Resolution,
        policy: &mut Policy,
    ) -> Result<(ApprovalRequest, bool)> {
        let index = self
            .pending
            .iter()
            .position(|r| r.ticket == ticket)
            .ok_or_else(|| Error::NotFound(format!("approval {ticket}")))?;
        let request = self.pending.remove(index);
        if policy.evaluate(&request.action) == Decision::Deny {
            return Ok((request, false));
        }
        let allowed = match resolution {
            Resolution::AllowOnce => true,
            Resolution::AllowForSession => {
                policy.grant(request.action.clone());
                true
            }
            Resolution::Deny => false,
        };
        Ok((request, allowed))
    }

    /// Removes expired requests; callers must treat them as denied.
    pub fn expire(&mut self, now_ms: u64) -> Vec<ApprovalRequest> {
        let (expired, pending) = std::mem::take(&mut self.pending)
            .into_iter()
            .partition(|r| r.expires_ms <= now_ms);
        self.pending = pending;
        expired
    }

    pub fn cancel_ticket(&mut self, ticket: &str) -> Option<ApprovalRequest> {
        let index = self.pending.iter().position(|r| r.ticket == ticket)?;
        Some(self.pending.remove(index))
    }

    pub fn cancel_session(&mut self, session: &SessionId) -> Vec<ApprovalRequest> {
        let (cancelled, pending) = std::mem::take(&mut self.pending)
            .into_iter()
            .partition(|r| &r.session == session);
        self.pending = pending;
        cancelled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup(preset: Preset) -> (tempfile::TempDir, Policy) {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("wt")).unwrap();
        let policy = Policy::new(&dir.path().join("wt"), preset).unwrap();
        (dir, policy)
    }

    #[test]
    fn presets_gate_actions() {
        let (_dir, read_only) = setup(Preset::ReadOnly);
        assert_eq!(
            read_only.evaluate(&Action::Read("src/a.rs".into())),
            Decision::Allow
        );
        assert_eq!(
            read_only.evaluate(&Action::Write("src/a.rs".into())),
            Decision::Deny
        );
        assert_eq!(
            read_only.evaluate(&Action::Execute("ls".into())),
            Decision::Deny
        );

        let (_dir, supervised) = setup(Preset::Supervised);
        assert_eq!(
            supervised.evaluate(&Action::Write("a".into())),
            Decision::Ask
        );
        assert_eq!(
            supervised.evaluate(&Action::Network("example.com".into())),
            Decision::Ask
        );

        let (_dir, worktree) = setup(Preset::Worktree);
        assert_eq!(
            worktree.evaluate(&Action::Write("new/file.rs".into())),
            Decision::Allow
        );
        assert_eq!(
            worktree.evaluate(&Action::Execute("cargo test".into())),
            Decision::Ask
        );
    }

    #[test]
    fn escapes_are_denied() {
        let (dir, policy) = setup(Preset::Worktree);
        assert_eq!(
            policy.evaluate(&Action::Write("../outside".into())),
            Decision::Deny
        );
        assert_eq!(
            policy.evaluate(&Action::Write("a/../../x".into())),
            Decision::Deny
        );
        assert_eq!(
            policy.evaluate(&Action::Write("/etc/passwd".into())),
            Decision::Deny
        );
        assert_eq!(
            policy.evaluate(&Action::Read("../secret".into())),
            Decision::Ask
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path(), dir.path().join("wt/link")).unwrap();
            assert_eq!(
                policy.evaluate(&Action::Write("link/escape".into())),
                Decision::Deny
            );
        }
    }

    #[test]
    fn approvals_grant_resolve_and_expire() {
        let (_dir, mut policy) = setup(Preset::Supervised);
        let session = SessionId::new("s").unwrap();
        let mut queue = ApprovalQueue::default();
        let cmd = Action::Execute("cargo test".into());
        let t1 = queue.request(session.clone(), cmd.clone(), 0, 1000);
        let (_, allowed) = queue
            .resolve(&t1, Resolution::AllowForSession, &mut policy)
            .unwrap();
        assert!(allowed);
        assert_eq!(policy.evaluate(&cmd), Decision::Allow);
        assert_eq!(
            policy.evaluate(&Action::Execute("cargo test; rm -rf /".into())),
            Decision::Ask
        );

        let outside = queue.request(session.clone(), Action::Write("../x".into()), 0, 1000);
        let (_, allowed) = queue
            .resolve(&outside, Resolution::AllowForSession, &mut policy)
            .unwrap();
        assert!(!allowed);
        assert_eq!(
            policy.evaluate(&Action::Write("../x".into())),
            Decision::Deny
        );

        queue.request(session.clone(), Action::Tool("web".into()), 0, 10);
        queue.request(session.clone(), Action::Tool("mcp".into()), 0, 100);
        assert_eq!(queue.expire(50).len(), 1);
        assert_eq!(queue.cancel_session(&session).len(), 1);
        assert!(queue.pending().is_empty());
        assert!(
            queue
                .resolve("missing", Resolution::Deny, &mut policy)
                .is_err()
        );
    }
}

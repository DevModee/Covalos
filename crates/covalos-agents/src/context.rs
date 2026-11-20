//! Shared context: an append-only, bounded board that sessions publish to and
//! read from by cursor. Agents never see each other's raw transcripts; they
//! exchange structured entries with references that can be re-verified.

use crate::{Error, Result, SessionId, TaskId, now_ms};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, VecDeque},
    fs,
    path::{Component, Path, PathBuf},
    process::Command,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EntryKind {
    Goal,
    Decision,
    Contract,
    Finding,
    TestResult,
    FileChange,
    FileRef,
    TaskCompleted,
    Blocker,
    Review,
    Handoff,
}

/// Something an entry points at, checkable later against the workspace.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "ref")]
pub enum Reference {
    /// A workspace-relative file and the FNV-1a 64 digest of its contents.
    File {
        path: PathBuf,
        digest: String,
    },
    Commit {
        sha: String,
    },
    Task {
        id: TaskId,
    },
    Entry {
        seq: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefState {
    Valid,
    Changed,
    Missing,
    /// Not checkable against a directory; the owner of the id verifies it.
    External,
}

impl Reference {
    pub fn file(root: &Path, relative: &Path) -> Result<Self> {
        let path = relative_path(relative)?;
        Ok(Self::File {
            digest: digest(&fs::read(root.join(&path))?),
            path,
        })
    }

    pub fn commit(sha: &str) -> Result<Self> {
        if (7..=64).contains(&sha.len()) && sha.bytes().all(|b| b.is_ascii_hexdigit()) {
            Ok(Self::Commit {
                sha: sha.to_ascii_lowercase(),
            })
        } else {
            Err(Error::Invalid(format!("not a commit id: {sha:?}")))
        }
    }

    pub fn verify(&self, root: &Path) -> RefState {
        match self {
            Self::File {
                path,
                digest: expected,
            } => {
                let Ok(path) = relative_path(path) else {
                    return RefState::Missing;
                };
                match fs::read(root.join(path)) {
                    Ok(bytes) if &digest(&bytes) == expected => RefState::Valid,
                    Ok(_) => RefState::Changed,
                    Err(_) => RefState::Missing,
                }
            }
            Self::Commit { sha } => {
                let found = Command::new("git")
                    .arg("-C")
                    .arg(root)
                    .args(["cat-file", "-e", &format!("{sha}^{{commit}}")])
                    .output()
                    .is_ok_and(|o| o.status.success());
                if found {
                    RefState::Valid
                } else {
                    RefState::Missing
                }
            }
            Self::Task { .. } | Self::Entry { .. } => RefState::External,
        }
    }
}

fn relative_path(path: &Path) -> Result<PathBuf> {
    let clean = path.components().all(|c| matches!(c, Component::Normal(_)));
    if clean && !path.as_os_str().is_empty() {
        Ok(path.to_path_buf())
    } else {
        Err(Error::Invalid(format!(
            "reference must be workspace-relative: {}",
            path.display()
        )))
    }
}

/// FNV-1a 64: stable across platforms and Rust versions. It detects drift,
/// it is not a security boundary.
pub fn digest(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{hash:016x}")
}

/// Rejects text that looks like a credential. Heuristic by nature: it blocks
/// well-known token shapes and `key=value` secrets, not every possible secret.
pub fn looks_secret(text: &str) -> bool {
    const PREFIXES: [(&str, usize); 8] = [
        ("sk-", 20),
        ("ghp_", 30),
        ("gho_", 30),
        ("github_pat_", 30),
        ("glpat-", 20),
        ("xoxb-", 20),
        ("xoxp-", 20),
        ("AIza", 30),
    ];
    if text.contains("PRIVATE KEY-----") {
        return true;
    }
    let token_char = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    for word in text.split(|c: char| !token_char(c)) {
        if PREFIXES
            .iter()
            .any(|(p, min)| word.starts_with(p) && word.len() >= *min)
        {
            return true;
        }
        if word.len() == 20
            && word.starts_with("AKIA")
            && word
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        {
            return true;
        }
    }
    let lower = text.to_ascii_lowercase();
    ["password", "passwd", "secret", "api_key", "apikey", "token"]
        .iter()
        .any(|key| {
            lower.match_indices(key).any(|(i, _)| {
                let rest = lower[i + key.len()..].trim_start();
                let Some(value) = rest.strip_prefix(['=', ':']) else {
                    return false;
                };
                let value = value.trim_start().trim_start_matches(['"', '\'']);
                value.chars().take_while(|c| !c.is_whitespace()).count() >= 8
            })
        })
}

/// A new entry before it is accepted onto the board.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Draft {
    pub author: Option<SessionId>,
    pub kind: EntryKind,
    pub text: String,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub refs: Vec<Reference>,
    #[serde(default)]
    pub supersedes: Option<u64>,
}

impl Draft {
    pub fn new(kind: EntryKind, text: impl Into<String>) -> Self {
        Self {
            author: None,
            kind,
            text: text.into(),
            pinned: false,
            refs: Vec::new(),
            supersedes: None,
        }
    }

    pub fn by(mut self, author: SessionId) -> Self {
        self.author = Some(author);
        self
    }

    pub fn pinned(mut self) -> Self {
        self.pinned = true;
        self
    }

    pub fn with_ref(mut self, reference: Reference) -> Self {
        self.refs.push(reference);
        self
    }

    pub fn superseding(mut self, seq: u64) -> Self {
        self.supersedes = Some(seq);
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub seq: u64,
    pub author: Option<SessionId>,
    pub kind: EntryKind,
    pub text: String,
    pub pinned: bool,
    #[serde(default)]
    pub refs: Vec<Reference>,
    #[serde(default)]
    pub supersedes: Option<u64>,
    pub created_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SharedContext {
    entries: VecDeque<Entry>,
    next_seq: u64,
    capacity: usize,
    max_entry_chars: usize,
}

const MAX_REFS: usize = 32;

impl SharedContext {
    pub fn new(capacity: usize, max_entry_chars: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            next_seq: 1,
            capacity: capacity.max(1),
            max_entry_chars: max_entry_chars.max(1),
        }
    }

    /// Accepts a draft, truncating its text to the entry limit. Drafts that
    /// look like they carry a secret are refused.
    pub fn publish(&mut self, draft: Draft) -> Result<u64> {
        if looks_secret(&draft.text) {
            return Err(Error::Invalid(
                "context entry refused: it looks like it contains a secret".into(),
            ));
        }
        if draft.refs.len() > MAX_REFS {
            return Err(Error::Invalid(format!(
                "at most {MAX_REFS} references per entry"
            )));
        }
        if let Some(old) = draft.supersedes
            && old >= self.next_seq
        {
            return Err(Error::NotFound(format!("context entry {old}")));
        }
        let seq = self.next_seq;
        self.next_seq += 1;
        self.entries.push_back(Entry {
            seq,
            author: draft.author,
            kind: draft.kind,
            text: draft.text.chars().take(self.max_entry_chars).collect(),
            pinned: draft.pinned,
            refs: draft.refs,
            supersedes: draft.supersedes,
            created_ms: now_ms(),
        });
        while self.entries.len() > self.capacity {
            match self.entries.iter().position(|e| !e.pinned) {
                Some(index) => {
                    self.entries.remove(index);
                }
                None => break,
            }
        }
        Ok(seq)
    }

    pub fn since(&self, cursor: u64) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(move |e| e.seq > cursor)
    }

    pub fn of_kind(&self, kind: EntryKind) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(move |e| e.kind == kind)
    }

    pub fn latest_seq(&self) -> u64 {
        self.next_seq - 1
    }

    fn superseded(&self) -> BTreeSet<u64> {
        self.entries.iter().filter_map(|e| e.supersedes).collect()
    }

    /// Revision chain ending at `seq`, newest first.
    pub fn history(&self, seq: u64) -> Vec<&Entry> {
        let mut chain = Vec::new();
        let mut next = Some(seq);
        while let Some(current) = next {
            let Some(entry) = self.entries.iter().find(|e| e.seq == current) else {
                break;
            };
            chain.push(entry);
            next = entry.supersedes;
        }
        chain
    }

    /// Prompt-ready digest: pinned entries first, then the newest entries that
    /// fit in `budget` characters. The reader's own entries and superseded
    /// revisions are left out.
    pub fn digest(&self, reader: Option<&SessionId>, budget: usize) -> String {
        let superseded = self.superseded();
        let visible = |e: &&Entry| {
            (reader.is_none() || e.author.as_ref() != reader) && !superseded.contains(&e.seq)
        };
        let mut chosen: Vec<&Entry> = Vec::new();
        let mut used = 0;
        let ordered = self
            .entries
            .iter()
            .filter(|e| e.pinned)
            .chain(self.entries.iter().rev().filter(|e| !e.pinned));
        for entry in ordered.filter(visible) {
            let cost = entry.text.chars().count() + 24;
            if used + cost > budget {
                if entry.pinned {
                    continue;
                }
                break;
            }
            used += cost;
            chosen.push(entry);
        }
        chosen.sort_by_key(|e| e.seq);
        chosen
            .iter()
            .map(|e| {
                let author = e.author.as_ref().map_or("user", SessionId::as_str);
                format!("[{:?} #{} from {author}] {}", e.kind, e.seq, e.text)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(kind: EntryKind, text: &str) -> Draft {
        Draft::new(kind, text)
    }

    #[test]
    fn bounded_with_pinned_entries_kept() {
        let mut context = SharedContext::new(3, 100);
        context
            .publish(note(EntryKind::Goal, "ship v4").pinned())
            .unwrap();
        for i in 0..5 {
            context
                .publish(note(EntryKind::Finding, &format!("f{i}")))
                .unwrap();
        }
        let texts: Vec<_> = context.since(0).map(|e| e.text.as_str()).collect();
        assert_eq!(texts, ["ship v4", "f3", "f4"]);
        assert_eq!(context.latest_seq(), 6);
        assert_eq!(context.since(5).count(), 1);
    }

    #[test]
    fn digest_respects_budget_and_skips_own_entries() {
        let a = SessionId::new("a").unwrap();
        let b = SessionId::new("b").unwrap();
        let mut context = SharedContext::new(10, 5);
        context
            .publish(note(EntryKind::Goal, "goal-is-long").pinned())
            .unwrap();
        context
            .publish(note(EntryKind::Decision, "mine").by(a.clone()))
            .unwrap();
        context
            .publish(note(EntryKind::Finding, "old").by(b.clone()))
            .unwrap();
        context
            .publish(note(EntryKind::Handoff, "new").by(b))
            .unwrap();
        let digest = context.digest(Some(&a), 60);
        assert!(digest.starts_with("[Goal #1 from user] goal-"));
        assert!(!digest.contains("mine"));
        assert!(digest.contains("new"));
        assert!(!digest.contains("old"));
    }

    #[test]
    fn revisions_hide_superseded_decisions() {
        let mut context = SharedContext::new(10, 100);
        let first = context
            .publish(note(EntryKind::Decision, "use sqlite"))
            .unwrap();
        let second = context
            .publish(note(EntryKind::Decision, "use postgres").superseding(first))
            .unwrap();
        assert!(
            context
                .publish(note(EntryKind::Decision, "x").superseding(99))
                .is_err()
        );
        let digest = context.digest(None, 1000);
        assert!(digest.contains("postgres") && !digest.contains("sqlite"));
        let history: Vec<_> = context.history(second).iter().map(|e| e.seq).collect();
        assert_eq!(history, [second, first]);
        assert_eq!(context.of_kind(EntryKind::Decision).count(), 2);
    }

    #[test]
    fn secrets_are_refused() {
        let mut context = SharedContext::new(10, 1000);
        for text in [
            "key is sk-ant-api03-abcdefghijklmnopqrstuv",
            "token ghp_abcdefghijklmnopqrstuvwxyz0123456789",
            "AKIAABCDEFGHIJKLMNOP",
            "DATABASE_PASSWORD=hunter2hunter2",
            "api_key: \"abcd1234efgh5678\"",
            "-----BEGIN RSA PRIVATE KEY-----",
        ] {
            assert!(
                context.publish(note(EntryKind::Finding, text)).is_err(),
                "{text}"
            );
        }
        for text in [
            "sk-learn is a library",
            "rotate the token weekly",
            "password: short",
        ] {
            assert!(
                context.publish(note(EntryKind::Finding, text)).is_ok(),
                "{text}"
            );
        }
    }

    #[test]
    fn references_detect_drift() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("api.md"), "v1").unwrap();
        let reference = Reference::file(dir.path(), Path::new("api.md")).unwrap();
        assert_eq!(reference.verify(dir.path()), RefState::Valid);
        fs::write(dir.path().join("api.md"), "v2").unwrap();
        assert_eq!(reference.verify(dir.path()), RefState::Changed);
        fs::remove_file(dir.path().join("api.md")).unwrap();
        assert_eq!(reference.verify(dir.path()), RefState::Missing);
        assert!(Reference::file(dir.path(), Path::new("../x")).is_err());
        assert!(Reference::file(dir.path(), Path::new("/etc/hosts")).is_err());
        assert!(Reference::commit("zz").is_err());
        assert_eq!(
            Reference::commit("ABCDEF1").unwrap().verify(dir.path()),
            RefState::Missing
        );
        assert_eq!(digest(b""), "cbf29ce484222325");
    }
}

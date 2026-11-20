//! Shared context: an append-only, bounded board that sessions publish to and
//! read from by cursor. Agents never see each other's raw transcripts.

use crate::{SessionId, now_ms};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EntryKind {
    Goal,
    Decision,
    Finding,
    FileRef,
    Handoff,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub seq: u64,
    pub author: Option<SessionId>,
    pub kind: EntryKind,
    pub text: String,
    pub pinned: bool,
    pub created_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SharedContext {
    entries: VecDeque<Entry>,
    next_seq: u64,
    capacity: usize,
    max_entry_chars: usize,
}

impl SharedContext {
    pub fn new(capacity: usize, max_entry_chars: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            next_seq: 1,
            capacity: capacity.max(1),
            max_entry_chars: max_entry_chars.max(1),
        }
    }

    /// `author` is `None` for entries written by the user or the orchestrator.
    pub fn publish(
        &mut self,
        author: Option<SessionId>,
        kind: EntryKind,
        text: &str,
        pinned: bool,
    ) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.entries.push_back(Entry {
            seq,
            author,
            kind,
            text: text.chars().take(self.max_entry_chars).collect(),
            pinned,
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
        seq
    }

    pub fn since(&self, cursor: u64) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(move |e| e.seq > cursor)
    }

    pub fn latest_seq(&self) -> u64 {
        self.next_seq - 1
    }

    /// Prompt-ready digest: pinned entries first, then the newest entries that
    /// fit in `budget` characters, excluding the reader's own entries.
    pub fn digest(&self, reader: Option<&SessionId>, budget: usize) -> String {
        let foreign = |e: &&Entry| reader.is_none() || e.author.as_ref() != reader;
        let mut chosen: Vec<&Entry> = Vec::new();
        let mut used = 0;
        let ordered = self
            .entries
            .iter()
            .filter(|e| e.pinned)
            .chain(self.entries.iter().rev().filter(|e| !e.pinned));
        for entry in ordered.filter(foreign) {
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
                format!("[{:?} from {author}] {}", e.kind, e.text)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_with_pinned_entries_kept() {
        let mut context = SharedContext::new(3, 100);
        context.publish(None, EntryKind::Goal, "ship v4", true);
        for i in 0..5 {
            context.publish(None, EntryKind::Finding, &format!("f{i}"), false);
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
        context.publish(None, EntryKind::Goal, "goal-is-long", true);
        context.publish(Some(a.clone()), EntryKind::Decision, "mine", false);
        context.publish(Some(b.clone()), EntryKind::Finding, "old", false);
        context.publish(Some(b), EntryKind::Handoff, "new", false);
        let digest = context.digest(Some(&a), 60);
        assert!(digest.starts_with("[Goal from user] goal-"));
        assert!(!digest.contains("mine"));
        assert!(digest.contains("new"));
        assert!(!digest.contains("old"));
    }
}

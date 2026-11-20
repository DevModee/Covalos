//! Append-only JSON Lines journal of every decision and result. Each record is
//! flushed and synced before the engine acts on it, so a crash loses at most
//! the record being written, which replay then ignores.

use crate::{
    budget::{Budget, Spend},
    plan::{PlanState, TaskSpec, TaskStatus},
};
use covalos_agents::{
    AccountId, Error, PlanId, Result, SessionId, TaskId, context::Draft, permission::Action,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Resolver {
    Policy,
    User,
    Expired,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "record")]
pub enum Record {
    PlanCreated {
        plan: PlanId,
        goal: String,
        budget: Budget,
        tasks: Vec<TaskSpec>,
        created_ms: u64,
    },
    TaskStarted {
        task: TaskId,
        attempt: u32,
        account: AccountId,
        model: Option<String>,
        session: SessionId,
    },
    TaskSpend {
        task: TaskId,
        attempt: u32,
        spend: Spend,
    },
    /// An action was decided without blocking the task.
    Approval {
        task: TaskId,
        action: Action,
        allowed: bool,
        by: Resolver,
    },
    TaskWaiting {
        task: TaskId,
        ticket: String,
        action: Action,
    },
    TaskResumed {
        task: TaskId,
        ticket: String,
        allowed: bool,
        by: Resolver,
    },
    TaskFinished {
        task: TaskId,
        attempt: u32,
        status: TaskStatus,
    },
    /// A task waiting to retry became eligible again.
    TaskRequeued {
        task: TaskId,
    },
    /// The engine restarted while this attempt was in flight.
    TaskInterrupted {
        task: TaskId,
        attempt: u32,
    },
    Context {
        draft: Draft,
    },
    Decision {
        task: Option<TaskId>,
        text: String,
    },
    Paused,
    Resumed,
    Cancelled,
    Settled {
        state: PlanState,
    },
}

pub struct Journal {
    file: File,
    path: PathBuf,
}

impl Journal {
    /// Creates a new journal; refuses to overwrite an existing one.
    pub fn create(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .create_new(true)
            .append(true)
            .open(path)?;
        Ok(Self {
            file,
            path: path.to_path_buf(),
        })
    }

    /// Opens an existing journal for appending. A torn final record is cut
    /// off so later appends never follow a corrupt line.
    pub fn open(path: &Path) -> Result<(Self, Vec<Record>)> {
        let (records, valid_len) = scan(&std::fs::read(path)?)?;
        if std::fs::metadata(path)?.len() != valid_len as u64 {
            // Truncate through a write handle: on Windows an append-only
            // handle lacks FILE_WRITE_DATA and `set_len` is refused.
            let truncate = OpenOptions::new().write(true).open(path)?;
            truncate.set_len(valid_len as u64)?;
            truncate.sync_all()?;
        }
        let file = OpenOptions::new().append(true).open(path)?;
        Ok((
            Self {
                file,
                path: path.to_path_buf(),
            },
            records,
        ))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn append(&mut self, record: &Record) -> Result<()> {
        let mut line = serde_json::to_vec(record)?;
        line.push(b'\n');
        self.file.write_all(&line)?;
        self.file.sync_data()?;
        Ok(())
    }
}

/// Reads all complete records. A torn final line (crash mid-write) is
/// ignored; corruption anywhere else is an error.
pub fn read(path: &Path) -> Result<Vec<Record>> {
    Ok(scan(&std::fs::read(path)?)?.0)
}

/// Parses records and returns the byte length of the valid prefix.
fn scan(bytes: &[u8]) -> Result<(Vec<Record>, usize)> {
    let mut records = Vec::new();
    let mut offset = 0;
    let mut valid = 0;
    let mut number = 0;
    while offset < bytes.len() {
        number += 1;
        let end = bytes[offset..]
            .iter()
            .position(|b| *b == b'\n')
            .map(|i| offset + i + 1);
        let stop = end.unwrap_or(bytes.len());
        let text = String::from_utf8_lossy(&bytes[offset..stop]);
        if !text.trim().is_empty() {
            let last = bytes[stop..].iter().all(u8::is_ascii_whitespace);
            match serde_json::from_str::<Record>(text.trim_end()) {
                Ok(record) if end.is_some() => records.push(record),
                Ok(_) => break,
                Err(_) if last => break,
                Err(error) => {
                    return Err(Error::Invalid(format!(
                        "journal line {number} is corrupt: {error}"
                    )));
                }
            }
        }
        offset = end.unwrap_or(bytes.len());
        valid = offset;
    }
    Ok((records, valid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_torn_tail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plan.jsonl");
        let mut journal = Journal::create(&path).unwrap();
        journal.append(&Record::Paused).unwrap();
        journal
            .append(&Record::Decision {
                task: None,
                text: "use sqlite".into(),
            })
            .unwrap();
        assert!(Journal::create(&path).is_err());
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"{\"record\":\"resu")
            .unwrap();
        let (mut journal, records) = Journal::open(&path).unwrap();
        assert_eq!(records.len(), 2);
        journal.append(&Record::Resumed).unwrap();
        assert_eq!(read(&path).unwrap().last(), Some(&Record::Resumed));
    }

    #[test]
    fn corruption_in_the_middle_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plan.jsonl");
        std::fs::write(
            &path,
            "{\"record\":\"paused\"}\ngarbage\n{\"record\":\"resumed\"}\n",
        )
        .unwrap();
        assert!(read(&path).is_err());
    }
}

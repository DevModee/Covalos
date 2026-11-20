//! Provider-neutral foundation for the COVALOS multi-agent hub.
//!
//! The crate has no UI dependency. It models providers, accounts, credentials,
//! sessions, normalized events, permissions, shared context, worktrees, models,
//! quotas and task plans. Provider processes are only described here; the host
//! application decides when to spawn them.

pub mod account;
pub mod context;
pub mod credentials;
pub mod event;
pub mod launch;
pub mod models;
pub mod permission;
pub mod plan;
pub mod profile;
pub mod provider;
pub mod session;
pub mod worktree;

use std::{
    fmt,
    time::{SystemTime, UNIX_EPOCH},
};

pub use account::{Account, ConnectionMethod};
pub use event::AgentEvent;
pub use provider::{ProviderKind, ProviderRegistry, ProviderSpec};
pub use session::{Session, SessionRegistry, SessionState};

#[derive(Debug)]
pub enum Error {
    InvalidId(String),
    Duplicate(String),
    NotFound(String),
    Unsupported(String),
    InvalidTransition { from: String, to: String },
    Invalid(String),
    Io(std::io::Error),
    Json(serde_json::Error),
    Git(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId(id) => write!(f, "invalid identifier: {id:?}"),
            Self::Duplicate(what) => write!(f, "already exists: {what}"),
            Self::NotFound(what) => write!(f, "not found: {what}"),
            Self::Unsupported(what) => write!(f, "unsupported: {what}"),
            Self::InvalidTransition { from, to } => {
                write!(f, "invalid session transition from {from} to {to}")
            }
            Self::Invalid(reason) => f.write_str(reason),
            Self::Io(error) => write!(f, "I/O error: {error}"),
            Self::Json(error) => write!(f, "JSON error: {error}"),
            Self::Git(message) => write!(f, "git: {message}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Identifiers end up in file names, branch names and credential keys, so they
/// are restricted to a portable lowercase alphabet.
fn validate_id(value: &str) -> Result<()> {
    let valid = !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
        && !value.starts_with(['-', '_']);
    if valid {
        Ok(())
    } else {
        Err(Error::InvalidId(value.to_owned()))
    }
}

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> $crate::Result<Self> {
                let value = value.into();
                $crate::validate_id(&value)?;
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(
                d: D,
            ) -> std::result::Result<Self, D::Error> {
                Self::new(String::deserialize(d)?).map_err(serde::de::Error::custom)
            }
        }
    };
}

id_type!(AccountId);
id_type!(SessionId);
id_type!(TaskId);
id_type!(RoleId);
id_type!(PlanId);

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_reject_path_and_branch_hazards() {
        for bad in ["", "-x", "a/b", "..", "A", "a b", "x\\y", &"a".repeat(65)] {
            assert!(SessionId::new(bad).is_err(), "{bad:?}");
        }
        assert_eq!(AccountId::new("work-2").unwrap().as_str(), "work-2");
    }

    #[test]
    fn ids_validate_on_deserialize() {
        assert!(serde_json::from_str::<TaskId>("\"../etc\"").is_err());
        assert!(serde_json::from_str::<TaskId>("\"build_1\"").is_ok());
    }
}

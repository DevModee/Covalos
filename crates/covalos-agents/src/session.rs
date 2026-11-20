use crate::{AccountId, AgentEvent, Error, Result, SessionId, event::Usage, now_ms};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionState {
    Created,
    Running,
    AwaitingApproval,
    Idle,
    Failed,
    Cancelled,
    Closed,
}

impl SessionState {
    pub fn is_active(self) -> bool {
        matches!(self, Self::Running | Self::AwaitingApproval)
    }
}

/// One provider conversation bound to one account and, optionally, one
/// worktree. Sessions never share process state; they only share context
/// through `context::SharedContext`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub account: AccountId,
    pub worktree: Option<PathBuf>,
    pub provider_session: Option<String>,
    pub model: Option<String>,
    pub state: SessionState,
    pub usage: Usage,
    pub last_message: Option<String>,
    pub last_error: Option<String>,
    pub created_ms: u64,
    pub updated_ms: u64,
}

impl Session {
    pub fn new(id: SessionId, account: AccountId, worktree: Option<PathBuf>) -> Self {
        let now = now_ms();
        Self {
            id,
            account,
            worktree,
            provider_session: None,
            model: None,
            state: SessionState::Created,
            usage: Usage::default(),
            last_message: None,
            last_error: None,
            created_ms: now,
            updated_ms: now,
        }
    }

    /// Marks a new turn as started before the provider emits its first event.
    pub fn begin_turn(&mut self) -> Result<()> {
        use SessionState::*;
        match self.state {
            Created | Idle | Failed | Cancelled => self.set(Running),
            from => Err(invalid(from, Running)),
        }
    }

    pub fn apply(&mut self, event: &AgentEvent) -> Result<SessionState> {
        use SessionState::*;
        let state = self.state;
        match event {
            AgentEvent::Started {
                provider_session,
                model,
            } => {
                if !matches!(state, Created | Idle | Running) {
                    return Err(invalid(state, Running));
                }
                self.provider_session = Some(provider_session.clone());
                if model.is_some() {
                    self.model.clone_from(model);
                }
                self.set(Running)?;
            }
            AgentEvent::TextDelta { .. }
            | AgentEvent::ToolStarted { .. }
            | AgentEvent::Cost { .. } => self.require_active()?,
            AgentEvent::Message { text } => {
                self.require_active()?;
                self.last_message = Some(text.clone());
            }
            AgentEvent::Usage(usage) => {
                self.require_active()?;
                self.usage.input_tokens += usage.input_tokens;
                self.usage.output_tokens += usage.output_tokens;
            }
            AgentEvent::ApprovalRequested { .. } if state == Running => {
                self.set(AwaitingApproval)?
            }
            AgentEvent::ApprovalRequested { .. } => return Err(invalid(state, AwaitingApproval)),
            AgentEvent::ApprovalResolved { .. } if state == AwaitingApproval => {
                self.set(Running)?
            }
            AgentEvent::ApprovalResolved { .. } => return Err(invalid(state, Running)),
            AgentEvent::TurnCompleted if state == Running => self.set(Idle)?,
            AgentEvent::TurnCompleted => return Err(invalid(state, Idle)),
            AgentEvent::Failed { message } if state != Closed => {
                self.last_error = Some(message.clone());
                self.set(Failed)?;
            }
            AgentEvent::Cancelled if state != Closed => self.set(Cancelled)?,
            AgentEvent::Closed => self.set(Closed)?,
            AgentEvent::Failed { .. } | AgentEvent::Cancelled => {
                return Err(invalid(state, state));
            }
        }
        self.updated_ms = now_ms();
        Ok(self.state)
    }

    fn require_active(&self) -> Result<()> {
        if self.state.is_active() {
            Ok(())
        } else {
            Err(invalid(self.state, SessionState::Running))
        }
    }

    fn set(&mut self, to: SessionState) -> Result<()> {
        if self.state == SessionState::Closed {
            return Err(invalid(self.state, to));
        }
        self.state = to;
        Ok(())
    }
}

fn invalid(from: SessionState, to: SessionState) -> Error {
    Error::InvalidTransition {
        from: format!("{from:?}"),
        to: format!("{to:?}"),
    }
}

#[derive(Default)]
pub struct SessionRegistry {
    sessions: BTreeMap<SessionId, Session>,
}

impl SessionRegistry {
    pub fn insert(&mut self, session: Session) -> Result<()> {
        if self.sessions.contains_key(&session.id) {
            return Err(Error::Duplicate(format!("session {}", session.id)));
        }
        self.sessions.insert(session.id.clone(), session);
        Ok(())
    }

    pub fn get(&self, id: &SessionId) -> Option<&Session> {
        self.sessions.get(id)
    }

    pub fn get_mut(&mut self, id: &SessionId) -> Result<&mut Session> {
        self.sessions
            .get_mut(id)
            .ok_or_else(|| Error::NotFound(format!("session {id}")))
    }

    pub fn apply(&mut self, id: &SessionId, event: &AgentEvent) -> Result<SessionState> {
        self.get_mut(id)?.apply(event)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Session> {
        self.sessions.values()
    }

    pub fn active_for(&self, account: &AccountId) -> usize {
        self.iter()
            .filter(|s| &s.account == account && s.state.is_active())
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str) -> Session {
        Session::new(
            SessionId::new(id).unwrap(),
            AccountId::new("acc").unwrap(),
            None,
        )
    }

    #[test]
    fn full_turn_with_approval() {
        let mut s = session("s");
        s.begin_turn().unwrap();
        s.apply(&AgentEvent::Started {
            provider_session: "p".into(),
            model: Some("m".into()),
        })
        .unwrap();
        s.apply(&AgentEvent::ApprovalRequested {
            ticket: "t".into(),
            summary: "write".into(),
        })
        .unwrap();
        assert_eq!(s.state, SessionState::AwaitingApproval);
        assert!(s.apply(&AgentEvent::TurnCompleted).is_err());
        s.apply(&AgentEvent::ApprovalResolved {
            ticket: "t".into(),
            allowed: true,
        })
        .unwrap();
        s.apply(&AgentEvent::Usage(Usage {
            input_tokens: 2,
            output_tokens: 1,
        }))
        .unwrap();
        s.apply(&AgentEvent::Usage(Usage {
            input_tokens: 2,
            output_tokens: 1,
        }))
        .unwrap();
        s.apply(&AgentEvent::Message { text: "hi".into() }).unwrap();
        assert_eq!(
            s.apply(&AgentEvent::TurnCompleted).unwrap(),
            SessionState::Idle
        );
        assert_eq!(s.usage.input_tokens, 4);
        assert_eq!(s.model.as_deref(), Some("m"));
        assert_eq!(s.last_message.as_deref(), Some("hi"));
    }

    #[test]
    fn failures_can_be_retried_but_closed_is_final() {
        let mut s = session("s");
        s.begin_turn().unwrap();
        s.apply(&AgentEvent::Failed {
            message: "boom".into(),
        })
        .unwrap();
        assert!(
            s.apply(&AgentEvent::Message {
                text: "late".into()
            })
            .is_err()
        );
        s.begin_turn().unwrap();
        s.apply(&AgentEvent::Closed).unwrap();
        assert!(s.begin_turn().is_err());
        assert!(
            s.apply(&AgentEvent::Failed {
                message: "x".into()
            })
            .is_err()
        );
    }

    #[test]
    fn registry_keeps_sessions_independent() {
        let mut registry = SessionRegistry::default();
        registry.insert(session("a")).unwrap();
        registry.insert(session("b")).unwrap();
        assert!(registry.insert(session("a")).is_err());
        let a = SessionId::new("a").unwrap();
        registry.get_mut(&a).unwrap().begin_turn().unwrap();
        assert_eq!(registry.active_for(&AccountId::new("acc").unwrap()), 1);
        assert_eq!(
            registry.get(&SessionId::new("b").unwrap()).unwrap().state,
            SessionState::Created
        );
    }
}

//! The boundary between the engine and whatever does the work. Executors run
//! on worker threads and talk to the engine only through a `Channel`.

use crate::{plan::TaskSpec, role::Role};
use covalos_agents::{
    AccountId, AgentEvent, ProviderKind, SessionId, TaskId, context::Draft, permission::Action,
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, RecvTimeoutError, Sender},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Everything an executor needs for one attempt of one task.
#[derive(Clone, Debug)]
pub struct Assignment {
    pub goal: String,
    pub task: TaskSpec,
    pub role: Role,
    pub account: AccountId,
    pub provider: ProviderKind,
    pub model: Option<String>,
    pub workdir: PathBuf,
    /// Shared-context digest; never another agent's transcript.
    pub context: String,
    pub attempt: u32,
    pub session: SessionId,
}

impl Assignment {
    pub fn prompt(&self) -> String {
        let responsibilities: String = self
            .role
            .responsibilities
            .iter()
            .map(|r| format!("- {r}\n"))
            .collect();
        let context = if self.context.is_empty() {
            "(none yet)".to_owned()
        } else {
            self.context.clone()
        };
        format!(
            "You are the {role} on a COVALOS plan.\n\
             Project goal: {goal}\n\n\
             Your responsibilities:\n{responsibilities}\n\
             Task {id}: {title}\n{instructions}\n\n\
             Shared project context:\n{context}\n\n\
             Rules: work only inside {dir}. Do not push, publish, deploy, install \
             dependencies, read credentials or expose network services; stop and \
             explain instead. This is attempt {attempt}.",
            role = self.role.name,
            goal = self.goal,
            id = self.task.id,
            title = self.task.title,
            instructions = self.task.instructions,
            dir = self.workdir.display(),
            attempt = self.attempt,
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Report {
    Event(AgentEvent),
    /// A file the executor changed, relative to the workdir.
    FileChanged(PathBuf),
    Publish(Draft),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Succeeded { summary: String },
    Failed { reason: String, retryable: bool },
    Cancelled,
}

pub(crate) enum Body {
    Report(Report),
    Approval { action: Action, reply: Sender<bool> },
    Done(Outcome),
}

pub(crate) struct Message {
    pub task: TaskId,
    pub attempt: u32,
    pub body: Body,
}

pub struct Channel {
    tx: Sender<Message>,
    task: TaskId,
    attempt: u32,
    cancel: CancelToken,
}

impl Channel {
    pub(crate) fn new(
        tx: Sender<Message>,
        task: TaskId,
        attempt: u32,
        cancel: CancelToken,
    ) -> Self {
        Self {
            tx,
            task,
            attempt,
            cancel,
        }
    }

    pub(crate) fn send(&self, body: Body) -> bool {
        self.tx
            .send(Message {
                task: self.task.clone(),
                attempt: self.attempt,
                body,
            })
            .is_ok()
    }

    pub fn report(&self, report: Report) {
        self.send(Body::Report(report));
    }

    /// Asks the engine for permission and blocks until it decides. Returns
    /// `false` when denied, when the task is cancelled, or when the engine is
    /// gone.
    pub fn request(&self, action: Action) -> bool {
        let (reply, answer) = mpsc::channel();
        if !self.send(Body::Approval { action, reply }) {
            return false;
        }
        loop {
            match answer.recv_timeout(Duration::from_millis(25)) {
                Ok(allowed) => return allowed && !self.is_cancelled(),
                Err(RecvTimeoutError::Timeout) if !self.is_cancelled() => {}
                Err(_) => return false,
            }
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    /// Sleeps unless cancelled first; returns whether it slept fully.
    pub fn sleep(&self, duration: Duration) -> bool {
        let until = Instant::now() + duration;
        while Instant::now() < until {
            if self.is_cancelled() {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5).min(until - Instant::now()));
        }
        !self.is_cancelled()
    }
}

pub trait Executor: Send + Sync {
    fn execute(&self, assignment: &Assignment, channel: &Channel) -> Outcome;
}

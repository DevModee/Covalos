//! The seam between Student Mode and whatever hosts it. Covalos implements this with its
//! code editor and agent sessions; a standalone app would implement it with its own.
use crate::cloud::Endpoint;
use std::path::Path;

/// A tutor message in flight.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TutorTicket {
    pub(crate) conversation: String,
    /// Replies already in the conversation when this message was sent.
    pub(crate) seen: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TutorPoll {
    Pending,
    Reply(String),
    Failed(String),
}

/// An AI account the tutor can run on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TutorAccount {
    pub(crate) id: String,
    pub(crate) label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TutorAccounts {
    Ready(Vec<TutorAccount>),
    /// Sign-in checks are still running; accounts may turn ready in a moment.
    Checking,
    /// Nothing connected.
    Missing,
}

pub(crate) struct CloudCredentials {
    pub(crate) endpoint: Endpoint,
    pub(crate) access: String,
    pub(crate) user_id: String,
}

pub(crate) trait StudentWorkspaceHost {
    /// Accounts that can run the tutor now. Calling it early starts the sign-in checks.
    fn tutor_accounts(&mut self) -> TutorAccounts;
    /// Sends `prompt` to the tutor on `account`, continuing `conversation` when given.
    /// `root` is the student's workspace, which the tutor may read but never edit.
    fn ask_tutor(
        &mut self,
        account: &str,
        conversation: Option<&str>,
        prompt: &str,
        root: &Path,
    ) -> Result<TutorTicket, String>;
    fn tutor_reply(&self, ticket: &TutorTicket) -> TutorPoll;
    /// Shows where the student signs in to an AI provider.
    fn open_agents(&mut self);
    /// Opens the file in the host's code editor.
    fn open_exercise(&mut self, path: &Path);
    /// The editor's current text for the file, including unsaved edits.
    fn buffer_text(&self, path: &Path) -> Option<String>;
    /// Saves unsaved edits so the tests run against what the student sees.
    fn save_buffer(&mut self, path: &Path) -> Result<(), String>;
    fn cloud(&self) -> Option<CloudCredentials>;
    /// Whether the interface is in Spanish, so the tutor answers in the same language.
    fn spanish(&self) -> bool;
}

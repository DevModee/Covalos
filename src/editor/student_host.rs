//! Covalos as the host of Student Mode: the tutor is a read-only agent session in the
//! student's practice workspace, exercises open in floating Clean Code editors, and
//! the backdrop is the identity field (cobalt dither over black in OLED Dark).
use super::Editor;
use super::i18n::{Language, tr};
use super::student::host::{
    CloudCredentials, StudentWorkspaceHost, TutorAccount, TutorAccounts, TutorPoll, TutorTicket,
};
use crate::agents_runtime::{ClaudeModes, NewSession};
use covalos_agents::permission::Preset;
use eframe::egui;
use std::path::Path;

struct EditorHost<'a> {
    editor: &'a mut Editor,
    ctx: egui::Context,
}

fn same_file(a: &Path, b: &Path) -> bool {
    a == b || a.canonicalize().ok() == b.canonicalize().ok()
}

impl StudentWorkspaceHost for EditorHost<'_> {
    fn tutor_accounts(&mut self) -> TutorAccounts {
        // Opening the runtime starts CLI detection and sign-in checks in the background.
        if !self.editor.ensure_runtime(&self.ctx) {
            return TutorAccounts::Missing;
        }
        let agents = &self.editor.hub.agents;
        let Some(runtime) = agents.runtime.as_ref() else {
            return TutorAccounts::Missing;
        };
        let mut ready: Vec<TutorAccount> = super::agent_chat::chat_accounts(runtime)
            .into_iter()
            .map(|a| TutorAccount {
                id: a.id.as_str().to_owned(),
                label: a.label,
            })
            .collect();
        // The account picked in Agents goes first, so the tutor matches it by default.
        if let super::agent_chat::Target::Account(chosen) = &agents.chat.target
            && let Some(at) = ready.iter().position(|a| a.id == chosen.as_str())
        {
            ready[..=at].rotate_right(1);
        }
        if !ready.is_empty() {
            TutorAccounts::Ready(ready)
        } else if runtime.connections_settling() {
            TutorAccounts::Checking
        } else {
            TutorAccounts::Missing
        }
    }

    fn ask_tutor(
        &mut self,
        account: &str,
        conversation: Option<&str>,
        prompt: &str,
        root: &Path,
    ) -> Result<TutorTicket, String> {
        if !self.editor.ensure_runtime(&self.ctx) {
            return Err(tr("Agents are unavailable.").to_owned());
        }
        let runtime = self
            .editor
            .hub
            .agents
            .runtime
            .as_mut()
            .ok_or_else(|| tr("Agents are unavailable.").to_owned())?;
        let existing = conversation.and_then(|id| {
            runtime
                .sessions
                .iter()
                .position(|s| s.meta.id.as_str() == id)
        });
        let index = match existing {
            Some(index) => index,
            None => {
                let accounts = super::agent_chat::chat_accounts(runtime);
                let account = accounts
                    .iter()
                    .find(|a| a.id.as_str() == account)
                    .ok_or_else(|| tr("Connect an AI in Agents to use the tutor.").to_owned())?;
                // Read only: the tutor can look at the student's files but never write
                // the exercise for them.
                runtime.create_session(NewSession {
                    title: tr("Student Mode tutor").to_owned(),
                    account: account.id.clone(),
                    model: None,
                    effort: None,
                    claude_modes: ClaudeModes::default(),
                    root: root.to_owned(),
                    worktree: false,
                    preset: Preset::ReadOnly,
                    allow_safe_commands: false,
                    share_context: false,
                })?
            }
        };
        let session = &runtime.sessions[index];
        let seen = session
            .meta
            .log
            .iter()
            .filter(|l| l.kind == "agent")
            .count();
        let id = session.meta.id.as_str().to_owned();
        runtime.send_chat(index, prompt, tr("Student Mode turn"))?;
        Ok(TutorTicket {
            conversation: id,
            seen,
        })
    }

    fn tutor_reply(&self, ticket: &TutorTicket) -> TutorPoll {
        let Some(runtime) = self.editor.hub.agents.runtime.as_ref() else {
            return TutorPoll::Failed(tr("Agents are unavailable.").to_owned());
        };
        let Some(session) = runtime
            .sessions
            .iter()
            .find(|s| s.meta.id.as_str() == ticket.conversation)
        else {
            return TutorPoll::Failed(tr("The tutor session was closed.").to_owned());
        };
        if session.running() {
            return TutorPoll::Pending;
        }
        let replies: Vec<&str> = session
            .meta
            .log
            .iter()
            .filter(|l| l.kind == "agent")
            .skip(ticket.seen)
            .map(|l| l.text.as_str())
            .collect();
        if !replies.is_empty() {
            return TutorPoll::Reply(replies.join("\n\n"));
        }
        let error = session
            .meta
            .log
            .iter()
            .rev()
            .find(|l| l.kind == "error")
            .map(|l| l.text.clone());
        TutorPoll::Failed(error.unwrap_or_else(|| {
            tr("The tutor did not answer. Check the session in Agents.").to_owned()
        }))
    }

    fn open_agents(&mut self) {
        self.editor.switch_mode(super::modes::Mode::Agents);
        self.editor.open_providers();
    }

    fn open_exercise(&mut self, path: &Path) {
        self.editor.open_file(path.to_owned());
        if let Some(index) = self
            .editor
            .documents
            .iter()
            .position(|d| same_file(&d.path, path))
        {
            self.editor.detach(index);
        }
    }

    fn buffer_text(&self, path: &Path) -> Option<String> {
        self.editor
            .documents
            .iter()
            .find(|d| same_file(&d.path, path))
            .map(|d| d.text.clone())
            .or_else(|| std::fs::read_to_string(path).ok())
    }

    fn save_buffer(&mut self, path: &Path) -> Result<(), String> {
        match self
            .editor
            .documents
            .iter_mut()
            .find(|d| same_file(&d.path, path))
        {
            Some(doc) if doc.dirty => doc
                .save()
                .map_err(|e| format!("{} {e}", tr("Could not save file."))),
            _ => Ok(()),
        }
    }

    fn cloud(&self) -> Option<CloudCredentials> {
        let (endpoint, session) = self.editor.cloud.signed_in()?;
        Some(CloudCredentials {
            endpoint: endpoint.clone(),
            access: session.access.expose().to_owned(),
            user_id: session.user_id.clone(),
        })
    }

    fn spanish(&self) -> bool {
        self.editor.preferences.language == Language::Spanish
    }
}

impl Editor {
    pub(super) fn student_view(&mut self, ui: &mut egui::Ui) {
        let rect = ui.available_rect_before_wrap();
        let mood = self.agent_mood(ui.input(|i| i.focused));
        self.identity
            .paint(ui, rect, &self.preferences.identity, mood);
        // The dither stays as ambience at the top; the reading area below goes dark.
        let top = rect.top() + rect.height() * 0.35;
        let vertices = [
            (egui::pos2(rect.left(), top), 0),
            (egui::pos2(rect.right(), top), 0),
            (rect.left_bottom(), 235),
            (rect.right_bottom(), 235),
        ]
        .map(|(pos, alpha)| egui::epaint::Vertex {
            pos,
            uv: egui::epaint::WHITE_UV,
            color: egui::Color32::from_black_alpha(alpha),
        });
        ui.painter().add(egui::Mesh {
            indices: vec![0, 1, 2, 2, 1, 3],
            vertices: vertices.to_vec(),
            ..Default::default()
        });
        // The controller is moved out for the frame so the host can borrow the editor.
        let mut student = std::mem::take(&mut self.student);
        let ctx = ui.ctx().clone();
        student.ui(ui, &mut EditorHost { editor: self, ctx });
        // Agents may have been opened from the view (Connect an AI); they replace it.
        if self.hub_open {
            student.open = false;
        }
        self.student = student;
    }
}

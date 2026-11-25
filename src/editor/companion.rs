use super::theme::colors;
use crate::agent_bridge::{self, Session, State};
use eframe::egui;
use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};
#[derive(Clone)]
pub(super) struct Snapshot {
    pub(super) id: String,
    pub(super) label: String,
    pub(super) state: State,
    pub(super) operation: String,
    pub(super) scope: String,
    pub(super) file: Option<String>,
    pub(super) external: Option<Session>,
}
#[derive(Clone)]
pub(super) struct Notice {
    pub(super) text: String,
    pub(super) state: State,
    pub(super) at: Instant,
}
#[derive(Default)]
pub(super) struct Companion {
    pub(super) open: bool,
    pub(super) sessions: Vec<Snapshot>,
    external: Vec<Session>,
    monitor: Option<agent_bridge::Monitor>,
    known: HashMap<String, State>,
    pub(super) history: VecDeque<Notice>,
    last_attention: Option<Instant>,
    pub(super) was_open: bool,
}
impl Companion {
    pub(super) fn start(&mut self, ctx: &egui::Context) {
        if self.monitor.is_some() {
            return;
        }
        if let Some(dir) = agent_bridge::directory() {
            let ctx = ctx.clone();
            self.monitor = Some(agent_bridge::Monitor::start(dir, move || {
                ctx.request_repaint()
            }));
        }
    }
    pub(super) fn sync(
        &mut self,
        mut internal: Vec<Snapshot>,
        ctx: &egui::Context,
        quiet: bool,
        interval: u64,
    ) {
        if let Some(monitor) = &self.monitor {
            for events in monitor.events.try_iter() {
                self.external = events;
            }
        }
        internal.extend(self.external.iter().map(|s| Snapshot {
            id: format!("external:{}:{}", s.agent, s.id),
            label: s.agent.clone(),
            state: s.state,
            operation: s.operation.clone(),
            scope: s.scope.clone(),
            file: s.file.clone(),
            external: Some(s.clone()),
        }));
        for session in &internal {
            let previous = self.known.insert(session.id.clone(), session.state);
            if previous != Some(session.state)
                && (previous.is_some() || session.state != State::Idle)
            {
                self.history.push_front(Notice {
                    text: format!("{} · {}", session.label, session.state.label()),
                    state: session.state,
                    at: Instant::now(),
                });
                self.history.truncate(50);
                let important = matches!(
                    session.state,
                    State::Waiting | State::Failed | State::Completed
                );
                if !quiet
                    && important
                    && !ctx.input(|i| i.focused)
                    && self
                        .last_attention
                        .is_none_or(|t| t.elapsed() >= Duration::from_secs(interval))
                {
                    ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                        egui::UserAttentionType::Informational,
                    ));
                    self.last_attention = Some(Instant::now());
                }
                if !quiet {
                    ctx.request_repaint_after(Duration::from_secs(6));
                }
            }
        }
        self.known
            .retain(|id, _| internal.iter().any(|s| &s.id == id));
        self.sessions = internal;
    }
    pub(super) fn reported_mood(&self) -> super::identity::Mood {
        use super::identity::Mood;
        for session in &self.external {
            if agent_bridge::now_ms().saturating_sub(session.updated_ms) > 300_000 {
                continue;
            }
            match session.state {
                State::Waiting => return Mood::Approval,
                State::Running => {
                    return if session.operation.starts_with("Write")
                        || session.operation.starts_with("Edit")
                    {
                        Mood::Coding
                    } else if session.operation.starts_with("Read")
                        || session.operation.starts_with("Grep")
                        || session.operation.starts_with("Glob")
                    {
                        Mood::Searching
                    } else {
                        Mood::Thinking
                    };
                }
                State::Failed
                    if agent_bridge::now_ms().saturating_sub(session.updated_ms) < 8000 =>
                {
                    return Mood::Error;
                }
                _ => {}
            }
        }
        Mood::Idle
    }
    /// Drop external fixture sessions and stop watching the bridge (evidence captures).
    #[cfg(all(test, windows))]
    pub(super) fn clear_external(&mut self) {
        self.monitor = None;
        self.external.clear();
        self.sessions.clear();
        self.known.clear();
        self.history.clear();
    }
    pub(super) fn counts(&self) -> (usize, usize) {
        (
            self.sessions
                .iter()
                .filter(|s| s.state == State::Running)
                .count(),
            self.sessions
                .iter()
                .filter(|s| s.state == State::Waiting)
                .count(),
        )
    }
}
pub(super) enum CompanionAction {
    Decide(Session, bool),
    Workspace(String),
    Focus(String),
}
pub(super) fn state_color(state: State) -> egui::Color32 {
    match state {
        State::Running => colors().accent,
        State::Waiting => colors().warning,
        State::Completed => colors().success,
        State::Failed => colors().error,
        _ => colors().muted,
    }
}
#[path = "companion_ui.rs"]
mod ui;

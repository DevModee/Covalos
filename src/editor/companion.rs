use super::{
    Editor, Pending,
    theme::{self, *},
};
use crate::agent_bridge::{self, Session, State};
use eframe::egui::{self, RichText, Ui};
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
    text: String,
    state: State,
    at: Instant,
}
#[derive(Default)]
pub(super) struct Companion {
    pub(super) open: bool,
    pub(super) sessions: Vec<Snapshot>,
    external: Vec<Session>,
    monitor: Option<agent_bridge::Monitor>,
    known: HashMap<String, State>,
    history: VecDeque<Notice>,
    last_attention: Option<Instant>,
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
impl Editor {
    pub(super) fn companion_panel(&mut self, ui: &mut Ui) {
        enter(ui, egui::Id::new("companion-entry"));
        let (running, waiting) = self.companion.counts();
        egui::Frame::new()
            .fill(colors().inset)
            .corner_radius(12)
            .inner_margin(egui::Margin::same(18))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (_, art) = ui.allocate_space(egui::vec2(76.0, 76.0));
                    super::assets::luma_badge(ui, art);
                    ui.vertical(|ui| {
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("AEROS COMPANION")
                                .size(10.0)
                                .color(colors().muted),
                        );
                        ui.label(
                            RichText::new(if waiting > 0 {
                                "Your attention, please."
                            } else if running > 0 {
                                "Work in motion."
                            } else {
                                "A quiet workspace."
                            })
                            .size(22.0)
                            .strong(),
                        );
                        ui.label(secondary(format!(
                            "{running} running   ·   {waiting} awaiting a decision"
                        )));
                    });
                });
            });
        ui.add_space(20.0);
        if self.companion.sessions.is_empty() {
            ui.label(RichText::new("Nothing needs your attention").strong());
            ui.add_space(6.0);
            ui.add(egui::Label::new(secondary("Sessions appear here when an agent runs in AEROS or connects from another tool. Luma keeps this space quiet until then.")).wrap());
            ui.add_space(16.0);
        }
        let mut action = None;
        for session in &self.companion.sessions {
            ui.push_id(&session.id,|ui|{
                egui::Frame::new().fill(colors().elevated).corner_radius(10).inner_margin(egui::Margin::same(14)).stroke(egui::Stroke::new(1.0_f32,if session.state==State::Waiting{colors().warning.gamma_multiply(0.55)}else{colors().border.gamma_multiply(0.6)})).show(ui,|ui|{
                ui.set_width(ui.available_width());
                ui.horizontal(|ui|{let color=state_color(session.state);let (_,r)=ui.allocate_space(egui::vec2(10.0,18.0));ui.painter().circle_filled(r.center(),3.0,color);ui.label(RichText::new(&session.label).strong());ui.label(RichText::new(session.state.label()).size(SMALL).color(color));});
                if !session.operation.is_empty(){ui.add(egui::Label::new(secondary(&session.operation)).wrap());}
                if let Some(file)=&session.file{ui.add(egui::Label::new(muted(file)).wrap());}
                ui.add(egui::Label::new(muted(&session.scope)).wrap());
                if let Some(external)=&session.external {
                    ui.label(muted(format!("Session {} · last event {} s ago",external.id,agent_bridge::now_ms().saturating_sub(external.updated_ms)/1000)));
                    if session.state==State::Waiting {
                        if external.approvable && external.deadline_ms>agent_bridge::now_ms(){
                            ui.label(RichText::new("Review the operation and workspace above. This decision applies once.").color(colors().warning));
                            ui.horizontal(|ui|{if ui.add(primary("Allow once")).clicked(){action=Some(CompanionAction::Decide(external.clone(),true));}
 if ui.button("Deny").clicked(){action=Some(CompanionAction::Decide(external.clone(),false));}});
                        }else{ui.label(muted("Answer this request in its original terminal."));}
                    }
                    ui.horizontal(|ui|{if ui.button("Copy session ID").clicked(){ui.ctx().copy_text(external.id.clone());}
 if !external.scope.is_empty()&&ui.button("Open workspace").clicked(){action=Some(CompanionAction::Workspace(external.scope.clone()));}});
                }else if ui.button("Open session").clicked(){action=Some(CompanionAction::Focus(session.id.clone()));}
                });
                ui.add_space(10.0);
            });
        }
        if self.hub.approval_pending() {
            section(
                ui,
                "AEROS buffer approval",
                "The proposed change is staged in Agent Hub. It has not been saved to disk.",
            );
            if ui.add(primary("Review proposed change")).clicked() {
                self.hub.focus_approvals();
                self.hub_open = true;
                self.companion.open = false;
            }
        }
        egui::CollapsingHeader::new("Notifications").show(ui, |ui| {
            for notice in &self.companion.history {
                ui.label(RichText::new(&notice.text).color(state_color(notice.state)));
            }
            if self.companion.history.is_empty() {
                ui.label(muted("No activity yet."));
            }
        });
        ui.add_space(12.0);
        ui.label(muted("Connected tools report their own activity. Approvals stay in the original tool unless a request is shown here."));
        if let Some(action) = action {
            match action {
                CompanionAction::Decide(session, allow) => {
                    let result = agent_bridge::directory()
                        .ok_or_else(|| std::io::Error::other("No bridge directory"))
                        .and_then(|dir| agent_bridge::decide(&dir, &session, allow));
                    match result {
                        Ok(()) => {
                            self.notice =
                                Some((false, if allow { "Allowed once" } else { "Denied" }.into()));
                            for s in &mut self.companion.sessions {
                                if let Some(ext) = &mut s.external
                                    && ext.ticket == session.ticket
                                {
                                    ext.approvable = false;
                                }
                            }
                        }
                        Err(error) => self.error(format!("Could not answer request. {error}")),
                    }
                }
                CompanionAction::Workspace(scope) => {
                    let path = std::path::PathBuf::from(scope);
                    if path.is_dir() {
                        self.request(Pending::OpenWorkspace(path), ui.ctx());
                    } else {
                        self.error("The reported workspace is unavailable.");
                    }
                }
                CompanionAction::Focus(id) => {
                    self.hub.focus_session(&id);
                    self.hub_open = true;
                    self.companion.open = false;
                }
            }
        }
    }
    pub(super) fn companion_surface(&mut self, ctx: &egui::Context) {
        if self.companion.open {
            let mut open = true;
            egui::Window::new("AEROS Companion")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .fixed_size(egui::vec2(
                    (ctx.content_rect().width() - 64.0).clamp(300.0, 540.0),
                    (ctx.content_rect().height() - 140.0).clamp(
                        220.0,
                        if self.companion.sessions.is_empty() {
                            340.0
                        } else {
                            560.0
                        },
                    ),
                ))
                .vscroll(true)
                .constrain_to(ctx.content_rect().shrink(16.0))
                .show(ctx, |ui| self.companion_panel(ui));
            self.companion.open &= open;
        }
        if !self.preferences.quiet_notifications
            && !self.companion.open
            && let Some(notice) = self
                .companion
                .history
                .front()
                .filter(|n| n.at.elapsed() < Duration::from_secs(6))
        {
            let notice = notice.clone();
            egui::Area::new(egui::Id::new("agent-notification"))
                .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -44.0))
                .show(ctx, |ui| {
                    theme::modal_frame().show(ui, |ui| {
                        super::motion::enter(
                            ui,
                            egui::Id::new("agent-notification-entry"),
                            super::motion::CONTROL,
                        );
                        ui.ctx().request_repaint_after(
                            Duration::from_secs(6).saturating_sub(notice.at.elapsed()),
                        );
                        ui.set_max_width(300.0);
                        ui.label(RichText::new(&notice.text).color(state_color(notice.state)));
                        if ui.button("Show agents").clicked() {
                            self.companion.open = true;
                        }
                    });
                });
        }
    }
}
enum CompanionAction {
    Decide(Session, bool),
    Workspace(String),
    Focus(String),
}
fn state_color(state: State) -> egui::Color32 {
    match state {
        State::Running => colors().accent,
        State::Waiting => colors().warning,
        State::Completed => colors().success,
        State::Failed => colors().error,
        _ => colors().muted,
    }
}

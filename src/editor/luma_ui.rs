//! Luma as tech lead: a short order box with an effort ceiling, a chat where
//! Luma reports what it launched and every real failure in full, the team's
//! live status, and the confirmations only the user can give.
use std::collections::BTreeMap;

use covalos_agents::permission::Resolution;
use covalos_engine::{PlanState, TaskStatus, plan::Task};
use eframe::egui::{self, Frame, Margin, RichText, TextEdit, Ui};

use super::{Editor, theme::*};
use crate::{
    agents_runtime::{LiveSession, Runtime, Staffing, Status},
    luma::{self, Decision, Effort},
};

const HISTORY: usize = 80;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tone {
    User,
    Info,
    Success,
    Warning,
    Failure,
}

#[derive(Clone, Debug)]
struct Line {
    tone: Tone,
    text: String,
    /// Full error output, shown verbatim under the line.
    detail: Option<String>,
}

/// An order held back because it looks harder than the user's ceiling.
struct Held {
    order: String,
    decision: Decision,
}

/// What Luma last reported, so each change is said once.
#[derive(Default)]
struct Seen {
    plan: Option<String>,
    settled: bool,
    tasks: BTreeMap<String, String>,
    sessions: BTreeMap<String, Status>,
    planning: bool,
}

#[derive(Default)]
pub(super) struct LumaChat {
    pub(super) draft: String,
    pub(super) focus: bool,
    lines: Vec<Line>,
    held: Option<Held>,
    seen: Seen,
}

impl LumaChat {
    fn say(&mut self, tone: Tone, text: impl Into<String>) {
        self.say_with(tone, text, None);
    }

    fn say_with(&mut self, tone: Tone, text: impl Into<String>, detail: Option<String>) {
        self.lines.push(Line {
            tone,
            text: text.into(),
            detail,
        });
        if self.lines.len() > HISTORY {
            self.lines.remove(0);
        }
    }

    /// Reports every change in the runtime since the last frame.
    fn watch(&mut self, runtime: &Runtime) {
        self.watch_planning(runtime);
        self.watch_plan(runtime);
        self.watch_sessions(runtime);
    }

    fn watch_planning(&mut self, runtime: &Runtime) {
        let planning = runtime.planning.is_some();
        if self.seen.planning && !planning {
            match &runtime.notice {
                Some((false, text)) => self.say_with(
                    Tone::Failure,
                    tr("The team plan could not start."),
                    Some(text.clone()),
                ),
                _ => self.say(Tone::Info, tr("Plan written. The team is starting.")),
            }
        }
        self.seen.planning = planning;
    }

    fn watch_plan(&mut self, runtime: &Runtime) {
        let Some(run) = &runtime.plan else { return };
        let plan = run.engine.plan();
        if self.seen.plan.as_deref() != Some(plan.id.as_str()) {
            self.seen.plan = Some(plan.id.as_str().to_owned());
            self.seen.settled = false;
            self.seen.tasks.clear();
        }
        for task in plan.tasks() {
            let key = format!("{:?}", task.status);
            let id = task.spec.id.to_string();
            if self.seen.tasks.get(&id) == Some(&key) {
                continue;
            }
            self.seen.tasks.insert(id, key);
            self.report_task(task);
        }
        if plan.state.is_settled() && !self.seen.settled {
            self.seen.settled = true;
            self.report_settled(&plan.state);
        }
    }

    fn report_task(&mut self, task: &Task) {
        let title = task.spec.title.clone();
        let account = task
            .account
            .as_ref()
            .map_or_else(|| tr("an agent").to_owned(), ToString::to_string);
        match &task.status {
            TaskStatus::Running => self.say(
                Tone::Info,
                trf(
                    "{task}: working on {account}.",
                    &[("task", title), ("account", account)],
                ),
            ),
            TaskStatus::AwaitingApproval => self.say(
                Tone::Warning,
                trf("{task} needs your confirmation.", &[("task", title)]),
            ),
            TaskStatus::Retrying { reason, .. } => self.say_with(
                Tone::Warning,
                trf("{task} hit a problem and will retry.", &[("task", title)]),
                Some(reason.clone()),
            ),
            TaskStatus::Succeeded => {
                self.say(Tone::Success, trf("{task}: done.", &[("task", title)]));
            }
            TaskStatus::Failed { reason } => self.say_with(
                Tone::Failure,
                trf("{task} failed.", &[("task", title)]),
                Some(reason.clone()),
            ),
            TaskStatus::Pending | TaskStatus::Cancelled | TaskStatus::Skipped { .. } => {}
        }
    }

    fn report_settled(&mut self, state: &PlanState) {
        match state {
            PlanState::Succeeded => {
                self.say(
                    Tone::Success,
                    tr("Team run finished. Every task succeeded."),
                );
            }
            PlanState::Failed { reason } => self.say_with(
                Tone::Failure,
                tr("Team run finished with failures."),
                Some(reason.clone()),
            ),
            PlanState::Cancelled => self.say(Tone::Warning, tr("Team run cancelled.")),
            PlanState::Running | PlanState::Paused => {}
        }
    }

    fn watch_sessions(&mut self, runtime: &Runtime) {
        for session in &runtime.sessions {
            let id = session.meta.id.as_str().to_owned();
            let status = session.meta.status;
            let before = self.seen.sessions.insert(id, status);
            if status == Status::Failed && before.is_some_and(|b| b != Status::Failed) {
                let error = last_log(session, &["error"]).unwrap_or_default();
                self.say_with(
                    Tone::Failure,
                    trf(
                        "The chat {title} failed.",
                        &[("title", session.meta.title.clone())],
                    ),
                    Some(error),
                );
            }
        }
    }
}

fn last_log(session: &LiveSession, kinds: &[&str]) -> Option<String> {
    session
        .meta
        .log
        .iter()
        .rev()
        .find(|l| kinds.contains(&l.kind.as_str()))
        .map(|l| l.text.clone())
}

fn effort_name(effort: Effort) -> &'static str {
    tr(effort.label())
}

fn staffing_text(decision: Decision, staffing: &Staffing) -> String {
    let model = staffing
        .model
        .clone()
        .unwrap_or_else(|| tr("default model").to_owned());
    let effort = staffing
        .effort
        .clone()
        .unwrap_or_else(|| tr("provider default").to_owned());
    let values = [
        ("rated", effort_name(decision.assessed).to_owned()),
        ("account", staffing.account.clone()),
        ("model", model),
        ("effort", effort),
    ];
    if staffing.team {
        trf(
            "Rated {rated}. {account} · {model} ({effort} effort) is planning a team of up to 3 agents, each in its own worktree.",
            &values,
        )
    } else {
        trf(
            "Rated {rated}. One developer on {account} · {model} ({effort} effort) is working in the project.",
            &values,
        )
    }
}

fn tone_color(tone: Tone) -> egui::Color32 {
    match tone {
        Tone::User => colors().text,
        Tone::Info => colors().accent,
        Tone::Success => colors().success,
        Tone::Warning => colors().warning,
        Tone::Failure => colors().error,
    }
}

fn task_brief(status: &TaskStatus) -> String {
    match status {
        TaskStatus::Pending => tr("Waiting for its turn").to_owned(),
        TaskStatus::Running => tr("Working").to_owned(),
        TaskStatus::AwaitingApproval => tr("Waiting for your confirmation").to_owned(),
        TaskStatus::Retrying { reason, .. } | TaskStatus::Failed { reason } => {
            luma::brief(reason, 90)
        }
        TaskStatus::Succeeded => tr("Done").to_owned(),
        TaskStatus::Cancelled => tr("Cancelled").to_owned(),
        TaskStatus::Skipped { .. } => tr("Skipped").to_owned(),
    }
}

/// A request only the user can answer, with the exact action shown.
fn confirmation(ui: &mut Ui, text: &str) -> Option<bool> {
    let mut answer = None;
    Frame::new()
        .fill(colors().warning.gamma_multiply(0.08))
        .corner_radius(8)
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(
                egui::Label::new(
                    RichText::new(crate::agent_gate::redact(text))
                        .monospace()
                        .size(SMALL),
                )
                .wrap(),
            );
            ui.horizontal(|ui| {
                if ui.add(primary("Allow once")).clicked() {
                    answer = Some(true);
                }
                if ui.button(tr("Deny")).clicked() {
                    answer = Some(false);
                }
            });
        });
    ui.add_space(4.0);
    answer
}

fn log_line(ui: &mut Ui, line: &Line) {
    let fill = if line.tone == Tone::User {
        colors().inset
    } else {
        egui::Color32::TRANSPARENT
    };
    Frame::new()
        .fill(fill)
        .corner_radius(8)
        .inner_margin(Margin::symmetric(8, 5))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                let (dot, _) = ui.allocate_exact_size(egui::vec2(10.0, 16.0), egui::Sense::hover());
                if line.tone != Tone::User {
                    ui.painter()
                        .circle_filled(dot.center(), 3.0, tone_color(line.tone));
                }
                ui.add(
                    egui::Label::new(RichText::new(&line.text).size(BODY).color(colors().text))
                        .wrap(),
                );
            });
            if let Some(detail) = line.detail.as_deref().filter(|d| !d.trim().is_empty()) {
                super::agent_hub::failure_text(ui, detail);
            }
        });
}

fn section_label(ui: &mut Ui, text: &str, color: egui::Color32) {
    ui.label(RichText::new(tr(text)).size(10.0).color(color));
}

impl Editor {
    /// Takes an order for Luma: rates it, and either launches it or asks
    /// before going above the user's effort ceiling.
    pub(super) fn luma_order(&mut self, order: &str) {
        let order = order.trim();
        if order.is_empty() {
            return;
        }
        let ceiling = self.preferences.luma_effort;
        let decision = luma::decide(order, ceiling);
        let chat = &mut self.hub.agents.luma;
        chat.say(Tone::User, order);
        if decision.capped() {
            chat.say(
                Tone::Warning,
                trf(
                    "This looks like {assessed} effort, above your {ceiling} ceiling. Raise it for this order, or keep {ceiling}?",
                    &[
                        ("assessed", effort_name(decision.assessed).to_owned()),
                        ("ceiling", effort_name(ceiling).to_owned()),
                    ],
                ),
            );
            chat.held = Some(Held {
                order: order.to_owned(),
                decision,
            });
            return;
        }
        self.luma_launch(order, decision);
    }

    fn luma_launch(&mut self, order: &str, decision: Decision) {
        let Some(root) = self.root.clone() else {
            self.hub
                .agents
                .luma
                .say(Tone::Warning, tr("Open a project folder first."));
            return;
        };
        let state = &mut self.hub.agents;
        let Some(runtime) = state.runtime.as_mut() else {
            let error = state.open_error_text().unwrap_or_default();
            state.luma.say_with(
                Tone::Failure,
                tr("The agent runtime is unavailable."),
                Some(error),
            );
            return;
        };
        match runtime.luma_dispatch(order, &root, decision) {
            Ok(staffing) => {
                runtime.notice = None;
                let text = staffing_text(decision, &staffing);
                state.luma.say(Tone::Info, text);
            }
            Err(error) => state.luma.say_with(
                Tone::Failure,
                tr("Luma could not start this order."),
                Some(error),
            ),
        }
    }

    /// Lets Luma report what changed; called after every runtime tick.
    pub(super) fn luma_watch(&mut self) {
        let state = &mut self.hub.agents;
        if let Some(runtime) = state.runtime.as_ref() {
            state.luma.watch(runtime);
        }
    }

    /// Luma's chat: order box, effort ceiling, team status and log.
    pub(super) fn luma_panel(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        self.ensure_runtime(&ctx);
        self.luma_effort_row(ui);
        ui.add_space(6.0);
        self.luma_input(ui);
        ui.add_space(8.0);
        self.luma_held(ui);
        self.luma_confirmations(ui);
        self.luma_team(ui);
        self.luma_log(ui);
    }

    fn luma_effort_row(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(muted("Max effort"));
            for effort in Effort::ALL {
                let selected = self.preferences.luma_effort == effort;
                let response = ui
                    .selectable_label(selected, effort_name(effort))
                    .on_hover_text(tr(
                        "Luma never uses more effort than this. Medium suits most work.",
                    ));
                if response.hovered() {
                    pointer_cursor(ui.ctx());
                }
                if response.clicked() {
                    self.preferences.luma_effort = effort;
                }
            }
        });
    }

    fn luma_input(&mut self, ui: &mut Ui) {
        let chat = &mut self.hub.agents.luma;
        let mut send = None;
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 72.0).max(80.0);
            let response = ui.add(
                TextEdit::singleline(&mut chat.draft)
                    .hint_text(tr("Tell Luma what to build or fix"))
                    .desired_width(width),
            );
            if chat.focus {
                response.request_focus();
                chat.focus = false;
            }
            let entered = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let ready = !chat.draft.trim().is_empty();
            let button = ui.add_enabled(ready, primary("Send"));
            if button.hovered() {
                pointer_cursor(ui.ctx());
            }
            if ready && (entered || button.clicked()) {
                send = Some(std::mem::take(&mut chat.draft));
            }
        });
        if let Some(order) = send {
            self.luma_order(&order);
        }
    }

    fn luma_held(&mut self, ui: &mut Ui) {
        let Some(held) = &self.hub.agents.luma.held else {
            return;
        };
        let raise = trf(
            "Use {effort}",
            &[("effort", effort_name(held.decision.assessed).to_owned())],
        );
        let keep = trf(
            "Keep {effort}",
            &[("effort", effort_name(held.decision.effort).to_owned())],
        );
        let mut choice = None;
        ui.horizontal(|ui| {
            if ui.add(primary(&raise)).clicked() {
                choice = Some(Some(true));
            }
            if ui.button(keep).clicked() {
                choice = Some(Some(false));
            }
            if ui.button(tr("Cancel")).clicked() {
                choice = Some(None);
            }
        });
        ui.add_space(6.0);
        let Some(choice) = choice else { return };
        let Some(held) = self.hub.agents.luma.held.take() else {
            return;
        };
        let Some(raise) = choice else {
            self.hub.agents.luma.say(Tone::Info, tr("Order cancelled."));
            return;
        };
        let mut decision = held.decision;
        if raise {
            decision.effort = decision.assessed;
        }
        self.luma_launch(&held.order, decision);
    }

    /// Everything waiting on the user, from chats and from the team run.
    fn luma_confirmations(&mut self, ui: &mut Ui) {
        let state = &mut self.hub.agents;
        let Some(runtime) = state.runtime.as_mut() else {
            return;
        };
        let chats = runtime.approvals.clone();
        let team: Vec<(String, String)> = runtime.plan.as_ref().map_or_else(Vec::new, |run| {
            run.engine
                .pending_approvals()
                .into_iter()
                .map(|(request, task)| {
                    let action = crate::agent_gate::describe(&request.action);
                    (request.ticket.clone(), format!("{task}: {action}"))
                })
                .collect()
        });
        if chats.is_empty() && team.is_empty() {
            return;
        }
        section_label(ui, "NEEDS YOUR CONFIRMATION", colors().warning);
        for request in &chats {
            let text = crate::agent_gate::describe(&request.action);
            if let Some(allow) = confirmation(ui, &text) {
                runtime.answer(request, allow, false);
            }
        }
        for (ticket, text) in &team {
            let Some(allow) = confirmation(ui, text) else {
                continue;
            };
            let resolution = if allow {
                Resolution::AllowOnce
            } else {
                Resolution::Deny
            };
            let result = runtime
                .plan
                .as_mut()
                .map(|run| run.engine.resolve(ticket, resolution));
            if let Some(Err(error)) = result {
                state.luma.say_with(
                    Tone::Failure,
                    tr("The answer could not be recorded."),
                    Some(error.to_string()),
                );
            }
        }
        ui.add_space(6.0);
    }

    /// One row per active agent: who it is, its state, what it last did.
    fn luma_team(&mut self, ui: &mut Ui) {
        let Some(runtime) = self.hub.agents.runtime.as_ref() else {
            return;
        };
        let mut rows: Vec<(String, egui::Color32, String)> = Vec::new();
        if let Some(run) = &runtime.plan
            && !run.engine.plan().state.is_settled()
        {
            for task in run.engine.plan().tasks() {
                rows.push((
                    task.spec.title.clone(),
                    super::agent_hub::task_color(&task.status),
                    task_brief(&task.status),
                ));
            }
        }
        for session in runtime.sessions.iter().filter(|s| s.running()) {
            let doing = last_log(session, &["agent", "tool", "started"])
                .map_or_else(|| tr("Starting…").to_owned(), |t| luma::brief(&t, 90));
            rows.push((session.meta.title.clone(), colors().accent, doing));
        }
        if rows.is_empty() {
            return;
        }
        section_label(ui, "TEAM", colors().muted);
        for (name, color, doing) in rows {
            ui.horizontal(|ui| {
                let (dot, _) = ui.allocate_exact_size(egui::vec2(10.0, 16.0), egui::Sense::hover());
                ui.painter().circle_filled(dot.center(), 3.0, color);
                ui.add(egui::Label::new(RichText::new(name).strong()).truncate());
            });
            ui.add(egui::Label::new(muted(doing)).wrap());
        }
        ui.add_space(8.0);
    }

    fn luma_log(&mut self, ui: &mut Ui) {
        let lines = &self.hub.agents.luma.lines;
        if lines.is_empty() {
            ui.add(
                egui::Label::new(muted(
                    "Give Luma an order. It rates the work, picks the model and effort within your ceiling, launches the agents and reports back here.",
                ))
                .wrap(),
            );
            return;
        }
        let start = lines.len().saturating_sub(12);
        for line in &lines[start..] {
            log_line(ui, line);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_keep_the_full_cli_output() {
        let mut chat = LumaChat::default();
        let output = "error: unknown option '--bogus'\nUsage: claude [options]";
        chat.report_settled(&PlanState::Failed {
            reason: output.into(),
        });
        let line = chat.lines.last().expect("a line");
        assert_eq!(line.tone, Tone::Failure);
        assert_eq!(line.detail.as_deref(), Some(output));
    }

    #[test]
    fn history_is_bounded() {
        let mut chat = LumaChat::default();
        for i in 0..(HISTORY + 5) {
            chat.say(Tone::Info, i.to_string());
        }
        assert_eq!(chat.lines.len(), HISTORY);
        assert_eq!(chat.lines[0].text, "5");
    }
}

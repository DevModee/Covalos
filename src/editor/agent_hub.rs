//! Agent Hub V4/V5 screens: accounts, live sessions with approvals and shared
//! context, and the autonomous engine. Drawn with the existing COVALOS
//! components (section, row, primary, checkbox, Frame surfaces).
use std::{path::PathBuf, sync::Arc};

use covalos_agents::{
    Account, AccountId, ConnectionMethod, ProviderKind,
    context::EntryKind,
    permission::{Preset, Resolution},
};
use covalos_engine::{TaskStatus, budget::Budget, role::RoleCatalog};
use eframe::egui::{self, Align, Frame, Layout, Margin, RichText, ScrollArea, TextEdit, Ui};

use super::{Editor, theme::*};
use crate::agents_runtime::{NewSession, PlanMode, Runtime, Status};

pub(super) struct State {
    pub(super) runtime: Option<Runtime>,
    open_error: Option<String>,
    provider: usize,
    method: usize,
    account_id: String,
    account_label: String,
    account_model: String,
    secret: String,
    secret_for: Option<AccountId>,
    status: Vec<(AccountId, String)>,
    new_title: String,
    new_account: Option<AccountId>,
    new_model: String,
    new_worktree: bool,
    new_preset: usize,
    new_safe: bool,
    new_share: bool,
    pub(super) selected: Option<usize>,
    note: String,
    goal: String,
    single_role: Option<String>,
    plan_worktrees: bool,
    budget_minutes: u32,
    budget_tokens: u32,
    max_parallel: usize,
}

impl Default for State {
    fn default() -> Self {
        Self {
            runtime: None,
            open_error: None,
            provider: 0,
            method: 0,
            account_id: String::new(),
            account_label: String::new(),
            account_model: String::new(),
            secret: String::new(),
            secret_for: None,
            status: Vec::new(),
            new_title: String::new(),
            new_account: None,
            new_model: String::new(),
            new_worktree: false,
            new_preset: 1,
            new_safe: true,
            new_share: true,
            selected: None,
            note: String::new(),
            goal: String::new(),
            single_role: Some("reviewer".into()),
            plan_worktrees: true,
            budget_minutes: 20,
            budget_tokens: 0,
            max_parallel: 2,
        }
    }
}

const PROVIDERS: [(ProviderKind, &str); 3] = [
    (ProviderKind::ClaudeCode, "Claude Code"),
    (ProviderKind::Codex, "OpenAI Codex"),
    (ProviderKind::Gemini, "Gemini CLI"),
];
const METHODS: [(ConnectionMethod, &str, &str); 3] = [
    (
        ConnectionMethod::SystemCli,
        "Existing CLI sign-in",
        "Uses the CLI exactly as you configured it.",
    ),
    (
        ConnectionMethod::OfficialLogin,
        "Isolated official sign-in",
        "Own config folder; sign in with the provider's login. Allows several accounts.",
    ),
    (
        ConnectionMethod::ApiKey,
        "API key",
        "Stored in Windows Credential Manager, passed only through the provider's variable.",
    ),
];
const PRESETS: [(Preset, &str); 3] = [
    (Preset::ReadOnly, "Read only"),
    (Preset::Supervised, "Supervised"),
    (Preset::Worktree, "Edit inside workdir"),
];

// ---------------------------------------------------------------- reading style
//
// Agent Hub reads like a document: solid panels over a dimmed field, three
// text tones (primary, secondary, metadata) and a 14.5/21 reading size.

const READ: f32 = 14.5;
const READ_LINE: f32 = 21.0;
const META: f32 = 12.5;
const META_LINE: f32 = 18.0;
const LABEL: f32 = 10.5;
const TITLE: f32 = 19.0;
const GUTTER: f32 = 28.0;

mod ink {
    use super::super::{motion::blend, theme::colors};
    use eframe::egui::Color32;

    /// Headlines and agent output: almost white.
    pub(super) fn primary() -> Color32 {
        blend(colors().text, Color32::WHITE, 0.45)
    }
    /// Supporting copy: clearly visible.
    pub(super) fn secondary() -> Color32 {
        blend(colors().secondary, colors().text, 0.45)
    }
    /// Metadata: quiet but readable.
    pub(super) fn meta() -> Color32 {
        blend(colors().muted, colors().secondary, 0.55)
    }
    /// Solid panel fill, a step above the dimmed background.
    pub(super) fn panel() -> Color32 {
        blend(colors().inset, colors().surface, 0.65)
    }
    pub(super) fn raised() -> Color32 {
        blend(colors().surface, colors().elevated, 0.7)
    }
}

fn card(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    Frame::new()
        .fill(ink::panel())
        .stroke(egui::Stroke::new(
            1.0_f32,
            super::motion::blend(colors().border, colors().control_border, 0.35),
        ))
        .inner_margin(Margin::same(18))
        .corner_radius(12)
        .shadow(egui::epaint::Shadow {
            offset: [0, 6],
            blur: 18,
            spread: 0,
            color: egui::Color32::from_black_alpha(70),
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 8.0;
            add(ui);
        });
    ui.add_space(14.0);
}

/// Metadata text: quiet but legible.
fn small(text: impl Into<String>) -> RichText {
    RichText::new(text.into())
        .size(META)
        .line_height(Some(META_LINE))
        .color(ink::meta())
}

/// Reading text for content people actually read.
fn read(text: impl Into<String>) -> RichText {
    RichText::new(text.into())
        .size(READ)
        .line_height(Some(READ_LINE))
        .color(ink::primary())
}

/// Supporting sentence under a heading.
fn support(text: impl Into<String>) -> RichText {
    RichText::new(text.into())
        .size(BODY + 0.5)
        .line_height(Some(19.0))
        .color(ink::secondary())
}

/// Uppercase section label.
fn label(text: &str) -> RichText {
    RichText::new(tr(text).to_uppercase())
        .size(LABEL)
        .strong()
        .extra_letter_spacing(0.9)
        .color(ink::meta())
}

fn heading(text: impl Into<String>) -> RichText {
    RichText::new(text.into())
        .size(TITLE)
        .strong()
        .color(ink::primary())
}

fn status_color(status: Status) -> egui::Color32 {
    match status {
        Status::Running => colors().accent,
        Status::Completed => colors().success,
        Status::Failed => colors().error,
        Status::Interrupted | Status::Cancelled => colors().warning,
        Status::Idle => ink::meta(),
    }
}

/// Small rounded tag with a tinted fill.
fn badge(ui: &mut Ui, text: &str, color: egui::Color32) {
    Frame::new()
        .fill(color.gamma_multiply(0.14))
        .stroke(egui::Stroke::new(1.0_f32, color.gamma_multiply(0.38)))
        .corner_radius(5)
        .inner_margin(Margin::symmetric(7, 2))
        .show(ui, |ui| {
            ui.label(
                RichText::new(text)
                    .size(LABEL)
                    .strong()
                    .extra_letter_spacing(0.6)
                    .color(color),
            );
        });
}

/// Neutral metadata chip.
fn chip(ui: &mut Ui, text: impl Into<String>) {
    Frame::new()
        .fill(ink::raised())
        .corner_radius(6)
        .inner_margin(Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.label(
                RichText::new(text.into())
                    .size(META)
                    .color(ink::secondary()),
            );
        });
}

fn event_style(kind: &str) -> (&'static str, egui::Color32) {
    let c = colors();
    match kind {
        "created" => ("CREATED", ink::secondary()),
        "started" => ("STARTED", c.accent),
        "you" => ("YOU", c.accent),
        "agent" => ("AGENT", ink::primary()),
        "tool" => ("TOOL", super::motion::blend(c.accent, c.success, 0.55)),
        "running" => ("RUNNING", c.accent),
        "usage" => ("USAGE", ink::meta()),
        "cost" => ("COST", ink::meta()),
        "provider" => ("LIMITS", ink::meta()),
        "done" => ("DONE", c.success),
        "context" => ("CONTEXT", c.syntax_string),
        "approval" => ("APPROVAL", c.warning),
        "recovery" => ("RECOVERY", c.warning),
        "cancelled" => ("CANCELLED", c.warning),
        "error" => ("ERROR", c.error),
        "closed" => ("CLOSED", ink::meta()),
        _ => ("EVENT", ink::meta()),
    }
}

/// Minutes and seconds since the session's first event.
fn clock(at_ms: u64, base_ms: u64) -> String {
    let s = at_ms.saturating_sub(base_ms) / 1000;
    format!("{:02}:{:02}", s / 60, s % 60)
}

struct Entry<'a> {
    kind: &'a str,
    text: String,
    at_ms: u64,
}

/// Collapses consecutive tool events into one row ("shell ×3").
fn timeline_entries(log: &[crate::agents_runtime::LogLine]) -> Vec<Entry<'_>> {
    fn flush<'a>(out: &mut Vec<Entry<'a>>, tools: &mut Vec<&'a str>, at_ms: u64) {
        if tools.is_empty() {
            return;
        }
        let mut names: Vec<String> = Vec::new();
        let mut seen: Vec<&str> = Vec::new();
        for name in tools.iter() {
            if seen.contains(name) {
                continue;
            }
            seen.push(name);
            let count = tools.iter().filter(|t| *t == name).count();
            names.push(if count > 1 {
                format!("{name} ×{count}")
            } else {
                (*name).to_owned()
            });
        }
        out.push(Entry {
            kind: "tool",
            text: names.join(" · "),
            at_ms,
        });
        tools.clear();
    }
    let mut out: Vec<Entry<'_>> = Vec::new();
    let mut tools: Vec<&str> = Vec::new();
    let mut first_tool_at = 0;
    for line in log {
        if line.kind == "tool" {
            if tools.is_empty() {
                first_tool_at = line.at_ms;
            }
            tools.push(&line.text);
            continue;
        }
        flush(&mut out, &mut tools, first_tool_at);
        out.push(Entry {
            kind: &line.kind,
            text: line.text.clone(),
            at_ms: line.at_ms,
        });
    }
    flush(&mut out, &mut tools, first_tool_at);
    out
}

/// One timeline row: time, a rail with a dot, a badge and the content.
fn timeline_row(ui: &mut Ui, time: &str, kind: &str, add: impl FnOnce(&mut Ui)) {
    let (badge_text, color) = event_style(kind);
    let response = ui.horizontal_top(|ui| {
        ui.add_sized(
            [40.0, 20.0],
            egui::Label::new(
                RichText::new(time)
                    .monospace()
                    .size(11.0)
                    .color(ink::meta()),
            ),
        );
        ui.add_space(22.0);
        ui.vertical(|ui| {
            ui.set_width(ui.available_width());
            badge(ui, badge_text, color);
            ui.add_space(2.0);
            add(ui);
        });
    });
    let rect = response.response.rect;
    let x = rect.left() + 52.0;
    let painter = ui.painter();
    painter.line_segment(
        [
            egui::pos2(x, rect.top() + 16.0),
            egui::pos2(x, rect.bottom() + 12.0),
        ],
        egui::Stroke::new(1.0_f32, colors().border),
    );
    painter.circle_filled(egui::pos2(x, rect.top() + 10.0), 3.5, color);
    painter.circle_stroke(
        egui::pos2(x, rect.top() + 10.0),
        6.0,
        egui::Stroke::new(1.0_f32, color.gamma_multiply(0.35)),
    );
    ui.add_space(12.0);
}

fn bubble(ui: &mut Ui, fill: egui::Color32, text: RichText) {
    Frame::new()
        .fill(fill)
        .corner_radius(8)
        .inner_margin(Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(egui::Label::new(text).wrap());
        });
}

fn provider_label(kind: &ProviderKind) -> &'static str {
    PROVIDERS
        .iter()
        .find(|(k, _)| k == kind)
        .map_or("Custom", |(_, l)| *l)
}

impl Editor {
    /// Drives the runtime every frame once the Agent Hub opened it.
    pub(super) fn poll_agent_runtime(&mut self, ctx: &egui::Context) {
        let Some(runtime) = self.hub.agents.runtime.as_mut() else {
            return;
        };
        if runtime.tick() {
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        } else if self.hub_open {
            ctx.request_repaint_after(std::time::Duration::from_millis(1000));
        }
    }

    /// Evidence harness: refresh every account's sign-in status line.
    #[cfg(test)]
    pub(super) fn evidence_account_status(&mut self) {
        let state = &mut self.hub.agents;
        let Some(runtime) = state.runtime.as_ref() else {
            return;
        };
        state.status = runtime
            .profiles
            .accounts()
            .iter()
            .filter(|a| a.method != ConnectionMethod::ApiKey && a.provider != ProviderKind::Gemini)
            .map(|a| {
                let text = runtime
                    .account_status(&a.id)
                    .unwrap_or_else(|e| format!("Status unavailable: {e}"));
                (a.id.clone(), text)
            })
            .collect();
    }

    pub(super) fn ensure_runtime(&mut self, ctx: &egui::Context) -> bool {
        let state = &mut self.hub.agents;
        if state.runtime.is_some() {
            return true;
        }
        if state.open_error.is_some() {
            return false;
        }
        let Some(dir) = crate::paths::config_dir().map(|d| d.join("agents")) else {
            state.open_error = Some("No user configuration directory.".into());
            return false;
        };
        let wake_ctx = ctx.clone();
        match Runtime::open(dir, Arc::new(move || wake_ctx.request_repaint())) {
            Ok(runtime) => {
                state.runtime = Some(runtime);
                true
            }
            Err(error) => {
                state.open_error = Some(error);
                false
            }
        }
    }

    fn runtime_unavailable(&mut self, ui: &mut Ui) -> bool {
        let ctx = ui.ctx().clone();
        if self.ensure_runtime(&ctx) {
            return false;
        }
        ui.colored_label(
            colors().error,
            self.hub.agents.open_error.clone().unwrap_or_default(),
        );
        true
    }

    fn agent_notice(&mut self, ui: &mut Ui) {
        let Some(runtime) = self.hub.agents.runtime.as_mut() else {
            return;
        };
        if let Some((ok, text)) = runtime.notice.clone() {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(
                    if ok {
                        colors().success
                    } else {
                        colors().warning
                    },
                    text,
                );
                if ui.small_button("✕").clicked() {
                    runtime.notice = None;
                }
            });
            ui.add_space(6.0);
        }
    }

    // ---------------------------------------------------------------- accounts

    pub(super) fn accounts_panel(&mut self, ui: &mut Ui) {
        if self.runtime_unavailable(ui) {
            return;
        }
        self.agent_notice(ui);
        ScrollArea::vertical()
            .id_salt("agent-accounts")
            .show(ui, |ui| {
                self.providers_detected(ui);
                self.account_list(ui);
                self.account_form(ui);
            });
    }

    fn providers_detected(&mut self, ui: &mut Ui) {
        let runtime = self.hub.agents.runtime.as_mut().expect("runtime");
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(label("Providers on this PC"));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button(tr("Detect again")).clicked() {
                        runtime.redetect();
                    }
                });
            });
            ui.add_space(4.0);
            for (kind, label) in &PROVIDERS {
                let found = runtime.detections.get(kind).cloned().unwrap_or_default();
                ui.horizontal_wrapped(|ui| {
                    let (dot, state) = if !found.done {
                        (colors().muted, "Detecting…".to_owned())
                    } else if let Some(version) = &found.version {
                        (colors().success, version.clone())
                    } else if found.program.is_some() {
                        (
                            colors().warning,
                            found.error.clone().unwrap_or_else(|| "Found".into()),
                        )
                    } else {
                        (colors().muted, "Not installed".into())
                    };
                    ui.colored_label(dot, "●");
                    ui.label(RichText::new(*label).strong());
                    ui.label(small(state));
                    if let Some(login) = &found.login {
                        ui.label(small(format!("· default sign-in: {login}")));
                    }
                });
                if let Some(path) = &found.program {
                    ui.label(small(path.display().to_string()));
                } else if found.done && *kind == ProviderKind::Gemini {
                    ui.label(small(
                        "Install the official Gemini CLI to detect it. COVALOS has no verified \
                         headless adapter for Gemini yet.",
                    ));
                }
                ui.add_space(4.0);
            }
            for tool in &runtime.external {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(
                        if tool.path.is_some() {
                            colors().secondary
                        } else {
                            colors().muted
                        },
                        "●",
                    );
                    ui.label(RichText::new(tool.name).strong());
                    ui.label(small(if tool.path.is_some() {
                        "Detected"
                    } else {
                        "Not found"
                    }));
                });
                ui.label(small(tool.note));
            }
            ui.add_space(4.0);
            ui.label(small(format!(
                "API keys are kept in: {}. Sign-in tokens stay in each CLI's own folder; \
                 COVALOS never reads or copies them.",
                runtime.credential_backend
            )));
        });
    }

    fn account_list(&mut self, ui: &mut Ui) {
        let state = &mut self.hub.agents;
        let runtime = state.runtime.as_mut().expect("runtime");
        let accounts: Vec<Account> = runtime.profiles.accounts().to_vec();
        section(
            ui,
            "Accounts",
            "Several accounts per provider are supported. Isolated sign-ins keep separate \
             config folders (CLAUDE_CONFIG_DIR / CODEX_HOME).",
        );
        if accounts.is_empty() {
            ui.label(secondary("No accounts yet. Add one below."));
            ui.add_space(8.0);
        }
        let mut remove = None;
        for account in &accounts {
            let key_state = runtime.has_api_key(account);
            card(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(&account.label).strong());
                    ui.label(small(format!(
                        "{} · {} · id {}",
                        provider_label(&account.provider),
                        METHODS
                            .iter()
                            .find(|(m, _, _)| *m == account.method)
                            .map_or("", |(_, l, _)| *l),
                        account.id
                    )));
                    if let Some(model) = &account.default_model {
                        ui.label(small(format!("· model {model}")));
                    }
                });
                if let Some((_, text)) = state.status.iter().find(|(id, _)| id == &account.id) {
                    ui.label(small(text.clone()));
                }
                if let Some(has) = key_state {
                    ui.label(small(if has {
                        "API key stored in the credential store"
                    } else {
                        "No API key stored yet"
                    }));
                }
                ui.horizontal_wrapped(|ui| {
                    if account.method != ConnectionMethod::ApiKey
                        && account.provider != ProviderKind::Gemini
                        && ui.button(tr("Check sign-in")).clicked()
                    {
                        let text = runtime
                            .account_status(&account.id)
                            .unwrap_or_else(|e| format!("Status unavailable: {e}"));
                        state.status.retain(|(id, _)| id != &account.id);
                        state.status.push((account.id.clone(), text));
                    }
                    if account.method == ConnectionMethod::OfficialLogin
                        && ui
                            .button(tr("Sign in (official)"))
                            .on_hover_text(tr(
                                "Opens the provider's own login in a new console window.",
                            ))
                            .clicked()
                    {
                        runtime.notice = Some(match runtime.sign_in(&account.id) {
                            Ok(()) => (
                                true,
                                "Finish signing in in the console window, then press Check \
                                 sign-in."
                                    .into(),
                            ),
                            Err(e) => (false, e),
                        });
                    }
                    let wants_key = account.method == ConnectionMethod::ApiKey
                        || (account.provider == ProviderKind::Codex
                            && account.method == ConnectionMethod::OfficialLogin);
                    if wants_key && ui.button(tr("Set API key…")).clicked() {
                        state.secret.clear();
                        state.secret_for = Some(account.id.clone());
                    }
                    if ui.button(tr("Remove")).clicked() {
                        remove = Some(account.id.clone());
                    }
                });
                if state.secret_for.as_ref() == Some(&account.id) {
                    ui.horizontal(|ui| {
                        ui.add(
                            TextEdit::singleline(&mut state.secret)
                                .password(true)
                                .hint_text(tr("Paste the API key"))
                                .desired_width(320.0),
                        );
                        if ui.add(primary("Save")).clicked() {
                            let key = std::mem::take(&mut state.secret);
                            let result = if account.method == ConnectionMethod::ApiKey {
                                runtime.save_api_key(&account.id, &key)
                            } else {
                                runtime.codex_api_login(&account.id, &key)
                            };
                            runtime.notice = Some(match result {
                                Ok(()) => (true, "API key saved.".into()),
                                Err(e) => (false, e),
                            });
                            state.secret_for = None;
                        }
                        if ui.button(tr("Cancel")).clicked() {
                            state.secret.clear();
                            state.secret_for = None;
                        }
                    });
                }
            });
        }
        if let Some(id) = remove {
            runtime.notice = Some(match runtime.remove_account(&id) {
                Ok(()) => (true, format!("Removed account {id}.")),
                Err(e) => (false, e),
            });
        }
    }

    fn account_form(&mut self, ui: &mut Ui) {
        let state = &mut self.hub.agents;
        let runtime = state.runtime.as_mut().expect("runtime");
        card(ui, |ui| {
            ui.label(label("Add account"));
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                for (i, (_, label)) in PROVIDERS.iter().enumerate() {
                    ui.selectable_value(&mut state.provider, i, *label);
                }
            });
            ui.horizontal_wrapped(|ui| {
                for (i, (_, label, _)) in METHODS.iter().enumerate() {
                    ui.selectable_value(&mut state.method, i, *label);
                }
            });
            ui.label(small(METHODS[state.method].2));
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.add(
                    TextEdit::singleline(&mut state.account_id)
                        .hint_text("id (e.g. claude-work)")
                        .desired_width(170.0),
                );
                ui.add(
                    TextEdit::singleline(&mut state.account_label)
                        .hint_text(tr("Label"))
                        .desired_width(170.0),
                );
                ui.add(
                    TextEdit::singleline(&mut state.account_model)
                        .hint_text(tr("Default model (optional)"))
                        .desired_width(170.0),
                );
            });
            if ui.add(primary("Add account")).clicked() {
                let id = state.account_id.trim().to_ascii_lowercase();
                let result = AccountId::new(id)
                    .map_err(|e| e.to_string())
                    .and_then(|id| {
                        let (kind, default_label) = &PROVIDERS[state.provider];
                        let label = state.account_label.trim();
                        let mut account = Account::new(
                            id,
                            kind.clone(),
                            if label.is_empty() {
                                *default_label
                            } else {
                                label
                            },
                            METHODS[state.method].0,
                        );
                        let model = state.account_model.trim();
                        if !model.is_empty() {
                            account.default_model = Some(model.to_owned());
                            account.declared_models = vec![model.to_owned()];
                        }
                        runtime.add_account(account)
                    });
                runtime.notice = Some(match result {
                    Ok(()) => {
                        state.account_id.clear();
                        state.account_label.clear();
                        state.account_model.clear();
                        (true, "Account added.".into())
                    }
                    Err(e) => (false, e),
                });
            }
        });
    }

    // ---------------------------------------------------------------- sessions

    pub(super) fn sessions_panel(&mut self, ui: &mut Ui) {
        if self.runtime_unavailable(ui) {
            return;
        }
        self.agent_notice(ui);
        self.approvals_strip(ui);
        let rect = ui.available_rect_before_wrap();
        let list_width = (rect.width() * 0.32).clamp(270.0, 340.0);
        let list = egui::Rect::from_min_size(rect.min, egui::vec2(list_width, rect.height()));
        let detail =
            egui::Rect::from_min_max(egui::pos2(list.right() + GUTTER, rect.top()), rect.max);
        ui.scope_builder(egui::UiBuilder::new().max_rect(list), |ui| {
            ui.set_max_width(list.width());
            ScrollArea::vertical()
                .id_salt("agent-session-list")
                .max_width(list.width())
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_max_width(list.width() - 14.0);
                    self.session_list(ui);
                    self.session_form(ui);
                    self.context_panel(ui);
                });
        });
        ui.scope_builder(egui::UiBuilder::new().max_rect(detail), |ui| {
            self.session_detail(ui);
        });
    }

    fn approvals_strip(&mut self, ui: &mut Ui) {
        let runtime = self.hub.agents.runtime.as_mut().expect("runtime");
        let pending = runtime.approvals.clone();
        if pending.is_empty() {
            return;
        }
        card(ui, |ui| {
            ui.horizontal(|ui| {
                badge(ui, "APPROVALS WAITING", colors().warning);
                ui.label(small(format!("{} pending", pending.len())));
            });
            for request in &pending {
                let title = runtime
                    .sessions
                    .iter()
                    .find(|s| s.meta.id.as_str() == request.session)
                    .map_or_else(|| request.session.clone(), |s| s.meta.title.clone());
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(&title).strong());
                    ui.label(small(format!("· {} · {}", request.tool, request.reason)));
                    let left = request.expires_ms.saturating_sub(covalos_agents::now_ms()) / 1000;
                    ui.label(small(format!("· {left}s")));
                });
                ui.label(
                    RichText::new(crate::agent_gate::redact(&crate::agent_gate::describe(
                        &request.action,
                    )))
                    .monospace()
                    .size(SMALL),
                );
                ui.horizontal(|ui| {
                    if ui.add(primary("Allow once")).clicked() {
                        runtime.answer(request, true, false);
                    }
                    if !request.once_only && ui.button(tr("Allow for session")).clicked() {
                        runtime.answer(request, true, true);
                    }
                    if ui.button(tr("Deny")).clicked() {
                        runtime.answer(request, false, false);
                    }
                });
                ui.add_space(4.0);
            }
        });
    }

    fn session_list(&mut self, ui: &mut Ui) {
        let state = &mut self.hub.agents;
        let runtime = state.runtime.as_ref().expect("runtime");
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(label("Sessions"));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let live = runtime.sessions.iter().filter(|s| s.running()).count();
                    if live > 0 {
                        badge(ui, &format!("{live} RUNNING"), colors().accent);
                    }
                });
            });
            if runtime.sessions.is_empty() {
                ui.label(support("No sessions yet. Create one below."));
            }
            for (index, session) in runtime.sessions.iter().enumerate() {
                let meta = &session.meta;
                let selected = state.selected == Some(index);
                let detail = format!(
                    "{} · {}{}",
                    provider_label(&meta.provider),
                    tr(meta.status.label()),
                    if meta.worktree_branch.is_some() {
                        " · worktree"
                    } else {
                        ""
                    }
                );
                let item = Frame::new()
                    .fill(if selected {
                        colors().accent_bg
                    } else {
                        egui::Color32::TRANSPARENT
                    })
                    .corner_radius(8)
                    .inner_margin(Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal_top(|ui| {
                            let (dot, _) = ui
                                .allocate_exact_size(egui::vec2(10.0, 18.0), egui::Sense::hover());
                            ui.painter().circle_filled(
                                dot.center(),
                                4.0,
                                status_color(meta.status),
                            );
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 2.0;
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&meta.title).size(BODY + 1.0).strong().color(
                                            if selected {
                                                ink::primary()
                                            } else {
                                                ink::secondary()
                                            },
                                        ),
                                    )
                                    .truncate(),
                                );
                                ui.label(small(detail));
                            });
                        });
                    })
                    .response
                    .interact(egui::Sense::click());
                if item.hovered() && !selected {
                    ui.painter().rect_stroke(
                        item.rect,
                        8,
                        egui::Stroke::new(1.0_f32, colors().border),
                        egui::StrokeKind::Inside,
                    );
                }
                if item.clicked() {
                    state.selected = Some(index);
                }
            }
        });
    }

    fn session_form(&mut self, ui: &mut Ui) {
        let root = self.root.clone();
        let state = &mut self.hub.agents;
        let runtime = state.runtime.as_mut().expect("runtime");
        let accounts: Vec<Account> = runtime
            .profiles
            .accounts()
            .iter()
            .filter(|a| a.provider != ProviderKind::Gemini && a.enabled)
            .cloned()
            .collect();
        card(ui, |ui| {
            ui.label(label("New session"));
            let Some(root) = root.clone() else {
                ui.label(secondary("Open a project folder first."));
                return;
            };
            if accounts.is_empty() {
                ui.label(secondary("Add a Claude Code or Codex account in Accounts."));
                return;
            }
            if state
                .new_account
                .as_ref()
                .is_none_or(|id| !accounts.iter().any(|a| &a.id == id))
            {
                state.new_account = Some(accounts[0].id.clone());
            }
            ui.add(
                TextEdit::singleline(&mut state.new_title)
                    .hint_text(tr("Title (optional)"))
                    .desired_width(f32::INFINITY),
            );
            egui::ComboBox::from_id_salt("agent-new-account")
                .width(ui.available_width())
                .truncate()
                .selected_text(
                    accounts
                        .iter()
                        .find(|a| Some(&a.id) == state.new_account.as_ref())
                        .map(|a| format!("{} · {}", a.label, provider_label(&a.provider)))
                        .unwrap_or_default(),
                )
                .show_ui(ui, |ui| {
                    for account in &accounts {
                        ui.selectable_value(
                            &mut state.new_account,
                            Some(account.id.clone()),
                            format!("{} · {}", account.label, provider_label(&account.provider)),
                        );
                    }
                });
            ui.add(
                TextEdit::singleline(&mut state.new_model)
                    .hint_text(tr("Model (empty = account default)"))
                    .desired_width(f32::INFINITY),
            );
            ui.horizontal_wrapped(|ui| {
                for (i, (_, label)) in PRESETS.iter().enumerate() {
                    ui.selectable_value(&mut state.new_preset, i, *label);
                }
            });
            checkbox(ui, &mut state.new_worktree, tr("Own git worktree"));
            checkbox(
                ui,
                &mut state.new_safe,
                tr("Auto-approve commands classified safe"),
            );
            checkbox(
                ui,
                &mut state.new_share,
                tr("Share context with other agents"),
            );
            ui.label(small(format!(
                "Project: {}",
                crate::agents_runtime::plain(&root).display()
            )));
            if ui.add(primary("Create session")).clicked() {
                let account = state.new_account.clone().expect("selected");
                let model = Some(state.new_model.trim().to_owned())
                    .filter(|m| !m.is_empty())
                    .or_else(|| {
                        accounts
                            .iter()
                            .find(|a| a.id == account)
                            .and_then(|a| a.default_model.clone())
                    });
                let result = runtime.create_session(NewSession {
                    title: std::mem::take(&mut state.new_title),
                    account,
                    model,
                    root,
                    worktree: state.new_worktree,
                    preset: PRESETS[state.new_preset].0,
                    allow_safe_commands: state.new_safe,
                    share_context: state.new_share,
                });
                match result {
                    Ok(index) => state.selected = Some(index),
                    Err(e) => runtime.notice = Some((false, e)),
                }
            }
        });
    }

    fn context_panel(&mut self, ui: &mut Ui) {
        let state = &mut self.hub.agents;
        let runtime = state.runtime.as_mut().expect("runtime");
        card(ui, |ui| {
            ui.label(label("Shared context"));
            ui.label(support(
                "Results and notes every session can read. Transcripts are never shared; \
                 secret-looking text is refused.",
            ));
            let entries: Vec<_> = runtime.context.since(0).cloned().collect();
            if entries.is_empty() {
                ui.label(small("Nothing shared yet."));
            }
            for entry in entries.iter().rev().take(12) {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let kind = serde_json::to_value(entry.kind)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_uppercase))
                        .unwrap_or_default();
                    badge(ui, &kind, colors().syntax_string);
                    ui.label(small(format!(
                        "#{}{}",
                        entry.seq,
                        if entry.pinned { " · pinned" } else { "" }
                    )));
                });
                ui.add(
                    egui::Label::new(
                        RichText::new(entry.text.chars().take(260).collect::<String>())
                            .size(BODY)
                            .line_height(Some(18.0))
                            .color(ink::secondary()),
                    )
                    .wrap(),
                );
            }
            ui.add_space(4.0);
            ui.add(
                TextEdit::multiline(&mut state.note)
                    .hint_text(tr("Add a decision or contract for all agents"))
                    .desired_rows(2)
                    .desired_width(f32::INFINITY),
            );
            if ui.button(tr("Publish decision")).clicked() && !state.note.trim().is_empty() {
                let text = std::mem::take(&mut state.note);
                if let Err(e) = runtime.publish_note(EntryKind::Decision, &text, true) {
                    runtime.notice = Some((false, e));
                }
            }
        });
    }

    fn session_detail(&mut self, ui: &mut Ui) {
        let state = &mut self.hub.agents;
        let runtime = state.runtime.as_mut().expect("runtime");
        let Some(index) = state.selected.filter(|i| *i < runtime.sessions.len()) else {
            section(
                ui,
                "Agent sessions",
                "Create a session to run Claude Code or Codex in this project. Events stream \
                 here as the provider reports them.",
            );
            return;
        };
        let audit = runtime.audit(&runtime.sessions[index].meta.id);
        let session = &mut runtime.sessions[index];
        let meta = session.meta.clone();
        let running = session.running();
        let mut close = false;
        let mut cancel = false;
        let mut send = None;

        // Header: title, status and the facts that identify the session.
        ui.horizontal(|ui| {
            ui.label(heading(&meta.title));
            ui.add_space(6.0);
            badge(
                ui,
                &tr(meta.status.label()).to_uppercase(),
                status_color(meta.status),
            );
        });
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            chip(ui, provider_label(&meta.provider));
            chip(
                ui,
                meta.model.clone().unwrap_or_else(|| "Default model".into()),
            );
            chip(
                ui,
                PRESETS
                    .iter()
                    .find(|(p, _)| *p == meta.preset)
                    .map_or("", |(_, l)| *l),
            );
            chip(
                ui,
                format!(
                    "{} turn{}",
                    meta.turns,
                    if meta.turns == 1 { "" } else { "s" }
                ),
            );
            chip(
                ui,
                format!("{} in · {} out", meta.input_tokens, meta.output_tokens),
            );
            if meta.cost_micros > 0 {
                chip(
                    ui,
                    format!("US$ {:.4} reported", meta.cost_micros as f64 / 1e6),
                );
            }
        });
        ui.add_space(12.0);
        let permissions = match meta.provider {
            ProviderKind::ClaudeCode => {
                "Every tool call passes the COVALOS gate; edits outside the workspace are                  denied. The process still runs as your Windows user: a worktree isolates                  files, not the process."
            }
            _ => {
                "Codex runs under its own sandbox (read-only or workspace-write). It has no \
                 approval hook, so its sandbox is the effective limit."
            }
        };
        ui.columns(2, |cols| {
            // Left-aligned, unjustified text in both columns.
            cols[0].with_layout(Layout::top_down(Align::Min), |ui| {
                ui.label(label("Workspace"));
                ui.add(
                    egui::Label::new(
                        RichText::new(meta.workdir.display().to_string())
                            .monospace()
                            .size(11.5)
                            .color(ink::secondary()),
                    )
                    .wrap(),
                );
                if let Some(branch) = &meta.worktree_branch {
                    ui.label(small(format!("Own worktree · branch {branch}")));
                }
                if let Some(id) = &meta.provider_session {
                    ui.label(small(format!("Provider session {id}")));
                }
            });
            cols[1].with_layout(Layout::top_down(Align::Min), |ui| {
                ui.label(label("Permissions"));
                ui.add(egui::Label::new(support(permissions)).wrap());
            });
        });
        ui.add_space(14.0);

        // Activity timeline.
        let composer = 132.0;
        let height = (ui.available_height() - composer).max(160.0);
        Frame::new()
            .fill(ink::panel())
            .stroke(egui::Stroke::new(1.0_f32, colors().border))
            .corner_radius(12)
            .inner_margin(Margin::symmetric(16, 14))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(label("Activity"));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(small(format!("{} events", meta.log.len())));
                    });
                });
                ui.add_space(8.0);
                ScrollArea::vertical()
                    .id_salt(("agent-log", index))
                    .max_height(height - 40.0)
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        let base = meta.log.first().map_or(0, |l| l.at_ms);
                        for entry in timeline_entries(&meta.log) {
                            let time = clock(entry.at_ms, base);
                            timeline_row(ui, &time, entry.kind, |ui| match entry.kind {
                                "agent" => bubble(ui, ink::raised(), read(&entry.text)),
                                "you" => bubble(
                                    ui,
                                    colors().accent_bg,
                                    read(&entry.text).color(ink::primary()),
                                ),
                                "error" => bubble(
                                    ui,
                                    colors().error.gamma_multiply(0.12),
                                    RichText::new(&entry.text)
                                        .size(META)
                                        .line_height(Some(META_LINE))
                                        .color(colors().error),
                                ),
                                "usage" | "cost" | "provider" | "closed" => {
                                    ui.label(small(&entry.text));
                                }
                                _ => {
                                    ui.add(egui::Label::new(support(&entry.text)).wrap());
                                }
                            });
                        }
                        if running {
                            let base_time = clock(covalos_agents::now_ms(), base);
                            timeline_row(ui, &base_time, "running", |ui| {
                                ui.horizontal(|ui| {
                                    busy_indicator(ui);
                                    ui.label(support("Working…"));
                                });
                                if !session.streaming.is_empty() {
                                    bubble(
                                        ui,
                                        ink::raised(),
                                        read(&session.streaming).color(ink::secondary()),
                                    );
                                }
                            });
                        }
                        if !audit.is_empty() {
                            ui.add_space(6.0);
                            ui.label(label("COVALOS gate decisions"));
                            ui.add_space(4.0);
                            for entry in &audit {
                                let allowed = entry["allow"] == true;
                                ui.horizontal_wrapped(|ui| {
                                    badge(
                                        ui,
                                        if allowed { "ALLOW" } else { "DENY" },
                                        if allowed {
                                            colors().success
                                        } else {
                                            colors().error
                                        },
                                    );
                                    ui.label(support(
                                        entry["tool"].as_str().unwrap_or_default().to_owned(),
                                    ));
                                    ui.label(
                                        RichText::new(entry["target"].as_str().unwrap_or_default())
                                            .monospace()
                                            .size(11.5)
                                            .color(ink::secondary()),
                                    );
                                    ui.label(small(
                                        entry["reason"].as_str().unwrap_or_default().to_owned(),
                                    ));
                                });
                            }
                        }
                    });
            });
        ui.add_space(12.0);

        // Composer.
        ui.add(
            TextEdit::multiline(&mut session.draft)
                .hint_text(tr(if running {
                    "Running… you can cancel the turn"
                } else {
                    "Message the agent"
                }))
                .font(egui::FontId::proportional(READ))
                .margin(Margin::symmetric(12, 10))
                .desired_rows(2)
                .desired_width(f32::INFINITY),
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if running {
                if ui.button(tr("Cancel turn")).clicked() {
                    cancel = true;
                }
            } else {
                if ui.add(primary("Send")).clicked() {
                    send = Some(std::mem::take(&mut session.draft));
                }
                if ui.button(tr("Close session")).clicked() {
                    close = true;
                }
            }
        });
        if cancel {
            runtime.cancel(index);
        }
        if let Some(prompt) = send
            && let Err(e) = runtime.send(index, &prompt)
        {
            runtime.sessions[index].draft = prompt;
            runtime.notice = Some((false, e));
        }
        if close {
            runtime.notice = Some(match runtime.close(index) {
                Ok(note) => (true, note),
                Err(e) => (false, e),
            });
            state.selected = None;
        }
    }

    // ---------------------------------------------------------------- autonomy

    pub(super) fn autonomy_panel(&mut self, ui: &mut Ui) {
        if self.runtime_unavailable(ui) {
            return;
        }
        self.agent_notice(ui);
        let root = self.root.clone();
        ScrollArea::vertical()
            .id_salt("agent-autonomy")
            .show(ui, |ui| {
                self.approvals_strip(ui);
                self.autonomy_controls(ui, root);
            });
    }

    fn autonomy_controls(&mut self, ui: &mut Ui, root: Option<PathBuf>) {
        let state = &mut self.hub.agents;
        let runtime = state.runtime.as_mut().expect("runtime");
        let running = runtime
            .plan
            .as_ref()
            .is_some_and(|p| !p.engine.plan().state.is_settled());
        if !runtime.recoverable.is_empty() {
            card(ui, |ui| {
                ui.label(label("Interrupted plans"));
                let mut recover: Option<PathBuf> = None;
                for journal in &runtime.recoverable {
                    ui.horizontal(|ui| {
                        ui.label(small(
                            journal
                                .file_stem()
                                .map(|s| s.to_string_lossy().into_owned())
                                .unwrap_or_default(),
                        ));
                        if !running && ui.button(tr("Recover")).clicked() {
                            recover = Some(journal.clone());
                        }
                    });
                }
                if let Some(journal) = recover
                    && let Err(e) = runtime.recover_plan(&journal)
                {
                    runtime.notice = Some((false, e));
                }
            });
        }
        if !running {
            card(ui, |ui| {
                ui.label(label("New objective"));
                ui.add(
                    TextEdit::multiline(&mut state.goal)
                        .hint_text(tr("Describe the goal"))
                        .desired_rows(3)
                        .desired_width(f32::INFINITY),
                );
                ui.horizontal_wrapped(|ui| {
                    ui.selectable_value(&mut state.single_role, None, tr("Template plan"));
                    for role in RoleCatalog::builtin().iter() {
                        ui.selectable_value(
                            &mut state.single_role,
                            Some(role.id.to_string()),
                            format!("1 task · {}", role.name),
                        );
                    }
                });
                ui.label(small(
                    "Template plan: architecture → backend/frontend → tests → review → release \
                     (the goal must name components). One task: a single role does the whole \
                     goal.",
                ));
                ui.horizontal_wrapped(|ui| {
                    ui.label(small("Time budget (min)"));
                    ui.add(egui::DragValue::new(&mut state.budget_minutes).range(1..=240));
                    ui.label(small("Token budget (0 = none)"));
                    ui.add(
                        egui::DragValue::new(&mut state.budget_tokens)
                            .range(0..=5_000_000)
                            .speed(1000),
                    );
                    ui.label(small("Parallel"));
                    ui.add(egui::DragValue::new(&mut state.max_parallel).range(1..=4));
                });
                checkbox(
                    ui,
                    &mut state.plan_worktrees,
                    tr("Each task in its own git worktree"),
                );
                let Some(root) = root.clone() else {
                    ui.label(secondary("Open a project folder first."));
                    return;
                };
                ui.label(small(format!(
                    "Project: {}",
                    crate::agents_runtime::plain(&root).display()
                )));
                if ui.add(primary("Start objective")).clicked() {
                    let budget = Budget {
                        wall_ms: Some(u64::from(state.budget_minutes) * 60_000),
                        tokens: (state.budget_tokens > 0).then_some(u64::from(state.budget_tokens)),
                        cost_micros: None,
                    };
                    let mode = match state.single_role.clone() {
                        Some(role) => PlanMode::Single { role },
                        None => PlanMode::Template,
                    };
                    if let Err(e) = runtime.start_plan(
                        &state.goal,
                        mode,
                        &root,
                        state.plan_worktrees,
                        budget,
                        state.max_parallel,
                    ) {
                        runtime.notice = Some((false, e));
                    }
                }
            });
        }
        let Some(run) = runtime.plan.as_mut() else {
            return;
        };
        let plan = run.engine.plan().clone();
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(&plan.goal).strong());
                ui.label(small(format!("{:?}", plan.state)));
            });
            ui.label(small(format!(
                "{} · spent {:.1} min · {} tokens{}",
                plan.id,
                plan.spend.wall_ms as f64 / 60_000.0,
                plan.spend.tokens,
                if plan.spend.cost_reported {
                    format!(" · US$ {:.4}", plan.spend.cost_micros as f64 / 1e6)
                } else {
                    String::new()
                }
            )));
            ui.horizontal(|ui| {
                let toggle = if plan.state == covalos_engine::PlanState::Paused {
                    ui.button(tr("Resume"))
                        .clicked()
                        .then(|| run.engine.resume())
                } else if !plan.state.is_settled() {
                    ui.button(tr("Pause")).clicked().then(|| run.engine.pause())
                } else {
                    None
                };
                let cancel = (!plan.state.is_settled() && ui.button(tr("Cancel plan")).clicked())
                    .then(|| run.engine.cancel());
                if let Some(Err(e)) = toggle.or(cancel) {
                    run.error = Some(e.to_string());
                }
            });
            if let Some(error) = &run.error {
                ui.colored_label(colors().error, error);
            }
            ui.add_space(6.0);
            for task in plan.tasks() {
                let (color, label) = match &task.status {
                    TaskStatus::Succeeded => (colors().success, "Succeeded".to_owned()),
                    TaskStatus::Failed { reason } => (colors().error, format!("Failed: {reason}")),
                    TaskStatus::Running => (colors().accent, "Running".into()),
                    TaskStatus::AwaitingApproval => (colors().warning, "Awaiting approval".into()),
                    TaskStatus::Retrying { reason, .. } => {
                        (colors().warning, format!("Retrying: {reason}"))
                    }
                    TaskStatus::Skipped { because } => {
                        (colors().muted, format!("Skipped (needs {because})"))
                    }
                    TaskStatus::Cancelled => (colors().muted, "Cancelled".into()),
                    TaskStatus::Pending => (colors().muted, "Pending".into()),
                };
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(color, "●");
                    ui.label(
                        RichText::new(format!("{} · {}", task.spec.id, task.spec.title)).strong(),
                    );
                    ui.label(small(format!(
                        "{} · {} · attempt {} · {} tokens · {}",
                        task.spec.role,
                        task.account
                            .as_ref()
                            .map_or_else(|| "unassigned".into(), ToString::to_string),
                        task.attempts,
                        task.spend.tokens,
                        label
                    )));
                });
            }
            let pending: Vec<(String, String)> = run
                .engine
                .pending_approvals()
                .into_iter()
                .map(|(request, task)| {
                    (
                        request.ticket.clone(),
                        format!("{task}: {}", crate::agent_gate::describe(&request.action)),
                    )
                })
                .collect();
            let mut resolve = None;
            for (ticket, text) in &pending {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(colors().warning, tr("Engine approval"));
                    ui.label(crate::agent_gate::redact(text));
                    if ui.add(primary("Allow once")).clicked() {
                        resolve = Some((ticket.clone(), Resolution::AllowOnce));
                    }
                    if ui.button(tr("Deny")).clicked() {
                        resolve = Some((ticket.clone(), Resolution::Deny));
                    }
                });
            }
            if let Some((ticket, resolution)) = resolve
                && let Err(e) = run.engine.resolve(&ticket, resolution)
            {
                run.error = Some(e.to_string());
            }
            let context: Vec<_> = run.engine.context().since(0).cloned().collect();
            if !context.is_empty() {
                ui.add_space(6.0);
                ui.label(small("Plan context"));
                for entry in context.iter().rev().take(10) {
                    ui.label(small(format!(
                        "#{} {:?} · {}",
                        entry.seq,
                        entry.kind,
                        entry.text.chars().take(260).collect::<String>()
                    )));
                }
            }
            ui.label(small(format!(
                "Journal: {}",
                run.engine.journal_path().display()
            )));
        });
    }
}

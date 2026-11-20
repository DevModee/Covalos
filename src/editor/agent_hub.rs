//! Agent Hub V4/V5 screens: accounts, live sessions with approvals and shared
//! context, and the autonomous engine. Drawn with the existing COVALOS
//! components (section, row, primary, checkbox, Frame surfaces).
use std::{path::PathBuf, sync::Arc};

use covalos_agents::{
    Account, AccountId, ConnectionMethod, ProviderKind, context::EntryKind, permission::Resolution,
};
use covalos_engine::{TaskStatus, budget::Budget, role::RoleCatalog};
use eframe::egui::{self, Align, Frame, Layout, Margin, RichText, ScrollArea, TextEdit, Ui};

use super::{Editor, theme::*};
use crate::agents_runtime::{PlanMode, PlanSetup, Runtime};

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
    pub(super) selected: Option<usize>,
    note: String,
    goal: String,
    single_role: Option<String>,
    plan_worktrees: bool,
    budget_minutes: u32,
    budget_tokens: u32,
    max_parallel: usize,
    pub(super) chat: super::agent_chat::ChatState,
    /// 0 template, 1 one role, 2 an agent writes the plan.
    planner: u8,
    planner_account: Option<AccountId>,
    pins: std::collections::BTreeMap<String, AccountId>,
}

impl State {
    pub(super) fn runtime_and_chat(&mut self) -> (&mut Runtime, &mut super::agent_chat::ChatState) {
        (self.runtime.as_mut().expect("runtime"), &mut self.chat)
    }

    pub(super) fn open_error_text(&self) -> Option<String> {
        self.open_error.clone()
    }
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
            selected: None,
            note: String::new(),
            goal: String::new(),
            single_role: Some("reviewer".into()),
            plan_worktrees: true,
            budget_minutes: 20,
            budget_tokens: 0,
            max_parallel: 2,
            chat: super::agent_chat::ChatState::default(),
            planner: 0,
            planner_account: None,
            pins: std::collections::BTreeMap::new(),
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
                        "Install the official Gemini CLI to detect it. Covalos has no verified \
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
                 Covalos never reads or copies them.",
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
                        super::agent_chat::provider_name(&account.provider),
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
        // Older callers (and the evidence harness) select a session by index.
        if let Some(index) = self.hub.agents.selected.take()
            && let Some(id) = self
                .hub
                .agents
                .runtime
                .as_ref()
                .and_then(|r| r.sessions.get(index))
                .map(|s| s.meta.id.as_str().to_owned())
        {
            self.hub.agents.chat.page = Some(id);
        }
        let rect = ui.available_rect_before_wrap();
        let side = if rect.width() > 960.0 { 290.0 } else { 0.0 };
        let main = egui::Rect::from_min_max(
            rect.min,
            egui::pos2(
                rect.right() - side - if side > 0.0 { GUTTER } else { 0.0 },
                rect.bottom(),
            ),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(main), |ui| {
            if side == 0.0 {
                self.approvals_strip(ui);
            }
            Frame::new()
                .fill(ink::panel())
                .stroke(egui::Stroke::new(1.0_f32, colors().border))
                .corner_radius(12)
                .inner_margin(Margin::same(14))
                .show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    self.agents_page(ui);
                });
        });
        if side > 0.0 {
            let column =
                egui::Rect::from_min_max(egui::pos2(rect.right() - side, rect.top()), rect.max);
            ui.scope_builder(egui::UiBuilder::new().max_rect(column), |ui| {
                ScrollArea::vertical()
                    .id_salt("agent-side")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.approvals_strip(ui);
                        self.context_panel(ui);
                    });
            });
        }
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

    // ---------------------------------------------------------------- orchestrator

    pub(super) fn autonomy_panel(&mut self, ui: &mut Ui) {
        if self.runtime_unavailable(ui) {
            return;
        }
        let root = self.root.clone();
        let rect = ui.available_rect_before_wrap();
        let wide = rect.width() > 900.0;
        let setup_width = if wide {
            (rect.width() * 0.4).clamp(340.0, 440.0)
        } else {
            rect.width()
        };
        ui.label(heading(tr("Orchestrator")));
        ui.label(support(tr(
            "Turn an objective into tasks, assign each role to one of your accounts, and supervise \
             the run. Everything shown comes from the engine's live state.",
        )));
        ui.add_space(12.0);
        self.agent_notice(ui);
        let top = ui.cursor().min.y;
        let setup = egui::Rect::from_min_max(
            egui::pos2(rect.left(), top),
            egui::pos2(rect.left() + setup_width, rect.bottom()),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(setup), |ui| {
            ScrollArea::vertical()
                .id_salt("orchestrator-setup")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_max_width(setup_width - 12.0);
                    self.orchestrator_setup(ui, root.clone());
                    if !wide {
                        self.orchestrator_live(ui);
                    }
                });
        });
        if wide {
            let live = egui::Rect::from_min_max(egui::pos2(setup.right() + GUTTER, top), rect.max);
            ui.scope_builder(egui::UiBuilder::new().max_rect(live), |ui| {
                ScrollArea::vertical()
                    .id_salt("orchestrator-live")
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.orchestrator_live(ui));
            });
        }
    }

    fn orchestrator_setup(&mut self, ui: &mut Ui, root: Option<PathBuf>) {
        let state = &mut self.hub.agents;
        let runtime = state.runtime.as_mut().expect("runtime");
        let accounts: Vec<Account> = runtime
            .profiles
            .accounts()
            .iter()
            .filter(|a| {
                a.enabled && matches!(a.provider, ProviderKind::ClaudeCode | ProviderKind::Codex)
            })
            .cloned()
            .collect();
        let describe = |a: &Account| {
            format!(
                "{} · {} · {}",
                a.label,
                super::agent_chat::provider_name(&a.provider),
                a.default_model
                    .clone()
                    .unwrap_or_else(|| "default model".into())
            )
        };
        card(ui, |ui| {
            ui.label(label("Orchestrator"));
            for (i, (title, detail)) in [
                (
                    "Template planner",
                    "Offline and deterministic: design → build → tests → review → release.",
                ),
                ("One role", "A single role does the whole objective."),
                (
                    "An agent writes the plan",
                    "Your chosen account plans the tasks as JSON; the engine validates it.",
                ),
            ]
            .iter()
            .enumerate()
            {
                ui.horizontal(|ui| {
                    ui.radio_value(
                        &mut state.planner,
                        i as u8,
                        RichText::new(tr(title)).strong(),
                    );
                });
                ui.label(small(tr(detail)));
            }
            if state.planner == 1 {
                ui.horizontal_wrapped(|ui| {
                    for role in RoleCatalog::builtin().iter() {
                        ui.selectable_value(
                            &mut state.single_role,
                            Some(role.id.to_string()),
                            &role.name,
                        );
                    }
                });
            }
            if state.planner == 2 {
                if accounts.is_empty() {
                    ui.label(small(tr("Add an account first.")));
                }
                egui::ComboBox::from_id_salt("planner-account")
                    .width(ui.available_width())
                    .truncate()
                    .selected_text(
                        accounts
                            .iter()
                            .find(|a| Some(&a.id) == state.planner_account.as_ref())
                            .map_or_else(|| tr("Choose the planning account").to_owned(), describe),
                    )
                    .show_ui(ui, |ui| {
                        for a in &accounts {
                            ui.selectable_value(
                                &mut state.planner_account,
                                Some(a.id.clone()),
                                describe(a),
                            );
                        }
                    });
            }
        });
        card(ui, |ui| {
            ui.label(label("Roles"));
            ui.label(small(tr(
                "Automatic lets the engine pick a signed-in account. A pinned role only runs on that \
                 account and uses its model.",
            )));
            ui.add_space(4.0);
            for role in RoleCatalog::builtin().iter() {
                let key = role.id.to_string();
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [110.0, 22.0],
                        egui::Label::new(RichText::new(&role.name).strong()).truncate(),
                    );
                    let current = state.pins.get(&key).cloned();
                    egui::ComboBox::from_id_salt(("pin", &key))
                        .width(ui.available_width())
                        .truncate()
                        .selected_text(
                            current
                                .as_ref()
                                .and_then(|id| accounts.iter().find(|a| &a.id == id))
                                .map_or_else(|| tr("Automatic").to_owned(), describe),
                        )
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(current.is_none(), tr("Automatic"))
                                .clicked()
                            {
                                state.pins.remove(&key);
                            }
                            for a in &accounts {
                                if ui
                                    .selectable_label(current.as_ref() == Some(&a.id), describe(a))
                                    .clicked()
                                {
                                    state.pins.insert(key.clone(), a.id.clone());
                                }
                            }
                        });
                });
            }
        });
        card(ui, |ui| {
            ui.label(label("Budget"));
            ui.horizontal_wrapped(|ui| {
                ui.label(small("Minutes"));
                ui.add(egui::DragValue::new(&mut state.budget_minutes).range(1..=240));
                ui.label(small("Tokens (0 = none)"));
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
        });
        let mut start = None;
        card(ui, |ui| {
            ui.label(label("Objective"));
            let Some(root) = root else {
                ui.label(secondary("Open a project folder first."));
                return;
            };
            ui.add(
                TextEdit::multiline(&mut state.goal)
                    .hint_text(tr(
                        "e.g. Build the landing page hero and pricing sections in React",
                    ))
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );
            ui.label(small(format!(
                "Project: {}",
                crate::agents_runtime::plain(&root).display()
            )));
            let busy = runtime.planning.is_some()
                || runtime
                    .plan
                    .as_ref()
                    .is_some_and(|p| !p.engine.plan().state.is_settled());
            if ui
                .add_enabled(
                    !busy && !state.goal.trim().is_empty(),
                    primary(tr("Start objective")),
                )
                .clicked()
            {
                start = Some(std::mem::take(&mut state.goal));
            }
        });
        if let Some(goal) = start {
            self.start_objective(&goal);
        }
    }

    /// Starts an objective with the orchestrator settings; used by the
    /// Orchestrator page and by the universal composer.
    pub(super) fn start_objective(&mut self, goal: &str) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let state = &mut self.hub.agents;
        let Some(runtime) = state.runtime.as_mut() else {
            return;
        };
        let mut pins = std::collections::BTreeMap::new();
        for (role, account) in &state.pins {
            if let Ok(role) = covalos_agents::RoleId::new(role.clone()) {
                pins.insert(role, account.clone());
            }
        }
        let setup = PlanSetup {
            worktrees: state.plan_worktrees,
            budget: Budget {
                wall_ms: Some(u64::from(state.budget_minutes) * 60_000),
                tokens: (state.budget_tokens > 0).then_some(u64::from(state.budget_tokens)),
                cost_micros: None,
            },
            max_parallel: state.max_parallel,
            pins,
        };
        let result = match state.planner {
            2 => match state.planner_account.clone() {
                Some(account) => {
                    let model = runtime
                        .profiles
                        .accounts()
                        .iter()
                        .find(|a| a.id == account)
                        .and_then(|a| a.default_model.clone());
                    runtime.request_plan(goal, account, model, &root, setup)
                }
                None => Err("Choose the account that writes the plan.".into()),
            },
            1 => runtime.start_plan(
                goal,
                PlanMode::Single {
                    role: state
                        .single_role
                        .clone()
                        .unwrap_or_else(|| "reviewer".into()),
                },
                &root,
                setup,
            ),
            _ => runtime.start_plan(goal, PlanMode::Template, &root, setup),
        };
        runtime.notice = Some(match result {
            Ok(()) if state.planner == 2 => (true, "The planner agent is writing the plan.".into()),
            Ok(()) => (true, "Objective started.".into()),
            Err(e) => (false, e),
        });
    }

    fn orchestrator_live(&mut self, ui: &mut Ui) {
        let state = &mut self.hub.agents;
        let runtime = state.runtime.as_mut().expect("runtime");
        let mut open_session = None;
        if let Some(planning) = &runtime.planning {
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    busy_indicator(ui);
                    ui.label(read(tr("An agent is writing the plan…")));
                });
                ui.label(small(planning.goal.clone()));
                if ui.small_button(tr("Watch its conversation")).clicked() {
                    open_session = Some(planning.session.as_str().to_owned());
                }
            });
        }
        if !runtime.recoverable.is_empty() {
            let running = runtime
                .plan
                .as_ref()
                .is_some_and(|p| !p.engine.plan().state.is_settled());
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
        let Some(run) = runtime.plan.as_mut() else {
            if runtime.planning.is_none() {
                card(ui, |ui| {
                    ui.label(label("Live run"));
                    ui.label(support(tr(
                        "No objective yet. Start one here or send it from any chat with \
                         Covalos Orchestrator selected.",
                    )));
                });
            }
            if let Some(id) = open_session {
                state.chat.page = Some(id);
                self.hub.focus_agents("agents");
            }
            return;
        };
        let plan = run.engine.plan().clone();
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(heading(&plan.goal));
                badge(
                    ui,
                    &format!("{:?}", plan.state).to_uppercase(),
                    plan_color(&plan.state),
                );
            });
            ui.label(small(format!(
                "spent {:.1} min · {} tokens{}",
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
        });
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
        if !pending.is_empty() {
            card(ui, |ui| {
                badge(ui, "APPROVALS WAITING", colors().warning);
                let mut resolve = None;
                for (ticket, text) in &pending {
                    ui.label(
                        RichText::new(crate::agent_gate::redact(text))
                            .monospace()
                            .size(SMALL),
                    );
                    ui.horizontal(|ui| {
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
            });
        }
        card(ui, |ui| {
            ui.label(label("Tasks"));
            for task in plan.tasks() {
                let status = task_label(&task.status);
                Frame::new()
                    .fill(ink::raised())
                    .corner_radius(8)
                    .inner_margin(Margin::symmetric(10, 7))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let (dot, _) = ui
                                .allocate_exact_size(egui::vec2(10.0, 16.0), egui::Sense::hover());
                            ui.painter()
                                .circle_filled(dot.center(), 4.0, task_color(&task.status));
                            ui.label(
                                RichText::new(&task.spec.title)
                                    .strong()
                                    .color(ink::primary()),
                            );
                        });
                        ui.label(small(format!(
                            "{} · {} · attempt {} · {} tokens · {}",
                            task.spec.role,
                            task.account
                                .as_ref()
                                .map_or_else(|| "unassigned".into(), ToString::to_string),
                            task.attempts,
                            task.spend.tokens,
                            status
                        )));
                    });
                ui.add_space(4.0);
            }
        });
        let context: Vec<_> = run.engine.context().since(0).cloned().collect();
        card(ui, |ui| {
            ui.label(label("Results"));
            if context.is_empty() {
                ui.label(small(tr(
                    "Agents publish their results here as tasks finish.",
                )));
            }
            for entry in context.iter().rev().take(12) {
                egui::CollapsingHeader::new(small(format!(
                    "#{} {:?} · {}",
                    entry.seq,
                    entry.kind,
                    entry.text.chars().take(70).collect::<String>()
                )))
                .id_salt(("plan-result", entry.seq))
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(support(entry.text.clone()))
                            .wrap()
                            .selectable(true),
                    );
                });
            }
            ui.label(small(format!(
                "Journal: {}",
                run.engine.journal_path().display()
            )));
        });
        if let Some(id) = open_session {
            state.chat.page = Some(id);
            self.hub.focus_agents("agents");
        }
    }
}

pub(super) fn task_color(status: &TaskStatus) -> egui::Color32 {
    match status {
        TaskStatus::Succeeded => colors().success,
        TaskStatus::Failed { .. } => colors().error,
        TaskStatus::Running => colors().accent,
        TaskStatus::AwaitingApproval | TaskStatus::Retrying { .. } => colors().warning,
        _ => colors().muted,
    }
}

fn task_label(status: &TaskStatus) -> String {
    match status {
        TaskStatus::Succeeded => "Succeeded".into(),
        TaskStatus::Failed { reason } => format!("Failed: {reason}"),
        TaskStatus::Running => "Running".into(),
        TaskStatus::AwaitingApproval => "Awaiting approval".into(),
        TaskStatus::Retrying { reason, .. } => format!("Retrying: {reason}"),
        TaskStatus::Skipped { because } => format!("Skipped (needs {because})"),
        TaskStatus::Cancelled => "Cancelled".into(),
        TaskStatus::Pending => "Pending".into(),
    }
}

fn plan_color(state: &covalos_engine::PlanState) -> egui::Color32 {
    use covalos_engine::PlanState;
    match state {
        PlanState::Paused => colors().warning,
        s if s.is_settled() => colors().secondary,
        _ => colors().accent,
    }
}

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
    selected: Option<usize>,
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

fn card(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    Frame::new()
        .fill(colors().surface)
        .stroke(egui::Stroke::new(
            1.0_f32,
            colors().border.gamma_multiply(0.8),
        ))
        .inner_margin(Margin::same(14))
        .corner_radius(12)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
    ui.add_space(10.0);
}

fn small(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).size(SMALL).color(colors().muted)
}

fn status_color(status: Status) -> egui::Color32 {
    match status {
        Status::Running => colors().accent,
        Status::Completed => colors().success,
        Status::Failed => colors().error,
        Status::Interrupted | Status::Cancelled => colors().warning,
        Status::Idle => colors().muted,
    }
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

    fn ensure_runtime(&mut self, ctx: &egui::Context) -> bool {
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
                ui.label(
                    RichText::new(tr("PROVIDERS ON THIS PC"))
                        .size(10.0)
                        .color(colors().muted),
                );
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
            ui.label(RichText::new(tr("Add account")).strong());
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
        let list_width = (rect.width() * 0.34).clamp(240.0, 330.0);
        let list = egui::Rect::from_min_size(rect.min, egui::vec2(list_width, rect.height()));
        let detail =
            egui::Rect::from_min_max(egui::pos2(list.right() + 20.0, rect.top()), rect.max);
        ui.scope_builder(egui::UiBuilder::new().max_rect(list), |ui| {
            ScrollArea::vertical()
                .id_salt("agent-session-list")
                .show(ui, |ui| {
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
            ui.label(
                RichText::new(tr("APPROVALS WAITING"))
                    .size(10.0)
                    .color(colors().warning),
            );
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
        ui.label(RichText::new(tr("Sessions")).size(LEAD).strong());
        ui.add_space(6.0);
        if runtime.sessions.is_empty() {
            ui.label(secondary("No sessions yet."));
        }
        for (index, session) in runtime.sessions.iter().enumerate() {
            let meta = &session.meta;
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
            let response = row(ui, &meta.title, &detail, state.selected == Some(index));
            let dot = response.rect.left_center() - egui::vec2(6.0, 0.0);
            ui.painter()
                .circle_filled(dot, 3.0, status_color(meta.status));
            if response.clicked() {
                state.selected = Some(index);
            }
        }
        ui.add_space(12.0);
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
            ui.label(RichText::new(tr("New session")).strong());
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
            ui.label(small(format!("Project: {}", root.display())));
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
            ui.label(RichText::new(tr("Shared context")).strong());
            ui.label(small(
                "Results and notes every session can read. Transcripts are never shared; \
                 secret-looking text is refused.",
            ));
            let entries: Vec<_> = runtime.context.since(0).cloned().collect();
            for entry in entries.iter().rev().take(12) {
                ui.label(small(format!(
                    "#{} {:?}{} · {}",
                    entry.seq,
                    entry.kind,
                    if entry.pinned { " · pinned" } else { "" },
                    entry.text.chars().take(220).collect::<String>()
                )));
            }
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
        ui.horizontal(|ui| {
            ui.label(RichText::new(&meta.title).size(LEAD).strong());
            ui.colored_label(status_color(meta.status), tr(meta.status.label()));
        });
        ui.label(small(format!(
            "{} · {} · {} · {} turns · {} in / {} out tokens{}",
            provider_label(&meta.provider),
            meta.model.clone().unwrap_or_else(|| "default model".into()),
            PRESETS
                .iter()
                .find(|(p, _)| *p == meta.preset)
                .map_or("", |(_, l)| *l),
            meta.turns,
            meta.input_tokens,
            meta.output_tokens,
            if meta.cost_micros > 0 {
                format!(" · US$ {:.4} reported", meta.cost_micros as f64 / 1e6)
            } else {
                String::new()
            }
        )));
        ui.label(small(format!(
            "Workdir {}{}",
            meta.workdir.display(),
            meta.worktree_branch
                .as_deref()
                .map(|b| format!(" · branch {b}"))
                .unwrap_or_default()
        )));
        ui.label(small(match meta.provider {
            ProviderKind::ClaudeCode => {
                "COVALOS gate: every tool call passes the session policy; edits outside the \
                 workdir are denied. Process limits: Claude permission mode and your Windows \
                 user. A worktree isolates files, not the process."
            }
            _ => {
                "Codex runs under its own sandbox (read-only or workspace-write). It has no \
                 hook for COVALOS approvals, so its sandbox is the effective limit."
            }
        }));
        if let Some(id) = &meta.provider_session {
            ui.label(small(format!(
                "Provider session {id} (resumed on the next message)"
            )));
        }
        ui.add_space(6.0);
        let mut close = false;
        let mut cancel = false;
        let mut send = None;
        let log_height = (ui.available_height() - 118.0).max(120.0);
        Frame::new()
            .fill(colors().canvas.gamma_multiply(0.96))
            .corner_radius(10)
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ScrollArea::vertical()
                    .id_salt(("agent-log", index))
                    .max_height(log_height)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for line in &meta.log {
                            let color = match line.kind.as_str() {
                                "error" => colors().error,
                                "you" => colors().accent,
                                "agent" => colors().text,
                                "approval" | "recovery" => colors().warning,
                                _ => colors().secondary,
                            };
                            ui.horizontal_wrapped(|ui| {
                                ui.label(small(format!("{:>8}", line.kind)));
                                ui.label(RichText::new(&line.text).color(color));
                            });
                        }
                        if !session.streaming.is_empty() {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(small("   delta"));
                                ui.label(
                                    RichText::new(&session.streaming).color(colors().secondary),
                                );
                            });
                        }
                        if !audit.is_empty() {
                            ui.add_space(6.0);
                            ui.label(small("Gate decisions"));
                            for entry in &audit {
                                ui.label(small(format!(
                                    "{} {} · {} ({})",
                                    if entry["allow"] == true { "✔" } else { "✖" },
                                    entry["tool"].as_str().unwrap_or_default(),
                                    entry["target"].as_str().unwrap_or_default(),
                                    entry["reason"].as_str().unwrap_or_default()
                                )));
                            }
                        }
                    });
            });
        ui.add_space(6.0);
        let running = session.running();
        ui.add(
            TextEdit::multiline(&mut session.draft)
                .hint_text(tr(if running {
                    "Running… you can cancel the turn"
                } else {
                    "Message the agent"
                }))
                .desired_rows(2)
                .desired_width(f32::INFINITY),
        );
        ui.horizontal(|ui| {
            if running {
                busy_indicator(ui);
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
                ui.label(RichText::new(tr("Interrupted plans")).strong());
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
                ui.label(RichText::new(tr("New objective")).strong());
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
                ui.label(small(format!("Project: {}", root.display())));
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

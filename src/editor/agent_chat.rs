//! Agent conversations: one chat surface backed by real runtime sessions, used
//! by the editor's right dock, the Agents page (with an optional split) and
//! detached windows. Everything shown comes from the session's own state.
use std::path::PathBuf;

use covalos_agents::{Account, AccountId, ConnectionMethod, ProviderKind, permission::Preset};
use eframe::egui::{
    self, Align, Frame, Id, Key, Layout, Margin, RichText, ScrollArea, TextEdit, Ui,
};

use super::{Editor, theme::*};
use crate::agents_runtime::{Connection, LiveSession, NewSession, Runtime, Status};

const PRESETS: [(Preset, &str); 3] = [
    (Preset::ReadOnly, "Read only"),
    (Preset::Supervised, "Supervised"),
    (Preset::Worktree, "Edit inside workdir"),
];

/// Where a conversation is drawn; each place remembers its own session.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(super) enum Slot {
    Dock,
    Page,
    Split,
    Window(String),
}

#[derive(Clone, PartialEq, Eq, Default)]
pub(super) enum Target {
    #[default]
    Auto,
    Account(AccountId),
    Orchestrator,
}

pub(super) struct ChatState {
    pub(super) dock: Option<String>,
    pub(super) page: Option<String>,
    pub(super) split: Option<String>,
    pub(super) detached: Vec<String>,
    pub(super) target: Target,
    pub(super) model: Option<String>,
    pub(super) effort: String,
    custom_model: String,
    preset: usize,
    worktree: bool,
    draft: String,
    attach: bool,
    pub(super) add_ai: Option<AddAi>,
    changes: Vec<(String, Result<String, String>)>,
    diff: Option<(String, String, String)>,
    pub(super) restored: bool,
}

impl Default for ChatState {
    fn default() -> Self {
        Self {
            dock: None,
            page: None,
            split: None,
            detached: Vec::new(),
            target: Target::Auto,
            model: None,
            effort: "Medium".into(),
            custom_model: String::new(),
            preset: 1,
            worktree: false,
            draft: String::new(),
            attach: true,
            add_ai: None,
            changes: Vec::new(),
            diff: None,
            restored: false,
        }
    }
}

impl ChatState {
    fn slot(&self, slot: &Slot) -> Option<String> {
        match slot {
            Slot::Dock => self.dock.clone(),
            Slot::Page => self.page.clone(),
            Slot::Split => self.split.clone().filter(|s| !s.is_empty()),
            Slot::Window(id) => Some(id.clone()),
        }
    }

    fn set(&mut self, slot: &Slot, id: Option<String>) {
        match slot {
            Slot::Dock => self.dock = id,
            Slot::Page => self.page = id,
            Slot::Split => self.split = Some(id.unwrap_or_default()),
            Slot::Window(old) => {
                if id.is_none() {
                    self.detached.retain(|d| d != old);
                }
            }
        }
    }
}

#[derive(Default)]
pub(super) struct AddAi {
    provider: usize,
    method: usize,
    label: String,
    model: String,
    key: String,
    created: Option<AccountId>,
    error: Option<String>,
    install: Option<std::sync::mpsc::Receiver<Result<(), String>>>,
}

fn cli_npm_package(provider: usize) -> Option<&'static str> {
    match provider {
        0 => Some("@anthropic-ai/claude-code"),
        1 => Some("@openai/codex"),
        2 => Some("@google/gemini-cli"),
        _ => None,
    }
}

fn spawn_cli_install(
    package: &'static str,
    ctx: egui::Context,
) -> std::sync::mpsc::Receiver<Result<(), String>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut command = std::process::Command::new(if cfg!(windows) { "npm.cmd" } else { "npm" });
        crate::process::configure(&mut command);
        let result = command
            .args(["install", "-g", package])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .output()
            .map_err(|e| format!("Could not run npm (install Node.js first): {e}"))
            .and_then(|out| {
                if out.status.success() {
                    Ok(())
                } else {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    let line = stderr
                        .lines()
                        .rev()
                        .find(|l| !l.trim().is_empty())
                        .unwrap_or("npm install failed");
                    Err(line.trim().to_owned())
                }
            });
        let _ = tx.send(result);
        ctx.request_repaint();
    });
    rx
}

enum Action {
    Select(Slot, Option<String>),
    Detach(String),
    Open(PathBuf),
    OpenGuided(String),
    Objective(String),
    OpenOrchestrator,
    AddAi,
}

pub(super) fn curated_cli_models(kind: &ProviderKind) -> &'static [(&'static str, &'static str)] {
    match kind {
        ProviderKind::ClaudeCode => &[
            ("claude-opus-5-5", "Opus 5.5"),
            ("claude-sonnet-5-5", "Sonnet 5.5"),
            ("claude-haiku-5-5", "Haiku 5.5"),
        ],
        ProviderKind::Gemini => &[
            ("gemini-4-argon", "Gemini 4 Argon"),
            ("gemini-4-neon", "Gemini 4 Neon"),
            ("gemini-4-hydrogen", "Gemini 4 Hydrogen"),
            ("gemini-3.8-flash", "Gemini 3.8 Flash"),
        ],
        ProviderKind::Codex => &[
            ("gpt-5.4", "GPT-5.4"),
            ("gpt-5.3-codex", "GPT-5.3 Codex"),
            ("gpt-5.2-codex", "GPT-5.2 Codex"),
            ("o4-mini", "o4-mini"),
        ],
        _ => &[],
    }
}

pub(super) fn curated_model_label(id: &str) -> Option<&'static str> {
    [
        ProviderKind::ClaudeCode,
        ProviderKind::Gemini,
        ProviderKind::Codex,
    ]
    .iter()
    .flat_map(curated_cli_models)
    .find(|(m_id, _)| *m_id == id)
    .map(|(_, label)| *label)
}

pub(super) fn model_belongs_to_provider(model: &str, provider: &ProviderKind) -> bool {
    let lower = model.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return false;
    }
    match provider {
        ProviderKind::ClaudeCode => {
            !lower.starts_with("gemini")
                && !lower.starts_with("gpt-")
                && !lower.starts_with("o1")
                && !lower.starts_with("o3")
                && !lower.starts_with("o4")
                && !lower.contains("codex")
        }
        ProviderKind::Codex => !lower.starts_with("claude") && !lower.starts_with("gemini"),
        ProviderKind::Gemini => lower.starts_with("gemini"),
        _ => true,
    }
}

pub(super) fn provider_name(kind: &ProviderKind) -> &'static str {
    match kind {
        ProviderKind::ClaudeCode => "Claude Code",
        ProviderKind::Codex => "Codex",
        ProviderKind::Gemini => "Gemini CLI",
        _ => "Custom",
    }
}

pub(super) fn method_name(method: ConnectionMethod) -> &'static str {
    match method {
        ConnectionMethod::SystemCli => "CLI sign-in",
        ConnectionMethod::OfficialLogin => "Isolated sign-in",
        ConnectionMethod::ApiKey => "API key",
        #[allow(unreachable_patterns)]
        _ => "Other",
    }
}

fn preset_name(preset: Preset) -> &'static str {
    PRESETS
        .iter()
        .find(|(p, _)| *p == preset)
        .map_or("Custom", |(_, l)| *l)
}

/// Accounts that can run a chat turn today.
fn chat_accounts(runtime: &Runtime) -> Vec<Account> {
    runtime
        .profiles
        .accounts()
        .iter()
        .filter(|a| {
            a.enabled && matches!(a.provider, ProviderKind::ClaudeCode | ProviderKind::Codex)
        })
        .cloned()
        .collect()
}

/// Models offered for an account: only what the account declares, what its
/// sessions already ran, and the model aliases the provider's CLI documents.
fn model_options(runtime: &Runtime, account: &Account) -> Vec<String> {
    let mut models: Vec<String> = Vec::new();
    let mut push = |m: &str| {
        if !m.trim().is_empty() && !models.iter().any(|x| x == m) {
            models.push(m.to_owned());
        }
    };
    if let Some(m) = &account.default_model {
        push(m);
    }
    for m in &account.declared_models {
        push(m);
    }
    for s in &runtime.sessions {
        if s.meta.account == account.id
            && let Some(m) = &s.meta.model
        {
            push(m);
        }
    }
    if account.provider == ProviderKind::ClaudeCode {
        for alias in ["sonnet", "opus", "haiku"] {
            push(alias);
        }
    }
    models
}

/// Dot color and words for an account's sign-in, from the provider's answer.
fn connection_view(runtime: &Runtime, account: &Account) -> (egui::Color32, String) {
    let detection = runtime.detections.get(&account.provider);
    if detection.is_some_and(|d| d.done && d.program.is_none()) {
        return (colors().error, "Not installed".into());
    }
    match runtime.connection.get(&account.id) {
        None | Some(Connection::Checking) => (colors().muted, "Checking…".into()),
        Some(Connection::SignedIn(_)) => (colors().success, "Connected".into()),
        Some(Connection::SignedOut(_)) => (colors().warning, "Signed out".into()),
        Some(Connection::Key(true)) => (colors().success, "API key stored".into()),
        Some(Connection::Key(false)) => (colors().warning, "No API key".into()),
        Some(Connection::Unknown(_)) => (colors().muted, "Status unknown".into()),
    }
}

fn ensure_connection(runtime: &mut Runtime, account: &AccountId) {
    let detected = runtime
        .profiles
        .accounts()
        .iter()
        .find(|a| &a.id == account)
        .and_then(|a| runtime.detections.get(&a.provider))
        .is_some_and(|d| d.done);
    if detected && !runtime.connection.contains_key(account) {
        runtime.refresh_connection(account);
    }
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

fn dot(ui: &mut Ui, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 14.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 3.5, color);
}

fn meta_text(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).size(SMALL).color(colors().muted)
}

fn fact(ui: &mut Ui, text: impl Into<String>) {
    ui.label(
        RichText::new(text.into())
            .size(SMALL)
            .color(colors().secondary),
    );
}

fn separator_dot(ui: &mut Ui) {
    ui.label(RichText::new("·").size(SMALL).color(colors().muted));
}

/// Opaque reading surface used behind every chat block.
fn surface() -> egui::Color32 {
    super::motion::blend(colors().inset, colors().surface, 0.55)
}

impl Editor {
    fn active_file(&self) -> Option<String> {
        let doc = self.active.and_then(|i| self.documents.get(i))?;
        let root = self.root.as_deref()?;
        let rel = doc.path.strip_prefix(root).unwrap_or(&doc.path);
        Some(rel.to_string_lossy().replace('\\', "/"))
    }

    /// Restores the dock layout saved in preferences once the runtime is open.
    fn restore_chat_layout(&mut self) {
        let chat = &mut self.hub.agents.chat;
        if chat.restored {
            return;
        }
        chat.restored = true;
        chat.dock = self.preferences.agent_session.clone();
        chat.detached = self.preferences.agent_detached.clone();
    }

    fn remember_chat_layout(&mut self) {
        let chat = &self.hub.agents.chat;
        if !chat.restored {
            return;
        }
        if self.preferences.agent_session != chat.dock {
            self.preferences.agent_session = chat.dock.clone();
        }
        if self.preferences.agent_detached != chat.detached {
            self.preferences.agent_detached = chat.detached.clone();
        }
    }

    /// The editor's right dock.
    pub(super) fn agent_dock(&mut self, ui: &mut Ui) {
        self.conversation(ui, Slot::Dock);
    }

    /// Agents page: one conversation, or two side by side.
    pub(super) fn agents_page(&mut self, ui: &mut Ui) {
        let rect = ui.available_rect_before_wrap();
        if self.hub.agents.chat.split.is_some() {
            let gap = 16.0;
            let half = (rect.width() - gap) / 2.0;
            let left = egui::Rect::from_min_size(rect.min, egui::vec2(half, rect.height()));
            let right =
                egui::Rect::from_min_max(egui::pos2(left.right() + gap, rect.top()), rect.max);
            ui.scope_builder(egui::UiBuilder::new().max_rect(left), |ui| {
                self.conversation(ui, Slot::Page)
            });
            ui.scope_builder(egui::UiBuilder::new().max_rect(right), |ui| {
                self.conversation(ui, Slot::Split)
            });
        } else {
            self.conversation(ui, Slot::Page);
        }
    }

    /// Detached conversations float over the editor like Clean Code files.
    pub(super) fn floating_agents(&mut self, ctx: &egui::Context) {
        if self.hub.agents.runtime.is_none() {
            return;
        }
        self.restore_chat_layout();
        let ids = self.hub.agents.chat.detached.clone();
        for id in ids {
            let title = self
                .hub
                .agents
                .runtime
                .as_ref()
                .and_then(|r| r.sessions.iter().find(|s| s.meta.id.as_str() == id))
                .map(|s| s.meta.title.clone());
            let Some(title) = title else {
                self.hub.agents.chat.detached.retain(|d| d != &id);
                continue;
            };
            let mut open = true;
            egui::Window::new(title)
                .id(Id::new(("agent-window", &id)))
                .default_size(egui::vec2(460.0, 560.0))
                .min_width(320.0)
                .min_height(320.0)
                .resizable(true)
                .collapsible(false)
                .open(&mut open)
                .frame(
                    Frame::new()
                        .fill(colors().surface)
                        .stroke(egui::Stroke::new(1.0_f32, colors().border))
                        .corner_radius(12)
                        .inner_margin(Margin::same(12))
                        .shadow(egui::epaint::Shadow {
                            offset: [0, 10],
                            blur: 28,
                            spread: 0,
                            color: egui::Color32::from_black_alpha(110),
                        }),
                )
                .show(ctx, |ui| {
                    let height = ui.available_height().max(300.0);
                    ui.set_min_height(height);
                    self.conversation(ui, Slot::Window(id.clone()));
                });
            if !open {
                self.hub.agents.chat.detached.retain(|d| d != &id);
            }
        }
        self.remember_chat_layout();
    }

    /// Start a real Agent Hub conversation from the Session Home composer and
    /// immediately select it in the right-hand dock so the user sees the reply.
    pub(super) fn start_chat_from_home(&mut self, ctx: &egui::Context, text: &str) -> Option<String> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }
        if !self.ensure_runtime(ctx) {
            self.notice = Some((true, tr("Agents are unavailable.").to_owned()));
            return None;
        }
        let Some(root) = self.root.clone() else {
            self.notice = Some((true, tr("Open a project folder").to_owned()));
            return None;
        };
        let active = self.active_file();
        let (runtime, chat) = self.hub.agents.runtime_and_chat();
        if chat.target == Target::Orchestrator {
            let goal = trimmed.to_owned();
            self.start_objective(&goal);
            self.preferences.assistant_open = true;
            self.hub_open = false;
            return None;
        }
        let accounts = chat_accounts(runtime);
        if accounts.is_empty() {
            chat.add_ai = Some(AddAi::default());
            self.notice = Some((true, tr("Connect an AI to start").to_owned()));
            return None;
        }
        let chosen = match &chat.target {
            Target::Account(id) => accounts.iter().find(|a| &a.id == id),
            Target::Auto | Target::Orchestrator => None,
        }
        .or_else(|| {
            accounts.iter().find(|a| {
                matches!(
                    runtime.connection.get(&a.id),
                    Some(Connection::SignedIn(_) | Connection::Key(true))
                )
            })
        })
        .unwrap_or(&accounts[0])
        .clone();
        chat.target = Target::Account(chosen.id.clone());
        let model = chat
            .model
            .clone()
            .filter(|m| model_belongs_to_provider(m, &chosen.provider));
        if chat.model.is_some() && model.is_none() {
            chat.model = None;
        }
        let title: String = trimmed
            .lines()
            .next()
            .unwrap_or_default()
            .chars()
            .take(48)
            .collect();
        let created = runtime.create_session(NewSession {
            title,
            account: chosen.id.clone(),
            model,
            root,
            worktree: chat.worktree,
            preset: PRESETS[chat.preset].0,
            allow_safe_commands: true,
            share_context: true,
        });
        match created {
            Ok(index) => {
                let prompt = with_file(trimmed, chat.attach.then_some(active.as_deref()).flatten());
                if let Err(e) = runtime.send(index, &prompt) {
                    runtime.sessions[index].draft = trimmed.to_owned();
                    runtime.notice = Some((false, e));
                }
                let id = runtime.sessions[index].meta.id.as_str().to_owned();
                chat.dock = Some(id.clone());
                self.preferences.assistant_open = true;
                self.hub_open = false;
                self.remember_chat_layout();
                Some(id)
            }
            Err(e) => {
                runtime.notice = Some((false, e.clone()));
                self.notice = Some((true, e));
                None
            }
        }
    }

    fn conversation(&mut self, ui: &mut Ui, slot: Slot) {
        let ctx = ui.ctx().clone();
        if !self.ensure_runtime(&ctx) {
            ui.label(secondary(tr("Agents are unavailable.")));
            ui.colored_label(
                colors().error,
                self.hub.agents.open_error_text().unwrap_or_default(),
            );
            return;
        }
        self.restore_chat_layout();
        let active = self.active_file();
        let root = self.root.clone();
        let mut actions: Vec<Action> = Vec::new();
        {
            let (runtime, chat) = self.hub.agents.runtime_and_chat();
            let current = chat.slot(&slot).and_then(|id| {
                runtime
                    .sessions
                    .iter()
                    .position(|s| s.meta.id.as_str() == id)
            });
            if chat.slot(&slot).is_some() && current.is_none() && !matches!(slot, Slot::Window(_)) {
                chat.set(&slot, None);
            }
            header(ui, runtime, chat, &slot, current, &mut actions);
            match current {
                Some(index) => {
                    session_body(ui, runtime, chat, index, active.as_deref(), &mut actions)
                }
                None => new_conversation(
                    ui,
                    runtime,
                    chat,
                    &slot,
                    root,
                    active.as_deref(),
                    &mut actions,
                ),
            }
        }
        for action in actions {
            match action {
                Action::Select(slot, id) => self.hub.agents.chat.set(&slot, id),
                Action::Detach(id) => {
                    let chat = &mut self.hub.agents.chat;
                    if !chat.detached.contains(&id) {
                        chat.detached.push(id.clone());
                    }
                    if slot == Slot::Dock && chat.dock.as_deref() == Some(id.as_str()) {
                        chat.dock = None;
                    }
                }
                Action::Open(path) => self.open_file(path),
                Action::OpenGuided(reply_text) => {
                    let flow = self.preferences.guided.flow_order;
                    if let Some(plan) = super::guided::extract_plan_from_text(&reply_text, flow) {
                        let first = plan.steps.first().map(|s| s.path.clone());
                        self.guided.load_plan(plan, self.root.as_deref());
                        if let Some(path) = first {
                            self.open_guided_file(&path);
                        }
                    }
                }
                Action::Objective(goal) => self.start_objective(&goal),
                Action::OpenOrchestrator => {
                    self.hub_open = true;
                    self.hub.focus_agents("orchestrator");
                }
                Action::AddAi => self.hub.agents.chat.add_ai = Some(AddAi::default()),
            }
        }
        self.remember_chat_layout();
    }
}

fn header(
    ui: &mut Ui,
    runtime: &mut Runtime,
    chat: &mut ChatState,
    slot: &Slot,
    current: Option<usize>,
    actions: &mut Vec<Action>,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add(egui::Button::new(
                    RichText::new(tr("+ Add AI")).size(SMALL).strong(),
                ))
                .on_hover_text(tr(
                    "Connect Claude Code, Codex or Gemini with an official method",
                ))
                .clicked()
            {
                actions.push(Action::AddAi);
            }
            if *slot == Slot::Split
                && icon_button(ui, Icon::Close, tr("Close split"), false).clicked()
            {
                chat.split = None;
            }
            if *slot == Slot::Page
                && chat.split.is_none()
                && icon_button(
                    ui,
                    Icon::Compare,
                    tr("Split: show a second conversation"),
                    false,
                )
                .clicked()
            {
                chat.split = Some(String::new());
            }
            if let Some(index) = current {
                let id = runtime.sessions[index].meta.id.as_str().to_owned();
                if !matches!(slot, Slot::Window(_))
                    && icon_button(ui, Icon::Detach, tr("Open in its own window"), false).clicked()
                {
                    actions.push(Action::Detach(id.clone()));
                }
                if !runtime.sessions[index].running()
                    && icon_button(
                        ui,
                        Icon::Trash,
                        tr("Close this conversation (its worktree is removed only if unchanged)"),
                        false,
                    )
                    .clicked()
                {
                    runtime.notice = Some(match runtime.close(index) {
                        Ok(note) => (true, note),
                        Err(e) => (false, e),
                    });
                    actions.push(Action::Select(slot.clone(), None));
                    return;
                }
            }
            if matches!(slot, Slot::Window(_)) {
                return;
            }
            let selected = current.map_or_else(
                || tr("New conversation").to_owned(),
                |i| runtime.sessions[i].meta.title.clone(),
            );
            egui::ComboBox::from_id_salt(("agent-switch", format!("{slot:?}")))
                .width(ui.available_width().max(80.0))
                .truncate()
                .selected_text(RichText::new(selected).strong())
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(current.is_none(), tr("+ New conversation"))
                        .clicked()
                    {
                        actions.push(Action::Select(slot.clone(), None));
                    }
                    for (i, session) in runtime.sessions.iter().enumerate().rev() {
                        let label = format!(
                            "{}  ·  {}  ·  {}",
                            session.meta.title,
                            provider_name(&session.meta.provider),
                            tr(session.meta.status.label())
                        );
                        if ui.selectable_label(current == Some(i), label).clicked() {
                            actions.push(Action::Select(
                                slot.clone(),
                                Some(session.meta.id.as_str().to_owned()),
                            ));
                        }
                    }
                });
        });
    });
    ui.add_space(6.0);
}

fn identity_line(ui: &mut Ui, runtime: &mut Runtime, session_index: usize) {
    let meta = runtime.sessions[session_index].meta.clone();
    ensure_connection(runtime, &meta.account);
    let account = runtime
        .profiles
        .accounts()
        .iter()
        .find(|a| a.id == meta.account)
        .cloned();
    Frame::new()
        .fill(surface())
        .corner_radius(10)
        .inner_margin(Margin::symmetric(10, 7))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                dot(ui, status_color(meta.status));
                ui.label(
                    RichText::new(provider_name(&meta.provider))
                        .size(SMALL)
                        .strong()
                        .color(colors().text),
                );
                separator_dot(ui);
                match &account {
                    Some(a) => {
                        ui.label(
                            RichText::new(&a.label)
                                .size(SMALL)
                                .color(colors().secondary),
                        )
                        .on_hover_text(format!(
                            "{} · {}",
                            a.id,
                            tr(method_name(a.method))
                        ));
                    }
                    None => fact(ui, format!("{} (removed)", meta.account)),
                }
                separator_dot(ui);
                fact(
                    ui,
                    meta.model
                        .clone()
                        .unwrap_or_else(|| tr("default model").into()),
                );
                if let Some(account) = &account {
                    let (color, text) = connection_view(runtime, account);
                    separator_dot(ui);
                    let response = ui.label(RichText::new(tr(&text)).size(SMALL).color(color));
                    if let Some(
                        Connection::SignedIn(t) | Connection::SignedOut(t) | Connection::Unknown(t),
                    ) = runtime.connection.get(&account.id)
                    {
                        response.on_hover_text(t.clone());
                    }
                }
                separator_dot(ui);
                fact(
                    ui,
                    format!(
                        "{}{}",
                        tr(preset_name(meta.preset)),
                        if meta.worktree_branch.is_some() {
                            " · worktree"
                        } else {
                            ""
                        }
                    ),
                );
            });
        });
}

fn session_body(
    ui: &mut Ui,
    runtime: &mut Runtime,
    chat: &mut ChatState,
    index: usize,
    active: Option<&str>,
    actions: &mut Vec<Action>,
) {
    identity_line(ui, runtime, index);
    ui.add_space(6.0);
    let composer_height = 112.0;
    let height = (ui.available_height() - composer_height).max(120.0);
    let id = runtime.sessions[index].meta.id.as_str().to_owned();
    let audit = runtime.audit(&runtime.sessions[index].meta.id);
    ScrollArea::vertical()
        .id_salt(("agent-timeline", &id))
        .max_height(height)
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            timeline(ui, &runtime.sessions[index], actions);
            changes_block(ui, chat, &runtime.sessions[index], actions);
            technical_log(ui, &runtime.sessions[index], &audit);
        });
    ui.add_space(6.0);
    let session = &mut runtime.sessions[index];
    let running = session.running();
    let stopping = session.stopping();
    let label = format!(
        "{} · {}",
        provider_name(&session.meta.provider),
        session
            .meta
            .model
            .clone()
            .unwrap_or_else(|| tr("default model").into())
    );
    let hint = if stopping {
        tr("Stopping… waiting for the provider to exit")
    } else if running {
        tr("Working… you can stop the turn")
    } else {
        tr("Message the agent")
    };
    let mut draft = std::mem::take(&mut session.draft);
    let result = composer(
        ui,
        ("composer", &id),
        &mut draft,
        hint,
        running,
        &mut chat.attach,
        active,
        |ui| {
            ui.label(meta_text(label));
        },
    );
    match result {
        Composed::Send(text) => {
            let prompt = with_file(&text, chat.attach.then_some(active).flatten());
            if let Err(e) = runtime.send(index, &prompt) {
                runtime.sessions[index].draft = text;
                runtime.notice = Some((false, e));
            }
        }
        Composed::Stop => {
            runtime.sessions[index].draft = draft;
            runtime.cancel(index);
        }
        Composed::Idle => runtime.sessions[index].draft = draft,
    }
    notice(ui, runtime);
}

fn notice(ui: &mut Ui, runtime: &mut Runtime) {
    if let Some((ok, text)) = runtime.notice.clone() {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(text).size(SMALL).color(if ok {
                colors().success
            } else {
                colors().warning
            }));
            if ui.small_button("✕").clicked() {
                runtime.notice = None;
            }
        });
    }
}

fn with_file(text: &str, file: Option<&str>) -> String {
    match file {
        Some(path) => format!("Active file in the editor: {path}\n\n{text}"),
        None => text.to_owned(),
    }
}

/// Messages and a compact activity trail; raw events live in the technical log.
fn timeline(ui: &mut Ui, session: &LiveSession, actions: &mut Vec<Action>) {
    let meta = &session.meta;
    let mut tools: Vec<&str> = Vec::new();
    let flush = |ui: &mut Ui, tools: &mut Vec<&str>| {
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
            let n = tools.iter().filter(|t| *t == name).count();
            names.push(if n > 1 {
                format!("{name} ×{n}")
            } else {
                (*name).to_owned()
            });
        }
        activity(ui, colors().accent, &names.join(" · "));
        tools.clear();
    };
    if meta.log.iter().all(|l| l.kind != "you") && !session.running() {
        ui.add_space(8.0);
        ui.label(secondary(tr(
            "Ask for a change, a review or an explanation. The agent works in the folder shown above.",
        )));
    }
    for line in &meta.log {
        if line.kind == "tool" {
            tools.push(&line.text);
            continue;
        }
        flush(ui, &mut tools);
        match line.kind.as_str() {
            "you" => user_message(ui, &line.text),
            "agent" => {
                agent_message(ui, provider_name(&meta.provider), &line.text);
                if line.text.contains("```covalos-guided")
                    && ui
                        .add(
                            egui::Button::new(
                                RichText::new(tr("Open in Guided Mode"))
                                    .size(SMALL)
                                    .strong()
                                    .color(colors().canvas),
                            )
                            .fill(colors().accent)
                            .corner_radius(7),
                        )
                        .clicked()
                {
                    actions.push(Action::OpenGuided(line.text.clone()));
                }
            }
            "error" => {
                Frame::new()
                    .fill(colors().error.gamma_multiply(0.12))
                    .corner_radius(8)
                    .inner_margin(Margin::symmetric(10, 7))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.add(
                            egui::Label::new(
                                RichText::new(&line.text).size(SMALL).color(colors().error),
                            )
                            .wrap()
                            .selectable(true),
                        );
                    });
                ui.add_space(4.0);
            }
            "done" => activity(ui, colors().success, &line.text),
            "cancelled" | "recovery" | "approval" => activity(ui, colors().warning, &line.text),
            "context" => activity(ui, colors().syntax_string, &line.text),
            _ => {}
        }
    }
    flush(ui, &mut tools);
    if session.running() {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            busy_indicator(ui);
            ui.label(secondary(if session.stopping() {
                tr("Stopping…")
            } else {
                tr("Working…")
            }));
        });
        if !session.streaming.is_empty() {
            ui.add(
                egui::Label::new(
                    RichText::new(&session.streaming)
                        .size(BODY)
                        .color(colors().secondary),
                )
                .wrap(),
            );
        }
    }
}

fn activity(ui: &mut Ui, color: egui::Color32, text: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.add_space(2.0);
        dot(ui, color.gamma_multiply(0.8));
        ui.add(egui::Label::new(meta_text(text.chars().take(240).collect::<String>())).wrap());
    });
}

fn user_message(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    let width = ui.available_width();
    ui.with_layout(Layout::top_down(Align::Max), |ui| {
        Frame::new()
            .fill(colors().accent_bg)
            .corner_radius(10)
            .inner_margin(Margin::symmetric(11, 8))
            .show(ui, |ui| {
                ui.set_max_width(width * 0.86);
                ui.add(
                    egui::Label::new(RichText::new(text).size(BODY + 0.5).color(colors().text))
                        .wrap()
                        .selectable(true),
                );
            });
    });
    ui.add_space(4.0);
}

fn agent_message(ui: &mut Ui, provider: &str, text: &str) {
    ui.add_space(4.0);
    Frame::new()
        .fill(surface())
        .corner_radius(10)
        .inner_margin(Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(meta_text(provider));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add(egui::Button::new(meta_text(tr("Copy"))).frame(false))
                        .clicked()
                    {
                        ui.ctx().copy_text(text.to_owned());
                    }
                });
            });
            ui.spacing_mut().item_spacing.y = 4.0;
            super::hub::markdown(ui, text);
        });
    ui.add_space(4.0);
}

fn changes_block(
    ui: &mut Ui,
    chat: &mut ChatState,
    session: &LiveSession,
    actions: &mut Vec<Action>,
) {
    let id = session.meta.id.as_str().to_owned();
    let workdir = session.meta.workdir.clone();
    ui.add_space(6.0);
    egui::CollapsingHeader::new(meta_text(tr("Files changed in this folder")))
        .id_salt(("agent-changes", &id))
        .default_open(false)
        .show(ui, |ui| {
            if ui.small_button(tr("Refresh")).clicked()
                || !chat.changes.iter().any(|(k, _)| k == &id)
            {
                let result = git(
                    &workdir,
                    &["status", "--porcelain=v1", "--untracked-files=all"],
                );
                chat.changes.retain(|(k, _)| k != &id);
                chat.changes.push((id.clone(), result));
            }
            let Some((_, result)) = chat.changes.iter().find(|(k, _)| k == &id) else {
                return;
            };
            let mut diff_request = None;
            match result {
                Err(e) => {
                    ui.label(meta_text(e.clone()));
                }
                Ok(text) if text.trim().is_empty() => {
                    ui.label(meta_text(tr("No changes in this folder.")));
                }
                Ok(text) => {
                    for line in text.lines().take(60) {
                        let (code, path) = line.split_at(line.len().min(3));
                        let path = path.trim().trim_matches('"').to_owned();
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(code.trim())
                                    .monospace()
                                    .size(SMALL)
                                    .color(colors().warning),
                            );
                            if ui
                                .add(
                                    egui::Button::new(RichText::new(&path).size(SMALL))
                                        .frame(false),
                                )
                                .on_hover_text(tr("Open in the editor"))
                                .clicked()
                            {
                                actions.push(Action::Open(workdir.join(&path)));
                            }
                            if ui.small_button(tr("Diff")).clicked() {
                                diff_request = Some(path.clone());
                            }
                        });
                    }
                }
            }
            if let Some(path) = diff_request {
                let diff =
                    git(&workdir, &["diff", "--no-ext-diff", "--", &path]).unwrap_or_else(|e| e);
                chat.diff = Some((id.clone(), path, diff));
            }
            if let Some((owner, path, diff)) = &chat.diff
                && owner == &id
            {
                ui.add_space(4.0);
                ui.label(meta_text(path.clone()));
                diff_view(ui, diff);
            }
        });
}

/// Unified diff with added and removed lines tinted.
pub(super) fn diff_view(ui: &mut Ui, diff: &str) {
    Frame::new()
        .fill(colors().inset)
        .corner_radius(8)
        .inner_margin(Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if diff.trim().is_empty() {
                ui.label(meta_text(tr("No textual diff (new or binary file).")));
            }
            for line in diff.lines().take(400) {
                let color = if line.starts_with('+') && !line.starts_with("+++") {
                    colors().success
                } else if line.starts_with('-') && !line.starts_with("---") {
                    colors().error
                } else if line.starts_with("@@") {
                    colors().accent
                } else {
                    colors().secondary
                };
                ui.label(RichText::new(line).monospace().size(11.5).color(color));
            }
        });
}

fn git(dir: &std::path::Path, args: &[&str]) -> Result<String, String> {
    let mut command = std::process::Command::new("git");
    crate::process::configure(&mut command);
    let output = command
        .current_dir(dir)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args([
            "--no-pager",
            "-c",
            "color.ui=false",
            "-c",
            "core.quotePath=false",
        ])
        .args(args)
        .output()
        .map_err(|e| format!("git is not available: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

fn technical_log(ui: &mut Ui, session: &LiveSession, audit: &[serde_json::Value]) {
    let meta = &session.meta;
    egui::CollapsingHeader::new(meta_text(format!(
        "{} · {} {}",
        tr("Technical log"),
        meta.log.len(),
        tr("events")
    )))
    .id_salt(("agent-tech", meta.id.as_str()))
    .default_open(false)
    .show(ui, |ui| {
        let base = meta.log.first().map_or(0, |l| l.at_ms);
        ui.label(meta_text(format!(
            "{} turns · {} in · {} out{}",
            meta.turns,
            meta.input_tokens,
            meta.output_tokens,
            if meta.cost_micros > 0 {
                format!(" · US$ {:.4} reported", meta.cost_micros as f64 / 1e6)
            } else {
                String::new()
            }
        )));
        ui.label(meta_text(
            crate::agents_runtime::plain(&meta.workdir)
                .display()
                .to_string(),
        ));
        if let Some(branch) = &meta.worktree_branch {
            ui.label(meta_text(format!("branch {branch}")));
        }
        if let Some(p) = &meta.provider_session {
            ui.label(meta_text(format!("provider session {p}")));
        }
        ui.add_space(4.0);
        for line in &meta.log {
            let s = line.at_ms.saturating_sub(base) / 1000;
            ui.label(
                RichText::new(format!(
                    "{:02}:{:02}  {:<9} {}",
                    s / 60,
                    s % 60,
                    line.kind,
                    line.text.chars().take(300).collect::<String>()
                ))
                .monospace()
                .size(11.0)
                .color(colors().secondary),
            );
        }
        for entry in audit {
            let allowed = entry["allow"] == true;
            ui.label(
                RichText::new(format!(
                    "gate {} {} {} — {}",
                    if allowed { "ALLOW" } else { "DENY " },
                    entry["tool"].as_str().unwrap_or_default(),
                    entry["target"].as_str().unwrap_or_default(),
                    entry["reason"].as_str().unwrap_or_default()
                ))
                .monospace()
                .size(11.0)
                .color(if allowed {
                    colors().success
                } else {
                    colors().error
                }),
            );
        }
    });
}

enum Composed {
    Send(String),
    Stop,
    Idle,
}

/// The universal composer: message, attached file, a target label on the left
/// of the action row, and send or stop on the right.
#[allow(clippy::too_many_arguments)]
fn composer(
    ui: &mut Ui,
    salt: impl std::hash::Hash,
    draft: &mut String,
    hint: &str,
    running: bool,
    attach: &mut bool,
    active: Option<&str>,
    target: impl FnOnce(&mut Ui),
) -> Composed {
    let id = ui.make_persistent_id(salt);
    let mut send = false;
    if !running
        && ui.memory(|m| m.has_focus(id))
        && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter))
    {
        send = true;
    }
    let mut result = Composed::Idle;
    Frame::new()
        .fill(surface())
        .stroke(egui::Stroke::new(1.0_f32, colors().control_border))
        .corner_radius(12)
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(
                TextEdit::multiline(draft)
                    .id(id)
                    .return_key(egui::KeyboardShortcut::new(
                        egui::Modifiers::SHIFT,
                        Key::Enter,
                    ))
                    .frame(false)
                    .hint_text(hint)
                    .font(egui::FontId::proportional(BODY + 1.0))
                    .desired_rows(2)
                    .desired_width(f32::INFINITY),
            );
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                target(ui);
                if let Some(file) = active {
                    let name = file.rsplit('/').next().unwrap_or(file);
                    let text = if *attach {
                        format!("+ {name}")
                    } else {
                        format!("– {name}")
                    };
                    if ui
                        .add(egui::Button::new(meta_text(text)).frame(false))
                        .on_hover_text(tr("Tell the agent which file is open in the editor"))
                        .clicked()
                    {
                        *attach = !*attach;
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if running {
                        if ui.add(egui::Button::new(tr("Stop"))).clicked() {
                            result = Composed::Stop;
                        }
                    } else if ui
                        .add_enabled(!draft.trim().is_empty(), primary(tr("Send")))
                        .clicked()
                    {
                        send = true;
                    }
                });
            });
        });
    if send && !running && !draft.trim().is_empty() {
        result = Composed::Send(std::mem::take(draft));
    }
    result
}

fn new_conversation(
    ui: &mut Ui,
    runtime: &mut Runtime,
    chat: &mut ChatState,
    slot: &Slot,
    root: Option<PathBuf>,
    active: Option<&str>,
    actions: &mut Vec<Action>,
) {
    let accounts = chat_accounts(runtime);
    for a in &accounts {
        ensure_connection(runtime, &a.id);
    }
    if let Target::Account(id) = &chat.target
        && !accounts.iter().any(|a| &a.id == id)
    {
        chat.target = Target::Auto;
    }
    if chat.target == Target::Auto
        && let Some(first) = accounts.first()
    {
        chat.target = Target::Account(first.id.clone());
        chat.model = first.default_model.clone();
    }
    let account = match &chat.target {
        Target::Account(id) => accounts.iter().find(|a| &a.id == id).cloned(),
        _ => None,
    };
    let ready = root.is_some() && !accounts.is_empty();
    let composer_height = if ready { 112.0 } else { 0.0 };
    let height = (ui.available_height() - composer_height).max(140.0);
    ScrollArea::vertical()
        .id_salt(("agent-new", format!("{slot:?}")))
        .max_height(height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add_space(10.0);
            if root.is_none() {
                ui.label(
                    RichText::new(tr("Open a project folder"))
                        .size(LEAD)
                        .strong(),
                );
                ui.label(secondary(tr(
                    "Agents work inside a project. Open a folder, then start a conversation here.",
                )));
                return;
            }
            if accounts.is_empty() {
                ui.label(
                    RichText::new(tr("Connect an AI to start"))
                        .size(LEAD)
                        .strong(),
                );
                ui.label(secondary(tr(
                    "Covalos drives Claude Code and Codex through their official CLIs. Your \
                     sign-in stays with each CLI.",
                )));
                ui.add_space(8.0);
                if ui.add(primary(tr("+ Add AI"))).clicked() {
                    actions.push(Action::AddAi);
                }
                return;
            }
            ui.label(RichText::new(tr("New conversation")).size(LEAD).strong());
            ui.add_space(8.0);
            target_picker(ui, runtime, chat, &accounts, account.as_ref());
            if chat.target == Target::Orchestrator {
                ui.add_space(8.0);
                plan_card(ui, runtime, actions);
            }
        });
    if !ready {
        notice(ui, runtime);
        return;
    }
    let label = match (&chat.target, &account) {
        (Target::Orchestrator, _) => tr("Covalos Orchestrator").to_owned(),
        (_, Some(a)) => {
            let model_display = chat
                .model
                .as_deref()
                .filter(|m| model_belongs_to_provider(m, &a.provider))
                .map(|m| curated_model_label(m).unwrap_or(m).to_owned())
                .unwrap_or_else(|| tr("default model").into());
            format!("{} · {}", provider_name(&a.provider), model_display)
        }
        _ => String::new(),
    };
    let hint = if chat.target == Target::Orchestrator {
        tr("Describe an objective; the orchestrator plans and assigns it")
    } else {
        tr("Message the agent")
    };
    let result = composer(
        ui,
        ("composer-new", format!("{slot:?}")),
        &mut chat.draft,
        hint,
        false,
        &mut chat.attach,
        active,
        |ui| {
            ui.label(meta_text(label));
        },
    );
    notice(ui, runtime);
    let Composed::Send(text) = result else { return };
    if chat.target == Target::Orchestrator {
        actions.push(Action::Objective(text));
        return;
    }
    let Some(account) = account else { return };
    let title: String = text
        .lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(48)
        .collect();
    let model = chat
        .model
        .clone()
        .filter(|m| model_belongs_to_provider(m, &account.provider));
    let created = runtime.create_session(NewSession {
        title,
        account: account.id.clone(),
        model,
        root: root.clone().expect("checked"),
        worktree: chat.worktree,
        preset: PRESETS[chat.preset].0,
        allow_safe_commands: true,
        share_context: true,
    });
    match created {
        Ok(index) => {
            let prompt = with_file(&text, chat.attach.then_some(active).flatten());
            if let Err(e) = runtime.send(index, &prompt) {
                runtime.sessions[index].draft = text;
                runtime.notice = Some((false, e));
            }
            let id = runtime.sessions[index].meta.id.as_str().to_owned();
            actions.push(Action::Select(slot.clone(), Some(id)));
        }
        Err(e) => {
            chat.draft = text;
            runtime.notice = Some((false, e));
        }
    }
}

/// Unified selector: an account (provider, sign-in, model, permissions) or the
/// Covalos Orchestrator.
fn target_picker(
    ui: &mut Ui,
    runtime: &mut Runtime,
    chat: &mut ChatState,
    accounts: &[Account],
    account: Option<&Account>,
) {
    ui.label(meta_text(tr("Talk to")));
    for a in accounts {
        let (color, state) = connection_view(runtime, a);
        let selected = chat.target == Target::Account(a.id.clone());
        let response = choice_row(
            ui,
            selected,
            provider_name(&a.provider),
            tr(method_name(a.method)),
            Some((color, state)),
        );
        if response.clicked() {
            chat.target = Target::Account(a.id.clone());
            chat.model = a.default_model.clone();
        }
    }
    if choice_row(
        ui,
        chat.target == Target::Orchestrator,
        "Covalos Orchestrator",
        tr("Plans an objective and assigns roles to your accounts"),
        None,
    )
    .clicked()
    {
        chat.target = Target::Orchestrator;
    }
    let Some(account) = account else { return };
    if chat.target == Target::Orchestrator {
        return;
    }
    if let Some(Connection::SignedOut(_)) = runtime.connection.get(&account.id) {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(tr("This account is signed out."))
                    .size(SMALL)
                    .color(colors().warning),
            );
            if account.method == ConnectionMethod::OfficialLogin
                && ui.small_button(tr("Sign in")).clicked()
            {
                runtime.notice = Some(match runtime.sign_in(&account.id) {
                    Ok(()) => (true, tr("Finish signing in in the console window.").into()),
                    Err(e) => (false, e),
                });
            }
            if ui.small_button(tr("Check again")).clicked() {
                runtime.refresh_connection(&account.id);
            }
        });
    }
    ui.add_space(6.0);
    egui::CollapsingHeader::new(meta_text(tr("Options")))
        .id_salt(("agent-new-options", &account.id))
        .default_open(false)
        .show(ui, |ui| {
            ui.label(meta_text(tr("Model")));
            let models = model_options(runtime, account);
            ui.horizontal_wrapped(|ui| {
                if ui
                    .selectable_label(chat.model.is_none(), tr("Default"))
                    .clicked()
                {
                    chat.model = None;
                }
                for m in &models {
                    if ui
                        .selectable_label(chat.model.as_deref() == Some(m.as_str()), m)
                        .clicked()
                    {
                        chat.model = Some(m.clone());
                    }
                }
                let custom = ui.add(
                    TextEdit::singleline(&mut chat.custom_model)
                        .hint_text(tr("other model id"))
                        .desired_width(120.0),
                );
                if custom.lost_focus() && !chat.custom_model.trim().is_empty() {
                    chat.model = Some(chat.custom_model.trim().to_owned());
                }
            });
            ui.add_space(6.0);
            ui.label(meta_text(tr("Permissions")));
            ui.horizontal_wrapped(|ui| {
                for (i, (_, label)) in PRESETS.iter().enumerate() {
                    ui.selectable_value(&mut chat.preset, i, tr(label));
                }
            });
            checkbox(ui, &mut chat.worktree, tr("Work in its own git worktree"));
        });
}

fn choice_row(
    ui: &mut Ui,
    selected: bool,
    title: &str,
    detail: &str,
    state: Option<(egui::Color32, String)>,
) -> egui::Response {
    let response = Frame::new()
        .fill(if selected {
            colors().accent_bg
        } else {
            surface()
        })
        .stroke(egui::Stroke::new(
            1.0_f32,
            if selected {
                colors().accent.gamma_multiply(0.6)
            } else {
                colors().border
            },
        ))
        .corner_radius(9)
        .inner_margin(Margin::symmetric(10, 7))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.label(
                        RichText::new(tr(title))
                            .size(BODY)
                            .strong()
                            .color(colors().text),
                    );
                    ui.add(egui::Label::new(meta_text(detail)).truncate());
                });
                if let Some((color, text)) = state {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(tr(&text)).size(SMALL).color(color));
                        dot(ui, color);
                    });
                }
            });
        })
        .response
        .interact(egui::Sense::click());
    ui.add_space(4.0);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Compact live view of the orchestrator's current plan.
fn plan_card(ui: &mut Ui, runtime: &Runtime, actions: &mut Vec<Action>) {
    let Some(run) = runtime.plan.as_ref() else {
        ui.label(meta_text(tr(
            "Roles, accounts and budgets are set in the Orchestrator page. Your message becomes the objective.",
        )));
        if ui.small_button(tr("Open Orchestrator")).clicked() {
            actions.push(Action::OpenOrchestrator);
        }
        return;
    };
    let plan = run.engine.plan();
    Frame::new()
        .fill(surface())
        .corner_radius(10)
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(&plan.goal).size(BODY).strong());
            ui.label(meta_text(format!("{:?}", plan.state)));
            for task in plan.tasks() {
                ui.horizontal(|ui| {
                    dot(ui, super::agent_hub::task_color(&task.status));
                    ui.add(
                        egui::Label::new(meta_text(format!(
                            "{} · {}",
                            task.spec.role, task.spec.title
                        )))
                        .truncate(),
                    );
                });
            }
            if ui.small_button(tr("Open Orchestrator")).clicked() {
                actions.push(Action::OpenOrchestrator);
            }
        });
}

// ---------------------------------------------------------------- add AI

const ADD_PROVIDERS: [(&str, &str); 4] = [
    (
        "Claude Code",
        "Anthropic's CLI. Official sign-in or an Anthropic API key.",
    ),
    (
        "Codex",
        "OpenAI's CLI. ChatGPT sign-in or an OpenAI API key.",
    ),
    (
        "Gemini CLI",
        "Google's CLI. Accounts can be saved; chats need its verified adapter.",
    ),
    (
        "Other",
        "Editors and agents without an official headless interface.",
    ),
];

/// Methods each provider really supports through its own CLI.
fn add_methods(
    provider: usize,
    isolated: bool,
) -> Vec<(ConnectionMethod, &'static str, &'static str)> {
    let mut out = vec![(
        ConnectionMethod::SystemCli,
        "Use my existing CLI sign-in",
        "Runs the CLI exactly as you already signed in on this PC.",
    )];
    if isolated {
        out.push((
            ConnectionMethod::OfficialLogin,
            "New isolated sign-in",
            "Separate config folder and the provider's own login. Several accounts side by side.",
        ));
    }
    match provider {
        0 => out.push((
            ConnectionMethod::ApiKey,
            "Anthropic API key",
            "Kept in Windows Credential Manager; passed only as ANTHROPIC_API_KEY.",
        )),
        1 => out.push((
            ConnectionMethod::OfficialLogin,
            "OpenAI API key",
            "Runs codex login --with-api-key in an isolated folder. Covalos does not keep the key.",
        )),
        2 => out.push((
            ConnectionMethod::ApiKey,
            "Gemini API key",
            "Kept in Windows Credential Manager; passed only as GEMINI_API_KEY.",
        )),
        _ => {}
    }
    out
}

impl Editor {
    pub(super) fn add_ai_modal(&mut self, ctx: &egui::Context) {
        let Some(mut form) = self.hub.agents.chat.add_ai.take() else {
            return;
        };
        if !self.ensure_runtime(ctx) {
            return;
        }
        let runtime = self.hub.agents.runtime.as_mut().expect("runtime");
        let mut keep = true;
        let modal = egui::Modal::new(Id::new("add-ai"))
            .frame(modal_frame())
            .show(ctx, |ui| {
                ui.set_width(480.0);
                ui.label(RichText::new(tr("Add AI")).size(HEADING).strong());
                ui.label(muted(tr(
                    "Connect with your subscription CLI or an API key.",
                )));
                ui.add_space(10.0);
                if let Some(id) = form.created.clone() {
                    add_ai_done(ui, runtime, &mut form, &id, &mut keep);
                    return;
                }
                for (i, (name, detail)) in ADD_PROVIDERS.iter().take(3).enumerate() {
                    let kind = match i {
                        0 => Some(ProviderKind::ClaudeCode),
                        1 => Some(ProviderKind::Codex),
                        2 => Some(ProviderKind::Gemini),
                        _ => None,
                    };
                    let state = kind.as_ref().and_then(|k| runtime.detections.get(k)).map(|d| {
                        if !d.done {
                            (colors().muted, tr("Detecting…").to_owned())
                        } else if d.program.is_some() {
                            (colors().success, tr("Installed").to_owned())
                        } else {
                            (colors().muted, tr("Not installed").to_owned())
                        }
                    });
                    if choice_row(ui, form.provider == i, name, tr(detail), state).clicked() {
                        form.provider = i;
                        form.method = 0;
                    }
                }
                ui.add_space(8.0);
                if form.provider == 3 {
                    for tool in &runtime.external {
                        ui.label(
                            RichText::new(format!(
                                "{} — {}",
                                tool.name,
                                if tool.path.is_some() {
                                    tr("detected")
                                } else {
                                    tr("not found")
                                }
                            ))
                            .size(BODY)
                            .strong(),
                        );
                        ui.label(meta_text(tool.note));
                    }
                    ui.label(meta_text(tr(
                        "These cannot be driven by Covalos: they expose no official headless \
                         interface, and reusing their sign-in would mean taking another app's tokens.",
                    )));
                    ui.add_space(8.0);
                    if ui.button(tr("Close")).clicked() {
                        keep = false;
                    }
                    return;
                }
                let kind = [
                    ProviderKind::ClaudeCode,
                    ProviderKind::Codex,
                    ProviderKind::Gemini,
                ][form.provider]
                    .clone();
                let installed = runtime
                    .detections
                    .get(&kind)
                    .is_some_and(|d| d.program.is_some());
                if let Some(rx) = &form.install {
                    match rx.try_recv() {
                        Ok(Ok(())) => {
                            form.install = None;
                            form.error = None;
                            runtime.redetect();
                        }
                        Ok(Err(err)) => {
                            form.install = None;
                            form.error = Some(err);
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            form.install = None;
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => {}
                    }
                }
                if !installed {
                    ui.label(RichText::new(tr("Not installed on this PC")).color(colors().warning));
                    ui.label(meta_text(match form.provider {
                        0 => "Install: npm install -g @anthropic-ai/claude-code",
                        1 => "Install the Codex app or: npm install -g @openai/codex",
                        _ => "Install: npm install -g @google/gemini-cli",
                    }));
                    ui.horizontal(|ui| {
                        if form.install.is_some() {
                            busy_indicator(ui);
                            ui.label(muted(tr("Installing CLI…")));
                        } else if let Some(pkg) = cli_npm_package(form.provider)
                            && ui.add(primary(tr("Install CLI"))).clicked()
                        {
                            form.error = None;
                            form.install = Some(spawn_cli_install(pkg, ctx.clone()));
                        }
                        if ui.small_button(tr("Detect again")).clicked() {
                            runtime.redetect();
                        }
                    });
                    ui.add_space(6.0);
                }
                let isolated = runtime
                    .registry
                    .get(&kind)
                    .is_ok_and(|s| s.supports_isolated_login());
                let methods: Vec<_> = add_methods(form.provider, isolated)
                    .into_iter()
                    .filter(|(m, _, _)| installed || *m == ConnectionMethod::ApiKey)
                    .collect();
                if methods.is_empty() {
                    return;
                }
                form.method = form.method.min(methods.len() - 1);
                ui.label(meta_text(tr("How to connect")));
                for (i, (_, title, detail)) in methods.iter().enumerate() {
                    if choice_row(ui, form.method == i, title, tr(detail), None).clicked() {
                        form.method = i;
                    }
                }
                let (method, title, _) = methods[form.method];
                let wants_key = title.contains("API key");
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add(
                        TextEdit::singleline(&mut form.label)
                            .hint_text(tr("Name (e.g. Work)"))
                            .desired_width(160.0),
                    );
                    ui.add(
                        TextEdit::singleline(&mut form.model)
                            .hint_text(tr("Default model (optional)"))
                            .desired_width(180.0),
                    );
                });
                if wants_key {
                    ui.add(
                        TextEdit::singleline(&mut form.key)
                            .password(true)
                            .hint_text(tr("Paste the API key"))
                            .desired_width(f32::INFINITY),
                    );
                }
                if let Some(error) = &form.error {
                    ui.label(RichText::new(error).size(SMALL).color(colors().error));
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let ready = !wants_key || !form.key.trim().is_empty();
                    if ui.add_enabled(ready, primary(tr("Add"))).clicked() {
                        match create_account(runtime, &kind, method, wants_key, &form) {
                            Ok(id) => {
                                form.key.clear();
                                form.error = None;
                                runtime.refresh_connection(&id);
                                form.created = Some(id);
                            }
                            Err(e) => form.error = Some(e),
                        }
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        keep = false;
                    }
                });
            });
        if modal.should_close() {
            keep = false;
        }
        if keep {
            self.hub.agents.chat.add_ai = Some(form);
        }
    }
}

fn create_account(
    runtime: &mut Runtime,
    kind: &ProviderKind,
    method: ConnectionMethod,
    wants_key: bool,
    form: &AddAi,
) -> Result<AccountId, String> {
    let base = match kind {
        ProviderKind::ClaudeCode => "claude",
        ProviderKind::Codex => "codex",
        _ => "gemini",
    };
    let taken: Vec<String> = runtime
        .profiles
        .accounts()
        .iter()
        .map(|a| a.id.to_string())
        .collect();
    let id = (1..100)
        .map(|n| {
            if n == 1 {
                base.to_owned()
            } else {
                format!("{base}-{n}")
            }
        })
        .find(|c| !taken.contains(c))
        .ok_or("Too many accounts for this provider.")?;
    let id = AccountId::new(id).map_err(|e| e.to_string())?;
    let label = if form.label.trim().is_empty() {
        format!("{} · {}", provider_name(kind), method_name(method))
    } else {
        form.label.trim().to_owned()
    };
    let mut account = Account::new(id.clone(), kind.clone(), &label, method);
    let model = form.model.trim();
    if !model.is_empty() {
        account.default_model = Some(model.to_owned());
        account.declared_models = vec![model.to_owned()];
    }
    runtime.add_account(account)?;
    if wants_key {
        let key = form.key.trim();
        let saved = if method == ConnectionMethod::ApiKey {
            runtime.save_api_key(&id, key)
        } else {
            runtime.codex_api_login(&id, key)
        };
        if let Err(e) = saved {
            let _ = runtime.remove_account(&id);
            return Err(e);
        }
    }
    Ok(id)
}

fn add_ai_done(
    ui: &mut Ui,
    runtime: &mut Runtime,
    form: &mut AddAi,
    id: &AccountId,
    keep: &mut bool,
) {
    let Some(account) = runtime
        .profiles
        .accounts()
        .iter()
        .find(|a| &a.id == id)
        .cloned()
    else {
        *keep = false;
        return;
    };
    ui.label(
        RichText::new(format!("{} {}", account.label, tr("added")))
            .size(LEAD)
            .strong(),
    );
    let (color, text) = connection_view(runtime, &account);
    ui.horizontal(|ui| {
        dot(ui, color);
        ui.label(RichText::new(tr(&text)).color(color));
    });
    if account.method == ConnectionMethod::OfficialLogin
        && !matches!(runtime.connection.get(id), Some(Connection::SignedIn(_)))
    {
        ui.label(meta_text(tr(
            "Sign in with the provider's own login. It opens in a console window.",
        )));
        if ui.add(primary(tr("Sign in now"))).clicked() {
            runtime.notice = Some(match runtime.sign_in(id) {
                Ok(()) => (true, tr("Finish signing in in the console window.").into()),
                Err(e) => (false, e),
            });
        }
    }
    if account.provider == ProviderKind::Gemini {
        ui.label(meta_text(tr(
            "Saved. Gemini chats stay disabled until Covalos has a verified Gemini CLI adapter.",
        )));
    }
    if let Some((ok, text)) = &runtime.notice {
        ui.label(RichText::new(text).size(SMALL).color(if *ok {
            colors().success
        } else {
            colors().warning
        }));
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button(tr("Check sign-in")).clicked() {
            runtime.refresh_connection(id);
        }
        if ui.add(primary(tr("Done"))).clicked() {
            *keep = false;
        }
        if ui.button(tr("Add another")).clicked() {
            *form = AddAi::default();
        }
    });
}

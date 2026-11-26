//! Agent conversations: one chat surface backed by real runtime sessions, used
//! by the editor's right dock, the Agents page (with an optional split) and
//! detached windows. Everything shown comes from the session's own state.
use crate::agents_runtime::ClaudeModes;
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
    pub(super) effort: Option<String>,
    claude_modes: ClaudeModes,
    picker: ModelPicker,
    preset: usize,
    worktree: bool,
    draft: String,
    attach: bool,
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
            effort: None,
            claude_modes: ClaudeModes::default(),
            picker: ModelPicker::default(),
            preset: 1,
            worktree: false,
            draft: String::new(),
            attach: true,
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

enum Action {
    Select(Slot, Option<String>),
    Detach(String),
    Open(PathBuf),
    OpenGuided(String),
    Fork(covalos_agents::SessionId, usize),
    Objective(String),
    OpenOrchestrator,
    AddAi,
}

pub(super) fn provider_mark(ui: &mut Ui, kind: &ProviderKind, side: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(side), egui::Sense::hover());
    draw_provider_mark(ui, rect, kind);
}

fn draw_provider_mark(ui: &Ui, rect: egui::Rect, kind: &ProviderKind) {
    use crate::assistant::Provider;
    let provider = match kind {
        ProviderKind::ClaudeCode => Provider::Claude,
        ProviderKind::Codex => Provider::Codex,
        ProviderKind::Gemini => Provider::Gemini,
        ProviderKind::Antigravity => Provider::Antigravity,
        _ => {
            draw_icon_sized(
                ui.painter(),
                rect.center(),
                Icon::Terminal,
                colors().secondary,
                rect.width().min(rect.height()),
            );
            return;
        }
    };
    super::brands::mark(ui.painter(), rect, provider);
}

fn effort_label(value: &str) -> &str {
    tr(match value {
        "none" => "None",
        "minimal" => "Minimal",
        "low" => "Low",
        "medium" => "Medium",
        "high" => "High",
        "xhigh" => "Extra High",
        "max" => "Max",
        "ultra" => "Ultra",
        _ => value,
    })
}

#[derive(Default)]
struct PickerState {
    search: String,
    favorites_only: bool,
    show_more: bool,
}

type Favorite = (AccountId, String);

#[derive(Clone, Default, PartialEq)]
pub(super) struct ModelChoice {
    account: Option<AccountId>,
    model: Option<String>,
    effort: Option<String>,
    claude_modes: ClaudeModes,
}

impl ModelChoice {
    fn select(&mut self, account: &AccountId, model: &crate::agent_provider::Model) {
        if self.account.as_ref() != Some(account) || self.model.as_ref() != Some(&model.id) {
            self.claude_modes = ClaudeModes::default();
            self.account = Some(account.clone());
            self.model = Some(model.id.clone());
            self.effort = model
                .default_effort
                .clone()
                .filter(|level| model.efforts.contains(level));
        }
    }
}

#[derive(Default)]
pub(super) struct ModelPicker {
    pub(super) favorites: std::collections::BTreeSet<Favorite>,
    state: PickerState,
}

fn picker_divider(ui: &mut Ui, height: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(1.0, height), egui::Sense::hover());
    ui.painter().vline(
        rect.center().x,
        rect.y_range(),
        egui::Stroke::new(1.0_f32, colors().border),
    );
}

pub(super) fn model_picker(
    ui: &mut Ui,
    runtime: &Runtime,
    choice: &mut ModelChoice,
    picker: &mut ModelPicker,
    locked_account: Option<&AccountId>,
) -> bool {
    let accounts: Vec<_> = chat_accounts(runtime)
        .into_iter()
        .filter(|a| locked_account.is_none_or(|id| id == &a.id))
        .collect();
    let account = choice
        .account
        .as_ref()
        .and_then(|id| accounts.iter().find(|a| &a.id == id));
    if let Some(account) = account {
        let models = runtime.models(&account.id);
        let legacy_default = account.provider == ProviderKind::ClaudeCode
            && choice.model.as_deref() == Some("default");
        if (choice.model.is_none() || legacy_default)
            && let Some(model) = models
                .iter()
                .find(|m| m.is_default)
                .or_else(|| models.first())
        {
            choice.select(&account.id, model);
        }
    }
    let model = account.and_then(|a| {
        let models = runtime.models(&a.id);
        choice
            .model
            .as_ref()
            .and_then(|id| models.iter().find(|m| &m.id == id))
            .or_else(|| {
                choice
                    .model
                    .is_none()
                    .then(|| models.iter().find(|m| m.is_default))
                    .flatten()
            })
    });
    if let Some(model) = model
        && let Some(stored) = &choice.effort
        && !model.efforts.contains(stored)
    {
        choice.effort = model
            .efforts
            .iter()
            .find(|e| e.eq_ignore_ascii_case(stored))
            .cloned();
    }
    let name = model.map_or_else(
        || {
            if accounts.is_empty() {
                tr("Connect account")
            } else if choice.model.is_some() {
                tr("Model unavailable")
            } else {
                tr("Select model")
            }
        },
        |model| &model.name,
    );
    let effort = model.and_then(|m| {
        choice
            .effort
            .as_ref()
            .filter(|level| m.efforts.contains(level))
            .or(m.default_effort.as_ref())
    });
    let mut label = effort.map_or_else(
        || name.to_owned(),
        |effort| format!("{name} {}", effort_label(effort)),
    );
    if choice.claude_modes.fast {
        label.push_str(" \u{26a1}");
    }
    if choice.claude_modes.ultracode {
        label.push_str(" \u{00b7} Ultra");
    }
    let response = ui
        .horizontal(|ui| {
            let text = RichText::new(&label).size(SMALL).color(colors().text);
            let padding = if account.is_some() { 28.0 } else { 0.0 };
            ui.spacing_mut().button_padding.x += padding * 0.5;
            let response = ui.add(
                egui::Button::new(text)
                    .truncate()
                    .stroke(egui::Stroke::NONE)
                    .fill(if super::theme::active() == super::theme::Theme::OledDark {
                        egui::Color32::from_gray(26)
                    } else {
                        colors().elevated
                    })
                    .corner_radius(CONTROL_RADIUS),
            );
            if let Some(account) = account {
                let mark_rect = egui::Rect::from_center_size(
                    response.rect.left_center() + egui::vec2(12.0, 0.0),
                    egui::Vec2::splat(13.0),
                );
                draw_provider_mark(ui, mark_rect, &account.provider);
            }
            response.on_hover_text(if choice.claude_modes.fast {
                format!(
                    "{label}\n{}",
                    tr("Request Fast from Claude. Uses separately billed credits when available.")
                )
            } else {
                label
            })
        })
        .inner;
    #[cfg(test)]
    super::motion::record(ui, "Model picker", response.rect);
    if accounts.is_empty() {
        return response.clicked();
    }
    let width = (ui.ctx().content_rect().width() - 32.0).clamp(330.0, 430.0);
    let mut open_providers = false;
    egui::Popup::menu(&response)
        .align(egui::RectAlign::BOTTOM_END)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .width(width)
        .frame(
            Frame::new()
                .fill(colors().surface)
                .stroke(egui::Stroke::new(1.0_f32, colors().control_border))
                .corner_radius(ISLAND_RADIUS)
                .inner_margin(Margin::same(6)),
        )
        .show(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(5.0, 2.0);
            ui.spacing_mut().interact_size.y = 22.0;
            ui.spacing_mut().button_padding = egui::vec2(5.0, 2.0);
            let rail_width = 42.0;
            let effort_width = 99.0;
            let model_width = width - rail_width - effort_width - 22.0;
            let height = (ui.ctx().content_rect().height() * 0.30).clamp(145.0, 195.0);
            let state = &mut picker.state;
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(rail_width, height),
                    Layout::top_down(Align::Center),
                    |ui| {
                        if icon_button_sized(
                            ui,
                            Icon::Star,
                            "Favorites",
                            state.favorites_only,
                            24.0,
                        )
                        .clicked()
                        {
                            state.favorites_only = true;
                            state.show_more = false;
                        }
                        ScrollArea::vertical()
                            .id_salt("accounts")
                            .max_height(height - 26.0)
                            .show(ui, |ui| {
                                for account in &accounts {
                                    let selected = !state.favorites_only
                                        && choice.account.as_ref() == Some(&account.id);
                                    let tip = match runtime.connection.get(&account.id) {
                                        Some(Connection::SignedIn(identity)) => {
                                            identity.email.as_ref().map_or_else(
                                                || account.label.clone(),
                                                |email| {
                                                    format!("{} \u{00b7} {email}", account.label)
                                                },
                                            )
                                        }
                                        _ => account.label.clone(),
                                    };
                                    let response = ui
                                        .add(
                                            egui::Button::new("")
                                                .selected(selected)
                                                .min_size(egui::vec2(27.0, 24.0)),
                                        )
                                        .on_hover_text(&tip);
                                    response.widget_info(|| {
                                        egui::WidgetInfo::selected(
                                            egui::WidgetType::SelectableLabel,
                                            true,
                                            selected,
                                            &tip,
                                        )
                                    });
                                    draw_provider_mark(
                                        ui,
                                        egui::Rect::from_center_size(
                                            response.rect.center(),
                                            egui::Vec2::splat(16.0),
                                        ),
                                        &account.provider,
                                    );
                                    if response.clicked() {
                                        if choice.account.as_ref() != Some(&account.id) {
                                            choice.account = Some(account.id.clone());
                                            choice.model = None;
                                            choice.effort = None;
        choice.claude_modes = ClaudeModes::default();
                                            let models = runtime.models(&account.id);
                                            if let Some(m) = models
                                                .iter()
                                                .find(|m| m.is_default)
                                                .or_else(|| models.first())
                                            {
                                                choice.select(&account.id, m);
                                            }
                                        }
                                        state.favorites_only = false;
                                        state.show_more = false;
                                    }
                                    if accounts
                                        .iter()
                                        .filter(|a| a.provider == account.provider)
                                        .count()
                                        > 1
                                    {
                                        ui.add_sized(
                                            [rail_width, 14.0],
                                            egui::Label::new(muted(&account.label)).truncate(),
                                        )
                                        .on_hover_text(tip);
                                    }
                                }
                            });
                    },
                );
                picker_divider(ui, height);
                ui.allocate_ui_with_layout(
                    egui::vec2(model_width, height),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_width(model_width);
                        ui.set_max_width(model_width);
                        let search = ui.add(
                            TextEdit::singleline(&mut state.search)
                                .hint_text(tr("Search models..."))
                                .desired_width(model_width),
                        );
                        if response.clicked() {
                            search.request_focus();
                        }
                        let query = state.search.trim().to_lowercase();
                        let mut models = Vec::new();
                        for account in &accounts {
                            if !state.favorites_only && choice.account.as_ref() != Some(&account.id)
                            {
                                continue;
                            }
                            for model in runtime.models(&account.id) {
                                if state.favorites_only
                                    && !picker
                                        .favorites
                                        .contains(&(account.id.clone(), model.id.clone()))
                                {
                                    continue;
                                }
                                if model.name.to_lowercase().contains(&query)
                                    || model.id.to_lowercase().contains(&query)
                                {
                                    models.push((account, model));
                                }
                            }
                        }
                        if models.is_empty() {
                            ui.label(muted(if state.favorites_only {
                                "No favorite models"
                            } else {
                                "No matching models"
                            }));
                        }
                        let expanded = state.show_more || !query.is_empty();
                        let limit = if expanded {
                            models.len()
                        } else {
                            models.len().min(4)
                        };
                        ScrollArea::vertical()
                            .id_salt("models")
                            .max_height(height - 24.0)
                            .show(ui, |ui| {
                                for (i, (account, model)) in models.iter().take(limit).enumerate() {
                                    ui.push_id((&account.id, &model.id), |ui| {
                                        ui.horizontal(|ui| {
                                            let favorite = (account.id.clone(), model.id.clone());
                                            let starred = picker.favorites.contains(&favorite);
                                            provider_mark(ui, &account.provider, 13.0);
                                            let current = choice.account.as_ref()
                                                == Some(&account.id)
                                                && choice
                                                    .model
                                                    .as_ref()
                                                    .map_or(model.is_default, |id| id == &model.id);
                                            let shortcut = [
                                                Key::Num1,
                                                Key::Num2,
                                                Key::Num3,
                                                Key::Num4,
                                                Key::Num5,
                                                Key::Num6,
                                                Key::Num7,
                                                Key::Num8,
                                                Key::Num9,
                                            ]
                                            .get(i)
                                            .copied();
                                            let name = if state.favorites_only {
                                                format!("{} \u{00b7} {}", model.name, account.label)
                                            } else {
                                                model.name.clone()
                                            };
                                            let mut button =
                                                egui::Button::new(RichText::new(name).size(SMALL))
                                                    .selected(current)
                                                    .truncate();
                                            if shortcut.is_some() {
                                                button = button.shortcut_text(
                                                    RichText::new(format!("Ctrl+{}", i + 1))
                                                        .size(10.5),
                                                );
                                            }
                                            let mut row = ui.add_sized(
                                                [(model_width - 49.0).max(50.0), 22.0],
                                                button,
                                            );
                                            if let Some(key) = shortcut {
                                                row = row.on_hover_text(format!(
                                                    "{}\nCtrl+{}",
                                                    model.id,
                                                    i + 1
                                                ));
                                                if ui.input_mut(|input| {
                                                    input.consume_key(egui::Modifiers::CTRL, key)
                                                }) {
                                                    choice.select(&account.id, model);
                                                }
                                            }
                                            if row.clicked() {
                                                choice.select(&account.id, model);
                                            }
                                            if icon_button_sized(
                                                ui,
                                                Icon::Star,
                                                if starred {
                                                    "Remove favorite"
                                                } else {
                                                    "Add favorite"
                                                },
                                                starred,
                                                22.0,
                                            )
                                            .clicked()
                                                && !picker.favorites.remove(&favorite)
                                            {
                                                picker.favorites.insert(favorite);
                                            }
                                        });
                                    });
                                }
                                if models.len() > limit && ui.button(tr("Show more")).clicked() {
                                    state.show_more = true;
                                }
                            });
                    },
                );
                picker_divider(ui, height);
                ui.allocate_ui_with_layout(
                    egui::vec2(effort_width, height),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_width(effort_width);
                        ui.set_max_width(effort_width);
                        ui.label(RichText::new(tr("Reasoning")).size(SMALL).strong());
                        let current = choice.account.as_ref().and_then(|id| {
                            let models = runtime.models(id);
                            choice
                                .model
                                .as_ref()
                                .and_then(|selected| {
                                    models.iter().find(|model| &model.id == selected)
                                })
                                .or_else(|| models.iter().find(|model| model.is_default))
                        });
                        if let Some(model) = current.filter(|model| !model.efforts.is_empty()) {
                            ScrollArea::vertical()
                                .id_salt("effort")
                                .max_height(height - 19.0)
                                .show(ui, |ui| {
                                    for effort in &model.efforts {
                                        let active = choice
                                            .effort
                                            .as_ref()
                                            .or(model.default_effort.as_ref())
                                            == Some(effort);
                                        if ui
                                            .selectable_label(active, effort_label(effort))
                                            .clicked()
                                        {
                                            choice.model = Some(model.id.clone());
                                            choice.effort = Some(effort.clone());
                                        }
                                    }
                                });
                        } else {
                            ui.label(muted("Provider controlled"));
                        }
                    },
                );
            });
            if let Some(account) = &choice.account {
                let support=runtime.claude_mode_support(account, choice.model.as_deref());
                if support.fast || support.ultracode {
                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        if support.fast {
                            ui.selectable_value(&mut choice.claude_modes.fast,false,tr("Standard"));
                            ui.selectable_value(&mut choice.claude_modes.fast,true,"\u{26a1} Fast")
                                .on_hover_text(tr("Request Fast from Claude. Uses separately billed credits when available."));
                        }
                        if support.ultracode && ui.selectable_label(choice.claude_modes.ultracode,"Ultracode")
                            .on_hover_text(tr("Let Claude use dynamic workflows for larger tasks.")).clicked() {
                            choice.claude_modes.ultracode = !choice.claude_modes.ultracode;
                        }
                    });
                }
            }
            if locked_account.is_none() && ui.small_button(tr("Manage providers")).clicked() {
                open_providers = true;
                ui.close();
            }
        });
    open_providers
}

pub(super) fn draft_picker(
    ui: &mut Ui,
    runtime: &mut Runtime,
    chat: &mut ChatState,
    root: Option<&std::path::Path>,
    composing: bool,
) -> bool {
    let mut choice = ModelChoice {
        account: match &chat.target {
            Target::Account(id) => Some(id.clone()),
            _ => None,
        },
        model: chat.model.clone(),
        effort: chat.effort.clone(),
        claude_modes: chat.claude_modes,
    };
    if choice.account.is_none() {
        choice.account = chat_accounts(runtime).first().map(|a| a.id.clone());
        choice.model = None;
        choice.effort = None;
        choice.claude_modes = ClaudeModes::default();
    }
    let open = model_picker(ui, runtime, &mut choice, &mut chat.picker, None);
    let selection_changed = choice.model != chat.model
        || choice
            .account
            .as_ref()
            .is_some_and(|id| chat.target != Target::Account(id.clone()));
    if chat.target != Target::Orchestrator {
        chat.target = choice.account.map_or(Target::Auto, Target::Account);
        chat.model = choice.model;
        chat.effort = choice.effort;
        chat.claude_modes = choice.claude_modes;
    }
    if (composing || selection_changed)
        && !chat.worktree
        && let Target::Account(account) = &chat.target
        && let Some(root) = root
    {
        runtime.prepare_draft(account, root, PRESETS[chat.preset].0);
    }
    open
}

pub(super) fn provider_name(kind: &ProviderKind) -> &'static str {
    match kind {
        ProviderKind::ClaudeCode => "Claude Code",
        ProviderKind::Codex => "Codex",
        ProviderKind::Gemini => "Gemini CLI",
        ProviderKind::Antigravity => "Antigravity",
        ProviderKind::OpenCode => "OpenCode",
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
pub(super) fn chat_accounts(runtime: &Runtime) -> Vec<Account> {
    runtime
        .profiles
        .accounts()
        .iter()
        .filter(|a| {
            a.enabled
                && runtime.connection_ready(&a.id)
                && matches!(
                    a.provider,
                    ProviderKind::ClaudeCode
                        | ProviderKind::Codex
                        | ProviderKind::Antigravity
                        | ProviderKind::OpenCode
                )
        })
        .cloned()
        .collect()
}

/// Dot color and words for an account's sign-in, from the provider's answer.
fn connection_view(runtime: &Runtime, account: &Account) -> (egui::Color32, String) {
    let detection = runtime.detections.get(&account.provider);
    if detection.is_some_and(|d| d.done && d.program.is_none()) {
        return (colors().error, "Not installed".into());
    }
    match runtime.connection.get(&account.id) {
        None | Some(Connection::Checking) => (colors().muted, "Checking…".into()),
        Some(Connection::SignedIn(_)) => (colors().success, "Authenticated".into()),
        Some(Connection::SignedOut(_)) => (colors().warning, "Signed out".into()),
        Some(Connection::GeminiMigrationRequired) => {
            (colors().warning, "Migration required".into())
        }
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
    pub(super) fn restore_chat_layout(&mut self) {
        let chat = &mut self.hub.agents.chat;
        if chat.restored {
            return;
        }
        chat.restored = true;
        chat.dock = self.preferences.agent_session.clone();
        chat.detached = self.preferences.agent_detached.clone();
        chat.picker
            .favorites
            .clone_from(&self.preferences.agent_favorites);
        if let Some(id) = &self.preferences.agent_account {
            chat.target = Target::Account(id.clone());
            chat.model.clone_from(&self.preferences.agent_model);
            chat.effort =
                Some(self.preferences.agent_effort.clone()).filter(|level| !level.is_empty());
        }
    }

    pub(super) fn remember_chat_layout(&mut self) {
        let chat = &self.hub.agents.chat;
        if !chat.restored {
            return;
        }
        self.preferences.agent_account = match &chat.target {
            Target::Account(id) => Some(id.clone()),
            _ => None,
        };
        if self.preferences.agent_favorites != chat.picker.favorites {
            self.preferences
                .agent_favorites
                .clone_from(&chat.picker.favorites);
        }
        self.preferences.agent_model.clone_from(&chat.model);
        self.preferences.agent_effort = chat.effort.clone().unwrap_or_default();
        if self.preferences.agent_session != chat.dock {
            self.preferences.agent_session = chat.dock.clone();
        }
        if self.preferences.agent_detached != chat.detached {
            self.preferences.agent_detached = chat.detached.clone();
        }
    }

    pub(super) fn conversations_sidebar(&mut self, ui: &mut Ui) {
        if !self.ensure_runtime(ui.ctx()) {
            ui.label(muted(tr("Agents are unavailable.")));
            return;
        }
        self.restore_chat_layout();
        let (runtime, chat) = self.hub.agents.runtime_and_chat();
        let mut select = None;
        let mut sessions: Vec<_> = runtime.sessions.iter().collect();
        sessions.sort_by_key(|session| {
            std::cmp::Reverse(
                session
                    .meta
                    .log
                    .last()
                    .map_or(session.meta.created_ms, |line| line.at_ms),
            )
        });
        let working = sessions.iter().filter(|session| session.running()).count();
        let pinned = &self.preferences.agent_pinned;
        let archived = &self.preferences.agent_archived;
        let mut row_actions: Vec<(String, RowAction)> = Vec::new();
        let idle: Vec<_> = sessions
            .iter()
            .filter(|session| !session.running())
            .copied()
            .collect();
        let pinned_rows: Vec<_> = idle
            .iter()
            .filter(|s| {
                pinned.contains(s.meta.id.as_str()) && !archived.contains(s.meta.id.as_str())
            })
            .copied()
            .collect();
        let archived_rows: Vec<_> = idle
            .iter()
            .filter(|s| archived.contains(s.meta.id.as_str()))
            .copied()
            .collect();
        // Remaining conversations grouped by project, keeping the most recent project first.
        let mut groups: Vec<(String, Vec<&LiveSession>)> = Vec::new();
        for session in idle.iter().filter(|s| {
            !pinned.contains(s.meta.id.as_str()) && !archived.contains(s.meta.id.as_str())
        }) {
            let project = project_name(session);
            match groups.iter_mut().find(|(name, _)| *name == project) {
                Some((_, rows)) => rows.push(session),
                None => groups.push((project, vec![session])),
            }
        }
        ScrollArea::vertical()
            .id_salt("saved-threads")
            .max_height(
                (ui.available_height()
                    - 94.0
                    - if working > 0 {
                        working.min(3) as f32 * 43.0 + 40.0
                    } else {
                        0.0
                    })
                .max(75.0),
            )
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if sessions.is_empty() {
                    ui.label(muted(tr("No conversations yet")));
                }
                let selected = chat.dock.as_deref();
                let mut section = |ui: &mut Ui, title: &str, rows: &[&LiveSession]| {
                    if rows.is_empty() {
                        return;
                    }
                    ui.add_space(6.0);
                    if !title.is_empty() {
                        ui.add(egui::Label::new(meta_text(title.to_owned())).truncate());
                    }
                    ui.indent(Id::new(("thread-group", title)), |ui| {
                        for session in rows {
                            let id = session.meta.id.as_str();
                            let (clicked, action) = thread_row(
                                ui,
                                session,
                                selected,
                                pinned.contains(id),
                                archived.contains(id),
                            );
                            if clicked {
                                select = Some(id.to_owned());
                            }
                            if let Some(action) = action {
                                row_actions.push((id.to_owned(), action));
                            }
                        }
                    });
                };
                section(ui, tr("Pinned"), &pinned_rows);
                for (project, rows) in &groups {
                    section(ui, project, rows);
                }
                if !archived_rows.is_empty() {
                    ui.add_space(6.0);
                    egui::CollapsingHeader::new(meta_text(format!(
                        "{} ({})",
                        tr("Archived"),
                        archived_rows.len()
                    )))
                    .id_salt("archived-threads")
                    .default_open(false)
                    .show(ui, |ui| section(ui, "", &archived_rows));
                }
            });
        if working > 0 {
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), (working.min(3) as f32 * 43.0) + 33.0),
                Layout::bottom_up(Align::Min),
                |ui| {
                    ScrollArea::vertical()
                        .id_salt("working-threads")
                        .max_height(working.min(3) as f32 * 43.0)
                        .show(ui, |ui| {
                            for session in sessions.iter().filter(|session| session.running()) {
                                let (clicked, _) =
                                    thread_row(ui, session, chat.dock.as_deref(), false, false);
                                if clicked {
                                    select = Some(session.meta.id.to_string());
                                }
                            }
                        });
                    ui.label(meta_text(format!("{} ({working})", tr("Working"))));
                },
            );
            ui.separator();
        }
        for (id, action) in row_actions {
            match action {
                RowAction::Pin => {
                    self.preferences.agent_pinned.insert(id.clone());
                    self.preferences.agent_archived.remove(&id);
                }
                RowAction::Unpin => {
                    self.preferences.agent_pinned.remove(&id);
                }
                RowAction::Archive => {
                    self.preferences.agent_archived.insert(id.clone());
                    self.preferences.agent_pinned.remove(&id);
                }
                RowAction::Unarchive => {
                    self.preferences.agent_archived.remove(&id);
                }
                RowAction::Delete => {
                    let (runtime, chat) = self.hub.agents.runtime_and_chat();
                    if let Some(index) = runtime
                        .sessions
                        .iter()
                        .position(|s| s.meta.id.as_str() == id)
                    {
                        match runtime.close(index) {
                            Ok(note) => {
                                runtime.notice = Some((true, note));
                                for slot in [&mut chat.dock, &mut chat.page, &mut chat.split] {
                                    if slot.as_deref() == Some(id.as_str()) {
                                        *slot = None;
                                    }
                                }
                                chat.detached.retain(|d| d != &id);
                                self.preferences.agent_pinned.remove(&id);
                                self.preferences.agent_archived.remove(&id);
                            }
                            Err(error) => runtime.notice = Some((false, error)),
                        }
                    }
                    self.remember_chat_layout();
                }
            }
        }
        if let Some(id) = select {
            self.hub.agents.chat.dock = Some(id);
            self.preferences.assistant_open = true;
            self.remember_chat_layout();
        }
    }

    /// The editor's right dock.
    pub(super) fn agent_dock(&mut self, ui: &mut Ui) {
        self.conversation(ui, Slot::Dock);
    }

    /// The Session Home floating chat island (shares `Slot::Dock` so switching
    /// to the editor moves the active conversation seamlessly to the right).
    pub(super) fn home_dock(&mut self, ui: &mut Ui) {
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
                .map(|_| tr("Conversation"));
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
    pub(super) fn start_chat_from_home(
        &mut self,
        ctx: &egui::Context,
        text: &str,
    ) -> Option<String> {
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
        let skill = self.skills.selected_input(&self.preferences.skills);
        let (runtime, chat) = self.hub.agents.runtime_and_chat();
        if chat.target == Target::Orchestrator {
            let order = trimmed.to_owned();
            self.luma_order(&order);
            self.companion.open = true;
            return None;
        }
        let accounts = chat_accounts(runtime);
        if accounts.is_empty() {
            self.open_providers();
            self.notice = Some((true, tr("Connect an AI to start").to_owned()));
            return None;
        }
        let chosen = match &chat.target {
            Target::Account(id) => accounts.iter().find(|a| &a.id == id),
            Target::Auto => accounts.first(),
            Target::Orchestrator => None,
        };
        let Some(chosen) = chosen.cloned() else {
            self.notice = Some((
                true,
                tr("The selected account is unavailable. Choose an account before sending.")
                    .to_owned(),
            ));
            return None;
        };
        if chat.target != Target::Account(chosen.id.clone()) {
            chat.model = None;
            chat.effort = None;
            chat.claude_modes = ClaudeModes::default();
        }
        chat.target = Target::Account(chosen.id.clone());
        let model = chat.model.clone();
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
            effort: chat.effort.clone(),
            claude_modes: chat.claude_modes,
            root,
            worktree: chat.worktree,
            preset: PRESETS[chat.preset].0,
            allow_safe_commands: true,
            share_context: true,
        });
        match created {
            Ok(index) => {
                let prompt =
                    with_file(trimmed, chat.attach.then_some(active.as_deref()).flatten()) + &skill;
                if let Err(e) = runtime.send_chat(index, &prompt, trimmed) {
                    runtime.sessions[index].draft = trimmed.to_owned();
                    runtime.notice = Some((false, tr(&e).to_owned()));
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
                self.notice = Some((true, tr(&e).to_owned()));
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
        let skill = self.skills.selected_input(&self.preferences.skills);
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
                Some(index) => session_body(
                    ui,
                    runtime,
                    chat,
                    index,
                    active.as_deref(),
                    &skill,
                    &mut actions,
                ),
                None => new_conversation(
                    ui,
                    runtime,
                    chat,
                    &slot,
                    root,
                    active.as_deref(),
                    &skill,
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
                Action::Fork(id, line) => {
                    let (runtime, chat) = self.hub.agents.runtime_and_chat();
                    match runtime.fork_chat(&id, line) {
                        Ok(index) => {
                            chat.set(&slot, Some(runtime.sessions[index].meta.id.to_string()))
                        }
                        Err(error) => runtime.notice = Some((false, error)),
                    }
                }
                Action::OpenGuided(reply_text) => {
                    let flow = self.preferences.guided.flow_order;
                    self.guided.watching_session = None;
                    if let Some(plan) = super::guided::extract_plan_from_text(&reply_text, flow) {
                        let first = plan.steps.first().map(|s| s.path.clone());
                        self.guided.load_plan(plan, self.root.as_deref());
                        if let Some(path) = first {
                            self.open_guided_file(&path);
                        }
                    } else {
                        self.guided.open = true;
                        self.notice = Some((
                            true,
                            tr("Could not read a valid guided plan from that reply.").to_owned(),
                        ));
                    }
                }
                Action::Objective(order) => {
                    self.luma_order(&order);
                    self.companion.open = true;
                }
                Action::OpenOrchestrator => {
                    self.companion.open = true;
                    self.hub.agents.luma.focus = true;
                }
                Action::AddAi => self.open_providers(),
            }
        }
        self.remember_chat_layout();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowAction {
    Pin,
    Unpin,
    Archive,
    Unarchive,
    Delete,
}

fn project_name(session: &LiveSession) -> String {
    let meta = &session.meta;
    meta.repo
        .as_deref()
        .unwrap_or(&meta.workdir)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(provider_name(&meta.provider))
        .to_owned()
}

fn relative_age(at_ms: u64) -> String {
    let secs = covalos_agents::now_ms().saturating_sub(at_ms) / 1000;
    match secs {
        0..60 => "now".to_owned(),
        60..3600 => format!("{}m", secs / 60),
        3600..86_400 => format!("{}h", secs / 3600),
        _ => format!("{}d", secs / 86_400),
    }
}

fn thread_row(
    ui: &mut Ui,
    session: &LiveSession,
    selected: Option<&str>,
    pinned: bool,
    archived: bool,
) -> (bool, Option<RowAction>) {
    let meta = &session.meta;
    let detail = if session.running() {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs(1));
        let elapsed = meta
            .log
            .iter()
            .rev()
            .find(|line| line.kind == "you")
            .map_or(0, |line| {
                covalos_agents::now_ms().saturating_sub(line.at_ms) / 1000
            });
        format!("{}  {elapsed}s", tr("Working"))
    } else {
        relative_age(meta.log.last().map_or(meta.created_ms, |line| line.at_ms))
    };
    let response = super::theme::row(ui, &meta.title, &detail, selected == Some(meta.id.as_str()))
        .on_hover_text(format!(
            "{}  \u{00b7}  {}  \u{00b7}  {}",
            provider_name(&meta.provider),
            project_name(session),
            meta.title
        ));
    let mut action = None;
    response.context_menu(|ui| {
        let pin = if pinned { tr("Unpin") } else { tr("Pin") };
        if ui.button(pin).clicked() {
            action = Some(if pinned {
                RowAction::Unpin
            } else {
                RowAction::Pin
            });
            ui.close();
        }
        let archive = if archived {
            tr("Unarchive")
        } else {
            tr("Archive")
        };
        if ui.button(archive).clicked() {
            action = Some(if archived {
                RowAction::Unarchive
            } else {
                RowAction::Archive
            });
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(
                !session.running(),
                egui::Button::new(RichText::new(tr("Delete conversation")).color(colors().error)),
            )
            .clicked()
        {
            action = Some(RowAction::Delete);
            ui.close();
        }
    });
    (response.clicked(), action)
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
        if !matches!(slot,Slot::Window(_)) {
            ui.label(RichText::new(tr("Conversation")).size(LEAD).strong());
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let more = icon_button(ui, Icon::More, tr("Conversation options"), false);
            egui::Popup::menu(&more).show(|ui| {
                if let Some(i) = current {
                    let session = &runtime.sessions[i];
                    let details = Id::new(("thread-details",session.meta.id.as_str()));
                    if ui.button(tr("Thread details")).clicked() {
                        ui.ctx().data_mut(|d| {let visible = d.get_temp(details).unwrap_or(false); d.insert_temp(details, !visible);});
                        ui.close();
                    }
                    if !matches!(slot, Slot::Window(_)) && ui.button(tr("Open in its own window")).clicked() {
                        actions.push(Action::Detach(session.meta.id.to_string()));
                        ui.close();
                    }
                    if !session.running() && ui.button(tr("Close this conversation (its worktree is removed only if unchanged)")).clicked() {
                        runtime.notice = Some(match runtime.close(i) { Ok(note) => (true,note), Err(e) => (false,e) });
                        actions.push(Action::Select(slot.clone(), None));
                        ui.close();
                    }
                }
                if *slot == Slot::Page && chat.split.is_none() && ui.button(tr("Split: show a second conversation")).clicked() { chat.split = Some(String::new()); ui.close(); }
                if *slot == Slot::Split && ui.button(tr("Close split")).clicked() { chat.split = None; ui.close(); }
                if ui.button(tr("Agent usage")).clicked() {ui.ctx().data_mut(|d| d.insert_temp(Id::new("agent-usage-open"), true)); ui.close();}
                if ui.button(tr("Manage providers")).clicked() {actions.push(Action::AddAi); ui.close();}
            });
            if !matches!(slot, Slot::Window(_)) && icon_button(ui, Icon::Add, tr("New chat"), false).clicked() {
                actions.push(Action::Select(slot.clone(), None));
            }
            if !matches!(slot, Slot::Window(_)) {
                let history = icon_button(ui,Icon::History,tr("Conversation history"),false);
                egui::Popup::menu(&history).show(|ui| {
                    ui.set_min_width(190.0);
                    if runtime.sessions.is_empty() {ui.label(muted(tr("No conversations yet")));}
                    ScrollArea::vertical().max_height(300.0).show(ui,|ui| {
                        for (i, session) in runtime.sessions.iter().enumerate().rev() {
                            if ui.selectable_label(current == Some(i),&session.meta.title).clicked() {
                                actions.push(Action::Select(slot.clone(),Some(session.meta.id.to_string())));ui.close();
                            }
                        }
                    });
                });
            }
        });
    });
    ui.separator();
    ui.add_space(7.0);
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
                ui.label(meta_text(tr(meta.status.label())));
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
                    if let Some(Connection::SignedOut(t) | Connection::Unknown(t)) =
                        runtime.connection.get(&account.id)
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
    skill: &str,
    actions: &mut Vec<Action>,
) {
    let account_id = runtime.sessions[index].meta.account.clone();
    ensure_connection(runtime, &account_id);
    let details = ui
        .ctx()
        .data(|d| {
            d.get_temp(Id::new((
                "thread-details",
                runtime.sessions[index].meta.id.as_str(),
            )))
        })
        .unwrap_or(false);
    if details {
        identity_line(ui, runtime, index);
    }
    let composer_height = 112.0;
    let height = (ui.available_height() - composer_height).max(120.0);
    let id = runtime.sessions[index].meta.id.as_str().to_owned();
    ScrollArea::vertical()
        .id_salt(("agent-timeline", &id))
        .max_height(height)
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            timeline(ui, &runtime.sessions[index], actions);
            Editor::conversation_approvals(ui, runtime, Some(&id));
            if details {
                changes_block(ui, chat, &runtime.sessions[index], actions);
                technical_log(ui, runtime, &runtime.sessions[index]);
            }
        });
    ui.add_space(6.0);
    let session = &mut runtime.sessions[index];
    let running = session.running();
    let stopping = session.stopping();
    let hint = if stopping {
        tr("Stopping… waiting for the provider to exit")
    } else if running {
        tr("Draft your next message")
    } else {
        tr("Message the agent")
    };
    let mut draft = std::mem::take(&mut session.draft);
    let account = session.meta.account.clone();
    let mut choice = ModelChoice {
        account: Some(account.clone()),
        model: session.meta.requested_model.clone(),
        effort: session.meta.effort.clone(),
        claude_modes: session.meta.claude_modes,
    };
    let previous = choice.clone();
    let mut manage = false;
    let mut editing = false;
    let result = composer(
        ui,
        ("composer", &id),
        &mut draft,
        hint,
        running,
        &mut chat.attach,
        active,
        |ui, field_changed| {
            editing = field_changed;
            if running {
                let name = runtime
                    .models(&account)
                    .iter()
                    .find(|m| Some(&m.id) == choice.model.as_ref())
                    .map(|m| m.name.as_str())
                    .or(choice.model.as_deref())
                    .unwrap_or(tr("Provider default"));
                provider_mark(ui, &runtime.sessions[index].meta.provider, 14.0);
                ui.label(meta_text(choice.effort.as_ref().map_or_else(
                    || name.to_owned(),
                    |e| format!("{name} {}", effort_label(e)),
                )));
            } else {
                manage = model_picker(ui, runtime, &mut choice, &mut chat.picker, Some(&account));
            }
        },
    );
    if manage {
        actions.push(Action::AddAi);
    }
    if choice != previous {
        runtime.sessions[index].meta.requested_model = choice.model;
        runtime.sessions[index].meta.effort = choice.effort;
        runtime.sessions[index].meta.claude_modes = choice.claude_modes;
        runtime.save_sessions();
    }
    match result {
        Composed::Send(text) => {
            let prompt = with_file(&text, chat.attach.then_some(active).flatten()) + skill;
            if !runtime.connection_ready(&account) {
                runtime.sessions[index].draft = text;
                runtime.notice = Some((false, tr("Connect an AI to start").to_owned()));
            } else if let Err(e) = runtime.send_chat(index, &prompt, &text) {
                runtime.sessions[index].draft = text;
                runtime.notice = Some((false, tr(&e).to_owned()));
            }
        }
        Composed::Stop => {
            runtime.sessions[index].draft = draft;
            runtime.cancel(index);
        }
        Composed::Idle => {
            let session = &mut runtime.sessions[index];
            session.draft = draft;
            if !running && !session.has_warm_session() && editing {
                let dir = session.meta.workdir.clone();
                let preset = session.meta.preset;
                runtime.prepare_draft(&account, &dir, preset);
            }
        }
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
            if ui.small_button(tr("Dismiss notice")).clicked() {
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
                format!("{} ×{n}", tool_title(name))
            } else {
                tool_title(name).to_owned()
            });
        }
        ui.horizontal_wrapped(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(16.0), egui::Sense::hover());
            draw_icon_sized(
                ui.painter(),
                rect.center(),
                Icon::Terminal,
                colors().muted,
                13.0,
            );
            ui.label(meta_text(names.join(" · ")));
        });
        tools.clear();
    };
    if meta.log.iter().all(|l| l.kind != "you") && !session.running() {
        ui.add_space(8.0);
        ui.label(secondary(tr(
            "Ask for a change, a review or an explanation. The agent works in the folder shown above.",
        )));
    }
    for (line_index, line) in meta.log.iter().enumerate() {
        if line.kind == "tool" {
            tools.push(&line.text);
            continue;
        }
        flush(ui, &mut tools);
        match line.kind.as_str() {
            "you" => user_message(ui, &line.text, line.at_ms),
            "agent" => {
                let (path, fork) = agent_message(
                    ui,
                    &meta.provider,
                    &line.text,
                    None,
                    Some((&meta.workdir, line.at_ms)),
                );
                if fork {
                    actions.push(Action::Fork(meta.id.clone(), line_index));
                }
                if let Some(path) = path {
                    actions.push(Action::Open(path));
                }
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
            "cancelled" | "recovery" | "approval" => activity(ui, colors().warning, &line.text),
            _ => {}
        }
    }
    flush(ui, &mut tools);
    if session.running() {
        let last_user = meta.log.iter().rposition(|line| line.kind == "you");
        if meta.status == Status::Running || !session.streaming.is_empty() || session.stopping() {
            let elapsed = last_user.map_or(0, |i| {
                covalos_agents::now_ms().saturating_sub(meta.log[i].at_ms) / 1000
            });
            let acp = matches!(
                meta.provider,
                ProviderKind::Antigravity | ProviderKind::Gemini | ProviderKind::OpenCode
            );
            let phase = if session.stopping() {
                tr("Stopping")
            } else if !session.streaming.is_empty() {
                tr("Writing response")
            } else if session.prompt_sent || (!acp && session.provider_started) {
                tr("Waiting for response")
            } else {
                tr("Preparing session")
            };
            // Steps since the last user prompt, for the progress card.
            let turn_start = last_user.map_or(0, |i| i + 1);
            let mut done_steps: Vec<String> = meta.log[turn_start..]
                .iter()
                .filter(|l| l.kind == "tool" || l.kind == "status")
                .map(|l| l.text.trim().chars().take(72).collect::<String>())
                .filter(|s| !s.is_empty())
                .collect();
            if done_steps.len() > 6 {
                let keep = done_steps.len() - 6;
                done_steps.drain(..keep);
            }
            progress_card(ui, &done_steps, phase, elapsed);
            if !session.streaming.is_empty() {
                let (path, _) = agent_message(
                    ui,
                    &meta.provider,
                    &session.streaming,
                    Some((phase, elapsed)),
                    Some((&meta.workdir, 0)),
                );
                if let Some(path) = path {
                    actions.push(Action::Open(path));
                }
            }
        }
    }
}

/// Live progress card: completed steps with checkmarks, the active step in bold next to
/// a pulsing four-dot grid, and the elapsed time underneath.
fn progress_card(ui: &mut Ui, done_steps: &[String], active_step: &str, elapsed: u64) {
    ui.add_space(6.0);
    let live = ui.style().animation_time > 0.0;
    let t = ui.input(|i| i.time) as f32;
    Frame::new()
        .fill(colors().inset)
        .stroke(egui::Stroke::new(
            1.0_f32,
            colors().border.gamma_multiply(0.85),
        ))
        .corner_radius(12)
        .inner_margin(Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 7.0;
            for step in done_steps {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    let (icon_r, _) =
                        ui.allocate_exact_size(egui::vec2(15.0, 15.0), egui::Sense::hover());
                    let c = icon_r.center();
                    let stroke_col = colors().muted.gamma_multiply(0.85);
                    ui.painter()
                        .circle_stroke(c, 6.2, egui::Stroke::new(1.1_f32, stroke_col));
                    ui.painter().line_segment(
                        [c + egui::vec2(-2.6, 0.2), c + egui::vec2(-0.6, 2.2)],
                        egui::Stroke::new(1.2_f32, stroke_col),
                    );
                    ui.painter().line_segment(
                        [c + egui::vec2(-0.6, 2.2), c + egui::vec2(3.0, -2.0)],
                        egui::Stroke::new(1.2_f32, stroke_col),
                    );
                    ui.add(
                        egui::Label::new(RichText::new(step).size(BODY).color(colors().muted))
                            .truncate(),
                    );
                });
            }
            // Active step row with animated 2x2 four-dot pulse + bold label
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let (icon_r, _) =
                    ui.allocate_exact_size(egui::vec2(15.0, 15.0), egui::Sense::hover());
                let c = icon_r.center();
                let offsets = [
                    egui::vec2(-2.6, -2.6),
                    egui::vec2(2.6, -2.6),
                    egui::vec2(2.6, 2.6),
                    egui::vec2(-2.6, 2.6),
                ];
                for (i, off) in offsets.iter().enumerate() {
                    let phase = if live {
                        ((t * 4.2 - i as f32 * 0.9).sin() * 0.5 + 0.5).clamp(0.28, 1.0)
                    } else {
                        0.75
                    };
                    ui.painter()
                        .circle_filled(c + *off, 1.7, colors().text.gamma_multiply(phase));
                }
                ui.add(
                    egui::Label::new(
                        RichText::new(active_step)
                            .size(BODY)
                            .strong()
                            .color(colors().text),
                    )
                    .truncate(),
                );
            });
        });
    ui.add_space(5.0);
    let time_str = if elapsed >= 60 {
        format!("{}m {:02}s", elapsed / 60, elapsed % 60)
    } else {
        format!("{elapsed}s")
    };
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        ui.label(
            RichText::new(format!("{active_step}   {time_str}  ›"))
                .size(SMALL)
                .color(colors().muted),
        );
    });
    if live {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(50));
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

fn user_message(ui: &mut Ui, text: &str, at_ms: u64) {
    ui.add_space(6.0);
    let width = ui.available_width();
    ui.with_layout(Layout::top_down(Align::Max), |ui| {
        let message = Frame::new()
            .fill(colors().elevated)
            .corner_radius(12)
            .inner_margin(Margin::symmetric(11, 8))
            .show(ui, |ui| {
                ui.set_max_width(width * 0.86);
                ui.add(
                    egui::Label::new(RichText::new(text).size(BODY + 0.5).color(colors().text))
                        .wrap()
                        .selectable(true),
                );
            });
        message_actions(ui, message.response.rect, text, at_ms, true);
    });
    ui.add_space(4.0);
}

fn tool_title(name: &str) -> &str {
    match name {
        "shell" | "command_execution" => tr("Terminal command"),
        _ => name,
    }
}

fn message_actions(
    ui: &mut Ui,
    message: egui::Rect,
    text: &str,
    at_ms: u64,
    align_right: bool,
) -> bool {
    let mut fork = false;
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 23.0),
        if align_right {
            Layout::right_to_left(Align::Center)
        } else {
            Layout::left_to_right(Align::Center)
        },
        |ui| {
            let row = ui.available_rect_before_wrap();
            let hitbox = message.union(egui::Rect::from_min_size(
                row.min,
                egui::vec2(row.width(), 23.0),
            ));
            let hovered = ui.rect_contains_pointer(hitbox);
            let builder = if hovered {
                egui::UiBuilder::new()
            } else {
                egui::UiBuilder::new().invisible()
            };
            ui.scope_builder(builder, |ui| {
                if icon_button_sized(ui, Icon::Copy, "Copy", false, 21.0).clicked() {
                    ui.ctx().copy_text(text.to_owned());
                }
                if !align_right {
                    fork = icon_button_sized(ui, Icon::Git, "Fork conversation", false, 21.0)
                        .clicked();
                }
                if let Ok(utc) = time::OffsetDateTime::from_unix_timestamp((at_ms / 1000) as i64) {
                    let stamp = utc;
                    ui.label(meta_text(format!(
                        "{:02}:{:02} UTC",
                        stamp.hour(),
                        stamp.minute()
                    )));
                }
            });
        },
    );
    fork
}

fn typing_dots(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(26.0, 15.0), egui::Sense::hover());
    let motion = ui.style().animation_time > 0.0;
    let seconds = ui.input(|i| i.time) as f32;
    for index in 0..3 {
        let lift = if motion {
            (seconds * 7.0 - index as f32 * 0.8).sin() * 2.0
        } else {
            0.0
        };
        ui.painter().circle_filled(
            rect.left_center() + egui::vec2(4.0 + index as f32 * 8.0, -lift),
            2.25,
            colors().accent,
        );
    }
    if motion {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(33));
    }
}

fn agent_message(
    ui: &mut Ui,
    provider: &ProviderKind,
    text: &str,
    progress: Option<(&str, u64)>,
    location: Option<(&std::path::Path, u64)>,
) -> (Option<PathBuf>, bool) {
    ui.add_space(9.0);
    let message = ui.scope(|ui| {
        ui.horizontal(|ui| {
            provider_mark(ui, provider, 16.0);
            ui.label(
                RichText::new(provider_name(provider))
                    .size(SMALL)
                    .strong()
                    .color(colors().secondary),
            );
        });
        Frame::new()
            .inner_margin(Margin {
                left: 24,
                right: 4,
                top: 2,
                bottom: 2,
            })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                if let Some((phase, seconds)) = progress {
                    ui.horizontal(|ui| {
                        typing_dots(ui);
                        ui.label(meta_text(format!("{seconds}s")))
                            .on_hover_text(phase);
                    });
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_secs(1));
                }
                location.and_then(|(root, _)| super::hub::markdown_in_project(ui, text, root))
            })
            .inner
    });
    let fork = progress.is_none()
        && location.is_some_and(|(_, stamp)| {
            message_actions(ui, message.response.rect, text, stamp, false)
        });
    ui.add_space(8.0);
    (message.inner, fork)
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

fn technical_log(ui: &mut Ui, runtime: &Runtime, session: &LiveSession) {
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
        for entry in runtime.audit(&session.meta.id) {
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
    target: impl FnOnce(&mut Ui, bool),
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
        .fill(if super::theme::active() == super::theme::Theme::OledDark {
            egui::Color32::from_gray(17)
        } else {
            colors().surface
        })
        .stroke(egui::Stroke::new(1.0_f32, colors().border))
        .corner_radius(16)
        .inner_margin(Margin::symmetric(13, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let input = ui.add(
                TextEdit::multiline(draft)
                    .id(id)
                    .return_key(egui::KeyboardShortcut::new(
                        egui::Modifiers::SHIFT,
                        Key::Enter,
                    ))
                    .frame(false)
                    .hint_text(meta_text(hint))
                    .font(egui::FontId::proportional(BODY + 1.0))
                    .desired_rows(2)
                    .desired_width(f32::INFINITY),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                if running {
                    if icon_button(ui, Icon::Stop, tr("Stop"), true).clicked() {
                        result = Composed::Stop;
                    }
                } else if ui
                    .add_enabled_ui(!draft.trim().is_empty(), |ui| {
                        icon_button(ui, Icon::Send, tr("Send"), false)
                    })
                    .inner
                    .clicked()
                {
                    send = true;
                }
                if let Some(file) = active {
                    let name = file.rsplit('/').next().unwrap_or(file);
                    let text = if *attach {
                        format!("+ {name}")
                    } else {
                        format!("– {name}")
                    };
                    let width = (ui.available_width() / 3.0).clamp(32.0, 128.0);
                    if ui
                        .add_sized(
                            [width, ui.spacing().interact_size.y],
                            egui::Button::new(meta_text(text)).frame(false).truncate(),
                        )
                        .on_hover_text(format!(
                            "{}\n{file}",
                            tr("Tell the agent which file is open in the editor")
                        ))
                        .clicked()
                    {
                        *attach = !*attach;
                    }
                }
                target(ui, input.changed() || input.gained_focus());
            });
        });
    if send && !running && !draft.trim().is_empty() {
        result = Composed::Send(std::mem::take(draft));
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn new_conversation(
    ui: &mut Ui,
    runtime: &mut Runtime,
    chat: &mut ChatState,
    slot: &Slot,
    root: Option<PathBuf>,
    active: Option<&str>,
    skill: &str,
    actions: &mut Vec<Action>,
) {
    let accounts = chat_accounts(runtime);
    for a in &accounts {
        ensure_connection(runtime, &a.id);
    }
    if chat.target == Target::Auto
        && let Some(first) = accounts.first()
    {
        chat.target = Target::Account(first.id.clone());
        chat.model = None;
        chat.effort = None;
        chat.claude_modes = ClaudeModes::default();
    }
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
            ui.add_space(18.0);
            Frame::new()
                .fill(colors().surface)
                .stroke(egui::Stroke::new(
                    1.0_f32,
                    colors().border.gamma_multiply(0.65),
                ))
                .corner_radius(14)
                .inner_margin(Margin::same(18))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let (badge, _) =
                            ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::hover());
                        ui.painter().rect_filled(badge, 9, colors().accent_bg);
                        draw_icon_sized(
                            ui.painter(),
                            badge.center(),
                            Icon::Sparkles,
                            colors().accent,
                            15.0,
                        );
                        ui.add_space(6.0);
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new(tr("New conversation"))
                                    .size(LEAD)
                                    .strong()
                                    .color(colors().text),
                            );
                            ui.label(muted(tr(
                                "Choose how the agent should work in this project.",
                            )));
                        });
                    });
                    ui.add_space(12.0);
                    target_picker(ui, chat);
                    if chat.target == Target::Orchestrator {
                        ui.add_space(10.0);
                        plan_card(ui, runtime, actions);
                    }
                });
        });
    if !ready {
        notice(ui, runtime);
        return;
    }
    let mut open_providers = false;
    let hint = if chat.target == Target::Orchestrator {
        tr("Give Luma an order; it staffs and coordinates the agents")
    } else {
        tr("Message the agent")
    };
    let mut draft = std::mem::take(&mut chat.draft);
    let mut attach = chat.attach;
    let result = composer(
        ui,
        ("composer-new", format!("{slot:?}")),
        &mut draft,
        hint,
        false,
        &mut attach,
        active,
        |ui, composing| {
            if chat.target == Target::Orchestrator {
                ui.label(meta_text(tr("Luma · lead developer")));
            } else {
                open_providers = draft_picker(ui, runtime, chat, root.as_deref(), composing);
            }
        },
    );
    chat.draft = draft;
    chat.attach = attach;
    if open_providers {
        actions.push(Action::AddAi);
    }
    notice(ui, runtime);
    let Composed::Send(text) = result else { return };
    if chat.target == Target::Orchestrator {
        actions.push(Action::Objective(text));
        return;
    }
    let Some(account) = accounts
        .iter()
        .find(|a| chat.target == Target::Account(a.id.clone()))
    else {
        chat.draft = text;
        runtime.notice = Some((
            false,
            tr("The selected account is unavailable. Choose an account before sending.").to_owned(),
        ));
        return;
    };
    let title: String = text
        .lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(48)
        .collect();
    let model = chat.model.clone();
    let created = runtime.create_session(NewSession {
        title,
        account: account.id.clone(),
        model,
        effort: chat.effort.clone(),
        claude_modes: chat.claude_modes,
        root: root.clone().expect("checked"),
        worktree: chat.worktree,
        preset: PRESETS[chat.preset].0,
        allow_safe_commands: true,
        share_context: true,
    });
    match created {
        Ok(index) => {
            let prompt = with_file(&text, chat.attach.then_some(active).flatten()) + skill;
            if let Err(e) = runtime.send_chat(index, &prompt, &text) {
                runtime.sessions[index].draft = text;
                runtime.notice = Some((false, tr(&e).to_owned()));
            }
            let id = runtime.sessions[index].meta.id.as_str().to_owned();
            actions.push(Action::Select(slot.clone(), Some(id)));
        }
        Err(e) => {
            chat.draft = text;
            runtime.notice = Some((false, tr(&e).to_owned()));
        }
    }
}

fn target_picker(ui: &mut Ui, chat: &mut ChatState) {
    if ui
        .selectable_label(
            chat.target == Target::Orchestrator,
            tr("Luma · lead developer"),
        )
        .clicked()
    {
        chat.target = if chat.target == Target::Orchestrator {
            Target::Auto
        } else {
            Target::Orchestrator
        };
    }
    ui.horizontal_wrapped(|ui| {
        for (index, (_, label)) in PRESETS.iter().enumerate() {
            ui.selectable_value(&mut chat.preset, index, tr(label));
        }
    });
    checkbox(ui, &mut chat.worktree, tr("Work in its own git worktree"));
}

fn plan_card(ui: &mut Ui, runtime: &Runtime, actions: &mut Vec<Action>) {
    let Some(run) = runtime.plan.as_ref() else {
        ui.label(meta_text(tr(
            "Luma picks the agents, model and effort within your ceiling. Your message becomes its order.",
        )));
        if ui.small_button(tr("Open Luma")).clicked() {
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
            ui.label(meta_text(super::agent_hub::plan_label(&plan.state)));
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
            if ui.small_button(tr("Open Luma")).clicked() {
                actions.push(Action::OpenOrchestrator);
            }
        });
}

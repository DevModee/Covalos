use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
};

use eframe::egui::{self, Id, Key, RichText, ScrollArea, TextEdit, Ui};

use super::{Editor, Pending, theme::*};
use crate::workspace;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum NotificationSource {
    Covalos,
    #[allow(dead_code)]
    Agent,
    #[allow(dead_code)]
    GitHub,
}

impl NotificationSource {
    fn label(self) -> &'static str {
        match self {
            Self::Covalos => "Covalos",
            Self::Agent => "Agent",
            Self::GitHub => "GitHub",
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum NotificationFilter {
    #[default]
    All,
    Covalos,
    Agent,
    GitHub,
}

impl NotificationFilter {
    const ALL: [Self; 4] = [Self::All, Self::Covalos, Self::Agent, Self::GitHub];

    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Covalos => "Covalos",
            Self::Agent => "Agent",
            Self::GitHub => "GitHub",
        }
    }

    fn matches(self, source: NotificationSource) -> bool {
        match self {
            Self::All => true,
            Self::Covalos => source == NotificationSource::Covalos,
            Self::Agent => source == NotificationSource::Agent,
            Self::GitHub => source == NotificationSource::GitHub,
        }
    }
}

pub(super) struct Notification {
    pub(super) source: NotificationSource,
    pub(super) message: String,
    pub(super) is_error: bool,
    pub(super) unread: bool,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum PaletteCategory {
    #[default]
    All,
    Files,
    Navigation,
    Git,
    Settings,
}

impl PaletteCategory {
    const ALL: [Self; 5] = [
        Self::All,
        Self::Files,
        Self::Navigation,
        Self::Git,
        Self::Settings,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Files => "Files",
            Self::Navigation => "Navigation",
            Self::Git => "Git",
            Self::Settings => "Settings",
        }
    }
}

pub(super) struct Palette {
    query: String,
    focus: bool,
    selected: usize,
    category: PaletteCategory,
    files: Vec<PathBuf>,
    results: mpsc::Receiver<Vec<PathBuf>>,
    stop: Arc<AtomicBool>,
}

impl Drop for Palette {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Palette {
    fn new(root: Option<PathBuf>, ctx: egui::Context) -> Self {
        let (sender, results) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        if let Some(root) = root {
            let stop = stop.clone();
            thread::spawn(move || {
                let files = workspace::project_files(&root, &stop);
                let _ = sender.send(files);
                ctx.request_repaint();
            });
        }
        Self {
            query: String::new(),
            focus: true,
            selected: 0,
            category: PaletteCategory::All,
            files: Vec::new(),
            results,
            stop,
        }
    }
}

#[derive(Clone, Copy)]
enum Action {
    OpenFolder,
    OpenFile,
    CreateProject,
    Search,
    Find,
    Save,
    Terminal,
    AgentHub,
    Sidebar,
    FocusMode,
    Notifications,
    SourceControl,
    Settings,
    Detach,
    DockAll,
    Library,
    About,
}

const ACTIONS: &[(Action, &str, PaletteCategory)] = &[
    (Action::OpenFolder, "Open folder", PaletteCategory::Files),
    (Action::OpenFile, "Open file", PaletteCategory::Files),
    (Action::CreateProject, "Create project", PaletteCategory::Files),
    (Action::Search, "Search workspace", PaletteCategory::Navigation),
    (Action::Find, "Find in active file", PaletteCategory::Navigation),
    (Action::Save, "Save active file", PaletteCategory::Files),
    (Action::Terminal, "Toggle terminal", PaletteCategory::Navigation),
    (Action::AgentHub, "Toggle Agent Hub", PaletteCategory::Navigation),
    (Action::Sidebar, "Toggle sidebar", PaletteCategory::Navigation),
    (Action::FocusMode, "Focus mode", PaletteCategory::Navigation),
    (Action::Notifications, "Notifications", PaletteCategory::Navigation),
    (Action::SourceControl, "Source control", PaletteCategory::Git),
    (Action::Settings, "Open settings", PaletteCategory::Settings),
    (Action::Detach, "Detach to floating editor", PaletteCategory::Navigation),
    (Action::DockAll, "Dock all", PaletteCategory::Navigation),
    (Action::Library, "Open Library Studio", PaletteCategory::Settings),
    (Action::About, "About Covalos", PaletteCategory::Settings),
];

enum Selection {
    Action(Action),
    File(PathBuf),
    Project(PathBuf),
    Ask(String),
}

impl Editor {
    pub(super) fn open_palette(&mut self, ctx: &egui::Context) {
        self.palette = Some(Palette::new(self.root.clone(), ctx.clone()));
    }

    pub(super) fn workspace_picker(&mut self, ui: &mut Ui) {
        let label = self
            .root
            .as_deref()
            .map(workspace::filename)
            .unwrap_or_else(|| tr("Projects").to_owned());
        ui.menu_button(
            RichText::new(label)
                .size(BODY)
                .color(if self.root.is_some() {
                    colors().text
                } else {
                    colors().secondary
                }),
            |ui| {
                ui.set_min_width(210.0);
                if !self.preferences.recent.is_empty() {
                    for root in self.preferences.recent.clone() {
                        let is_active = self.root.as_ref() == Some(&root);
                        let name = workspace::filename(&root);
                        let badge_color = project_badge_color(&name);
                        let initial = name
                            .chars()
                            .next()
                            .unwrap_or('P')
                            .to_ascii_uppercase()
                            .to_string();
                        let (row_rect, resp) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width().max(190.0), 30.0),
                            egui::Sense::click(),
                        );
                        if resp.hovered() || is_active {
                            ui.painter().rect_filled(
                                row_rect,
                                6,
                                if is_active {
                                    colors().elevated
                                } else {
                                    colors().hover
                                },
                            );
                        }
                        let badge = egui::Rect::from_center_size(
                            row_rect.left_center() + egui::vec2(14.0, 0.0),
                            egui::vec2(18.0, 18.0),
                        );
                        ui.painter()
                            .rect_filled(badge, 5, badge_color.gamma_multiply(0.24));
                        ui.painter().rect_stroke(
                            badge,
                            5,
                            egui::Stroke::new(1.0_f32, badge_color.gamma_multiply(0.75)),
                            egui::StrokeKind::Inside,
                        );
                        ui.painter().text(
                            badge.center(),
                            egui::Align2::CENTER_CENTER,
                            initial,
                            egui::FontId::proportional(11.0),
                            badge_color,
                        );
                        ui.painter().text(
                            row_rect.left_center() + egui::vec2(30.0, 0.0),
                            egui::Align2::LEFT_CENTER,
                            &name,
                            egui::FontId::proportional(BODY),
                            if is_active {
                                colors().text
                            } else {
                                colors().secondary
                            },
                        );
                        if is_active {
                            ui.painter().circle_filled(
                                row_rect.right_center() - egui::vec2(10.0, 0.0),
                                3.0,
                                badge_color,
                            );
                        }
                        if resp.on_hover_text(root.display().to_string()).clicked() {
                            ui.close();
                            self.request(Pending::OpenWorkspace(root), ui.ctx());
                        }
                    }
                    ui.separator();
                }
                if ui.button(tr("Open folder…")).clicked() {
                    ui.close();
                    self.pick_workspace(ui.ctx());
                }
                if ui.button(tr("Create project…")).clicked() {
                    ui.close();
                    self.pick_new_project(ui.ctx());
                }
            },
        );
    }

    pub(super) fn welcome(&mut self, ui: &mut Ui) {
        super::motion::enter(
            ui,
            Id::new(("workspace-home-entry", self.root.clone())),
            self.preferences.transitions.seconds(),
        );
        let area = ui.available_rect_before_wrap();
        let mood = self.agent_mood(ui.input(|i| i.focused));
        self.identity
            .paint(ui, area, &self.preferences.identity, mood);
        if self.root.is_some() {
            self.project_session_home(ui, area);
            return;
        }
        // One centred column inside the available content area (not the window), with a
        // readable maximum width. Its vertical position comes from content known up
        // front, so loading recents never shifts the layout.
        let width = (area.width() - 64.0).clamp(240.0, 640.0).min(area.width());
        let left = area.center().x - width * 0.5;
        let rows = if width > 440.0 { 2.0 } else { 4.0 };
        let recent = self.preferences.recent.len().min(8) as f32;
        let estimate = 136.0
            + rows * 76.0
            + if recent > 0.0 {
                70.0 + recent * 52.0
            } else {
                0.0
            };
        let top = ((area.height() - estimate) * 0.42).clamp(24.0, 160.0);
        let rect = egui::Rect::from_min_size(
            egui::pos2(left, area.top() + top),
            egui::vec2(width, (area.height() - top).max(1.0)),
        );
        // Reading scrim: the background stays alive at the edges and calms behind content.
        super::mascot::soft_ellipse(
            ui.painter(),
            egui::pos2(rect.center().x, area.top() + top + 190.0),
            egui::vec2(width * 0.85, 300.0),
            colors().canvas.gamma_multiply(0.92),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            ui.set_width(width);
            ScrollArea::vertical()
                .id_salt("welcome")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    let title = self
                        .root
                        .as_deref()
                        .map(workspace::filename)
                        .unwrap_or_else(|| tr("Welcome to Covalos").to_owned());
                    ui.horizontal(|ui| {
                        let text_width = width;
                        ui.allocate_ui_with_layout(
                            egui::vec2(text_width, 112.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                ui.add_space(12.0);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(title)
                                            .size(if width > 480.0 { DISPLAY } else { 28.0 })
                                            .strong(),
                                    )
                                    .wrap(),
                                );
                                ui.add_space(8.0);
                                ui.add(
                                    egui::Label::new(secondary(if self.root.is_some() {
                                        "Find a file or explore your workspace."
                                    } else {
                                        "Open a project or continue your work."
                                    }))
                                    .wrap(),
                                );
                            },
                        );
                    });
                    ui.add_space(24.0);
                    let mac = cfg!(target_os = "macos");
                    let cards = [
                        (
                            Icon::Explorer,
                            "Open folder",
                            if mac { "⌘ O" } else { "Ctrl O" },
                        ),
                        (
                            Icon::Code,
                            "Open file",
                            if mac { "⇧ ⌘ O" } else { "Ctrl Shift O" },
                        ),
                        (Icon::Add, "Create project", "New folder"),
                        (Icon::Hub, "Agents", "Claude · Codex · Gemini"),
                    ];
                    let columns = if width > 440.0 { 2 } else { 1 };
                    let gap = 10.0;
                    let card_w = (width - gap * (columns as f32 - 1.0)) / columns as f32;
                    let mut clicked = None;
                    for chunk in cards.chunks(columns) {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = gap;
                            for &(icon, title, detail) in chunk {
                                if home_card(ui, card_w, icon, title, detail).clicked() {
                                    clicked = Some(title);
                                }
                            }
                        });
                        ui.add_space(gap - 4.0);
                    }
                    match clicked {
                        Some("Open folder") => self.pick_workspace(ui.ctx()),
                        Some("Open file") => self.pick_file(),
                        Some("Create project") => self.pick_new_project(ui.ctx()),
                        Some(_) => self.hub_open = true,
                        None => {}
                    }
                    if !self.preferences.setup_completed && self.root.is_none() {
                        ui.add_space(20.0);
                        self.initial_setup_card(ui);
                    }
                    if !self.preferences.recent.is_empty() {
                        ui.add_space(28.0);
                        section(ui, tr("Recent projects"), "");
                        for root in self.preferences.recent.clone() {
                            let response = ui.add_sized(
                                [ui.available_width(), 52.0],
                                egui::Button::new("").frame(false),
                            );
                            let painter = ui.painter_at(response.rect);
                            if response.hovered() {
                                painter.rect_filled(response.rect, CONTROL_RADIUS, colors().hover);
                            }
                            draw_icon(
                                &painter,
                                response.rect.left_center() + egui::vec2(16.0, 0.0),
                                Icon::Explorer,
                                colors().muted,
                            );
                            let text_rect = response.rect.shrink2(egui::vec2(40.0, 4.0));
                            let painter = painter.with_clip_rect(text_rect);
                            painter.text(
                                text_rect.left_top() + egui::vec2(0.0, 5.0),
                                egui::Align2::LEFT_TOP,
                                workspace::filename(&root),
                                egui::FontId::proportional(BODY),
                                colors().text,
                            );
                            let displayed = root.display().to_string();
                            let displayed = displayed.strip_prefix(r"\\?\").unwrap_or(&displayed);
                            painter.text(
                                text_rect.left_top() + egui::vec2(0.0, 26.0),
                                egui::Align2::LEFT_TOP,
                                displayed,
                                egui::FontId::proportional(SMALL),
                                colors().muted,
                            );
                            if response.clicked() {
                                self.request(Pending::OpenWorkspace(root.clone()), ui.ctx());
                            }
                            response.on_hover_text(root.display().to_string());
                        }
                    }
                });
        });
    }

    /// Minimalist project home shown once a workspace is open and no file tab is active.
    fn project_session_home(&mut self, ui: &mut Ui, area: egui::Rect) {
        let width = (area.width() - 64.0).clamp(340.0, 680.0).min(area.width());
        let left = area.center().x - width * 0.5;
        let top = ((area.height() - 400.0) * 0.34).clamp(36.0, 170.0);
        let rect = egui::Rect::from_min_size(
            egui::pos2(left, area.top() + top),
            egui::vec2(width, (area.height() - top).max(1.0)),
        );
        // Soft reading scrim keeps ASCII / Dither background visible around the edges.
        super::mascot::soft_ellipse(
            ui.painter(),
            egui::pos2(rect.center().x, area.top() + top + 140.0),
            egui::vec2(width * 0.85, 240.0),
            colors().inset.gamma_multiply(0.85),
        );
        let auto_skill_id = Id::new("home-auto-skill");
        let usage_modal_id = Id::new("agent-usage-open");
        let mut auto_skill: bool = ui
            .ctx()
            .data(|d| d.get_temp(auto_skill_id))
            .unwrap_or(true);
        let mut usage_open: bool = ui
            .ctx()
            .data(|d| d.get_temp(usage_modal_id))
            .unwrap_or(false);
        let mut send_prompt_now = false;
        let mut open_add_ai = false;
        let mut quick_action: Option<&str> = None;

        // Collect real aggregated token metrics from local runtime sessions.
        let (total_in, total_out, total_turns) = self.hub.agents.runtime.as_ref().map_or(
            (0u64, 0u64, 0u32),
            |rt| {
                rt.sessions.iter().fold((0u64, 0u64, 0u32), |acc, s| {
                    (
                        acc.0.saturating_add(s.meta.input_tokens),
                        acc.1.saturating_add(s.meta.output_tokens),
                        acc.2.saturating_add(s.meta.turns),
                    )
                })
            },
        );
        let accounts: Vec<(covalos_agents::AccountId, String, covalos_agents::ProviderKind)> =
            self.hub
                .agents
                .runtime
                .as_ref()
                .map(|rt| {
                    rt.profiles
                        .accounts()
                        .iter()
                        .map(|a| (a.id.clone(), a.label.clone(), a.provider.clone()))
                        .collect()
                })
                .unwrap_or_default();

        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            ui.set_width(width);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(tr("Where should we begin?"))
                        .size(26.0)
                        .strong()
                        .color(colors().text),
                );
            });
            ui.add_space(20.0);
            // Floating rounded prompt composer.
            egui::Frame::new()
                .fill(colors().surface.gamma_multiply(0.95))
                .corner_radius(14)
                .stroke(egui::Stroke::new(1.0_f32, colors().border))
                .inner_margin(egui::Margin::symmetric(16, 14))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let response = ui.add(
                        TextEdit::multiline(&mut self.chat.prompt)
                            .hint_text(tr("Ask anything. @ files, $ skills, / commands"))
                            .frame(false)
                            .desired_rows(3)
                            .desired_width(f32::INFINITY)
                            .char_limit(12_000),
                    );
                    let enter = response.has_focus()
                        && ui.input(|i| i.key_pressed(Key::Enter) && !i.modifiers.shift);
                    if enter && !self.chat.prompt.trim().is_empty() {
                        send_prompt_now = true;
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        if icon_button(ui, Icon::Add, "Open file", false).clicked() {
                            self.pick_file();
                        }
                        let skill_pill = egui::Button::new(
                            RichText::new(if auto_skill {
                                tr("Auto skills")
                            } else {
                                tr("Manual skills")
                            })
                            .size(SMALL)
                            .color(if auto_skill {
                                colors().accent
                            } else {
                                colors().muted
                            }),
                        )
                        .fill(if auto_skill {
                            colors().accent_bg
                        } else {
                            egui::Color32::TRANSPARENT
                        })
                        .stroke(egui::Stroke::new(1.0_f32, colors().border))
                        .corner_radius(8);
                        if ui
                            .add(skill_pill)
                            .on_hover_text(tr(
                                "Automatically attach matching enabled skills without asking.",
                            ))
                            .clicked()
                        {
                            auto_skill = !auto_skill;
                        }
                        let guided_armed = self.guided.armed;
                        let guided_pill = egui::Button::new(
                            RichText::new(tr("Guided Mode"))
                                .size(SMALL)
                                .color(if guided_armed {
                                    colors().accent
                                } else {
                                    colors().muted
                                }),
                        )
                        .fill(if guided_armed {
                            colors().accent_bg
                        } else {
                            egui::Color32::TRANSPARENT
                        })
                        .stroke(egui::Stroke::new(1.0_f32, colors().border))
                        .corner_radius(8);
                        if ui
                            .add(guided_pill)
                            .on_hover_text(tr(
                                "Ask the AI for a step-by-step lyric code-along with dependency-safe file order.",
                            ))
                            .clicked()
                        {
                            self.guided.armed = !self.guided.armed;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let can_send = !self.chat.prompt.trim().is_empty();
                            let send_btn = egui::Button::new(
                                RichText::new("↑").strong().color(if can_send {
                                    colors().canvas
                                } else {
                                    colors().muted
                                }),
                            )
                            .fill(if can_send {
                                colors().accent
                            } else {
                                colors().elevated
                            })
                            .stroke(egui::Stroke::NONE)
                            .corner_radius(14)
                            .min_size(egui::vec2(28.0, 28.0));
                            if ui.add_enabled(can_send, send_btn).clicked() {
                                send_prompt_now = true;
                            }
                            // Separate Effort selector pill.
                            let effort_label = tr(&self.hub.agents.chat.effort);
                            ui.menu_button(
                                RichText::new(effort_label)
                                    .size(SMALL)
                                    .color(colors().secondary),
                                |ui| {
                                    ui.label(muted(tr("Thinking effort")));
                                    for level in ["Low", "Medium", "High", "Extra High"] {
                                        let active = self.hub.agents.chat.effort == level;
                                        if ui.selectable_label(active, tr(level)).clicked() {
                                            self.hub.agents.chat.effort = level.to_owned();
                                            ui.close();
                                        }
                                    }
                                },
                            );
                            // Full Model picker with official provider marks and per-model Thinking effort submenu.
                            let model_text = self
                                .hub
                                .agents
                                .chat
                                .model
                                .as_deref()
                                .map(|m| {
                                    super::agent_chat::curated_model_label(m)
                                        .unwrap_or(m)
                                        .to_owned()
                                })
                                .unwrap_or_else(|| tr("Model").to_owned());
                            ui.menu_button(
                                RichText::new(model_text)
                                    .size(SMALL)
                                    .color(colors().text),
                                |ui| {
                                    ui.set_min_width(250.0);
                                    for (kind, brand_provider, title) in [
                                        (
                                            covalos_agents::ProviderKind::ClaudeCode,
                                            crate::assistant::Provider::Claude,
                                            "Claude",
                                        ),
                                        (
                                            covalos_agents::ProviderKind::Gemini,
                                            crate::assistant::Provider::Gemini,
                                            "Gemini",
                                        ),
                                        (
                                            covalos_agents::ProviderKind::Codex,
                                            crate::assistant::Provider::Codex,
                                            "ChatGPT",
                                        ),
                                    ] {
                                        let is_gemini =
                                            kind == covalos_agents::ProviderKind::Gemini;
                                        ui.horizontal(|ui| {
                                            let (mrect, _) = ui.allocate_exact_size(
                                                egui::vec2(15.0, 15.0),
                                                egui::Sense::hover(),
                                            );
                                            super::brands::mark(
                                                ui.painter(),
                                                mrect,
                                                brand_provider,
                                            );
                                            ui.label(
                                                RichText::new(title)
                                                    .size(SMALL)
                                                    .strong()
                                                    .color(colors().secondary),
                                            );
                                            if is_gemini {
                                                ui.label(
                                                    RichText::new(tr("Coming soon"))
                                                        .size(10.5)
                                                        .color(colors().muted),
                                                );
                                            }
                                        });
                                        for (id, display) in
                                            super::agent_chat::curated_cli_models(&kind)
                                        {
                                            let is_selected = !is_gemini
                                                && self
                                                    .hub
                                                    .agents
                                                    .chat
                                                    .model
                                                    .as_deref()
                                                    == Some(*id);
                                            let row_label = if is_selected {
                                                format!(
                                                    "•  {display}   {}",
                                                    tr(&self.hub.agents.chat.effort)
                                                )
                                            } else {
                                                format!("   {display}")
                                            };
                                            if is_gemini {
                                                if ui
                                                    .button(
                                                        RichText::new(row_label)
                                                            .color(colors().muted),
                                                    )
                                                    .clicked()
                                                {
                                                    self.notice = Some((
                                                        true,
                                                        tr(
                                                            "Gemini CLI chat is coming soon. Use Codex or Claude Code for now.",
                                                        )
                                                        .to_owned(),
                                                    ));
                                                    ui.close();
                                                }
                                                continue;
                                            }
                                            ui.menu_button(
                                                RichText::new(row_label).color(if is_selected {
                                                    colors().accent
                                                } else {
                                                    colors().text
                                                }),
                                                |ui| {
                                                    ui.set_min_width(140.0);
                                                    ui.label(muted(tr("Thinking effort")));
                                                    for level in
                                                        ["Low", "Medium", "High", "Extra High"]
                                                    {
                                                        let active = is_selected
                                                            && self.hub.agents.chat.effort == level;
                                                        let text = if active {
                                                            format!("•  {}", tr(level))
                                                        } else {
                                                            format!("   {}", tr(level))
                                                        };
                                                        if ui
                                                            .button(RichText::new(text).color(
                                                                if active {
                                                                    colors().accent
                                                                } else {
                                                                    colors().text
                                                                },
                                                            ))
                                                            .clicked()
                                                        {
                                                            self.hub.agents.chat.model =
                                                                Some((*id).to_owned());
                                                            self.hub.agents.chat.effort =
                                                                level.to_owned();
                                                            if let Some((acc_id, _, _)) = accounts
                                                                .iter()
                                                                .find(|(_, _, p)| *p == kind)
                                                            {
                                                                self.hub.agents.chat.target =
                                                                    super::agent_chat::Target::Account(
                                                                        acc_id.clone(),
                                                                    );
                                                            }
                                                            ui.close();
                                                        }
                                                    }
                                                },
                                            );
                                        }
                                        ui.add_space(6.0);
                                    }
                                    if !accounts.is_empty() {
                                        ui.separator();
                                        ui.label(muted(tr("Accounts")));
                                        for (acc_id, label, _) in &accounts {
                                            if ui.button(label).clicked() {
                                                self.hub.agents.chat.target =
                                                    super::agent_chat::Target::Account(
                                                        acc_id.clone(),
                                                    );
                                                ui.close();
                                            }
                                        }
                                    }
                                    ui.separator();
                                    if ui.button(tr("View usage")).clicked() {
                                        usage_open = true;
                                        ui.close();
                                    }
                                    if ui.button(tr("+ Add AI")).clicked() {
                                        open_add_ai = true;
                                        ui.close();
                                    }
                                },
                            );
                        });
                    });
                });
            // Compact 1-line telemetry bar right under the composer (Ctx + Token usage button).
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(muted("Ctx"));
                let (bar_rect, _) =
                    ui.allocate_exact_size(egui::vec2(48.0, 6.0), egui::Sense::hover());
                ui.painter()
                    .rect_filled(bar_rect, 3, colors().elevated);
                let ratio = ((total_in + total_out) as f32 / 1_000_000.0).clamp(0.0, 1.0);
                if ratio > 0.005 {
                    let fill_rect = egui::Rect::from_min_size(
                        bar_rect.min,
                        egui::vec2((bar_rect.width() * ratio).max(4.0), bar_rect.height()),
                    );
                    ui.painter().rect_filled(fill_rect, 3, colors().accent);
                }
                ui.label(muted(format!(
                    "{} · {} {}",
                    format_tokens(total_in + total_out),
                    total_turns,
                    tr("turns")
                )));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(egui::Button::new(muted(tr("View usage"))).frame(false))
                        .clicked()
                    {
                        usage_open = true;
                    }
                });
            });
            ui.add_space(12.0);
            // Minimalist quick-start rows.
            for (icon, color, label) in [
                (Icon::Compare, colors().accent, "Review my changes"),
                (Icon::Search, colors().success, "Plan an implementation"),
                (Icon::Terminal, colors().warning, "Find and fix a bug"),
                (Icon::Book, colors().error, "Write tests for a change"),
            ] {
                let (row_rect, resp) = ui.allocate_exact_size(
                    egui::vec2(width, 30.0),
                    egui::Sense::click(),
                );
                if resp.hovered() {
                    ui.painter().rect_filled(row_rect, 7, colors().hover);
                }
                draw_icon_sized(
                    ui.painter(),
                    row_rect.left_center() + egui::vec2(14.0, 0.0),
                    icon,
                    color,
                    14.0,
                );
                ui.painter().text(
                    row_rect.left_center() + egui::vec2(32.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    tr(label),
                    egui::FontId::proportional(BODY),
                    if resp.hovered() {
                        colors().text
                    } else {
                        colors().secondary
                    },
                );
                if resp.clicked() {
                    quick_action = Some(label);
                }
            }
        });
        ui.ctx().data_mut(|d| {
            d.insert_temp(auto_skill_id, auto_skill);
            d.insert_temp(usage_modal_id, usage_open);
        });
        if usage_open {
            self.agent_usage_modal(ui.ctx(), usage_modal_id);
        }
        if open_add_ai {
            self.hub.agents.chat.add_ai = Some(Default::default());
        }
        if let Some(action) = quick_action {
            match action {
                "Review my changes" => {
                    self.sidebar = super::Sidebar::Git;
                    self.sidebar_open = true;
                    self.git_refresh(ui.ctx(), None);
                }
                other => {
                    self.chat.prompt = format!("{}: ", tr(other));
                    self.preferences.assistant_open = true;
                }
            }
        }
        if send_prompt_now {
            let trimmed = self.chat.prompt.trim().to_owned();
            // Handle CLI-style /model and /effort slash commands right in the composer.
            if let Some(model) = trimmed.strip_prefix("/model").map(str::trim)
                && !model.is_empty()
                && !model.starts_with('-')
            {
                self.hub.agents.chat.model = Some(model.to_owned());
                self.chat.prompt.clear();
                self.notice = Some((false, format!("Model: {model}")));
                return;
            }
            if let Some(effort) = trimmed.strip_prefix("/effort").map(str::trim) {
                let normalized = match effort.to_ascii_lowercase().as_str() {
                    "low" => Some("Low"),
                    "medium" => Some("Medium"),
                    "high" => Some("High"),
                    "extra" | "extra high" | "extra_high" | "max" => Some("Extra High"),
                    _ => None,
                };
                if let Some(level) = normalized {
                    self.hub.agents.chat.effort = level.to_owned();
                    self.chat.prompt.clear();
                    self.notice = Some((false, format!("Effort: {level}")));
                    return;
                }
            }
            // If user pasted a GitHub SKILL.md URL, import it directly through the safe skill importer.
            if let Some(url) = trimmed
                .split_whitespace()
                .find(|w| w.starts_with("https://github.com/") && w.ends_with("/SKILL.md"))
            {
                self.skills.start_import_url(url, ui.ctx());
                self.chat.prompt.clear();
                self.notice = Some((false, tr("Importing skill from GitHub…").to_owned()));
                return;
            }
            if auto_skill {
                self.skills
                    .auto_select_for_prompt(&self.preferences.skills, &trimmed);
            }
            let outgoing = if self.guided.armed {
                super::guided::build_guided_prompt(&trimmed, self.preferences.guided.flow_order)
            } else {
                trimmed
            };
            if let Some(session_id) = self.start_chat_from_home(ui.ctx(), &outgoing) {
                if self.guided.armed {
                    self.guided.watching_session = Some(session_id);
                }
                self.chat.prompt.clear();
            }
        }
    }

    /// Real local token usage dashboard (Input/Output tokens, Sessions, Models & Agents breakdown).
    fn agent_usage_modal(&mut self, ctx: &egui::Context, open_id: Id) {
        let width = (ctx.content_rect().width() - 96.0).clamp(520.0, 740.0);
        let mut close_now = false;
        let area = egui::Modal::default_area(Id::new("agent-usage-modal")).anchor(
            egui::Align2::CENTER_CENTER,
            egui::vec2(0.0, 0.0),
        );
        let modal = egui::Modal::new(Id::new("agent-usage-modal"))
            .backdrop_color(egui::Color32::from_black_alpha(150))
            .frame(
                egui::Frame::new()
                    .fill(colors().surface)
                    .corner_radius(MODAL_RADIUS)
                    .stroke(egui::Stroke::new(1.0_f32, colors().control_border))
                    .inner_margin(egui::Margin::same(18)),
            )
            .area(area)
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tr("Agent usage")).size(HEADING).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icon_button(ui, Icon::Close, "Close", false).clicked() {
                            close_now = true;
                        }
                    });
                });
                ui.add_space(12.0);
                let mut total_in = 0u64;
                let mut total_out = 0u64;
                let mut total_turns = 0u32;
                let mut total_sessions = 0usize;
                let mut by_model: std::collections::BTreeMap<String, (u64, u64, u32)> =
                    Default::default();
                let mut by_agent: std::collections::BTreeMap<&'static str, (u64, u64, u32)> =
                    Default::default();
                if let Some(rt) = &self.hub.agents.runtime {
                    total_sessions = rt.sessions.len();
                    for session in &rt.sessions {
                        let m = &session.meta;
                        total_in = total_in.saturating_add(m.input_tokens);
                        total_out = total_out.saturating_add(m.output_tokens);
                        total_turns = total_turns.saturating_add(m.turns);
                        let model_name = m
                            .model
                            .clone()
                            .unwrap_or_else(|| "default".to_owned());
                        let entry = by_model.entry(model_name).or_default();
                        entry.0 += m.input_tokens;
                        entry.1 += m.output_tokens;
                        entry.2 += m.turns;
                        let agent_name = super::agent_chat::provider_name(&m.provider);
                        let a_entry = by_agent.entry(agent_name).or_default();
                        a_entry.0 += m.input_tokens;
                        a_entry.1 += m.output_tokens;
                        a_entry.2 += m.turns;
                    }
                }
                // 4 KPI cards.
                let card_w = (width - 24.0) / 4.0;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    for (label, value, detail) in [
                        (
                            tr("Input tokens"),
                            format_tokens(total_in),
                            tr("Session total"),
                        ),
                        (
                            tr("Output tokens"),
                            format_tokens(total_out),
                            tr("Session total"),
                        ),
                        (
                            tr("Sessions"),
                            total_sessions.to_string(),
                            tr("Local workspaces"),
                        ),
                        (
                            tr("Turns"),
                            total_turns.to_string(),
                            tr("Completed turns"),
                        ),
                    ] {
                        egui::Frame::new()
                            .fill(colors().inset)
                            .corner_radius(8)
                            .stroke(egui::Stroke::new(1.0_f32, colors().border))
                            .inner_margin(egui::Margin::same(10))
                            .show(ui, |ui| {
                                ui.set_width((card_w - 20.0).max(90.0));
                                ui.label(muted(label));
                                ui.add_space(4.0);
                                ui.label(RichText::new(value).size(HEADING).strong());
                                ui.label(muted(detail));
                            });
                    }
                });
                ui.add_space(14.0);
                if total_in + total_out == 0 && total_turns == 0 {
                    ui.vertical_centered(|ui| {
                        ui.add_space(14.0);
                        ui.label(muted(tr("Start a conversation to see token usage.")));
                        ui.add_space(14.0);
                    });
                    return;
                }
                let max_tokens = (total_in + total_out).max(1) as f32;
                let col_w = (width - 16.0) * 0.5;
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 16.0;
                    ui.allocate_ui_with_layout(
                        egui::vec2(col_w, 180.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.label(RichText::new(tr("Models")).strong());
                            ui.add_space(6.0);
                            for (name, (inp, out, turns)) in &by_model {
                                usage_bar_row(
                                    ui,
                                    name,
                                    *inp + *out,
                                    max_tokens,
                                    *turns,
                                    colors().accent,
                                );
                            }
                        },
                    );
                    ui.allocate_ui_with_layout(
                        egui::vec2(col_w, 180.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.label(RichText::new(tr("Agents")).strong());
                            ui.add_space(6.0);
                            for (name, (inp, out, turns)) in &by_agent {
                                usage_bar_row(
                                    ui,
                                    name,
                                    *inp + *out,
                                    max_tokens,
                                    *turns,
                                    colors().success,
                                );
                            }
                        },
                    );
                });
            });
        if close_now || modal.should_close() {
            ctx.data_mut(|d| d.insert_temp(open_id, false));
        }
    }

    fn initial_setup_card(&mut self, ui: &mut Ui) {
        let old = (
            self.preferences.theme,
            self.preferences.font_size,
            self.preferences.high_contrast,
            self.preferences.brightness.to_bits(),
        );
        egui::Frame::new()
            .fill(colors().surface.gamma_multiply(0.92))
            .corner_radius(10)
            .stroke(egui::Stroke::new(1.0_f32, colors().border))
            .inner_margin(egui::Margin::same(16))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tr("Initial setup")).size(LEAD).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(egui::Button::new(muted(tr("Done"))).frame(false))
                            .clicked()
                        {
                            self.preferences.setup_completed = true;
                        }
                    });
                });
                ui.label(muted(tr("Choose a few defaults before you start.")));
                ui.add_space(10.0);
                setting_row(
                    ui,
                    "Import settings from another editor",
                    "Import matching setup from VS Code, Cursor, Windsurf, or Zed.",
                    |ui| {
                        self.import_editor_menu(ui);
                    },
                );
                setting_row(
                    ui,
                    "Surface brightness",
                    "Adjust background clarity across chrome, Explorer, and editor.",
                    |ui| {
                        ui.add(
                            egui::Slider::new(&mut self.preferences.brightness, 0.0..=1.0)
                                .step_by(0.05)
                                .show_value(false),
                        );
                    },
                );
                setting_row(
                    ui,
                    "Editor density",
                    "Use tighter spacing across code and lists.",
                    |ui| {
                        checkbox(ui, &mut self.preferences.compact, tr("Compact"));
                    },
                );
                setting_row(
                    ui,
                    "Reduce motion",
                    "Disable animations and ambient transitions.",
                    |ui| {
                        checkbox(ui, &mut self.preferences.reduce_motion, tr("Reduce motion"));
                    },
                );
            });
        if old
            != (
                self.preferences.theme,
                self.preferences.font_size,
                self.preferences.high_contrast,
                self.preferences.brightness.to_bits(),
            )
        {
            set_brightness(self.preferences.brightness);
            install(
                ui.ctx(),
                self.preferences.font_size,
                self.preferences.high_contrast,
                self.preferences.theme,
            );
            self.syntax =
                egui_extras::syntax_highlighting::CodeTheme::dark(self.preferences.font_size);
        }
    }

    fn import_editor_menu(&mut self, ui: &mut Ui) {
        ui.menu_button(tr("Import"), |ui| {
            for editor in super::preferences::ExternalEditor::ALL {
                if ui.button(editor.name()).clicked() {
                    ui.close();
                    match self.preferences.import_from(editor) {
                        Ok(fields) => {
                            let msg = format!(
                                "Imported {} from {}.",
                                fields.join(", "),
                                editor.name()
                            );
                            self.notice = Some((false, msg.clone()));
                            self.notify(NotificationSource::Covalos, msg, false);
                            install(
                                ui.ctx(),
                                self.preferences.font_size,
                                self.preferences.high_contrast,
                                self.preferences.theme,
                            );
                            self.syntax = egui_extras::syntax_highlighting::CodeTheme::dark(
                                self.preferences.font_size,
                            );
                        }
                        Err(err) => {
                            self.error(err);
                        }
                    }
                }
            }
        });
    }

    pub(super) fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let width = (ctx.content_rect().width() - 80.0).clamp(520.0, 780.0);
        let height = (ctx.content_rect().height() - 100.0).clamp(360.0, 600.0);
        let tab_id = Id::new("settings-active-section");
        let query_id = Id::new("settings-search-query");
        let mut active_tab: SettingsSection = ctx
            .data(|d| d.get_temp(tab_id))
            .unwrap_or(SettingsSection::General);
        let mut search: String = ctx.data(|d| d.get_temp(query_id)).unwrap_or_default();

        let mut open = self.settings_open;
        let mut open_add_ai_from_settings = false;
        let mut open_rules_file: Option<PathBuf> = None;
        let rules_id = Id::new("settings-agent-rules-draft");
        let rules_loaded_id = Id::new("settings-agent-rules-loaded");
        let mut rules_draft: String = if ctx.data(|d| d.get_temp::<bool>(rules_loaded_id)).unwrap_or(false) {
            ctx.data(|d| d.get_temp(rules_id)).unwrap_or_default()
        } else {
            let loaded = super::preferences::Preferences::load_agent_rules();
            ctx.data_mut(|d| {
                d.insert_temp(rules_loaded_id, true);
                d.insert_temp(rules_id, loaded.clone());
            });
            loaded
        };
        let area = egui::Modal::default_area(Id::new("settings-modal")).anchor(
            egui::Align2::CENTER_CENTER,
            egui::vec2(0.0, 0.0),
        );
        let modal = egui::Modal::new(Id::new("settings-modal"))
            .backdrop_color(egui::Color32::from_black_alpha(150))
            .frame(
                egui::Frame::new()
                    .fill(colors().surface)
                    .corner_radius(MODAL_RADIUS)
                    .stroke(egui::Stroke::new(1.0_f32, colors().control_border))
                    .shadow(egui::Shadow {
                        offset: [0, 12],
                        blur: 32,
                        spread: 0,
                        color: egui::Color32::from_black_alpha(140),
                    })
                    .inner_margin(egui::Margin::same(14)),
            )
            .area(area)
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.set_height(height);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tr("Settings")).size(LEAD).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icon_button(ui, Icon::Close, "Close", false).clicked() {
                            open = false;
                        }
                    });
                });
                ui.add_space(6.0);
                let old = (
                    self.preferences.theme,
                    self.preferences.font_size,
                    self.preferences.high_contrast,
                    self.preferences.brightness.to_bits(),
                );
                let nav_w = 176.0;
                let content_h = (height - 46.0).max(220.0);
                ui.horizontal_top(|ui| {
                    // Left navigation column with search and clean sections.
                    ui.allocate_ui_with_layout(
                        egui::vec2(nav_w, content_h),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.add(
                                TextEdit::singleline(&mut search)
                                    .hint_text(tr("Search settings"))
                                    .desired_width(nav_w - 8.0)
                                    .char_limit(80),
                            );
                            ui.add_space(10.0);
                            let needle = search.trim().to_ascii_lowercase();
                            for item in SettingsSection::ALL {
                                if !needle.is_empty() && !item.matches(&needle) {
                                    continue;
                                }
                                let selected = active_tab == item;
                                let (rect, response) = ui.allocate_exact_size(
                                    egui::vec2(nav_w - 8.0, 28.0),
                                    egui::Sense::click(),
                                );
                                if selected || response.hovered() {
                                    ui.painter().rect_filled(
                                        rect,
                                        6,
                                        if selected {
                                            colors().elevated
                                        } else {
                                            colors().hover
                                        },
                                    );
                                }
                                draw_icon_sized(
                                    ui.painter(),
                                    rect.left_center() + egui::vec2(14.0, 0.0),
                                    item.icon(),
                                    if selected {
                                        colors().text
                                    } else {
                                        colors().muted
                                    },
                                    14.0,
                                );
                                ui.painter().text(
                                    rect.left_center() + egui::vec2(28.0, 0.0),
                                    egui::Align2::LEFT_CENTER,
                                    tr(item.label()),
                                    egui::FontId::proportional(BODY),
                                    if selected {
                                        colors().text
                                    } else {
                                        colors().secondary
                                    },
                                );
                                if response.clicked() {
                                    active_tab = item;
                                }
                            }
                        },
                    );
                    ui.add_space(6.0);
                    // Right minimalist content column (explicit top-down layout inside a defined card).
                    let body_w = (ui.available_width() - 4.0).max(260.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(body_w, content_h),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            egui::Frame::new()
                                .fill(colors().inset)
                                .corner_radius(8)
                                .stroke(egui::Stroke::new(1.0_f32, colors().border))
                                .inner_margin(egui::Margin::same(16))
                                .show(ui, |ui| {
                                    ui.set_min_size(egui::vec2(
                                        (body_w - 32.0).max(220.0),
                                        (content_h - 32.0).max(160.0),
                                    ));
                                    ScrollArea::vertical()
                                        .id_salt("settings-body")
                                        .auto_shrink([false, false])
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            ui.label(
                                                RichText::new(tr(active_tab.label()))
                                                    .size(HEADING)
                                                    .strong(),
                                            );
                                            ui.add_space(12.0);
                                            match active_tab {
                                SettingsSection::General => {
                                    setting_row(
                                        ui,
                                        "Version",
                                        &format!(
                                            "{} · build {}",
                                            crate::brand::RELEASE,
                                            crate::brand::VERSION
                                        ),
                                        |ui| {
                                            if ui.button(tr("About Covalos")).clicked() {
                                                self.about_open = true;
                                            }
                                        },
                                    );
                                    setting_row(
                                        ui,
                                        "Language",
                                        "Switch interface language live.",
                                        |ui| {
                                            let response = egui::ComboBox::from_id_salt(
                                                "appearance-language",
                                            )
                                            .selected_text(self.preferences.language.native_name())
                                            .show_ui(ui, |ui| {
                                                for language in super::i18n::Language::ALL {
                                                    let option = ui.selectable_value(
                                                        &mut self.preferences.language,
                                                        language,
                                                        language.native_name(),
                                                    );
                                                    #[cfg(test)]
                                                    super::motion::record(
                                                        ui,
                                                        format!(
                                                            "Language {}",
                                                            language.native_name()
                                                        ),
                                                        option.rect,
                                                    );
                                                    let _ = option;
                                                }
                                            });
                                            #[cfg(test)]
                                            super::motion::record(
                                                ui,
                                                "Language selector",
                                                response.response.rect,
                                            );
                                            let _ = response;
                                        },
                                    );
                                    setting_row(
                                        ui,
                                        "Import from another editor",
                                        "Import font size, theme, and density from VS Code, Cursor, Windsurf, or Zed.",
                                        |ui| {
                                            self.import_editor_menu(ui);
                                        },
                                    );
                                    setting_row(
                                        ui,
                                        "Initial setup",
                                        "Show the quick setup checklist on the Welcome view.",
                                        |ui| {
                                            let label = if self.preferences.setup_completed {
                                                tr("Show again")
                                            } else {
                                                tr("Visible")
                                            };
                                            if ui
                                                .add_enabled(
                                                    self.preferences.setup_completed,
                                                    egui::Button::new(label),
                                                )
                                                .clicked()
                                            {
                                                self.preferences.setup_completed = false;
                                            }
                                        },
                                    );
                                    setting_row(
                                        ui,
                                        "Library Studio shortcut",
                                        "Keyboard shortcut to open Library Studio.",
                                        |ui| {
                                            let valid = super::panels::parse_shortcut(
                                                &self.preferences.library_shortcut,
                                            )
                                            .is_some();
                                            ui.add(
                                                egui::TextEdit::singleline(
                                                    &mut self.preferences.library_shortcut,
                                                )
                                                .desired_width(120.0)
                                                .text_color(if valid {
                                                    colors().text
                                                } else {
                                                    colors().error
                                                }),
                                            );
                                        },
                                    );
                                }
                                SettingsSection::Appearance => {
                                    setting_row(
                                        ui,
                                        "Theme",
                                        "Tune the workspace for long editing sessions.",
                                        |ui| {
                                            egui::ComboBox::from_id_salt("appearance-theme")
                                                .selected_text(tr(self.preferences.theme.name()))
                                                .show_ui(ui, |ui| {
                                                    for theme in Theme::ALL {
                                                        ui.selectable_value(
                                                            &mut self.preferences.theme,
                                                            theme,
                                                            tr(theme.name()),
                                                        );
                                                    }
                                                });
                                        },
                                    );
                                    ui.add_space(6.0);
                                    ui.horizontal_wrapped(|ui| {
                                        for theme in Theme::ALL {
                                            let selected = self.preferences.theme == theme;
                                            let response = ui.add_sized(
                                                [72.0, 42.0],
                                                egui::Button::new("")
                                                    .fill(theme.colors().canvas)
                                                    .stroke(egui::Stroke::new(
                                                        if selected { 2.0_f32 } else { 1.0_f32 },
                                                        if selected {
                                                            colors().accent
                                                        } else {
                                                            colors().border
                                                        },
                                                    )),
                                            );
                                            let p = ui.painter_at(response.rect);
                                            p.rect_filled(
                                                egui::Rect::from_min_size(
                                                    response.rect.min + egui::vec2(8.0, 9.0),
                                                    egui::vec2(8.0, 23.0),
                                                ),
                                                2,
                                                theme.colors().surface,
                                            );
                                            for (y, w, c) in [
                                                (10.0, 34.0, theme.colors().accent),
                                                (17.0, 42.0, theme.colors().secondary),
                                                (24.0, 28.0, theme.colors().syntax_string),
                                            ] {
                                                p.line_segment(
                                                    [
                                                        response.rect.min + egui::vec2(22.0, y),
                                                        response.rect.min + egui::vec2(22.0 + w, y),
                                                    ],
                                                    egui::Stroke::new(2.0_f32, c),
                                                );
                                            }
                                            #[cfg(test)]
                                            super::motion::record(
                                                ui,
                                                format!("Theme {}", theme.name()),
                                                response.rect,
                                            );
                                            if response.on_hover_text(theme.name()).clicked() {
                                                self.preferences.theme = theme;
                                            }
                                        }
                                    });
                                    ui.add_space(10.0);
                                    ui.add(
                                        egui::Slider::new(
                                            &mut self.preferences.brightness,
                                            0.0..=1.0,
                                        )
                                        .step_by(0.05)
                                        .text(tr("Surface brightness")),
                                    );
                                    ui.add(
                                        egui::Slider::new(&mut self.preferences.motion, 0.0..=1.0)
                                            .step_by(0.05)
                                            .text(tr("Motion intensity")),
                                    );
                                    ui.add(
                                        egui::Slider::new(
                                            &mut self.preferences.explorer_zoom,
                                            super::preferences::EXPLORER_ZOOM.0
                                                ..=super::preferences::EXPLORER_ZOOM.1,
                                        )
                                        .step_by(0.05)
                                        .text(tr("Explorer zoom")),
                                    );
                                    ui.add(
                                        egui::Slider::new(
                                            &mut self.preferences.ui_scale,
                                            super::preferences::UI_SCALE.0
                                                ..=super::preferences::UI_SCALE.1,
                                        )
                                        .step_by(0.05)
                                        .text(tr("Interface scale")),
                                    );
                                    ui.horizontal_wrapped(|ui| {
                                        if ui.button(tr("Reset Explorer zoom")).clicked() {
                                            self.preferences.explorer_zoom = 1.0;
                                        }
                                        if ui.button(tr("Reset interface scale")).clicked() {
                                            self.preferences.ui_scale = 1.0;
                                        }
                                    });
                                    ui.add_space(6.0);
                                    ui.horizontal(|ui| {
                                        ui.label(secondary(tr("Navigation transitions")));
                                        for t in super::preferences::Transitions::ALL {
                                            ui.selectable_value(
                                                &mut self.preferences.transitions,
                                                t,
                                                tr(t.name()),
                                            );
                                        }
                                    });
                                    checkbox(
                                        ui,
                                        &mut self.preferences.high_contrast,
                                        tr("High contrast controls"),
                                    );
                                    checkbox(
                                        ui,
                                        &mut self.preferences.reduce_motion,
                                        tr("Reduce motion"),
                                    );
                                }
                                SettingsSection::Background => {
                                    let prefs = &mut self.preferences.identity;
                                    ui.horizontal(|ui| {
                                        ui.label(secondary(tr("Preset")));
                                        let selector = egui::ComboBox::from_id_salt(
                                            "background-preset",
                                        )
                                        .selected_text(prefs.background.name())
                                        .show_ui(ui, |ui| {
                                            for preset in super::identity::Background::ALL {
                                                let response = ui.selectable_value(
                                                    &mut prefs.background,
                                                    preset,
                                                    tr(preset.name()),
                                                );
                                                if response.clicked() {
                                                    prefs.background_chosen = true;
                                                }
                                                #[cfg(test)]
                                                super::motion::record(
                                                    ui,
                                                    format!("Background {}", preset.name()),
                                                    response.rect,
                                                );
                                                let _ = response;
                                            }
                                        });
                                        #[cfg(test)]
                                        super::motion::record(
                                            ui,
                                            "Background selector",
                                            selector.response.rect,
                                        );
                                        let _ = selector;
                                    });
                                    let (_, preview) =
                                        ui.allocate_space(egui::vec2(ui.available_width(), 96.0));
                                    ui.painter().rect_filled(preview, 6, colors().canvas);
                                    self.identity.paint(
                                        ui,
                                        preview,
                                        prefs,
                                        super::identity::Mood::Idle,
                                    );
                                    if prefs.background == super::identity::Background::Ascii {
                                        egui::ComboBox::from_id_salt("ascii-pattern")
                                            .selected_text(prefs.pattern.name())
                                            .show_ui(ui, |ui| {
                                                for pattern in super::identity::Pattern::ALL {
                                                    ui.selectable_value(
                                                        &mut prefs.pattern,
                                                        pattern,
                                                        pattern.name(),
                                                    );
                                                }
                                            });
                                    }
                                    checkbox(ui, &mut prefs.animated, tr("Animate backgrounds"));
                                    checkbox(ui, &mut prefs.interactive, tr("Pointer reactions"));
                                    ui.add(
                                        egui::Slider::new(&mut prefs.fps, 5..=60)
                                            .text(tr("Ambient FPS"))
                                            .suffix(" fps"),
                                    );
                                    ui.add(
                                        egui::Slider::new(&mut prefs.speed, 0.1..=2.0)
                                            .text(tr("Speed")),
                                    );
                                    ui.add(
                                        egui::Slider::new(&mut prefs.density, 0.1..=1.0)
                                            .text(tr("Density")),
                                    );
                                    ui.add(
                                        egui::Slider::new(&mut prefs.intensity, 0.02..=0.4)
                                            .text(tr("Intensity")),
                                    );
                                    ui.horizontal(|ui| {
                                        checkbox(ui, &mut prefs.custom_tint, tr("Custom color"));
                                        if prefs.custom_tint {
                                            ui.color_edit_button_srgb(&mut prefs.tint);
                                        }
                                    });
                                    checkbox(
                                        ui,
                                        &mut prefs.behind_code,
                                        tr("Also show behind code"),
                                    )
                                    .on_hover_text(tr(
                                        "Off by default. Keep the code area quiet.",
                                    ));
                                    ui.add_space(10.0);
                                    egui::CollapsingHeader::new(tr("Luma companion")).show(
                                        ui,
                                        |ui| {
                                            checkbox(
                                                ui,
                                                &mut prefs.mascot,
                                                tr("Show Luma in workspace"),
                                            );
                                            checkbox(
                                                ui,
                                                &mut prefs.mascot_motion,
                                                tr("Animate Luma"),
                                            );
                                            ui.add(
                                                egui::Slider::new(
                                                    &mut prefs.mascot_intensity,
                                                    0.0..=1.0,
                                                )
                                                .text(tr("Motion intensity")),
                                            );
                                            ui.add(
                                                egui::Slider::new(
                                                    &mut prefs.mascot_size,
                                                    40.0..=80.0,
                                                )
                                                .text(tr("Size"))
                                                .suffix(" pt"),
                                            );
                                        },
                                    );
                                }
                                SettingsSection::Editor => {
                                    setting_row(
                                        ui,
                                        "Code colors",
                                        "Syntax palette independent of the UI theme.",
                                        |ui| {
                                            egui::ComboBox::from_id_salt("syntax-style")
                                                .selected_text(tr(self.preferences.syntax.name()))
                                                .show_ui(ui, |ui| {
                                                    for style in super::syntax::SyntaxStyle::ALL {
                                                        ui.selectable_value(
                                                            &mut self.preferences.syntax,
                                                            style,
                                                            tr(style.name()),
                                                        );
                                                    }
                                                });
                                        },
                                    );
                                    super::syntax::set_style(self.preferences.syntax);
                                    let preview = "fn orbit(r: f32) -> Vec2 {\n    let angle = TAU * 0.25; // quarter\n    vec2(r * angle.cos(), 42.0)\n}";
                                    let size = self.preferences.font_size.min(15.0);
                                    let line = size
                                        * if self.preferences.compact {
                                            COMPACT_CODE_LINE_HEIGHT
                                        } else {
                                            CODE_LINE_HEIGHT
                                        };
                                    egui::Frame::new()
                                        .fill(colors().inset)
                                        .corner_radius(8)
                                        .stroke(egui::Stroke::new(1.0_f32, colors().border))
                                        .inner_margin(egui::Margin::symmetric(12, 10))
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            let job = super::syntax::highlight(
                                                ui.ctx(),
                                                preview,
                                                "rs",
                                                size,
                                                line,
                                            );
                                            ui.add(egui::Label::new(job).extend());
                                        });
                                    ui.add_space(10.0);
                                    ui.add(
                                        egui::Slider::new(
                                            &mut self.preferences.font_size,
                                            11.0..=26.0,
                                        )
                                        .step_by(1.0)
                                        .text(tr("Editor font size"))
                                        .suffix(" pt"),
                                    );
                                    ui.horizontal(|ui| {
                                        ui.label(secondary(tr("Editor density")));
                                        ui.selectable_value(
                                            &mut self.preferences.compact,
                                            true,
                                            tr("Compact"),
                                        );
                                        ui.selectable_value(
                                            &mut self.preferences.compact,
                                            false,
                                            tr("Comfortable"),
                                        );
                                        if ui.button(tr("Reset editor zoom")).clicked() {
                                            self.preferences.font_size = 14.0;
                                        }
                                    });
                                    ui.add(
                                        egui::Label::new(muted(
                                            "Ctrl + wheel or Ctrl +/− zooms the panel under the pointer; Ctrl+0 resets it.",
                                        ))
                                        .wrap(),
                                    );
                                    ui.add_space(12.0);
                                    ui.separator();
                                    ui.add_space(8.0);
                                    ui.label(
                                        RichText::new(tr("Guided Mode"))
                                            .size(LEAD)
                                            .strong()
                                            .color(colors().text),
                                    );
                                    ui.add_space(4.0);
                                    ui.horizontal(|ui| {
                                        ui.label(secondary(tr("Step order")));
                                        ui.selectable_value(
                                            &mut self.preferences.guided.flow_order,
                                            super::guided::FlowOrder::Natural,
                                            tr("Natural (Just-in-Time)"),
                                        );
                                        ui.selectable_value(
                                            &mut self.preferences.guided.flow_order,
                                            super::guided::FlowOrder::BottomUp,
                                            tr("Dependencies first"),
                                        );
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label(secondary(tr("Line matching")));
                                        ui.selectable_value(
                                            &mut self.preferences.guided.match_mode,
                                            super::guided::MatchMode::Flexible,
                                            tr("Flexible"),
                                        );
                                        ui.selectable_value(
                                            &mut self.preferences.guided.match_mode,
                                            super::guided::MatchMode::Strict,
                                            tr("Strict"),
                                        );
                                    });
                                    checkbox(
                                        ui,
                                        &mut self.preferences.guided.show_why,
                                        tr("Show step explanations"),
                                    );
                                }
                                SettingsSection::Notifications => {
                                    setting_row(
                                        ui,
                                        "Quiet agent notifications",
                                        "Suppress taskbar attention when agents finish.",
                                        |ui| {
                                            checkbox(
                                                ui,
                                                &mut self.preferences.quiet_notifications,
                                                "",
                                            );
                                        },
                                    );
                                    ui.add(
                                        egui::Slider::new(
                                            &mut self.preferences.notification_interval,
                                            5..=120,
                                        )
                                        .text(tr("Taskbar attention interval"))
                                        .suffix(" s"),
                                    );
                                }
                                SettingsSection::Agents => {
                                    setting_row(
                                        ui,
                                        "Connected accounts & CLIs",
                                        "Use your Claude, Codex, or Gemini subscription via official CLI or connect an API key.",
                                        |ui| {
                                            if ui.add(primary(tr("+ Add AI"))).clicked() {
                                                open_add_ai_from_settings = true;
                                            }
                                        },
                                    );
                                    setting_row(
                                        ui,
                                        "Allow Auto buffer edits",
                                        "Auto applies an eligible replacement to the active buffer. Use Save to write the file.",
                                        |ui| {
                                            checkbox(
                                                ui,
                                                &mut self.preferences.allow_auto_buffer,
                                                "",
                                            );
                                        },
                                    );
                                    ui.add_space(4.0);
                                    ui.label(
                                        RichText::new(tr("Global agent rules (local)"))
                                            .size(BODY)
                                            .strong(),
                                    );
                                    ui.add(
                                        egui::Label::new(muted(
                                            "Saved locally on this PC (agent-rules.md). Applied to models, Agent Hub, and Luma.",
                                        ))
                                        .wrap(),
                                    );
                                    ui.add_space(6.0);
                                    ui.add(
                                        TextEdit::multiline(&mut rules_draft)
                                            .hint_text(tr("Write strict rules for all AI models, Agent Hub, and Luma…"))
                                            .desired_rows(5)
                                            .desired_width(f32::INFINITY)
                                            .char_limit(24_000),
                                    );
                                    ui.add_space(6.0);
                                    ui.horizontal(|ui| {
                                        if ui.add(primary(tr("Save rules"))).clicked() {
                                            match super::preferences::Preferences::save_agent_rules(
                                                &rules_draft,
                                            ) {
                                                Ok(_) => {
                                                    self.notice = Some((
                                                        false,
                                                        tr("Saved local agent rules.").to_owned(),
                                                    ));
                                                }
                                                Err(err) => {
                                                    self.error(format!(
                                                        "Could not save agent rules. {err}"
                                                    ));
                                                }
                                            }
                                        }
                                        if ui.button(tr("Open file")).clicked() {
                                            match super::preferences::Preferences::save_agent_rules(
                                                &rules_draft,
                                            ) {
                                                Ok(path) => {
                                                    open_rules_file = Some(path);
                                                }
                                                Err(err) => {
                                                    self.error(format!(
                                                        "Could not open agent rules. {err}"
                                                    ));
                                                }
                                            }
                                        }
                                    });
                                }
                                SettingsSection::Integrations => {
                                    let status = self.discord.status();
                                    let d = &mut self.preferences.discord;
                                    checkbox(
                                        ui,
                                        &mut d.enabled,
                                        tr("Show Covalos activity in Discord"),
                                    );
                                    checkbox(ui, &mut d.show_activity, tr("Show current activity"));
                                    checkbox(ui, &mut d.show_language, tr("Show language"));
                                    checkbox(ui, &mut d.show_project, tr("Show project name"));
                                    checkbox(ui, &mut d.show_elapsed, tr("Show session time"));
                                    ui.label(muted(format!("{}: {}", tr("Status"), tr(status))));
                                    ui.add(
                                        egui::Label::new(muted(
                                            "Paths, file names, prompts and accounts are never sent. Only the local Discord app receives the status.",
                                        ))
                                        .wrap(),
                                    );
                                }
                            }
                                        });
                                });
                        },
                    );
                });
                if old
                    != (
                        self.preferences.theme,
                        self.preferences.font_size,
                        self.preferences.high_contrast,
                        self.preferences.brightness.to_bits(),
                    )
                {
                    set_brightness(self.preferences.brightness);
                    install(
                        ctx,
                        self.preferences.font_size,
                        self.preferences.high_contrast,
                        self.preferences.theme,
                    );
                    self.syntax = egui_extras::syntax_highlighting::CodeTheme::dark(
                        self.preferences.font_size,
                    );
                }
            });
        if modal.should_close() {
            open = false;
        }
        self.settings_open = open;
        if open_add_ai_from_settings {
            self.settings_open = false;
            self.hub.agents.chat.add_ai = Some(Default::default());
        }
        if let Some(path) = open_rules_file {
            self.settings_open = false;
            self.open_file(path);
        }
        ctx.data_mut(|d| {
            d.insert_temp(tab_id, active_tab);
            d.insert_temp(query_id, search);
            d.insert_temp(rules_id, rules_draft);
        });
    }

    pub(super) fn about_window(&mut self, ctx: &egui::Context) {
        if !self.about_open {
            return;
        }
        egui::Window::new(tr("About Covalos"))
            .open(&mut self.about_open)
            .collapsible(false)
            .resizable(false)
            .fixed_size(egui::vec2(400.0, 240.0))
            .constrain_to(ctx.content_rect().shrink(16.0))
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let (_, icon_rect) = ui.allocate_space(egui::vec2(56.0, 56.0));
                    super::assets::paint(ui, icon_rect, "covalos_app");
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(crate::brand::NAME).size(HEADING).strong());
                        ui.label(muted(format!(
                            "{} · build {}",
                            crate::brand::RELEASE,
                            crate::brand::VERSION
                        )));
                        ui.add_space(4.0);
                        ui.add(
                            egui::Label::new(secondary(tr(crate::brand::DESCRIPTION))).wrap(),
                        );
                    });
                });
                ui.add_space(14.0);
                ui.separator();
                ui.add_space(10.0);
                ui.add(
                    egui::Label::new(muted(tr(
                        "Nocturne Design System · Native Rust renderer (eframe/egui).",
                    )))
                    .wrap(),
                );
                ui.add_space(6.0);
                ui.add(
                    egui::Label::new(muted(tr(
                        "Third-party assets: Lucide Icons (ISC), Material Icon Theme (MIT), Geist & IBM Plex Mono (OFL).",
                    )))
                    .wrap(),
                );
            });
    }

    pub(super) fn notifications_modal(&mut self, ctx: &egui::Context) {
        if !self.notifications_open {
            return;
        }
        let width = (ctx.content_rect().width() * 0.56).clamp(340.0, 620.0);
        let area = egui::Modal::default_area(Id::new("notifications-modal")).anchor(
            egui::Align2::CENTER_TOP,
            egui::vec2(0.0, ctx.content_rect().height() * 0.12),
        );
        let mut close_requested = false;
        let modal = egui::Modal::new(Id::new("notifications-modal"))
            .frame(modal_frame().inner_margin(egui::Margin::same(14)))
            .area(area)
            .show(ctx, |ui| {
                super::motion::enter(ui, Id::new("notifications-entry"), 0.16);
                ui.set_width(width);
                // Floating search header with inline clear/close control.
                ui.horizontal(|ui| {
                    let (_, icon_rect) = ui.allocate_space(egui::vec2(18.0, 24.0));
                    draw_icon_sized(
                        ui.painter(),
                        icon_rect.center(),
                        Icon::Search,
                        colors().muted,
                        15.0,
                    );
                    ui.add(
                        TextEdit::singleline(&mut self.notifications_query)
                            .hint_text(tr("Search notifications"))
                            .frame(false)
                            .desired_width((ui.available_width() - 34.0).max(120.0))
                            .char_limit(120),
                    );
                    if icon_button(ui, Icon::Close, "Close", false).clicked() {
                        close_requested = true;
                    }
                });
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(8.0);
                // Source filter pills + clear all action.
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    for filter in NotificationFilter::ALL {
                        let selected = self.notifications_filter == filter;
                        let button = egui::Button::new(
                            RichText::new(tr(filter.label()))
                                .size(SMALL)
                                .color(if selected {
                                    colors().text
                                } else {
                                    colors().secondary
                                }),
                        )
                        .fill(if selected {
                            colors().elevated
                        } else {
                            egui::Color32::TRANSPARENT
                        })
                        .stroke(if selected {
                            egui::Stroke::new(1.0_f32, colors().border)
                        } else {
                            egui::Stroke::NONE
                        })
                        .corner_radius(6);
                        if ui.add(button).clicked() {
                            self.notifications_filter = filter;
                        }
                    }
                    if !self.notifications.is_empty() {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(egui::Button::new(muted(tr("Clear all"))).frame(false))
                                .clicked()
                            {
                                self.notifications.clear();
                            }
                        });
                    }
                });
                ui.add_space(10.0);
                let query = self.notifications_query.trim().to_ascii_lowercase();
                let items: Vec<&Notification> = self
                    .notifications
                    .iter()
                    .rev()
                    .filter(|item| {
                        self.notifications_filter.matches(item.source)
                            && (query.is_empty()
                                || item.message.to_ascii_lowercase().contains(&query))
                    })
                    .collect();
                if items.is_empty() {
                    ui.add_space(14.0);
                    ui.vertical_centered(|ui| {
                        ui.label(muted(tr("No notifications yet.")));
                    });
                    ui.add_space(14.0);
                } else {
                    ScrollArea::vertical()
                        .id_salt("notifications-list")
                        .max_height((ctx.content_rect().height() * 0.42).min(300.0))
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 4.0;
                            for item in items {
                                egui::Frame::new()
                                    .fill(colors().inset)
                                    .corner_radius(6)
                                    .stroke(egui::Stroke::new(1.0_f32, colors().border))
                                    .inner_margin(egui::Margin::symmetric(10, 8))
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                RichText::new(tr(item.source.label()))
                                                    .size(SMALL)
                                                    .color(if item.is_error {
                                                        colors().error
                                                    } else {
                                                        colors().accent
                                                    })
                                                    .strong(),
                                            );
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(&item.message)
                                                        .size(BODY)
                                                        .color(colors().text),
                                                )
                                                .wrap(),
                                            );
                                        });
                                    });
                            }
                        });
                }
            });
        if close_requested || modal.should_close() {
            self.notifications_open = false;
        }
    }

    pub(super) fn palette_modal(&mut self, ctx: &egui::Context) {
        let Some(mut palette) = self.palette.take() else {
            return;
        };
        if let Ok(files) = palette.results.try_recv() {
            palette.files = files;
        }
        let palette_width = (ctx.content_rect().width() * 0.60).clamp(360.0, 680.0);
        let area = egui::Modal::default_area(Id::new("command-palette")).anchor(
            egui::Align2::CENTER_TOP,
            egui::vec2(0.0, ctx.content_rect().height() * 0.15),
        );
        let modal = egui::Modal::new(Id::new("command-palette"))
            .frame(modal_frame().inner_margin(egui::Margin::same(14)))
            .area(area)
            .show(ctx, |ui| {
                super::motion::enter(ui, Id::new("quick-access-entry"), 0.20);
                ui.set_width(palette_width);
                let down = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowDown));
                let up = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowUp));
                let confirm = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter));
                ui.horizontal(|ui| {
                    let (_, icon_rect) = ui.allocate_space(egui::vec2(18.0, 24.0));
                    draw_icon_sized(
                        ui.painter(),
                        icon_rect.center(),
                        Icon::Search,
                        colors().muted,
                        15.0,
                    );
                    let input = ui.add(
                        TextEdit::singleline(&mut palette.query)
                            .hint_text(tr("Find a file, > command, @ project, ? Codex"))
                            .frame(false)
                            .desired_width((ui.available_width() - 36.0).max(120.0))
                            .char_limit(12_000),
                    );
                    if std::mem::take(&mut palette.focus) {
                        input.request_focus();
                    }
                    if input.changed() {
                        palette.selected = 0;
                    }
                    ui.label(muted(tr("Esc")));
                });
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    for category in PaletteCategory::ALL {
                        let selected = palette.category == category;
                        let btn = egui::Button::new(
                            RichText::new(tr(category.label()))
                                .size(SMALL)
                                .color(if selected {
                                    colors().text
                                } else {
                                    colors().secondary
                                }),
                        )
                        .fill(if selected {
                            colors().elevated
                        } else {
                            egui::Color32::TRANSPARENT
                        })
                        .stroke(if selected {
                            egui::Stroke::new(1.0_f32, colors().border)
                        } else {
                            egui::Stroke::NONE
                        })
                        .corner_radius(6);
                        if ui.add(btn).clicked() {
                            palette.category = category;
                            palette.selected = 0;
                        }
                    }
                });
                ui.add_space(8.0);
                let query = palette.query.trim();
                let mut choices = Vec::new();
                if let Some(prompt) = query.strip_prefix('?') {
                    if self.root.is_some() && !prompt.trim().is_empty() {
                        choices.push((
                            Selection::Ask(prompt.trim().to_owned()),
                            "Ask Codex in this workspace".to_owned(),
                        ));
                    }
                } else if let Some(filter) = query.strip_prefix('@') {
                    let filter = filter.trim().to_lowercase();
                    choices.extend(self.preferences.recent.iter().filter_map(|root| {
                        let title = workspace::filename(root);
                        title
                            .to_lowercase()
                            .contains(&filter)
                            .then(|| (Selection::Project(root.clone()), title))
                    }));
                } else if palette.category == PaletteCategory::Files
                    && !query.starts_with('>')
                    && let Some(root) = &self.root
                {
                    let filter = query.to_lowercase().replace('\\', "/");
                    choices.extend(
                        palette
                            .files
                            .iter()
                            .filter_map(|file| {
                                let label = file
                                    .strip_prefix(root)
                                    .unwrap_or(file)
                                    .display()
                                    .to_string();
                                (filter.is_empty()
                                    || label.to_lowercase().replace('\\', "/").contains(&filter))
                                .then(|| (Selection::File(file.clone()), label))
                            })
                            .take(30),
                    );
                } else if query.is_empty() || query.starts_with('>') {
                    let filter = query.trim_start_matches('>').trim().to_lowercase();
                    choices.extend(
                        ACTIONS
                            .iter()
                            .filter(|(_, label, cat)| {
                                (palette.category == PaletteCategory::All
                                    || *cat == palette.category)
                                    && label.to_lowercase().contains(&filter)
                            })
                            .map(|(action, label, _)| {
                                (Selection::Action(*action), (*label).to_owned())
                            }),
                    );
                } else if let Some(root) = &self.root {
                    let filter = query.to_lowercase().replace('\\', "/");
                    choices.extend(
                        palette
                            .files
                            .iter()
                            .filter_map(|file| {
                                let label = file
                                    .strip_prefix(root)
                                    .unwrap_or(file)
                                    .display()
                                    .to_string();
                                label
                                    .to_lowercase()
                                    .replace('\\', "/")
                                    .contains(&filter)
                                    .then(|| (Selection::File(file.clone()), label))
                            })
                            .take(30),
                    );
                }
                if !choices.is_empty() {
                    if down {
                        palette.selected = (palette.selected + 1) % choices.len();
                    }
                    if up {
                        palette.selected = (palette.selected + choices.len() - 1) % choices.len();
                    }
                    palette.selected = palette.selected.min(choices.len() - 1);
                    if confirm {
                        return Some(choices.remove(palette.selected).0);
                    }
                }
                let selected = ScrollArea::vertical()
                    .id_salt("palette-results")
                    .max_height((ctx.content_rect().height() * 0.48).min(360.0))
                    .show(ui, |ui| {
                        if choices.is_empty() {
                            ui.add_space(16.0);
                            ui.label(secondary(tr("No matching items.")));
                            ui.add_space(16.0);
                        }
                        for (index, (choice, label)) in choices.into_iter().enumerate() {
                            let hint = match &choice {
                                Selection::File(_) => "File",
                                Selection::Project(_) => "Project",
                                Selection::Ask(_) => "Codex",
                                Selection::Action(Action::OpenFolder) => "Ctrl O",
                                Selection::Action(Action::OpenFile) => "Ctrl Shift O",
                                Selection::Action(Action::Save) => "Ctrl S",
                                Selection::Action(Action::Sidebar) => "Ctrl B",
                                Selection::Action(Action::Find) => "Ctrl F",
                                Selection::Action(Action::Terminal) => "Ctrl `",
                                Selection::Action(_) => "Command",
                            };
                            let response = row(ui, &label, hint, index == palette.selected);
                            if (up || down) && index == palette.selected {
                                response.scroll_to_me(None);
                            }
                            if response.clicked() {
                                return Some(choice);
                            }
                        }
                        None
                    })
                    .inner;
                ui.add_space(8.0);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(muted("↑ ↓  Navigate"));
                    ui.label(muted(tr("Enter  Open")));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(muted(">  Commands"));
                    });
                });
                selected
            });
        match modal.inner {
            Some(Selection::Action(action)) => match action {
                Action::OpenFolder => self.pick_workspace(ctx),
                Action::OpenFile => self.pick_file(),
                Action::CreateProject => self.pick_new_project(ctx),
                Action::Search => self.show_workspace_search(),
                Action::Find => self.show_find(),
                Action::Save => self.save_active(),
                Action::Terminal => self.preferences.tools_open = !self.preferences.tools_open,
                Action::AgentHub => self.hub_open = !self.hub_open,
                Action::Sidebar => self.toggle_sidebar(),
                Action::FocusMode => self.clean.focus = !self.clean.focus,
                Action::Notifications => self.notifications_open = !self.notifications_open,
                Action::SourceControl => {
                    self.sidebar = super::Sidebar::Git;
                    self.sidebar_open = true;
                    if self.root.is_none() {
                        self.sidebar_requested = true;
                    }
                    self.git_refresh(ctx, None);
                }
                Action::Settings => self.settings_open = true,
                Action::Detach => {
                    if let Some(index) = self.active {
                        self.detach(index);
                    }
                }
                Action::DockAll => self.dock_all(ctx.input(|i| i.time)),
                Action::Library => self.open_library(),
                Action::About => self.about_open = true,
            },
            Some(Selection::File(file)) => self.open_file(file),
            Some(Selection::Project(root)) => self.request(Pending::OpenWorkspace(root), ctx),
            Some(Selection::Ask(prompt)) => {
                self.preferences.assistant_open = true;
                self.chat.prompt = prompt;
                self.hub_open = false;
                self.send_prompt(ctx);
            }
            None if !modal.should_close() => self.palette = Some(palette),
            None => {}
        }
    }
}

/// Home action card: icon tile, title, shortcut; lifts and lights its border on hover.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsSection {
    General,
    Appearance,
    Background,
    Editor,
    Notifications,
    Agents,
    Integrations,
}

impl SettingsSection {
    const ALL: [Self; 7] = [
        Self::General,
        Self::Appearance,
        Self::Background,
        Self::Editor,
        Self::Notifications,
        Self::Agents,
        Self::Integrations,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Appearance => "Appearance",
            Self::Background => "Background & ASCII",
            Self::Editor => "Editor",
            Self::Notifications => "Notifications",
            Self::Agents => "Agents",
            Self::Integrations => "Integrations",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Self::General => Icon::Settings,
            Self::Appearance => Icon::Palette,
            Self::Background => Icon::Sparkles,
            Self::Editor => Icon::Code,
            Self::Notifications => Icon::Bell,
            Self::Agents => Icon::Hub,
            Self::Integrations => Icon::Globe,
        }
    }

    fn matches(self, query: &str) -> bool {
        let keywords = match self {
            Self::General => "general version language import setup shortcut",
            Self::Appearance => "appearance theme obsidian dark light scale zoom motion contrast",
            Self::Background => "background ascii dither bloom orbital luma mascot tint",
            Self::Editor => "editor font size syntax colors density compact",
            Self::Notifications => "notifications quiet taskbar attention",
            Self::Agents => "agents auto buffer edits",
            Self::Integrations => "integrations discord presence",
        };
        keywords.contains(query) || self.label().to_ascii_lowercase().contains(query)
    }
}

fn project_badge_color(name: &str) -> egui::Color32 {
    const PALETTE: [egui::Color32; 6] = [
        egui::Color32::from_rgb(96, 165, 250),  // sky blue
        egui::Color32::from_rgb(52, 211, 153),  // emerald
        egui::Color32::from_rgb(251, 191, 36),  // amber
        egui::Color32::from_rgb(244, 114, 182), // rose
        egui::Color32::from_rgb(167, 139, 250), // violet
        egui::Color32::from_rgb(56, 189, 248),  // cyan
    ];
    let hash = name
        .bytes()
        .fold(0usize, |acc, b| acc.wrapping_mul(31).wrapping_add(b as usize));
    PALETTE[hash % PALETTE.len()]
}

fn format_tokens(n: u64) -> String {
    if n >= 1_000_000_000 {
        format!("{:.1}B", n as f64 / 1_000_000_000.0)
    } else if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

fn usage_bar_row(
    ui: &mut Ui,
    name: &str,
    tokens: u64,
    max_tokens: f32,
    turns: u32,
    color: egui::Color32,
) {
    ui.horizontal(|ui| {
        ui.add(egui::Label::new(RichText::new(name).size(SMALL)).truncate());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(muted(format!("{} · {turns}", format_tokens(tokens))));
        });
    });
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 5.0),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(rect, 2, colors().elevated);
    let ratio = (tokens as f32 / max_tokens).clamp(0.0, 1.0);
    if ratio > 0.005 {
        let fill = egui::Rect::from_min_size(
            rect.min,
            egui::vec2((rect.width() * ratio).max(4.0), rect.height()),
        );
        ui.painter().rect_filled(fill, 2, color);
    }
    ui.add_space(6.0);
}

fn setting_row(ui: &mut Ui, title: &str, subtitle: &str, control: impl FnOnce(&mut Ui)) {
    let full_w = ui.available_width();
    let control_w = 156.0_f32.min(full_w * 0.40);
    let left_w = (full_w - control_w - 12.0).max(120.0);
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(left_w, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(left_w);
                ui.label(
                    RichText::new(tr(title))
                        .size(BODY)
                        .color(colors().text)
                        .strong(),
                );
                if !subtitle.is_empty() {
                    ui.add_space(2.0);
                    ui.add(egui::Label::new(muted(subtitle)).wrap());
                }
            },
        );
        ui.allocate_ui_with_layout(
            egui::vec2(control_w, 28.0),
            egui::Layout::right_to_left(egui::Align::Center),
            control,
        );
    });
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
}

fn home_card(ui: &mut Ui, width: f32, icon: Icon, title: &str, detail: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 64.0), egui::Sense::click());
    #[cfg(test)]
    super::motion::record(ui, format!("Home {title}"), rect);
    let hover = super::motion::state(
        ui,
        response.id.with("card-hover"),
        response.hovered() || response.has_focus(),
        super::motion::CONTROL,
    );
    let press = super::motion::state(
        ui,
        response.id.with("card-press"),
        response.is_pointer_button_down_on(),
        super::motion::HOVER,
    );
    let r = rect.translate(egui::vec2(0.0, -2.0 * hover + press * 1.5));
    let painter = ui.painter();
    if hover > 0.01 {
        painter.add(
            egui::Shadow {
                offset: [0, 6],
                blur: 18,
                spread: 0,
                color: egui::Color32::from_black_alpha((60.0 * hover) as u8),
            }
            .as_shape(r, 12),
        );
    }
    painter.rect(
        r,
        12,
        super::motion::blend(
            colors().surface.gamma_multiply(0.92),
            colors().elevated,
            hover,
        ),
        egui::Stroke::new(
            1.0_f32,
            super::motion::blend(colors().border, colors().accent.gamma_multiply(0.7), hover),
        ),
        egui::StrokeKind::Inside,
    );
    let tile = egui::Rect::from_center_size(
        r.left_center() + egui::vec2(32.0, 0.0),
        egui::vec2(36.0, 36.0),
    );
    painter.rect_filled(
        tile,
        10,
        super::motion::blend(colors().inset, colors().accent_bg, hover),
    );
    draw_icon(
        painter,
        tile.center(),
        icon,
        super::motion::blend(colors().secondary, colors().accent_hover, hover),
    );
    let text = r.left_center() + egui::vec2(62.0, 0.0);
    painter.text(
        text - egui::vec2(0.0, 9.0),
        egui::Align2::LEFT_CENTER,
        tr(title),
        egui::FontId::proportional(BODY + 1.0),
        colors().text,
    );
    painter.text(
        text + egui::vec2(0.0, 10.0),
        egui::Align2::LEFT_CENTER,
        tr(detail),
        egui::FontId::proportional(SMALL),
        colors().muted,
    );
    // Arrow nudges right on hover.
    let arrow = r.right_center() - egui::vec2(20.0 - hover * 4.0, 0.0);
    let s = egui::Stroke::new(1.4_f32, colors().muted.gamma_multiply(0.4 + 0.6 * hover));
    painter.line_segment(
        [arrow - egui::vec2(5.0, 0.0), arrow + egui::vec2(4.0, 0.0)],
        s,
    );
    painter.add(egui::Shape::line(
        vec![
            arrow + egui::vec2(0.5, -3.5),
            arrow + egui::vec2(4.0, 0.0),
            arrow + egui::vec2(0.5, 3.5),
        ],
        s,
    ));
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, title));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

#[cfg(test)]
mod tests {
    use super::super::preferences::Preferences;
    use super::*;

    #[test]
    fn command_palette_arrows_choose_the_selected_command() {
        let ctx = egui::Context::default();
        let mut editor = Editor::new(&ctx, None, Preferences::default());
        editor.open_palette(&ctx);
        editor.palette.as_mut().unwrap().query = "> toggle".into();
        let input = |events| egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1366.0, 768.0),
            )),
            events,
            ..Default::default()
        };
        let press = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let _ = ctx.run(input(vec![]), |ctx| editor.render(ctx));
        let _ = ctx.run(input(vec![press(Key::ArrowDown)]), |ctx| editor.render(ctx));
        let _ = ctx.run(input(vec![press(Key::Enter)]), |ctx| editor.render(ctx));
        assert!(editor.palette.is_none());
        assert!(editor.hub_open);
        assert!(!editor.preferences.tools_open);
    }
}

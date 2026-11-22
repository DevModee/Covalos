use std::path::Path;

use eframe::egui::{
    self, Align, Context, Id, Key, KeyboardShortcut, Layout, Margin, Modifiers, RichText,
    ScrollArea, SidePanel, TextEdit, TextStyle, TopBottomPanel, Ui,
    text::{CCursor, CCursorRange, LayoutJob},
};

use super::{Editor, Pending, Sidebar};
use crate::workspace::{self, MAX_SEARCH_HITS};

use super::theme::*;

impl Editor {
    pub(super) fn shortcuts(&mut self, ctx: &Context) {
        let shortcut = |modifiers, key| {
            ctx.input_mut(|input| input.consume_shortcut(&KeyboardShortcut::new(modifiers, key)))
        };
        // Region zoom: the Explorer when the pointer is over it, otherwise the editor.
        let over_explorer = self.explorer_rect.is_some_and(|r| {
            ctx.input(|i| i.pointer.hover_pos())
                .is_some_and(|p| r.contains(p))
        });
        let mut step = 0.0_f32;
        let mut reset = false;
        if shortcut(Modifiers::COMMAND, Key::Equals) || shortcut(Modifiers::COMMAND, Key::Plus) {
            step = 1.0;
        } else if shortcut(Modifiers::COMMAND, Key::Minus) {
            step = -1.0;
        } else if shortcut(Modifiers::COMMAND, Key::Num0) {
            reset = true;
        }
        let wheel = ctx.input(|i| i.zoom_delta());
        if (wheel - 1.0).abs() > 0.001 {
            step = if wheel > 1.0 { 1.0 } else { -1.0 };
        }
        if step != 0.0 || reset {
            if over_explorer {
                let (lo, hi) = super::preferences::EXPLORER_ZOOM;
                self.preferences.explorer_zoom = if reset {
                    1.0
                } else {
                    (self.preferences.explorer_zoom + step * 0.05).clamp(lo, hi)
                };
            } else {
                let (lo, hi) = super::preferences::EDITOR_FONT;
                self.preferences.font_size = if reset {
                    14.0
                } else {
                    (self.preferences.font_size + step).clamp(lo, hi)
                };
                install(
                    ctx,
                    self.preferences.font_size,
                    self.preferences.high_contrast,
                    self.preferences.theme,
                );
            }
        }
        if shortcut(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::A) {
            self.hub_open = false;
            self.preferences.assistant_open = !self.preferences.assistant_open;
        }
        if let Some((modifiers, key)) = parse_shortcut(&self.preferences.library_shortcut)
            && shortcut(modifiers, key)
        {
            if self.studio_open {
                self.studio_open = false;
            } else {
                self.open_library();
            }
        }
        if shortcut(Modifiers::COMMAND, Key::B) {
            self.toggle_sidebar();
        }
        if shortcut(Modifiers::COMMAND, Key::Backtick) {
            self.preferences.tools_open = !self.preferences.tools_open;
        }
        if shortcut(Modifiers::COMMAND, Key::P) {
            self.open_palette(ctx);
        }
        if shortcut(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::O) {
            self.pick_file();
        } else if shortcut(Modifiers::COMMAND, Key::O) {
            self.pick_workspace(ctx);
        }
        if shortcut(Modifiers::COMMAND, Key::N) {
            self.show_new_file();
        }
        if shortcut(Modifiers::COMMAND, Key::S) {
            self.save_active();
        }
        if shortcut(Modifiers::COMMAND, Key::W)
            && let Some(index) = self.active
        {
            self.request(Pending::CloseTab(index), ctx);
        }
        if shortcut(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::F) {
            self.show_workspace_search();
        } else if shortcut(Modifiers::COMMAND, Key::F) {
            self.show_find();
        }
        if self.find_open {
            if shortcut(Modifiers::SHIFT, Key::F3) {
                self.find(true);
            } else if shortcut(Modifiers::NONE, Key::F3) {
                self.find(false);
            }
            if shortcut(Modifiers::NONE, Key::Escape) {
                self.find_open = false;
                self.focus_editor = true;
            }
        }
    }

    fn show_new_file(&mut self) {
        if self.root.is_some() {
            self.notice = None;
            self.new_file = Some(String::new());
            self.focus_new_file = true;
        }
    }

    pub(super) fn toggle_sidebar(&mut self) {
        if self.root.is_none() {
            self.sidebar_requested = !self.sidebar_requested;
            self.sidebar_open = self.sidebar_requested;
        } else {
            self.sidebar_open = !self.sidebar_open;
        }
    }

    pub(super) fn show_workspace_search(&mut self) {
        self.sidebar = Sidebar::Search;
        self.sidebar_open = true;
        if self.root.is_none() {
            self.sidebar_requested = true;
        }
        self.search_focus = true;
    }

    pub(super) fn top_bar(&mut self, ctx: &Context) {
        TopBottomPanel::top("menu")
            .exact_height(CHROME_HEIGHT)
            .show_separator_line(false)
            .frame(panel_frame(Margin::symmetric(12, 6)))
            .show(ctx, |ui| {
                let rect = ui.available_rect_before_wrap();
                let drag = ui.interact(rect, Id::new("window-drag"), egui::Sense::click_and_drag());
                if drag.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
                if drag.double_clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                }
                let left_width = if rect.width() > 1100.0 { 330.0 } else { 250.0 };
                let right_width = if rect.width() > 1000.0 { 312.0 } else { 248.0 };
                let left =
                    egui::Rect::from_min_size(rect.min, egui::vec2(left_width, rect.height()));
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(left)
                        .layout(Layout::left_to_right(Align::Center)),
                    |ui| {
                        ui.set_clip_rect(left);
                        // Chrome controls read as quiet text until hovered.
                        let w = &mut ui.visuals_mut().widgets;
                        w.inactive.bg_stroke = egui::Stroke::NONE;
                        w.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
                        w.hovered.bg_stroke = egui::Stroke::NONE;
                        let (_, mark) = ui.allocate_space(egui::vec2(22.0, 28.0));
                        draw_icon(ui.painter(), mark.center(), Icon::Brand, colors().accent);
                        ui.label(RichText::new(BRAND).size(LEAD).strong());
                        ui.label(RichText::new("Alpha").size(SMALL).color(colors().muted))
                            .on_hover_text(crate::brand::RELEASE);
                        ui.add_space(8.0);
                        ui.menu_button(tr("File"), |ui| {
                            if ui.button(tr("Library Studio")).clicked() {
                                ui.close();
                                self.open_library();
                            }
                            ui.separator();
                            if ui.button(tr("Open folder…")).clicked() {
                                ui.close();
                                self.pick_workspace(ctx);
                            }
                            if ui.button(tr("Open file…")).clicked() {
                                ui.close();
                                self.pick_file();
                            }
                            if ui
                                .add_enabled(
                                    self.root.is_some(),
                                    egui::Button::new(tr("New file…")),
                                )
                                .clicked()
                            {
                                ui.close();
                                self.show_new_file();
                            }
                            ui.separator();
                            if ui
                                .add_enabled(self.active.is_some(), egui::Button::new(tr("Save")))
                                .clicked()
                            {
                                ui.close();
                                self.save_active();
                            }
                            if ui
                                .add_enabled(
                                    !self.documents.is_empty(),
                                    egui::Button::new(tr("Save all")),
                                )
                                .clicked()
                            {
                                ui.close();
                                if self.save_all() {
                                    self.notice = Some((false, "Saved all files".into()));
                                }
                            }
                            if ui
                                .add_enabled(
                                    self.active.is_some(),
                                    egui::Button::new(tr("Close tab")),
                                )
                                .clicked()
                                && let Some(index) = self.active
                            {
                                ui.close();
                                self.request(Pending::CloseTab(index), ctx);
                            }
                            ui.separator();
                            if ui.button(tr("About Covalos")).clicked() {
                                ui.close();
                                self.about_open = true;
                            }
                        });
                        if rect.width() > 1100.0 {
                            self.workspace_picker(ui);
                        }
                    },
                );
                // The command field is centred on the window, independent of how wide the
                // side zones are, and shrinks before it would touch them.
                let side = left_width.max(right_width) + 16.0;
                let command_width = (rect.width() - 2.0 * side).min(520.0);
                let command = egui::Rect::from_center_size(
                    rect.center(),
                    egui::vec2(command_width.max(0.0), 30.0),
                );
                if command_width >= 140.0 {
                    ui.scope_builder(egui::UiBuilder::new().max_rect(command), |ui| {
                        let response = ui.add_sized(
                            command.size(),
                            egui::Button::new("")
                                .fill(colors().inset)
                                .stroke(egui::Stroke::new(1.0_f32, colors().border)),
                        );
                        draw_icon(
                            ui.painter(),
                            response.rect.left_center() + egui::vec2(16.0, 0.0),
                            Icon::Search,
                            colors().muted,
                        );
                        ui.painter().text(
                            response.rect.left_center() + egui::vec2(32.0, 0.0),
                            egui::Align2::LEFT_CENTER,
                            if command_width > 320.0 {
                                tr("Search files and commands")
                            } else {
                                tr("Quick access")
                            },
                            egui::FontId::proportional(BODY),
                            colors().secondary,
                        );
                        ui.painter().text(
                            response.rect.right_center() - egui::vec2(12.0, 0.0),
                            egui::Align2::RIGHT_CENTER,
                            if cfg!(target_os = "macos") {
                                "⌘ P"
                            } else {
                                "Ctrl P"
                            },
                            egui::FontId::proportional(SMALL),
                            colors().muted,
                        );
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::WidgetType::Button,
                                true,
                                "Quick access",
                            )
                        });
                        if response.clicked() {
                            self.open_palette(ctx);
                        }
                    });
                }
                let right = egui::Rect::from_min_max(
                    egui::pos2(rect.right() - right_width, rect.top()),
                    rect.max,
                );
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(right)
                        .layout(Layout::right_to_left(Align::Center)),
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        if icon_button(ui, Icon::Close, "Close window", false).clicked() {
                            self.request(Pending::Exit, ctx);
                        }
                        if icon_button(
                            ui,
                            if maximized {
                                Icon::Restore
                            } else {
                                Icon::Maximize
                            },
                            "Maximize or restore",
                            false,
                        )
                        .clicked()
                        {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                        }
                        if icon_button(ui, Icon::Minimize, "Minimize", false).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }
                        ui.add_space(8.0);
                        if icon_button(ui, Icon::Settings, "Settings", self.settings_open).clicked()
                        {
                            self.settings_open = !self.settings_open;
                        }
                        // One destination for every provider: Agents. Codex, Claude and
                        // Gemini are providers inside it, not separate places.
                        let agents = if rect.width() > 900.0 {
                            ui.selectable_label(self.hub_open, tr("Agents"))
                        } else {
                            icon_button(ui, Icon::Hub, "Agents", self.hub_open)
                        };
                        #[cfg(test)]
                        super::motion::record(ui, "Agents", agents.rect);
                        if agents
                            .on_hover_text(tr(
                                "Agents — sessions, providers and the side chat (Ctrl+Shift+A)",
                            ))
                            .clicked()
                        {
                            self.hub_open = !self.hub_open;
                            if self.hub_open {
                                self.studio_open = false;
                            }
                        }
                    },
                );
            });
    }

    pub(super) fn status_bar(&mut self, ctx: &Context) {
        TopBottomPanel::bottom("status")
            .exact_height(STATUS_HEIGHT)
            .show_separator_line(false)
            .frame(panel_frame(Margin::symmetric(12, 2)))
            .show(ctx, |ui| {
                let rect = ui.available_rect_before_wrap();
                let right_width = if self.active.is_some() { 290.0 } else { 100.0 };
                let left = egui::Rect::from_min_max(
                    rect.min,
                    egui::pos2(rect.right() - right_width - 8.0, rect.bottom()),
                );
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(left)
                        .layout(Layout::left_to_right(Align::Center)),
                    |ui| {
                        ui.set_clip_rect(left);
                        if let Some((error, text)) = &self.notice {
                            ui.add(
                                egui::Label::new(muted(text).color(if *error {
                                    colors().error
                                } else {
                                    colors().secondary
                                }))
                                .truncate(),
                            )
                            .on_hover_text(text);
                        } else if let Some(root) = &self.root {
                            let name = workspace::filename(root);
                            let (_, project_icon) = ui.allocate_space(egui::vec2(16.0, 18.0));
                            draw_icon(
                                ui.painter(),
                                project_icon.center(),
                                Icon::Explorer,
                                colors().muted,
                            );
                            ui.add(egui::Label::new(muted(name)).truncate())
                                .on_hover_text(root.display().to_string());
                        } else {
                            ui.label(muted(tr("Open a workspace")));
                        }
                    },
                );
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(egui::Rect::from_min_max(
                            egui::pos2(rect.right() - right_width, rect.top()),
                            rect.max,
                        ))
                        .layout(Layout::right_to_left(Align::Center)),
                    |ui| {
                        if ui
                            .selectable_label(self.preferences.tools_open, muted(tr("Terminal")))
                            .clicked()
                        {
                            self.preferences.tools_open = !self.preferences.tools_open;
                        }
                        if let Some(doc) = self.active.map(|index| &self.documents[index]) {
                            ui.label(muted(tr("UTF-8")));
                            let (line, col) = workspace::cursor_position(&doc.text, self.cursor);
                            ui.label(muted(format!("Ln {line}, Col {col}")));
                        }
                    },
                );
            });
    }

    pub(super) fn activity_rail(&mut self, ctx: &Context) {
        SidePanel::left("activity")
            .exact_width(RAIL_WIDTH)
            .resizable(false)
            .show_separator_line(false)
            .frame(panel_frame(Margin::symmetric(6, 10)))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                // Top cluster: primary workspace navigation and agents.
                ui.vertical_centered(|ui| {
                    for (tab, icon, label) in [
                        (Sidebar::Files, Icon::Explorer, "Explorer"),
                        (Sidebar::Search, Icon::Search, "Search"),
                        (Sidebar::Git, Icon::Git, "Source control"),
                    ] {
                        let shown =
                            !self.studio_open && self.sidebar_visible() && self.sidebar == tab;
                        if icon_button(ui, icon, label, shown).clicked() {
                            self.studio_open = false;
                            if shown {
                                self.toggle_sidebar();
                            } else {
                                self.sidebar = tab;
                                self.sidebar_open = true;
                                if self.root.is_none() {
                                    self.sidebar_requested = true;
                                }
                                if tab == Sidebar::Search {
                                    self.search_focus = true;
                                }
                                if tab == Sidebar::Git {
                                    self.git_refresh(ctx, None);
                                }
                            }
                        }
                    }
                    if icon_button(
                        ui,
                        Icon::Hub,
                        "Agents",
                        self.hub_open || self.preferences.assistant_open,
                    )
                    .clicked()
                    {
                        self.hub_open = false;
                        self.studio_open = false;
                        self.preferences.assistant_open = !self.preferences.assistant_open;
                    }
                });
                // Bottom cluster: terminal, notifications, library studio, and settings.
                ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    if icon_button(ui, Icon::Settings, "Settings", self.settings_open).clicked() {
                        self.settings_open = !self.settings_open;
                    }
                    let bell = icon_button(
                        ui,
                        Icon::Bell,
                        "Notifications",
                        self.notifications_open,
                    );
                    if self.notifications.iter().any(|n| n.unread) {
                        ui.painter().circle_filled(
                            bell.rect.right_top() + egui::vec2(-6.0, 6.0),
                            3.0,
                            colors().accent,
                        );
                    }
                    if bell.clicked() {
                        self.notifications_open = !self.notifications_open;
                        if self.notifications_open {
                            for item in &mut self.notifications {
                                item.unread = false;
                            }
                        }
                    }
                    if icon_button(ui, Icon::Library, "Library Studio", self.studio_open).clicked()
                    {
                        if self.studio_open {
                            self.studio_open = false;
                        } else {
                            self.open_library();
                        }
                    }
                    if icon_button(ui, Icon::Terminal, "Terminal", self.preferences.tools_open)
                        .clicked()
                    {
                        self.preferences.tools_open = !self.preferences.tools_open;
                    }
                });
            });
    }

    pub(super) fn luma_dock(&mut self, ui: &mut Ui) {
        if !self.preferences.identity.mascot {
            return;
        }
        let height = (self.preferences.identity.mascot_size + 24.0).max(80.0);
        if ui.available_height() < height + 140.0 {
            return;
        }
        // A real bottom panel inside the sidebar: the tree above only receives the space
        // that remains, so rows never slide underneath Luma.
        egui::TopBottomPanel::bottom("luma-dock")
            .exact_height(height)
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::new().inner_margin(Margin::symmetric(2, 8)))
            .show_inside(ui, |dock| {
                let footer = dock.max_rect();
                dock.painter().hline(
                    footer.x_range().shrink(4.0),
                    footer.top() - 8.0,
                    egui::Stroke::new(1.0_f32, colors().border.gamma_multiply(0.6)),
                );
                dock.with_layout(Layout::left_to_right(Align::Center), |dock| {
                    let mood = self.agent_mood(dock.input(|i| i.focused));
                    if self.identity.mascot(
                        dock,
                        &self.preferences.identity,
                        mood,
                        self.preferences.reduce_motion,
                    ) {
                        self.companion.open = true;
                    }
                    dock.vertical(|ui| {
                        ui.add_space(6.0);
                        ui.label(RichText::new(tr("Luma")).strong().color(colors().secondary));
                        ui.add(egui::Label::new(muted(mood.name())).truncate());
                        if ui
                            .add(egui::Button::new(muted(tr("Companion ↗"))).frame(false))
                            .clicked()
                        {
                            self.companion.open = true;
                        }
                    });
                });
            });
    }

    pub(super) fn sidebar_visible(&self) -> bool {
        self.sidebar_open && (self.root.is_some() || self.sidebar_requested)
    }

    pub(super) fn sidebar(&mut self, ctx: &Context) {
        let zoom = self.preferences.explorer_zoom;
        EXPLORER_ZOOM.with(|z| z.set(zoom));
        // Bounds: never squeeze the editor below its minimum, never collapse the tree.
        let assistant = if self.preferences.assistant_open {
            self.preferences.assistant_width
        } else {
            0.0
        };
        let max = (ctx.content_rect().width() - RAIL_WIDTH - MIN_EDITOR_WIDTH - assistant)
            .clamp(180.0, 480.0);
        let min = (160.0 * zoom).min(max);
        let mut collapse_sidebar = false;
        let panel = SidePanel::left("workspace")
            .default_width(self.preferences.sidebar_width.clamp(min, max))
            .width_range(min..=max)
            .show_separator_line(false)
            .frame(sidebar_frame(Margin::symmetric(PANEL_PADDING, 8)))
            .show(ctx, |ui| {
                self.luma_dock(ui);
                let mut selected = None;
                // Integrated compact header: collapse toggle, workspace pill, and view actions.
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if icon_button(ui, Icon::PanelLeft, "Toggle sidebar", false).clicked() {
                        collapse_sidebar = true;
                    }
                    if let Some(root) = self.root.clone() {
                        let actions_w = if self.sidebar == Sidebar::Files {
                            68.0
                        } else {
                            0.0
                        };
                        let title_w = (ui.available_width() - actions_w).max(40.0);
                        let root_label = ui
                            .allocate_ui_with_layout(
                                egui::vec2(title_w, 28.0),
                                Layout::left_to_right(Align::Center),
                                |ui| {
                                    ui.set_min_width(ui.available_width());
                                    let (_, icon_rect) =
                                        ui.allocate_space(egui::vec2(16.0, 18.0));
                                    draw_icon_sized(
                                        ui.painter(),
                                        icon_rect.center(),
                                        Icon::Folder,
                                        colors().muted,
                                        14.0,
                                    );
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(workspace::filename(&root))
                                                .strong()
                                                .color(colors().text),
                                        )
                                        .truncate(),
                                    )
                                },
                            )
                            .inner
                            .on_hover_text(root.display().to_string());
                        root_label.context_menu(|ui| {
                            selected = super::files::tree_menu(ui, &root, &root, true);
                        });
                        if self.sidebar == Sidebar::Files {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.spacing_mut().item_spacing.x = 2.0;
                                if icon_button(ui, Icon::Refresh, "Refresh files", false).clicked()
                                {
                                    self.directories.clear();
                                }
                                if icon_button(ui, Icon::FilePlus, "New file", false).clicked() {
                                    self.show_new_file();
                                }
                            });
                        }
                    } else {
                        let label = match self.sidebar {
                            Sidebar::Files => "Explorer",
                            Sidebar::Search => "Search",
                            Sidebar::Git => "Git",
                        };
                        ui.add(
                            egui::Label::new(
                                RichText::new(tr(label)).strong().color(colors().secondary),
                            )
                            .truncate(),
                        );
                    }
                });
                ui.add_space(6.0);
                let Some(root) = self.root.clone() else {
                    ui.label(muted(tr("Open a folder to browse files.")));
                    return;
                };
                match self.sidebar {
                    Sidebar::Files => {
                        // Compact live filter for quickly narrowing visible files in the tree.
                        ui.horizontal(|ui| {
                            let clear_w = if self.explorer_filter.is_empty() {
                                0.0
                            } else {
                                24.0
                            };
                            ui.add(
                                TextEdit::singleline(&mut self.explorer_filter)
                                    .hint_text(tr("Filter files…"))
                                    .margin(Margin::symmetric(8, 4))
                                    .desired_width((ui.available_width() - clear_w).max(60.0))
                                    .char_limit(120),
                            );
                            if !self.explorer_filter.is_empty()
                                && ui
                                    .small_button("×")
                                    .on_hover_text(tr("Clear filter"))
                                    .clicked()
                            {
                                self.explorer_filter.clear();
                            }
                        });
                        ui.add_space(6.0);
                        let filter = self.explorer_filter.trim().to_ascii_lowercase();
                        let active = self
                            .active
                            .map(|index| self.documents[index].path.as_path());
                        let from_tree = ScrollArea::vertical()
                            .id_salt("files")
                            .show(ui, |ui| {
                                tree(
                                    ui,
                                    &root,
                                    &root,
                                    active,
                                    &filter,
                                    &mut self.directories,
                                    0,
                                )
                            })
                            .inner;
                        if let Some(action) = selected.or(from_tree) {
                            self.tree_action(action);
                        }
                    }
                    Sidebar::Search => self.search_panel(ui, &root),
                    Sidebar::Git => self.git_panel(ui),
                }
            });
        if collapse_sidebar {
            self.toggle_sidebar();
        }
        self.preferences.sidebar_width = panel.response.rect.width();
        self.explorer_rect = Some(panel.response.rect);
    }

    fn search_panel(&mut self, ui: &mut Ui, root: &Path) {
        let response = ui.add(
            TextEdit::singleline(&mut self.workspace_query)
                .hint_text(tr("Find text in files"))
                .desired_width(f32::INFINITY)
                .char_limit(200),
        );
        if std::mem::take(&mut self.search_focus) {
            response.request_focus();
        }
        if (response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter)))
            || ui.button(tr("Search workspace")).clicked()
        {
            self.start_search(ui.ctx());
        }
        let status = if self.search_job.is_some() {
            format!("Searching… {} matches", self.search_hits.len())
        } else if self.search_hits.len() == MAX_SEARCH_HITS {
            format!("Showing first {MAX_SEARCH_HITS} matches")
        } else {
            format!("{} matches", self.search_hits.len())
        };
        if !self.workspace_query.is_empty() {
            ui.label(muted(status));
        }
        ui.add_space(5.0);
        let mut selected = None;
        ScrollArea::vertical()
            .id_salt("search-results")
            .show(ui, |ui| {
                for (index, hit) in self.search_hits.iter().enumerate() {
                    let name = workspace::filename(&hit.path);
                    let path = hit.path.strip_prefix(root).unwrap_or(&hit.path);
                    let row = format!("{name}:{}\n{}", hit.line, hit.text);
                    if ui
                        .add(
                            egui::Button::new(RichText::new(row).small())
                                .frame(false)
                                .wrap(),
                        )
                        .on_hover_text(path.display().to_string())
                        .clicked()
                    {
                        selected = Some(index);
                    }
                }
            });
        if let Some(index) = selected {
            self.select_hit(index);
        }
    }

    /// Shown when the active file changed on disk while it had unsaved edits.
    fn disk_banner(&mut self, ui: &mut Ui) {
        let Some(index) = self.active else { return };
        let path = self.documents[index].path.clone();
        if !self.disk_conflicts.contains(&path) {
            return;
        }
        let mut choice = None;
        egui::Frame::new()
            .fill(colors().warning.gamma_multiply(0.14))
            .inner_margin(Margin::symmetric(12, 6))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(tr(
                            "This file changed on disk (an agent or another tool edited it) while you have unsaved edits.",
                        ))
                        .color(colors().text),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(tr("Keep mine")).clicked() {
                            choice = Some(false);
                        }
                        if ui.button(tr("Reload from disk")).clicked() {
                            choice = Some(true);
                        }
                    });
                });
            });
        match choice {
            Some(true) => {
                if let Err(e) = self.documents[index].reload() {
                    self.error(format!("Could not reload the file. {e}"));
                }
                self.disk_conflicts.retain(|p| p != &path);
            }
            Some(false) => {
                self.documents[index].keep_mine();
                self.disk_conflicts.retain(|p| p != &path);
            }
            None => {}
        }
    }

    pub(super) fn editor_panel(&mut self, ui: &mut Ui) {
        self.clean.dock_target = Some(ui.max_rect());
        if self.sidebar == Sidebar::Git && self.tools.showing_diff() {
            self.git_diff_surface(ui);
            return;
        }
        if self.documents.is_empty() || self.active.is_none() {
            self.welcome(ui);
            return;
        }
        super::motion::enter(
            ui,
            Id::new("editor-surface-entry"),
            self.preferences.transitions.seconds(),
        );
        if self.preferences.identity.behind_code {
            let mood = self.agent_mood(ui.input(|i| i.focused));
            self.identity.paint(
                ui,
                ui.available_rect_before_wrap(),
                &self.preferences.identity,
                mood,
            );
        }
        self.tabs(ui);
        self.disk_banner(ui);
        if self.find_open {
            self.find_bar(ui);
            ui.separator();
        }
        let Some(index) = self.active else { return };
        let mut detach_active = false;
        let mut open_find = false;
        let mut toggle_terminal = false;
        let doc = &self.documents[index];
        let breadcrumb = self
            .root
            .as_ref()
            .and_then(|root| doc.path.strip_prefix(root).ok())
            .unwrap_or(&doc.path);
        let parts: Vec<String> = breadcrumb
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect();
        let (parents, file_name) = match parts.split_last() {
            Some((last, rest)) => (rest.join("  ›  "), last.as_str()),
            None => (String::new(), ""),
        };
        let doc_path = doc.path.clone();
        egui::Frame::new()
            .inner_margin(Margin::symmetric(12, 0))
            .show(ui, |ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), BREADCRUMB_HEIGHT),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            if icon_button(ui, Icon::Detach, "Detach to floating editor", false)
                                .clicked()
                            {
                                detach_active = true;
                            }
                            if icon_button(
                                ui,
                                Icon::PanelBottom,
                                "Toggle terminal",
                                self.preferences.tools_open,
                            )
                            .clicked()
                            {
                                toggle_terminal = true;
                            }
                            if icon_button(ui, Icon::Search, "Find in active file", self.find_open)
                                .clicked()
                            {
                                open_find = true;
                            }
                            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                let (_, icon_rect) = ui.allocate_space(egui::vec2(14.0, 16.0));
                                super::assets::file(ui, icon_rect.center(), &doc_path);
                                if !parents.is_empty() {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(format!("{parents}  ›"))
                                                .size(SMALL)
                                                .color(colors().muted),
                                        )
                                        .truncate(),
                                    )
                                    .on_hover_text(doc_path.display().to_string());
                                }
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(file_name)
                                            .size(SMALL)
                                            .color(colors().text)
                                            .strong(),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(doc_path.display().to_string());
                            });
                        });
                    },
                );
            });
        if open_find {
            self.show_find();
        }
        if toggle_terminal {
            self.preferences.tools_open = !self.preferences.tools_open;
        }
        if detach_active {
            self.detach(index);
            return;
        }
        let doc = &mut self.documents[index];
        ui.add_space(4.0);
        let id = Id::new(("document", &doc.path));
        let selection = self.selection.take();
        if let Some((start, end)) = selection {
            let mut state = TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
            state.cursor.set_char_range(Some(CCursorRange::two(
                CCursor::new(start),
                CCursor::new(end),
            )));
            state.store(ui.ctx(), id);
        }
        let focus = std::mem::take(&mut self.focus_editor);
        let language = doc
            .path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_owned();
        ScrollArea::both()
            .id_salt(id.with("scroll"))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    let (_, gutter) = ui.allocate_space(egui::vec2(44.0, 1.0));
                    let mut layouter = |ui: &Ui, buffer: &dyn egui::TextBuffer, _width: f32| {
                        let text = buffer.as_str();
                        let font_size = TextStyle::Monospace.resolve(ui.style()).size;
                        let line_height = font_size
                            * if ui.spacing().interact_size.y <= 24.0 {
                                COMPACT_CODE_LINE_HEIGHT
                            } else {
                                CODE_LINE_HEIGHT
                            };
                        let mut job = if language.is_empty() || text.len() > 512 * 1024 {
                            let mut job = LayoutJob::simple(
                                text.to_owned(),
                                TextStyle::Monospace.resolve(ui.style()),
                                colors().text,
                                f32::INFINITY,
                            );
                            for section in &mut job.sections {
                                section.format.line_height = Some(line_height);
                            }
                            job
                        } else {
                            super::syntax::highlight(
                                ui.ctx(),
                                text,
                                &language,
                                font_size,
                                line_height,
                            )
                        };
                        job.wrap.max_width = f32::INFINITY;
                        ui.fonts_mut(|fonts| fonts.layout_job(job))
                    };
                    let decorations = ui.painter().add(egui::Shape::Noop);
                    let popup_key = super::lsp::popup_keys(ui, &self.lsp, id, &doc.path);
                    let output = TextEdit::multiline(&mut doc.text)
                        .id(id)
                        .code_editor()
                        .frame(false)
                        .margin(Margin::symmetric(8, 8))
                        .desired_rows(18)
                        .desired_width(ui.available_width())
                        .layouter(&mut layouter)
                        .show(ui);
                    super::lsp::after_edit(ui, &mut self.lsp, doc, id, &output, popup_key);
                    if focus {
                        output.response.request_focus();
                    }
                    if output.response.changed() {
                        doc.update_dirty();
                        self.find_match = None;
                        self.find_empty = false;
                    }
                    if let Some(cursor) = output.cursor_range {
                        self.cursor = cursor.primary.index;
                    }
                    if let Some((_, end)) = selection {
                        let cursor = output
                            .galley
                            .pos_from_cursor(CCursor::new(end))
                            .translate(output.galley_pos.to_vec2());
                        ui.scroll_to_rect(cursor, Some(Align::Center));
                    }
                    if !output.response.has_focus()
                        && let Some((start, end)) = self.find_match
                    {
                        let start = output.galley.pos_from_cursor(CCursor::new(start));
                        let end = output.galley.pos_from_cursor(CCursor::new(end));
                        let rect = start.union(end).translate(output.galley_pos.to_vec2());
                        ui.painter()
                            .rect_filled(rect, 2, colors().accent.gamma_multiply(0.30));
                    }
                    let clip = ui.clip_rect();
                    let rows = &output.galley.rows;
                    let origin = output.galley_pos;
                    let palette = super::syntax::current();
                    let font = TextStyle::Monospace.resolve(ui.style());
                    let space = ui.fonts_mut(|f| f.glyph_width(&font, ' ')).max(1.0);
                    let caret = output.galley.pos_from_cursor(CCursor::new(self.cursor));
                    let active_row = rows
                        .iter()
                        .position(|row| {
                            caret.center().y >= row.pos.y
                                && caret.center().y < row.pos.y + row.size.y
                        })
                        .unwrap_or(0);
                    let first =
                        rows.partition_point(|row| origin.y + row.pos.y + row.size.y < clip.top());
                    let last = rows
                        .partition_point(|row| origin.y + row.pos.y < clip.bottom())
                        .min(rows.len());
                    let mut shapes = Vec::new();
                    // Active line band across gutter and text.
                    if let Some(row) = rows.get(active_row) {
                        let y = origin.y + row.pos.y;
                        shapes.push(egui::Shape::rect_filled(
                            egui::Rect::from_min_max(
                                egui::pos2(gutter.left(), y),
                                egui::pos2(clip.right(), y + row.size.y),
                            ),
                            0,
                            palette.active_line,
                        ));
                    }
                    // Indent guides at columns 0, 4, 8… inside leading whitespace. Blank
                    // lines inherit the indentation of the next non-blank line.
                    let scan_end = (last + 200).min(rows.len());
                    let scan_start = first.min(active_row);
                    let mut levels = vec![0usize; scan_end.saturating_sub(scan_start)];
                    let mut carry = 0;
                    for i in (scan_start..scan_end).rev() {
                        let level = leading_indent(&rows[i]).unwrap_or(carry);
                        carry = level;
                        levels[i - scan_start] = level;
                    }
                    // Guide spacing follows the file: the smallest indentation in view.
                    let unit = levels
                        .iter()
                        .copied()
                        .filter(|l| *l > 0)
                        .min()
                        .unwrap_or(4)
                        .clamp(2, 8);
                    let level_at = |i: usize| levels.get(i.wrapping_sub(scan_start)).copied();
                    let active_col = level_at(active_row)
                        .unwrap_or(0)
                        .checked_sub(1)
                        .map(|c| c - c % unit);
                    let in_block = |i: usize| -> bool {
                        let Some(col) = active_col else { return false };
                        let (lo, hi) = if i <= active_row {
                            (i, active_row)
                        } else {
                            (active_row, i)
                        };
                        (lo..=hi).all(|r| level_at(r).is_none_or(|l| l > col))
                    };
                    for (i, row) in rows.iter().enumerate().take(last).skip(first) {
                        let level = level_at(i).unwrap_or(0);
                        let y = origin.y + row.pos.y;
                        let mut col = 0;
                        while col < level {
                            let x = (origin.x + col as f32 * space + space * 0.5).round() + 0.5;
                            let lit = Some(col) == active_col && in_block(i);
                            shapes.push(egui::Shape::line_segment(
                                [egui::pos2(x, y), egui::pos2(x, y + row.size.y)],
                                egui::Stroke::new(
                                    1.0_f32,
                                    if lit {
                                        palette.guide_active
                                    } else {
                                        palette.guide
                                    },
                                ),
                            ));
                            col += unit;
                        }
                    }
                    // Bracket matching around the caret.
                    if let Some((a, b)) = matching_bracket(&doc.text, self.cursor) {
                        for index in [a, b] {
                            let r = output
                                .galley
                                .pos_from_cursor(CCursor::new(index))
                                .translate(origin.to_vec2());
                            let rect =
                                egui::Rect::from_min_size(r.min, egui::vec2(space, r.height()));
                            shapes.push(egui::Shape::rect_filled(
                                rect,
                                2,
                                palette.bracket.gamma_multiply(0.16),
                            ));
                            shapes.push(egui::Shape::rect_stroke(
                                rect,
                                2,
                                egui::Stroke::new(1.0_f32, palette.bracket.gamma_multiply(0.7)),
                                egui::StrokeKind::Inside,
                            ));
                        }
                    }
                    ui.painter().set(decorations, egui::Shape::Vec(shapes));
                    ui.painter().vline(
                        gutter.right() + 2.0,
                        clip.y_range(),
                        egui::Stroke::new(1.0_f32, colors().border.gamma_multiply(0.5)),
                    );
                    for (index, row) in rows.iter().enumerate().take(last).skip(first) {
                        ui.painter().text(
                            egui::pos2(gutter.right() - 5.0, origin.y + row.pos.y),
                            egui::Align2::RIGHT_TOP,
                            (index + 1).to_string(),
                            font.clone(),
                            if index == active_row {
                                colors().secondary
                            } else {
                                colors().disabled
                            },
                        );
                    }
                });
            });
    }

    fn tabs(&mut self, ui: &mut Ui) {
        let mut active = None;
        let mut close = None;
        let mut detach = None;
        let mut new_file = false;
        let mut toggle_focus = false;
        let bar = egui::Frame::new()
            .inner_margin(Margin {
                left: 8,
                right: 8,
                top: 5,
                bottom: 0,
            })
            .show(ui, |ui| {
                let actions_w = if self.root.is_some() { 68.0 } else { 36.0 };
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(
                            (ui.available_width() - actions_w).max(80.0),
                            TAB_HEIGHT - 6.0,
                        ),
                        Layout::left_to_right(Align::Center),
                        |ui| {
                            ScrollArea::horizontal()
                                .id_salt("tabs")
                                .auto_shrink([false, true])
                                .scroll_bar_visibility(
                                    egui::scroll_area::ScrollBarVisibility::AlwaysHidden,
                                )
                                .show(ui, |ui| {
                                    ui.spacing_mut().item_spacing.x = 3.0;
                                    ui.horizontal(|ui| {
                                        let mut indicator = None;
                                        let tab_height = TAB_HEIGHT - 8.0;
                                        // Reserved before the tabs so the pill paints underneath them.
                                        let pill_slot = ui.painter().add(egui::Shape::Noop);
                                        for (index, document) in self.documents.iter().enumerate() {
                                            if self.clean.is_floating(&document.path) {
                                                continue;
                                            }
                                            let selected = self.active == Some(index);
                                            let name = workspace::filename(&document.path);
                                            let label_width = ui.fonts_mut(|fonts| {
                                                fonts
                                                    .layout_no_wrap(
                                                        name.clone(),
                                                        egui::FontId::proportional(BODY),
                                                        colors().text,
                                                    )
                                                    .size()
                                                    .x
                                            });
                                            let width = (label_width + 62.0).clamp(96.0, 232.0);
                                            let response = ui.add_sized(
                                                [width, tab_height],
                                                egui::Button::new("").frame(false),
                                            );
                                            #[cfg(test)]
                                            super::motion::record(
                                                ui,
                                                format!("Tab {}", name),
                                                response.rect,
                                            );
                                            let painter = ui.painter_at(response.rect.expand(2.0));
                                            let hover = super::motion::spring(
                                                ui.ctx(),
                                                response.id.with("hover"),
                                                if response.hovered() && !selected {
                                                    1.0
                                                } else {
                                                    0.0
                                                },
                                                super::motion::Spring::SNAPPY,
                                            );
                                            if hover > 0.01 {
                                                painter.rect_filled(
                                                    response.rect,
                                                    7,
                                                    colors().hover.gamma_multiply(hover * 0.8),
                                                );
                                            }
                                            if selected {
                                                indicator = Some(response.rect);
                                            }
                                            super::assets::file(
                                                ui,
                                                response.rect.left_center()
                                                    + egui::vec2(16.0, 0.0),
                                                &document.path,
                                            );
                                            let text_rect = egui::Rect::from_min_max(
                                                response.rect.left_top() + egui::vec2(30.0, 0.0),
                                                response.rect.right_bottom()
                                                    - egui::vec2(26.0, 0.0),
                                            );
                                            painter.with_clip_rect(text_rect).text(
                                                text_rect.left_center(),
                                                egui::Align2::LEFT_CENTER,
                                                &name,
                                                egui::FontId::proportional(BODY),
                                                if selected {
                                                    colors().text
                                                } else {
                                                    colors().secondary
                                                },
                                            );
                                            let close_rect = egui::Rect::from_center_size(
                                                response.rect.right_center()
                                                    - egui::vec2(14.0, 0.0),
                                                egui::vec2(20.0, 20.0),
                                            );
                                            let close_response = ui.interact(
                                                close_rect,
                                                Id::new(("tab-close", &document.path)),
                                                egui::Sense::click(),
                                            );
                                            if close_response.hovered() {
                                                painter.rect_filled(close_rect, 5, colors().hover);
                                                draw_icon_sized(
                                                    &painter,
                                                    close_rect.center(),
                                                    Icon::Close,
                                                    colors().text,
                                                    14.0,
                                                );
                                            } else if document.dirty {
                                                painter.circle_filled(
                                                    close_rect.center(),
                                                    3.5,
                                                    colors().accent,
                                                );
                                            } else if selected || response.hovered() {
                                                draw_icon_sized(
                                                    &painter,
                                                    close_rect.center(),
                                                    Icon::Close,
                                                    colors().muted,
                                                    14.0,
                                                );
                                            }
                                            response.widget_info(|| {
                                                egui::WidgetInfo::labeled(
                                                    egui::WidgetType::SelectableLabel,
                                                    true,
                                                    &name,
                                                )
                                            });
                                            if close_response.clicked() {
                                                close = Some(index);
                                            } else if response.clicked() {
                                                active = Some(index);
                                            }
                                            if response.middle_clicked() {
                                                close = Some(index);
                                            }
                                            response.context_menu(|ui| {
                                                if ui
                                                    .button(tr("Detach to floating editor"))
                                                    .clicked()
                                                {
                                                    detach = Some(index);
                                                    ui.close();
                                                }
                                                if ui.button(tr("Close tab")).clicked() {
                                                    close = Some(index);
                                                    ui.close();
                                                }
                                            });
                                            response.on_hover_text(
                                                document.path.display().to_string(),
                                            );
                                            close_response.on_hover_text(tr("Close tab"));
                                        }
                                        if let Some(rect) = indicator {
                                            // The selected pill and its accent rule travel between tabs
                                            // on a damped spring instead of jumping.
                                            let pill = super::motion::spring_rect(
                                                ui.ctx(),
                                                Id::new("active-tab-pill"),
                                                rect,
                                                super::motion::Spring::SMOOTH,
                                            );
                                            ui.painter().set(
                                                pill_slot,
                                                egui::Shape::Vec(vec![
                                                    egui::Shape::rect_filled(
                                                        pill,
                                                        7,
                                                        colors().elevated,
                                                    ),
                                                    egui::Shape::rect_stroke(
                                                        pill,
                                                        7,
                                                        egui::Stroke::new(
                                                            1.0_f32,
                                                            colors().border,
                                                        ),
                                                        egui::StrokeKind::Inside,
                                                    ),
                                                ]),
                                            );
                                            ui.painter().hline(
                                                (pill.left() + 10.0)..=(pill.right() - 10.0),
                                                pill.bottom() + 3.0,
                                                egui::Stroke::new(2.0_f32, colors().accent),
                                            );
                                        }
                                    });
                                });
                        },
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        if icon_button(ui, Icon::Focus, "Focus mode", self.clean.focus).clicked() {
                            toggle_focus = true;
                        }
                        if self.root.is_some()
                            && icon_button(ui, Icon::Add, "New file", false).clicked()
                        {
                            new_file = true;
                        }
                    });
                });
            });
        ui.painter().hline(
            bar.response.rect.x_range(),
            bar.response.rect.bottom() + 3.5,
            egui::Stroke::new(1.0_f32, colors().border),
        );
        ui.add_space(4.0);
        if toggle_focus {
            self.clean.focus = !self.clean.focus;
        }
        if new_file {
            self.show_new_file();
        }
        if let Some(index) = detach {
            self.detach(index);
        }
        if let Some(index) = active {
            self.activate(index);
        }
        if let Some(index) = close {
            self.request(Pending::CloseTab(index), ui.ctx());
        }
    }

    fn find_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let response = ui.add(
                TextEdit::singleline(&mut self.find_query)
                    .hint_text(tr("Find in file"))
                    .desired_width(210.0)
                    .char_limit(200),
            );
            if std::mem::take(&mut self.find_focus) {
                response.request_focus();
            }
            if response.changed() {
                self.find_match = None;
                self.find(false);
            }
            let entered = response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter));
            if entered
                || ui
                    .small_button("↓")
                    .on_hover_text(tr("Next match (F3)"))
                    .clicked()
            {
                self.find(false);
                response.request_focus();
            }
            if ui
                .small_button("↑")
                .on_hover_text(tr("Previous match (Shift+F3)"))
                .clicked()
            {
                self.find(true);
                response.request_focus();
            }
            if ui
                .small_button("×")
                .on_hover_text(tr("Close find"))
                .clicked()
            {
                self.find_open = false;
                self.focus_editor = true;
            }
            if self.find_empty {
                ui.label(muted(tr("No matches")));
            }
        });
    }

    pub(super) fn assistant_panel(&mut self, ctx: &Context) {
        let max_width = (ctx.content_rect().width() * 0.30)
            .min(
                ctx.content_rect().width()
                    - RAIL_WIDTH
                    - self.preferences.sidebar_width
                    - MIN_EDITOR_WIDTH,
            )
            .clamp(240.0, 520.0);
        let id = Id::new("assistant-panel-motion");
        let progress = super::motion::spring(
            ctx,
            id,
            if self.preferences.assistant_open {
                1.0
            } else {
                0.0
            },
            super::motion::Spring::SMOOTH,
        )
        .clamp(0.0, 1.0);
        let previous = ctx.data_mut(|d| {
            let previous = d.get_temp::<f32>(id.with("previous")).unwrap_or(progress);
            d.insert_temp(id.with("previous"), progress);
            previous
        });
        if progress <= 0.001 {
            return;
        }
        let desired = self.preferences.assistant_width.min(max_width).max(240.0);
        let animating = progress < 0.999 || previous < 0.999;
        let panel = SidePanel::right("assistant")
            .default_width(desired)
            .frame(island(Margin::same(12)));
        let panel = if animating {
            panel
                .exact_width((desired * progress).max(1.0))
                .resizable(false)
        } else {
            panel.width_range(240.0..=max_width)
        };
        let result = panel.show(ctx, |ui| {
            ui.set_min_width(ui.available_width());
            let origin = ui.available_rect_before_wrap().min;
            let rect = egui::Rect::from_min_size(
                origin,
                egui::vec2((desired - 24.0).max(1.0), ui.available_height()),
            );
            let mut content = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt("assistant-content")
                    .max_rect(rect),
            );
            content.set_clip_rect(ui.clip_rect());
            content.multiply_opacity(progress);
            self.agent_dock(&mut content);
        });
        let width = result.response.rect.width();
        ctx.layer_painter(egui::LayerId::background()).vline(
            result.response.rect.left(),
            result.response.rect.y_range(),
            egui::Stroke::new(1.0_f32, colors().border),
        );
        if !animating && width + 1.0 < max_width {
            self.preferences.assistant_width = width;
        }
    }

    pub(super) fn modals(&mut self, ctx: &Context) {
        if let Some(action) = self.pending.take() {
            let modal = egui::Modal::new(Id::new("close_confirmation"))
                .frame(modal_frame())
                .show(ctx, |ui| {
                    ui.heading("Save your changes?");
                    ui.label(if matches!(action, Pending::CloseTab(_)) {
                        "This file has unsaved changes."
                    } else {
                        "One or more files have unsaved changes."
                    });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.add(primary(tr("Save"))).clicked() {
                            return Some(true);
                        }
                        if ui.button(tr("Discard")).clicked() {
                            return Some(false);
                        }
                        if ui.button(tr("Cancel")).clicked() {
                            ui.close();
                        }
                        None
                    })
                    .inner
                });
            match modal.inner {
                Some(false) => self.perform(action, ctx),
                Some(true) => {
                    let saved = match action {
                        Pending::CloseTab(index) => self.documents[index]
                            .save()
                            .map_err(|err| format!("Could not save file. {err}")),
                        _ => {
                            if self.save_all() {
                                Ok(())
                            } else {
                                Err(String::new())
                            }
                        }
                    };
                    match saved {
                        Ok(()) => self.perform(action, ctx),
                        Err(error) if !error.is_empty() => self.error(error),
                        Err(_) => {}
                    }
                }
                None if !modal.should_close() => self.pending = Some(action),
                None => {}
            }
        }
        if let Some(mut name) = self.new_file.take() {
            let modal = egui::Modal::new(Id::new("new_file"))
                .frame(modal_frame())
                .show(ctx, |ui| {
                    ui.heading(tr("New file"));
                    if let Some((true, message)) = &self.notice {
                        ui.label(RichText::new(message).color(colors().error));
                    }
                    let input = ui.add(
                        TextEdit::singleline(&mut name)
                            .hint_text(tr("Path relative to the workspace"))
                            .desired_width(310.0)
                            .char_limit(260),
                    );
                    if std::mem::take(&mut self.focus_new_file) {
                        input.request_focus();
                    }
                    let enter =
                        input.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter));
                    ui.add_space(5.0);
                    ui.horizontal(|ui| {
                        if ui.button(tr("Cancel")).clicked() {
                            return Some(false);
                        }
                        if ui
                            .add_enabled(!name.trim().is_empty(), primary(tr("Create")))
                            .clicked()
                            || (enter && !name.trim().is_empty())
                        {
                            return Some(true);
                        }
                        None
                    })
                    .inner
                });
            match modal.inner {
                Some(true) if !self.create_file(&name) => self.new_file = Some(name),
                None if !modal.should_close() => self.new_file = Some(name),
                _ => {}
            }
        }
    }
}

/// Leading indentation of a laid-out row in columns, or `None` for a blank row.
fn leading_indent(row: &egui::epaint::text::Row) -> Option<usize> {
    let mut n = 0;
    for glyph in &row.glyphs {
        match glyph.chr {
            ' ' => n += 1,
            '\t' => n += 4,
            '\n' | '\r' => return None,
            _ => return Some(n),
        }
    }
    None
}

/// Character indices of the bracket pair touching `cursor`, if any.
fn matching_bracket(text: &str, cursor: usize) -> Option<(usize, usize)> {
    const LIMIT: usize = 40_000;
    const PAIRS: [(char, char); 3] = [('(', ')'), ('[', ']'), ('{', '}')];
    let start = cursor.saturating_sub(1);
    let window: Vec<char> = text.chars().skip(start).take(2).collect();
    for (offset, c) in window.iter().enumerate() {
        let at = start + offset;
        if let Some(&(open, close)) = PAIRS.iter().find(|(o, _)| o == c) {
            let mut depth = 0usize;
            for (i, ch) in text.chars().skip(at).take(LIMIT).enumerate() {
                if ch == open {
                    depth += 1;
                } else if ch == close {
                    depth -= 1;
                    if depth == 0 {
                        return Some((at, at + i));
                    }
                }
            }
        } else if let Some(&(open, close)) = PAIRS.iter().find(|(_, cl)| cl == c) {
            let base = at.saturating_sub(LIMIT);
            let before: Vec<char> = text.chars().skip(base).take(at - base + 1).collect();
            let mut depth = 0usize;
            for (i, ch) in before.iter().enumerate().rev() {
                if *ch == close {
                    depth += 1;
                } else if *ch == open {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some((base + i, at));
                    }
                }
            }
        }
    }
    None
}

const TREE_INDENT: f32 = 13.0;
const TREE_ROW: f32 = 24.0;

thread_local! {
    static EXPLORER_ZOOM: std::cell::Cell<f32> = const { std::cell::Cell::new(1.0) };
}

fn explorer_zoom() -> f32 {
    EXPLORER_ZOOM.with(|z| z.get())
}

/// One Explorer row: indent guides, animated chevron for folders, upstream file icon,
/// label, animated hover and selection with an accent marker.
fn tree_row(
    ui: &mut Ui,
    depth: usize,
    name: &str,
    icon: &'static str,
    openness: Option<f32>,
    selected: bool,
) -> egui::Response {
    let z = explorer_zoom();
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), (TREE_ROW * z).round()),
        egui::Sense::click(),
    );
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::SelectableLabel, true, name));
    let painter = ui.painter_at(rect);
    let hover = super::motion::state(
        ui,
        response.id.with("tree-hover"),
        response.hovered(),
        super::motion::HOVER,
    );
    let selection = super::motion::state(
        ui,
        response.id.with("tree-selection"),
        selected,
        super::motion::CONTROL,
    );
    let fill = super::motion::blend(
        colors().hover.gamma_multiply(hover),
        colors().accent_bg,
        selection,
    );
    painter.rect_filled(rect, 5, fill);
    if selection > 0.01 {
        let h = (TREE_ROW * z - 10.0) * selection;
        painter.rect_filled(
            egui::Rect::from_center_size(
                rect.left_center() + egui::vec2(1.0, 0.0),
                egui::vec2(2.0, h),
            ),
            1,
            colors().accent,
        );
    }
    // Indent guides, brighter on the hovered branch.
    for level in 0..depth {
        let x = rect.left() + 12.0 * z + level as f32 * TREE_INDENT * z;
        painter.vline(
            x,
            rect.y_range(),
            egui::Stroke::new(
                1.0_f32,
                colors()
                    .border
                    .gamma_multiply(if hover > 0.0 && level + 1 == depth {
                        1.6
                    } else {
                        0.9
                    }),
            ),
        );
    }
    let x = rect.left() + 6.0 * z + depth as f32 * TREE_INDENT * z;
    let center_y = rect.center().y;
    if let Some(open) = openness {
        // Lucide chevron, rotated from right (closed) to down (open).
        let texture = super::assets::texture(ui.ctx(), "lucide-chevron-right");
        egui::Image::from_texture(&texture)
            .tint(super::motion::blend(colors().muted, colors().text, hover))
            .rotate(open * std::f32::consts::FRAC_PI_2, egui::Vec2::splat(0.5))
            .paint_at(
                ui,
                egui::Rect::from_center_size(
                    egui::pos2(x + 6.0 * z, center_y),
                    egui::vec2(14.0 * z, 14.0 * z),
                ),
            );
    }
    super::assets::paint(
        ui,
        egui::Rect::from_center_size(
            egui::pos2(x + 23.0 * z, center_y),
            egui::vec2(16.0 * z, 16.0 * z),
        ),
        icon,
    );
    let label_x = x + 36.0 * z;
    let max_width = (rect.right() - label_x - 6.0).max(1.0);
    let galley = ui.fonts_mut(|fonts| {
        fonts.layout_job(egui::text::LayoutJob {
            wrap: egui::text::TextWrapping {
                max_width,
                max_rows: 1,
                break_anywhere: true,
                ..Default::default()
            },
            ..egui::text::LayoutJob::simple(
                name.to_owned(),
                egui::FontId::proportional(BODY * z),
                if selected || hover > 0.5 {
                    colors().text
                } else {
                    super::motion::blend(colors().secondary, colors().text, hover)
                },
                max_width,
            )
        })
    });
    painter.galley(
        egui::pos2(label_x, center_y - galley.size().y / 2.0),
        galley,
        colors().text,
    );
    if response.has_focus() {
        painter.rect_stroke(
            rect,
            5,
            egui::Stroke::new(1.0_f32, colors().accent),
            egui::StrokeKind::Inside,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn tree(
    ui: &mut Ui,
    root: &Path,
    path: &Path,
    active: Option<&Path>,
    filter: &str,
    directories: &mut std::collections::HashMap<std::path::PathBuf, super::DirectoryLoad>,
    depth: usize,
) -> Option<super::files::TreeAction> {
    use super::{
        DirectoryLoad,
        files::{TreeAction, tree_menu},
    };
    use std::sync::mpsc::{self, TryRecvError};

    let loading = |ui: &mut Ui| {
        ui.horizontal(|ui| {
            ui.add_space(12.0 + depth as f32 * TREE_INDENT);
            ui.label(muted(tr("Loading…")));
        });
    };
    let entries = match directories.remove(path) {
        Some(DirectoryLoad::Ready(entries)) => entries,
        Some(DirectoryLoad::Pending(receiver)) => match receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => {
                directories.insert(path.to_owned(), DirectoryLoad::Pending(receiver));
                loading(ui);
                return None;
            }
            Err(TryRecvError::Disconnected) => Err("Directory scan was interrupted.".into()),
        },
        None => {
            let folder = path.to_owned();
            let ctx = ui.ctx().clone();
            let (sender, receiver) = mpsc::channel();
            std::thread::spawn(move || {
                let contents =
                    workspace::read_directory(&folder).map_err(|error| error.to_string());
                if sender.send(contents).is_ok() {
                    ctx.request_repaint();
                }
            });
            directories.insert(path.to_owned(), DirectoryLoad::Pending(receiver));
            loading(ui);
            return None;
        }
    };
    let mut selected = None;
    ui.spacing_mut().item_spacing.y = 1.0;
    match &entries {
        Ok(entries) => {
            for entry in entries {
                let name = workspace::filename(&entry.path);
                let matches = filter.is_empty() || name.to_ascii_lowercase().contains(filter);
                if entry.directory {
                    let id = ui.make_persistent_id(("tree", &entry.path));
                    let mut state =
                        egui::collapsing_header::CollapsingState::load_with_default_open(
                            ui.ctx(),
                            id,
                            false,
                        );
                    let openness = state.openness(ui.ctx());
                    let response = tree_row(
                        ui,
                        depth,
                        &name,
                        super::assets::folder_name(&entry.path, openness > 0.5),
                        Some(openness),
                        false,
                    )
                    .on_hover_text(entry.path.display().to_string());
                    #[cfg(test)]
                    super::motion::record(
                        ui,
                        format!(
                            "Folder {}",
                            entry.path.strip_prefix(root).unwrap().display()
                        ),
                        response.rect,
                    );
                    if response.clicked() {
                        state.toggle(ui);
                    }
                    response.context_menu(|ui| {
                        if let Some(action) = tree_menu(ui, root, &entry.path, true) {
                            selected = Some(action);
                        }
                    });
                    state.store(ui.ctx());
                    let child_filter = if matches { "" } else { filter };
                    if let Some(inner) = state.show_body_unindented(ui, |ui| {
                        tree(
                            ui,
                            root,
                            &entry.path,
                            active,
                            child_filter,
                            directories,
                            depth + 1,
                        )
                    }) && let Some(action) = inner.inner
                    {
                        selected = Some(action);
                    }
                } else {
                    if !matches {
                        continue;
                    }
                    let response = tree_row(
                        ui,
                        depth,
                        &name,
                        super::assets::file_name(&entry.path),
                        None,
                        active == Some(entry.path.as_path()),
                    )
                    .on_hover_text(entry.path.display().to_string());
                    #[cfg(test)]
                    super::motion::record(ui, format!("File {}", name), response.rect);
                    if response.clicked() {
                        selected = Some(TreeAction::Open(entry.path.clone()));
                    }
                    response.context_menu(|ui| {
                        if let Some(action) = tree_menu(ui, root, &entry.path, false) {
                            selected = Some(action);
                        }
                    });
                }
            }
        }
        Err(error) => {
            ui.label(RichText::new(error).small().color(colors().error));
        }
    }
    directories.insert(path.to_owned(), super::DirectoryLoad::Ready(entries));
    selected
}

/// Parse a shortcut such as "Ctrl+Shift+L" (Ctrl/Cmd map to the platform command key).
pub(super) fn parse_shortcut(text: &str) -> Option<(Modifiers, Key)> {
    let mut modifiers = Modifiers::NONE;
    let mut key = None;
    for part in text.split('+').map(str::trim).filter(|p| !p.is_empty()) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "cmd" | "command" => modifiers = modifiers.plus(Modifiers::COMMAND),
            "shift" => modifiers = modifiers.plus(Modifiers::SHIFT),
            "alt" | "option" => modifiers = modifiers.plus(Modifiers::ALT),
            other => {
                key = Key::from_name(&other.to_ascii_uppercase()).or_else(|| Key::from_name(other))
            }
        }
    }
    key.map(|k| (modifiers, k))
}

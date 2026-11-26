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
        for mode in super::modes::Mode::ALL {
            if shortcut(super::modes::Mode::modifiers(), mode.key()) {
                self.switch_mode(mode);
            }
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
        if shortcut(Modifiers::ALT, Key::Z) {
            self.preferences.word_wrap = !self.preferences.word_wrap;
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
            if self.entry_draft.is_none() && shortcut(Modifiers::NONE, Key::Escape) {
                self.find_open = false;
                self.focus_editor = true;
            }
        }
    }

    fn show_new_file(&mut self) {
        if let Some(dir) = self.editor_directory() {
            self.begin_entry(super::files::EntryKind::File, dir, None);
        }
    }

    pub(super) fn editor_directory(&self) -> Option<std::path::PathBuf> {
        self.active
            .and_then(|i| self.documents.get(i))
            .and_then(|doc| doc.path.parent())
            .or(self.root.as_deref())
            .map(Path::to_owned)
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
        let bar = TopBottomPanel::top("menu")
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
                let left_width = if rect.width() > 1100.0 { 400.0 } else { 290.0 };
                let right_width = if rect.width() > 1000.0 { 380.0 } else { 230.0 };
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
                        ui.label(
                            RichText::new(crate::brand::RELEASE)
                                .size(SMALL)
                                .color(colors().muted),
                        );
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
                let mut command = egui::Rect::from_center_size(
                    rect.center(),
                    egui::vec2(command_width.max(0.0), 30.0),
                );
                // Keep room for the back/forward arrows between the left zone and the field.
                command.min.x = command.min.x.max(rect.left() + left_width + 68.0);
                let command_width = command.width();
                if command_width >= 140.0 {
                    ui.scope_builder(egui::UiBuilder::new().max_rect(command), |ui| {
                        let response = ui.add_sized(
                            command.size(),
                            egui::Button::new("")
                                .fill(colors().inset)
                                .stroke(egui::Stroke::new(1.0_f32, colors().border))
                                .corner_radius(15),
                        );
                        let hover = super::motion::state(
                            ui,
                            response.id.with("command-hover"),
                            response.hovered(),
                            super::motion::HOVER,
                        );
                        if hover > 0.01 {
                            ui.painter().rect_stroke(
                                response.rect,
                                15,
                                egui::Stroke::new(
                                    1.0_f32,
                                    colors().control_border.gamma_multiply(hover),
                                ),
                                egui::StrokeKind::Inside,
                            );
                        }
                        // Interactive Luma avatar inside the left edge of the top-center pill
                        let luma_rect = egui::Rect::from_center_size(
                            response.rect.left_center() + egui::vec2(18.0, 0.0),
                            egui::vec2(26.0, 26.0),
                        );
                        let luma_click = ui.interact(
                            luma_rect,
                            Id::new("top-bar-luma-trigger"),
                            egui::Sense::click(),
                        );
                        let mood = self.agent_mood(ui.input(|i| i.focused));
                        self.identity.portrait(
                            ui,
                            luma_rect,
                            &self.preferences.identity,
                            mood,
                            self.preferences.reduce_motion,
                        );
                        if luma_click
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .on_hover_text(tr("Open Luma Notch"))
                            .clicked()
                        {
                            self.companion.open = !self.companion.open;
                        }
                        ui.painter().text(
                            response.rect.left_center() + egui::vec2(38.0, 0.0),
                            egui::Align2::LEFT_CENTER,
                            if command_width > 340.0 {
                                tr("Ask Luma or search files and commands")
                            } else {
                                tr("Quick access")
                            },
                            egui::FontId::proportional(BODY),
                            colors().secondary,
                        );
                        ui.painter().text(
                            response.rect.right_center() - egui::vec2(14.0, 0.0),
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
                            self.companion.open = true;
                        }
                    });
                    // Back and forward sit just left of the command field, like a browser.
                    let arrows = egui::Rect::from_min_max(
                        egui::pos2(command.left() - 64.0, rect.top()),
                        egui::pos2(command.left() - 4.0, rect.bottom()),
                    );
                    ui.scope_builder(
                        egui::UiBuilder::new()
                            .max_rect(arrows)
                            .layout(Layout::left_to_right(Align::Center)),
                        |ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            let back = ui
                                .add_enabled_ui(self.nav.can_go_back(), |ui| {
                                    icon_button(ui, Icon::Back, tr("Go back (Alt+Left)"), false)
                                })
                                .inner;
                            let forward = ui
                                .add_enabled_ui(self.nav.can_go_forward(), |ui| {
                                    icon_button(
                                        ui,
                                        Icon::ChevronRight,
                                        tr("Go forward (Alt+Right)"),
                                        false,
                                    )
                                })
                                .inner;
                            if back.clicked() {
                                self.go_back();
                            } else if forward.clicked() {
                                self.go_forward();
                            }
                        },
                    );
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
                        // One destination for every provider: Agents. Codex, Claude and
                        // Gemini are providers inside it, not separate places.
                        let compact = rect.width() <= 1000.0;
                        if let Some(mode) = super::modes::segmented(ui, self.mode_state(), compact)
                        {
                            self.switch_mode(mode);
                        }
                    },
                );
            });
        self.chrome_bottom = Some(bar.response.rect.bottom());
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
                        (Sidebar::Chats, Icon::History, "Conversations"),
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
                    let bell =
                        icon_button(ui, Icon::Bell, "Notifications", self.notifications_open);
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

    #[allow(dead_code)]
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
                        self.open_luma();
                    }
                    dock.vertical(|ui| {
                        ui.add_space(6.0);
                        ui.label(RichText::new(tr("Luma")).strong().color(colors().secondary));
                        ui.add(egui::Label::new(muted(mood.name())).truncate());
                        let order =
                            ui.add(egui::Button::new(muted(tr("Give an order ↗"))).frame(false));
                        if order.hovered() {
                            pointer_cursor(ui.ctx());
                        }
                        if order.clicked() {
                            self.open_luma();
                        }
                    });
                });
            });
    }

    /// Opens Luma's chat with the order box focused.
    pub(super) fn open_luma(&mut self) {
        self.companion.open = true;
        self.hub.agents.luma.focus = true;
    }

    pub(super) fn sidebar_visible(&self) -> bool {
        self.sidebar_open && (self.root.is_some() || self.sidebar_requested)
    }

    pub(super) fn sidebar(&mut self, ctx: &Context) {
        let zoom = self.preferences.explorer_zoom;
        EXPLORER_ZOOM.with(|z| z.set(zoom));
        // Bounds: never squeeze the editor below its minimum, never collapse the tree.
        let assistant = if self.preferences.assistant_open && !self.in_session_home() {
            self.preferences.assistant_width
        } else {
            0.0
        };
        let max = (ctx.content_rect().width() - RAIL_WIDTH - MIN_EDITOR_WIDTH - assistant)
            .clamp(180.0, 480.0);
        let min = (110.0 * zoom).min(max);
        let mut collapse_sidebar = false;
        let panel = SidePanel::left("workspace")
            .default_width(self.preferences.sidebar_width.clamp(min, max))
            .width_range(min..=max)
            .show_separator_line(false)
            .frame(sidebar_frame(Margin::symmetric(PANEL_PADDING, 8)))
            .show(ctx, |ui| {
                let mut selected = None;
                // Integrated compact header: collapse toggle, workspace pill, and view actions.
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if icon_button_sized(ui, Icon::PanelLeft, "Toggle sidebar", false, 24.0)
                        .clicked()
                    {
                        collapse_sidebar = true;
                    }
                    if let Some(root) = self.root.clone() {
                        let actions_w = if self.sidebar == Sidebar::Files {
                            50.0
                        } else {
                            0.0
                        };
                        let title_w = (ui.available_width() - actions_w).max(60.0);
                        let root_label = ui
                            .allocate_ui_with_layout(
                                egui::vec2(title_w, 28.0),
                                Layout::left_to_right(Align::Center),
                                |ui| {
                                    ui.set_min_width(ui.available_width());
                                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                                    let (_, icon_rect) = ui.allocate_space(egui::vec2(16.0, 18.0));
                                    draw_icon_sized(
                                        ui.painter(),
                                        icon_rect.center(),
                                        Icon::Folder,
                                        colors().muted,
                                        14.0,
                                    );
                                    self.workspace_picker(ui);
                                },
                            )
                            .response
                            .on_hover_text(root.display().to_string());
                        root_label.context_menu(|ui| {
                            selected = super::files::tree_menu(ui, &root, &root, true);
                        });
                        if self.sidebar == Sidebar::Files {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.spacing_mut().item_spacing.x = 2.0;
                                if icon_button_sized(
                                    ui,
                                    Icon::Refresh,
                                    "Refresh files",
                                    false,
                                    24.0,
                                )
                                .clicked()
                                {
                                    self.directories.clear();
                                }
                                if icon_button_sized(ui, Icon::FilePlus, "New file", false, 24.0)
                                    .clicked()
                                {
                                    self.begin_entry(
                                        super::files::EntryKind::File,
                                        root.clone(),
                                        None,
                                    );
                                }
                            });
                        }
                    } else {
                        let label = match self.sidebar {
                            Sidebar::Files => "Explorer",
                            Sidebar::Search => "Search",
                            Sidebar::Git => "Git",
                            Sidebar::Chats => "Conversations",
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
                if self.sidebar == Sidebar::Chats {
                    self.conversations_sidebar(ui);
                    return;
                }
                let Some(root) = self.root.clone() else {
                    if self.entry_draft.is_some() && self.sidebar == Sidebar::Files {
                        self.entry_row(ui);
                    } else {
                        ui.label(muted(tr("Open a folder to browse files.")));
                    }
                    return;
                };
                match self.sidebar {
                    Sidebar::Files => {
                        if self.entry_draft.is_some() {
                            self.entry_row(ui);
                        } else {
                            search_field(
                                ui,
                                "explorer-filter",
                                &mut self.explorer_filter,
                                "Filter files\u{2026}",
                                120,
                            );
                            ui.add_space(5.0);
                        }
                        let filter = self.explorer_filter.trim().to_ascii_lowercase();
                        let active = self
                            .active
                            .map(|index| self.documents[index].path.as_path());
                        let from_tree = ScrollArea::vertical()
                            .id_salt("files")
                            .show(ui, |ui| {
                                tree(ui, &root, &root, active, &filter, &mut self.directories, 0)
                            })
                            .inner;
                        let background = ui.allocate_response(
                            ui.available_size().max(egui::Vec2::ZERO),
                            egui::Sense::click(),
                        );
                        #[cfg(test)]
                        super::motion::record(ui, "Explorer background", background.rect);
                        let blank = super::files::creation_context(&background, &root);
                        if let Some(action) = selected.or(from_tree).or(blank) {
                            self.tree_action(action, ui.ctx());
                        }
                    }
                    Sidebar::Search => self.search_panel(ui, &root),
                    Sidebar::Git => self.git_panel(ui),
                    Sidebar::Chats => unreachable!(),
                }
            });
        let width = panel.response.rect.width();
        if collapse_sidebar || (width < 140.0 * zoom && !ctx.input(|i| i.pointer.any_down())) {
            self.toggle_sidebar();
            self.preferences.sidebar_width = 240.0;
        } else {
            self.preferences.sidebar_width = width;
        }
        self.explorer_rect = Some(panel.response.rect);
    }

    fn search_panel(&mut self, ui: &mut Ui, root: &Path) {
        let response = search_field(
            ui,
            "workspace-search",
            &mut self.workspace_query,
            "Find text in files",
            200,
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
        let directory = self.editor_directory();
        if let Some(dir) = &directory {
            let backdrop = ui.interact(
                ui.max_rect(),
                ui.id().with("editor-background"),
                egui::Sense::click(),
            );
            #[cfg(test)]
            super::motion::record(
                ui,
                "Home background",
                egui::Rect::from_min_size(
                    ui.max_rect().min + egui::vec2(24.0, 38.0),
                    egui::Vec2::splat(8.0),
                ),
            );
            if let Some(action) = super::files::creation_context(&backdrop, dir) {
                self.tree_action(action, ui.ctx());
            }
        }
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
        let mut context_action = None;
        let mut detach_active = false;
        let mut open_find = false;
        let mut toggle_terminal = false;
        let mut reveal = None;
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
                                ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
                                ui.spacing_mut().item_spacing.x = 0.0;
                                let (_, icon_rect) = ui.allocate_space(egui::vec2(18.0, 16.0));
                                super::assets::file(ui, icon_rect.center(), &doc_path);
                                let last = parts.len().saturating_sub(1);
                                for (depth, part) in parts.iter().enumerate() {
                                    if depth > 0 {
                                        ui.label(
                                            RichText::new("›").size(SMALL).color(colors().muted),
                                        );
                                    }
                                    let response = crumb(ui, part, depth == last);
                                    if response.clicked() {
                                        reveal = Some(depth);
                                    }
                                    response.on_hover_text(if depth == last {
                                        doc_path.display().to_string()
                                    } else {
                                        tr("Reveal in Explorer").to_owned()
                                    });
                                }
                            });
                        });
                    },
                );
            });
        if let Some(depth) = reveal {
            if depth + 1 >= parts.len() {
                self.focus_editor = true;
            } else {
                self.reveal_in_explorer(ui.ctx(), &doc_path);
            }
        }
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
        let id = Id::new(("document", &self.documents[index].path));
        if ui.memory(|m| m.has_focus(id))
            && ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::D))
        {
            let range = TextEdit::load_state(ui.ctx(), id)
                .and_then(|state| state.cursor.char_range())
                .map(|range| {
                    let (a, b) = (range.primary.index, range.secondary.index);
                    (a.min(b), a.max(b))
                })
                .unwrap_or((self.cursor, self.cursor));
            if let Some(next) = next_occurrence(&self.documents[index].text, range) {
                self.selection = Some(next);
            }
        }
        let doc = &mut self.documents[index];
        ui.add_space(4.0);
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
        let wrap = self.preferences.word_wrap;
        let use_ligatures = self.preferences.font_ligatures && !language.is_empty();
        let suggestions = self.preferences.inline_suggestions;
        let caret_before = self.cursor;
        let mut laid_ligatures = Vec::new();
        let mut scroll = ScrollArea::new([!wrap, true])
            .id_salt(id.with("scroll"))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    let (_, gutter) = ui.allocate_space(egui::vec2(48.0, 1.0));
                    let mut layouter = |ui: &Ui, buffer: &dyn egui::TextBuffer, width: f32| {
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
                        laid_ligatures = if use_ligatures {
                            let mut found = super::ligatures::find(text, caret_before);
                            super::ligatures::drawable(
                                ui.ctx(),
                                &TextStyle::Monospace.resolve(ui.style()),
                                &mut found,
                            );
                            found
                        } else {
                            Vec::new()
                        };
                        super::ligatures::hide(&mut job, &laid_ligatures);
                        job.wrap.max_width = if wrap { width } else { f32::INFINITY };
                        ui.fonts_mut(|fonts| fonts.layout_job(job))
                    };
                    let decorations = ui.painter().add(egui::Shape::Noop);
                    let popup_key = super::lsp::popup_keys(ui, &self.lsp, id, &doc.path);
                    if suggestions {
                        let popup_open = self.lsp.popup_open(&doc.path);
                        self.ghost.keys(ui, id, &doc.path, caret_before, popup_open);
                    }
                    // Pairs, smart Enter/Backspace, block indent, comments and line moves.
                    let edited = super::editing::keys(ui, id, &mut doc.text, &language);
                    let output = TextEdit::multiline(&mut doc.text)
                        .id(id)
                        .code_editor()
                        .frame(false)
                        .margin(Margin {
                            left: 12,
                            right: 12,
                            top: 12,
                            bottom: 24,
                        })
                        .desired_rows(18)
                        .desired_width(ui.available_width())
                        .layouter(&mut layouter)
                        .show(ui);
                    if let Some(dir) = &directory {
                        #[cfg(test)]
                        super::motion::record(
                            ui,
                            "Document text",
                            output.response.rect.intersect(ui.clip_rect()),
                        );
                        context_action = super::files::document_context(
                            &output.response,
                            dir,
                            &doc.path,
                            self.root.as_deref(),
                        );
                    }
                    super::lsp::after_edit(ui, &mut self.lsp, doc, id, &output, popup_key);
                    if suggestions {
                        let popup_open = self.lsp.popup_open(&doc.path);
                        if self.ghost.after_edit(
                            ui,
                            &mut doc.text,
                            &doc.path,
                            id,
                            &output,
                            popup_open,
                        ) {
                            doc.update_dirty();
                        }
                    }
                    if focus {
                        output.response.request_focus();
                    }
                    if edited || output.response.changed() {
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
                    let palette = super::syntax::current();
                    let font = TextStyle::Monospace.resolve(ui.style());
                    super::ligatures::paint(
                        ui.painter(),
                        &output,
                        &laid_ligatures,
                        &font,
                        palette.operator(),
                    );
                    let marks =
                        self.git_gutter
                            .marks(ui.ctx(), self.root.as_deref(), &doc.path, &doc.text);
                    let mut shapes = decorate(ui, &output, gutter, &doc.text, self.cursor, marks);
                    // Guided Learning Mode: translucent bracket-colored block hover + explanation card
                    if self.guided.open
                        && self.guided.session.is_some()
                        && self.preferences.guided.learn_hover
                        && let Some(hover_pos) = output.response.hover_pos()
                    {
                        let rows = &output.galley.rows;
                        let origin = output.galley_pos;
                        let lines = row_lines(rows);
                        if let Some(row_idx) = rows.iter().position(|r| {
                            let y0 = origin.y + r.pos.y;
                            hover_pos.y >= y0 && hover_pos.y < y0 + r.size.y
                        }) && let Some(&hovered_line) = lines.get(row_idx)
                            && let Some(info) = super::guided::learn_block_for_hover(
                                self.guided.session.as_ref(),
                                self.root.as_deref(),
                                self.preferences.guided.learn_hover,
                                self.preferences.guided.match_mode,
                                &doc.path,
                                &doc.text,
                                hovered_line,
                            )
                        {
                            let (start_l, end_l) = info.line_range;
                            let mut top_y: Option<f32> = None;
                            let mut bot_y: Option<f32> = None;
                            for (ri, r) in rows.iter().enumerate() {
                                let l = lines[ri];
                                if l >= start_l && l <= end_l {
                                    let y0 = origin.y + r.pos.y;
                                    let y1 = y0 + r.size.y;
                                    top_y = Some(top_y.map_or(y0, |t| t.min(y0)));
                                    bot_y = Some(bot_y.map_or(y1, |b| b.max(y1)));
                                }
                            }
                            if let (Some(y0), Some(y1)) = (top_y, bot_y) {
                                let clip = ui.clip_rect();
                                let block_rect = egui::Rect::from_min_max(
                                    egui::pos2(gutter.right() + 6.0, y0 - 2.0),
                                    egui::pos2(
                                        (clip.right() - 12.0).max(gutter.right() + 80.0),
                                        y1 + 2.0,
                                    ),
                                );
                                let tint = palette.bracket;
                                shapes.push(egui::Shape::rect_filled(
                                    block_rect,
                                    7,
                                    tint.gamma_multiply(0.11),
                                ));
                                shapes.push(egui::Shape::rect_stroke(
                                    block_rect,
                                    7,
                                    egui::Stroke::new(1.0_f32, tint.gamma_multiply(0.42)),
                                    egui::StrokeKind::Inside,
                                ));
                                shapes.push(egui::Shape::rect_filled(
                                    egui::Rect::from_min_size(
                                        block_rect.left_top() + egui::vec2(1.0, 5.0),
                                        egui::vec2(3.0, (block_rect.height() - 10.0).max(6.0)),
                                    ),
                                    2,
                                    tint.gamma_multiply(0.85),
                                ));
                                paint_learn_hover_card(
                                    ui.ctx(),
                                    hover_pos,
                                    block_rect,
                                    &info,
                                    tint,
                                );
                            }
                        }
                    }
                    ui.painter().set(decorations, egui::Shape::Vec(shapes));
                });
            });
        self.minimap_visor(ui, &mut scroll, index);
        if let Some(action) = context_action {
            self.tree_action(action, ui.ctx());
        }
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
                                                    12,
                                                    colors().hover.gamma_multiply(hover * 0.7),
                                                );
                                            }
                                            if selected {
                                                indicator = Some(response.rect);
                                            }
                                            super::assets::file(
                                                ui,
                                                response.rect.left_center() + egui::vec2(16.0, 0.0),
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
                                            response
                                                .on_hover_text(document.path.display().to_string());
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
                                                        12,
                                                        colors().elevated,
                                                    ),
                                                    egui::Shape::rect_stroke(
                                                        pill,
                                                        12,
                                                        egui::Stroke::new(1.0_f32, colors().border),
                                                        egui::StrokeKind::Inside,
                                                    ),
                                                ]),
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
        let _ = bar;
        ui.add_space(2.0);
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

    pub(super) fn in_session_home(&self) -> bool {
        self.root.is_some()
            && !self.hub_open
            && !(self.sidebar == Sidebar::Git && self.tools.showing_diff())
            && (self.documents.is_empty() || self.active.is_none())
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
        if self.in_session_home() {
            return;
        }
        let show_right_dock = self.preferences.assistant_open;
        let progress = super::motion::spring(
            ctx,
            id,
            if show_right_dock { 1.0 } else { 0.0 },
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
        let desired = self.preferences.assistant_width.min(max_width).max(220.0);
        let animating = progress < 0.999 || previous < 0.999;
        let panel = SidePanel::right("assistant")
            .default_width(desired)
            .show_separator_line(false)
            .frame(side_island(Margin::same(12)));
        let panel = if animating {
            panel
                .exact_width((desired * progress).max(1.0))
                .resizable(false)
        } else {
            panel.width_range(160.0..=max_width)
        };
        let result = panel.show(ctx, |ui| {
            let avail_w = ui.available_width().max(1.0);
            ui.set_min_width(avail_w);
            let origin = ui.available_rect_before_wrap().min;
            let rect =
                egui::Rect::from_min_size(origin, egui::vec2(avail_w, ui.available_height()));
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
        if !animating {
            if width < 195.0 && !ctx.input(|i| i.pointer.any_down()) {
                self.preferences.assistant_open = false;
                self.preferences.assistant_width = 300.0;
            } else if width + 1.0 < max_width {
                self.preferences.assistant_width = width;
            }
        }
    }

    pub(super) fn modals(&mut self, ctx: &Context) {
        if let Some(action) = self.pending.take() {
            let dirty_names: Vec<String> = match &action {
                Pending::CloseTab(index) => self
                    .documents
                    .get(*index)
                    .map(|d| vec![workspace::filename(&d.path)])
                    .unwrap_or_default(),
                _ => self
                    .documents
                    .iter()
                    .filter(|d| d.dirty)
                    .take(4)
                    .map(|d| workspace::filename(&d.path))
                    .collect(),
            };
            let modal =
                egui::Modal::new(Id::new("close_confirmation"))
                    .backdrop_color(egui::Color32::from_black_alpha(145))
                    .frame(modal_frame().inner_margin(Margin::same(22)))
                    .show(ctx, |ui| {
                        ui.set_min_width(340.0);
                        ui.set_max_width(400.0);
                        ui.horizontal(|ui| {
                            let (badge, _) = ui
                                .allocate_exact_size(egui::vec2(34.0, 34.0), egui::Sense::hover());
                            ui.painter().rect_filled(badge, 10, colors().accent_bg);
                            draw_icon_sized(
                                ui.painter(),
                                badge.center(),
                                Icon::Save,
                                colors().accent,
                                17.0,
                            );
                            ui.add_space(6.0);
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(tr("Save your changes?"))
                                        .size(LEAD + 1.0)
                                        .strong()
                                        .color(colors().text),
                                );
                                ui.label(secondary(if matches!(action, Pending::CloseTab(_)) {
                                    "This file has unsaved changes."
                                } else {
                                    "One or more files have unsaved changes."
                                }));
                            });
                        });
                        if !dirty_names.is_empty() {
                            ui.add_space(10.0);
                            egui::Frame::new()
                                .fill(colors().inset)
                                .stroke(egui::Stroke::new(1.0_f32, colors().border))
                                .corner_radius(9)
                                .inner_margin(Margin::symmetric(10, 7))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.label(
                                        RichText::new(dirty_names.join("  ·  "))
                                            .size(SMALL)
                                            .color(colors().secondary),
                                    );
                                });
                        }
                        ui.add_space(16.0);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            if pointer(ui.add(primary(tr("Save")).min_size(egui::vec2(88.0, 30.0))))
                                .clicked()
                            {
                                return Some(true);
                            }
                            let discard = egui::Button::new(
                                RichText::new(tr("Discard"))
                                    .size(BODY)
                                    .color(colors().error),
                            )
                            .fill(colors().error.gamma_multiply(0.12))
                            .stroke(egui::Stroke::new(
                                1.0_f32,
                                colors().error.gamma_multiply(0.35),
                            ))
                            .corner_radius(CONTROL_RADIUS)
                            .min_size(egui::vec2(88.0, 30.0));
                            if pointer(ui.add(discard)).clicked() {
                                return Some(false);
                            }
                            if pointer(ui.add(
                                egui::Button::new(tr("Cancel")).min_size(egui::vec2(82.0, 30.0)),
                            ))
                            .clicked()
                            {
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

/// Floating educational tooltip shown when hovering a highlighted code block in Guided Mode.
fn paint_learn_hover_card(
    ctx: &Context,
    hover_pos: egui::Pos2,
    block_rect: egui::Rect,
    info: &super::guided::LearnBlockInfo,
    tint: egui::Color32,
) {
    let screen = ctx.content_rect();
    let card_w = 340.0_f32.min((screen.width() - 32.0).max(220.0));
    let x = (hover_pos.x + 22.0)
        .min(screen.right() - card_w - 16.0)
        .max(screen.left() + 12.0);
    let below_y = (block_rect.bottom() + 8.0).min(screen.bottom() - 210.0);
    let y = if below_y > hover_pos.y + 16.0 {
        below_y
    } else {
        (block_rect.top() - 160.0).max(screen.top() + CHROME_HEIGHT + 8.0)
    };
    egui::Area::new(Id::new("guided-learn-hover-card"))
        .order(egui::Order::Tooltip)
        .fixed_pos(egui::pos2(x, y))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(colors().elevated)
                .stroke(egui::Stroke::new(1.0_f32, tint.gamma_multiply(0.55)))
                .corner_radius(11)
                .shadow(floating_shadow(active(), 22))
                .inner_margin(Margin::symmetric(13, 10))
                .show(ui, |ui| {
                    ui.set_width(card_w);
                    ui.horizontal(|ui| {
                        let (dot, _) =
                            ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                        ui.painter().circle_filled(dot.center(), 4.0, tint);
                        ui.add(
                            egui::Label::new(
                                RichText::new(&info.title)
                                    .size(SMALL)
                                    .strong()
                                    .color(colors().text),
                            )
                            .truncate(),
                        );
                    });
                    ui.add_space(5.0);
                    ui.label(
                        RichText::new(tr("This line"))
                            .size(10.5)
                            .strong()
                            .color(tint),
                    );
                    ui.add(
                        egui::Label::new(
                            RichText::new(&info.line_summary)
                                .size(SMALL)
                                .color(colors().text),
                        )
                        .wrap(),
                    );
                    if let Some(why) = info.why.as_deref().filter(|s| !s.trim().is_empty()) {
                        ui.add_space(5.0);
                        ui.label(
                            RichText::new(tr("Why this block"))
                                .size(10.5)
                                .strong()
                                .color(colors().muted),
                        );
                        ui.add(
                            egui::Label::new(
                                RichText::new(why)
                                    .size(SMALL)
                                    .italics()
                                    .color(colors().secondary),
                            )
                            .wrap(),
                        );
                    }
                    if let Some(detail) = info.detail.as_deref().filter(|s| !s.trim().is_empty()) {
                        ui.add_space(4.0);
                        ui.add(
                            egui::Label::new(
                                RichText::new(detail).size(SMALL).color(colors().text),
                            )
                            .wrap(),
                        );
                    }
                    if !info.calls.is_empty() {
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(tr("Call flow"))
                                .size(10.5)
                                .strong()
                                .color(colors().muted),
                        );
                        for call in info.calls.iter().take(4) {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(format!("→  {call}"))
                                        .size(11.5)
                                        .color(colors().text),
                                )
                                .wrap(),
                            );
                        }
                    }
                });
        });
}

/// Logical line index of every laid-out row; wrapped continuation rows share their
/// line's index.
fn row_lines(rows: &[egui::epaint::text::PlacedRow]) -> Vec<usize> {
    let mut line = 0;
    let mut out = Vec::with_capacity(rows.len());
    for (i, _) in rows.iter().enumerate() {
        if i > 0 && rows[i - 1].ends_with_newline {
            line += 1;
        }
        out.push(line);
    }
    out
}

/// Editor decorations for one frame. Returns the shapes drawn under the text (active line,
/// indent guides, bracket pair) and paints the gutter directly: line numbers with the
/// active one highlighted and Git change bars.
fn decorate(
    ui: &Ui,
    output: &egui::text_edit::TextEditOutput,
    gutter: egui::Rect,
    text: &str,
    cursor: usize,
    marks: Option<&[Option<super::git_gutter::Mark>]>,
) -> Vec<egui::Shape> {
    let clip = ui.clip_rect();
    let rows = &output.galley.rows;
    let origin = output.galley_pos;
    let palette = super::syntax::current();
    let font = TextStyle::Monospace.resolve(ui.style());
    let space = ui.fonts_mut(|f| f.glyph_width(&font, ' ')).max(1.0);
    let caret = output.galley.pos_from_cursor(CCursor::new(cursor));
    let active_row = rows
        .iter()
        .position(|row| caret.center().y >= row.pos.y && caret.center().y < row.pos.y + row.size.y)
        .unwrap_or(0);
    let first = rows.partition_point(|row| origin.y + row.pos.y + row.size.y < clip.top());
    let last = rows
        .partition_point(|row| origin.y + row.pos.y < clip.bottom())
        .min(rows.len());
    let lines = row_lines(rows);
    let active_line = lines.get(active_row).copied().unwrap_or(0);
    let mut shapes = Vec::new();
    // Active line band across gutter and text, covering every wrapped row of the line,
    // with rounded ends and an accent marker in the gutter.
    for (i, row) in rows.iter().enumerate().take(last).skip(first) {
        if lines[i] == active_line {
            let y = origin.y + row.pos.y;
            shapes.push(egui::Shape::rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(gutter.left() + 2.0, y),
                    egui::pos2(clip.right() - 4.0, y + row.size.y),
                ),
                4,
                palette.active_line,
            ));
            let starts_line = i == 0 || lines[i - 1] != active_line;
            if starts_line {
                shapes.push(egui::Shape::rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(gutter.left() + 3.0, y + 3.0),
                        egui::vec2(2.5, (row.size.y - 6.0).max(4.0)),
                    ),
                    2,
                    colors().accent.gamma_multiply(0.85),
                ));
            }
        }
    }
    let view = Viewport {
        rows,
        lines: &lines,
        origin,
        space,
        active_row,
        first,
        last,
    };
    indent_guides(&mut shapes, &view);
    // Bracket matching around the caret.
    if let Some((a, b)) = matching_bracket(text, cursor) {
        for index in [a, b] {
            let r = output
                .galley
                .pos_from_cursor(CCursor::new(index))
                .translate(origin.to_vec2());
            let rect = egui::Rect::from_min_size(r.min, egui::vec2(space, r.height()));
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
    ui.painter().vline(
        gutter.right() + 2.0,
        clip.y_range(),
        egui::Stroke::new(1.0_f32, colors().border.gamma_multiply(0.5)),
    );
    for (i, row) in rows.iter().enumerate().take(last).skip(first) {
        let y = origin.y + row.pos.y;
        let line = lines[i];
        let starts_line = i == 0 || lines[i - 1] != line;
        if starts_line {
            ui.painter().text(
                egui::pos2(gutter.right() - 9.0, y),
                egui::Align2::RIGHT_TOP,
                (line + 1).to_string(),
                font.clone(),
                if line == active_line {
                    colors().text
                } else {
                    colors().disabled
                },
            );
        }
        if let Some(mark) = marks.and_then(|m| m.get(line).copied().flatten()) {
            git_mark(
                ui.painter(),
                egui::pos2(gutter.right() - 4.0, y),
                row.size.y,
                mark,
                starts_line,
            );
        }
    }
    shapes
}

/// The visible slice of the laid-out document, shared by the decoration passes.
struct Viewport<'a> {
    rows: &'a [egui::epaint::text::PlacedRow],
    lines: &'a [usize],
    origin: egui::Pos2,
    space: f32,
    active_row: usize,
    first: usize,
    last: usize,
}

/// Thin Git change bar in the gutter; deletions show a small wedge at the line's top edge.
fn git_mark(
    painter: &egui::Painter,
    at: egui::Pos2,
    height: f32,
    mark: super::git_gutter::Mark,
    starts_line: bool,
) {
    use super::git_gutter::Mark;
    match mark {
        Mark::Added | Mark::Modified => {
            let color = if mark == Mark::Added {
                colors().success
            } else {
                colors().accent
            };
            painter.rect_filled(
                egui::Rect::from_min_size(at, egui::vec2(3.0, height)),
                1,
                color,
            );
        }
        Mark::Deleted if starts_line => {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    at + egui::vec2(0.0, -3.0),
                    at + egui::vec2(5.0, 0.0),
                    at + egui::vec2(0.0, 3.0),
                ],
                colors().error,
                egui::Stroke::NONE,
            ));
        }
        Mark::Deleted => {}
    }
}

/// Indent guides at each indentation level inside leading whitespace, the active block's
/// guide brighter. Blank lines inherit the indentation of the next non-blank line and
/// wrapped continuation rows keep their line's guides.
fn indent_guides(shapes: &mut Vec<egui::Shape>, view: &Viewport<'_>) {
    let Viewport {
        rows,
        lines,
        origin,
        space,
        active_row,
        first,
        last,
    } = *view;
    let palette = super::syntax::current();
    let scan_end = (last + 200).min(rows.len());
    let scan_start = first.min(active_row);
    let starts = |i: usize| i == 0 || lines[i - 1] != lines[i];
    let mut levels = vec![0usize; scan_end.saturating_sub(scan_start)];
    let mut carry = 0;
    for i in (scan_start..scan_end).rev() {
        if starts(i) {
            carry = leading_indent(&rows[i]).unwrap_or(carry);
            levels[i - scan_start] = carry;
        }
    }
    for i in scan_start + 1..scan_end {
        if !starts(i) {
            levels[i - scan_start] = levels[i - scan_start - 1];
        }
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
}

/// The selection Ctrl+D moves to: the word under the caret when nothing is selected,
/// otherwise the next occurrence of the selected text after it, wrapping to the top.
fn next_occurrence(text: &str, (start, end): (usize, usize)) -> Option<(usize, usize)> {
    let byte = |chars: usize| {
        text.char_indices()
            .nth(chars)
            .map_or(text.len(), |(b, _)| b)
    };
    let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    if start == end {
        let chars: Vec<char> = text.chars().collect();
        let mut a = start.min(chars.len());
        while a > 0 && is_word(chars[a - 1]) {
            a -= 1;
        }
        let mut b = start.min(chars.len());
        while b < chars.len() && is_word(chars[b]) {
            b += 1;
        }
        return (a < b).then_some((a, b));
    }
    let needle = &text[byte(start)..byte(end)];
    let after = byte(end);
    let found = text[after..]
        .find(needle)
        .map(|i| after + i)
        .or_else(|| text.find(needle))?;
    let first = text[..found].chars().count();
    Some((first, first + needle.chars().count()))
}

/// A breadcrumb segment with a soft hover fill.
fn crumb(ui: &mut Ui, text: &str, current: bool) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(
        text.to_owned(),
        egui::FontId::proportional(SMALL),
        colors().text,
    );
    let size = galley.size() + egui::vec2(10.0, 6.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let hover = super::motion::state(
        ui,
        response.id.with("crumb-hover"),
        response.hovered(),
        super::motion::HOVER,
    );
    if hover > 0.01 {
        ui.painter()
            .rect_filled(rect, 5, colors().hover.gamma_multiply(hover));
    }
    let color = if current {
        colors().text
    } else {
        super::motion::blend(colors().muted, colors().text, hover)
    };
    ui.painter()
        .galley(rect.min + egui::vec2(5.0, 3.0), galley, color);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, text));
    pointer(response)
}

impl Editor {
    /// Opens the Explorer on the active file, expanding each folder above it.
    fn reveal_in_explorer(&mut self, ctx: &Context, path: &Path) {
        self.studio_open = false;
        self.sidebar = Sidebar::Files;
        self.sidebar_open = true;
        let Some(root) = self.root.clone() else {
            return;
        };
        for folder in path.ancestors().skip(1) {
            if folder == root || !folder.starts_with(&root) {
                break;
            }
            let id = Id::new(("explorer-dir", folder));
            let mut state =
                egui::collapsing_header::CollapsingState::load_with_default_open(ctx, id, false);
            state.set_open(true);
            state.store(ctx);
        }
    }

    /// Interactive code minimap along the right edge of the editor, with click/drag
    /// scrubbing, syntax-tinted line silhouettes, and an Alt-peek / hover code loupe.
    fn minimap_visor<R>(
        &self,
        ui: &mut Ui,
        scroll: &mut egui::scroll_area::ScrollAreaOutput<R>,
        index: usize,
    ) {
        let rect = scroll.inner_rect;
        let Some(doc) = self.documents.get(index) else {
            return;
        };
        let alt_held = ui.input(|i| i.modifiers.alt && !i.modifiers.ctrl && !i.modifiers.shift);
        let want_open = (self.preferences.minimap || alt_held)
            && rect.width() >= 280.0
            && rect.height() >= 80.0;
        let reveal = super::motion::spring(
            ui.ctx(),
            Id::new(("minimap-reveal", &doc.path)),
            if want_open { 1.0 } else { 0.0 },
            super::motion::Spring::SNAPPY,
        )
        .clamp(0.0, 1.0);
        if reveal < 0.04 {
            self.overview_ruler(ui, rect, index);
            return;
        }
        let lines: Vec<&str> = doc.text.lines().collect();
        let total_lines = lines.len().max(1);
        let target_w = if alt_held { 108.0 } else { 88.0 };
        let width = (target_w * reveal).max(8.0);
        let max_h = (rect.height() - 16.0).max(40.0);
        // Natural fixed line pitch (3.2px per line) so short files stay compact at the top-right
        // instead of stretching 8 lines across the whole screen; long files compress to fit.
        let natural_pitch = 3.2_f32;
        let max_inner_h = (max_h - 14.0).max(20.0);
        let used_h = (total_lines as f32 * natural_pitch).min(max_inner_h);
        let card_h = (used_h + 14.0).clamp(44.0, max_h);
        let map_rect = egui::Rect::from_min_max(
            egui::pos2(rect.right() - width - 10.0, rect.top() + 8.0),
            egui::pos2(rect.right() - 10.0, rect.top() + 8.0 + card_h),
        );
        let interact = ui.interact(
            map_rect,
            Id::new(("minimap-interact", &doc.path)),
            egui::Sense::click_and_drag(),
        );
        if alt_held && reveal > 0.3 {
            ui.painter()
                .add(floating_shadow(active(), 18).as_shape(map_rect, 10));
        }
        let painter = ui.painter().with_clip_rect(map_rect);
        // Card backdrop.
        painter.rect(
            map_rect,
            10,
            colors().elevated.gamma_multiply(0.92 * reveal),
            egui::Stroke::new(1.0_f32, colors().border.gamma_multiply(0.75 * reveal)),
            egui::StrokeKind::Inside,
        );
        let inner = egui::Rect::from_min_size(
            map_rect.min + egui::vec2(7.0, 7.0),
            egui::vec2((map_rect.width() - 14.0).max(4.0), used_h.max(1.0)),
        );
        let max_samples = ((inner.height() / 2.2) as usize).clamp(1, 320);
        let step = (total_lines as f32 / max_samples as f32).max(1.0);
        let pitch = (inner.height() / total_lines as f32).min(natural_pitch);
        let bar_h = (pitch * 0.62).clamp(1.2, 2.1);
        let samples = (total_lines as f32 / step).ceil() as usize;

        for s in 0..samples {
            let line_idx = ((s as f32) * step).floor() as usize;
            let Some(line) = lines.get(line_idx) else {
                break;
            };
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let indent = line
                .chars()
                .take_while(|c| *c == ' ' || *c == '\t')
                .map(|c| if c == '\t' { 4 } else { 1 })
                .sum::<usize>();
            let base_x = inner.left() + ((indent as f32) * 1.1).min(inner.width() * 0.42);
            let y = inner.top() + (line_idx as f32) * pitch;
            let color = minimap_line_color(trimmed).gamma_multiply(0.72 * reveal);
            // Draw up to 4 miniature token segments per line so it reads like real mini-code
            let mut cursor_x = base_x;
            for word in trimmed.split_whitespace().take(4) {
                let seg_w = (word.chars().count() as f32 * 1.15).clamp(2.5, 22.0);
                let avail = inner.right() - cursor_x;
                if avail < 2.0 {
                    break;
                }
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(cursor_x, y),
                        egui::vec2(seg_w.min(avail), bar_h),
                    ),
                    0.8,
                    color,
                );
                cursor_x += seg_w + 2.2;
            }
        }

        // Diagnostics & find match ticks on the right edge of the minimap
        let y_for_line = |line: usize| inner.top() + (line as f32 + 0.5) * pitch;
        if self.find_open && !self.find_query.is_empty() {
            let matcher = workspace::literal_query(&self.find_query);
            let mut line = 0;
            let mut scanned = 0;
            for found in matcher.find_iter(&doc.text).take(500) {
                line += doc.text[scanned..found.start()].matches('\n').count();
                scanned = found.start();
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(inner.right() - 5.0, y_for_line(line) - 1.0),
                        egui::vec2(5.0, 2.0),
                    ),
                    1,
                    colors().accent,
                );
            }
        }
        let diagnostics = self
            .lsp
            .diagnostics
            .get(&crate::agents_runtime::plain(&doc.path));
        for diag in diagnostics.into_iter().flatten().take(300) {
            let color = match diag.severity {
                1 => colors().error,
                2 => colors().warning,
                _ => continue,
            };
            painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(inner.right() - 6.0, y_for_line(diag.start.0 as usize) - 1.5),
                    egui::vec2(6.0, 3.0),
                ),
                1,
                color,
            );
        }

        // Active caret indicator line
        let (caret_line, _) = workspace::cursor_position(&doc.text, self.cursor);
        let caret_y = y_for_line(caret_line.saturating_sub(1));
        painter.hline(
            inner.x_range(),
            caret_y,
            egui::Stroke::new(1.5_f32, colors().accent.gamma_multiply(0.9 * reveal)),
        );

        // Visible viewport slider band
        let content_h = scroll.content_size.y.max(rect.height()).max(1.0);
        let view_h = rect.height().min(content_h);
        let max_offset = (content_h - view_h).max(0.0);
        let top_frac = (scroll.state.offset.y / content_h).clamp(0.0, 1.0);
        let frac_h = (view_h / content_h).clamp(0.06, 1.0);
        let slider = egui::Rect::from_min_max(
            egui::pos2(
                map_rect.left() + 2.0,
                inner.top() + top_frac * inner.height(),
            ),
            egui::pos2(
                map_rect.right() - 2.0,
                (inner.top() + (top_frac + frac_h) * inner.height()).min(inner.bottom()),
            ),
        );
        let slider_alpha = if interact.hovered() || interact.dragged() {
            0.22
        } else {
            0.13
        };
        painter.rect(
            slider,
            4,
            colors().accent.gamma_multiply(slider_alpha * reveal),
            egui::Stroke::new(1.0_f32, colors().accent.gamma_multiply(0.38 * reveal)),
            egui::StrokeKind::Inside,
        );

        // Click or drag on the minimap to jump/scrub immediately
        if (interact.clicked() || interact.dragged())
            && let Some(pos) = interact.interact_pointer_pos()
            && max_offset > 0.0
        {
            let ratio = ((pos.y - inner.top()) / inner.height().max(1.0)).clamp(0.0, 1.0);
            let target_y = (ratio * content_h - view_h * 0.5).clamp(0.0, max_offset);
            scroll.state.offset.y = target_y;
            scroll.state.store(ui.ctx(), scroll.id);
            ui.ctx().request_repaint();
        }

        // Hover / Alt-peek floating code preview loupe
        let hover_pos = interact.hover_pos().or_else(|| {
            interact
                .dragged()
                .then(|| interact.interact_pointer_pos())
                .flatten()
        });
        if let Some(pos) = hover_pos
            && reveal > 0.7
            && !lines.is_empty()
        {
            let ratio = ((pos.y - inner.top()) / inner.height().max(1.0)).clamp(0.0, 1.0);
            let target_line = ((ratio * (total_lines.saturating_sub(1)) as f32).round() as usize)
                .min(total_lines.saturating_sub(1));
            let start = target_line.saturating_sub(4);
            let end = (start + 9).min(total_lines);
            let snippet = lines[start..end].join("\n");
            let lang = doc.path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let loupe_w = 340.0_f32.min((rect.width() - width - 36.0).max(180.0));
            let loupe_x = (map_rect.left() - loupe_w - 10.0).max(rect.left() + 12.0);
            let loupe_y = (pos.y - 84.0).clamp(
                rect.top() + 8.0,
                (rect.bottom() - 180.0).max(rect.top() + 8.0),
            );
            egui::Area::new(Id::new(("minimap-loupe", &doc.path)))
                .order(egui::Order::Tooltip)
                .fixed_pos(egui::pos2(loupe_x, loupe_y))
                .interactable(false)
                .show(ui.ctx(), |ui| {
                    egui::Frame::new()
                        .fill(colors().elevated)
                        .stroke(egui::Stroke::new(1.0_f32, colors().control_border))
                        .corner_radius(10)
                        .shadow(floating_shadow(active(), 22))
                        .inner_margin(Margin::symmetric(12, 9))
                        .show(ui, |ui| {
                            ui.set_width(loupe_w);
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!("{} {}–{}", tr("Lines"), start + 1, end))
                                        .size(11.0)
                                        .strong()
                                        .color(colors().accent),
                                );
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(
                                        RichText::new(tr("Click to jump"))
                                            .size(10.5)
                                            .color(colors().muted),
                                    );
                                });
                            });
                            ui.add_space(5.0);
                            let job = if !lang.is_empty() {
                                super::syntax::highlight(ui.ctx(), &snippet, lang, 11.5, 15.0)
                            } else {
                                LayoutJob::simple(
                                    snippet,
                                    egui::FontId::monospace(11.5),
                                    colors().text,
                                    loupe_w,
                                )
                            };
                            ui.add(egui::Label::new(job).truncate());
                        });
                });
        }
        interact.on_hover_cursor(egui::CursorIcon::PointingHand);
    }

    /// Colour marks along the right edge of the editor: diagnostics, find matches and the
    /// caret line, placed by their position in the file.
    fn overview_ruler(&self, ui: &Ui, rect: egui::Rect, index: usize) {
        let Some(doc) = self.documents.get(index) else {
            return;
        };
        let total = doc.text.matches('\n').count() + 1;
        if total < 2 || rect.height() < 40.0 {
            return;
        }
        let lane = egui::Rect::from_min_max(
            egui::pos2(rect.right() - 9.0, rect.top()),
            egui::pos2(rect.right() - 5.0, rect.bottom()),
        );
        let painter = ui.painter().with_clip_rect(lane.expand(1.0));
        let y = |line: usize| lane.top() + lane.height() * (line as f32 + 0.5) / total as f32;
        let tick = |line: usize, color: egui::Color32, height: f32| {
            painter.rect_filled(
                egui::Rect::from_center_size(
                    egui::pos2(lane.center().x, y(line)),
                    egui::vec2(lane.width(), height),
                ),
                1,
                color,
            );
        };
        if self.find_open && !self.find_query.is_empty() {
            let matcher = workspace::literal_query(&self.find_query);
            let mut line = 0;
            let mut scanned = 0;
            for found in matcher.find_iter(&doc.text).take(2000) {
                line += doc.text[scanned..found.start()].matches('\n').count();
                scanned = found.start();
                tick(line, colors().accent.gamma_multiply(0.8), 2.0);
            }
        }
        let diagnostics = self
            .lsp
            .diagnostics
            .get(&crate::agents_runtime::plain(&doc.path));
        for diag in diagnostics.into_iter().flatten().take(500) {
            let color = match diag.severity {
                1 => colors().error,
                2 => colors().warning,
                _ => continue,
            };
            tick(diag.start.0 as usize, color, 3.0);
        }
        let (line, _) = workspace::cursor_position(&doc.text, self.cursor);
        tick(
            line.saturating_sub(1),
            colors().text.gamma_multiply(0.8),
            2.0,
        );
    }
}

const TREE_INDENT: f32 = 13.0;
const TREE_ROW: f32 = 24.0;

/// Fast semantic tint for a minimap silhouette bar so structure is readable at a glance.
fn minimap_line_color(trimmed: &str) -> egui::Color32 {
    if trimmed.starts_with("//")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
        || trimmed.starts_with('#')
            && !trimmed.starts_with("#[")
            && !trimmed.starts_with("#include")
    {
        colors().muted
    } else if trimmed.starts_with("fn ")
        || trimmed.starts_with("pub ")
        || trimmed.starts_with("struct ")
        || trimmed.starts_with("enum ")
        || trimmed.starts_with("impl ")
        || trimmed.starts_with("function ")
        || trimmed.starts_with("export ")
        || trimmed.starts_with("class ")
        || trimmed.starts_with("def ")
        || trimmed.starts_with("async ")
    {
        colors().accent
    } else if trimmed.starts_with("use ")
        || trimmed.starts_with("import ")
        || trimmed.starts_with("from ")
        || trimmed.starts_with("#include")
    {
        colors().syntax_string
    } else if trimmed.contains('"') || trimmed.contains('\'') {
        colors().syntax_number
    } else {
        colors().secondary
    }
}

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
        colors().elevated,
        selection,
    );
    painter.rect_filled(rect, 7, fill);
    if selection > 0.01 {
        painter.rect_stroke(
            rect,
            7,
            egui::Stroke::new(1.0_f32, colors().border.gamma_multiply(selection)),
            egui::StrokeKind::Inside,
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
                    egui::vec2(13.0 * z, 13.0 * z),
                ),
            );
        // Uniform muted-gray folder icon so colorful file-type icons stay distinct.
        draw_icon_sized(
            &painter,
            egui::pos2(x + 23.0 * z, center_y),
            if open > 0.5 {
                Icon::FolderOpen
            } else {
                Icon::Folder
            },
            super::motion::blend(colors().muted, colors().secondary, open.max(hover)),
            15.0 * z,
        );
    } else {
        super::assets::paint(
            ui,
            egui::Rect::from_center_size(
                egui::pos2(x + 23.0 * z, center_y),
                egui::vec2(16.0 * z, 16.0 * z),
            ),
            icon,
        );
    }
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
                    let id = Id::new(("explorer-dir", &entry.path));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctrl_d_selects_the_word_then_its_next_occurrence() {
        let text = "let total = 1;\ntotal += 2;\nprint(total)";
        assert_eq!(next_occurrence(text, (6, 6)), Some((4, 9)));
        assert_eq!(next_occurrence(text, (4, 9)), Some((15, 20)));
        assert_eq!(next_occurrence(text, (33, 38)), Some((4, 9)));
        assert_eq!(next_occurrence("a + b", (2, 2)), None);
    }
}

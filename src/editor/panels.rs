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

    pub(super) fn show_workspace_search(&mut self) {
        self.sidebar = Sidebar::Search;
        self.search_focus = true;
    }

    pub(super) fn top_bar(&mut self, ctx: &Context) {
        TopBottomPanel::top("menu")
            .exact_height(CHROME_HEIGHT)
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
                let left_width = if rect.width() > 1100.0 { 280.0 } else { 216.0 };
                let right_width = if rect.width() > 1000.0 { 312.0 } else { 248.0 };
                let left =
                    egui::Rect::from_min_size(rect.min, egui::vec2(left_width, rect.height()));
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(left)
                        .layout(Layout::left_to_right(Align::Center)),
                    |ui| {
                        ui.set_clip_rect(left);
                        let (_, mark) = ui.allocate_space(egui::vec2(22.0, 28.0));
                        draw_icon(ui.painter(), mark.center(), Icon::Brand, colors().accent);
                        ui.label(RichText::new(BRAND).size(LEAD).strong());
                        ui.add_space(8.0);
                        ui.menu_button("File", |ui| {
                            if ui.button("Open folder…").clicked() {
                                ui.close();
                                self.pick_workspace(ctx);
                            }
                            if ui.button("Open file…").clicked() {
                                ui.close();
                                self.pick_file();
                            }
                            if ui
                                .add_enabled(self.root.is_some(), egui::Button::new("New file…"))
                                .clicked()
                            {
                                ui.close();
                                self.show_new_file();
                            }
                            ui.separator();
                            if ui
                                .add_enabled(self.active.is_some(), egui::Button::new("Save"))
                                .clicked()
                            {
                                ui.close();
                                self.save_active();
                            }
                            if ui
                                .add_enabled(
                                    !self.documents.is_empty(),
                                    egui::Button::new("Save all"),
                                )
                                .clicked()
                            {
                                ui.close();
                                if self.save_all() {
                                    self.notice = Some((false, "Saved all files".into()));
                                }
                            }
                            if ui
                                .add_enabled(self.active.is_some(), egui::Button::new("Close tab"))
                                .clicked()
                                && let Some(index) = self.active
                            {
                                ui.close();
                                self.request(Pending::CloseTab(index), ctx);
                            }
                        });
                        if rect.width() > 1100.0 {
                            self.workspace_picker(ui);
                        }
                    },
                );
                let middle = egui::Rect::from_min_max(
                    egui::pos2(rect.left() + left_width + 12.0, rect.top()),
                    egui::pos2(rect.right() - right_width - 20.0, rect.bottom()),
                );
                let command_width = middle.width().min(520.0);
                let command =
                    egui::Rect::from_center_size(middle.center(), egui::vec2(command_width, 30.0));
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
                            "Search files and commands"
                        } else {
                            "Quick access"
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
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Quick access")
                    });
                    if response.clicked() {
                        self.open_palette(ctx);
                    }
                });
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
                            self.settings_open = true;
                        }
                        if rect.width() > 1000.0 {
                            if ui.selectable_label(self.hub_open, "Agent Hub").clicked() {
                                self.hub_open = !self.hub_open;
                            }
                            if ui
                                .selectable_label(
                                    !self.hub_open && self.preferences.assistant_open,
                                    "Codex",
                                )
                                .clicked()
                            {
                                self.hub_open = false;
                                self.preferences.assistant_open = !self.preferences.assistant_open;
                            }
                        } else {
                            if icon_button(ui, Icon::Hub, "Agent Hub", self.hub_open).clicked() {
                                self.hub_open = !self.hub_open;
                            }
                            if icon_button(
                                ui,
                                Icon::Code,
                                "Toggle Codex panel",
                                self.preferences.assistant_open,
                            )
                            .clicked()
                            {
                                self.hub_open = false;
                                self.preferences.assistant_open = !self.preferences.assistant_open;
                            }
                        }
                    },
                );
                ui.painter().hline(
                    rect.x_range(),
                    rect.bottom() + 6.0,
                    egui::Stroke::new(1.0_f32, colors().border),
                );
            });
    }

    pub(super) fn status_bar(&mut self, ctx: &Context) {
        TopBottomPanel::bottom("status")
            .exact_height(if self.preferences.identity.mascot {
                STATUS_HEIGHT.max(self.preferences.identity.mascot_size + 4.0)
            } else {
                STATUS_HEIGHT
            })
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
                        let mood = self.agent_mood(ui.input(|i| i.focused));
                        if self.identity.mascot(
                            ui,
                            &self.preferences.identity,
                            mood,
                            self.preferences.reduce_motion,
                        ) {
                            self.hub_open = true;
                        }
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
                            draw_icon(
                                ui.painter(),
                                left.left_center() + egui::vec2(7.0, 0.0),
                                Icon::Explorer,
                                colors().muted,
                            );
                            ui.add_space(18.0);
                            ui.add(egui::Label::new(muted(name)).truncate())
                                .on_hover_text(root.display().to_string());
                        } else {
                            ui.label(muted("Open a workspace"));
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
                            .selectable_label(self.preferences.tools_open, muted("Terminal"))
                            .clicked()
                        {
                            self.preferences.tools_open = !self.preferences.tools_open;
                        }
                        if let Some(doc) = self.active.map(|index| &self.documents[index]) {
                            ui.label(muted("UTF-8"));
                            let before: String = doc.text.chars().take(self.cursor).collect();
                            let line = before.bytes().filter(|&b| b == b'\n').count() + 1;
                            let col = before
                                .rsplit('\n')
                                .next()
                                .map_or(1, |s| s.chars().count() + 1);
                            ui.label(muted(format!("Ln {line}, Col {col}")));
                        }
                    },
                );
                ui.painter().hline(
                    rect.x_range(),
                    rect.top() - 2.0,
                    egui::Stroke::new(1.0_f32, colors().border),
                );
            });
    }

    pub(super) fn activity_rail(&mut self, ctx: &Context) {
        SidePanel::left("activity")
            .exact_width(RAIL_WIDTH)
            .resizable(false)
            .frame(panel_frame(Margin::symmetric(8, 12)))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    for (tab, icon, label) in [
                        (Sidebar::Files, Icon::Explorer, "Explorer"),
                        (Sidebar::Search, Icon::Search, "Search"),
                        (Sidebar::Git, Icon::Git, "Source control"),
                    ] {
                        if icon_button(ui, icon, label, self.sidebar == tab).clicked() {
                            self.sidebar = tab;
                            if tab == Sidebar::Search {
                                self.search_focus = true;
                            }
                            if tab == Sidebar::Git {
                                self.git_refresh(ctx, None);
                            }
                        }
                    }
                    ui.separator();
                    if icon_button(ui, Icon::Hub, "Agent Hub", false).clicked() {
                        self.hub_open = true;
                    }
                    if icon_button(ui, Icon::Settings, "Settings", self.settings_open).clicked() {
                        self.settings_open = true;
                    }
                });
            });
    }

    pub(super) fn sidebar(&mut self, ctx: &Context) {
        let panel = SidePanel::left("workspace")
            .default_width(self.preferences.sidebar_width)
            .width_range(155.0..=340.0)
            .frame(panel_frame(Margin::same(PANEL_PADDING)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = match self.sidebar {
                        Sidebar::Files => "Explorer",
                        Sidebar::Search => "Search",
                        Sidebar::Git => "Git",
                    };
                    ui.allocate_ui_with_layout(
                        egui::vec2((ui.available_width() - 40.0).max(40.0), 28.0),
                        Layout::left_to_right(Align::Center),
                        |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.add(
                                egui::Label::new(
                                    RichText::new(label).strong().color(colors().secondary),
                                )
                                .truncate(),
                            );
                        },
                    );
                    if self.sidebar == Sidebar::Files && self.root.is_some() {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if icon_button(ui, Icon::Add, "New file", false).clicked() {
                                self.show_new_file();
                            }
                        });
                    }
                });
                ui.add_space(12.0);
                let Some(root) = self.root.clone() else {
                    ui.label(muted("Open a folder to browse files."));
                    return;
                };
                match self.sidebar {
                    Sidebar::Files => {
                        let mut selected = None;
                        ui.horizontal(|ui| {
                            let root_label = ui
                                .allocate_ui_with_layout(
                                    egui::vec2((ui.available_width() - 40.0).max(40.0), 28.0),
                                    Layout::left_to_right(Align::Center),
                                    |ui| {
                                        ui.set_min_width(ui.available_width());
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(workspace::filename(&root)).strong(),
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
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if icon_button(ui, Icon::Refresh, "Refresh files", false).clicked()
                                {
                                    self.directories.clear();
                                }
                            });
                        });
                        ui.add_space(4.0);
                        let active = self
                            .active
                            .map(|index| self.documents[index].path.as_path());
                        let from_tree = ScrollArea::vertical()
                            .id_salt("files")
                            .show(ui, |ui| {
                                tree(ui, &root, &root, active, &mut self.directories)
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
        self.preferences.sidebar_width = panel.response.rect.width();
        ctx.layer_painter(egui::LayerId::background()).vline(
            panel.response.rect.right(),
            panel.response.rect.y_range(),
            egui::Stroke::new(1.0_f32, colors().border),
        );
    }

    fn search_panel(&mut self, ui: &mut Ui, root: &Path) {
        let response = ui.add(
            TextEdit::singleline(&mut self.workspace_query)
                .hint_text("Find text in files")
                .desired_width(f32::INFINITY)
                .char_limit(200),
        );
        if std::mem::take(&mut self.search_focus) {
            response.request_focus();
        }
        if (response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter)))
            || ui.button("Search workspace").clicked()
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

    pub(super) fn editor_panel(&mut self, ui: &mut Ui) {
        if self.documents.is_empty() {
            self.welcome(ui);
            return;
        }
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
        if self.find_open {
            self.find_bar(ui);
            ui.separator();
        }
        let Some(index) = self.active else { return };
        let doc = &mut self.documents[index];
        let breadcrumb = self
            .root
            .as_ref()
            .and_then(|root| doc.path.strip_prefix(root).ok())
            .unwrap_or(&doc.path);
        let path_label = breadcrumb
            .components()
            .map(|part| part.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("  ›  ");
        egui::Frame::new()
            .inner_margin(Margin::symmetric(16, 0))
            .show(ui, |ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), BREADCRUMB_HEIGHT),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.add(egui::Label::new(muted(&path_label)).truncate())
                            .on_hover_text(doc.path.display().to_string());
                    },
                );
            });
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
        let syntax = &self.syntax;
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
                        let mut job = if language.is_empty() || text.len() > 256 * 1024 {
                            LayoutJob::simple(
                                text.to_owned(),
                                TextStyle::Monospace.resolve(ui.style()),
                                colors().text,
                                f32::INFINITY,
                            )
                        } else {
                            egui_extras::syntax_highlighting::highlight(
                                ui.ctx(),
                                ui.style(),
                                syntax,
                                text,
                                &language,
                            )
                        };
                        let font_size = TextStyle::Monospace.resolve(ui.style()).size;
                        for section in &mut job.sections {
                            section.format.font_id.size = font_size;
                            section.format.line_height = Some(
                                font_size
                                    * if ui.spacing().interact_size.y <= 24.0 {
                                        COMPACT_CODE_LINE_HEIGHT
                                    } else {
                                        CODE_LINE_HEIGHT
                                    },
                            );
                            section.format.color =
                                syntax_color(section.format.color, section.format.italics);
                        }
                        job.wrap.max_width = f32::INFINITY;
                        ui.fonts_mut(|fonts| fonts.layout_job(job))
                    };
                    let output = TextEdit::multiline(&mut doc.text)
                        .id(id)
                        .code_editor()
                        .frame(false)
                        .margin(Margin::symmetric(8, 8))
                        .desired_rows(18)
                        .desired_width(ui.available_width())
                        .layouter(&mut layouter)
                        .show(ui);
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
                    let first = rows.partition_point(|row| {
                        output.galley_pos.y + row.pos.y + row.size.y < clip.top()
                    });
                    for (index, row) in rows.iter().enumerate().skip(first) {
                        let top = output.galley_pos.y + row.pos.y;
                        if top > clip.bottom() {
                            break;
                        }
                        ui.painter().text(
                            egui::pos2(gutter.right() - 5.0, top),
                            egui::Align2::RIGHT_TOP,
                            (index + 1).to_string(),
                            TextStyle::Monospace.resolve(ui.style()),
                            colors().disabled,
                        );
                    }
                });
            });
    }

    fn tabs(&mut self, ui: &mut Ui) {
        let mut active = None;
        let mut close = None;
        egui::Frame::new().fill(colors().surface).show(ui, |ui| {
            ScrollArea::horizontal()
                .id_salt("tabs")
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.horizontal(|ui| {
                        for (index, document) in self.documents.iter().enumerate() {
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
                            let width = (label_width + 72.0).clamp(128.0, 232.0);
                            let response = ui
                                .add_sized([width, TAB_HEIGHT], egui::Button::new("").frame(false));
                            let painter = ui.painter_at(response.rect);
                            if selected {
                                painter.rect_filled(response.rect, 0, colors().canvas);
                            } else if response.hovered() {
                                painter.rect_filled(response.rect, 0, colors().hover);
                            }
                            draw_icon(
                                &painter,
                                response.rect.left_center() + egui::vec2(18.0, 0.0),
                                Icon::Code,
                                file_color(&document.path),
                            );
                            let text_rect = egui::Rect::from_min_max(
                                response.rect.left_top() + egui::vec2(34.0, 0.0),
                                response.rect.right_bottom() - egui::vec2(30.0, 0.0),
                            );
                            painter.with_clip_rect(text_rect).text(
                                text_rect.left_center(),
                                egui::Align2::LEFT_CENTER,
                                &name,
                                egui::FontId::proportional(BODY),
                                if selected {
                                    colors().text
                                } else {
                                    colors().muted
                                },
                            );
                            if selected {
                                painter.hline(
                                    response.rect.x_range().shrink(14.0),
                                    response.rect.bottom() - 1.0,
                                    egui::Stroke::new(2.0_f32, colors().accent),
                                );
                            }
                            let close_rect = egui::Rect::from_center_size(
                                response.rect.right_center() - egui::vec2(16.0, 0.0),
                                egui::vec2(24.0, 24.0),
                            );
                            let close_response = ui.interact(
                                close_rect,
                                Id::new(("tab-close", &document.path)),
                                egui::Sense::click(),
                            );
                            if close_response.hovered() {
                                painter.rect_filled(close_rect, 4, colors().hover);
                                draw_icon(
                                    &painter,
                                    close_rect.center(),
                                    Icon::Close,
                                    colors().text,
                                );
                            } else if document.dirty {
                                painter.circle_filled(close_rect.center(), 3.0, colors().accent);
                            } else if selected || response.hovered() {
                                draw_icon(
                                    &painter,
                                    close_rect.center(),
                                    Icon::Close,
                                    colors().muted,
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
                            response.on_hover_text(document.path.display().to_string());
                            close_response.on_hover_text("Close tab");
                        }
                    });
                });
        });
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
                    .hint_text("Find in file")
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
                    .on_hover_text("Next match (F3)")
                    .clicked()
            {
                self.find(false);
                response.request_focus();
            }
            if ui
                .small_button("↑")
                .on_hover_text("Previous match (Shift+F3)")
                .clicked()
            {
                self.find(true);
                response.request_focus();
            }
            if ui.small_button("×").on_hover_text("Close find").clicked() {
                self.find_open = false;
                self.focus_editor = true;
            }
            if self.find_empty {
                ui.label(muted("No matches"));
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
        let panel = SidePanel::right("assistant")
            .default_width(self.preferences.assistant_width.min(max_width))
            .width_range(240.0..=max_width)
            .frame(panel_frame(Margin::same(12)))
            .show(ctx, |ui| self.workspace_chat(ui));
        let width = panel.response.rect.width();
        ctx.layer_painter(egui::LayerId::background()).vline(
            panel.response.rect.left(),
            panel.response.rect.y_range(),
            egui::Stroke::new(1.0_f32, colors().border),
        );
        if width + 1.0 < max_width {
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
                        if ui.add(primary("Save")).clicked() {
                            return Some(true);
                        }
                        if ui.button("Discard").clicked() {
                            return Some(false);
                        }
                        if ui.button("Cancel").clicked() {
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
                    ui.heading("New file");
                    if let Some((true, message)) = &self.notice {
                        ui.label(RichText::new(message).color(colors().error));
                    }
                    let input = ui.add(
                        TextEdit::singleline(&mut name)
                            .hint_text("Path relative to the workspace")
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
                        if ui.button("Cancel").clicked() {
                            return Some(false);
                        }
                        if ui
                            .add_enabled(!name.trim().is_empty(), primary("Create"))
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

fn tree(
    ui: &mut Ui,
    root: &Path,
    path: &Path,
    active: Option<&Path>,
    directories: &mut std::collections::HashMap<std::path::PathBuf, super::DirectoryLoad>,
) -> Option<super::files::TreeAction> {
    use super::{
        DirectoryLoad,
        files::{TreeAction, tree_menu},
    };
    use std::sync::mpsc::{self, TryRecvError};

    let entries = match directories.remove(path) {
        Some(DirectoryLoad::Ready(entries)) => entries,
        Some(DirectoryLoad::Pending(receiver)) => match receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => {
                directories.insert(path.to_owned(), DirectoryLoad::Pending(receiver));
                ui.label(muted("Loading…"));
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
            ui.label(muted("Loading…"));
            return None;
        }
    };
    let mut selected = None;
    match &entries {
        Ok(entries) => {
            for entry in entries {
                let name = workspace::filename(&entry.path);
                if entry.directory {
                    let response =
                        egui::CollapsingHeader::new(RichText::new(name).color(colors().text))
                            .id_salt(&entry.path)
                            .show(ui, |ui| tree(ui, root, &entry.path, active, directories));
                    response.header_response.context_menu(|ui| {
                        if let Some(action) = tree_menu(ui, root, &entry.path, true) {
                            selected = Some(action);
                        }
                    });
                    if let Some(Some(action)) = response.body_returned {
                        selected = Some(action);
                    }
                } else {
                    let response = row(
                        ui,
                        &format!("      {name}"),
                        "",
                        active == Some(entry.path.as_path()),
                    )
                    .on_hover_text(entry.path.display().to_string());
                    draw_icon(
                        ui.painter(),
                        response.rect.left_center() + egui::vec2(17.0, 0.0),
                        Icon::File,
                        file_color(&entry.path),
                    );
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

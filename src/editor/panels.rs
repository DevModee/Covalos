use std::path::Path;

use eframe::egui::{
    self, Align, Color32, Context, FontId, Frame, Id, Key, KeyboardShortcut, Layout, Margin,
    Modifiers, RichText, ScrollArea, SidePanel, TextEdit, TextStyle, TopBottomPanel, Ui,
    text::{CCursor, CCursorRange, LayoutJob},
};

use super::{Editor, Pending, Sidebar};
use crate::workspace::{self, MAX_SEARCH_HITS};

const SURFACE: Color32 = Color32::from_rgb(25, 26, 32);
const CANVAS: Color32 = Color32::from_rgb(19, 20, 25);
const BORDER: Color32 = Color32::from_rgb(48, 49, 59);
const TEXT: Color32 = Color32::from_rgb(222, 223, 231);
const MUTED: Color32 = Color32::from_rgb(154, 157, 172);
const ACCENT: Color32 = Color32::from_rgb(181, 168, 246);
const ERROR: Color32 = Color32::from_rgb(247, 156, 160);

pub(super) fn install_theme(ctx: &Context) {
    let mut style = egui::Style::default();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(9.0, 4.0);
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(14.0));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(12.0));
    style
        .text_styles
        .insert(TextStyle::Monospace, FontId::monospace(14.0));
    style.visuals = egui::Visuals::dark();
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.panel_fill = CANVAS;
    style.visuals.window_fill = SURFACE;
    style.visuals.extreme_bg_color = CANVAS;
    style.visuals.faint_bg_color = SURFACE;
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(38, 39, 49);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(56, 54, 72);
    style.visuals.selection.bg_fill = Color32::from_rgb(67, 59, 102);
    style.visuals.hyperlink_color = ACCENT;
    ctx.set_style(style);
}

fn panel_frame(margin: Margin) -> Frame {
    Frame::new()
        .fill(SURFACE)
        .inner_margin(margin)
        .stroke(egui::Stroke::new(1.0_f32, BORDER))
}

fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text).small().color(MUTED)
}

impl Editor {
    pub(super) fn shortcuts(&mut self, ctx: &Context) {
        let shortcut = |modifiers, key| {
            ctx.input_mut(|input| input.consume_shortcut(&KeyboardShortcut::new(modifiers, key)))
        };
        if shortcut(Modifiers::COMMAND, Key::O) {
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

    fn show_workspace_search(&mut self) {
        self.sidebar = Sidebar::Search;
        self.search_focus = true;
    }

    pub(super) fn top_bar(&mut self, ctx: &Context) {
        TopBottomPanel::top("menu")
            .frame(panel_frame(Margin::symmetric(14, 6)))
            .show(ctx, |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
                    ui.label(RichText::new("AEROS").strong().color(ACCENT).size(15.0));
                    ui.separator();
                    ui.menu_button("File", |ui| {
                        if ui.button("Open Folder…").clicked() {
                            ui.close();
                            self.pick_workspace(ctx);
                        }
                        if ui
                            .add_enabled(self.root.is_some(), egui::Button::new("New File…"))
                            .clicked()
                        {
                            ui.close();
                            self.show_new_file();
                        }
                        if ui
                            .add_enabled(self.active.is_some(), egui::Button::new("Save"))
                            .clicked()
                        {
                            ui.close();
                            self.save_active();
                        }
                        if ui
                            .add_enabled(!self.documents.is_empty(), egui::Button::new("Save All"))
                            .clicked()
                        {
                            ui.close();
                            if self.save_all() {
                                self.notice = Some((false, "Saved all files".into()));
                            }
                        }
                        if ui
                            .add_enabled(self.active.is_some(), egui::Button::new("Close Tab"))
                            .clicked()
                        {
                            ui.close();
                            if let Some(index) = self.active {
                                self.request(Pending::CloseTab(index), ctx);
                            }
                        }
                    });
                    ui.menu_button("Edit", |ui| {
                        if ui
                            .add_enabled(self.active.is_some(), egui::Button::new("Find in File"))
                            .clicked()
                        {
                            ui.close();
                            self.show_find();
                        }
                        if ui
                            .add_enabled(
                                self.root.is_some(),
                                egui::Button::new("Find in Workspace"),
                            )
                            .clicked()
                        {
                            ui.close();
                            self.show_workspace_search();
                        }
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.selectable_label(self.ai_open, "Codex").clicked() {
                            self.ai_open = !self.ai_open;
                        }
                        if ui.button("Open Folder").clicked() {
                            self.pick_workspace(ctx);
                        }
                    });
                });
            });
    }

    pub(super) fn status_bar(&self, ctx: &Context) {
        TopBottomPanel::bottom("status")
            .frame(panel_frame(Margin::symmetric(12, 3)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if let Some((error, notice)) = &self.notice {
                        ui.label(RichText::new(notice).small().color(if *error {
                            ERROR
                        } else {
                            TEXT
                        }));
                    } else if let Some(root) = &self.root {
                        ui.label(muted(root.display().to_string()))
                            .on_hover_text(root.display().to_string());
                    } else {
                        ui.label(muted("No workspace open"));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some(doc) = self.active.map(|index| &self.documents[index]) {
                            let mut line = 1;
                            let mut column = 1;
                            for ch in doc.text.chars().take(self.cursor) {
                                if ch == '\n' {
                                    line += 1;
                                    column = 1;
                                } else {
                                    column += 1;
                                }
                            }
                            ui.label(muted("UTF-8"));
                            ui.label(muted(format!("Ln {line}, Col {column}")));
                        }
                    });
                });
            });
    }

    pub(super) fn sidebar(&mut self, ctx: &Context) {
        SidePanel::left("workspace")
            .default_width(235.0)
            .width_range(170.0..=360.0)
            .frame(panel_frame(Margin::same(10)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(self.sidebar == Sidebar::Files, "Explorer")
                        .clicked()
                    {
                        self.sidebar = Sidebar::Files;
                    }
                    if ui
                        .selectable_label(self.sidebar == Sidebar::Search, "Search")
                        .clicked()
                    {
                        self.show_workspace_search();
                    }
                    if self.sidebar == Sidebar::Files && self.root.is_some() {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.small_button("+").on_hover_text("New file").clicked() {
                                self.show_new_file();
                            }
                        });
                    }
                });
                ui.add_space(5.0);
                ui.separator();
                ui.add_space(6.0);
                let Some(root) = self.root.clone() else {
                    ui.label(muted("Open a folder to browse files."));
                    return;
                };
                match self.sidebar {
                    Sidebar::Files => {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(workspace::filename(&root)).strong());
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui
                                    .small_button("↻")
                                    .on_hover_text("Refresh files")
                                    .clicked()
                                {
                                    self.directories.clear();
                                }
                            });
                        });
                        ui.add_space(4.0);
                        let active = self
                            .active
                            .map(|index| self.documents[index].path.as_path());
                        let selected = ScrollArea::vertical()
                            .id_salt("files")
                            .show(ui, |ui| tree(ui, &root, active, &mut self.directories))
                            .inner;
                        if let Some(path) = selected {
                            self.open_file(path);
                        }
                    }
                    Sidebar::Search => self.search_panel(ui, &root),
                }
            });
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
            ui.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space((ui.available_height() * 0.34).max(32.0));
                    ui.heading(if self.root.is_some() {
                        "Open a file"
                    } else {
                        "Open a workspace"
                    });
                    ui.add_space(4.0);
                    if self.root.is_some() {
                        ui.label(muted("Choose a file from the Explorer to start editing."));
                    } else {
                        ui.label(muted("Choose a local project folder to start editing."));
                        ui.add_space(12.0);
                        if ui.button("Open Folder").clicked() {
                            self.pick_workspace(ui.ctx());
                        }
                    }
                });
            });
            return;
        }
        self.tabs(ui);
        ui.separator();
        if self.find_open {
            self.find_bar(ui);
            ui.separator();
        }
        let Some(index) = self.active else { return };
        let doc = &mut self.documents[index];
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
                                TEXT,
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
                        ui.painter().rect_filled(
                            rect,
                            2,
                            Color32::from_rgba_unmultiplied(181, 168, 246, 76),
                        );
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
                            MUTED,
                        );
                    }
                });
            });
    }

    fn tabs(&mut self, ui: &mut Ui) {
        let mut active = None;
        let mut close = None;
        ScrollArea::horizontal().id_salt("tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                for (index, document) in self.documents.iter().enumerate() {
                    let selected = self.active == Some(index);
                    ui.horizontal(|ui| {
                        let name = workspace::filename(&document.path);
                        let label = if document.dirty {
                            format!("{name} •")
                        } else {
                            name
                        };
                        if ui
                            .selectable_label(selected, label)
                            .on_hover_text(document.path.display().to_string())
                            .clicked()
                        {
                            active = Some(index);
                        }
                        if ui.small_button("×").on_hover_text("Close tab").clicked() {
                            close = Some(index);
                        }
                    });
                    ui.separator();
                }
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
        SidePanel::right("assistant").default_width(335.0).width_range(260.0..=520.0)
            .frame(panel_frame(Margin::same(12)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Codex").strong().size(17.0));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add_enabled(!self.messages.is_empty(), egui::Button::new("New chat")).clicked() {
                            self.reset_chat();
                        }
                    });
                });
                ui.label(muted("Workspace access · Read only"));
                ui.add_space(6.0);
                ui.separator();
                let chat_height = (ui.available_height() - 115.0).max(80.0);
                ui.scope(|ui| {
                    ui.set_height(chat_height);
                    ScrollArea::vertical().id_salt("chat").stick_to_bottom(true).show(ui, |ui| {
                        if self.messages.is_empty() {
                            ui.add_space(12.0);
                            ui.label(muted(if self.root.is_some() {
                                "Ask about the workspace, request a code review, or get help tracing a bug."
                            } else {
                                "Open a workspace to ask Codex about your project."
                            }));
                        }
                        for message in &self.messages {
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                let color = if message.author == "Error" { ERROR } else { ACCENT };
                                ui.label(RichText::new(message.author).strong().color(color));
                                if message.author == "Codex" && ui.small_button("Copy").clicked() {
                                    ui.ctx().copy_text(message.text.clone());
                                }
                            });
                            ui.add(egui::Label::new(&message.text).wrap().selectable(true));
                            ui.add_space(5.0);
                        }
                        if self.run.is_some() {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(muted(self.activity));
                            });
                        }
                    });
                });
                ui.separator();
                ui.add_enabled(self.root.is_some(), TextEdit::multiline(&mut self.prompt)
                    .hint_text("Ask Codex…").desired_rows(3).char_limit(12_000).desired_width(f32::INFINITY));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let Some(run) = &self.run {
                        if ui.button("Stop").clicked() {
                            run.cancel();
                            self.activity = "Stopping";
                        }
                    } else if ui.add_enabled(self.root.is_some() && !self.prompt.trim().is_empty(),
                        egui::Button::new(RichText::new("Send").strong().color(ACCENT))).clicked()
                    {
                        self.send_prompt(ctx);
                    }
                });
            });
    }

    pub(super) fn modals(&mut self, ctx: &Context) {
        if let Some(action) = self.pending.take() {
            let modal = egui::Modal::new(Id::new("close_confirmation")).show(ctx, |ui| {
                ui.heading("Save your changes?");
                ui.label(if matches!(action, Pending::CloseTab(_)) {
                    "This file has unsaved changes."
                } else {
                    "One or more files have unsaved changes."
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
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
            let modal = egui::Modal::new(Id::new("new_file")).show(ctx, |ui| {
                ui.heading("New file");
                if let Some((true, message)) = &self.notice {
                    ui.label(RichText::new(message).color(ERROR));
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
                let enter = input.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter));
                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        return Some(false);
                    }
                    if ui
                        .add_enabled(!name.trim().is_empty(), egui::Button::new("Create"))
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
    path: &Path,
    active: Option<&Path>,
    directories: &mut std::collections::HashMap<
        std::path::PathBuf,
        Result<Vec<workspace::Entry>, String>,
    >,
) -> Option<std::path::PathBuf> {
    let entries = directories
        .remove(path)
        .unwrap_or_else(|| workspace::read_directory(path).map_err(|error| error.to_string()));
    let mut selected = None;
    match &entries {
        Ok(entries) => {
            for entry in entries {
                let name = workspace::filename(&entry.path);
                if entry.directory {
                    let found = egui::CollapsingHeader::new(RichText::new(name).color(TEXT))
                        .id_salt(&entry.path)
                        .show(ui, |ui| tree(ui, &entry.path, active, directories))
                        .body_returned
                        .flatten();
                    if found.is_some() {
                        selected = found;
                    }
                } else if ui
                    .selectable_label(active == Some(entry.path.as_path()), name)
                    .on_hover_text(entry.path.display().to_string())
                    .clicked()
                {
                    selected = Some(entry.path.clone());
                }
            }
        }
        Err(error) => {
            ui.label(RichText::new(error).small().color(ERROR));
        }
    }
    directories.insert(path.to_owned(), entries);
    selected
}

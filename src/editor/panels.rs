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
            .frame(panel_frame(Margin::symmetric(14, 8)))
            .show(ctx, |ui| {
                let ctx = ui.ctx().clone();
                let available = ui.available_width();
                let height = 31.0;
                let bar = ui.available_rect_before_wrap();
                let sides = (available * 0.32).min(380.0);
                let center = (available - 2.0 * sides - 20.0).clamp(155.0, 490.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(available, height),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        let (_, mark) = ui.allocate_space(egui::vec2(21.0, 26.0));
                        draw_icon(ui.painter(), mark.center(), Icon::Brand, ACCENT);
                        ui.label(RichText::new(BRAND).strong().size(16.0));
                        ui.separator();
                        ui.menu_button("File", |ui| {
                            if ui.button("Open folder…").clicked() {
                                ui.close();
                                self.pick_workspace(&ctx);
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
                                self.request(Pending::CloseTab(index), &ctx);
                            }
                        });
                        if available >= 1020.0 {
                            self.workspace_picker(ui);
                        }
                    },
                );
                let command_rect = egui::Rect::from_center_size(
                    egui::pos2(bar.center().x, bar.top() + height / 2.0),
                    egui::vec2(center, height),
                );
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(command_rect)
                        .layout(Layout::centered_and_justified(egui::Direction::LeftToRight)),
                    |ui| {
                        if ui
                            .add(
                                egui::Button::new(muted("Quick access     Ctrl / ⌘ P")).fill(INSET),
                            )
                            .clicked()
                        {
                            self.open_palette(&ctx);
                        }
                    },
                );
                let actions = egui::Rect::from_min_max(
                    egui::pos2(bar.right() - sides, bar.top()),
                    egui::pos2(bar.right(), bar.top() + height),
                );
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(actions)
                        .layout(Layout::right_to_left(Align::Center)),
                    |ui| {
                        if ui.button("Settings").clicked() {
                            self.settings_open = true;
                        }
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
                            if self.hub_open {
                                self.hub_open = false;
                                self.preferences.assistant_open = true;
                            } else {
                                self.preferences.assistant_open = !self.preferences.assistant_open;
                            }
                        }
                    },
                );
            });
    }

    pub(super) fn status_bar(&mut self, ctx: &Context) {
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
                        let text = root.to_string_lossy();
                        let display = text.strip_prefix(r"\\?\").unwrap_or(&text);
                        ui.add(egui::Label::new(muted(display)).truncate())
                            .on_hover_text(display);
                    } else {
                        ui.label(muted("No workspace open"));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .selectable_label(self.preferences.tools_open, "Terminal")
                            .clicked()
                        {
                            self.preferences.tools_open = !self.preferences.tools_open;
                        }
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

    pub(super) fn activity_rail(&mut self, ctx: &Context) {
        SidePanel::left("activity")
            .exact_width(49.0)
            .resizable(false)
            .frame(panel_frame(Margin::symmetric(5, 10)))
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
            .frame(panel_frame(Margin::same(10)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = match self.sidebar {
                        Sidebar::Files => "Explorer",
                        Sidebar::Search => "Search",
                        Sidebar::Git => "Git",
                    };
                    ui.label(RichText::new(label).strong().color(TEXT));
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
                        let mut selected = None;
                        ui.horizontal(|ui| {
                            let root_label = ui
                                .add(
                                    egui::Label::new(
                                        RichText::new(workspace::filename(&root)).strong(),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(root.display().to_string());
                            root_label.context_menu(|ui| {
                                selected = super::files::tree_menu(ui, &root, &root, true);
                            });
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
        self.tabs(ui);
        ui.separator();
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
        ui.label(muted(
            breadcrumb
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("  /  "),
        ));
        ui.add_space(2.0);
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
                        let font_size = TextStyle::Monospace.resolve(ui.style()).size;
                        for section in &mut job.sections {
                            section.format.font_id.size = font_size;
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
                            .rect_filled(rect, 2, ACCENT.gamma_multiply(0.30));
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
        let panel = SidePanel::right("assistant")
            .default_width(self.preferences.assistant_width)
            .width_range(230.0..=(ctx.content_rect().width() * 0.42).clamp(260.0, 520.0))
            .frame(panel_frame(Margin::same(12)))
            .show(ctx, |ui| self.workspace_chat(ui));
        let width = panel.response.rect.width();
        if width < (ctx.content_rect().width() * 0.42).clamp(260.0, 520.0) {
            self.preferences.assistant_width = width;
        }
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
                    let response = egui::CollapsingHeader::new(RichText::new(name).color(TEXT))
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
                    let response = ui
                        .selectable_label(active == Some(entry.path.as_path()), name)
                        .on_hover_text(entry.path.display().to_string());
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
            ui.label(RichText::new(error).small().color(ERROR));
        }
    }
    directories.insert(path.to_owned(), super::DirectoryLoad::Ready(entries));
    selected
}

use std::path::{Path, PathBuf};

use eframe::egui::{self, Context, Id, Key, RichText, TextEdit, Ui};

use super::{Editor, Pending, theme::*};
use crate::workspace;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum EntryKind {
    File,
    Folder,
}

pub(super) enum TreeAction {
    Open(PathBuf),
    Create(EntryKind, PathBuf),
    Terminal(PathBuf),
    CopyPath(PathBuf),
    Find,
    Move(PathBuf),
    Delete(PathBuf),
}

pub(super) struct EntryDraft {
    parent: PathBuf,
    source: Option<PathBuf>,
    kind: EntryKind,
    pub(super) name: String,
    focus: bool,
    error: Option<String>,
}

impl EntryDraft {
    fn base_directory<'a>(&'a self, project: Option<&'a Path>) -> &'a Path {
        project
            .filter(|root| self.parent.starts_with(root))
            .unwrap_or(&self.parent)
    }
}

fn menu_row(ui: &mut Ui, icon: Icon, title: &str) -> bool {
    let response = ui.add_sized(
        [ui.available_width(), 28.0],
        egui::Button::new("").frame(false),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tr(title)));
    let hover = super::motion::state(
        ui,
        response.id.with("menu-hover"),
        response.hovered() || response.has_focus(),
        super::motion::HOVER,
    );
    let painter = ui.painter_at(response.rect);
    painter.rect_filled(response.rect, 6, colors().hover.gamma_multiply(hover));
    let left = response.rect.left_center();
    draw_icon_sized(
        &painter,
        left + egui::vec2(14.0, 0.0),
        icon,
        colors().secondary,
        15.0,
    );
    painter.text(
        left + egui::vec2(31.0, 0.0),
        egui::Align2::LEFT_CENTER,
        tr(title),
        egui::FontId::proportional(BODY),
        colors().text,
    );
    #[cfg(test)]
    super::motion::record(ui, title, response.rect);
    response.clicked()
}

pub(super) fn tree_menu(
    ui: &mut Ui,
    root: &Path,
    path: &Path,
    directory: bool,
) -> Option<TreeAction> {
    let parent = if directory {
        path
    } else {
        path.parent().unwrap_or(root)
    };
    let mut action = creation_menu(ui, parent);
    if path != root {
        ui.separator();
        if menu_row(ui, Icon::Copy, "Copy relative path") {
            action = Some(TreeAction::CopyPath(path.to_owned()));
        }
        if menu_row(ui, Icon::Type, "Rename or move") {
            action = Some(TreeAction::Move(path.to_owned()));
        }
        if menu_row(ui, Icon::Trash, "Delete") {
            action = Some(TreeAction::Delete(path.to_owned()));
        }
    }
    if action.is_some() {
        ui.close();
    }
    action
}

pub(super) fn creation_menu(ui: &mut Ui, parent: &Path) -> Option<TreeAction> {
    ui.set_min_width(184.0);
    ui.label(muted(workspace::filename(parent)));
    let action = if menu_row(ui, Icon::FilePlus, "New file") {
        TreeAction::Create(EntryKind::File, parent.to_owned())
    } else if menu_row(ui, Icon::Folder, "New folder") {
        TreeAction::Create(EntryKind::Folder, parent.to_owned())
    } else if menu_row(ui, Icon::Terminal, "New terminal here") {
        TreeAction::Terminal(parent.to_owned())
    } else {
        return None;
    };
    Some(action)
}

pub(super) fn creation_context(response: &egui::Response, folder: &Path) -> Option<TreeAction> {
    let mut action = None;
    response.context_menu(|menu| {
        action = creation_menu(menu, folder);
        if action.is_some() {
            menu.close();
        }
    });
    action
}

pub(super) fn document_context(
    response: &egui::Response,
    folder: &Path,
    file: &Path,
    root: Option<&Path>,
) -> Option<TreeAction> {
    let mut action = None;
    response.context_menu(|menu| {
        action = creation_menu(menu, folder);
        menu.separator();
        if menu_row(menu, Icon::Search, "Find in file") {
            action = Some(TreeAction::Find);
        }
        let label = if root.is_some_and(|root| file.starts_with(root)) {
            "Copy relative path"
        } else {
            "Copy path"
        };
        if menu_row(menu, Icon::Copy, label) {
            action = Some(TreeAction::CopyPath(file.to_owned()));
        }
        if action.is_some() {
            menu.close();
        }
    });
    action
}

impl Editor {
    pub(super) fn begin_entry(
        &mut self,
        kind: EntryKind,
        parent: PathBuf,
        source: Option<PathBuf>,
    ) {
        let name = source
            .as_deref()
            .and_then(|p| p.strip_prefix(self.root.as_deref()?).ok())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.entry_draft = Some(EntryDraft {
            parent,
            source,
            kind,
            name,
            focus: true,
            error: None,
        });
        self.hub_open = false;
        self.studio_open = false;
        self.clean.focus = false;
        self.sidebar = super::Sidebar::Files;
        self.sidebar_open = true;
        self.sidebar_requested |= self.root.is_none();
        self.focus_editor = false;
        self.explorer_filter.clear();
    }

    pub(super) fn tree_action(&mut self, action: TreeAction, ctx: &Context) {
        match action {
            TreeAction::Open(path) => self.open_file(path),
            TreeAction::Create(kind, parent) => self.begin_entry(kind, parent, None),
            TreeAction::Terminal(parent) => {
                self.clean.focus = false;
                self.show_tool("terminal");
                self.terminals
                    .open_tab(crate::agents_runtime::plain(&parent), ctx);
            }
            TreeAction::Find => self.show_find(),
            TreeAction::CopyPath(path) => {
                let path = self
                    .root
                    .as_deref()
                    .and_then(|root| path.strip_prefix(root).ok())
                    .unwrap_or(&path);
                ctx.copy_text(path.display().to_string());
            }
            TreeAction::Move(path) => {
                let Some(parent) = path.parent() else { return };
                self.begin_entry(
                    if path.is_dir() {
                        EntryKind::Folder
                    } else {
                        EntryKind::File
                    },
                    parent.to_owned(),
                    Some(path),
                );
            }
            TreeAction::Delete(path) => {
                if self
                    .documents
                    .iter()
                    .any(|doc| doc.path == path && doc.dirty)
                {
                    self.error(tr("Save or close the modified file before deleting it."));
                    return;
                }
                self.pending_delete = Some(path);
            }
        }
    }

    pub(super) fn entry_row(&mut self, ui: &mut Ui) {
        let Some(mut draft) = self.entry_draft.take() else {
            return;
        };
        let root = draft.base_directory(self.root.as_deref());
        let relative = draft.parent.strip_prefix(root).unwrap_or(Path::new(""));
        let location = if draft.source.is_some() || relative.as_os_str().is_empty() {
            workspace::filename(root)
        } else {
            relative.display().to_string()
        };
        let title = if draft.source.is_some() {
            "Rename or move"
        } else if draft.kind == EntryKind::Folder {
            "New folder"
        } else {
            "New file"
        };
        let mut confirm = false;
        let mut cancel = false;
        ui.push_id("explorer-entry", |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(tr(title)).size(SMALL).strong());
                ui.add(egui::Label::new(muted(location)).truncate())
                    .on_hover_text(draft.parent.display().to_string());
            });
            let id = ui.make_persistent_id("entry-name");
            if draft.focus {
                ui.memory_mut(|m| m.request_focus(id));
            }
            if ui.memory(|m| m.has_focus(id)) {
                confirm = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter));
            }
            let focused = ui.memory(|m| m.has_focus(id));
            let border = if draft.error.is_some() {
                colors().error
            } else if focused {
                colors().accent
            } else {
                colors().control_border
            };
            egui::Frame::new()
                .fill(colors().inset)
                .stroke(egui::Stroke::new(1.0_f32, border))
                .corner_radius(8)
                .inner_margin(egui::Margin::symmetric(6, 3))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.0;
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(16.0, 23.0), egui::Sense::hover());
                        draw_icon_sized(
                            ui.painter(),
                            rect.center(),
                            if draft.kind == EntryKind::Folder {
                                Icon::Folder
                            } else {
                                Icon::FilePlus
                            },
                            colors().accent,
                            14.0,
                        );
                        let input = ui.add(
                            TextEdit::singleline(&mut draft.name)
                                .id(id)
                                .margin(egui::Margin::ZERO)
                                .frame(false)
                                .font(egui::FontId::monospace(SMALL))
                                .hint_text(tr(if draft.kind == EntryKind::Folder {
                                    "Folder name"
                                } else {
                                    "File name"
                                }))
                                .desired_width((ui.available_width() - 50.0).max(24.0))
                                .char_limit(512),
                        );
                        #[cfg(test)]
                        super::motion::record(ui, "Entry name", input.rect);
                        if input.has_focus() || input.lost_focus() {
                            cancel =
                                ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape));
                        }
                        if input.changed() {
                            draft.error = None;
                        }
                        if draft.focus && draft.source.is_some() {
                            let mut state = TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
                            state
                                .cursor
                                .set_char_range(Some(egui::text::CCursorRange::two(
                                    egui::text::CCursor::new(0),
                                    egui::text::CCursor::new(draft.name.chars().count()),
                                )));
                            state.store(ui.ctx(), id);
                        }
                        draft.focus = false;
                        confirm |= ui
                            .add_enabled_ui(!draft.name.trim().is_empty(), |ui| {
                                icon_button_sized(ui, Icon::Check, "Confirm", false, 20.0)
                            })
                            .inner
                            .clicked();
                        cancel |=
                            icon_button_sized(ui, Icon::Close, "Cancel", false, 20.0).clicked();
                    });
                });
            if let Some(message) = &draft.error {
                ui.add(
                    egui::Label::new(RichText::new(message).size(SMALL).color(colors().error))
                        .wrap(),
                );
            }
            ui.add_space(4.0);
        });
        if cancel {
            return;
        }
        if confirm && !draft.name.trim().is_empty() {
            let relative = if draft.source.is_some() {
                PathBuf::from(draft.name.trim())
            } else {
                PathBuf::from(relative).join(draft.name.trim())
            };
            match self.commit_entry(&draft, &relative) {
                Ok(path) => {
                    if let Some(root) = self.root.as_deref().filter(|root| path.starts_with(root)) {
                        for dir in path.ancestors().skip(1).take_while(|dir| *dir != root) {
                            let mut state =
                                egui::collapsing_header::CollapsingState::load_with_default_open(
                                    ui.ctx(),
                                    Id::new(("explorer-dir", dir)),
                                    true,
                                );
                            state.set_open(true);
                            state.store(ui.ctx());
                        }
                    }
                    return;
                }
                Err(error) => {
                    draft.error = Some(match error.kind() {
                        std::io::ErrorKind::AlreadyExists => {
                            tr("That name is already in use.").to_owned()
                        }
                        std::io::ErrorKind::NotFound => {
                            tr("The parent folder does not exist.").to_owned()
                        }
                        _ => tr(&error.to_string()).to_owned(),
                    });
                    draft.focus = true;
                }
            }
        }
        self.entry_draft = Some(draft);
    }

    fn commit_entry(&mut self, draft: &EntryDraft, relative: &Path) -> std::io::Result<PathBuf> {
        let root = draft.base_directory(self.root.as_deref());
        let name = relative.to_string_lossy();
        let target = if let Some(source) = &draft.source {
            let target = workspace::move_entry(root, source, &name)?;
            for doc in &mut self.documents {
                if let Ok(suffix) = doc.path.strip_prefix(source) {
                    doc.path = if suffix.as_os_str().is_empty() {
                        target.clone()
                    } else {
                        target.join(suffix)
                    };
                }
            }
            target
        } else if draft.kind == EntryKind::File {
            let target = workspace::create_file(root, &name)?;
            self.open_file(target.clone());
            target
        } else {
            workspace::create_folder(root, &name)?
        };
        self.directories.clear();
        Ok(target)
    }

    pub(super) fn file_dialogs(&mut self, ctx: &Context) {
        if let Some(path) = self.pending_delete.take() {
            let modal = egui::Modal::new(Id::new("delete-entry"))
                .frame(modal_frame())
                .show(ctx, |ui| {
                    ui.heading(format!("Delete {}?", workspace::filename(&path)));
                    ui.label(muted(path.display().to_string()));
                    ui.label(tr("This permanently deletes a file or an empty folder."));
                    ui.horizontal(|ui| {
                        if ui.button(tr("Cancel")).clicked() {
                            return Some(false);
                        }
                        if ui
                            .button(RichText::new(tr("Delete")).color(colors().error))
                            .clicked()
                        {
                            return Some(true);
                        }
                        None
                    })
                    .inner
                });
            match modal.inner {
                Some(true) => {
                    let Some(root) = &self.root else { return };
                    match workspace::delete_entry(root, &path) {
                        Ok(()) => {
                            if let Some(index) =
                                self.documents.iter().position(|doc| doc.path == path)
                            {
                                self.perform(Pending::CloseTab(index), ctx);
                            }
                            self.directories.clear();
                            self.notice = Some((false, "Deleted from workspace".into()));
                        }
                        Err(error) => self.error(format!("Could not delete entry. {error}")),
                    }
                }
                None if !modal.should_close() => self.pending_delete = Some(path),
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_beside_loose_document_with_or_without_another_project_open() {
        let fixture = tempfile::tempdir().unwrap();
        let loose = fixture.path().join("notes");
        let project = fixture.path().join("project");
        std::fs::create_dir(&loose).unwrap();
        std::fs::create_dir(&project).unwrap();
        let file = loose.join("first.txt");
        std::fs::write(&file, "original").unwrap();
        let ctx = Context::default();
        for (root, name) in [(None, "second.txt"), (Some(project.clone()), "third.txt")] {
            let mut editor = Editor::new(&ctx, root, Default::default());
            editor.open_file(file.clone());
            let dir = editor.editor_directory().unwrap();
            editor.tree_action(TreeAction::Create(EntryKind::File, dir), &ctx);
            assert!(editor.sidebar_visible());
            let draft = editor.entry_draft.take().unwrap();
            let path = editor.commit_entry(&draft, Path::new(name)).unwrap();
            assert_eq!(path, loose.join(name).canonicalize().unwrap());
            assert_eq!(editor.documents[editor.active.unwrap()].path, path);
            assert!(!project.join(name).exists());
        }
    }
}

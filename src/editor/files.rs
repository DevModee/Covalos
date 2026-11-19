use std::path::{Path, PathBuf};

use eframe::egui::{self, Context, Id, Key, RichText, TextEdit, Ui};

use super::{Editor, Pending, theme::*};
use crate::workspace;

pub(super) enum TreeAction {
    Open(PathBuf),
    CreateFile(String),
    CreateFolder(String),
    Move(PathBuf),
    Delete(PathBuf),
}

pub(super) fn tree_menu(
    ui: &mut Ui,
    root: &Path,
    path: &Path,
    directory: bool,
) -> Option<TreeAction> {
    let mut action = None;
    let root_item = root == path;
    let parent = if directory {
        path
    } else {
        path.parent().unwrap_or(root)
    };
    let relative = parent.strip_prefix(root).unwrap_or(parent);
    let prefix = if relative.as_os_str().is_empty() {
        String::new()
    } else {
        format!("{}/", relative.display())
    };
    if ui.button(tr("New file here…")).clicked() {
        action = Some(TreeAction::CreateFile(prefix.clone()));
    }
    if ui.button(tr("New folder here…")).clicked() {
        action = Some(TreeAction::CreateFolder(prefix));
    }
    if !root_item {
        if ui.button(tr("Rename or move…")).clicked() {
            action = Some(TreeAction::Move(path.to_owned()));
        }
        if ui.button(tr("Delete…")).clicked() {
            action = Some(TreeAction::Delete(path.to_owned()));
        }
    }
    if action.is_some() {
        ui.close();
    }
    action
}

pub(super) enum FileChange {
    Folder,
    Move(PathBuf),
}

impl Editor {
    pub(super) fn tree_action(&mut self, action: TreeAction) {
        match action {
            TreeAction::Open(path) => self.open_file(path),
            TreeAction::CreateFile(prefix) => {
                self.notice = None;
                self.new_file = Some(prefix);
                self.focus_new_file = true;
            }
            TreeAction::CreateFolder(prefix) => {
                self.notice = None;
                self.file_change = Some((FileChange::Folder, prefix));
                self.focus_new_file = true;
            }
            TreeAction::Move(path) => {
                let Some(root) = &self.root else { return };
                let relative = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .display()
                    .to_string();
                self.notice = None;
                self.file_change = Some((FileChange::Move(path), relative));
                self.focus_new_file = true;
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

    pub(super) fn file_dialogs(&mut self, ctx: &Context) {
        if let Some((change, mut name)) = self.file_change.take() {
            let header = match &change {
                FileChange::Folder => "New folder",
                FileChange::Move(_) => "Rename or move",
            };
            let modal = egui::Modal::new(Id::new("change-file"))
                .frame(modal_frame())
                .show(ctx, |ui| {
                    ui.heading(header);
                    if let Some((true, message)) = &self.notice {
                        ui.label(RichText::new(message).color(colors().error));
                    }
                    let input = ui.add(
                        TextEdit::singleline(&mut name)
                            .hint_text(tr("Path relative to the workspace"))
                            .desired_width(340.0)
                            .char_limit(512),
                    );
                    if std::mem::take(&mut self.focus_new_file) {
                        input.request_focus();
                    }
                    let enter =
                        input.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter));
                    ui.horizontal(|ui| {
                        if ui.button(tr("Cancel")).clicked() {
                            return Some(false);
                        }
                        if (ui
                            .add_enabled(!name.trim().is_empty(), primary(tr("Confirm")))
                            .clicked()
                            || enter)
                            && !name.trim().is_empty()
                        {
                            return Some(true);
                        }
                        None
                    })
                    .inner
                });
            match modal.inner {
                Some(true) if !self.apply_file_change(&change, &name) => {
                    self.file_change = Some((change, name))
                }
                None if !modal.should_close() => self.file_change = Some((change, name)),
                _ => {}
            }
        }
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

    fn apply_file_change(&mut self, change: &FileChange, name: &str) -> bool {
        let Some(root) = &self.root else { return false };
        let result = match change {
            FileChange::Folder => workspace::create_folder(root, name),
            FileChange::Move(path) => workspace::move_entry(root, path, name),
        };
        match result {
            Ok(target) => {
                if let FileChange::Move(source) = change {
                    for doc in &mut self.documents {
                        if let Ok(relative) = doc.path.strip_prefix(source) {
                            doc.path = if relative.as_os_str().is_empty() {
                                target.clone()
                            } else {
                                target.join(relative)
                            };
                        }
                    }
                }
                self.directories.clear();
                self.notice = Some((
                    false,
                    format!("Workspace updated · {}", workspace::filename(&target)),
                ));
                true
            }
            Err(error) => {
                self.error(format!("Could not update workspace. {error}"));
                false
            }
        }
    }
}

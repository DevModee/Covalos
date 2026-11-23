#[cfg(all(test, windows))]
mod evidence;
#[cfg(test)]
mod visual_tests;

mod agent_chat;
mod agent_hub;
mod assets;
mod brands;
mod clean;
mod companion;
mod discord;
mod dither;
mod files;
mod ghost;
mod git_gutter;
mod guided;
mod hub;
mod i18n;
mod icons;
mod identity;
mod ligatures;
mod lsp;
mod mascot;
mod motion;
mod navigation;
mod panels;
mod preferences;
mod preview;
mod pty;
mod sessions;
mod shell;
mod skills;
mod studio;
mod syntax;
mod terminal;
mod theme;
mod tools;
mod ui_icons;

use i18n::tr;
use preferences::Preferences;
pub(crate) use theme::TITLE;

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
    thread,
};

use eframe::egui;

use crate::{
    assistant,
    workspace::{self, Document, Entry, SearchHit},
};

#[derive(Clone, Copy, Default, PartialEq)]
enum Sidebar {
    #[default]
    Files,
    Search,
    Git,
    Chats,
}

enum Pending {
    OpenWorkspace(PathBuf),
    CloseTab(usize),
    Exit,
}

enum DirectoryLoad {
    Pending(Receiver<Result<Vec<Entry>, String>>),
    Ready(Result<Vec<Entry>, String>),
}

struct SearchJob {
    hits: Receiver<SearchHit>,
    cancelled: Arc<AtomicBool>,
}

impl Drop for SearchJob {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

pub struct Editor {
    root: Option<PathBuf>,
    directories: HashMap<PathBuf, DirectoryLoad>,
    documents: Vec<Document>,
    active: Option<usize>,
    focus_editor: bool,
    cursor: usize,
    selection: Option<(usize, usize)>,
    sidebar: Sidebar,
    workspace_query: String,
    search_focus: bool,
    search_hits: Vec<SearchHit>,
    search_job: Option<SearchJob>,
    find_open: bool,
    find_focus: bool,
    find_query: String,
    find_match: Option<(usize, usize)>,
    find_empty: bool,
    preferences: Preferences,
    saved_preferences: Preferences,
    preferences_path: Option<PathBuf>,
    settings_open: bool,
    about_open: bool,
    hub_open: bool,
    companion: companion::Companion,
    identity: identity::Identity,
    tools: tools::Tools,
    terminals: terminal::Terminals,
    lsp: lsp::Lsp,
    /// Inline ghost-text completion in the code editor.
    ghost: ghost::Ghost,
    /// Added, modified and deleted line markers against `HEAD`.
    git_gutter: git_gutter::GitGutter,
    preview: preview::Preview,
    palette: Option<shell::Palette>,
    chat: hub::Chat,
    chat_saved_revision: u64,
    chat_observed_revision: u64,
    chat_save_due: Option<std::time::Instant>,
    conversation_writable: bool,
    conversation_exit_warning: bool,
    hub: hub::Hub,
    skills: skills::Skills,
    include_active_file: bool,
    include_project_note: bool,
    notice: Option<(bool, String)>,
    pending: Option<Pending>,
    force_close: bool,
    entry_draft: Option<files::EntryDraft>,
    pending_delete: Option<PathBuf>,
    syntax: egui_extras::syntax_highlighting::CodeTheme,
    clean: clean::Clean,
    studio: studio::Studio,
    studio_open: bool,
    nav: navigation::Navigation,
    /// Side panel opened from the activity bar while no project is open.
    sidebar_requested: bool,
    /// User toggle for collapsing or expanding the sidebar (Ctrl+B).
    sidebar_open: bool,
    /// Live filename filter for the Explorer tree.
    explorer_filter: String,
    notifications: Vec<shell::Notification>,
    notifications_open: bool,
    notifications_query: String,
    notifications_filter: shell::NotificationFilter,
    discord: discord::Presence,
    guided: guided::GuidedController,
    session_started: u64,
    explorer_rect: Option<egui::Rect>,
    disk_checked: Option<std::time::Instant>,
    /// Open files changed on disk while they had unsaved edits.
    disk_conflicts: Vec<PathBuf>,
    #[cfg(test)]
    mood_override: Option<identity::Mood>,
}

impl Editor {
    pub fn from_preferences(ctx: &egui::Context, root: Option<PathBuf>) -> Self {
        let path = Preferences::path();
        let loaded = path.as_deref().map(Preferences::load).transpose();
        let (preferences, error) = match loaded {
            Ok(value) => (value.unwrap_or_default(), None),
            Err(error) => (Preferences::default(), Some(error)),
        };
        let mut editor = Self::new(ctx, root, preferences);
        editor.preferences_path = path;
        editor.restore_conversation();
        editor.companion.start(ctx);
        if let Some(error) = error {
            editor.error(format!("Could not read settings. {error}"));
        }
        editor
    }

    fn new(ctx: &egui::Context, root: Option<PathBuf>, preferences: Preferences) -> Self {
        theme::install_fonts(ctx, preferences.ubuntu_interface, preferences.hack_editor);
        // Ctrl +/- must zoom the panel under the pointer, not the whole interface.
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
        theme::set_brightness(preferences.brightness);
        theme::install(
            ctx,
            preferences.font_size,
            preferences.high_contrast,
            preferences.theme,
        );
        let mut editor = Self {
            root: None,
            directories: HashMap::new(),
            documents: Vec::new(),
            active: None,
            focus_editor: false,
            cursor: 0,
            selection: None,
            sidebar: Sidebar::Files,
            workspace_query: String::new(),
            search_focus: false,
            search_hits: Vec::new(),
            search_job: None,
            find_open: false,
            find_focus: false,
            find_query: String::new(),
            find_match: None,
            find_empty: false,
            saved_preferences: preferences.clone(),
            preferences,
            preferences_path: None,
            settings_open: false,
            about_open: false,
            hub_open: false,
            companion: companion::Companion::default(),
            identity: identity::Identity::default(),
            tools: tools::Tools::default(),
            terminals: terminal::Terminals::default(),
            lsp: lsp::Lsp::default(),
            ghost: ghost::Ghost::default(),
            git_gutter: git_gutter::GitGutter::default(),
            preview: preview::Preview::default(),
            palette: None,
            chat: hub::Chat::default(),
            chat_saved_revision: 0,
            chat_observed_revision: 0,
            chat_save_due: None,
            conversation_writable: true,
            conversation_exit_warning: false,
            hub: hub::Hub::default(),
            skills: skills::Skills::default(),
            include_active_file: false,
            include_project_note: false,
            notice: None,
            pending: None,
            force_close: false,
            entry_draft: None,
            pending_delete: None,
            clean: clean::Clean::default(),
            studio: studio::Studio::default(),
            studio_open: false,
            nav: navigation::Navigation::default(),
            sidebar_requested: false,
            sidebar_open: true,
            explorer_filter: String::new(),
            notifications: Vec::new(),
            notifications_open: false,
            notifications_query: String::new(),
            notifications_filter: shell::NotificationFilter::All,
            discord: discord::Presence::default(),
            guided: guided::GuidedController::default(),
            session_started: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
            explorer_rect: None,
            disk_checked: None,
            disk_conflicts: Vec::new(),
            #[cfg(test)]
            mood_override: None,
            syntax: egui_extras::syntax_highlighting::CodeTheme::dark(
                egui::TextStyle::Monospace.resolve(&ctx.style()).size,
            ),
        };
        let startup_root = root.or_else(|| {
            editor
                .preferences
                .last_workspace
                .clone()
                .filter(|p| p.is_dir())
        });
        if let Some(root) = startup_root {
            editor.open_workspace(root);
        }
        editor
    }

    fn pick_workspace(&mut self, ctx: &egui::Context) {
        let mut dialog = rfd::FileDialog::new().set_title("Open workspace");
        if let Some(root) = &self.root {
            dialog = dialog.set_directory(root);
        }
        if let Some(path) = dialog.pick_folder() {
            self.request(Pending::OpenWorkspace(path), ctx);
        }
    }

    fn open_workspace(&mut self, path: PathBuf) {
        let root = match path.canonicalize() {
            Ok(root) if root.is_dir() => root,
            Ok(_) => return self.error(tr("Choose a directory to open as a workspace.")),
            Err(error) => return self.error(format!("Could not open workspace. {error}")),
        };
        if self.root.as_ref() == Some(&root) {
            return;
        }
        if !self.save_conversation() {
            return;
        }
        self.snapshot_workspace_state();
        self.guided.save_to_disk(self.root.as_deref());
        self.preferences.remember(&root);
        self.reset_chat();
        self.hub.change_workspace(
            self.preferences.note(&root).to_owned(),
            self.preferences.tasks(&root),
        );
        self.include_project_note = false;
        self.guided.clear(None);
        self.guided.restore_for_workspace(&root);
        self.entry_draft = None;
        self.root = Some(root.clone());
        self.hub_open = false;
        self.tools.workspace_changed();
        self.terminals.workspace_changed(self.root.as_deref());
        self.directories.clear();
        self.documents.clear();
        self.active = None;
        self.sidebar = Sidebar::Files;
        self.sidebar_open = true;
        self.explorer_filter.clear();
        self.workspace_query.clear();
        self.search_hits.clear();
        self.search_job = None;
        self.find_open = false;
        self.notice = None;
        self.restore_conversation();
        // Restore previously open tabs and active file for this workspace.
        if let Some(saved) = self.preferences.workspace_files(&root).cloned() {
            for file in &saved.files {
                if file.is_file() {
                    self.open_file(file.clone());
                }
            }
            if let Some(active_path) = saved.active
                && let Some(idx) = self.documents.iter().position(|d| d.path == active_path)
            {
                self.activate(idx);
            }
        }
    }

    fn snapshot_workspace_state(&mut self) {
        if let Some(root) = self.root.clone() {
            let files: Vec<PathBuf> = self.documents.iter().map(|d| d.path.clone()).collect();
            let active = self
                .active
                .and_then(|i| self.documents.get(i))
                .map(|d| d.path.clone());
            self.preferences
                .remember_workspace_files(&root, files, active);
        }
        self.preferences.agent_model = self.hub.agents.chat.model.clone();
        self.preferences.agent_effort = self.hub.agents.chat.effort.clone().unwrap_or_default();
        self.preferences.agent_session = self.hub.agents.chat.dock.clone();
        self.preferences.agent_detached = self.hub.agents.chat.detached.clone();
    }

    fn persist_all_state(&mut self) {
        self.snapshot_workspace_state();
        self.guided.save_to_disk(self.root.as_deref());
        if let Some(rt) = self.hub.agents.runtime.as_mut() {
            rt.save_sessions();
        }
        if let Some(path) = &self.preferences_path {
            self.saved_preferences = self.preferences.clone();
            let _ = self.preferences.save(path);
        }
    }

    fn pick_file(&mut self) {
        let mut dialog = rfd::FileDialog::new().set_title("Open file");
        if let Some(root) = &self.root {
            dialog = dialog.set_directory(root);
        }
        if let Some(path) = dialog.pick_file() {
            self.open_file(path);
        }
    }

    fn pick_new_project(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Choose a name and location for the project")
            .save_file()
        else {
            return;
        };
        match std::fs::create_dir(&path) {
            Ok(()) => self.request(Pending::OpenWorkspace(path), ctx),
            Err(error) => self.error(format!("Could not create project folder. {error}")),
        }
    }

    fn open_file(&mut self, path: PathBuf) {
        let path = match path.canonicalize() {
            Ok(path) => path,
            Err(error) => return self.error(format!("Could not open file. {error}")),
        };
        match self.documents.iter().position(|doc| doc.path == path) {
            Some(index) => self.activate(index),
            None => match Document::open(path) {
                Ok(document) => {
                    self.documents.push(document);
                    self.activate(self.documents.len() - 1);
                    self.notice = None;
                }
                Err(error) => self.error(format!("Could not open file. {error}")),
            },
        }
    }

    fn activate(&mut self, index: usize) {
        if let Some(path) = self.documents.get(index).map(|d| d.path.clone())
            && self.raise_floating(&path)
        {
            return;
        }
        if self.active != Some(index) {
            self.active = Some(index);
            self.include_active_file = false;
            self.cursor = 0;
            self.selection = None;
            self.find_match = None;
            self.find_empty = false;
        }
        self.focus_editor = true;
    }

    fn save_active(&mut self) {
        // A focused floating editor saves its own document.
        let floating = self
            .clean
            .focused
            .as_ref()
            .and_then(|path| self.documents.iter().position(|d| &d.path == path));
        let Some(document) = floating
            .or(self.active)
            .and_then(|index| self.documents.get_mut(index))
        else {
            return;
        };
        match document.save() {
            Ok(()) => {
                self.notice = Some((
                    false,
                    format!("Saved {}", workspace::filename(&document.path)),
                ))
            }
            Err(error) => self.error(format!("Could not save file. {error}")),
        }
    }

    fn save_all(&mut self) -> bool {
        for document in &mut self.documents {
            if let Err(error) = document.save() {
                let name = workspace::filename(&document.path);
                self.error(format!("Could not save {name}. {error}"));
                return false;
            }
        }
        true
    }

    fn request(&mut self, action: Pending, ctx: &egui::Context) {
        let dirty = match &action {
            Pending::CloseTab(index) => self.documents[*index].dirty,
            Pending::OpenWorkspace(path) => {
                if path.canonicalize().ok().as_ref() == self.root.as_ref() && self.root.is_some() {
                    return;
                }
                self.documents.iter().any(|doc| doc.dirty)
            }
            Pending::Exit => self.documents.iter().any(|doc| doc.dirty),
        };
        if dirty {
            self.pending = Some(action);
        } else {
            self.perform(action, ctx);
        }
    }

    fn perform(&mut self, action: Pending, ctx: &egui::Context) {
        match action {
            Pending::OpenWorkspace(path) => self.open_workspace(path),
            Pending::CloseTab(index) => {
                if self.active == Some(index) {
                    self.include_active_file = false;
                }
                self.documents.remove(index);
                self.active = self.active.and_then(|active| match active.cmp(&index) {
                    std::cmp::Ordering::Less => Some(active),
                    std::cmp::Ordering::Equal if self.documents.is_empty() => None,
                    std::cmp::Ordering::Equal => Some(active.min(self.documents.len() - 1)),
                    std::cmp::Ordering::Greater => Some(active - 1),
                });
                if let Some(active) = self.active
                    && self.clean.is_floating(&self.documents[active].path)
                {
                    self.active = self.next_docked(active);
                }
                self.cursor = 0;
                self.selection = None;
                self.find_match = None;
                self.find_empty = false;
                self.focus_editor = self.active.is_some();
            }
            Pending::Exit => {
                if !self.save_conversation() {
                    self.conversation_exit_warning = true;
                    return;
                }
                self.persist_all_state();
                self.force_close = true;
                self.chat.run.take();
                self.hub.general_chat.run.take();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    #[cfg(test)]
    fn create_file(&mut self, name: &str) -> bool {
        let Some(root) = &self.root else { return false };
        match workspace::create_file(root, name) {
            Ok(path) => {
                if let Some(parent) = path.parent() {
                    self.directories.remove(parent);
                }
                self.open_file(path);
                true
            }
            Err(error) => {
                self.error(format!("Could not create file. {error}"));
                false
            }
        }
    }

    fn error(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.editor_log(&message);
        self.notify(shell::NotificationSource::Covalos, message.clone(), true);
        self.notice = Some((true, message));
    }

    fn notify(
        &mut self,
        source: shell::NotificationSource,
        message: impl Into<String>,
        is_error: bool,
    ) {
        const MAX_NOTIFICATIONS: usize = 64;
        let message = message.into();
        if self.notifications.len() >= MAX_NOTIFICATIONS {
            self.notifications.remove(0);
        }
        self.notifications.push(shell::Notification {
            source,
            message,
            is_error,
            unread: !self.notifications_open,
        });
    }

    fn show_find(&mut self) {
        if self.active.is_some() {
            self.find_open = true;
            self.find_focus = true;
        }
    }

    fn find(&mut self, reverse: bool) {
        self.find_empty = false;
        let Some(doc) = self.active.and_then(|index| self.documents.get(index)) else {
            return;
        };
        if self.find_query.is_empty() {
            self.find_match = None;
            return;
        }
        let anchor = self.find_match.map_or(
            self.cursor,
            |(start, end)| if reverse { start } else { end },
        );
        let anchor = doc
            .text
            .char_indices()
            .nth(anchor)
            .map_or(doc.text.len(), |(index, _)| index);
        let matcher = workspace::literal_query(&self.find_query);
        let found = if reverse {
            matcher
                .find_iter(&doc.text[..anchor])
                .last()
                .or_else(|| matcher.find_iter(&doc.text).last())
        } else {
            matcher
                .find_at(&doc.text, anchor)
                .or_else(|| matcher.find(&doc.text))
        };
        self.find_match = found.map(|m| {
            let start = doc.text[..m.start()].chars().count();
            (start, start + m.as_str().chars().count())
        });
        self.find_empty = found.is_none();
        if self.find_match.is_some() {
            self.selection = self.find_match;
        }
    }

    fn start_search(&mut self, ctx: &egui::Context) {
        self.search_hits.clear();
        self.search_job = None;
        let Some(root) = self.root.clone() else {
            return;
        };
        if self.workspace_query.is_empty() {
            return;
        }
        let query = self.workspace_query.clone();
        let (sender, hits) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            workspace::search(&root, &query, worker_cancelled, |hit| {
                let received = sender.send(hit).is_ok();
                ctx.request_repaint();
                received
            });
            drop(sender);
            ctx.request_repaint();
        });
        self.search_job = Some(SearchJob { hits, cancelled });
    }

    fn select_hit(&mut self, index: usize) {
        let hit = &self.search_hits[index];
        let (path, line, column) = (hit.path.clone(), hit.line, hit.column);
        self.open_file(path.clone());
        let Some(doc) = self.active.map(|index| &self.documents[index]) else {
            return;
        };
        if doc.path != path {
            return;
        }
        let row_start = doc
            .text
            .split_inclusive('\n')
            .take(line - 1)
            .map(|row| row.chars().count())
            .sum::<usize>();
        let row_length = doc
            .text
            .split('\n')
            .nth(line - 1)
            .map_or(0, |row| row.chars().count());
        let index = row_start + column.min(row_length);
        self.selection = Some((index, index));
    }

    fn conversation_close_modal(&mut self, ctx: &egui::Context) {
        if !self.conversation_exit_warning {
            return;
        }
        egui::Window::new(tr("Conversation could not be saved"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(tr("Your latest chat changes are still in memory."));
                ui.horizontal(|ui| {
                    if ui.button(tr("Retry save and close")).clicked() {
                        self.conversation_exit_warning = false;
                        self.perform(Pending::Exit, ctx);
                    }
                    if ui.button(tr("Close without saving chat")).clicked() {
                        self.conversation_exit_warning = false;
                        self.chat_saved_revision = self.chat.revision;
                        self.perform(Pending::Exit, ctx);
                    }
                    if ui.button(tr("Keep editing")).clicked() {
                        self.conversation_exit_warning = false;
                    }
                });
            });
    }

    fn restore_conversation(&mut self) {
        self.chat_saved_revision = 0;
        self.chat_observed_revision = 0;
        self.chat_save_due = None;
        self.conversation_writable = true;
        let Some((config, root)) = self.preferences_path.as_deref().zip(self.root.as_deref())
        else {
            return;
        };
        match sessions::Conversation::load(config, root) {
            Ok(Some(value)) => self.chat = hub::Chat::restore(value),
            Ok(None) => {}
            Err(error) => {
                self.conversation_writable = false;
                self.chat_saved_revision = self.chat.revision;
                self.error(format!("Could not restore the conversation. Its stored file has been preserved. {error}"));
            }
        }
    }

    fn save_conversation(&mut self) -> bool {
        self.chat_save_due = None;
        if self.chat_saved_revision == self.chat.revision {
            return true;
        }
        let Some((config, root)) = self.preferences_path.as_deref().zip(self.root.as_deref())
        else {
            return true;
        };
        let result = if self.conversation_writable {
            self.chat.snapshot(root).save(config)
        } else {
            Err(std::io::Error::other(
                "Repair or move the unreadable conversation file before saving this workspace's chat.",
            ))
        };
        match result {
            Ok(()) => {
                self.chat_saved_revision = self.chat.revision;
                true
            }
            Err(error) => {
                self.error(format!("Could not save the conversation. {error}"));
                false
            }
        }
    }

    fn autosave_conversation(&mut self, ctx: &egui::Context) {
        if self.preferences_path.is_none() || self.root.is_none() {
            return;
        }
        if self.chat_observed_revision != self.chat.revision {
            self.chat_observed_revision = self.chat.revision;
            self.chat_save_due =
                Some(std::time::Instant::now() + std::time::Duration::from_millis(750));
        }
        if let Some(due) = self.chat_save_due {
            let remaining = due.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                self.save_conversation();
            } else {
                ctx.request_repaint_after(remaining);
            }
        }
    }

    fn reset_chat(&mut self) {
        let revision = self.chat.revision.wrapping_add(1);
        self.chat = hub::Chat::default();
        self.chat.revision = revision;
        self.hub.discard_workflow();
        self.save_task_history();
        self.include_active_file = false;
    }

    fn send_prompt(&mut self, ctx: &egui::Context) {
        let prompt = self.chat.prompt.trim();
        let Some(root) = self.root.clone() else {
            return;
        };
        if prompt.is_empty() || self.chat.run.is_some() || self.hub.workflow.is_some() {
            return;
        }
        let mut message = prompt.to_owned();
        let include = std::mem::take(&mut self.include_active_file);
        let input = if let Some(document) = self.active.map(|index| &self.documents[index])
            && let Ok(path) = document.path.strip_prefix(&root)
        {
            if include {
                message.push_str(&format!("\n\nAttached: {}", path.display()));
            }
            assistant::with_file_context(prompt, path, include.then_some(&document.text))
        } else {
            prompt.to_owned()
        };
        self.chat.push("You", message);
        self.chat.prompt.clear();
        self.tools.log_agent("Codex request started");
        self.chat
            .start(assistant::Scope::Workspace, root, input, ctx);
    }

    /// Picks up files that agents, git or other tools changed on disk.
    fn check_disk(&mut self, ctx: &egui::Context) {
        if self
            .disk_checked
            .is_some_and(|t| t.elapsed() < std::time::Duration::from_millis(900))
        {
            return;
        }
        self.disk_checked = Some(std::time::Instant::now());
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
        let mut log = Vec::new();
        for doc in &mut self.documents {
            match doc.sync_from_disk() {
                workspace::DiskState::Reloaded => {
                    log.push(format!("Reloaded {} (changed on disk)", doc.path.display()));
                }
                workspace::DiskState::Conflict => {
                    if !self.disk_conflicts.contains(&doc.path) {
                        self.disk_conflicts.push(doc.path.clone());
                    }
                }
                workspace::DiskState::Same => {}
            }
        }
        for line in log {
            self.editor_log(&line);
        }
        let open: Vec<PathBuf> = self.documents.iter().map(|d| d.path.clone()).collect();
        self.disk_conflicts.retain(|p| open.contains(p));
    }

    fn poll(&mut self) {
        if let Some(job) = &self.search_job {
            loop {
                match job.hits.try_recv() {
                    Ok(hit) => self.search_hits.push(hit),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.search_job = None;
                        break;
                    }
                }
            }
        }
    }
}

impl Editor {
    fn agent_mood(&self, focused: bool) -> identity::Mood {
        #[cfg(test)]
        if let Some(mood) = self.mood_override {
            return mood;
        }
        if !focused {
            return identity::Mood::Sleeping;
        }
        if self.hub.approval_pending() || self.companion.counts().1 > 0 {
            return identity::Mood::Approval;
        }
        if self.search_job.is_some() {
            return identity::Mood::Searching;
        }
        let mood = self.hub.mood(&self.chat);
        if mood == identity::Mood::Idle {
            self.companion.reported_mood()
        } else {
            mood
        }
    }

    fn render(&mut self, ctx: &egui::Context) {
        i18n::set(self.preferences.language);
        syntax::set_style(self.preferences.syntax);
        motion::set_intensity(if self.preferences.reduce_motion {
            0.0
        } else {
            self.preferences.motion
        });
        let focused = ctx.input(|input| input.focused);
        ctx.style_mut(|style| {
            style.animation_time = if focused && !self.preferences.reduce_motion {
                theme::HOVER_SECONDS
            } else {
                0.0
            };
            style.spacing.interact_size.y = if self.preferences.compact { 24.0 } else { 28.0 };
        });
        self.poll();
        self.poll_chats(ctx);
        self.poll_agent_runtime(ctx);
        self.tools.poll();
        self.terminals.poll();
        self.check_disk(ctx);
        self.lsp_sync(ctx);
        if self.terminals.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
        let snapshots = self.hub.snapshots(&self.chat, self.root.as_deref());
        self.companion.sync(
            snapshots,
            ctx,
            self.preferences.quiet_notifications,
            self.preferences.notification_interval,
        );
        let mood = self.agent_mood(focused);
        self.identity.begin(
            ctx,
            &self.preferences.identity,
            self.preferences.reduce_motion,
            // The background only runs where it can be seen: Home (also when every
            // file floats in Clean Code), the Agents area, or behind code if enabled.
            self.hub_open
                || self.documents.is_empty()
                || self.active.is_none()
                || self.preferences.identity.behind_code,
            mood,
        );
        if (ctx.zoom_factor() - self.preferences.ui_scale).abs() > 0.001 {
            ctx.set_zoom_factor(self.preferences.ui_scale);
        }
        self.update_presence();
        self.tick_guided();
        if ctx.input(|input| input.viewport().close_requested()) && !self.force_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request(Pending::Exit, ctx);
        }

        if self.pending.is_none()
            && self.palette.is_none()
            && self.pending_delete.is_none()
            && !self.conversation_exit_warning
        {
            self.shortcuts(ctx);
        }
        self.begin_navigation(ctx);
        // The gaps around the rounded islands are not covered by any panel; fill them with
        // the outer chrome colour so the window clear colour never shows as black seams.
        ctx.layer_painter(egui::LayerId::background()).rect_filled(
            ctx.content_rect(),
            0,
            theme::colors().canvas,
        );
        self.top_bar(ctx);
        self.status_bar(ctx);
        if self.studio_open && !self.hub_open {
            self.activity_rail(ctx);
            egui::CentralPanel::default()
                .frame(theme::centre_island(egui::Margin::same(0)).fill(theme::colors().canvas))
                .show(ctx, |ui| self.library_studio(ui));
        } else if self.hub_open {
            egui::CentralPanel::default()
                .frame(theme::centre_island(egui::Margin::same(0)).fill(theme::colors().canvas))
                .show(ctx, |ui| self.agent_hub(ui));
        } else {
            let focus = self.clean.focus;
            if self.preferences.tools_open && !focus {
                self.tools_panel(ctx);
            }
            self.activity_rail(ctx);
            if !focus {
                // Without a project the welcome view takes the whole work area;
                // the panel returns when a folder opens or the user asks for it.
                if self.sidebar_visible() {
                    self.sidebar(ctx);
                }
                self.assistant_panel(ctx);
            }
            self.preview_panel(ctx);
            egui::CentralPanel::default()
                .frame(theme::centre_island(egui::Margin::same(0)))
                .show(ctx, |ui| self.editor_panel(ui));
        }
        if !self.hub_open && !self.studio_open {
            self.floating_editors(ctx);
        }
        if !self.studio_open {
            self.floating_agents(ctx);
        }
        self.guided_overlay(ctx);
        self.tunnel_modal(ctx);
        self.settings_window(ctx);
        self.about_window(ctx);
        self.notifications_modal(ctx);
        self.palette_modal(ctx);
        self.modals(ctx);
        self.agent_usage_modal(ctx);
        self.conversation_close_modal(ctx);
        self.file_dialogs(ctx);
        self.companion_surface(ctx);
        self.end_navigation(ctx);
        theme::pointer_cursor(ctx);
    }
}
impl Editor {
    /// Coarse, privacy-safe activity for Discord Rich Presence. Paths and file names are
    /// never sent; the project name only with explicit opt-in.
    fn update_presence(&mut self) {
        let prefs = self.preferences.discord.clone();
        let language = self
            .active
            .and_then(|i| self.documents.get(i))
            .and_then(|d| d.path.extension())
            .and_then(|e| e.to_str())
            .map(language_name);
        let state = if !prefs.show_activity {
            None
        } else if self.studio_open {
            Some("In Library Studio".to_owned())
        } else if self.hub_open || self.chat.run.is_some() {
            Some("Coordinating agents".to_owned())
        } else if self.sidebar == Sidebar::Git && self.tools.showing_diff() {
            Some("Reviewing changes".to_owned())
        } else if let Some(language) = language.filter(|_| prefs.show_language) {
            Some(format!("Editing {language}"))
        } else if self.active.is_some() {
            Some("Editing code".to_owned())
        } else if self.root.is_some() {
            Some("Working on a project".to_owned())
        } else {
            None
        };
        let state = match (state, self.root.as_deref().filter(|_| prefs.show_project)) {
            (Some(state), Some(root)) => Some(format!("{state} · {}", workspace::filename(root))),
            (state, _) => state,
        };
        let (language_asset, language_text) = match language.filter(|_| prefs.show_language) {
            Some(lang) => (
                Some(
                    lang.to_ascii_lowercase()
                        .replace('#', "sharp")
                        .replace("++", "pp"),
                ),
                Some(lang.to_owned()),
            ),
            None => (None, None),
        };
        let activity = discord::Activity {
            details: "Covalos · In Beta".to_owned(),
            state,
            started: prefs.show_elapsed.then_some(self.session_started),
            language_asset,
            language_text,
        };
        self.discord.update(&prefs, activity);
    }
}

fn language_name(extension: &str) -> &'static str {
    match extension.to_ascii_lowercase().as_str() {
        "rs" => "Rust",
        "ts" | "tsx" | "mts" | "cts" => "TypeScript",
        "js" | "jsx" | "mjs" | "cjs" => "JavaScript",
        "py" => "Python",
        "go" => "Go",
        "java" => "Java",
        "kt" | "kts" => "Kotlin",
        "cs" => "C#",
        "c" | "h" => "C",
        "cpp" | "cc" | "hpp" => "C++",
        "swift" => "Swift",
        "rb" => "Ruby",
        "php" => "PHP",
        "html" | "htm" => "HTML",
        "css" | "scss" => "CSS",
        "md" => "Markdown",
        "json" => "JSON",
        "toml" => "TOML",
        "yaml" | "yml" => "YAML",
        _ => "code",
    }
}

impl eframe::App for Editor {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.render(ctx);
        self.sync_preview(ctx, frame);
        self.autosave_conversation(ctx);
        self.snapshot_workspace_state();
        if self.preferences != self.saved_preferences
            && !ctx.input(|input| input.pointer.any_down())
        {
            self.saved_preferences = self.preferences.clone();
            if let Some(path) = &self.preferences_path
                && let Err(error) = self.preferences.save(path)
            {
                self.error(format!("Could not save editor settings. {error}"));
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = self.save_conversation();
        self.persist_all_state();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unreadable_history_is_preserved_and_failed_exit_can_be_cancelled() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().canonicalize()?;
        let config = dir.path().join("settings/editor.json");
        let ctx = egui::Context::default();
        sessions::Conversation::new(
            &root,
            assistant::Provider::Codex,
            None,
            "",
            std::iter::empty(),
            false,
        )
        .save(&config)?;
        let file = std::fs::read_dir(config.parent().unwrap().join("conversations"))?
            .next()
            .unwrap()?
            .path();
        std::fs::write(&file, "{corrupted history")?;
        let mut editor = Editor::new(&ctx, Some(root), Preferences::default());
        editor.preferences_path = Some(config);
        editor.restore_conversation();
        assert!(!editor.conversation_writable);
        assert!(editor.notice.as_ref().is_some_and(|(error, _)| *error));
        editor.chat.push("You", "New message".into());
        editor.perform(Pending::Exit, &ctx);
        assert!(editor.conversation_exit_warning);
        assert!(!editor.force_close);
        assert_eq!(std::fs::read_to_string(file)?, "{corrupted history");
        Ok(())
    }

    #[test]
    fn conversations_survive_restart_switching_and_new_chat() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        let a = dir.path().join("project-a");
        let b = dir.path().join("project-b");
        std::fs::create_dir_all(&a)?;
        std::fs::create_dir_all(&b)?;
        let config = dir.path().join("settings/editor.json");
        let ctx = egui::Context::default();
        let mut first = Editor::new(&ctx, Some(a.clone()), Preferences::default());
        first.preferences_path = Some(config.clone());
        first.chat.push("You", "¿Dónde quedamos?".into());
        first.chat.session = Some("session-recovery-test".into());
        first.chat.prompt = "Continuar mañana".into();
        first.open_workspace(b);
        assert!(first.chat.messages.is_empty());
        first.chat.push("You", "Independent project".into());
        first.open_workspace(a.clone());
        assert_eq!(first.chat.messages[0].text, "¿Dónde quedamos?");
        assert_eq!(first.chat.prompt, "Continuar mañana");
        assert!(first.save_conversation());
        drop(first);
        let mut second = Editor::new(&ctx, Some(a.clone()), Preferences::default());
        second.preferences_path = Some(config.clone());
        second.restore_conversation();
        assert_eq!(
            second.chat.session.as_deref(),
            Some("session-recovery-test")
        );
        assert_eq!(second.chat.prompt, "Continuar mañana");
        assert!(second.chat.run.is_none());
        assert!(second.hub.workflow.is_none());
        assert!(!second.hub.approval_pending());
        second.reset_chat();
        assert!(second.save_conversation());
        drop(second);
        let mut third = Editor::new(&ctx, Some(a), Preferences::default());
        third.preferences_path = Some(config);
        third.restore_conversation();
        assert!(third.chat.messages.is_empty());
        assert!(third.chat.session.is_none());
        Ok(())
    }

    #[test]
    fn workspace_editing_and_find_render_in_the_normal_view() -> std::io::Result<()> {
        let folder = tempfile::tempdir()?;
        let ctx = egui::Context::default();
        let mut editor = Editor::new(&ctx, Some(folder.path().to_owned()), Preferences::default());
        assert!(editor.create_file("example.rs"));
        let document = &mut editor.documents[0];
        document.text.push_str("fn main() { }\n// Unicode: café\n");
        document.update_dirty();
        editor.save_active();
        assert!(!editor.documents[0].dirty);
        editor.show_find();
        editor.find_query = "CAFÉ".into();
        editor.find(false);
        assert!(editor.find_match.is_some());

        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1390.0, 860.0),
                )),
                ..Default::default()
            },
            |ctx| editor.render(ctx),
        );
        assert!(!output.shapes.is_empty());
        assert_eq!(
            std::fs::read_to_string(folder.path().join("example.rs"))?,
            "fn main() { }\n// Unicode: café\n"
        );
        Ok(())
    }

    #[test]
    #[ignore = "opens a native Windows test window and captures its own rendered frames"]
    #[cfg(windows)]
    fn capture_native_smoke_frames() -> Result<(), Box<dyn std::error::Error>> {
        use eframe::egui::{Event, UserData, ViewportBuilder, ViewportCommand};
        use winit::platform::windows::EventLoopBuilderExtWindows;

        struct Smoke {
            editor: Editor,
            fixture: PathBuf,
            output: PathBuf,
            sent: bool,
            frame: usize,
            deadline: std::time::Instant,
        }

        impl eframe::App for Smoke {
            fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
                use std::time::Duration;

                if self.frame >= 3 {
                    return;
                }
                assert!(
                    std::time::Instant::now() < self.deadline,
                    "native render did not report a screenshot"
                );
                let screenshot = ctx.input(|input| {
                    input.events.iter().find_map(|event| {
                        if let Event::Screenshot { image, .. } = event {
                            Some(image.clone())
                        } else {
                            None
                        }
                    })
                });
                if let Some(pixels) = screenshot {
                    let bytes = pixels
                        .pixels
                        .iter()
                        .flat_map(|pixel| pixel.to_array())
                        .collect::<Vec<_>>();
                    image::save_buffer(
                        self.output.join(format!("frame-{}.png", self.frame)),
                        &bytes,
                        pixels.size[0] as u32,
                        pixels.size[1] as u32,
                        image::ColorType::Rgba8,
                    )
                    .expect("save rendered frame");
                    self.frame += 1;
                    self.sent = false;
                    match self.frame {
                        1 => self.editor.open_file(self.fixture.join("src/main.rs")),
                        2 => self.editor.hub_open = true,
                        _ => {
                            ctx.send_viewport_cmd(ViewportCommand::Close);
                            return;
                        }
                    }
                }
                self.editor.render(ctx);
                if !self.sent {
                    self.sent = true;
                    ctx.send_viewport_cmd(ViewportCommand::Screenshot(UserData::default()));
                }
                ctx.request_repaint_after(Duration::from_millis(100));
            }
        }

        let output = std::env::var_os("COVALOS_SMOKE_DIR")
            .or_else(|| std::env::var_os("AEROS_SMOKE_DIR"))
            .map(PathBuf::from)
            .ok_or("set COVALOS_SMOKE_DIR")?;
        std::fs::create_dir_all(&output)?;
        let fixture = tempfile::tempdir()?;
        std::fs::create_dir(fixture.path().join("src"))?;
        std::fs::write(
            fixture.path().join("src/main.rs"),
            "fn main() {\n    println!(\"Hello from COVALOS\");\n}\n",
        )?;
        std::fs::write(
            fixture.path().join("Cargo.toml"),
            "[package]\nname = \"sample-workspace\"\n",
        )?;
        let root = fixture.path().to_owned();
        let options = eframe::NativeOptions {
            viewport: ViewportBuilder::default()
                .with_inner_size([1040.0, 690.0])
                .with_title("COVALOS render test"),
            event_loop_builder: Some(Box::new(|builder| {
                builder.with_any_thread(true);
            })),
            ..Default::default()
        };
        eframe::run_native(
            "COVALOS render test",
            options,
            Box::new(move |cc| {
                let editor = Editor::new(&cc.egui_ctx, Some(root.clone()), Preferences::default());
                Ok(Box::new(Smoke {
                    editor,
                    fixture: root,
                    output,
                    sent: false,
                    frame: 0,
                    deadline: std::time::Instant::now() + std::time::Duration::from_secs(12),
                }))
            }),
        )?;
        Ok(())
    }

    #[test]
    fn compact_views_dirty_close_and_workspace_switch_preserve_edits() -> std::io::Result<()> {
        let first = tempfile::tempdir()?;
        let second = tempfile::tempdir()?;
        let ctx = egui::Context::default();
        let mut editor = Editor::new(&ctx, Some(first.path().to_owned()), Preferences::default());
        assert!(editor.create_file("first.rs"));
        assert!(editor.create_file("second.rs"));
        editor.documents[0].text = "let n = 1;".into();
        editor.documents[0].update_dirty();
        editor.request(Pending::CloseTab(0), &ctx);
        assert!(editor.pending.is_some());
        assert_eq!(editor.documents.len(), 2);
        editor.pending = None;
        editor.request(Pending::OpenWorkspace(second.path().to_owned()), &ctx);
        assert!(editor.pending.is_some());
        assert!(
            editor
                .root
                .as_ref()
                .is_some_and(|root| root == &first.path().canonicalize().unwrap())
        );
        editor.pending = None;
        editor.save_all();
        editor.request(Pending::CloseTab(0), &ctx);
        assert_eq!(editor.active, Some(0));
        assert_eq!(workspace::filename(&editor.documents[0].path), "second.rs");

        for (size, scale) in [
            ([920.0, 600.0], 1.0),
            ([1390.0, 860.0], 1.25),
            ([1920.0, 1080.0], 1.5),
        ] {
            ctx.set_pixels_per_point(scale);
            for hub in [false, true] {
                editor.hub_open = hub;
                editor.preferences.tools_open = true;
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(size[0], size[1]),
                        )),
                        ..Default::default()
                    },
                    |ctx| editor.render(ctx),
                );
                assert!(!output.shapes.is_empty());
                assert!(
                    output
                        .shapes
                        .iter()
                        .all(|shape| shape.clip_rect.is_finite())
                );
            }
        }
        editor.open_palette(&ctx);
        let output = ctx.run(egui::RawInput::default(), |ctx| editor.render(ctx));
        assert!(!output.shapes.is_empty());
        Ok(())
    }
}

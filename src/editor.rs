mod files;
mod hub;
mod panels;
mod preferences;
mod shell;
mod skills;
mod theme;
mod tools;

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
    hub_open: bool,
    tools: tools::Tools,
    palette: Option<shell::Palette>,
    chat: hub::Chat,
    hub: hub::Hub,
    skills: skills::Skills,
    include_active_file: bool,
    include_project_note: bool,
    notice: Option<(bool, String)>,
    pending: Option<Pending>,
    force_close: bool,
    new_file: Option<String>,
    file_change: Option<(files::FileChange, String)>,
    pending_delete: Option<PathBuf>,
    focus_new_file: bool,
    syntax: egui_extras::syntax_highlighting::CodeTheme,
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
        if let Some(error) = error {
            editor.error(format!("Could not read settings. {error}"));
        }
        editor
    }

    fn new(ctx: &egui::Context, root: Option<PathBuf>, preferences: Preferences) -> Self {
        theme::install(ctx, preferences.font_size, preferences.high_contrast);
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
            hub_open: false,
            tools: tools::Tools::default(),
            palette: None,
            chat: hub::Chat::default(),
            hub: hub::Hub::default(),
            skills: skills::Skills::default(),
            include_active_file: false,
            include_project_note: false,
            notice: None,
            pending: None,
            force_close: false,
            new_file: None,
            file_change: None,
            pending_delete: None,
            focus_new_file: false,
            syntax: egui_extras::syntax_highlighting::CodeTheme::from_style(&ctx.style()),
        };
        if let Some(root) = root {
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
            Ok(_) => return self.error("Choose a directory to open as a workspace."),
            Err(error) => return self.error(format!("Could not open workspace. {error}")),
        };
        if self.root.as_ref() == Some(&root) {
            return;
        }
        self.preferences.remember(&root);
        self.reset_chat();
        self.hub.change_workspace(
            self.preferences.note(&root).to_owned(),
            self.preferences.tasks(&root),
        );
        self.include_project_note = false;
        self.root = Some(root);
        self.hub_open = false;
        self.tools.workspace_changed();
        self.directories.clear();
        self.documents.clear();
        self.active = None;
        self.sidebar = Sidebar::Files;
        self.workspace_query.clear();
        self.search_hits.clear();
        self.search_job = None;
        self.find_open = false;
        self.notice = None;
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
        let Some(document) = self.active.and_then(|index| self.documents.get_mut(index)) else {
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
                self.cursor = 0;
                self.selection = None;
                self.find_match = None;
                self.find_empty = false;
                self.focus_editor = self.active.is_some();
            }
            Pending::Exit => {
                self.force_close = true;
                self.chat.run.take();
                self.hub.general_chat.run.take();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

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
        self.notice = Some((true, message));
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

    fn reset_chat(&mut self) {
        self.chat = hub::Chat::default();
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
    fn render(&mut self, ctx: &egui::Context) {
        self.poll();
        self.poll_chats(ctx);
        self.tools.poll();
        if ctx.input(|input| input.viewport().close_requested()) && !self.force_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request(Pending::Exit, ctx);
        }

        if self.pending.is_none()
            && self.new_file.is_none()
            && self.palette.is_none()
            && self.file_change.is_none()
            && self.pending_delete.is_none()
        {
            self.shortcuts(ctx);
        }
        self.top_bar(ctx);
        self.status_bar(ctx);
        if self.hub_open {
            egui::CentralPanel::default().show(ctx, |ui| self.agent_hub(ui));
        } else {
            if self.preferences.tools_open {
                self.tools_panel(ctx);
            }
            self.activity_rail(ctx);
            self.sidebar(ctx);
            if self.preferences.assistant_open {
                self.assistant_panel(ctx);
            }
            egui::CentralPanel::default().show(ctx, |ui| self.editor_panel(ui));
        }
        self.settings_window(ctx);
        self.palette_modal(ctx);
        self.modals(ctx);
        self.file_dialogs(ctx);
    }
}
impl eframe::App for Editor {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.render(ctx);
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
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let output = std::env::var_os("AEROS_SMOKE_DIR")
            .map(PathBuf::from)
            .ok_or("set AEROS_SMOKE_DIR")?;
        std::fs::create_dir_all(&output)?;
        let fixture = tempfile::tempdir()?;
        std::fs::create_dir(fixture.path().join("src"))?;
        std::fs::write(
            fixture.path().join("src/main.rs"),
            "fn main() {\n    println!(\"Hello from AEROS\");\n}\n",
        )?;
        std::fs::write(
            fixture.path().join("Cargo.toml"),
            "[package]\nname = \"sample-workspace\"\n",
        )?;
        let root = fixture.path().to_owned();
        let options = eframe::NativeOptions {
            viewport: ViewportBuilder::default()
                .with_inner_size([1040.0, 690.0])
                .with_title("AEROS render test"),
            event_loop_builder: Some(Box::new(|builder| {
                builder.with_any_thread(true);
            })),
            ..Default::default()
        };
        eframe::run_native(
            "AEROS render test",
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

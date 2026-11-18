mod panels;

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
}

enum Pending {
    OpenWorkspace(PathBuf),
    CloseTab(usize),
    Exit,
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

struct ChatMessage {
    author: &'static str,
    text: String,
}

pub struct Editor {
    root: Option<PathBuf>,
    directories: HashMap<PathBuf, Result<Vec<Entry>, String>>,
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
    ai_open: bool,
    prompt: String,
    include_active_file: bool,
    messages: Vec<ChatMessage>,
    session: Option<String>,
    run: Option<assistant::Run>,
    activity: &'static str,
    notice: Option<(bool, String)>,
    pending: Option<Pending>,
    force_close: bool,
    new_file: Option<String>,
    focus_new_file: bool,
    syntax: egui_extras::syntax_highlighting::CodeTheme,
}

impl Editor {
    pub fn new(ctx: &egui::Context, root: Option<PathBuf>) -> Self {
        panels::install_theme(ctx);
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
            ai_open: true,
            prompt: String::new(),
            include_active_file: false,
            messages: Vec::new(),
            session: None,
            run: None,
            activity: "",
            notice: None,
            pending: None,
            force_close: false,
            new_file: None,
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
        self.root = Some(root);
        self.directories.clear();
        self.documents.clear();
        self.active = None;
        self.sidebar = Sidebar::Files;
        self.workspace_query.clear();
        self.search_hits.clear();
        self.search_job = None;
        self.find_open = false;
        self.reset_chat();
        self.prompt.clear();
        self.notice = None;
    }

    fn open_file(&mut self, path: PathBuf) {
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
                self.run.take();
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
        self.notice = Some((true, message.into()));
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
        self.run = None;
        self.session = None;
        self.messages.clear();
        self.include_active_file = false;
        self.activity = "";
    }

    fn send_prompt(&mut self, ctx: &egui::Context) {
        let prompt = self.prompt.trim();
        let Some(root) = self.root.clone() else {
            return;
        };
        if prompt.is_empty() || self.run.is_some() {
            return;
        }
        let mut message = prompt.to_owned();
        let include = std::mem::take(&mut self.include_active_file);
        let input = if let Some(document) = self.active.map(|index| &self.documents[index]) {
            let path = document.path.strip_prefix(&root).unwrap_or(&document.path);
            if include {
                message.push_str(&format!("\n\nAttached: {}", path.display()));
            }
            assistant::with_file_context(prompt, path, include.then_some(&document.text))
        } else {
            prompt.to_owned()
        };
        self.messages.push(ChatMessage {
            author: "You",
            text: message,
        });
        self.prompt.clear();
        self.activity = "Waiting for Codex";
        let ctx = ctx.clone();
        self.run = Some(assistant::start(
            root,
            self.session.clone(),
            input,
            move || ctx.request_repaint(),
        ));
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
        if let Some(run) = &self.run {
            for event in run.events.try_iter() {
                match event {
                    assistant::Event::Session(id) => self.session = Some(id),
                    assistant::Event::Activity => self.activity = "Inspecting workspace",
                    assistant::Event::Message(text) => self.messages.push(ChatMessage {
                        author: "Codex",
                        text,
                    }),
                    assistant::Event::Error(text) => {
                        self.messages.push(ChatMessage {
                            author: "Error",
                            text,
                        });
                        self.activity = "";
                    }
                    assistant::Event::Cancelled => {
                        self.messages.push(ChatMessage {
                            author: "Codex",
                            text: "Request stopped.".into(),
                        });
                        self.activity = "";
                    }
                    assistant::Event::Done => self.activity = "",
                }
            }
            if self.activity.is_empty() {
                self.run = None;
            }
        }
    }
}

impl Editor {
    fn render(&mut self, ctx: &egui::Context) {
        self.poll();
        if ctx.input(|input| input.viewport().close_requested()) && !self.force_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request(Pending::Exit, ctx);
        }

        if self.pending.is_none() && self.new_file.is_none() {
            self.shortcuts(ctx);
        }
        self.top_bar(ctx);
        self.status_bar(ctx);
        self.sidebar(ctx);
        if self.ai_open {
            self.assistant_panel(ctx);
        }
        egui::CentralPanel::default().show(ctx, |ui| self.editor_panel(ui));
        self.modals(ctx);
    }
}
impl eframe::App for Editor {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.render(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_editing_and_find_render_in_the_normal_view() -> std::io::Result<()> {
        let folder = tempfile::tempdir()?;
        let ctx = egui::Context::default();
        let mut editor = Editor::new(&ctx, Some(folder.path().to_owned()));
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
}

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
};

use eframe::egui::{
    self, CornerRadius, Id, Key, RichText, ScrollArea, Sense, Stroke, TextEdit, Ui,
};

use super::{Editor, Pending, theme::*};
use crate::workspace;

pub(super) struct Palette {
    query: String,
    focus: bool,
    files: Vec<PathBuf>,
    results: mpsc::Receiver<Vec<PathBuf>>,
    stop: Arc<AtomicBool>,
}

impl Drop for Palette {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Palette {
    fn new(root: Option<PathBuf>, ctx: egui::Context) -> Self {
        let (sender, results) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        if let Some(root) = root {
            let stop = stop.clone();
            thread::spawn(move || {
                let files = workspace::project_files(&root, &stop);
                let _ = sender.send(files);
                ctx.request_repaint();
            });
        }
        Self {
            query: String::new(),
            focus: true,
            files: Vec::new(),
            results,
            stop,
        }
    }
}

#[derive(Clone, Copy)]
enum Action {
    OpenFolder,
    OpenFile,
    CreateProject,
    Search,
    Find,
    Save,
    Terminal,
    AgentHub,
    Settings,
}

const ACTIONS: &[(Action, &str)] = &[
    (Action::OpenFolder, "Open folder"),
    (Action::OpenFile, "Open file"),
    (Action::CreateProject, "Create project"),
    (Action::Search, "Search workspace"),
    (Action::Find, "Find in active file"),
    (Action::Save, "Save active file"),
    (Action::Terminal, "Toggle terminal"),
    (Action::AgentHub, "Toggle Agent Hub"),
    (Action::Settings, "Open settings"),
];

enum Selection {
    Action(Action),
    File(PathBuf),
    Project(PathBuf),
    Ask(String),
}

impl Editor {
    pub(super) fn open_palette(&mut self, ctx: &egui::Context) {
        self.palette = Some(Palette::new(self.root.clone(), ctx.clone()));
    }

    pub(super) fn workspace_picker(&mut self, ui: &mut Ui) {
        let label = self
            .root
            .as_deref()
            .map(workspace::filename)
            .unwrap_or_else(|| "Projects".into());
        ui.menu_button(RichText::new(label).color(MUTED), |ui| {
            if ui.button("Open folder…").clicked() {
                ui.close();
                self.pick_workspace(ui.ctx());
            }
            if ui.button("Create project…").clicked() {
                ui.close();
                self.pick_new_project(ui.ctx());
            }
            if !self.preferences.recent.is_empty() {
                ui.separator();
                for root in self.preferences.recent.clone() {
                    if ui
                        .button(workspace::filename(&root))
                        .on_hover_text(root.display().to_string())
                        .clicked()
                    {
                        ui.close();
                        self.request(Pending::OpenWorkspace(root), ui.ctx());
                    }
                }
            }
        });
    }

    pub(super) fn welcome(&mut self, ui: &mut Ui) {
        let area = ui.available_rect_before_wrap();
        let centered_width = (area.width() - 36.0).clamp(180.0, 520.0);
        let pad = ((area.width() - centered_width) / 2.0).max(0.0);
        let side = area.left() + pad;
        let height = area.height();
        let top = area.top() + (height * 0.12).clamp(10.0, 78.0);
        let content = egui::Rect::from_min_max(
            egui::pos2(side, top),
            egui::pos2(side + centered_width, area.bottom()),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(content), |ui| {
            ui.set_width(centered_width);
            ScrollArea::vertical().id_salt("welcome").show(ui, |ui| {
                constellation(ui, centered_width, (height * 0.24).clamp(74.0, 132.0));
                ui.add_space(12.0);
                let title = self
                    .root
                    .as_deref()
                    .map(workspace::filename)
                    .unwrap_or_else(|| TITLE.into());
                ui.label(RichText::new(title).size(32.0).strong().color(TEXT));
                ui.label(muted(if self.root.is_some() {
                    "Open a file from the Explorer or search the project."
                } else {
                    "Open a local project, edit code, or work with Codex."
                }));
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.add(primary("Open folder")).clicked() {
                        self.pick_workspace(ui.ctx());
                    }
                    if ui.button("Open file").clicked() {
                        self.pick_file();
                    }
                    if ui.button("Create project").clicked() {
                        self.pick_new_project(ui.ctx());
                    }
                    if ui.button("Agent Hub").clicked() {
                        self.hub_open = true;
                    }
                });
                if !self.preferences.recent.is_empty() {
                    ui.add_space(12.0);
                    card().show(ui, |ui| {
                        ui.set_min_width((centered_width - 30.0).max(1.0));
                        ui.label(RichText::new("Recent projects").strong());
                        for root in self.preferences.recent.clone() {
                            if ui
                                .add(egui::Button::new(workspace::filename(&root)).frame(false))
                                .on_hover_text(root.display().to_string())
                                .clicked()
                            {
                                self.request(Pending::OpenWorkspace(root), ui.ctx());
                            }
                        }
                    });
                }
            });
        });
    }

    pub(super) fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        egui::Window::new("Settings")
            .open(&mut self.settings_open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(RichText::new("Appearance").strong());
                let font_changed = ui
                    .add(
                        egui::Slider::new(&mut self.preferences.font_size, 11.0..=26.0)
                            .step_by(1.0)
                            .text("Editor font size"),
                    )
                    .changed();
                let contrast_changed = ui
                    .checkbox(
                        &mut self.preferences.high_contrast,
                        "High contrast controls",
                    )
                    .changed();
                if font_changed || contrast_changed {
                    install(
                        ctx,
                        self.preferences.font_size,
                        self.preferences.high_contrast,
                    );
                }
                ui.label(muted(THEME_NAME));
                ui.separator();
                ui.checkbox(&mut self.preferences.allow_auto_buffer, "Allow opt-in Auto buffer edits")
                    .on_hover_text("Auto can apply one proposed replacement in the active editor buffer. Saving remains manual.");
            });
    }

    pub(super) fn palette_modal(&mut self, ctx: &egui::Context) {
        let Some(mut palette) = self.palette.take() else {
            return;
        };
        if let Ok(files) = palette.results.try_recv() {
            palette.files = files;
        }
        let modal = egui::Modal::new(Id::new("command-palette")).show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() * 0.56).clamp(280.0, 660.0));
            ui.heading("Quick access");
            let input = ui.add(
                TextEdit::singleline(&mut palette.query)
                    .hint_text("Open a file   ·   > command   ·   @ project   ·   ? ask Codex")
                    .desired_width(f32::INFINITY)
                    .char_limit(12_000),
            );
            if std::mem::take(&mut palette.focus) {
                input.request_focus();
            }
            let query = palette.query.trim();
            let mut choices = Vec::new();
            if let Some(prompt) = query.strip_prefix('?') {
                if self.root.is_some() && !prompt.trim().is_empty() {
                    choices.push((
                        Selection::Ask(prompt.trim().to_owned()),
                        "Ask Codex in this workspace".to_owned(),
                    ));
                }
            } else if let Some(filter) = query.strip_prefix('@') {
                let filter = filter.trim().to_lowercase();
                choices.extend(self.preferences.recent.iter().filter_map(|root| {
                    let title = workspace::filename(root);
                    title
                        .to_lowercase()
                        .contains(&filter)
                        .then(|| (Selection::Project(root.clone()), title))
                }));
            } else if query.is_empty() || query.starts_with('>') {
                let filter = query.trim_start_matches('>').trim().to_lowercase();
                choices.extend(
                    ACTIONS
                        .iter()
                        .filter(|(_, label)| label.to_lowercase().contains(&filter))
                        .map(|(action, label)| (Selection::Action(*action), (*label).to_owned())),
                );
            } else if let Some(root) = &self.root {
                let filter = query.to_lowercase().replace('\\', "/");
                choices.extend(
                    palette
                        .files
                        .iter()
                        .filter_map(|file| {
                            let label = file
                                .strip_prefix(root)
                                .unwrap_or(file)
                                .display()
                                .to_string();
                            label
                                .to_lowercase()
                                .replace('\\', "/")
                                .contains(&filter)
                                .then(|| (Selection::File(file.clone()), label))
                        })
                        .take(30),
                );
            }
            if input.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) && !choices.is_empty()
            {
                return Some(choices.remove(0).0);
            }
            ScrollArea::vertical()
                .max_height(300.0)
                .show(ui, |ui| {
                    if choices.is_empty() {
                        ui.label(muted("No matching items."));
                    }
                    for (choice, label) in choices {
                        if ui.button(label).clicked() {
                            return Some(choice);
                        }
                    }
                    None
                })
                .inner
        });
        match modal.inner {
            Some(Selection::Action(action)) => match action {
                Action::OpenFolder => self.pick_workspace(ctx),
                Action::OpenFile => self.pick_file(),
                Action::CreateProject => self.pick_new_project(ctx),
                Action::Search => self.show_workspace_search(),
                Action::Find => self.show_find(),
                Action::Save => self.save_active(),
                Action::Terminal => self.preferences.tools_open = !self.preferences.tools_open,
                Action::AgentHub => self.hub_open = !self.hub_open,
                Action::Settings => self.settings_open = true,
            },
            Some(Selection::File(file)) => self.open_file(file),
            Some(Selection::Project(root)) => self.request(Pending::OpenWorkspace(root), ctx),
            Some(Selection::Ask(prompt)) => {
                self.preferences.assistant_open = true;
                self.chat.prompt = prompt;
                self.hub_open = false;
                self.send_prompt(ctx);
            }
            None if !modal.should_close() => self.palette = Some(palette),
            None => {}
        }
    }
}

fn constellation(ui: &mut Ui, width: f32, height: f32) {
    let (rect, _) = ui.allocate_at_least(egui::vec2(width, height), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect(
        rect,
        CornerRadius::same(PANEL_RADIUS),
        INSET,
        Stroke::new(1.0_f32, BORDER),
        egui::StrokeKind::Inside,
    );
    for column in 1..12 {
        let x = rect.left() + rect.width() * column as f32 / 12.0;
        painter.vline(
            x,
            rect.y_range().shrink(1.0),
            Stroke::new(1.0_f32, BORDER.gamma_multiply(0.44)),
        );
    }
    for row in 1..5 {
        let y = rect.top() + rect.height() * row as f32 / 5.0;
        painter.hline(
            rect.x_range().shrink(1.0),
            y,
            Stroke::new(1.0_f32, BORDER.gamma_multiply(0.44)),
        );
    }
    let center = rect.center();
    let radius = height * 0.25;
    for ring in 0..3 {
        let size = radius * (1.45 + ring as f32 * 0.39);
        let points = (0..=80)
            .map(|i| {
                let angle = i as f32 / 80.0 * std::f32::consts::TAU;
                let x = angle.cos() * size * 2.0;
                let y = angle.sin() * size * 0.38;
                center + egui::vec2(x * 0.96 + y * 0.27, -x * 0.27 + y * 0.96)
            })
            .collect();
        painter.add(egui::Shape::line(
            points,
            Stroke::new(1.0_f32, ACCENT.gamma_multiply(0.34 - ring as f32 * 0.08)),
        ));
    }
    painter.circle_filled(center, radius, SURFACE);
    painter.circle_stroke(
        center,
        radius,
        Stroke::new(1.0_f32, ACCENT.gamma_multiply(0.68)),
    );
    for i in 0..49 {
        let x = rect.left() + 12.0 + (rect.width() - 24.0) * ((i * 67 + 19) % 101) as f32 / 101.0;
        let y = rect.top() + 8.0 + (rect.height() - 16.0) * ((i * 43 + 11) % 103) as f32 / 103.0;
        let point = egui::pos2(x, y);
        if point.distance(center) < radius + 4.0 {
            continue;
        }
        if i % 8 == 0 {
            painter.line_segment(
                [point - egui::vec2(3.0, 0.0), point + egui::vec2(3.0, 0.0)],
                Stroke::new(1.0_f32, ACCENT),
            );
            painter.line_segment(
                [point - egui::vec2(0.0, 3.0), point + egui::vec2(0.0, 3.0)],
                Stroke::new(1.0_f32, ACCENT),
            );
        } else {
            painter.circle_filled(point, 1.0, MUTED.gamma_multiply(0.5));
        }
    }
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        BRAND.chars().next().unwrap_or_default().to_string(),
        egui::FontId::monospace(24.0),
        ACCENT_HOVER,
    );
}

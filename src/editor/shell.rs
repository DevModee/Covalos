use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
};

use eframe::egui::{self, Id, Key, RichText, ScrollArea, Sense, Stroke, TextEdit, Ui};

use super::{Editor, Pending, theme::*};
use crate::workspace;

pub(super) struct Palette {
    query: String,
    focus: bool,
    selected: usize,
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
            selected: 0,
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
        ui.menu_button(RichText::new(label).color(colors().muted), |ui| {
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
        let width = (area.width() - 64.0).clamp(240.0, 640.0).min(area.width());
        let top = if area.height() < 560.0 {
            24.0
        } else {
            (area.height() * 0.16).clamp(24.0, 120.0)
        };
        let rect = egui::Rect::from_min_size(
            egui::pos2(area.center().x - width * 0.5, area.top() + top),
            egui::vec2(width, (area.height() - top).max(1.0)),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            ui.set_width(width);
            ScrollArea::vertical().id_salt("welcome").show(ui, |ui| {
                let title = self
                    .root
                    .as_deref()
                    .map(workspace::filename)
                    .unwrap_or_else(|| format!("Welcome to {BRAND}"));
                ui.horizontal(|ui| {
                    let text_width = if width > 400.0 { width - 132.0 } else { width };
                    ui.allocate_ui_with_layout(
                        egui::vec2(text_width, 112.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.add_space(12.0);
                            ui.add(
                                egui::Label::new(
                                    RichText::new(title)
                                        .size(if width > 480.0 { DISPLAY } else { 28.0 })
                                        .strong(),
                                )
                                .wrap(),
                            );
                            ui.add_space(8.0);
                            ui.add(
                                egui::Label::new(secondary(if self.root.is_some() {
                                    "Find a file or explore your workspace."
                                } else {
                                    "Open a project or continue your work."
                                }))
                                .wrap(),
                            );
                        },
                    );
                    if width > 400.0 {
                        constellation(ui, 112.0, 104.0);
                    }
                });
                ui.add_space(24.0);
                ui.add_space(4.0);
                if row(
                    ui,
                    "Open folder",
                    if cfg!(target_os = "macos") {
                        "⌘ O"
                    } else {
                        "Ctrl O"
                    },
                    false,
                )
                .clicked()
                {
                    self.pick_workspace(ui.ctx());
                }
                if row(
                    ui,
                    "Open file",
                    if cfg!(target_os = "macos") {
                        "⇧ ⌘ O"
                    } else {
                        "Ctrl Shift O"
                    },
                    false,
                )
                .clicked()
                {
                    self.pick_file();
                }
                if row(ui, "Create project", "", false).clicked() {
                    self.pick_new_project(ui.ctx());
                }
                if row(ui, "Open Agent Hub", "", false).clicked() {
                    self.hub_open = true;
                }
                if !self.preferences.recent.is_empty() {
                    ui.add_space(28.0);
                    section(ui, "Recent projects", "");
                    for root in self.preferences.recent.clone() {
                        let response = ui.add_sized(
                            [ui.available_width(), 52.0],
                            egui::Button::new("").frame(false),
                        );
                        let painter = ui.painter_at(response.rect);
                        if response.hovered() {
                            painter.rect_filled(response.rect, CONTROL_RADIUS, colors().hover);
                        }
                        draw_icon(
                            &painter,
                            response.rect.left_center() + egui::vec2(16.0, 0.0),
                            Icon::Explorer,
                            colors().muted,
                        );
                        let text_rect = response.rect.shrink2(egui::vec2(40.0, 4.0));
                        let painter = painter.with_clip_rect(text_rect);
                        painter.text(
                            text_rect.left_top() + egui::vec2(0.0, 5.0),
                            egui::Align2::LEFT_TOP,
                            workspace::filename(&root),
                            egui::FontId::proportional(BODY),
                            colors().text,
                        );
                        let displayed = root.display().to_string();
                        let displayed = displayed.strip_prefix(r"\\?\").unwrap_or(&displayed);
                        painter.text(
                            text_rect.left_top() + egui::vec2(0.0, 26.0),
                            egui::Align2::LEFT_TOP,
                            displayed,
                            egui::FontId::proportional(SMALL),
                            colors().muted,
                        );
                        if response.clicked() {
                            self.request(Pending::OpenWorkspace(root.clone()), ui.ctx());
                        }
                        response.on_hover_text(root.display().to_string());
                    }
                }
            });
        });
    }

    pub(super) fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        egui::Window::new("Settings").open(&mut self.settings_open)
            .collapsible(false).resizable(false).fixed_size(egui::vec2(440.0,(ctx.content_rect().height()-140.0).clamp(260.0,440.0)))
            .vscroll(true)
            .constrain_to(ctx.content_rect().shrink(16.0))
            .show(ctx,|ui| {
                ui.set_max_width(440.0);
                let previous_theme = self.preferences.theme;
                section(ui,"Appearance","Tune the workspace for long editing sessions.");
                ui.horizontal(|ui| {
                    ui.label(secondary("Theme"));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{egui::ComboBox::from_id_salt("appearance-theme").selected_text(self.preferences.theme.name()).show_ui(ui, |ui| { for theme in Theme::ALL { ui.selectable_value(&mut self.preferences.theme, theme, theme.name()); } });});
                });
                ui.add_space(12.0);
                ui.label(secondary("Editor font size"));
                let font_changed=ui.add(egui::Slider::new(&mut self.preferences.font_size,11.0..=26.0).step_by(1.0).suffix(" pt")).changed();
                ui.add_space(12.0);
                let contrast_changed=checkbox(ui,&mut self.preferences.high_contrast,"High contrast controls").changed();
                checkbox(ui,&mut self.preferences.compact,"Compact interface");
                checkbox(ui,&mut self.preferences.reduce_motion,"Reduce motion");
                ui.add_space(20.0);
                section(ui,"Agent editing","Control how proposed changes reach your editor.");
                checkbox(ui,&mut self.preferences.allow_auto_buffer,"Allow Auto buffer edits")
                    .on_hover_text("Auto applies an eligible replacement to the active buffer. Use Save to write the file.");
                if font_changed || contrast_changed || previous_theme != self.preferences.theme {
                    install(ctx,self.preferences.font_size,self.preferences.high_contrast,self.preferences.theme);
                    self.syntax = egui_extras::syntax_highlighting::CodeTheme::dark(self.preferences.font_size);
                }
                ui.add_space(8.0);
            });
    }

    pub(super) fn palette_modal(&mut self, ctx: &egui::Context) {
        let Some(mut palette) = self.palette.take() else {
            return;
        };
        if let Ok(files) = palette.results.try_recv() {
            palette.files = files;
        }
        let palette_width = (ctx.content_rect().width() * 0.60).clamp(360.0, 680.0);
        let area = egui::Modal::default_area(Id::new("command-palette")).anchor(
            egui::Align2::CENTER_TOP,
            egui::vec2(0.0, ctx.content_rect().height() * 0.15),
        );
        let modal = egui::Modal::new(Id::new("command-palette"))
            .frame(modal_frame())
            .area(area)
            .show(ctx, |ui| {
                ui.set_width(palette_width);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Quick access").strong().size(LEAD));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(muted("Esc"));
                    });
                });
                ui.add_space(12.0);
                let down = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowDown));
                let up = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowUp));
                let confirm = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter));
                let input = ui.add(
                    TextEdit::singleline(&mut palette.query)
                        .hint_text("Find a file, > command, @ project, ? Codex")
                        .margin(egui::Margin::same(10))
                        .desired_width(f32::INFINITY)
                        .char_limit(12_000),
                );
                if std::mem::take(&mut palette.focus) {
                    input.request_focus();
                }
                if input.changed() {
                    palette.selected = 0;
                }
                ui.add_space(8.0);
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
                            .map(|(action, label)| {
                                (Selection::Action(*action), (*label).to_owned())
                            }),
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
                if !choices.is_empty() {
                    if down {
                        palette.selected = (palette.selected + 1) % choices.len();
                    }
                    if up {
                        palette.selected = (palette.selected + choices.len() - 1) % choices.len();
                    }
                    palette.selected = palette.selected.min(choices.len() - 1);
                    if confirm {
                        return Some(choices.remove(palette.selected).0);
                    }
                }
                let selected = ScrollArea::vertical()
                    .id_salt("palette-results")
                    .max_height((ctx.content_rect().height() * 0.48).min(360.0))
                    .show(ui, |ui| {
                        if choices.is_empty() {
                            ui.add_space(16.0);
                            ui.label(secondary("No matching items."));
                            ui.add_space(16.0);
                        }
                        for (index, (choice, label)) in choices.into_iter().enumerate() {
                            let hint = match &choice {
                                Selection::File(_) => "File",
                                Selection::Project(_) => "Project",
                                Selection::Ask(_) => "Codex",
                                Selection::Action(Action::OpenFolder) => "Ctrl O",
                                Selection::Action(Action::OpenFile) => "Ctrl Shift O",
                                Selection::Action(Action::Save) => "Ctrl S",
                                Selection::Action(_) => "Command",
                            };
                            let response = row(ui, &label, hint, index == palette.selected);
                            if (up || down) && index == palette.selected {
                                response.scroll_to_me(None);
                            }
                            if response.clicked() {
                                return Some(choice);
                            }
                        }
                        None
                    })
                    .inner;
                ui.add_space(8.0);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(muted("↑ ↓  Navigate"));
                    ui.label(muted("Enter  Open"));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(muted(">  Commands"));
                    });
                });
                selected
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
    let center = rect.center();
    for ring in 0..2 {
        let points = (0..=64)
            .map(|i| {
                let a = i as f32 / 64.0 * std::f32::consts::TAU;
                let x = a.cos() * (42.0 + ring as f32 * 9.0);
                let y = a.sin() * (16.0 + ring as f32 * 4.0);
                center + egui::vec2(x * 0.9 + y * 0.43, -x * 0.43 + y * 0.9)
            })
            .collect();
        painter.add(egui::Shape::line(
            points,
            Stroke::new(1.0_f32, colors().accent.gamma_multiply(0.28)),
        ));
    }
    painter.circle_filled(center, 19.0, colors().canvas);
    draw_icon(&painter, center, Icon::Brand, colors().accent);
    painter.circle_filled(center + egui::vec2(43.0, -16.0), 2.5, colors().accent);
}

#[cfg(test)]
mod tests {
    use super::super::preferences::Preferences;
    use super::*;

    #[test]
    fn command_palette_arrows_choose_the_selected_command() {
        let ctx = egui::Context::default();
        let mut editor = Editor::new(&ctx, None, Preferences::default());
        editor.open_palette(&ctx);
        editor.palette.as_mut().unwrap().query = "> toggle".into();
        let input = |events| egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1366.0, 768.0),
            )),
            events,
            ..Default::default()
        };
        let press = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let _ = ctx.run(input(vec![]), |ctx| editor.render(ctx));
        let _ = ctx.run(input(vec![press(Key::ArrowDown)]), |ctx| editor.render(ctx));
        let _ = ctx.run(input(vec![press(Key::Enter)]), |ctx| editor.render(ctx));
        assert!(editor.palette.is_none());
        assert!(editor.hub_open);
        assert!(!editor.preferences.tools_open);
    }
}

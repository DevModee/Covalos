use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
};

use eframe::egui::{self, Id, Key, RichText, ScrollArea, TextEdit, Ui};

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
            if ui.button(tr("Open folder…")).clicked() {
                ui.close();
                self.pick_workspace(ui.ctx());
            }
            if ui.button(tr("Create project…")).clicked() {
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
        enter(ui, Id::new("workspace-home-entry"));
        let area = ui.available_rect_before_wrap();
        let mood = self.agent_mood(ui.input(|i| i.focused));
        self.identity
            .paint(ui, area, &self.preferences.identity, mood);
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
        // Reading scrim: the background stays alive at the edges and calms behind content.
        super::mascot::soft_ellipse(
            ui.painter(),
            egui::pos2(rect.center().x, area.top() + top + 190.0),
            egui::vec2(width * 0.85, 300.0),
            colors().canvas.gamma_multiply(0.92),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            ui.set_width(width);
            ScrollArea::vertical().id_salt("welcome").show(ui, |ui| {
                let title = self
                    .root
                    .as_deref()
                    .map(workspace::filename)
                    .unwrap_or_else(|| tr("Welcome to AEROS").to_owned());
                ui.horizontal(|ui| {
                    let text_width = width;
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
                });
                ui.add_space(24.0);
                let mac = cfg!(target_os = "macos");
                let cards = [
                    (
                        Icon::Explorer,
                        "Open folder",
                        if mac { "⌘ O" } else { "Ctrl O" },
                    ),
                    (
                        Icon::Code,
                        "Open file",
                        if mac { "⇧ ⌘ O" } else { "Ctrl Shift O" },
                    ),
                    (Icon::Add, "Create project", "New folder"),
                    (Icon::Hub, "Agent Hub", "Codex · Claude"),
                ];
                let columns = if width > 440.0 { 2 } else { 1 };
                let gap = 10.0;
                let card_w = (width - gap * (columns as f32 - 1.0)) / columns as f32;
                let mut clicked = None;
                for chunk in cards.chunks(columns) {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = gap;
                        for &(icon, title, detail) in chunk {
                            if home_card(ui, card_w, icon, title, detail).clicked() {
                                clicked = Some(title);
                            }
                        }
                    });
                    ui.add_space(gap - 4.0);
                }
                match clicked {
                    Some("Open folder") => self.pick_workspace(ui.ctx()),
                    Some("Open file") => self.pick_file(),
                    Some("Create project") => self.pick_new_project(ui.ctx()),
                    Some(_) => self.hub_open = true,
                    None => {}
                }
                if !self.preferences.recent.is_empty() {
                    ui.add_space(28.0);
                    section(ui, tr("Recent projects"), "");
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
        egui::Window::new(tr("Settings")).open(&mut self.settings_open)
            .collapsible(false).resizable(false)
            .fixed_size(egui::vec2(470.0,(ctx.content_rect().height()-140.0).clamp(240.0,560.0)))
            .vscroll(true).constrain_to(ctx.content_rect().shrink(16.0))
            .show(ctx, |ui| {
                ui.set_max_width(470.0);
                let old = (self.preferences.theme,self.preferences.font_size,self.preferences.high_contrast);
                section(ui, tr("Appearance"), tr("Tune the workspace for long editing sessions."));
                ui.horizontal(|ui| {
                    ui.label(secondary(tr("Theme")));
                    egui::ComboBox::from_id_salt("appearance-theme").selected_text(tr(self.preferences.theme.name()))
                        .show_ui(ui, |ui| {for theme in Theme::ALL {ui.selectable_value(&mut self.preferences.theme,theme,tr(theme.name()));}});
                });
                ui.horizontal(|ui| {
                    ui.label(secondary(tr("Language")));
                    let response = egui::ComboBox::from_id_salt("appearance-language")
                        .selected_text(self.preferences.language.native_name())
                        .show_ui(ui, |ui| {
                            for language in super::i18n::Language::ALL {
                                let option = ui.selectable_value(
                                    &mut self.preferences.language,
                                    language,
                                    language.native_name(),
                                );
                                #[cfg(test)]
                                super::motion::record(ui, format!("Language {}", language.native_name()), option.rect);
                                let _ = option;
                            }
                        });
                    #[cfg(test)]
                    super::motion::record(ui, "Language selector", response.response.rect);
                    let _ = response;
                });
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| { for theme in Theme::ALL {
                    let selected=self.preferences.theme==theme;
                    let response=ui.add_sized([72.0,42.0],egui::Button::new("").fill(theme.colors().canvas).stroke(egui::Stroke::new(if selected{2.0_f32}else{1.0_f32},if selected{colors().accent}else{colors().border})));
                    let p=ui.painter_at(response.rect);p.rect_filled(egui::Rect::from_min_size(response.rect.min+egui::vec2(8.0,9.0),egui::vec2(8.0,23.0)),2,theme.colors().surface);for (y,w,c) in [(10.0,34.0,theme.colors().accent),(17.0,42.0,theme.colors().secondary),(24.0,28.0,theme.colors().syntax_string)]{p.line_segment([response.rect.min+egui::vec2(22.0,y),response.rect.min+egui::vec2(22.0+w,y)],egui::Stroke::new(2.0_f32,c));}
                    #[cfg(test)] super::motion::record(ui,format!("Theme {}",theme.name()),response.rect);
                    if response.on_hover_text(theme.name()).clicked(){self.preferences.theme=theme;}
                }});
                ui.add_space(8.0);
                ui.add(egui::Slider::new(&mut self.preferences.font_size,11.0..=26.0).step_by(1.0).text(tr("Editor font size")).suffix(" pt"));
                checkbox(ui,&mut self.preferences.high_contrast,tr("High contrast controls"));
                checkbox(ui,&mut self.preferences.compact,tr("Compact interface"));
                checkbox(ui,&mut self.preferences.reduce_motion,tr("Reduce motion"));
                if old != (self.preferences.theme,self.preferences.font_size,self.preferences.high_contrast) {
                    install(ctx,self.preferences.font_size,self.preferences.high_contrast,self.preferences.theme);
                    self.syntax=egui_extras::syntax_highlighting::CodeTheme::dark(self.preferences.font_size);
                }
                ui.add_space(16.0);
                egui::CollapsingHeader::new(tr("Background & ASCII")).default_open(true).show(ui, |ui| {
                    let prefs=&mut self.preferences.identity;
                    ui.horizontal(|ui| {
                        ui.label(secondary(tr("Preset")));
                        let selector=egui::ComboBox::from_id_salt("background-preset").selected_text(prefs.background.name()).show_ui(ui, |ui| {
                            for preset in super::identity::Background::ALL {let response=ui.selectable_value(&mut prefs.background,preset,tr(preset.name())); #[cfg(test)] super::motion::record(ui,format!("Background {}",preset.name()),response.rect); let _=response;}
                        });
                        #[cfg(test)] super::motion::record(ui,"Background selector",selector.response.rect);
                        let _=selector;
                    });
                    let (_,preview)=ui.allocate_space(egui::vec2(ui.available_width(),100.0));
                    ui.painter().rect_filled(preview,6,colors().canvas);
                    self.identity.paint(ui,preview,prefs,super::identity::Mood::Idle);
                    checkbox(ui,&mut prefs.animated,tr("Animate backgrounds"));
                    checkbox(ui,&mut prefs.interactive,tr("Pointer reactions"));
                    ui.add(egui::Slider::new(&mut prefs.fps,5..=60).text(tr("Ambient FPS")).suffix(" fps"));
                    ui.add(egui::Slider::new(&mut prefs.speed,0.1..=2.0).text(tr("Speed")));
                    ui.add(egui::Slider::new(&mut prefs.density,0.1..=1.0).text(tr("Density")));
                    ui.add(egui::Slider::new(&mut prefs.intensity,0.02..=0.4).text(tr("Intensity")));
                    ui.horizontal(|ui| { checkbox(ui,&mut prefs.custom_tint,tr("Custom color"));if prefs.custom_tint {ui.color_edit_button_srgb(&mut prefs.tint);} });
                    if prefs.background==super::identity::Background::Ascii {
                        egui::ComboBox::from_id_salt("ascii-pattern").selected_text(prefs.pattern.name()).show_ui(ui,|ui| {
                            for pattern in super::identity::Pattern::ALL {ui.selectable_value(&mut prefs.pattern,pattern,pattern.name());}
                        });
                    }
                    checkbox(ui,&mut prefs.behind_code,tr("Also show behind code")).on_hover_text(tr("Off by default. Keep the code area quiet."));
                    ui.label(muted(tr("Animation pauses without focus, when minimized, or with Reduce motion.")));
                });
                ui.add_space(12.0);
                egui::CollapsingHeader::new(tr("Luma companion")).show(ui, |ui| {
                    let prefs=&mut self.preferences.identity;
                    checkbox(ui,&mut prefs.mascot,tr("Show Luma in workspace"));
                    checkbox(ui,&mut prefs.mascot_motion,tr("Animate Luma"));
                    ui.add(egui::Slider::new(&mut prefs.mascot_intensity,0.0..=1.0).text(tr("Motion intensity")));
                    checkbox(ui,&mut prefs.interactive,tr("Follow pointer and open companion"));
                    ui.add(egui::Slider::new(&mut prefs.mascot_size,40.0..=80.0).text(tr("Size")).suffix(" pt"));
                    ui.label(muted(tr("Original celestial silk moth. Its state follows actual agent and search activity.")));
                });
                ui.add_space(16.0);
                checkbox(ui,&mut self.preferences.quiet_notifications,tr("Quiet agent notifications"));
                ui.add(egui::Slider::new(&mut self.preferences.notification_interval,5..=120).text(tr("Taskbar attention interval")).suffix(" s"));
                section(ui, tr("Agent editing"), tr("Control how proposed changes reach your editor."));
                checkbox(ui,&mut self.preferences.allow_auto_buffer,tr("Allow Auto buffer edits"))
                    .on_hover_text(tr("Auto applies an eligible replacement to the active buffer. Use Save to write the file."));
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
                super::motion::enter(ui, Id::new("quick-access-entry"), 0.20);
                ui.set_width(palette_width);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tr("Quick access")).strong().size(LEAD));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(muted(tr("Esc")));
                    });
                });
                ui.add_space(12.0);
                let down = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowDown));
                let up = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowUp));
                let confirm = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter));
                let input = ui.add(
                    TextEdit::singleline(&mut palette.query)
                        .hint_text(tr("Find a file, > command, @ project, ? Codex"))
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
                            ui.label(secondary(tr("No matching items.")));
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
                    ui.label(muted(tr("Enter  Open")));
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

/// Home action card: icon tile, title, shortcut; lifts and lights its border on hover.
fn home_card(ui: &mut Ui, width: f32, icon: Icon, title: &str, detail: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 64.0), egui::Sense::click());
    #[cfg(test)]
    super::motion::record(ui, format!("Home {title}"), rect);
    let hover = super::motion::state(
        ui,
        response.id.with("card-hover"),
        response.hovered() || response.has_focus(),
        super::motion::CONTROL,
    );
    let press = super::motion::state(
        ui,
        response.id.with("card-press"),
        response.is_pointer_button_down_on(),
        super::motion::HOVER,
    );
    let r = rect.translate(egui::vec2(0.0, -2.0 * hover + press * 1.5));
    let painter = ui.painter();
    if hover > 0.01 {
        painter.add(
            egui::Shadow {
                offset: [0, 6],
                blur: 18,
                spread: 0,
                color: egui::Color32::from_black_alpha((60.0 * hover) as u8),
            }
            .as_shape(r, 12),
        );
    }
    painter.rect(
        r,
        12,
        super::motion::blend(
            colors().surface.gamma_multiply(0.92),
            colors().elevated,
            hover,
        ),
        egui::Stroke::new(
            1.0_f32,
            super::motion::blend(colors().border, colors().accent.gamma_multiply(0.7), hover),
        ),
        egui::StrokeKind::Inside,
    );
    let tile = egui::Rect::from_center_size(
        r.left_center() + egui::vec2(32.0, 0.0),
        egui::vec2(36.0, 36.0),
    );
    painter.rect_filled(
        tile,
        10,
        super::motion::blend(colors().inset, colors().accent_bg, hover),
    );
    draw_icon(
        painter,
        tile.center(),
        icon,
        super::motion::blend(colors().secondary, colors().accent_hover, hover),
    );
    let text = r.left_center() + egui::vec2(62.0, 0.0);
    painter.text(
        text - egui::vec2(0.0, 9.0),
        egui::Align2::LEFT_CENTER,
        tr(title),
        egui::FontId::proportional(BODY + 1.0),
        colors().text,
    );
    painter.text(
        text + egui::vec2(0.0, 10.0),
        egui::Align2::LEFT_CENTER,
        tr(detail),
        egui::FontId::proportional(SMALL),
        colors().muted,
    );
    // Arrow nudges right on hover.
    let arrow = r.right_center() - egui::vec2(20.0 - hover * 4.0, 0.0);
    let s = egui::Stroke::new(1.4_f32, colors().muted.gamma_multiply(0.4 + 0.6 * hover));
    painter.line_segment(
        [arrow - egui::vec2(5.0, 0.0), arrow + egui::vec2(4.0, 0.0)],
        s,
    );
    painter.add(egui::Shape::line(
        vec![
            arrow + egui::vec2(0.5, -3.5),
            arrow + egui::vec2(4.0, 0.0),
            arrow + egui::vec2(0.5, 3.5),
        ],
        s,
    ));
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, title));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
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

//! The top-right mode switcher: Guided, Agents and Student as one segmented control.
//! Agents and Student replace the central view and exclude each other; Guided is an
//! overlay. Switching only flips visibility, so tabs, sessions and lessons survive.
use super::theme::{Icon, SMALL, colors, draw_icon};
use eframe::egui::{self, Key, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Guided,
    Agents,
    Student,
}

impl Mode {
    pub(super) const ALL: [Self; 3] = [Self::Guided, Self::Agents, Self::Student];

    fn label(self) -> &'static str {
        match self {
            Self::Guided => "Guided",
            Self::Agents => "Agents",
            Self::Student => "Student",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Self::Guided => Icon::Wand,
            Self::Agents => Icon::Hub,
            Self::Student => Icon::Book,
        }
    }

    pub(super) fn key(self) -> Key {
        match self {
            Self::Guided => Key::G,
            Self::Agents => Key::H,
            Self::Student => Key::L,
        }
    }

    pub(super) fn modifiers() -> Modifiers {
        Modifiers::COMMAND.plus(Modifiers::SHIFT)
    }

    fn tooltip(self) -> &'static str {
        match self {
            Self::Guided => {
                "Guided Mode: type code line by line from an agent's plan (Ctrl+Shift+G)"
            }
            Self::Agents => "Agents: sessions, providers and the side chat (Ctrl+Shift+H)",
            Self::Student => "Student Mode: lessons, practice and progress (Ctrl+Shift+L)",
        }
    }
}

/// Which surfaces are showing. `studio` is the Library Studio, which the central
/// modes also hide.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ModeState {
    pub(super) guided: bool,
    pub(super) agents: bool,
    pub(super) student: bool,
    pub(super) studio: bool,
}

impl ModeState {
    pub(super) fn active(self, mode: Mode) -> bool {
        match mode {
            Mode::Guided => self.guided,
            Mode::Agents => self.agents,
            Mode::Student => self.student,
        }
    }

    /// Pressing a segment toggles it; opening a central mode closes the other one.
    pub(super) fn toggle(mut self, mode: Mode) -> Self {
        match mode {
            Mode::Guided => {
                self.guided = !self.guided;
                if self.guided {
                    self.student = false;
                }
            }
            Mode::Agents => {
                self.agents = !self.agents;
                if self.agents {
                    self.student = false;
                    self.studio = false;
                }
            }
            Mode::Student => {
                self.student = !self.student;
                if self.student {
                    self.agents = false;
                    self.studio = false;
                    self.guided = false;
                }
            }
        }
        self
    }
}

/// Draws the control and returns the segment that was pressed. `compact` shows icons
/// only for narrow windows.
pub(super) fn segmented(ui: &mut egui::Ui, state: ModeState, compact: bool) -> Option<Mode> {
    let font = egui::FontId::proportional(SMALL);
    let height = 26.0;
    let widths: Vec<f32> = Mode::ALL
        .iter()
        .map(|m| {
            if compact {
                30.0
            } else {
                let galley = ui.fonts_mut(|f| {
                    f.layout_no_wrap(
                        super::i18n::tr(m.label()).to_owned(),
                        font.clone(),
                        egui::Color32::WHITE,
                    )
                });
                galley.size().x + 38.0
            }
        })
        .collect();
    let total: f32 = widths.iter().sum::<f32>() + 4.0;
    let (outer, _) = ui.allocate_exact_size(egui::vec2(total, height), egui::Sense::hover());
    let painter = ui.painter_at(outer.expand(1.0));
    let radius = height * 0.5;
    painter.rect(
        outer,
        radius,
        colors().inset,
        egui::Stroke::new(1.0_f32, colors().border),
        egui::StrokeKind::Inside,
    );
    let mut pressed = None;
    let mut x = outer.left() + 2.0;
    for (mode, width) in Mode::ALL.into_iter().zip(widths) {
        let rect = egui::Rect::from_min_size(
            egui::pos2(x, outer.top() + 2.0),
            egui::vec2(width, height - 4.0),
        );
        x += width;
        let id = ui.id().with(("mode-segment", mode.label()));
        let response = ui
            .interact(rect, id, egui::Sense::click())
            .on_hover_text(super::i18n::tr(mode.tooltip()));
        let active = state.active(mode);
        let t = ui.ctx().animate_bool_with_time(id, active, 0.16);
        let hover = if response.hovered() { 1.0 } else { 0.0 };
        if t > 0.01 {
            painter.rect_filled(rect, radius - 2.0, colors().elevated.gamma_multiply(t));
            painter.rect_stroke(
                rect,
                radius - 2.0,
                egui::Stroke::new(1.0_f32, colors().border.gamma_multiply(t)),
                egui::StrokeKind::Inside,
            );
        } else if hover > 0.0 {
            painter.rect_filled(rect, radius - 2.0, colors().hover.gamma_multiply(0.6));
        }
        let ink = super::motion::blend(colors().secondary, colors().text, t.max(hover * 0.5));
        let icon_center = if compact {
            rect.center()
        } else {
            egui::pos2(rect.left() + 15.0, rect.center().y)
        };
        draw_icon(&painter, icon_center, mode.icon(), ink);
        if !compact {
            painter.text(
                egui::pos2(rect.left() + 27.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                super::i18n::tr(mode.label()),
                font.clone(),
                ink,
            );
        }
        // A small accent dot marks the active segment without flooding it with colour.
        if t > 0.01 {
            painter.circle_filled(
                egui::pos2(rect.center().x, rect.bottom() - 1.5),
                1.4 * t,
                colors().accent.gamma_multiply(t),
            );
        }
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::SelectableLabel,
                true,
                active,
                mode.label(),
            )
        });
        #[cfg(test)]
        super::motion::record(ui, mode.label(), rect);
        if super::theme::pointer(response).clicked() {
            pressed = Some(mode);
        }
    }
    pressed
}

impl super::Editor {
    pub(super) fn mode_state(&self) -> ModeState {
        ModeState {
            guided: self.guided.open,
            agents: self.hub_open,
            student: self.student.open,
            studio: self.studio_open,
        }
    }

    pub(super) fn apply_mode(&mut self, state: ModeState) {
        self.guided.open = state.guided;
        self.hub_open = state.agents;
        self.student.open = state.student;
        self.studio_open = state.studio;
    }

    pub(super) fn switch_mode(&mut self, mode: Mode) {
        self.apply_mode(self.mode_state().toggle(mode));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn central_modes_exclude_each_other() {
        let s = ModeState::default().toggle(Mode::Agents);
        assert!(s.agents && !s.student);
        let s = s.toggle(Mode::Student);
        assert!(s.student && !s.agents);
        let s = s.toggle(Mode::Agents);
        assert!(s.agents && !s.student);
    }

    #[test]
    fn segments_toggle_off_when_pressed_again() {
        for mode in Mode::ALL {
            let on = ModeState::default().toggle(mode);
            assert!(on.active(mode));
            assert!(!on.toggle(mode).active(mode));
        }
    }

    #[test]
    fn guided_overlays_agents_but_not_student() {
        let s = ModeState::default()
            .toggle(Mode::Agents)
            .toggle(Mode::Guided);
        assert!(s.agents && s.guided);
        let s = s.toggle(Mode::Student);
        assert!(s.student && !s.guided && !s.agents);
        let s = s.toggle(Mode::Guided);
        assert!(s.guided && !s.student);
    }

    #[test]
    fn central_modes_close_the_library_studio() {
        let studio = ModeState {
            studio: true,
            ..Default::default()
        };
        assert!(!studio.toggle(Mode::Student).studio);
        assert!(!studio.toggle(Mode::Agents).studio);
        assert!(studio.toggle(Mode::Guided).studio);
    }

    #[test]
    fn shortcuts_are_distinct() {
        let [a, b, c] = Mode::ALL.map(Mode::key);
        assert!(a != b && b != c && a != c);
    }

    #[test]
    fn segmented_control_reports_clicks() {
        let ctx = egui::Context::default();
        let mut pressed = None;
        let at = egui::pos2(14.0, 14.0);
        // Hover, press and release arrive in separate frames, as they do with a mouse.
        for frame in 0..4 {
            let mut input = egui::RawInput::default();
            match frame {
                1 => input.events.push(egui::Event::PointerMoved(at)),
                2 | 3 => input.events.push(egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: frame == 2,
                    modifiers: Modifiers::NONE,
                }),
                _ => {}
            }
            let _ = ctx.run(input, |ctx| {
                egui::Area::new(egui::Id::new("modes"))
                    .fixed_pos(egui::pos2(0.0, 0.0))
                    .show(ctx, |ui| {
                        if let Some(mode) = segmented(ui, ModeState::default(), false) {
                            pressed = Some(mode);
                        }
                    });
            });
        }
        assert_eq!(pressed, Some(Mode::Guided));
    }
}

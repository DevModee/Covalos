//! Original AEROS identity primitives. Bounded work; no background threads or asset downloads.
use super::theme;
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Stroke, Ui};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Background {
    Off,
    #[default]
    Orbital,
    Neural,
    Void,
    Ascii,
    Minimal,
}
impl Background {
    pub(super) const ALL: [Self; 6] = [
        Self::Off,
        Self::Orbital,
        Self::Neural,
        Self::Void,
        Self::Ascii,
        Self::Minimal,
    ];
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Orbital => "Orbital",
            Self::Neural => "Neural",
            Self::Void => "Void",
            Self::Ascii => "ASCII",
            Self::Minimal => "Minimal",
        }
    }
    fn animated(self) -> bool {
        !matches!(self, Self::Off | Self::Minimal)
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Pattern {
    #[default]
    Waves,
    Geometry,
    Particles,
    Orbits,
    Network,
    Sequence,
}
impl Pattern {
    pub(super) const ALL: [Self; 6] = [
        Self::Waves,
        Self::Geometry,
        Self::Particles,
        Self::Orbits,
        Self::Network,
        Self::Sequence,
    ];
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Waves => "Waves",
            Self::Geometry => "Geometry",
            Self::Particles => "Particles",
            Self::Orbits => "Orbits",
            Self::Network => "Network",
            Self::Sequence => "Sequence",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct IdentityPreferences {
    pub(super) background: Background,
    pub(super) pattern: Pattern,
    pub(super) animated: bool,
    pub(super) fps: u32,
    pub(super) speed: f32,
    pub(super) density: f32,
    pub(super) intensity: f32,
    pub(super) tint: [u8; 3],
    pub(super) custom_tint: bool,
    pub(super) behind_code: bool,
    pub(super) mascot: bool,
    pub(super) mascot_size: f32,
    pub(super) mascot_motion: bool,
    pub(super) interactive: bool,
}
impl Default for IdentityPreferences {
    fn default() -> Self {
        Self {
            background: Background::Orbital,
            pattern: Pattern::Waves,
            animated: true,
            fps: 20,
            speed: 0.6,
            density: 0.45,
            intensity: 0.16,
            tint: [176, 154, 250],
            custom_tint: false,
            behind_code: false,
            mascot: true,
            mascot_size: 22.0,
            mascot_motion: true,
            interactive: true,
        }
    }
}
impl IdentityPreferences {
    pub(super) fn sanitize(&mut self) {
        self.fps = self.fps.clamp(5, 30);
        self.speed = finite(self.speed, 0.6).clamp(0.1, 2.0);
        self.density = finite(self.density, 0.45).clamp(0.1, 1.0);
        self.intensity = finite(self.intensity, 0.16).clamp(0.02, 0.40);
        self.mascot_size = finite(self.mascot_size, 22.0).clamp(18.0, 32.0);
    }
}
fn finite(value: f32, default: f32) -> f32 {
    if value.is_finite() { value } else { default }
}
#[derive(Default)]
pub(super) struct Clock {
    phase: f32,
    last: Option<f64>,
    active: bool,
}
impl Clock {
    fn advance(&mut self, now: f64, active: bool, speed: f32) {
        if active && self.active {
            self.phase += (now - self.last.unwrap_or(now)).clamp(0.0, 0.1) as f32 * speed;
        }
        self.last = Some(now);
        self.active = active;
    }
}
#[derive(Default)]
pub(super) struct Identity {
    clock: Clock,
    mascot_clock: Clock,
    ascii: String,
    ascii_key: Option<(u64, Pattern, u32)>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Mood {
    #[default]
    Idle,
    Thinking,
    Coding,
    Searching,
    Approval,
    Success,
    Error,
    Sleeping,
}
impl Mood {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Thinking => "Thinking",
            Self::Coding => "Coding",
            Self::Searching => "Searching",
            Self::Approval => "Waiting for approval",
            Self::Success => "Completed",
            Self::Error => "Error",
            Self::Sleeping => "Sleeping",
        }
    }
    pub(super) fn busy(self) -> bool {
        matches!(self, Self::Thinking | Self::Coding | Self::Searching)
    }
}
impl Identity {
    pub(super) fn begin(
        &mut self,
        ctx: &egui::Context,
        prefs: &IdentityPreferences,
        reduce: bool,
        background_visible: bool,
        mood: Mood,
    ) {
        let visible = ctx.input(|i| i.focused && !i.viewport().minimized.unwrap_or(false));
        let background_active = visible
            && !reduce
            && background_visible
            && prefs.animated
            && prefs.background.animated();
        let mascot_active =
            visible && !reduce && prefs.mascot && prefs.mascot_motion && mood.busy();
        let now = ctx.input(|i| i.time);
        self.clock.advance(now, background_active, prefs.speed);
        self.mascot_clock.advance(now, mascot_active, prefs.speed);
        if background_active || mascot_active {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(
                1.0 / f64::from(prefs.fps.clamp(5, 30)),
            ));
        }
    }
    pub(super) fn paint(&mut self, ui: &Ui, rect: Rect, prefs: &IdentityPreferences, mood: Mood) {
        if matches!(prefs.background, Background::Off | Background::Minimal) || rect.width() < 80.0
        {
            return;
        }
        let p = ui.painter_at(rect);
        let tint = if prefs.custom_tint {
            Color32::from_rgb(prefs.tint[0], prefs.tint[1], prefs.tint[2])
        } else {
            theme::colors().accent
        };
        let color = tint.gamma_multiply(prefs.intensity);
        let t = self.clock.phase;
        let count = (64.0 * prefs.density) as usize;
        match prefs.background {
            Background::Orbital => {
                let center = rect.center() + egui::vec2(rect.width() * 0.20, 0.0);
                let radius = rect.width().min(rect.height()) * 0.39;
                for ring in 0..3 {
                    let angle = ring as f32 * 0.8 + 0.22;
                    let point = |a: f32| {
                        let x = a.cos() * radius;
                        let y = a.sin() * radius * (0.30 + ring as f32 * 0.10);
                        center
                            + egui::vec2(
                                x * angle.cos() - y * angle.sin(),
                                x * angle.sin() + y * angle.cos(),
                            )
                    };
                    p.add(egui::Shape::closed_line(
                        (0..64)
                            .map(|i| point(i as f32 / 64.0 * std::f32::consts::TAU))
                            .collect(),
                        Stroke::new(0.8_f32, color),
                    ));
                    p.circle_filled(
                        point(t * 0.16 + ring as f32 * 2.0),
                        2.4,
                        color.gamma_multiply(2.0),
                    );
                }
                for i in 0..count {
                    let pos = star(rect, i, t * 0.015);
                    p.circle_filled(pos, if i % 7 == 0 { 1.4 } else { 0.7 }, color);
                }
            }
            Background::Neural => {
                let n = count.clamp(8, 48);
                let mut points = [Pos2::ZERO; 48];
                for (i, point) in points.iter_mut().take(n).enumerate() {
                    *point = star(rect, i, t * 0.012);
                }
                let nodes = &points[..n];
                for (i, a) in nodes.iter().enumerate() {
                    p.circle_filled(*a, 1.8, color.gamma_multiply(1.7));
                    for b in nodes.iter().skip(i + 1).take(4) {
                        if a.distance(*b) < rect.width() * 0.30 {
                            p.line_segment([*a, *b], Stroke::new(0.7_f32, color));
                        }
                    }
                }
            }
            Background::Void => {
                for i in 0..count {
                    let pos = star(rect, i, t * 0.004);
                    let pulse = 0.7 + 0.3 * (t * 0.2 + i as f32).sin();
                    p.circle_filled(
                        pos,
                        if i % 9 == 0 { 1.6 } else { 0.8 },
                        color.gamma_multiply(pulse),
                    );
                }
            }
            Background::Ascii => {
                let tick = (t * 8.0) as u64;
                let key = (tick, prefs.pattern, (prefs.density * 100.0) as u32);
                if self.ascii_key != Some(key) {
                    self.ascii.clear();
                    self.ascii.reserve(900);
                    let reactive = if mood.busy() { 1.3 } else { 1.0 };
                    for y in 0..15 {
                        for x in 0..48 {
                            let a = (x as f32 - 24.0) / 10.0;
                            let b = (y as f32 - 7.0) / 5.0;
                            let time = tick as f32 / 8.0;
                            let v = match prefs.pattern {
                                Pattern::Waves => {
                                    (a * 2.5 - time * reactive).sin() * (b * 2.0 + time * 0.4).cos()
                                }
                                Pattern::Geometry => (a.abs() + b.abs() - time * 0.3).sin(),
                                Pattern::Particles => {
                                    ((x * 31 + y * 17 + tick as usize * 3) % 97) as f32 / 48.0 - 1.0
                                }
                                Pattern::Orbits => ((a * a + b * b).sqrt() * 5.0 - time).cos(),
                                Pattern::Network => (a * 3.0 + time * 0.2).sin() * (b * 5.0).sin(),
                                Pattern::Sequence => {
                                    ((x + y + tick as usize) % 11) as f32 / 5.0 - 1.0
                                }
                            };
                            let c = if v < 1.0 - prefs.density * 1.8 {
                                ' '
                            } else if v < 0.3 {
                                '.'
                            } else if v < 0.6 {
                                ':'
                            } else if v < 0.8 {
                                '+'
                            } else {
                                '*'
                            };
                            self.ascii.push(c);
                        }
                        self.ascii.push('\n');
                    }
                    self.ascii_key = Some(key);
                }
                p.text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    &self.ascii,
                    FontId::monospace((rect.width() / 52.0).clamp(7.0, 14.0)),
                    color.gamma_multiply(1.8),
                );
            }
            Background::Off | Background::Minimal => {}
        }
    }
    pub(super) fn mascot(
        &self,
        ui: &mut Ui,
        prefs: &IdentityPreferences,
        mood: Mood,
        reduce: bool,
    ) -> bool {
        if !prefs.mascot {
            return false;
        }
        let size = prefs.mascot_size;
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(size + 8.0, size),
            if prefs.interactive {
                egui::Sense::click()
            } else {
                egui::Sense::hover()
            },
        );
        let p = ui.painter_at(rect);
        let center = rect.center();
        let s = size * 0.42;
        let animated = prefs.mascot_motion && !reduce && self.mascot_clock.active;
        let bob = if animated {
            (self.mascot_clock.phase * 3.0).sin() * 1.0
        } else {
            0.0
        };
        let c = center + egui::vec2(0.0, bob);
        let colors = theme::colors();
        let ink = match mood {
            Mood::Approval => colors.warning,
            Mood::Success => colors.success,
            Mood::Error => colors.error,
            _ => colors.accent,
        };
        // Luma is a kite-shaped orbital probe, drawn from original vector geometry.
        p.add(egui::Shape::convex_polygon(
            vec![
                c + egui::vec2(0.0, -s),
                c + egui::vec2(s, 0.0),
                c + egui::vec2(0.0, s),
                c + egui::vec2(-s, 0.0),
            ],
            colors.accent_bg,
            Stroke::new(1.2_f32, ink),
        ));
        let pointer = ui.input(|i| i.pointer.hover_pos());
        let look = if prefs.interactive {
            pointer
                .map(|pos| (pos - c).normalized() * 1.0)
                .unwrap_or_default()
        } else {
            egui::Vec2::ZERO
        };
        for x in [-s * 0.3, s * 0.3] {
            let eye = c + egui::vec2(x, -s * 0.04) + look;
            if mood == Mood::Sleeping {
                p.line_segment(
                    [eye - egui::vec2(1.5, 0.0), eye + egui::vec2(1.5, 0.0)],
                    Stroke::new(1.1_f32, ink),
                );
            } else {
                p.circle_filled(eye, 1.25, ink);
            }
        }
        p.line_segment(
            [
                c + egui::vec2(-s * 0.13, s * 0.35),
                c + egui::vec2(s * 0.13, s * 0.35),
            ],
            Stroke::new(0.9_f32, ink),
        );
        p.circle_filled(c + egui::vec2(s * 0.85, -s * 0.78), 1.5, ink);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                true,
                format!("Luma: {}. Open agent companion", mood.name()),
            )
        });
        response
            .on_hover_text(format!(
                "Luma · {}
Open agent companion",
                mood.name()
            ))
            .clicked()
    }
}
fn star(rect: Rect, i: usize, t: f32) -> Pos2 {
    let seed = i as f32;
    let x = ((seed * 0.618_034 + t * (1.0 + seed % 3.0)).fract() + 1.0).fract();
    let y = (seed * 0.414_214 + 0.15 + (t + seed).sin() * 0.012).rem_euclid(1.0);
    rect.min + egui::vec2(x * rect.width(), y * rect.height())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn animation_clock_freezes_without_focus_and_does_not_jump_on_resume() {
        let mut c = Clock::default();
        c.advance(1.0, true, 1.0);
        c.advance(1.05, true, 1.0);
        let phase = c.phase;
        c.advance(100.0, false, 1.0);
        c.advance(200.0, false, 1.0);
        assert_eq!(c.phase, phase);
        c.advance(300.0, true, 1.0);
        assert_eq!(c.phase, phase);
        c.advance(300.05, true, 1.0);
        assert!((c.phase - phase - 0.05).abs() < 0.001);
    }
    #[test]
    fn identity_preferences_bound_untrusted_configuration() {
        let mut p = IdentityPreferences {
            fps: u32::MAX,
            speed: f32::NAN,
            density: 99.0,
            intensity: -1.0,
            mascot_size: 200.0,
            ..Default::default()
        };
        p.sanitize();
        assert_eq!(p.fps, 30);
        assert_eq!(p.speed, 0.6);
        assert_eq!(p.density, 1.0);
        assert_eq!(p.intensity, 0.02);
        assert_eq!(p.mascot_size, 32.0);
    }
}

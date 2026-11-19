//! Original AEROS identity primitives. Bounded work; no background threads or asset downloads.
use super::theme;
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Stroke, Ui};
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
    pub(super) mascot_intensity: f32,
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
            mascot_size: 56.0,
            mascot_motion: true,
            mascot_intensity: 0.65,
            interactive: true,
        }
    }
}
impl IdentityPreferences {
    pub(super) fn sanitize(&mut self) {
        self.fps = self.fps.clamp(5, 60);
        self.speed = finite(self.speed, 0.6).clamp(0.1, 2.0);
        self.density = finite(self.density, 0.45).clamp(0.1, 1.0);
        self.intensity = finite(self.intensity, 0.16).clamp(0.02, 0.40);
        self.mascot_size = finite(self.mascot_size, 56.0).clamp(40.0, 80.0);
        self.mascot_intensity = finite(self.mascot_intensity, 0.65).clamp(0.0, 1.0);
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
    pointer: egui::Vec2,
    previous_pointer: Option<Pos2>,
    influence: Option<Pos2>,
    until: f64,
    last: Option<f64>,
    now: f64,
    ripple: Option<(Pos2, f64)>,
    mascot_hover: bool,
    scene_active: bool,
    orbital_cache: Option<OrbitalCache>,
    mascot_click: f64,
}
struct OrbitalCache {
    key: [u32; 4],
    color: Color32,
    shapes: Vec<egui::Shape>,
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
            Self::Idle => "Ready when you are",
            Self::Thinking => "Thinking",
            Self::Coding => "Coding",
            Self::Searching => "Searching",
            Self::Approval => "Waiting for approval",
            Self::Success => "Completed",
            Self::Error => "Needs attention",
            Self::Sleeping => "Resting",
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
        let (now, visible, pointer, click) = ctx.input(|i| {
            (
                i.time,
                i.focused && !i.viewport().minimized.unwrap_or(false),
                i.pointer.hover_pos(),
                i.pointer.primary_clicked(),
            )
        });
        let dt = (now - self.last.unwrap_or(now)).clamp(0.0, 0.1) as f32;
        self.last = Some(now);
        self.now = now;
        if pointer != self.previous_pointer && pointer.is_some() || click {
            self.until = now + 1.2;
        }
        self.previous_pointer = pointer;
        let target = if prefs.interactive && visible {
            pointer
                .map(|p| {
                    let r = ctx.content_rect();
                    egui::vec2(
                        (p.x - r.center().x) / (r.width() * 0.5),
                        (p.y - r.center().y) / (r.height() * 0.5),
                    )
                    .clamp(egui::vec2(-1.0, -1.0), egui::vec2(1.0, 1.0))
                })
                .unwrap_or_default()
        } else {
            egui::Vec2::ZERO
        };
        if reduce {
            self.pointer = egui::Vec2::ZERO;
        } else {
            self.pointer += (target - self.pointer) * (1.0 - (-12.0 * dt).exp());
        }
        self.scene_active = visible && prefs.interactive && !reduce && prefs.animated;
        self.influence = if self.scene_active { pointer } else { None };
        if click && visible && prefs.interactive && !reduce && prefs.animated {
            self.ripple = pointer.map(|p| (p, now));
        }
        if self.ripple.is_some_and(|(_, start)| now - start > 1.1) {
            self.ripple = None;
        }
        let background_active = visible
            && !reduce
            && background_visible
            && prefs.animated
            && prefs.background.animated();
        let mascot_active = visible
            && !reduce
            && prefs.mascot
            && prefs.mascot_motion
            && prefs.mascot_intensity > 0.0
            && (mood.busy()
                || self.mascot_hover
                || now < self.until
                || now - self.mascot_click < 0.6);
        self.mascot_hover = false;
        self.clock.advance(now, background_active, prefs.speed);
        self.mascot_clock.advance(now, mascot_active, 1.0);
        if background_active || mascot_active {
            let interacting = now < self.until || now - self.mascot_click < 0.6;
            let fps = if interacting {
                60
            } else {
                prefs.fps.clamp(5, 60)
            };
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(1.0 / f64::from(fps)));
        }
    }
    fn displace(&self, pos: Pos2, depth: f32) -> Pos2 {
        let mut p = pos
            + if self.scene_active {
                self.pointer * (6.0 + depth * 12.0)
            } else {
                egui::Vec2::ZERO
            };
        if let Some(cursor) = self.influence {
            let d = p - cursor;
            let distance = d.length();
            if distance > 0.1 && distance < 160.0 {
                p += d / distance * (1.0 - distance / 160.0).powi(2) * (10.0 + 12.0 * depth);
            }
        }
        p
    }
    pub(super) fn paint(&mut self, ui: &Ui, rect: Rect, prefs: &IdentityPreferences, _mood: Mood) {
        if matches!(prefs.background, Background::Off | Background::Minimal) || rect.width() < 80.0
        {
            return;
        }
        let p = ui.painter_at(rect);
        let palette = theme::colors();
        let tint = if prefs.custom_tint {
            Color32::from_rgb(prefs.tint[0], prefs.tint[1], prefs.tint[2])
        } else {
            palette.accent
        };
        let color = tint.gamma_multiply(prefs.intensity);
        let t = self.clock.phase;
        let center = rect.center()
            + egui::vec2(rect.width() * 0.23, 0.0)
            + if self.scene_active {
                self.pointer * 14.0
            } else {
                egui::Vec2::ZERO
            };
        let radius = rect.width().min(rect.height()) * 0.38;
        let count = (90.0 * prefs.density) as usize;
        match prefs.background {
            Background::Orbital => {
                halo(
                    &p,
                    center,
                    radius * 1.1,
                    tint.gamma_multiply(prefs.intensity * 0.14),
                );
                let key = [
                    center.x.to_bits(),
                    center.y.to_bits(),
                    radius.to_bits(),
                    ui.ctx().pixels_per_point().to_bits(),
                ];
                if !self
                    .orbital_cache
                    .as_ref()
                    .is_some_and(|cache| cache.key == key && cache.color == color)
                {
                    let mut tracks = Vec::with_capacity(288);
                    for ring in 0..4 {
                        let angle = ring as f32 * 0.65 + 0.24;
                        let r = radius * (0.82 + ring as f32 * 0.07);
                        let point = |a: f32| {
                            let x = a.cos() * r;
                            let y = a.sin() * r * (0.26 + ring as f32 * 0.10);
                            center
                                + egui::vec2(
                                    x * angle.cos() - y * angle.sin(),
                                    x * angle.sin() + y * angle.cos(),
                                )
                        };
                        for i in 0..72 {
                            let a = i as f32 / 72.0 * std::f32::consts::TAU;
                            let depth = (a.sin() + 1.0) * 0.5;
                            tracks.push(egui::epaint::ClippedShape {
                                clip_rect: Rect::EVERYTHING,
                                shape: egui::Shape::line_segment(
                                    [point(a), point(a + std::f32::consts::TAU / 72.0)],
                                    Stroke::new(
                                        0.6 + depth * 0.5,
                                        color.gamma_multiply(0.5 + depth),
                                    ),
                                ),
                            });
                        }
                    }
                    let shapes = ui
                        .ctx()
                        .tessellate(tracks, ui.ctx().pixels_per_point())
                        .into_iter()
                        .filter_map(|primitive| match primitive.primitive {
                            egui::epaint::Primitive::Mesh(mesh) => Some(egui::Shape::mesh(mesh)),
                            _ => None,
                        })
                        .collect();
                    self.orbital_cache = Some(OrbitalCache { key, color, shapes });
                }
                if let Some(cache) = &self.orbital_cache {
                    p.extend(cache.shapes.iter().cloned());
                }
                for ring in 0..4 {
                    let angle = ring as f32 * 0.65 + 0.24;
                    let r = radius * (0.82 + ring as f32 * 0.07);
                    let point = |a: f32| {
                        let x = a.cos() * r;
                        let y = a.sin() * r * (0.26 + ring as f32 * 0.10);
                        center
                            + egui::vec2(
                                x * angle.cos() - y * angle.sin(),
                                x * angle.sin() + y * angle.cos(),
                            )
                    };
                    let planet = point(t * 0.16 * (1.0 + ring as f32 * 0.15) + ring as f32 * 1.8);
                    halo(&p, planet, 9.0, color.gamma_multiply(0.5));
                    p.circle_filled(planet, 2.0 + ring as f32 * 0.3, color.gamma_multiply(2.5));
                }
                for i in 0..count {
                    let pos = self.displace(star(rect, i, t * 0.007), i as f32 % 3.0 / 3.0);
                    p.circle_filled(
                        pos,
                        if i % 7 == 0 { 1.3 } else { 0.7 },
                        color.gamma_multiply(if i % 7 == 0 { 1.7 } else { 0.8 }),
                    );
                }
                sphere(
                    &p,
                    center,
                    radius * 0.22,
                    palette.canvas,
                    tint,
                    prefs.intensity,
                );
            }
            Background::Neural => {
                let n = count.clamp(12, 48);
                let mut nodes = [Pos2::ZERO; 48];
                for (i, node) in nodes.iter_mut().take(n).enumerate() {
                    *node = self.displace(star(rect, i, t * 0.006), 0.7);
                }
                for (i, a) in nodes[..n].iter().enumerate() {
                    let proximity = self
                        .influence
                        .map_or(0.0, |c| (1.0 - a.distance(c) / 220.0).max(0.0));
                    for b in nodes[..n].iter().skip(i + 1) {
                        let d = a.distance(*b);
                        if d < 170.0 {
                            p.line_segment(
                                [*a, *b],
                                Stroke::new(
                                    0.7_f32,
                                    color.gamma_multiply(
                                        (1.0 - d / 170.0) * (0.7 + proximity * 2.0),
                                    ),
                                ),
                            );
                        }
                    }
                    if proximity > 0.1 {
                        halo(&p, *a, 14.0, color.gamma_multiply(proximity * 0.4));
                    }
                    p.circle_filled(
                        *a,
                        1.4 + proximity * 1.8,
                        color.gamma_multiply(1.2 + proximity * 2.0),
                    );
                }
            }
            Background::Void => {
                halo(
                    &p,
                    center,
                    radius,
                    tint.gamma_multiply(prefs.intensity * 0.08),
                );
                for i in 0..count {
                    let depth = (i % 5) as f32 / 5.0;
                    let pos = self.displace(star(rect, i, t * 0.002 * (1.0 + depth)), depth);
                    p.circle_filled(pos, 0.6 + depth * 1.5, color.gamma_multiply(0.5 + depth));
                }
            }
            Background::Ascii => {
                let columns = (42.0 + prefs.density * 22.0) as usize;
                let rows = 22;
                let step = (rect.width().min(780.0) / (columns as f32)).clamp(7.0, 15.0);
                let origin =
                    center - egui::vec2(columns as f32 * step * 0.5, rows as f32 * step * 0.65);
                let glyphs = [' ', '.', ':', '+', '*', '#'];
                let font = FontId::monospace(step * 0.84);
                let galleys =
                    glyphs.map(|c| p.layout_no_wrap(c.to_string(), font.clone(), Color32::WHITE));
                for y in 0..rows {
                    for x in 0..columns {
                        let base = origin + egui::vec2(x as f32 * step, y as f32 * step * 1.3);
                        let pos = self.displace(base, 0.8);
                        let a = (x as f32 - columns as f32 * 0.5) / 13.0;
                        let b = (y as f32 - rows as f32 * 0.5) / 7.0;
                        let distance = self.influence.map_or(900.0, |c| base.distance(c));
                        let influence = (1.0 - distance / 180.0).max(0.0);
                        let wave = match prefs.pattern {
                            Pattern::Waves => (a * 2.4 - t * 0.7).sin() * (b * 2.0 + t * 0.2).cos(),
                            Pattern::Geometry => (a.abs() + b.abs() - t * 0.3).sin(),
                            Pattern::Particles => {
                                (a * 7.0 + b * 11.0 + t * 0.4).sin() * (b * 4.0 - a).cos()
                            }
                            Pattern::Orbits => ((a * a + b * b).sqrt() * 5.0 - t).cos(),
                            Pattern::Network => (a * 3.0 + t * 0.2).sin() * (b * 5.0).sin(),
                            Pattern::Sequence => ((a + b) * 3.0 - t * 0.5).sin(),
                        };
                        let v = ((wave + 1.0) * 0.5 + influence * 0.5).clamp(0.0, 0.999);
                        let index = (v * 5.0) as usize;
                        if index > 0 && v > 0.5 - prefs.density * 0.3 {
                            p.galley_with_override_text_color(
                                pos,
                                galleys[index].clone(),
                                color.gamma_multiply(0.6 + v + influence),
                            );
                        }
                    }
                }
            }
            Background::Off | Background::Minimal => {}
        }
        if prefs.animated
            && let Some((origin, start)) = self.ripple
            && rect.contains(origin)
        {
            let age = (self.now - start) as f32;
            let fade = (1.0 - age / 1.1).max(0.0);
            p.circle_stroke(
                origin,
                12.0 + age * 110.0,
                Stroke::new(0.8_f32, color.gamma_multiply(fade * 1.5)),
            );
        }
    }
    pub(super) fn mascot(
        &mut self,
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
            egui::vec2(size, size + 4.0),
            if prefs.interactive {
                egui::Sense::click()
            } else {
                egui::Sense::hover()
            },
        );
        #[cfg(test)]
        super::motion::record(ui, "Luma", rect);
        self.mascot_hover = response.hovered();
        if response.clicked() {
            self.mascot_click = self.now;
            ui.ctx().request_repaint();
        }
        let animated = prefs.mascot_motion && !reduce && self.mascot_clock.active;
        let intensity = if prefs.mascot_motion && !reduce {
            prefs.mascot_intensity
        } else {
            0.0
        };
        let hover = if intensity > 0.0 && prefs.interactive {
            super::motion::state(
                ui,
                response.id.with("curious"),
                response.hovered(),
                super::motion::CONTROL,
            ) * intensity
        } else {
            0.0
        };
        let bob = if animated {
            (self.mascot_clock.phase * 2.1).sin() * 0.8 * intensity
        } else {
            0.0
        };
        let click_age = (self.now - self.mascot_click) as f32;
        let bounce = if animated && (0.0..0.6).contains(&click_age) {
            (click_age / 0.6 * std::f32::consts::PI).sin() * 3.0 * intensity
        } else {
            0.0
        };
        let art = Rect::from_center_size(
            rect.center() + egui::vec2(0.0, bob - bounce - hover),
            egui::vec2(size * (1.0 + hover * 0.04), size),
        );
        super::assets::paint(ui, art, "luma");
        let face = art.min + egui::vec2(art.width() * 0.5, art.height() * 0.365);
        let mut pointer = if prefs.interactive && prefs.mascot_motion && !reduce {
            self.pointer * size * 0.016 * intensity
        } else {
            egui::Vec2::ZERO
        };
        if animated {
            match mood {
                Mood::Thinking => pointer.y -= size * 0.018,
                Mood::Coding => pointer.y += size * 0.012,
                Mood::Searching => {
                    pointer.x += (self.mascot_clock.phase * 1.8).sin() * size * 0.035
                }
                _ => {}
            }
        }
        let ink = Color32::from_rgb(39, 29, 57);
        let p = ui.painter_at(rect);
        let eye_r = (size * 0.025).max(1.1);
        for x in [-size * 0.067, size * 0.067] {
            let c = face + egui::vec2(x, 0.0) + pointer;
            if mood == Mood::Sleeping {
                p.line_segment(
                    [c - egui::vec2(eye_r, 0.0), c + egui::vec2(eye_r, 0.0)],
                    Stroke::new(1.1_f32, ink),
                );
            } else if mood == Mood::Success {
                p.add(egui::Shape::line(
                    vec![
                        c + egui::vec2(-eye_r, 0.0),
                        c + egui::vec2(0.0, -eye_r),
                        c + egui::vec2(eye_r, 0.0),
                    ],
                    Stroke::new(1.2_f32, ink),
                ));
            } else {
                p.circle_filled(c, eye_r * (1.0 + hover * 0.18), ink);
                p.circle_filled(
                    c - egui::vec2(eye_r * 0.2, eye_r * 0.3),
                    eye_r * 0.25,
                    Color32::WHITE,
                );
            }
        }
        let mouth = face + egui::vec2(0.0, size * 0.055);
        if mood == Mood::Approval || mood == Mood::Error {
            p.circle_stroke(mouth, size * 0.018, Stroke::new(1.0_f32, ink));
        } else {
            p.line_segment(
                [
                    mouth - egui::vec2(size * 0.018, 0.0),
                    mouth + egui::vec2(size * 0.018, 0.0),
                ],
                Stroke::new(0.9_f32, ink),
            );
        }
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                true,
                format!("Luma: {}. Open agent companion", mood.name()),
            )
        });
        response
            .on_hover_text(format!("Luma · {}\nOpen agent companion", mood.name()))
            .clicked()
    }
}
fn sphere(
    p: &egui::Painter,
    center: Pos2,
    radius: f32,
    dark: Color32,
    tint: Color32,
    intensity: f32,
) {
    let mut mesh = egui::Mesh::default();
    let rings = 12;
    let slices = 48;
    for ring in 0..=rings {
        let r = ring as f32 / rings as f32;
        for slice in 0..=slices {
            let a = slice as f32 / slices as f32 * std::f32::consts::TAU;
            let x = r * a.cos();
            let y = r * a.sin();
            let z = (1.0 - r * r).max(0.0).sqrt();
            let light = (-x * 0.4 - y * 0.55 + z * 0.7).max(0.0);
            let rim = r.powi(8) * (-y * 0.5 + 0.5);
            let color = super::motion::blend(
                dark,
                tint,
                ((light * 0.36 + rim * 0.32) * (intensity * 3.0)).clamp(0.0, 0.65),
            );
            mesh.colored_vertex(center + egui::vec2(x, y) * radius, color);
        }
    }
    for ring in 0..rings {
        for slice in 0..slices {
            let a = (ring * (slices + 1) + slice) as u32;
            let b = a + (slices + 1) as u32;
            mesh.add_triangle(a, b, a + 1);
            mesh.add_triangle(a + 1, b, b + 1);
        }
    }
    p.add(egui::Shape::mesh(mesh));
}
fn halo(p: &egui::Painter, center: Pos2, radius: f32, color: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(center, color);
    for i in 0..=32 {
        let a = i as f32 / 32.0 * std::f32::consts::TAU;
        mesh.colored_vertex(
            center + egui::vec2(a.cos(), a.sin()) * radius,
            Color32::TRANSPARENT,
        );
    }
    for i in 1..=32 {
        mesh.add_triangle(0, i, i + 1);
    }
    p.add(egui::Shape::mesh(mesh));
}
fn star(rect: Rect, i: usize, t: f32) -> Pos2 {
    let s = i as f32;
    let x = (s * 0.618_034 + t * (1.0 + s % 3.0)).rem_euclid(1.0);
    let y = (s * 0.414_214 + 0.15 + (t + s).sin() * 0.012).rem_euclid(1.0);
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
        assert_eq!(p.fps, 60);
        assert_eq!(p.speed, 0.6);
        assert_eq!(p.density, 1.0);
        assert_eq!(p.intensity, 0.02);
        assert_eq!(p.mascot_size, 80.0);
    }
}

#[cfg(test)]
mod interaction_tests {
    use super::*;
    fn frame(
        ctx: &egui::Context,
        identity: &mut Identity,
        prefs: &IdentityPreferences,
        time: f64,
        pointer: Pos2,
        reduce: bool,
    ) -> f64 {
        let out = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 650.0))),
                time: Some(time),
                focused: true,
                events: vec![egui::Event::PointerMoved(pointer)],
                ..Default::default()
            },
            |ctx| {
                identity.begin(ctx, prefs, reduce, true, Mood::Idle);
                egui::CentralPanel::default().show(ctx, |ui| {
                    identity.paint(ui, ui.available_rect_before_wrap(), prefs, Mood::Idle)
                });
            },
        );
        ctx.tessellate(out.shapes, out.pixels_per_point)
            .iter()
            .map(|p| match &p.primitive {
                egui::epaint::Primitive::Mesh(m) => m
                    .vertices
                    .iter()
                    .map(|v| {
                        f64::from(v.pos.x) * 0.17
                            + f64::from(v.pos.y) * 0.31
                            + f64::from(v.color.a()) * 0.07
                    })
                    .sum(),
                _ => 0.0,
            })
            .sum()
    }
    #[test]
    fn pointer_deforms_each_scene_and_reduced_motion_freezes_it() {
        for background in [
            Background::Orbital,
            Background::Neural,
            Background::Ascii,
            Background::Void,
        ] {
            let ctx = egui::Context::default();
            let mut identity = Identity::default();
            let prefs = IdentityPreferences {
                background,
                ..Default::default()
            };
            let a = frame(
                &ctx,
                &mut identity,
                &prefs,
                1.0,
                egui::pos2(100.0, 100.0),
                false,
            );
            let b = frame(
                &ctx,
                &mut identity,
                &prefs,
                1.0,
                egui::pos2(700.0, 350.0),
                false,
            );
            assert!((a - b).abs() > 1.0, "pointer did not affect {background:?}");
            let a = frame(
                &ctx,
                &mut identity,
                &prefs,
                2.0,
                egui::pos2(100.0, 100.0),
                true,
            );
            let b = frame(
                &ctx,
                &mut identity,
                &prefs,
                3.0,
                egui::pos2(700.0, 350.0),
                true,
            );
            assert!(
                (a - b).abs() < 0.01,
                "reduced motion changed {background:?}"
            );
        }
    }
    #[test]
    fn static_background_preserves_mascot_motion_clock() {
        let ctx = egui::Context::default();
        let mut identity = Identity::default();
        let prefs = IdentityPreferences {
            animated: false,
            ..Default::default()
        };
        let a = frame(
            &ctx,
            &mut identity,
            &prefs,
            1.0,
            egui::pos2(100.0, 100.0),
            false,
        );
        let b = frame(
            &ctx,
            &mut identity,
            &prefs,
            1.05,
            egui::pos2(700.0, 350.0),
            false,
        );
        assert!((a - b).abs() < 0.01);
        assert!(identity.mascot_clock.phase > 0.0);
        assert_eq!(identity.clock.phase, 0.0);
    }
}

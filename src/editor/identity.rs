//! Original COVALOS identity primitives. Bounded work; no background threads or asset downloads.
use super::theme;
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Stroke, Ui};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Background {
    Off,
    #[default]
    Dither,
    Bloom,
    Orbital,
    Neural,
    Void,
    Ascii,
    Minimal,
}
impl Background {
    pub(super) const ALL: [Self; 8] = [
        Self::Off,
        Self::Dither,
        Self::Bloom,
        Self::Orbital,
        Self::Neural,
        Self::Void,
        Self::Ascii,
        Self::Minimal,
    ];
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Dither => "Dither Field",
            Self::Bloom => "Dither Bloom",
            Self::Orbital => "Orbital",
            Self::Neural => "Neural",
            Self::Void => "Void",
            Self::Ascii => "ASCII Flow",
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
            Self::Waves => "Nebula",
            Self::Geometry => "Prism",
            Self::Particles => "Drift",
            Self::Orbits => "Orbit",
            Self::Network => "Contour",
            Self::Sequence => "Signal",
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
            background: Background::Dither,
            pattern: Pattern::Waves,
            animated: true,
            fps: 24,
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
    mood: (Mood, f64),
    awake_until: f64,
    dither: super::dither::Input,
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
        let content = ctx.content_rect();
        let (now, visible, pointer, click) = ctx.input(|i| {
            (
                i.time,
                i.focused
                    && !i.viewport().minimized.unwrap_or(false)
                    && content.width() > 64.0
                    && content.height() > 64.0,
                i.pointer.hover_pos(),
                i.pointer.primary_clicked(),
            )
        });
        let first = self.last.is_none();
        let dt = (now - self.last.unwrap_or(now)).clamp(0.0, 0.1) as f32;
        self.last = Some(now);
        self.now = now;
        let typing = ctx.input(|i| {
            i.events.iter().any(|e| {
                matches!(
                    e,
                    egui::Event::Key { .. } | egui::Event::Text(_) | egui::Event::MouseWheel { .. }
                )
            })
        });
        if pointer != self.previous_pointer && pointer.is_some() || click {
            self.until = now + 1.2;
        }
        if first || typing || pointer != self.previous_pointer || click {
            // Luma stays animated for a while after any input, then rests in a still pose.
            self.awake_until = now + 8.0;
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
        let dithered = matches!(prefs.background, Background::Dither | Background::Bloom);
        if dithered && self.scene_active && background_visible {
            self.dither.record(now, pointer, click);
            if click {
                self.until = self.until.max(now + super::dither::RIPPLE_SECONDS);
            }
        } else {
            self.dither.clear();
        }
        if click && !dithered && visible && prefs.interactive && !reduce && prefs.animated {
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
        if first {
            // The initial mood is not a change; no reaction burst at startup.
            self.mood = (mood, now - 10.0);
        } else if self.mood.0 != mood {
            // Only one-shot reactions (hop, shake, attention) need the fast frame window;
            // waking from Sleeping or switching between work moods does not.
            let reacts = matches!(mood, Mood::Success | Mood::Error | Mood::Approval);
            self.mood = (mood, if reacts { now } else { now - 10.0 });
            self.awake_until = self.awake_until.max(now + 3.0);
        }
        let hovering = self.mascot_hover;
        self.mascot_hover = false;
        let mascot_active = visible
            && !reduce
            && prefs.mascot
            && prefs.mascot_motion
            && prefs.mascot_intensity > 0.0
            && (now < self.awake_until
                || mood.busy()
                || mood == Mood::Approval
                || now - self.mood.1 < 1.6
                || hovering
                || now - self.mascot_click < 0.6);
        self.clock.advance(now, background_active, prefs.speed);
        self.mascot_clock.advance(now, mascot_active, 1.0);
        if background_active || mascot_active {
            let interacting = now < self.until
                || (background_active && self.dither.busy())
                || now - self.mascot_click < 0.6
                || now - self.mood.1 < 1.6
                || hovering;
            let fps = if interacting {
                60
            } else if background_active {
                prefs.fps.clamp(5, 60)
            } else if mood.busy() || mood == Mood::Approval {
                24
            } else {
                // Idle breathing and blinking only; keep the cost negligible.
                12
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
        if prefs.background == Background::Off || rect.width() < 80.0 {
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
        let background = match prefs.background {
            Background::Dither | Background::Bloom if !super::dither::available() => {
                Background::Minimal
            }
            other => other,
        };
        match background {
            Background::Dither | Background::Bloom => {
                // Neutral ink for the field; the accent appears only where the user acts.
                let gamma = |c: Color32| {
                    egui::Rgba::from_rgba_premultiplied(
                        f32::from(c.r()) / 255.0,
                        f32::from(c.g()) / 255.0,
                        f32::from(c.b()) / 255.0,
                        1.0,
                    )
                };
                let mut frame = super::dither::Frame {
                    variant: if background == Background::Bloom {
                        super::dither::Variant::Bloom
                    } else {
                        super::dither::Variant::Field
                    },
                    time: t * 4.0,
                    ink: gamma(if background == Background::Bloom {
                        tint
                    } else {
                        palette.text
                    }),
                    // The brush and ripples use a saturated violet like the reference; a
                    // custom tint replaces it.
                    accent: gamma(if prefs.custom_tint {
                        tint
                    } else if theme::active().is_dark() {
                        Color32::from_rgb(128, 92, 246)
                    } else {
                        Color32::from_rgb(104, 60, 214)
                    }),
                    strength: (prefs.intensity / 0.16).clamp(0.25, 2.4),
                    density: prefs.density,
                    pixel: 4.0,
                    light: !theme::active().is_dark(),
                    trail: [[0.0, 0.0, 1.0]; super::dither::TRAIL],
                    ripples: [[0.0, 0.0, -1.0]; super::dither::RIPPLES],
                };
                self.dither.fill(&mut frame, rect, self.now);
                super::dither::paint(&p, rect, frame);
            }
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
                // ASCII Flow. Two layers: a quiet dot-grid paper and a composed glyph field.
                // Every preset is weighted by a soft stage to the right of the content
                // column, so the left side stays calm and the composition has a focus.
                let dark = palette.canvas.r() < 128;
                let gain = if dark { 1.0 } else { 2.4 };
                let ink_base = if dark {
                    tint
                } else {
                    super::motion::blend(tint, palette.text, 0.55)
                };
                let step = (16.0 - prefs.density * 5.0).clamp(10.0, 16.0);
                let row_h = step * 1.3;
                let cols = ((rect.width() / step) as usize + 1).min(240);
                let rows = ((rect.height() / row_h) as usize + 1).min(120);
                let glyphs = [' ', '.', '·', ':', '-', '=', '+', '*', '#', '%', '@'];
                let font = FontId::monospace(step * 0.92);
                let galleys =
                    glyphs.map(|c| p.layout_no_wrap(c.to_string(), font.clone(), Color32::WHITE));
                let ripple = self.ripple.map(|(o, s)| (o, (self.now - s) as f32));
                let stage = rect.center()
                    + egui::vec2(rect.width() * 0.2, -rect.height() * 0.04)
                    + if self.scene_active {
                        self.pointer * 10.0
                    } else {
                        egui::Vec2::ZERO
                    };
                let stage_r = rect.width().min(rect.height()) * 0.62;
                let far = if self.scene_active {
                    self.pointer * 3.0
                } else {
                    egui::Vec2::ZERO
                };
                let near = if self.scene_active {
                    self.pointer * 9.0
                } else {
                    egui::Vec2::ZERO
                };
                // Prism: a rotating lit sphere with a tilted ring, sampled per cell.
                let sphere_r = stage_r * 0.42;
                let (ls, lc) = (t * 0.35).sin_cos();
                let light = egui::vec2(-0.55 + ls * 0.35, -0.6 + lc * 0.2);
                for y in 0..rows {
                    for x in 0..cols {
                        let base = rect.min + egui::vec2(x as f32 * step, y as f32 * row_h);
                        let cell_c = base + egui::vec2(step * 0.5, row_h * 0.5);
                        // Paper layer: every other cell carries a faint dot.
                        if (x + y) % 2 == 0 {
                            p.circle_filled(
                                cell_c + far,
                                0.55,
                                ink_base.gamma_multiply((prefs.intensity * 0.55 * gain).min(1.0)),
                            );
                        }
                        let d = cell_c - stage;
                        let stage_w = (1.0 - (d.length() / stage_r).powf(1.6)).clamp(0.0, 1.0);
                        let mut sample = egui::vec2(x as f32, y as f32 * 1.3) * 0.07;
                        let mut lens = 0.0;
                        if let Some(c) = self.influence {
                            let dc = cell_c - c;
                            let dist = dc.length();
                            if dist < 200.0 {
                                let k = 1.0 - dist / 200.0;
                                sample -= dc / dist.max(1.0) * k * k * 1.4;
                                lens = (1.0 - ((dist - 64.0) / 30.0).abs()).max(0.0) * 0.5
                                    + k * k * 0.18;
                            }
                        }
                        let mut wave = 0.0;
                        if let Some((o, age)) = ripple {
                            let dist = cell_c.distance(o);
                            wave = (1.0 - ((dist - age * 560.0) / 30.0).abs()).max(0.0)
                                * (1.0 - age / 1.1).max(0.0);
                        }
                        let (v, tile) = match prefs.pattern {
                            Pattern::Waves => {
                                // Nebula: halftone clouds drawn as dots, no glyphs.
                                let n = flow(sample * 0.8, t * 0.6);
                                let v = (n * (0.35 + stage_w * 0.9) + lens + wave).min(1.0);
                                if v > 0.08 {
                                    let r = (step * 0.42 * v.powf(0.8)).max(0.6);
                                    let hot = lens + wave;
                                    p.circle_filled(
                                        cell_c + near * v,
                                        r,
                                        super::motion::blend(ink_base, palette.text, hot.min(0.6))
                                            .gamma_multiply(
                                                (prefs.intensity
                                                    * gain
                                                    * (1.2 + v * 2.2 + hot * 2.0))
                                                    .min(1.0),
                                            ),
                                    );
                                }
                                continue;
                            }
                            Pattern::Geometry => {
                                let q = (cell_c - stage) / sphere_r;
                                let r2 = q.x * q.x + q.y * q.y;
                                let mut v: f32 = 0.0;
                                if r2 < 1.0 {
                                    let z = (1.0 - r2).sqrt();
                                    let lambert =
                                        (-(q.x * light.x + q.y * light.y) + z * 0.6).max(0.0);
                                    // Longitude bands give the surface a sense of rotation.
                                    let lon = q.x.atan2(z) + t * 0.5;
                                    let bands = ((lon * 6.0).sin() * 0.5 + 0.5) * 0.25;
                                    v = (lambert * 0.85 + bands).min(1.0);
                                }
                                // Tilted ring around the sphere, hidden behind it.
                                let (rs, rc) = (-0.42_f32).sin_cos();
                                let rx = q.x * rc + q.y * rs;
                                let ry = -q.x * rs + q.y * rc;
                                let e = (rx / 1.65).powi(2) + (ry / 0.42).powi(2);
                                let front = ry > 0.0 || r2 > 1.0;
                                if (e - 1.0).abs() < 0.09 && front {
                                    let a = ry.atan2(rx) - t * 0.8;
                                    v = v.max(0.55 + 0.4 * (a * 3.0).sin().abs());
                                }
                                ((v + lens + wave).min(0.999), v > 0.7)
                            }
                            Pattern::Particles => {
                                // Drift: flow bands; dense cells become tinted tiles.
                                let n = flow(sample, t);
                                let band = ((sample.y * 0.9 + n * 2.4 - t * 0.12).sin() * 0.5
                                    + 0.5)
                                    .powf(3.0);
                                let v = (n * band * 1.5 * (0.3 + stage_w) + lens + wave).min(0.999);
                                (v, v > 0.62)
                            }
                            Pattern::Orbits => {
                                let dist = d.length() / stage_r;
                                let ring = ((dist * 14.0 - t * 1.2).sin() * 0.5 + 0.5).powf(4.0);
                                ((ring * stage_w * 1.1 + lens + wave).min(0.999), false)
                            }
                            Pattern::Network => {
                                // Contour: topographic iso-lines of the flow field.
                                let n = flow(sample * 0.6, t * 0.4);
                                let iso = 1.0 - ((n * 7.0).fract() - 0.5).abs() * 2.0;
                                (
                                    (iso.powf(2.6) * (0.25 + stage_w) + lens + wave).min(0.999),
                                    false,
                                )
                            }
                            Pattern::Sequence => {
                                // Signal: a travelling waveform across the stage.
                                let u = d.x / stage_r;
                                let amp = (1.0 - u.abs()).max(0.0) * (0.5 + 0.5 * (t * 0.7).sin());
                                let wave_y = (u * 9.0 - t * 1.6).sin() * amp * 0.35
                                    + (u * 23.0 + t).sin() * amp * 0.06;
                                let dy = d.y / stage_r - wave_y;
                                let v = ((1.0 - dy.abs() * 4.5).max(0.0)
                                    * (1.0 - u.abs()).max(0.0)
                                    + lens
                                    + wave)
                                    .min(0.999);
                                (v, false)
                            }
                        };
                        let index = (v * glyphs.len() as f32) as usize;
                        if index == 0 {
                            continue;
                        }
                        let hot = lens + wave;
                        if tile {
                            p.rect_filled(
                                egui::Rect::from_min_size(
                                    base + near * 0.5,
                                    egui::vec2(step - 1.0, row_h - 1.0),
                                ),
                                1,
                                ink_base
                                    .gamma_multiply((prefs.intensity * gain * 0.55 * v).min(1.0)),
                            );
                        }
                        let ink = if hot > 0.3 {
                            super::motion::blend(ink_base, palette.text, (hot - 0.3).min(0.55))
                        } else {
                            ink_base
                        };
                        p.galley_with_override_text_color(
                            base + near * v + egui::vec2(step * 0.04, row_h * 0.08),
                            galleys[index].clone(),
                            ink.gamma_multiply(
                                (prefs.intensity * gain * (1.0 + v * 3.0 + hot * 3.0)).min(1.0),
                            ),
                        );
                    }
                }
            }
            Background::Minimal => {
                // A quiet dot grid with a soft spotlight under the cursor.
                let step = 22.0;
                let cols = (rect.width() / step) as usize + 1;
                let rows = (rect.height() / step) as usize + 1;
                let dot = tint.gamma_multiply(prefs.intensity * 1.4);
                for y in 0..rows.min(80) {
                    for x in 0..cols.min(140) {
                        let base =
                            rect.min + egui::vec2(x as f32 * step + 11.0, y as f32 * step + 11.0);
                        let k = self
                            .influence
                            .map_or(0.0, |c| (1.0 - base.distance(c) / 150.0).max(0.0));
                        let pos = if let Some(c) = self.influence {
                            let d = base - c;
                            base + d / d.length().max(1.0) * k * k * 6.0
                        } else {
                            base
                        };
                        p.circle_filled(
                            pos,
                            0.8 + k * 1.1,
                            dot.gamma_multiply((1.0 + k * 3.0).min(4.0)),
                        );
                    }
                }
            }
            Background::Off => {}
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
        let pose = self.pose(
            ui,
            response.id,
            rect.center(),
            prefs,
            mood,
            reduce,
            response.hovered(),
        );
        super::mascot::paint(&ui.painter_at(rect.expand(6.0)), rect, pose);
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
impl Identity {
    #[allow(clippy::too_many_arguments)]
    fn pose(
        &self,
        ui: &Ui,
        id: egui::Id,
        center: Pos2,
        prefs: &IdentityPreferences,
        mood: Mood,
        reduce: bool,
        hovered: bool,
    ) -> super::mascot::Pose {
        let motion = if prefs.mascot_motion && !reduce {
            prefs.mascot_intensity.max(0.2)
        } else {
            0.0
        };
        let live = motion > 0.0 && prefs.interactive && self.mascot_clock.active;
        let look = super::mascot::look(
            ui.ctx(),
            id.with("look"),
            center,
            self.previous_pointer,
            live,
        );
        let hover = if motion > 0.0 && prefs.interactive {
            super::motion::state(ui, id.with("curious"), hovered, super::motion::CONTROL)
        } else {
            0.0
        };
        super::mascot::Pose {
            look,
            t: if motion > 0.0 {
                self.mascot_clock.phase
            } else {
                0.0
            },
            hover,
            click: if motion > 0.0 {
                (self.now - self.mascot_click) as f32
            } else {
                99.0
            },
            mood_age: if motion > 0.0 {
                (self.now - self.mood.1) as f32
            } else {
                99.0
            },
            mood,
            motion,
        }
    }
    /// A larger, non-interactive Luma for empty states and the Companion.
    pub(super) fn portrait(
        &self,
        ui: &Ui,
        rect: Rect,
        prefs: &IdentityPreferences,
        mood: Mood,
        reduce: bool,
    ) {
        let id = ui
            .id()
            .with(("luma-portrait", rect.min.x as i32, rect.min.y as i32));
        let pose = self.pose(ui, id, rect.center(), prefs, mood, reduce, false);
        super::mascot::paint(&ui.painter_at(rect.expand(8.0)), rect, pose);
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
fn hash(x: i32, y: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841);
    h ^= h >> 13;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 16;
    (h & 0xffff) as f32 / 65535.0
}
fn value_noise(p: egui::Vec2) -> f32 {
    let (x0, y0) = (p.x.floor(), p.y.floor());
    let (fx, fy) = (p.x - x0, p.y - y0);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let (xi, yi) = (x0 as i32, y0 as i32);
    let a = hash(xi, yi);
    let b = hash(xi + 1, yi);
    let c = hash(xi, yi + 1);
    let d = hash(xi + 1, yi + 1);
    egui::lerp(egui::lerp(a..=b, sx)..=egui::lerp(c..=d, sx), sy)
}
/// Organic drifting field in 0..1: domain-warped fractal value noise.
fn flow(p: egui::Vec2, t: f32) -> f32 {
    let warp = egui::vec2(
        value_noise(p * 0.6 + egui::vec2(t * 0.11, 3.1)),
        value_noise(p * 0.6 + egui::vec2(7.4, -t * 0.09)),
    );
    let q = p + warp * 2.2 + egui::vec2(t * 0.05, -t * 0.03);
    let n = value_noise(q) * 0.55 + value_noise(q * 2.1) * 0.3 + value_noise(q * 4.3) * 0.15;
    ((n - 0.32) * 1.9).clamp(0.0, 1.0)
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
    fn dither_records_trail_and_click_ripples() {
        let ctx = egui::Context::default();
        let prefs = IdentityPreferences::default();
        assert_eq!(prefs.background, Background::Dither);
        let mut identity = Identity::default();
        let pos = Pos2::new(400.0, 300.0);
        for (i, events) in [
            vec![egui::Event::PointerMoved(pos)],
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        ]
        .into_iter()
        .enumerate()
        {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 650.0))),
                    time: Some(i as f64 * 0.05),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ctx| identity.begin(ctx, &prefs, false, true, Mood::Idle),
            );
        }
        let mut frame = super::super::dither::Frame {
            variant: super::super::dither::Variant::Field,
            time: 0.0,
            ink: egui::Rgba::WHITE,
            accent: egui::Rgba::WHITE,
            strength: 1.0,
            density: 0.5,
            pixel: 4.0,
            light: false,
            trail: [[0.0; 3]; super::super::dither::TRAIL],
            ripples: [[0.0; 3]; super::super::dither::RIPPLES],
        };
        identity.dither.fill(
            &mut frame,
            Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 650.0)),
            0.3,
        );
        assert!(frame.trail[0][2] < 1.0, "{:?}", frame.trail);
        assert!(frame.ripples[0][2] >= 0.0, "{:?}", frame.ripples);
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

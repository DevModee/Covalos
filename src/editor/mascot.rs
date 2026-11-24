//! Luma, the COVALOS companion: an original vector character drawn with egui meshes.
//!
//! A soft squircle pod wrapped by the COVALOS orbital ring. The pod turns to face the cursor
//! (eyes and contour lines shift with perspective), breathes while idle and changes its
//! expression, rim light and orbit rhythm with the agent state. Everything is procedural,
//! resolution independent and costs a few hundred vertices per frame.
use super::identity::Mood;
use super::motion::blend;
use eframe::egui::{self, Color32, Mesh, Painter, Pos2, Rect, Shape, Stroke, Vec2, vec2};
use std::f32::consts::{PI, TAU};

/// Everything the drawing needs. `t` is a monotonic phase in seconds that only advances
/// while motion is allowed, so a frozen phase renders a still, composed pose.
#[derive(Clone, Copy)]
pub(super) struct Pose {
    pub(super) look: Vec2,
    pub(super) t: f32,
    pub(super) hover: f32,
    /// Seconds since the last click, or a large value.
    pub(super) click: f32,
    /// Seconds since the mood last changed, drives one-shot reactions.
    pub(super) mood_age: f32,
    pub(super) mood: Mood,
    pub(super) motion: f32,
}

struct Palette {
    edge: Color32,
    body: Color32,
    light: Color32,
    rim: Color32,
    eye: Color32,
    ring: Color32,
    signal: Color32,
}

fn palette(mood: Mood) -> Palette {
    let theme = super::theme::colors();
    let dark = theme.canvas.r() < 128;
    let accent = theme.accent;
    let signal = match mood {
        Mood::Approval => theme.warning,
        Mood::Error => theme.error,
        Mood::Success => theme.success,
        _ => accent,
    };
    Palette {
        edge: if dark {
            Color32::from_rgb(20, 17, 30)
        } else {
            Color32::from_rgb(52, 42, 86)
        },
        body: if dark {
            Color32::from_rgb(44, 37, 70)
        } else {
            Color32::from_rgb(86, 70, 146)
        },
        light: blend(Color32::from_rgb(132, 112, 214), accent, 0.35),
        rim: signal,
        eye: Color32::from_rgb(246, 243, 255),
        ring: blend(accent, signal, 0.6),
        signal,
    }
}

/// Smoothly track where Luma should look. Stored per mascot instance in egui memory.
pub(super) fn look(
    ctx: &egui::Context,
    id: egui::Id,
    center: Pos2,
    pointer: Option<Pos2>,
    live: bool,
) -> Vec2 {
    let target = match pointer {
        Some(p) if live => {
            let d = p - center;
            let reach = 260.0;
            let len = d.length();
            if len < 1.0 {
                Vec2::ZERO
            } else {
                d / len * (len / reach).min(1.0).powf(0.7)
            }
        }
        _ => Vec2::ZERO,
    };
    let dt = ctx.input(|i| i.stable_dt).clamp(0.0, 0.1);
    ctx.data_mut(|d| {
        let current: Vec2 = d.get_temp(id).unwrap_or(target);
        let next = current + (target - current) * (1.0 - (-10.0 * dt).exp());
        d.insert_temp(id, next);
        next
    })
}

fn squircle(center: Pos2, w: f32, h: f32, a: f32) -> Pos2 {
    let (s, c) = a.sin_cos();
    let n = 2.0 / 3.4;
    center
        + vec2(
            w * c.signum() * c.abs().powf(n),
            h * s.signum() * s.abs().powf(n),
        )
}

/// Paint Luma inside `rect`. The character keeps its proportions and centres itself.
pub(super) fn paint(painter: &Painter, rect: Rect, pose: Pose) {
    let size = rect.width().min(rect.height());
    let pal = palette(pose.mood);
    let m = pose.motion;
    let t = pose.t;

    // Body dynamics: breathing, hover lift, click squash and stretch, error shake.
    let breath = (t * 1.7).sin() * 0.5 + 0.5;
    let click = if pose.click < 0.55 {
        let k = pose.click / 0.55;
        (k * PI).sin() * (1.0 - k)
    } else {
        0.0
    };
    let shake = if pose.mood == Mood::Error && pose.mood_age < 0.7 {
        (pose.mood_age * 46.0).sin() * (1.0 - pose.mood_age / 0.7) * size * 0.035
    } else {
        0.0
    };
    let hop = if pose.mood == Mood::Success && pose.mood_age < 0.8 {
        (pose.mood_age / 0.8 * PI).sin() * size * 0.06
    } else {
        0.0
    };
    let lift = (breath * 0.012 + pose.hover * 0.025) * size * m + click * size * 0.05 + hop;
    let center = rect.center() + vec2(shake, size * 0.04 - lift);
    let squash = 1.0 + click * 0.10 - pose.hover * 0.02 * m;
    let w = size * 0.33 * squash * (1.0 + breath * 0.012 * m);
    let h = size * 0.30 / squash * (1.0 - breath * 0.01 * m);

    // Facing direction. Moods steer the gaze on top of the cursor.
    let mut look = pose.look * 0.9;
    match pose.mood {
        Mood::Thinking => look = look * 0.4 + vec2(0.35, -0.55),
        Mood::Searching => look = vec2((t * 1.6).sin() * 0.8, 0.1 + look.y * 0.3),
        Mood::Coding => look = look * 0.5 + vec2(0.0, 0.35),
        Mood::Sleeping => look = vec2(0.0, 0.25),
        _ => {}
    }
    let yaw = look.x.clamp(-1.0, 1.0);
    let pitch = look.y.clamp(-1.0, 1.0);

    // Ground shadow.
    soft_ellipse(
        painter,
        rect.center() + vec2(0.0, size * 0.40),
        vec2(w * (0.95 - lift / size * 2.0), size * 0.045),
        Color32::from_black_alpha(70),
    );

    // Back half of the orbit ring, behind the pod.
    let ring_tilt = -0.32 + yaw * 0.22;
    let ring_speed = match pose.mood {
        Mood::Thinking | Mood::Searching => 2.4,
        Mood::Coding => 1.6,
        Mood::Sleeping => 0.15,
        Mood::Approval => 0.5,
        _ => 0.7,
    };
    let ring = Orbit {
        center: center + vec2(0.0, h * 0.05),
        rx: w * 1.42,
        ry: h * (0.34 + pitch * 0.12).abs().max(0.16),
        tilt: ring_tilt,
    };
    let moon_angle = t * ring_speed + click * 6.0 + 0.8;
    ring.stroke(
        painter,
        true,
        Stroke::new((size * 0.018).max(1.0), pal.ring.gamma_multiply(0.38)),
    );
    if moon_angle.sin() < 0.0 {
        moon(painter, ring.point(moon_angle), size, pal.signal, 0.55);
    }

    // Signal glow for moods that ask for attention.
    let pulse = (t * 3.2).sin() * 0.5 + 0.5;
    let glow = match pose.mood {
        Mood::Approval => 0.30 + pulse * 0.25,
        Mood::Error => 0.35,
        Mood::Success => (1.0 - pose.mood_age / 1.6).max(0.0) * 0.5 + 0.12,
        Mood::Thinking | Mood::Coding | Mood::Searching => 0.16 + pulse * 0.08,
        _ => 0.08 + pose.hover * 0.12,
    };
    soft_ellipse(
        painter,
        center,
        vec2(w * 1.6, h * 1.6),
        pal.signal.gamma_multiply(glow * 0.5),
    );

    // Pod body: a lit squircle mesh. Normal from the superellipse, light from the upper left
    // and slightly opposite the gaze, so turning reads as a real rotation.
    let mut mesh = Mesh::default();
    let rings = 8;
    let slices = 44;
    let light_dir = {
        let v = [-0.45 - yaw * 0.25, -0.65 - pitch * 0.2, 0.62];
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        [v[0] / l, v[1] / l, v[2] / l]
    };
    for ring_i in 0..=rings {
        let r = ring_i as f32 / rings as f32;
        let r_shape = r.powf(0.85);
        for s in 0..=slices {
            let a = s as f32 / slices as f32 * TAU;
            let edge = squircle(Pos2::ZERO, 1.0, 1.0, a);
            let nx = edge.x * r_shape;
            let ny = edge.y * r_shape;
            let nz = (1.0 - (nx * nx + ny * ny).min(1.0)).sqrt();
            let lambert = (nx * light_dir[0] + ny * light_dir[1] + nz * light_dir[2]).max(0.0);
            let rim = r.powi(7) * (0.55 + 0.45 * (ny * 0.5 + 0.5));
            let mut c = blend(pal.edge, pal.body, (0.35 + lambert * 0.9).min(1.0));
            c = blend(c, pal.light, (lambert.powi(6) * 0.75).min(1.0));
            c = blend(c, pal.rim, (rim * 0.55).min(1.0));
            mesh.colored_vertex(center + vec2(nx * w, ny * h), c);
        }
    }
    for ring_i in 0..rings {
        for s in 0..slices {
            let a = (ring_i * (slices + 1) + s) as u32;
            let b = a + (slices + 1) as u32;
            mesh.add_triangle(a, b, a + 1);
            mesh.add_triangle(a + 1, b, b + 1);
        }
    }
    painter.add(Shape::mesh(mesh));

    // Contour lines: meridian and equator, bent by the facing direction.
    let contour = Stroke::new((size * 0.008).max(0.6), pal.light.gamma_multiply(0.22));
    let meridian: Vec<Pos2> = (0..=24)
        .map(|i| {
            let v = -PI / 2.0 + i as f32 / 24.0 * PI;
            squircle_inset(center, w, h, yaw * 0.62 * v.cos(), v.sin())
        })
        .collect();
    painter.add(Shape::line(meridian, contour));
    let equator: Vec<Pos2> = (0..=24)
        .map(|i| {
            let u = -PI / 2.0 + i as f32 / 24.0 * PI;
            squircle_inset(center, w, h, u.sin(), pitch * 0.55 * u.cos() + 0.04)
        })
        .collect();
    painter.add(Shape::line(equator, contour));

    // Rim line.
    let outline: Vec<Pos2> = (0..64)
        .map(|i| squircle(center, w, h, i as f32 / 64.0 * TAU))
        .collect();
    painter.add(Shape::closed_line(
        outline,
        Stroke::new((size * 0.012).max(0.8), pal.rim.gamma_multiply(0.35 + glow)),
    ));

    // Face. Features slide across the surface with perspective foreshortening.
    let face = center + vec2(yaw * w * 0.42, pitch * h * 0.36 - h * 0.04);
    let spread = w * 0.30 * (1.0 - yaw.abs() * 0.28);
    let blink_cycle = (t * 0.27).fract();
    let blink = if m > 0.0 && blink_cycle > 0.965 {
        1.0 - ((blink_cycle - 0.965) / 0.035 * PI).sin()
    } else {
        1.0
    };
    let eye_w = size * 0.062;
    let eye_h = size * 0.135 * (1.0 + pose.hover * 0.08);
    // Soft rosy/lilac cheeks for warm, cozy expressions
    if matches!(
        pose.mood,
        Mood::Idle | Mood::Success | Mood::Approval | Mood::Sleeping | Mood::Error
    ) {
        let blush_color = Color32::from_rgba_unmultiplied(248, 150, 198, 72);
        for side in [-1.0_f32, 1.0] {
            let cheek = face + vec2(side * (spread + eye_w * 0.65), eye_h * 0.48);
            painter.circle_filled(cheek, eye_w * 0.58, blush_color);
        }
    }

    for side in [-1.0_f32, 1.0] {
        // The far eye shrinks slightly when Luma turns.
        let depth = 1.0 - (side * yaw).max(0.0) * 0.22;
        let c = face + vec2(side * spread, 0.0);
        let ew = eye_w * depth;
        match pose.mood {
            Mood::Sleeping => arc(
                painter,
                c + vec2(0.0, -eye_h * 0.05),
                ew * 0.9,
                0.0,
                PI,
                Stroke::new(ew * 0.42, pal.eye.gamma_multiply(0.85)),
            ),
            Mood::Success => arc(
                painter,
                c + vec2(0.0, eye_h * 0.14),
                ew * 1.0,
                PI,
                TAU,
                Stroke::new(ew * 0.52, pal.eye),
            ),
            Mood::Error => {
                // Dizzy spiral eyes (Coucou-inspired)
                let pts: Vec<Pos2> = (0..=18)
                    .map(|i| {
                        let frac = i as f32 / 18.0;
                        let angle = frac * TAU * 1.75 + t * 3.0 * side;
                        let rad = ew * 0.85 * frac;
                        c + vec2(angle.cos() * rad, angle.sin() * rad)
                    })
                    .collect();
                painter.add(Shape::line(pts, Stroke::new((ew * 0.28).max(1.0), pal.eye)));
            }
            Mood::Approval => {
                let r = ew * (0.82 + pulse * 0.08);
                painter.circle_filled(c, r, pal.eye);
                painter.circle_filled(c + vec2(-r * 0.3, -r * 0.35), r * 0.30, Color32::WHITE);
            }
            _ => {
                let narrow = if pose.mood == Mood::Coding { 0.62 } else { 1.0 };
                let eh = (eye_h * narrow * blink).max(ew * 0.5);
                let eye = Rect::from_center_size(c, vec2(ew, eh));
                painter.rect_filled(eye, ew * 0.5, pal.eye);
                // A small catch light keeps the pills alive.
                painter.circle_filled(
                    eye.center_top() + vec2(-ew * 0.12, ew * 0.55),
                    ew * 0.18,
                    Color32::from_white_alpha(220),
                );
            }
        }
    }

    // Subtle expressive mouth below the eyes
    let mouth_c = face + vec2(0.0, eye_h * 0.62);
    match pose.mood {
        Mood::Idle | Mood::Success => {
            arc(
                painter,
                mouth_c - vec2(0.0, eye_w * 0.22),
                eye_w * if pose.mood == Mood::Success { 0.72 } else { 0.48 },
                0.22 * PI,
                0.78 * PI,
                Stroke::new((eye_w * 0.26).max(1.0), pal.eye.gamma_multiply(0.85)),
            );
        }
        Mood::Approval => {
            painter.circle_stroke(
                mouth_c,
                eye_w * 0.28,
                Stroke::new((eye_w * 0.22).max(1.0), pal.eye.gamma_multiply(0.85)),
            );
        }
        Mood::Coding | Mood::Searching => {
            painter.line_segment(
                [mouth_c + vec2(-eye_w * 0.3, 0.0), mouth_c + vec2(eye_w * 0.3, 0.0)],
                Stroke::new((eye_w * 0.22).max(1.0), pal.eye.gamma_multiply(0.75)),
            );
        }
        _ => {}
    }

    // Floating mood badge in the top-left corner of Luma (Coucou-style thinking dots / amber OK dot)
    if size >= 24.0 {
        let badge_pos = center + vec2(-w * 0.78, -h * 0.72);
        match pose.mood {
            Mood::Thinking | Mood::Coding | Mood::Searching => {
                let badge_r = (size * 0.13).clamp(4.0, 9.5);
                painter.circle_filled(badge_pos, badge_r, pal.signal);
                for i in 0..3 {
                    let phase = ((t * 5.0 - i as f32 * 0.7).sin() * 0.5 + 0.5).clamp(0.25, 1.0);
                    let dx = (i as f32 - 1.0) * badge_r * 0.48;
                    painter.circle_filled(
                        badge_pos + vec2(dx, 0.0),
                        (badge_r * 0.18).max(1.0),
                        Color32::from_white_alpha((phase * 255.0) as u8),
                    );
                }
            }
            Mood::Approval => {
                let badge_r = (size * 0.11).clamp(3.5, 8.0) * (0.92 + pulse * 0.14);
                painter.circle_filled(badge_pos, badge_r, pal.signal);
                painter.circle_stroke(
                    badge_pos,
                    badge_r,
                    Stroke::new(1.0_f32, pal.edge),
                );
            }
            Mood::Sleeping => {
                let drift = ((t * 0.9).fract()) * size * 0.12;
                let alpha = (1.0 - (t * 0.9).fract()) * 0.85;
                painter.text(
                    badge_pos + vec2(w * 1.45, -drift),
                    egui::Align2::CENTER_CENTER,
                    "z",
                    egui::FontId::proportional((size * 0.18).clamp(8.0, 14.0)),
                    pal.eye.gamma_multiply(alpha),
                );
            }
            _ => {}
        }
    }

    // Front half of the orbit ring and its moon.
    ring.stroke(
        painter,
        false,
        Stroke::new((size * 0.022).max(1.0), pal.ring.gamma_multiply(0.85)),
    );
    if moon_angle.sin() >= 0.0 {
        moon(painter, ring.point(moon_angle), size, pal.signal, 1.0);
    }

    // State accessories.
    let head = center + vec2(0.0, -h * 1.18);
    match pose.mood {
        Mood::Thinking => {
            for i in 0..3 {
                let phase = t * 2.6 - i as f32 * 0.55;
                let rise = (phase.sin() * 0.5 + 0.5) * size * 0.04;
                painter.circle_filled(
                    head + vec2((i as f32 - 1.0) * size * 0.075 + w * 0.25, -rise),
                    size * (0.018 + i as f32 * 0.004),
                    pal.signal
                        .gamma_multiply(0.45 + 0.5 * (phase.sin() * 0.5 + 0.5)),
                );
            }
        }
        Mood::Coding => {
            // Three typing bars that grow and reset like lines of code being written.
            for i in 0..3 {
                let p = (t * 0.9 + i as f32 * 0.37).fract().powf(0.6);
                let y = head.y + i as f32 * size * 0.035 - size * 0.02;
                let x0 = center.x + w * 0.9;
                let len = size * (0.06 + 0.07 * p);
                painter.line_segment(
                    [egui::pos2(x0, y), egui::pos2(x0 + len, y)],
                    Stroke::new(size * 0.016, pal.signal.gamma_multiply(0.35 + 0.5 * p)),
                );
            }
        }
        Mood::Approval => {
            let c = head + vec2(w * 0.55, size * 0.02);
            let r = size * (0.035 + pulse * 0.012);
            painter.circle_filled(c, r, pal.signal.gamma_multiply(0.9));
            painter.circle_stroke(
                c,
                r + size * 0.03 * pulse,
                Stroke::new(1.0_f32, pal.signal.gamma_multiply(0.5 * (1.0 - pulse))),
            );
        }
        Mood::Success => {
            let age = pose.mood_age.min(1.6);
            let fade = (1.0 - age / 1.6).max(0.0);
            if fade > 0.0 {
                for i in 0..5 {
                    let a = -PI * 0.5 + (i as f32 - 2.0) * 0.55;
                    let d = size * (0.42 + age * 0.12);
                    sparkle(
                        painter,
                        center + vec2(a.cos(), a.sin()) * d,
                        size * 0.035 * (0.6 + fade),
                        pal.signal.gamma_multiply(fade),
                    );
                }
            }
        }
        Mood::Sleeping => {
            let font = egui::FontId::proportional(size * 0.13);
            for i in 0..2 {
                let k = (t * 0.35 + i as f32 * 0.5).fract();
                painter.text(
                    head + vec2(w * 0.7 + k * size * 0.08, -k * size * 0.16),
                    egui::Align2::CENTER_CENTER,
                    "z",
                    font.clone(),
                    pal.light.gamma_multiply((1.0 - k) * 0.8),
                );
            }
        }
        _ => {}
    }
}

fn squircle_inset(center: Pos2, w: f32, h: f32, x: f32, y: f32) -> Pos2 {
    // Map a point in the unit disc onto the squircle so contour lines stay inside the pod.
    let r = (x * x + y * y).sqrt().min(1.0);
    if r < 1e-4 {
        return center;
    }
    let e = squircle(Pos2::ZERO, w, h, y.atan2(x));
    center + e.to_vec2() * r * 0.97
}

struct Orbit {
    center: Pos2,
    rx: f32,
    ry: f32,
    tilt: f32,
}
impl Orbit {
    fn point(&self, a: f32) -> Pos2 {
        let (s, c) = self.tilt.sin_cos();
        let x = a.cos() * self.rx;
        let y = a.sin() * self.ry;
        self.center + vec2(x * c - y * s, x * s + y * c)
    }
    fn stroke(&self, painter: &Painter, back: bool, stroke: Stroke) {
        // The front of the ellipse has sin(a) >= 0. A small opening stays on the back:
        // the COVALOS orbit is never a closed circle.
        let (from, to) = if back { (PI, TAU - 0.35) } else { (0.0, PI) };
        let points: Vec<Pos2> = (0..=36)
            .map(|i| self.point(from + (to - from) * i as f32 / 36.0))
            .collect();
        painter.add(Shape::line(points, stroke));
    }
}

fn moon(painter: &Painter, at: Pos2, size: f32, color: Color32, alpha: f32) {
    soft_ellipse(
        painter,
        at,
        Vec2::splat(size * 0.07),
        color.gamma_multiply(0.35 * alpha),
    );
    painter.circle_filled(
        at,
        size * 0.026,
        blend(color, Color32::WHITE, 0.35).gamma_multiply(alpha),
    );
}

fn arc(painter: &Painter, center: Pos2, r: f32, from: f32, to: f32, stroke: Stroke) {
    let points: Vec<Pos2> = (0..=12)
        .map(|i| {
            let a = from + (to - from) * i as f32 / 12.0;
            center + vec2(a.cos(), a.sin()) * r
        })
        .collect();
    painter.add(Shape::line(points, stroke));
}

fn sparkle(painter: &Painter, c: Pos2, r: f32, color: Color32) {
    let k = r * 0.28;
    painter.add(Shape::convex_polygon(
        vec![
            c + vec2(0.0, -r),
            c + vec2(k, -k),
            c + vec2(r, 0.0),
            c + vec2(k, k),
            c + vec2(0.0, r),
            c + vec2(-k, k),
            c + vec2(-r, 0.0),
            c + vec2(-k, -k),
        ],
        color,
        Stroke::NONE,
    ));
}

pub(super) fn soft_ellipse(painter: &Painter, center: Pos2, radius: Vec2, color: Color32) {
    if color.a() == 0 {
        return;
    }
    let mut mesh = Mesh::default();
    mesh.colored_vertex(center, color);
    for i in 0..=36 {
        let a = i as f32 / 36.0 * TAU;
        mesh.colored_vertex(
            center + vec2(a.cos() * radius.x, a.sin() * radius.y),
            Color32::TRANSPARENT,
        );
    }
    for i in 1..=36 {
        mesh.add_triangle(0, i, i + 1);
    }
    painter.add(Shape::mesh(mesh));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_mood_paints_finite_geometry_inside_its_box() {
        let ctx = egui::Context::default();
        for mood in [
            Mood::Idle,
            Mood::Thinking,
            Mood::Coding,
            Mood::Searching,
            Mood::Approval,
            Mood::Success,
            Mood::Error,
            Mood::Sleeping,
        ] {
            let out = ctx.run(egui::RawInput::default(), |ctx| {
                let painter = ctx.layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    egui::Id::new("luma-test"),
                ));
                let rect = Rect::from_min_size(egui::pos2(20.0, 20.0), vec2(96.0, 96.0));
                let pose = Pose {
                    look: vec2(0.7, -0.4),
                    t: 3.3,
                    hover: 1.0,
                    click: 0.2,
                    mood_age: 0.3,
                    mood,
                    motion: 1.0,
                };
                paint(&painter, rect, pose);
            });
            let mut vertices = 0;
            for p in ctx.tessellate(out.shapes, 1.0) {
                if let egui::epaint::Primitive::Mesh(mesh) = p.primitive {
                    for v in &mesh.vertices {
                        assert!(v.pos.x.is_finite() && v.pos.y.is_finite(), "{mood:?}");
                        assert!(v.pos.x > -10.0 && v.pos.x < 150.0, "{mood:?} x {}", v.pos.x);
                    }
                    vertices += mesh.vertices.len();
                }
            }
            assert!(vertices > 200, "{mood:?} painted too little");
        }
    }
}

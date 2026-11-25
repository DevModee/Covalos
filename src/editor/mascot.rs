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
    eye: Color32,
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
    // Coucou-inspired minimalist soft squircle body: luminous pastel-tinted body with
    // crisp dark eyes on dark themes, and rich theme-accented body with light eyes on light themes.
    Palette {
        edge: if dark {
            blend(theme.text, accent, 0.34)
        } else {
            blend(accent, theme.text, 0.25)
        },
        body: if dark {
            blend(Color32::from_rgb(246, 246, 252), accent, 0.14)
        } else {
            blend(accent, theme.surface, 0.12)
        },
        light: if dark {
            Color32::from_rgb(255, 255, 255)
        } else {
            blend(accent, Color32::WHITE, 0.35)
        },
        eye: if dark {
            Color32::from_rgb(22, 20, 30)
        } else {
            Color32::from_rgb(250, 250, 255)
        },
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
    let lift = (breath * 0.015 + pose.hover * 0.025) * size * m + click * size * 0.05 + hop;
    let center = rect.center() + vec2(shake, size * 0.02 - lift);
    let squash = 1.0 + click * 0.09 - pose.hover * 0.02 * m;
    let w = size * 0.36 * squash * (1.0 + breath * 0.012 * m);
    let h = size * 0.29 / squash * (1.0 - breath * 0.01 * m);

    // Facing direction: eyes smoothly track the mouse cursor, with gentle mood overrides.
    let mut look = pose.look;
    match pose.mood {
        Mood::Thinking => look = look * 0.45 + vec2(0.28, -0.42),
        Mood::Searching => look = vec2((t * 1.8).sin() * 0.75, 0.05 + look.y * 0.3),
        Mood::Sleeping => look = vec2(0.0, 0.2),
        _ => {}
    }
    let yaw = look.x.clamp(-1.0, 1.0);
    let pitch = look.y.clamp(-1.0, 1.0);

    // Soft ambient halo behind Luma (Coucou-style gentle glow).
    let pulse = (t * 3.0).sin() * 0.5 + 0.5;
    let glow = match pose.mood {
        Mood::Approval => 0.22 + pulse * 0.16,
        Mood::Error => 0.24,
        Mood::Success => 0.20,
        Mood::Thinking | Mood::Coding | Mood::Searching => 0.14 + pulse * 0.06,
        _ => 0.09 + pose.hover * 0.10,
    };
    soft_ellipse(
        painter,
        center,
        vec2(w * 1.45, h * 1.45),
        pal.signal.gamma_multiply(glow * 0.45),
    );

    // Minimalist floating little paws (waves on hover or greeting, like Coucou's mascot!)
    if size >= 34.0 {
        let paw_r = (size * 0.058).clamp(2.4, 5.5);
        // Left resting little round paw
        let left_bob = if pose.mood == Mood::Coding {
            (t * 9.0).sin() * paw_r * 0.45
        } else {
            (t * 1.7 + 0.5).sin() * paw_r * 0.25 * m
        };
        painter.circle_filled(
            center + vec2(-w * 1.24, h * 0.38 + left_bob),
            paw_r,
            pal.body,
        );
        // Right waving paw (raises & waves gently on hover or Success, bobs when Coding)
        let waving = pose.hover > 0.15 || pose.mood == Mood::Success;
        if waving {
            let wave = (t * 7.5).sin() * 0.38 * m;
            let base = center + vec2(w * 1.18, h * 0.18);
            let tip = base
                + vec2(
                    paw_r * 1.35 * (0.8 + wave * 0.3),
                    -paw_r * 1.45 + wave * paw_r,
                );
            painter.line_segment([base, tip], Stroke::new(paw_r * 1.65, pal.body));
        } else {
            let right_bob = if pose.mood == Mood::Coding {
                (t * 9.0 + PI).sin() * paw_r * 0.45
            } else {
                (t * 1.7 + 1.2).sin() * paw_r * 0.25 * m
            };
            painter.circle_filled(
                center + vec2(w * 1.24, h * 0.38 + right_bob),
                paw_r,
                pal.body,
            );
        }
    }

    // Minimalist plump squircle body with soft vertical gradient (clean, no noisy grid lines).
    let mut mesh = Mesh::default();
    let rings = 6;
    let slices = 44;
    for ring_i in 0..=rings {
        let r = ring_i as f32 / rings as f32;
        for s in 0..=slices {
            let a = s as f32 / slices as f32 * TAU;
            let edge = squircle(Pos2::ZERO, 1.0, 1.0, a);
            let nx = edge.x * r;
            let ny = edge.y * r;
            // Smooth top-to-bottom tint: brighter at top, gentle theme accent toward bottom.
            let v_grad = ((ny + 1.0) * 0.5).clamp(0.0, 1.0);
            let mut c = blend(pal.light, pal.body, (v_grad * 0.65).min(1.0));
            c = blend(c, pal.edge, (v_grad.powi(2) * 0.55).min(1.0));
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

    // Face: eyes smoothly track the mouse cursor across the clean squircle surface.
    let face = center + vec2(yaw * w * 0.34, pitch * h * 0.28);
    let spread = w * 0.28 * (1.0 - yaw.abs() * 0.16);
    let blink_cycle = (t * 0.26).fract();
    let blink = if m > 0.0 && blink_cycle > 0.965 {
        1.0 - ((blink_cycle - 0.965) / 0.035 * PI).sin()
    } else {
        1.0
    };
    let eye_w = (size * 0.064).max(1.8);
    let eye_h = (size * 0.132 * (1.0 + pose.hover * 0.06)).max(2.6);
    // Soft rosy cheeks when happy, idle on hover, or dizzy
    if pose.hover > 0.2 || matches!(pose.mood, Mood::Success | Mood::Error) {
        let blush_color = Color32::from_rgba_unmultiplied(244, 132, 178, 86);
        for side in [-1.0_f32, 1.0] {
            let cheek = face + vec2(side * (spread + eye_w * 0.72), eye_h * 0.44);
            painter.circle_filled(cheek, eye_w * 0.52, blush_color);
        }
    }

    for side in [-1.0_f32, 1.0] {
        let depth = 1.0 - (side * yaw).max(0.0) * 0.14;
        let c = face + vec2(side * spread, 0.0);
        let ew = eye_w * depth;
        match pose.mood {
            Mood::Sleeping => arc(
                painter,
                c + vec2(0.0, -eye_h * 0.05),
                ew * 0.9,
                0.0,
                PI,
                Stroke::new((ew * 0.40).max(1.2), pal.eye.gamma_multiply(0.85)),
            ),
            Mood::Success => arc(
                painter,
                c + vec2(0.0, eye_h * 0.12),
                ew * 0.95,
                PI,
                TAU,
                Stroke::new((ew * 0.48).max(1.4), pal.eye),
            ),
            Mood::Error => {
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
                let r = ew * (0.82 + pulse * 0.06);
                painter.circle_filled(c, r, pal.eye);
                painter.circle_filled(c + vec2(-r * 0.28, -r * 0.30), r * 0.26, Color32::WHITE);
            }
            _ => {
                let narrow = if pose.mood == Mood::Coding { 0.68 } else { 1.0 };
                let eh = (eye_h * narrow * blink).max(ew * 0.45);
                let eye = Rect::from_center_size(c, vec2(ew, eh));
                painter.rect_filled(eye, ew * 0.5, pal.eye);
            }
        }
    }

    // Minimalist pet mouth (only when smiling on hover/success or surprised on approval)
    let mouth_c = face + vec2(0.0, eye_h * 0.58);
    if pose.hover > 0.25 || pose.mood == Mood::Success {
        arc(
            painter,
            mouth_c - vec2(0.0, eye_w * 0.22),
            eye_w * 0.52,
            0.20 * PI,
            0.80 * PI,
            Stroke::new((eye_w * 0.24).max(1.0), pal.eye.gamma_multiply(0.85)),
        );
    } else if pose.mood == Mood::Approval {
        painter.circle_stroke(
            mouth_c,
            eye_w * 0.24,
            Stroke::new((eye_w * 0.20).max(1.0), pal.eye.gamma_multiply(0.85)),
        );
    }

    // Coucou-style top-left mood badge (3 animated dots when working, amber dot for OK, z when sleeping)
    if size >= 24.0 {
        let badge_pos = center + vec2(-w * 0.80, -h * 0.72);
        match pose.mood {
            Mood::Thinking | Mood::Coding | Mood::Searching => {
                let badge_r = (size * 0.135).clamp(4.2, 9.5);
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
                let badge_r = (size * 0.115).clamp(3.8, 8.2) * (0.92 + pulse * 0.14);
                painter.circle_filled(badge_pos, badge_r, pal.signal);
            }
            Mood::Sleeping => {
                let drift = ((t * 0.9).fract()) * size * 0.12;
                let alpha = (1.0 - (t * 0.9).fract()) * 0.85;
                painter.text(
                    badge_pos + vec2(w * 1.45, -drift),
                    egui::Align2::CENTER_CENTER,
                    "z",
                    egui::FontId::proportional((size * 0.18).clamp(8.0, 14.0)),
                    pal.body.gamma_multiply(alpha),
                );
            }
            _ => {}
        }
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

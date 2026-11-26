//! Dot-matrix loaders: a compact grid glyph and a capsule with a shimmering label.

use std::f32::consts::TAU;
use std::time::Duration;

use eframe::egui::{self, Color32, FontId, Pos2, Sense, Stroke, StrokeKind, Vec2, text::LayoutJob};

use super::theme::{self, BODY};

/// Frame used when motion is off, so the glyph still reads as a pattern.
const STATIC_TIME: f32 = 0.35;
/// Alpha of an unlit dot relative to the tint.
const UNLIT_ALPHA: f32 = 0.09;
const CAPSULE_HEIGHT: f32 = 34.0;
const CAPSULE_GRID: f32 = 18.0;
const CAPSULE_PAD: f32 = 12.0;
const CAPSULE_GAP: f32 = 10.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LoaderPattern {
    Ring,
    Diamond,
    Wave,
    Pulse,
    Orbit,
    Blocks,
}

impl LoaderPattern {
    pub(super) const ALL: [Self; 6] = [
        Self::Ring,
        Self::Diamond,
        Self::Wave,
        Self::Pulse,
        Self::Orbit,
        Self::Blocks,
    ];
}

/// Brightness 0..=1 for each cell of a `grid` x `grid` matrix (row-major) at `t` seconds.
pub(super) fn pattern_cells(pattern: LoaderPattern, grid: usize, t: f32) -> Vec<f32> {
    let center = (grid as f32 - 1.0) * 0.5;
    let mut cells = Vec::with_capacity(grid * grid);
    for row in 0..grid {
        for col in 0..grid {
            let dx = col as f32 - center;
            let dy = row as f32 - center;
            let value = match pattern {
                LoaderPattern::Ring => ring(dx, dy, center, t),
                LoaderPattern::Diamond => {
                    // Expanding and contracting diamond outline.
                    let radius = (0.55 + 0.45 * (t * 2.6).sin()) * center;
                    (1.0 - ((dx.abs() + dy.abs()) - radius).abs()).max(0.0)
                }
                LoaderPattern::Wave => {
                    let phase = (col + row) as f32 / grid as f32 * TAU - t * 4.0;
                    0.5 + 0.5 * phase.sin()
                }
                LoaderPattern::Pulse => {
                    // The center cell is always lit; the block breathes around it.
                    let radius = (0.5 + 0.5 * (t * 3.0).sin()) * center + 0.3;
                    1.0 - (dx.abs().max(dy.abs()) - radius).clamp(0.0, 1.0)
                }
                LoaderPattern::Orbit => orbit(dx, dy, center, t),
                LoaderPattern::Blocks => {
                    let seed = hash01(col as u32, row as u32);
                    let wave = 0.5 + 0.5 * (t * 2.2 + seed * TAU).sin();
                    smoothstep(0.35, 0.85, wave)
                }
            };
            cells.push(value.clamp(0.0, 1.0));
        }
    }
    cells
}

/// Rotating ring: a bright head with a fading tail on the outer circle.
fn ring(dx: f32, dy: f32, center: f32, t: f32) -> f32 {
    let radius = center * 0.85;
    let on_ring = (1.0 - ((dx.hypot(dy) - radius).abs() / 0.9)).max(0.0);
    let behind = ((t * TAU * 0.7 - dy.atan2(dx)) / TAU).rem_euclid(1.0);
    on_ring * (1.0 - behind).powi(2)
}

/// A single dot circling the grid, followed by a short trail.
fn orbit(dx: f32, dy: f32, center: f32, t: f32) -> f32 {
    let radius = center * 0.85;
    (0..5)
        .map(|k| {
            let angle = t * TAU * 0.8 - k as f32 * 0.5;
            let (hx, hy) = (angle.cos() * radius, angle.sin() * radius);
            let dist2 = (dx - hx).powi(2) + (dy - hy).powi(2);
            (-dist2 / 0.45).exp() * (1.0 - k as f32 / 5.0)
        })
        .fold(0.0, f32::max)
}

fn smoothstep(lo: f32, hi: f32, x: f32) -> f32 {
    let x = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Deterministic per-cell value in 0..1 so the block pattern is stable across frames.
fn hash01(x: u32, y: u32) -> f32 {
    let mut h = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xFFFF) as f32 / 65535.0
}

/// Animation time in seconds. When motion is off (zero animation time or an
/// unfocused window) this is a fixed frame and no repaint is requested;
/// otherwise the next frame is scheduled at ~30fps rather than continuously.
fn frame_time(ui: &egui::Ui) -> f32 {
    let animate = ui.style().animation_time > 0.0 && ui.input(|i| i.focused);
    if animate {
        ui.ctx().request_repaint_after(Duration::from_millis(33));
        ui.input(|i| i.time) as f32
    } else {
        STATIC_TIME
    }
}

fn paint_grid(
    painter: &egui::Painter,
    rect: egui::Rect,
    pattern: LoaderPattern,
    tint: Color32,
    t: f32,
) {
    // Seven dots only fit once the glyph is large enough to keep them distinct.
    let grid = if rect.width() >= 28.0 { 7 } else { 5 };
    let pitch = rect.width() / grid as f32;
    let radius = pitch * 0.3;
    for (index, level) in pattern_cells(pattern, grid, t).into_iter().enumerate() {
        let center = Pos2::new(
            rect.left() + (index % grid) as f32 * pitch + pitch * 0.5,
            rect.top() + (index / grid) as f32 * pitch + pitch * 0.5,
        );
        if level > 0.05 {
            painter.circle_filled(center, radius * 1.9, tint.gamma_multiply(level * 0.16));
        }
        let alpha = UNLIT_ALPHA + (1.0 - UNLIT_ALPHA) * level;
        painter.circle_filled(center, radius, tint.gamma_multiply(alpha));
    }
}

/// Compact grid-only glyph, a drop-in replacement for a small spinner.
pub(super) fn dot_glyph(
    ui: &mut egui::Ui,
    size: f32,
    pattern: LoaderPattern,
    tint: Color32,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, false, "Loading"));
    let t = frame_time(ui);
    if ui.is_rect_visible(rect) {
        paint_grid(ui.painter(), rect, pattern, tint, t);
    }
    response
}

/// Capsule with a dot grid and a label whose letters catch a sweeping highlight.
pub(super) fn dot_loader(
    ui: &mut egui::Ui,
    label: &str,
    pattern: LoaderPattern,
    tint: Color32,
) -> egui::Response {
    let colors = theme::colors();
    let t = frame_time(ui);
    let galley = ui.painter().layout_job(shimmer_job(label, t, colors));
    let width = CAPSULE_PAD * 2.0 + CAPSULE_GRID + CAPSULE_GAP + galley.size().x;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, CAPSULE_HEIGHT), Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, false, label));
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect(
            rect,
            CAPSULE_HEIGHT * 0.5,
            colors.inset,
            Stroke::new(1.0_f32, colors.border.gamma_multiply(0.6)),
            StrokeKind::Inside,
        );
        let grid_rect = egui::Rect::from_min_size(
            Pos2::new(
                rect.left() + CAPSULE_PAD,
                rect.center().y - CAPSULE_GRID * 0.5,
            ),
            Vec2::splat(CAPSULE_GRID),
        );
        paint_grid(painter, grid_rect, pattern, tint, t);
        let text_pos = Pos2::new(
            grid_rect.right() + CAPSULE_GAP,
            rect.center().y - galley.size().y * 0.5,
        );
        painter.galley(text_pos, galley, colors.text);
    }
    response
}

/// One section per character so each can carry its own blend of secondary and text color.
fn shimmer_job(label: &str, t: f32, colors: theme::Palette) -> LayoutJob {
    let count = label.chars().count() as f32;
    // The band starts and ends off the text so the sweep loops without a pop.
    let head = (t * 0.55).fract() * (count + 8.0) - 4.0;
    let mut job = LayoutJob::default();
    for (index, ch) in label.chars().enumerate() {
        let weight = (-((index as f32 - head) / 2.2).powi(2)).exp();
        let color = colors.secondary.lerp_to_gamma(colors.text, weight);
        job.append(
            ch.encode_utf8(&mut [0; 4]),
            0.0,
            egui::TextFormat::simple(FontId::proportional(BODY), color),
        );
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_count_and_range_hold_for_all_patterns() {
        for pattern in LoaderPattern::ALL {
            for grid in [5, 7] {
                for step in 0..40 {
                    let cells = pattern_cells(pattern, grid, step as f32 * 0.073);
                    assert_eq!(cells.len(), grid * grid, "{pattern:?} {grid}");
                    assert!(cells.iter().all(|v| (0.0..=1.0).contains(v)), "{pattern:?}");
                }
            }
        }
    }

    #[test]
    fn every_pattern_animates() {
        for pattern in LoaderPattern::ALL {
            for grid in [5, 7] {
                assert_ne!(
                    pattern_cells(pattern, grid, 0.0),
                    pattern_cells(pattern, grid, 0.5),
                    "{pattern:?} {grid}"
                );
            }
        }
    }

    #[test]
    fn ring_leaves_the_center_dark() {
        for step in 0..20 {
            let cells = pattern_cells(LoaderPattern::Ring, 7, step as f32 * 0.11);
            assert_eq!(cells[3 * 7 + 3], 0.0);
        }
    }

    #[test]
    fn pulse_keeps_the_center_lit() {
        for step in 0..20 {
            let cells = pattern_cells(LoaderPattern::Pulse, 7, step as f32 * 0.11);
            assert!(cells[3 * 7 + 3] > 0.99);
        }
    }

    #[test]
    fn ring_lights_something() {
        let cells = pattern_cells(LoaderPattern::Ring, 7, 0.3);
        assert!(cells.iter().any(|v| *v > 0.5));
    }
}

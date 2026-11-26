//! Dot-matrix score donut.
//! Four score areas drawn as square dots on a ring, with a value list and a per-axis
//! evolution column chart. Everything is painted by hand so no data is invented: an
//! axis without a score shows `--` and very dim dots.
use std::f32::consts::TAU;

use eframe::egui::{
    self, Align2, Color32, CursorIcon, FontId, Painter, Pos2, Rect, Sense, Shape, Stroke, Vec2,
    WidgetInfo, WidgetType, pos2, vec2,
};

use super::super::theme::{self, BODY, DISPLAY, SMALL};

const BLOCK_HEIGHT: f32 = 260.0;
const WIDE_MIN_WIDTH: f32 = 420.0;
const ROW_HEIGHT: f32 = 28.0;
const HISTORY_COLUMNS: usize = 10;
const HISTORY_DOT_ROWS: usize = 8;
const HISTORY_PITCH: f32 = 6.0;
const HISTORY_HEIGHT: f32 = 20.0 + HISTORY_DOT_ROWS as f32 * HISTORY_PITCH;
const HOLE_RATIO: f32 = 0.5;
const DOT_PITCH: f32 = 5.0;
const DOT_SIZE: f32 = 3.2;
const ANIM_SECONDS: f32 = 0.18;
const MARKER_MARGIN: f32 = 26.0;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct ChartAxis {
    pub(super) label: String,
    /// 0..=100, `None` when no graded session exists yet.
    pub(super) score: Option<f32>,
    pub(super) color: Color32,
    /// Past session scores, oldest first, 0..=100.
    pub(super) history: Vec<f32>,
}

/// Fraction of the ring thickness that is lit for a score, 0 when there is no data.
pub(super) fn lit_fraction(score: Option<f32>) -> f32 {
    score.map_or(0.0, |s| (s / 100.0).clamp(0.0, 1.0))
}

/// Angle in radians, clockwise from straight up, in `0..TAU`.
fn angle_from_top(center: Pos2, pos: Pos2) -> f32 {
    let d = pos - center;
    let a = d.x.atan2(-d.y);
    if a < 0.0 { a + TAU } else { a }
}

fn polar(center: Pos2, radius: f32, angle: f32) -> Pos2 {
    pos2(
        center.x + radius * angle.sin(),
        center.y - radius * angle.cos(),
    )
}

/// Sector under `pos`, or `None` in the center hole and outside the ring.
pub(super) fn sector_at(
    center: Pos2,
    inner: f32,
    outer: f32,
    sectors: usize,
    pos: Pos2,
) -> Option<usize> {
    if sectors == 0 {
        return None;
    }
    let dist = (pos - center).length();
    if dist < inner || dist > outer {
        return None;
    }
    let step = TAU / sectors as f32;
    Some(((angle_from_top(center, pos) / step) as usize).min(sectors - 1))
}

pub(super) fn score_donut(
    ui: &mut egui::Ui,
    id_salt: &str,
    axes: &[ChartAxis],
    selected: &mut Option<usize>,
) -> egui::Response {
    let colors = theme::colors();
    let base = ui.make_persistent_id(id_salt);
    if selected.is_some_and(|i| i >= axes.len()) {
        *selected = None;
    }

    // One block for the whole widget; inner hit areas are placed by hand so the donut
    // can be painted after the rows have reported their hover.
    let width = ui.available_width();
    let wide = width >= WIDE_MIN_WIDTH;
    let side = 220.0_f32.min(BLOCK_HEIGHT - 20.0);
    let list_height = axes.len() as f32 * ROW_HEIGHT + 12.0 + HISTORY_HEIGHT;
    let height = if wide {
        BLOCK_HEIGHT.max(list_height)
    } else {
        side + 12.0 + list_height
    };
    let (block, mut response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let donut_rect = if wide {
        Rect::from_min_size(
            block.min + vec2(0.0, (height - side) / 2.0),
            Vec2::splat(side),
        )
    } else {
        Rect::from_min_size(
            pos2(block.center().x - side / 2.0, block.top()),
            Vec2::splat(side),
        )
    };
    let list_rect = if wide {
        let left = donut_rect.right() + 24.0;
        Rect::from_min_max(
            pos2(left, block.top() + (height - list_height) / 2.0),
            pos2(block.right().max(left + 1.0), block.bottom()),
        )
    } else {
        Rect::from_min_max(
            pos2(block.left(), donut_rect.bottom() + 12.0),
            block.right_bottom(),
        )
    };

    let center = donut_rect.center();
    let outer = side / 2.0 - MARKER_MARGIN;
    let inner = outer * HOLE_RATIO;

    let donut_resp = ui.interact(donut_rect, base.with("donut"), Sense::click());
    let donut_hover = donut_resp
        .hover_pos()
        .and_then(|p| sector_at(center, inner, outer, axes.len(), p));
    let row_rects: Vec<Rect> = (0..axes.len())
        .map(|i| {
            Rect::from_min_size(
                list_rect.min + vec2(0.0, i as f32 * ROW_HEIGHT),
                vec2(list_rect.width(), ROW_HEIGHT - 2.0),
            )
        })
        .collect();
    let row_resps: Vec<egui::Response> = row_rects
        .iter()
        .enumerate()
        .map(|(i, r)| ui.interact(*r, base.with(("row", i)), Sense::click()))
        .collect();
    let row_hover = row_resps.iter().position(|r| r.hovered());

    // Hover is a temporary override of the persistent selection.
    let hover = donut_hover.or(row_hover);
    let active = hover.or(*selected);
    if hover.is_some() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    if donut_resp.clicked()
        && let Some(i) = donut_hover
    {
        toggle(selected, i);
        response.mark_changed();
    }
    for (i, r) in row_resps.iter().enumerate() {
        if r.clicked() {
            toggle(selected, i);
            response.mark_changed();
        }
    }

    let anim: Vec<f32> = (0..axes.len())
        .map(|i| {
            ui.ctx().animate_bool_with_time(
                base.with(("sector", i)),
                active == Some(i),
                ANIM_SECONDS,
            )
        })
        .collect();

    let painter = ui.painter_at(block);
    painter.rect_filled(block, 10.0, colors.inset);
    draw_donut(
        &painter,
        center,
        inner,
        outer,
        axes,
        &anim,
        active.is_some(),
    );
    draw_markers(&painter, center, outer, &colors);
    draw_center(&painter, center, inner, axes, active, &colors);

    for (i, axis) in axes.iter().enumerate() {
        draw_row(
            &painter,
            row_rects[i],
            axis,
            anim[i],
            active.is_some(),
            &colors,
        );
    }
    let hist_top = list_rect.top() + axes.len() as f32 * ROW_HEIGHT + 12.0;
    let hist_rect = Rect::from_min_max(
        pos2(list_rect.left(), hist_top),
        pos2(list_rect.right(), hist_top + HISTORY_HEIGHT),
    );
    draw_history(
        ui,
        base,
        hist_rect,
        active.and_then(|i| axes.get(i)),
        &colors,
    );

    response.widget_info(|| {
        let text = match selected.and_then(|i| axes.get(i)) {
            Some(a) => match a.score {
                Some(s) => format!("Score areas, selected {}: {:.0} of 100", a.label, s),
                None => format!("Score areas, selected {}: no data yet", a.label),
            },
            None => "Score areas, nothing selected".to_owned(),
        };
        WidgetInfo::labeled(WidgetType::Other, true, text)
    });
    response
}

fn toggle(selected: &mut Option<usize>, i: usize) {
    *selected = if *selected == Some(i) { None } else { Some(i) };
}

fn draw_donut(
    painter: &Painter,
    center: Pos2,
    inner: f32,
    outer: f32,
    axes: &[ChartAxis],
    anim: &[f32],
    any_active: bool,
) {
    let n = axes.len();
    if n == 0 {
        return;
    }
    let step = TAU / n as f32;
    let cells = (outer / DOT_PITCH).ceil() as i32 + 1;
    for gy in -cells..=cells {
        for gx in -cells..=cells {
            // Half-pitch offset keeps the grid symmetric around the center.
            let p = center + vec2((gx as f32 + 0.5) * DOT_PITCH, (gy as f32 + 0.5) * DOT_PITCH);
            let r = (p - center).length();
            if r < inner || r > outer {
                continue;
            }
            let angle = angle_from_top(center, p);
            let k = ((angle / step) as usize).min(n - 1);
            let off = angle - k as f32 * step;
            // Blank lane between sectors so the four areas read as separate.
            if r * off.min(step - off).sin() < 1.8 {
                continue;
            }
            let axis = &axes[k];
            let t = anim[k];
            let radial = (r - inner) / (outer - inner);
            let color = if axis.score.is_none() {
                axis.color.gamma_multiply(0.07 + 0.03 * t)
            } else if radial <= lit_fraction(axis.score) {
                let emphasis = if any_active { 0.4 + 0.6 * t } else { 0.8 };
                axis.color.gamma_multiply(emphasis)
            } else {
                axis.color.gamma_multiply(0.10 + 0.06 * t)
            };
            painter.rect_filled(Rect::from_center_size(p, Vec2::splat(DOT_SIZE)), 0.5, color);
        }
    }

    // Thin arc with end ticks around the highlighted segment only.
    for (k, axis) in axes.iter().enumerate() {
        let t = anim[k];
        if t < 0.01 {
            continue;
        }
        let color = axis.color.gamma_multiply(t);
        let stroke = Stroke::new(1.2_f32, color);
        let (a0, a1) = (k as f32 * step + 0.03, (k + 1) as f32 * step - 0.03);
        let arc_r = outer + 6.0;
        let segments = ((a1 - a0) / 0.04).ceil().max(2.0) as usize;
        let points = (0..=segments)
            .map(|s| polar(center, arc_r, a0 + (a1 - a0) * s as f32 / segments as f32))
            .collect();
        painter.add(Shape::line(points, stroke));
        for a in [a0, a1] {
            painter.line_segment(
                [
                    polar(center, outer + 2.0, a),
                    polar(center, outer + 10.0, a),
                ],
                stroke,
            );
        }
    }
}

fn draw_markers(painter: &Painter, center: Pos2, outer: f32, colors: &theme::Palette) {
    let font = FontId::monospace(9.5);
    for (label, quarter) in [("00", 0.0), ("25", 1.0), ("50", 2.0), ("75", 3.0)] {
        let a = quarter * TAU / 4.0;
        painter.line_segment(
            [
                polar(center, outer + 11.0, a),
                polar(center, outer + 15.0, a),
            ],
            Stroke::new(1.0_f32, colors.muted.gamma_multiply(0.7)),
        );
        painter.text(
            polar(center, outer + 22.0, a),
            Align2::CENTER_CENTER,
            label,
            font.clone(),
            colors.muted,
        );
    }
}

/// Letter-spaced monospace text wrapped on spaces into at most two lines.
fn draw_spaced(painter: &Painter, center: Pos2, text: &str, max_width: f32, color: Color32) {
    let font = FontId::monospace(9.5);
    let spacing = 1.2;
    let advance = painter
        .layout_no_wrap("M".into(), font.clone(), color)
        .size()
        .x
        + spacing;
    let per_line = ((max_width / advance) as usize).max(3);
    let mut lines: Vec<String> = vec![String::new()];
    for word in text.to_uppercase().split_whitespace() {
        let cur = lines.last_mut().expect("lines starts non-empty");
        let needed = cur.chars().count() + usize::from(!cur.is_empty()) + word.chars().count();
        if needed <= per_line {
            if !cur.is_empty() {
                cur.push(' ');
            }
            cur.push_str(word);
        } else if lines.len() < 2 {
            lines.push(word.to_owned());
        } else {
            let last = lines.last_mut().expect("two lines");
            let keep = per_line.saturating_sub(1);
            *last = last.chars().take(keep).collect::<String>() + ".";
            break;
        }
    }
    let line_h = 12.0;
    let top = center.y - (lines.len() as f32 - 1.0) * line_h / 2.0;
    for (li, line) in lines.iter().enumerate() {
        let count = line.chars().count().min(per_line);
        let mut x = center.x - count as f32 * advance / 2.0 + advance / 2.0;
        for c in line.chars().take(per_line) {
            painter.text(
                pos2(x, top + li as f32 * line_h),
                Align2::CENTER_CENTER,
                c,
                font.clone(),
                color,
            );
            x += advance;
        }
    }
}

fn draw_center(
    painter: &Painter,
    center: Pos2,
    inner: f32,
    axes: &[ChartAxis],
    active: Option<usize>,
    colors: &theme::Palette,
) {
    let (name, score) = match active.and_then(|i| axes.get(i)) {
        Some(a) => (a.label.clone(), a.score),
        None => {
            // Mean of graded areas only; nothing graded means nothing to show.
            let graded: Vec<f32> = axes.iter().filter_map(|a| a.score).collect();
            let mean =
                (!graded.is_empty()).then(|| graded.iter().sum::<f32>() / graded.len() as f32);
            ("Overall".to_owned(), mean)
        }
    };
    draw_spaced(
        painter,
        center + vec2(0.0, -24.0),
        &name,
        inner * 1.7,
        colors.muted,
    );
    let value = score.map_or_else(
        || "--".to_owned(),
        |s| format!("{:.0}", s.clamp(0.0, 100.0)),
    );
    let big = painter.layout_no_wrap(value, FontId::monospace(DISPLAY * 0.8), colors.text);
    let small = painter.layout_no_wrap("/100".into(), FontId::monospace(SMALL), colors.muted);
    let total = big.size().x + 3.0 + small.size().x;
    let left = center.x - total / 2.0;
    let y = center.y + 6.0;
    let baseline_gap = big.size().y - small.size().y;
    painter.galley(pos2(left, y - big.size().y / 2.0), big.clone(), colors.text);
    painter.galley(
        pos2(
            left + big.size().x + 3.0,
            y + baseline_gap / 2.0 - small.size().y / 2.0 + 2.0,
        ),
        small,
        colors.muted,
    );
}

fn draw_row(
    painter: &Painter,
    rect: Rect,
    axis: &ChartAxis,
    t: f32,
    any_active: bool,
    colors: &theme::Palette,
) {
    // Inactive rows dim only while another row is highlighted.
    let dim = if any_active { 0.5 + 0.5 * t } else { 1.0 };
    if t > 0.01 {
        painter.rect_filled(rect, 6.0, colors.elevated.gamma_multiply(t));
    }
    let icon_color = if axis.score.is_some() {
        axis.color.gamma_multiply(dim)
    } else {
        axis.color.gamma_multiply(0.25)
    };
    let icon_origin = pos2(rect.left() + 12.0, rect.center().y - 4.0);
    for gy in 0..3 {
        for gx in 0..3 {
            let p = icon_origin + vec2(gx as f32 * 4.0, gy as f32 * 4.0);
            painter.rect_filled(Rect::from_center_size(p, Vec2::splat(2.4)), 0.4, icon_color);
        }
    }
    painter.text(
        pos2(rect.left() + 34.0, rect.center().y),
        Align2::LEFT_CENTER,
        &axis.label,
        FontId::proportional(BODY),
        colors.text.gamma_multiply(dim),
    );
    let (value, value_color) = match axis.score {
        Some(s) => (
            format!("{:.0}", s.clamp(0.0, 100.0)),
            colors.text.gamma_multiply(dim),
        ),
        None => ("--".to_owned(), colors.muted.gamma_multiply(dim)),
    };
    painter.text(
        pos2(rect.right() - 12.0, rect.center().y),
        Align2::RIGHT_CENTER,
        value,
        FontId::monospace(BODY),
        value_color,
    );
}

fn draw_history(
    ui: &mut egui::Ui,
    base: egui::Id,
    rect: Rect,
    axis: Option<&ChartAxis>,
    colors: &theme::Palette,
) {
    let painter = ui.painter_at(rect.expand(2.0));
    let note = |text: &str| {
        let galley = painter.layout(
            text.to_owned(),
            FontId::proportional(SMALL),
            colors.muted,
            rect.width(),
        );
        painter.galley(rect.min + vec2(0.0, 4.0), galley, colors.muted);
    };
    let Some(axis) = axis else {
        note("Select an area to see how it evolves");
        return;
    };
    if axis.history.len() < 2 {
        note("Evolution appears after two graded sessions");
        return;
    }
    let start = axis.history.len().saturating_sub(HISTORY_COLUMNS);
    let shown = &axis.history[start..];
    painter.text(
        rect.min,
        Align2::LEFT_TOP,
        format!("EVOLUTION  LAST {}", shown.len()),
        FontId::monospace(9.5),
        colors.muted,
    );
    let slot = (rect.width() / HISTORY_COLUMNS as f32).clamp(10.0, 22.0);
    let bottom = rect.bottom();
    for (k, &score) in shown.iter().enumerate() {
        let score = score.clamp(0.0, 100.0);
        let x = rect.left() + (k as f32 + 0.5) * slot;
        let lit = (lit_fraction(Some(score)) * HISTORY_DOT_ROWS as f32).round() as usize;
        for row in 0..HISTORY_DOT_ROWS {
            let y = bottom - (row as f32 + 0.5) * HISTORY_PITCH;
            let color = if row < lit {
                axis.color.gamma_multiply(0.9)
            } else {
                axis.color.gamma_multiply(0.12)
            };
            for dx in [-2.0, 2.0] {
                painter.rect_filled(
                    Rect::from_center_size(pos2(x + dx, y), Vec2::splat(3.2)),
                    0.5,
                    color,
                );
            }
        }
        let col = Rect::from_min_max(
            pos2(
                x - slot / 2.0,
                bottom - HISTORY_DOT_ROWS as f32 * HISTORY_PITCH,
            ),
            pos2(x + slot / 2.0, bottom),
        );
        ui.interact(col, base.with(("hist", k)), Sense::hover())
            .on_hover_text(format!("Session {}: {:.0} / 100", start + k + 1, score));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: Pos2 = pos2(100.0, 100.0);

    #[test]
    fn sector_at_maps_quadrants_clockwise_from_top() {
        let at = |dx: f32, dy: f32| sector_at(C, 20.0, 50.0, 4, C + vec2(dx, dy));
        assert_eq!(at(30.0, -30.0), Some(0));
        assert_eq!(at(30.0, 30.0), Some(1));
        assert_eq!(at(-30.0, 30.0), Some(2));
        assert_eq!(at(-30.0, -30.0), Some(3));
    }

    #[test]
    fn sector_at_rejects_hole_outside_and_empty() {
        assert_eq!(sector_at(C, 20.0, 50.0, 4, C), None);
        assert_eq!(sector_at(C, 20.0, 50.0, 4, C + vec2(10.0, 0.0)), None);
        assert_eq!(sector_at(C, 20.0, 50.0, 4, C + vec2(60.0, 0.0)), None);
        assert_eq!(sector_at(C, 20.0, 50.0, 0, C + vec2(30.0, 0.0)), None);
    }

    #[test]
    fn lit_fraction_clamps_and_handles_none() {
        assert_eq!(lit_fraction(None), 0.0);
        assert_eq!(lit_fraction(Some(-5.0)), 0.0);
        assert_eq!(lit_fraction(Some(50.0)), 0.5);
        assert_eq!(lit_fraction(Some(250.0)), 1.0);
    }

    fn axis(score: Option<f32>, history: Vec<f32>) -> ChartAxis {
        ChartAxis {
            label: "Code & correctness".to_owned(),
            score,
            color: Color32::from_rgb(240, 160, 40),
            history,
        }
    }

    fn render(axes: &[ChartAxis], mut sel: Option<usize>) {
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                score_donut(ui, "t", axes, &mut sel);
            });
        });
    }

    #[test]
    fn renders_without_panicking() {
        render(&[], None);
        let empty = vec![axis(None, vec![]); 4];
        render(&empty, Some(0));
        let full = vec![
            axis(Some(82.0), (1..=12).map(|i| i as f32 * 8.0).collect()),
            axis(Some(0.0), vec![10.0]),
            axis(Some(100.0), vec![50.0, 60.0]),
            axis(None, vec![]),
        ];
        render(&full, Some(2));
        render(&full, Some(9));
    }
}

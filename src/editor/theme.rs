use eframe::egui::{self, Color32, Context, FontId, Frame, Margin, RichText, Stroke, TextStyle};

pub(crate) const BRAND: &str = "AEROS";
pub(crate) const TITLE: &str = "AEROS Editor";
pub(super) const CANVAS: Color32 = Color32::from_rgb(11, 11, 15);
pub(super) const SURFACE: Color32 = Color32::from_rgb(17, 17, 22);
pub(super) const INSET: Color32 = Color32::from_rgb(23, 23, 30);
pub(super) const ELEVATED: Color32 = Color32::from_rgb(30, 29, 39);
pub(super) const BORDER: Color32 = Color32::from_rgb(43, 41, 53);
pub(super) const TEXT: Color32 = Color32::from_rgb(240, 238, 245);
pub(super) const MUTED: Color32 = Color32::from_rgb(162, 158, 175);
pub(super) const ACCENT: Color32 = Color32::from_rgb(167, 139, 250);
pub(super) const ACCENT_HOVER: Color32 = Color32::from_rgb(188, 167, 255);
pub(super) const ACCENT_BG: Color32 = Color32::from_rgb(44, 34, 70);
pub(super) const SUCCESS: Color32 = Color32::from_rgb(133, 209, 173);
pub(super) const ERROR: Color32 = Color32::from_rgb(247, 156, 160);
pub(super) const PANEL_RADIUS: u8 = 12;

pub(super) fn install(ctx: &Context, font_size: f32, high_contrast: bool) {
    let mut style = egui::Style::default();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.interact_size.y = 27.0;
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(14.0));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(12.0));
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(21.0));
    style
        .text_styles
        .insert(TextStyle::Monospace, FontId::monospace(font_size));
    let visuals = &mut style.visuals;
    *visuals = egui::Visuals::dark();
    let border = if high_contrast { MUTED } else { BORDER };
    visuals.override_text_color = Some(TEXT);
    visuals.panel_fill = CANVAS;
    visuals.window_fill = SURFACE;
    visuals.window_stroke = Stroke::new(1.0_f32, border);
    visuals.window_corner_radius = 16.into();
    visuals.extreme_bg_color = INSET;
    visuals.faint_bg_color = SURFACE;
    visuals.selection.bg_fill = ACCENT_BG;
    visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.hyperlink_color = ACCENT_HOVER;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, border);
    for widget in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = 8.into();
        widget.bg_stroke = Stroke::new(1.0_f32, border);
        widget.fg_stroke = Stroke::new(1.0_f32, TEXT);
        widget.bg_fill = ELEVATED;
        widget.weak_bg_fill = ELEVATED;
    }
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT_HOVER);
    visuals.widgets.hovered.weak_bg_fill = ACCENT_BG;
    ctx.set_style(style);
}

pub(super) fn panel_frame(margin: Margin) -> Frame {
    Frame::new()
        .fill(SURFACE)
        .inner_margin(margin)
        .stroke(Stroke::new(1.0_f32, BORDER))
}

pub(super) fn card() -> Frame {
    Frame::new()
        .fill(SURFACE)
        .inner_margin(Margin::same(14))
        .corner_radius(PANEL_RADIUS)
        .stroke(Stroke::new(1.0_f32, BORDER))
}

pub(super) fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text).small().color(MUTED)
}

pub(super) fn primary(text: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(text).strong().color(TEXT))
        .fill(ACCENT_BG)
        .stroke(Stroke::new(1.0_f32, ACCENT))
}

#[derive(Clone, Copy)]
pub(super) enum Icon {
    Brand,
    Explorer,
    Search,
    Git,
    Hub,
    Settings,
}

pub(super) fn icon_button(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    selected: bool,
) -> egui::Response {
    let response = ui.add(
        egui::Button::new("")
            .selected(selected)
            .min_size(egui::vec2(35.0, 35.0)),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    let color = if selected || response.hovered() {
        ACCENT_HOVER
    } else {
        MUTED
    };
    draw_icon(ui.painter(), response.rect.center(), icon, color);
    response.on_hover_text(label)
}

pub(super) fn draw_icon(painter: &egui::Painter, center: egui::Pos2, icon: Icon, color: Color32) {
    let stroke = Stroke::new(1.55_f32, color);
    let point = |x, y| center + egui::vec2(x, y);
    let line = |a, b| painter.line_segment([a, b], stroke);
    let outline = |points: &[(f32, f32)]| {
        painter.add(egui::Shape::closed_line(
            points.iter().map(|&(x, y)| point(x, y)).collect(),
            stroke,
        ));
    };
    match icon {
        Icon::Brand => {
            outline(&[(0.0, -8.0), (7.0, 0.0), (0.0, 8.0), (-7.0, 0.0)]);
            outline(&[(0.0, -4.2), (3.5, 0.0), (0.0, 4.2), (-3.5, 0.0)]);
        }
        Icon::Explorer => {
            outline(&[
                (-8.0, -5.0),
                (-2.0, -5.0),
                (0.0, -2.5),
                (8.0, -2.5),
                (8.0, 6.5),
                (-8.0, 6.5),
            ]);
        }
        Icon::Search => {
            painter.circle_stroke(point(-2.0, -2.0), 5.3, stroke);
            line(point(2.0, 2.0), point(7.0, 7.0));
        }
        Icon::Git => {
            line(point(-4.5, -4.0), point(-4.5, 4.0));
            painter.add(egui::Shape::line(
                vec![
                    point(-4.5, 2.0),
                    point(3.8, 2.0),
                    point(5.0, 0.0),
                    point(5.0, -2.0),
                ],
                stroke,
            ));
            for (x, y) in [(-4.5, -6.0), (-4.5, 6.0), (5.0, -4.0)] {
                painter.circle_stroke(point(x, y), 2.0, stroke);
            }
        }
        Icon::Hub => {
            outline(&[
                (0.0, -8.0),
                (7.0, -4.0),
                (7.0, 4.0),
                (0.0, 8.0),
                (-7.0, 4.0),
                (-7.0, -4.0),
            ]);
            painter.circle_filled(center, 2.0, color);
        }
        Icon::Settings => {
            painter.circle_stroke(center, 5.8, stroke);
            painter.circle_stroke(center, 2.4, stroke);
            for i in 0..8 {
                let angle = i as f32 * std::f32::consts::TAU / 8.0;
                let direction = egui::vec2(angle.cos(), angle.sin());
                line(center + direction * 5.9, center + direction * 8.1);
            }
        }
    }
}

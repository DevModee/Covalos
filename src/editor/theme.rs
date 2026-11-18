use eframe::egui::{self, Color32, Context, FontId, Frame, Margin, RichText, Stroke, TextStyle};

pub(crate) const BRAND: &str = "AEROS";
pub(crate) const TITLE: &str = "AEROS Editor";
pub(super) const THEME_NAME: &str = "Orbital";
pub(super) const CANVAS: Color32 = Color32::from_rgb(16, 19, 24);
pub(super) const SURFACE: Color32 = Color32::from_rgb(21, 24, 30);
pub(super) const INSET: Color32 = Color32::from_rgb(18, 21, 27);
pub(super) const ELEVATED: Color32 = Color32::from_rgb(29, 34, 43);
pub(super) const HOVER: Color32 = Color32::from_rgb(35, 40, 51);
pub(super) const BORDER: Color32 = Color32::from_rgb(41, 47, 57);
pub(super) const CONTROL_BORDER: Color32 = Color32::from_rgb(65, 73, 89);
pub(super) const TEXT: Color32 = Color32::from_rgb(229, 233, 240);
pub(super) const SECONDARY: Color32 = Color32::from_rgb(162, 172, 186);
pub(super) const MUTED: Color32 = Color32::from_rgb(135, 147, 164);
pub(super) const DISABLED: Color32 = Color32::from_rgb(103, 113, 128);
pub(super) const ACCENT: Color32 = Color32::from_rgb(176, 154, 250);
pub(super) const ACCENT_HOVER: Color32 = Color32::from_rgb(199, 183, 255);
pub(super) const ACCENT_BG: Color32 = Color32::from_rgb(41, 38, 60);
pub(super) const SUCCESS: Color32 = Color32::from_rgb(137, 202, 171);
pub(super) const ERROR: Color32 = Color32::from_rgb(239, 147, 154);
pub(super) const WARNING: Color32 = Color32::from_rgb(224, 187, 128);
pub(super) const SYNTAX_STRING: Color32 = Color32::from_rgb(173, 204, 164);
pub(super) const SYNTAX_NUMBER: Color32 = Color32::from_rgb(224, 183, 144);

pub(super) const UNIT: f32 = 4.0;
pub(super) const SMALL: f32 = 12.0;
pub(super) const BODY: f32 = 13.0;
pub(super) const LEAD: f32 = 15.0;
pub(super) const HEADING: f32 = 20.0;
pub(super) const DISPLAY: f32 = 40.0;
pub(super) const CHROME_HEIGHT: f32 = 44.0;
pub(super) const RAIL_WIDTH: f32 = 48.0;
pub(super) const TAB_HEIGHT: f32 = 36.0;
pub(super) const BREADCRUMB_HEIGHT: f32 = 28.0;
pub(super) const STATUS_HEIGHT: f32 = 25.0;
pub(super) const PANEL_PADDING: i8 = 12;
pub(super) const CONTROL_RADIUS: u8 = 6;
pub(super) const CHECKBOX_RADIUS: u8 = 2;
pub(super) const PANEL_RADIUS: u8 = 10;
pub(super) const MODAL_RADIUS: u8 = 12;
pub(super) const HOVER_SECONDS: f32 = 0.10;
pub(super) const MIN_EDITOR_WIDTH: f32 = 320.0;
pub(super) const CHAT_COMPOSER_HEIGHT: f32 = 132.0;
pub(super) const CODE_LINE_HEIGHT: f32 = 1.4;
pub(super) const COMPACT_CODE_LINE_HEIGHT: f32 = 1.25;

pub(super) fn install_fonts(ctx: &Context) {
    let mut definitions = egui::FontDefinitions::default();
    definitions.font_data.insert(
        "Geist".into(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../../assets/fonts/Geist.ttf"
        ))),
    );
    definitions.font_data.insert(
        "IBM Plex Mono".into(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../../assets/fonts/IBMPlexMono.ttf"
        ))),
    );
    definitions
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "Geist".into());
    definitions
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, "IBM Plex Mono".into());
    ctx.set_fonts(definitions);
}

pub(super) fn install(ctx: &Context, font_size: f32, high_contrast: bool) {
    let mut style = egui::Style::default();
    style.spacing.item_spacing = egui::vec2(8.0, 4.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.interact_size.y = 28.0;
    style.spacing.window_margin = Margin::same(20);
    style.spacing.menu_margin = Margin::same(6);
    style.spacing.indent = 16.0;
    style.spacing.scroll.bar_width = 6.0;
    style.spacing.scroll.floating_width = 3.0;
    style.animation_time = HOVER_SECONDS;
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(BODY));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(BODY));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(SMALL));
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(HEADING));
    style
        .text_styles
        .insert(TextStyle::Monospace, FontId::monospace(font_size));
    let visuals = &mut style.visuals;
    *visuals = egui::Visuals::dark();
    let border = if high_contrast { SECONDARY } else { BORDER };
    visuals.override_text_color = Some(TEXT);
    visuals.weak_text_color = Some(MUTED);
    visuals.panel_fill = CANVAS;
    visuals.window_fill = SURFACE;
    visuals.window_stroke = Stroke::new(
        1.0_f32,
        if high_contrast {
            SECONDARY
        } else {
            CONTROL_BORDER
        },
    );
    visuals.window_corner_radius = MODAL_RADIUS.into();
    visuals.window_highlight_topmost = false;
    visuals.menu_corner_radius = CONTROL_RADIUS.into();
    visuals.extreme_bg_color = INSET;
    visuals.faint_bg_color = SURFACE;
    visuals.selection.bg_fill = ACCENT_BG;
    visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.hyperlink_color = ACCENT_HOVER;
    visuals.indent_has_left_vline = true;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, border);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    visuals.disabled_alpha = 0.5;
    for widget in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = CONTROL_RADIUS.into();
        widget.bg_stroke = Stroke::new(
            1.0_f32,
            if high_contrast {
                SECONDARY
            } else {
                CONTROL_BORDER
            },
        );
        widget.fg_stroke = Stroke::new(1.0_f32, TEXT);
        widget.bg_fill = ELEVATED;
        widget.weak_bg_fill = Color32::TRANSPARENT;
    }
    visuals.widgets.hovered.bg_fill = HOVER;
    visuals.widgets.hovered.weak_bg_fill = HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, MUTED);
    visuals.widgets.active.bg_fill = ACCENT_BG;
    visuals.widgets.active.weak_bg_fill = ACCENT_BG;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.widgets.open.weak_bg_fill = ACCENT_BG;
    ctx.set_style(style);
}

pub(super) fn panel_frame(margin: Margin) -> Frame {
    Frame::new().fill(SURFACE).inner_margin(margin)
}

pub(super) fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text).size(SMALL).color(MUTED)
}

pub(super) fn secondary(text: impl Into<String>) -> RichText {
    RichText::new(text).size(BODY).color(SECONDARY)
}

pub(super) fn primary(text: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(text).strong().color(CANVAS))
        .fill(ACCENT)
        .stroke(Stroke::NONE)
        .corner_radius(CONTROL_RADIUS)
}

pub(super) fn checkbox(
    ui: &mut egui::Ui,
    checked: &mut bool,
    label: impl Into<egui::WidgetText>,
) -> egui::Response {
    ui.scope(|ui| {
        let visuals = &mut ui.style_mut().visuals.widgets;
        for state in [
            &mut visuals.inactive,
            &mut visuals.hovered,
            &mut visuals.active,
            &mut visuals.open,
        ] {
            state.corner_radius = CHECKBOX_RADIUS.into();
        }
        ui.checkbox(checked, label)
    })
    .inner
}

pub(super) fn busy_indicator(ui: &mut egui::Ui) -> egui::Response {
    if ui.input(|input| input.focused) && ui.style().animation_time > 0.0 {
        return ui.add(egui::Spinner::new().color(ACCENT).size(14.0));
    }
    let (rect, response) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
    ui.painter()
        .circle_stroke(rect.center(), 4.0, Stroke::new(1.5_f32, MUTED));
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, false, "Working"));
    response
}

pub(super) fn section(ui: &mut egui::Ui, title: &str, description: &str) {
    ui.label(RichText::new(title).size(LEAD).strong());
    if !description.is_empty() {
        ui.add(egui::Label::new(secondary(description)).wrap());
    }
    ui.add_space(UNIT * 3.0);
}

pub(super) fn row(ui: &mut egui::Ui, label: &str, detail: &str, selected: bool) -> egui::Response {
    let height = ui.spacing().interact_size.y.max(24.0);
    let response = ui.add_sized(
        [ui.available_width(), height],
        egui::Button::new("").frame(false).selected(selected),
    );
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::SelectableLabel, true, label));
    let painter = ui.painter_at(response.rect);
    let fill = if selected {
        ACCENT_BG
    } else if response.hovered() {
        HOVER
    } else {
        Color32::TRANSPARENT
    };
    painter.rect_filled(response.rect, 4, fill);
    if selected {
        painter.rect_filled(
            egui::Rect::from_min_size(
                response.rect.left_top() + egui::vec2(0.0, 4.0),
                egui::vec2(2.0, height - 8.0),
            ),
            1,
            ACCENT,
        );
    }
    let detail_width = if detail.is_empty() { 0.0 } else { 104.0 };
    let max_width = (response.rect.width() - 24.0 - detail_width).max(1.0);
    let galley = ui.fonts_mut(|fonts| {
        fonts.layout_job(egui::text::LayoutJob {
            wrap: egui::text::TextWrapping {
                max_width,
                max_rows: 1,
                break_anywhere: true,
                ..Default::default()
            },
            ..egui::text::LayoutJob::simple(
                label.to_owned(),
                FontId::proportional(BODY),
                if selected { TEXT } else { SECONDARY },
                max_width,
            )
        })
    });
    painter.galley(
        response.rect.left_center() + egui::vec2(12.0, -galley.size().y / 2.0),
        galley,
        TEXT,
    );
    if !detail.is_empty() {
        painter.text(
            response.rect.right_center() - egui::vec2(12.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            detail,
            FontId::proportional(SMALL),
            MUTED,
        );
    }
    if response.has_focus() {
        painter.rect_stroke(
            response.rect,
            4,
            Stroke::new(1.0_f32, ACCENT),
            egui::StrokeKind::Inside,
        );
    }
    response
}

#[derive(Clone, Copy)]
pub(super) enum Icon {
    Brand,
    Explorer,
    Search,
    Git,
    Hub,
    Settings,
    Close,
    Add,
    Refresh,
    Minimize,
    Maximize,
    Restore,
    File,
    Code,
    Terminal,
}

pub(super) fn icon_button(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    selected: bool,
) -> egui::Response {
    let response = ui.add(
        egui::Button::new("")
            .frame(false)
            .selected(selected)
            .min_size(egui::vec2(32.0, 32.0)),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    let hover = response.hovered() || response.has_focus();
    let color = if selected || hover {
        ACCENT_HOVER
    } else {
        MUTED
    };
    let progress = ui.ctx().animate_bool_with_easing(
        response.id.with("hover"),
        hover,
        egui::emath::easing::cubic_out,
    );
    if selected || progress > 0.0 {
        ui.painter().rect_filled(
            response.rect.shrink(1.0),
            CONTROL_RADIUS,
            if selected {
                ACCENT_BG
            } else {
                HOVER.gamma_multiply(progress)
            },
        );
    }
    if selected {
        ui.painter().rect_filled(
            egui::Rect::from_min_size(
                response.rect.left_center() + egui::vec2(-5.0, -7.0),
                egui::vec2(2.0, 14.0),
            ),
            1,
            ACCENT,
        );
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect,
            CONTROL_RADIUS,
            Stroke::new(1.0_f32, ACCENT),
            egui::StrokeKind::Inside,
        );
    }
    draw_icon(ui.painter(), response.rect.center(), icon, color);
    response.on_hover_text(label)
}

pub(super) fn draw_icon(painter: &egui::Painter, center: egui::Pos2, icon: Icon, color: Color32) {
    let stroke = Stroke::new(1.4_f32, color);
    let p = |x, y| center + egui::vec2(x, y);
    let line = |a, b| {
        painter.line_segment([a, b], stroke);
    };
    let outline = |points: &[(f32, f32)]| {
        painter.add(egui::Shape::closed_line(
            points.iter().map(|&(x, y)| p(x, y)).collect(),
            stroke,
        ));
    };
    match icon {
        Icon::Brand => {
            outline(&[(-7.0, 5.0), (0.0, -8.0), (7.0, 5.0)]);
            line(p(-3.0, 0.5), p(3.0, 0.5));
            painter.add(egui::Shape::line(
                (0..25)
                    .map(|i| {
                        let a = i as f32 / 24.0 * std::f32::consts::PI * 1.55;
                        p(a.cos() * 10.0, a.sin() * 4.5 + 1.0)
                    })
                    .collect(),
                Stroke::new(1.1_f32, color),
            ));
        }
        Icon::Explorer => outline(&[
            (-8.0, -5.0),
            (-2.0, -5.0),
            (0.0, -2.5),
            (8.0, -2.5),
            (8.0, 6.5),
            (-8.0, 6.5),
        ]),
        Icon::Search => {
            painter.circle_stroke(p(-2.0, -2.0), 5.0, stroke);
            line(p(2.0, 2.0), p(7.0, 7.0));
        }
        Icon::Git => {
            line(p(-4.5, -4.0), p(-4.5, 4.0));
            painter.add(egui::Shape::line(
                vec![p(-4.5, 2.0), p(3.8, 2.0), p(5.0, 0.0), p(5.0, -2.0)],
                stroke,
            ));
            for (x, y) in [(-4.5, -6.0), (-4.5, 6.0), (5.0, -4.0)] {
                painter.circle_stroke(p(x, y), 2.0, stroke);
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
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let v = egui::vec2(a.cos(), a.sin());
                line(center + v * 5.9, center + v * 8.1);
            }
        }
        Icon::Close => {
            line(p(-4.0, -4.0), p(4.0, 4.0));
            line(p(-4.0, 4.0), p(4.0, -4.0));
        }
        Icon::Add => {
            line(p(-5.0, 0.0), p(5.0, 0.0));
            line(p(0.0, -5.0), p(0.0, 5.0));
        }
        Icon::Refresh => {
            painter.add(egui::Shape::line(
                (0..22)
                    .map(|i| {
                        let a = i as f32 / 21.0 * 5.0;
                        p(a.cos() * 6.0, a.sin() * 6.0)
                    })
                    .collect(),
                stroke,
            ));
            line(p(6.0, 0.0), p(6.0, -5.0));
            line(p(6.0, 0.0), p(1.0, 0.0));
        }
        Icon::Minimize => line(p(-5.0, 2.0), p(5.0, 2.0)),
        Icon::Maximize => outline(&[(-4.5, -4.5), (4.5, -4.5), (4.5, 4.5), (-4.5, 4.5)]),
        Icon::Restore => {
            outline(&[(-5.0, -2.0), (2.0, -2.0), (2.0, 5.0), (-5.0, 5.0)]);
            line(p(-2.0, -5.0), p(5.0, -5.0));
            line(p(5.0, -5.0), p(5.0, 2.0));
        }
        Icon::File => outline(&[
            (-5.0, -7.0),
            (1.0, -7.0),
            (5.0, -3.0),
            (5.0, 7.0),
            (-5.0, 7.0),
        ]),
        Icon::Code => {
            line(p(-3.0, -4.0), p(-7.0, 0.0));
            line(p(-7.0, 0.0), p(-3.0, 4.0));
            line(p(3.0, -4.0), p(7.0, 0.0));
            line(p(7.0, 0.0), p(3.0, 4.0));
        }
        Icon::Terminal => {
            outline(&[(-8.0, -6.0), (8.0, -6.0), (8.0, 6.0), (-8.0, 6.0)]);
            line(p(-4.0, -2.0), p(-1.0, 0.0));
            line(p(-1.0, 0.0), p(-4.0, 2.0));
            line(p(1.0, 3.0), p(5.0, 3.0));
        }
    }
}

pub(super) fn file_color(path: &std::path::Path) -> Color32 {
    match path.extension().and_then(|e| e.to_str()) {
        Some("rs" | "toml") => SYNTAX_NUMBER,
        Some("js" | "ts" | "tsx") => WARNING,
        Some("md") => SECONDARY,
        _ => MUTED,
    }
}

pub(super) fn syntax_color(color: Color32, italic: bool) -> Color32 {
    let [r, g, b, _] = color.to_array();
    if italic || r.max(g).max(b) < 145 {
        MUTED
    } else if r.abs_diff(g) < 20 && g.abs_diff(b) < 20 {
        TEXT
    } else if b > r || (b > g && r > g) {
        ACCENT_HOVER
    } else if g > r && g > b {
        SYNTAX_STRING
    } else {
        SYNTAX_NUMBER
    }
}

pub(super) fn modal_frame() -> Frame {
    Frame::new()
        .fill(SURFACE)
        .inner_margin(Margin::same(20))
        .corner_radius(MODAL_RADIUS)
        .stroke(Stroke::new(1.0_f32, CONTROL_BORDER))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(color: Color32) -> f64 {
        let channel = |v: u8| {
            let value = f64::from(v) / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
    }

    #[test]
    fn normal_text_tokens_keep_readable_contrast() {
        for (foreground, background) in [
            (TEXT, CANVAS),
            (TEXT, SURFACE),
            (SECONDARY, SURFACE),
            (MUTED, SURFACE),
            (ACCENT, ACCENT_BG),
            (SYNTAX_STRING, CANVAS),
            (SYNTAX_NUMBER, CANVAS),
        ] {
            let a = luminance(foreground);
            let b = luminance(background);
            let ratio = (a.max(b) + 0.05) / (a.min(b) + 0.05);
            assert!(ratio >= 4.5, "Insufficient text contrast: {ratio:.2}");
        }
    }
}

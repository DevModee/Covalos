pub(super) use super::i18n::{tr, trf};
use eframe::egui::{self, Color32, Context, FontId, Frame, Margin, RichText, Stroke, TextStyle};

pub(crate) const BRAND: &str = "AEROS";
pub(crate) const TITLE: &str = "AEROS Editor";

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
pub(super) const MODAL_RADIUS: u8 = 12;
pub(super) const HOVER_SECONDS: f32 = super::motion::HOVER;
pub(super) const MIN_EDITOR_WIDTH: f32 = 320.0;
pub(super) const CHAT_COMPOSER_HEIGHT: f32 = 132.0;
pub(super) const CODE_LINE_HEIGHT: f32 = 1.4;
pub(super) const COMPACT_CODE_LINE_HEIGHT: f32 = 1.25;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Theme {
    #[default]
    Dark,
    Midnight,
    Violet,
    Light,
    HighContrast,
}
impl Theme {
    pub(super) const ALL: [Self; 5] = [
        Self::Dark,
        Self::Midnight,
        Self::Violet,
        Self::Light,
        Self::HighContrast,
    ];
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Dark => "AEROS Dark",
            Self::Midnight => "AEROS Midnight",
            Self::Violet => "AEROS Violet",
            Self::Light => "AEROS Light",
            Self::HighContrast => "High Contrast",
        }
    }
    pub(super) fn is_dark(self) -> bool {
        self != Self::Light
    }
}
#[derive(Clone, Copy)]
pub(super) struct Palette {
    pub(super) canvas: Color32,
    pub(super) surface: Color32,
    pub(super) inset: Color32,
    pub(super) elevated: Color32,
    pub(super) hover: Color32,
    pub(super) border: Color32,
    pub(super) control_border: Color32,
    pub(super) text: Color32,
    pub(super) secondary: Color32,
    pub(super) muted: Color32,
    pub(super) disabled: Color32,
    pub(super) accent: Color32,
    pub(super) accent_hover: Color32,
    pub(super) accent_bg: Color32,
    pub(super) success: Color32,
    pub(super) error: Color32,
    pub(super) warning: Color32,
    pub(super) syntax_string: Color32,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) syntax_number: Color32,
}
impl Theme {
    pub(super) fn colors(self) -> Palette {
        match self {
            Self::Dark => Palette {
                canvas: Color32::from_rgb(16, 19, 24),
                surface: Color32::from_rgb(21, 24, 30),
                inset: Color32::from_rgb(18, 21, 27),
                elevated: Color32::from_rgb(29, 34, 43),
                hover: Color32::from_rgb(35, 40, 51),
                border: Color32::from_rgb(41, 47, 57),
                control_border: Color32::from_rgb(65, 73, 89),
                text: Color32::from_rgb(229, 233, 240),
                secondary: Color32::from_rgb(162, 172, 186),
                muted: Color32::from_rgb(135, 147, 164),
                disabled: Color32::from_rgb(103, 113, 128),
                accent: Color32::from_rgb(176, 154, 250),
                accent_hover: Color32::from_rgb(199, 183, 255),
                accent_bg: Color32::from_rgb(41, 38, 60),
                success: Color32::from_rgb(137, 202, 171),
                error: Color32::from_rgb(239, 147, 154),
                warning: Color32::from_rgb(224, 187, 128),
                syntax_string: Color32::from_rgb(173, 204, 164),
                syntax_number: Color32::from_rgb(224, 183, 144),
            },
            Self::Midnight => Palette {
                canvas: Color32::from_rgb(10, 15, 24),
                surface: Color32::from_rgb(15, 22, 34),
                inset: Color32::from_rgb(11, 18, 29),
                elevated: Color32::from_rgb(23, 32, 48),
                hover: Color32::from_rgb(29, 41, 60),
                border: Color32::from_rgb(34, 45, 63),
                control_border: Color32::from_rgb(59, 74, 96),
                text: Color32::from_rgb(229, 233, 240),
                secondary: Color32::from_rgb(162, 172, 186),
                muted: Color32::from_rgb(135, 147, 164),
                disabled: Color32::from_rgb(103, 113, 128),
                accent: Color32::from_rgb(145, 187, 255),
                accent_hover: Color32::from_rgb(181, 208, 255),
                accent_bg: Color32::from_rgb(28, 46, 72),
                success: Color32::from_rgb(137, 202, 171),
                error: Color32::from_rgb(239, 147, 154),
                warning: Color32::from_rgb(224, 187, 128),
                syntax_string: Color32::from_rgb(173, 204, 164),
                syntax_number: Color32::from_rgb(224, 183, 144),
            },
            Self::Violet => Palette {
                canvas: Color32::from_rgb(20, 16, 27),
                surface: Color32::from_rgb(27, 22, 35),
                inset: Color32::from_rgb(23, 19, 31),
                elevated: Color32::from_rgb(37, 30, 47),
                hover: Color32::from_rgb(46, 37, 59),
                border: Color32::from_rgb(51, 43, 63),
                control_border: Color32::from_rgb(77, 64, 94),
                text: Color32::from_rgb(229, 233, 240),
                secondary: Color32::from_rgb(162, 172, 186),
                muted: Color32::from_rgb(135, 147, 164),
                disabled: Color32::from_rgb(103, 113, 128),
                accent: Color32::from_rgb(195, 167, 253),
                accent_hover: Color32::from_rgb(199, 183, 255),
                accent_bg: Color32::from_rgb(49, 36, 67),
                success: Color32::from_rgb(137, 202, 171),
                error: Color32::from_rgb(239, 147, 154),
                warning: Color32::from_rgb(224, 187, 128),
                syntax_string: Color32::from_rgb(173, 204, 164),
                syntax_number: Color32::from_rgb(224, 183, 144),
            },
            Self::Light => Palette {
                canvas: Color32::from_rgb(247, 248, 250),
                surface: Color32::from_rgb(237, 240, 244),
                inset: Color32::from_rgb(255, 255, 255),
                elevated: Color32::from_rgb(255, 255, 255),
                hover: Color32::from_rgb(222, 226, 234),
                border: Color32::from_rgb(202, 208, 219),
                control_border: Color32::from_rgb(135, 145, 160),
                text: Color32::from_rgb(28, 34, 44),
                secondary: Color32::from_rgb(65, 77, 93),
                muted: Color32::from_rgb(79, 91, 107),
                disabled: Color32::from_rgb(110, 120, 133),
                accent: Color32::from_rgb(98, 66, 166),
                accent_hover: Color32::from_rgb(79, 48, 144),
                accent_bg: Color32::from_rgb(224, 215, 243),
                success: Color32::from_rgb(34, 111, 76),
                error: Color32::from_rgb(175, 43, 59),
                warning: Color32::from_rgb(135, 88, 13),
                syntax_string: Color32::from_rgb(44, 103, 60),
                syntax_number: Color32::from_rgb(145, 76, 25),
            },
            Self::HighContrast => Palette {
                canvas: Color32::from_rgb(0, 0, 0),
                surface: Color32::from_rgb(6, 6, 6),
                inset: Color32::from_rgb(0, 0, 0),
                elevated: Color32::from_rgb(16, 16, 16),
                hover: Color32::from_rgb(37, 37, 37),
                border: Color32::from_rgb(154, 154, 154),
                control_border: Color32::from_rgb(210, 210, 210),
                text: Color32::from_rgb(255, 255, 255),
                secondary: Color32::from_rgb(235, 235, 235),
                muted: Color32::from_rgb(194, 194, 194),
                disabled: Color32::from_rgb(150, 150, 150),
                accent: Color32::from_rgb(218, 202, 255),
                accent_hover: Color32::from_rgb(243, 235, 255),
                accent_bg: Color32::from_rgb(47, 31, 67),
                success: Color32::from_rgb(153, 255, 190),
                error: Color32::from_rgb(255, 168, 175),
                warning: Color32::from_rgb(255, 224, 135),
                syntax_string: Color32::from_rgb(183, 240, 169),
                syntax_number: Color32::from_rgb(255, 208, 152),
            },
        }
    }
}
thread_local! {
    // egui renders on its UI thread. Each test and viewport installs its palette before drawing.
    static ACTIVE_THEME: std::cell::Cell<Theme> = const { std::cell::Cell::new(Theme::Dark) };
}
pub(super) fn active() -> Theme {
    ACTIVE_THEME.with(|theme| theme.get())
}

pub(super) fn colors() -> Palette {
    ACTIVE_THEME.with(|theme| theme.get().colors())
}

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

pub(super) fn install(ctx: &Context, font_size: f32, high_contrast: bool, theme: Theme) {
    ACTIVE_THEME.with(|active| active.set(theme));
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
    *visuals = if theme.is_dark() {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    let border = if high_contrast {
        colors().secondary
    } else {
        colors().border
    };
    visuals.override_text_color = Some(colors().text);
    visuals.weak_text_color = Some(colors().muted);
    visuals.panel_fill = colors().canvas;
    visuals.window_fill = colors().surface;
    visuals.window_stroke = Stroke::new(
        1.0_f32,
        if high_contrast {
            colors().secondary
        } else {
            colors().control_border
        },
    );
    visuals.window_corner_radius = MODAL_RADIUS.into();
    visuals.window_highlight_topmost = false;
    visuals.menu_corner_radius = CONTROL_RADIUS.into();
    visuals.extreme_bg_color = colors().inset;
    visuals.faint_bg_color = colors().surface;
    visuals.selection.bg_fill = colors().accent_bg;
    visuals.selection.stroke = Stroke::new(1.0_f32, colors().accent);
    visuals.hyperlink_color = colors().accent_hover;
    visuals.text_cursor.stroke = Stroke::new(2.0_f32, colors().accent_hover);
    visuals.indent_has_left_vline = true;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, border);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, colors().text);
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
                colors().secondary
            } else {
                colors().control_border
            },
        );
        widget.fg_stroke = Stroke::new(1.0_f32, colors().text);
        widget.bg_fill = colors().elevated;
        widget.weak_bg_fill = Color32::TRANSPARENT;
    }
    visuals.widgets.hovered.bg_fill = colors().hover;
    visuals.widgets.hovered.weak_bg_fill = colors().hover;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, colors().muted);
    visuals.widgets.active.bg_fill = colors().accent_bg;
    visuals.widgets.active.weak_bg_fill = colors().accent_bg;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, colors().accent);
    visuals.widgets.open.weak_bg_fill = colors().accent_bg;
    ctx.set_style(style);
}

pub(super) fn panel_frame(margin: Margin) -> Frame {
    Frame::new().fill(colors().surface).inner_margin(margin)
}

pub(super) fn muted(text: impl Into<String>) -> RichText {
    let text = text.into();
    RichText::new(tr(&text)).size(SMALL).color(colors().muted)
}

pub(super) fn secondary(text: impl Into<String>) -> RichText {
    let text = text.into();
    RichText::new(tr(&text))
        .size(BODY)
        .color(colors().secondary)
}

pub(super) fn primary(text: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(tr(text)).strong().color(colors().canvas))
        .fill(colors().accent)
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
        return ui.add(egui::Spinner::new().color(colors().accent).size(14.0));
    }
    let (rect, response) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
    ui.painter()
        .circle_stroke(rect.center(), 4.0, Stroke::new(1.5_f32, colors().muted));
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, false, "Working"));
    response
}

pub(super) fn section(ui: &mut egui::Ui, title: &str, description: &str) {
    ui.label(RichText::new(tr(title)).size(LEAD).strong());
    if !description.is_empty() {
        ui.add(egui::Label::new(secondary(description)).wrap());
    }
    ui.add_space(UNIT * 3.0);
}

pub(super) fn row(ui: &mut egui::Ui, label: &str, detail: &str, selected: bool) -> egui::Response {
    let (label, detail) = (tr(label), tr(detail));
    let height = ui.spacing().interact_size.y.max(24.0);
    let response = ui.add_sized(
        [ui.available_width(), height],
        egui::Button::new("").frame(false).selected(selected),
    );
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::SelectableLabel, true, label));
    let painter = ui.painter_at(response.rect);
    let hovered = super::motion::state(
        ui,
        response.id.with("row-hover"),
        response.hovered(),
        super::motion::HOVER,
    );
    let selection = super::motion::state(
        ui,
        response.id.with("row-selection"),
        selected,
        super::motion::CONTROL,
    );
    let fill = super::motion::blend(
        colors().hover.gamma_multiply(hovered),
        colors().accent_bg,
        selection,
    );
    painter.rect_filled(response.rect, 4, fill);
    if selected {
        painter.rect_filled(
            egui::Rect::from_min_size(
                response.rect.left_top() + egui::vec2(0.0, 4.0),
                egui::vec2(2.0, height - 8.0),
            ),
            1,
            colors().accent,
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
                if selected {
                    colors().text
                } else {
                    colors().secondary
                },
                max_width,
            )
        })
    });
    painter.galley(
        response.rect.left_center() + egui::vec2(12.0, -galley.size().y / 2.0),
        galley,
        colors().text,
    );
    if !detail.is_empty() {
        painter.text(
            response.rect.right_center() - egui::vec2(12.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            detail,
            FontId::proportional(SMALL),
            colors().muted,
        );
    }
    if response.has_focus() {
        painter.rect_stroke(
            response.rect,
            4,
            Stroke::new(1.0_f32, colors().accent),
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
    #[cfg(test)]
    super::motion::record(ui, label, response.rect);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    let hover = response.hovered() || response.has_focus();
    let color = if selected || hover {
        colors().accent_hover
    } else {
        colors().muted
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
                colors().accent_bg
            } else {
                colors().hover.gamma_multiply(progress)
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
            colors().accent,
        );
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect,
            CONTROL_RADIUS,
            Stroke::new(1.0_f32, colors().accent),
            egui::StrokeKind::Inside,
        );
    }
    let press = super::motion::state(
        ui,
        response.id.with("press"),
        response.is_pointer_button_down_on(),
        super::motion::CONTROL,
    );
    draw_icon(
        ui.painter(),
        response.rect.center() + egui::vec2(0.0, press * 0.75),
        icon,
        color,
    );
    response.on_hover_text(tr(label))
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

pub(super) fn modal_frame() -> Frame {
    Frame::new()
        .fill(colors().surface)
        .inner_margin(Margin::same(20))
        .corner_radius(MODAL_RADIUS)
        .stroke(Stroke::new(1.0_f32, colors().control_border))
}

/// Short surface entry; input remains available throughout the transition.
pub(super) fn enter(ui: &mut egui::Ui, id: egui::Id) {
    super::motion::enter(ui, id, super::motion::VIEW);
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
        for theme in Theme::ALL {
            ACTIVE_THEME.with(|active| active.set(theme));
            for (foreground, background) in [
                (colors().text, colors().canvas),
                (colors().text, colors().surface),
                (colors().secondary, colors().surface),
                (colors().muted, colors().surface),
                (colors().accent, colors().accent_bg),
                (colors().syntax_string, colors().canvas),
                (colors().syntax_number, colors().canvas),
            ] {
                let a = luminance(foreground);
                let b = luminance(background);
                let ratio = (a.max(b) + 0.05) / (a.min(b) + 0.05);
                assert!(
                    ratio >= 4.5,
                    "{}: insufficient contrast {ratio:.2}",
                    theme.name()
                );
            }
        }
        ACTIVE_THEME.with(|active| active.set(Theme::Dark));
    }
}

pub(super) use super::i18n::{tr, trf};
use eframe::egui::{self, Color32, Context, FontId, Frame, Margin, RichText, Stroke, TextStyle};

pub(crate) const BRAND: &str = crate::brand::NAME;
pub(crate) const TITLE: &str = crate::brand::TITLE;

pub(super) const UNIT: f32 = 4.0;
pub(super) const SMALL: f32 = 12.0;
pub(super) const BODY: f32 = 13.0;
pub(super) const LEAD: f32 = 15.0;
pub(super) const HEADING: f32 = 20.0;
pub(super) const DISPLAY: f32 = 40.0;
pub(super) const CHROME_HEIGHT: f32 = 40.0;
pub(super) const RAIL_WIDTH: f32 = 44.0;
pub(super) const TAB_HEIGHT: f32 = 34.0;
pub(super) const BREADCRUMB_HEIGHT: f32 = 26.0;
pub(super) const STATUS_HEIGHT: f32 = 24.0;
pub(super) const PANEL_PADDING: i8 = 10;
pub(super) const CONTROL_RADIUS: u8 = 6;
pub(super) const CHECKBOX_RADIUS: u8 = 2;
pub(super) const MODAL_RADIUS: u8 = 12;
pub(super) const HOVER_SECONDS: f32 = super::motion::HOVER;
pub(super) const MIN_EDITOR_WIDTH: f32 = 320.0;
pub(super) const CHAT_COMPOSER_HEIGHT: f32 = 132.0;
pub(super) const CODE_LINE_HEIGHT: f32 = 1.32;
pub(super) const COMPACT_CODE_LINE_HEIGHT: f32 = 1.16;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Theme {
    #[default]
    Obsidian,
    Nocturne,
    Dark,
    Midnight,
    Violet,
    Light,
    HighContrast,
}
impl Theme {
    pub(super) const ALL: [Self; 7] = [
        Self::Obsidian,
        Self::Nocturne,
        Self::Dark,
        Self::Midnight,
        Self::Violet,
        Self::Light,
        Self::HighContrast,
    ];
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Obsidian => "Covalos Obsidian",
            Self::Nocturne => "Covalos Nocturne",
            Self::Dark => "Covalos Graphite",
            Self::Midnight => "Covalos Midnight",
            Self::Violet => "Covalos Violet",
            Self::Light => "Covalos Light",
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
    /// Text caret and the selection tint used by every text field and the code editor.
    pub(super) caret: Color32,
    pub(super) selection: Color32,
}
impl Theme {
    pub(super) fn colors(self) -> Palette {
        match self {
            // Neutral obsidian chrome framing an elevated island card with hairline borders
            // and a crisp electric-blue focus accent.
            Self::Obsidian => Palette {
                canvas: Color32::from_rgb(12, 12, 14),
                surface: Color32::from_rgb(20, 20, 23),
                inset: Color32::from_rgb(22, 22, 26),
                elevated: Color32::from_rgb(31, 31, 36),
                hover: Color32::from_rgb(39, 39, 46),
                border: Color32::from_rgb(42, 42, 48),
                control_border: Color32::from_rgb(63, 63, 72),
                text: Color32::from_rgb(242, 242, 245),
                secondary: Color32::from_rgb(176, 176, 186),
                muted: Color32::from_rgb(138, 138, 150),
                disabled: Color32::from_rgb(98, 98, 110),
                accent: Color32::from_rgb(106, 176, 255),
                accent_hover: Color32::from_rgb(148, 200, 255),
                accent_bg: Color32::from_rgb(21, 37, 63),
                success: Color32::from_rgb(110, 222, 165),
                error: Color32::from_rgb(255, 132, 146),
                warning: Color32::from_rgb(246, 198, 118),
                syntax_string: Color32::from_rgb(122, 222, 164),
                syntax_number: Color32::from_rgb(245, 178, 116),
                caret: Color32::from_rgb(96, 170, 255),
                selection: Color32::from_rgba_unmultiplied(59, 130, 246, 76),
            },
            // Deep violet-black base: surfaces separated by value, hairline borders, a lilac
            // accent and a luminous pink caret. Neutrals carry a slight violet cast.
            Self::Nocturne => Palette {
                canvas: Color32::from_rgb(9, 8, 13),
                surface: Color32::from_rgb(17, 15, 23),
                inset: Color32::from_rgb(13, 11, 19),
                elevated: Color32::from_rgb(26, 23, 35),
                hover: Color32::from_rgb(33, 29, 44),
                border: Color32::from_rgb(35, 31, 46),
                control_border: Color32::from_rgb(60, 54, 76),
                text: Color32::from_rgb(232, 228, 240),
                secondary: Color32::from_rgb(170, 164, 186),
                muted: Color32::from_rgb(140, 133, 158),
                disabled: Color32::from_rgb(98, 92, 112),
                accent: Color32::from_rgb(201, 164, 255),
                accent_hover: Color32::from_rgb(222, 197, 255),
                accent_bg: Color32::from_rgb(40, 31, 60),
                success: Color32::from_rgb(126, 220, 190),
                error: Color32::from_rgb(255, 138, 158),
                warning: Color32::from_rgb(245, 199, 126),
                syntax_string: Color32::from_rgb(134, 225, 193),
                syntax_number: Color32::from_rgb(247, 168, 124),
                caret: Color32::from_rgb(255, 122, 198),
                selection: Color32::from_rgba_unmultiplied(150, 104, 255, 72),
            },
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
                caret: Color32::from_rgb(199, 183, 255),
                selection: Color32::from_rgba_unmultiplied(176, 154, 250, 60),
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
                caret: Color32::from_rgb(181, 208, 255),
                selection: Color32::from_rgba_unmultiplied(120, 170, 255, 60),
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
                caret: Color32::from_rgb(232, 170, 255),
                selection: Color32::from_rgba_unmultiplied(195, 167, 253, 64),
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
                caret: Color32::from_rgb(160, 40, 130),
                selection: Color32::from_rgba_unmultiplied(98, 66, 166, 52),
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
                caret: Color32::from_rgb(255, 255, 255),
                selection: Color32::from_rgba_unmultiplied(218, 202, 255, 96),
            },
        }
    }
}
thread_local! {
    // egui renders on its UI thread. Each test and viewport installs its palette before drawing.
    static ACTIVE_THEME: std::cell::Cell<Theme> = const { std::cell::Cell::new(Theme::Obsidian) };
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
    visuals.selection.bg_fill = colors().selection;
    visuals.selection.stroke = Stroke::new(1.0_f32, colors().accent_hover);
    visuals.hyperlink_color = colors().accent_hover;
    visuals.text_cursor.stroke = Stroke::new(2.0_f32, colors().caret);
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

/// Corner radius and spacing of the workbench islands: recessed outer chrome framing
/// elevated, rounded content surfaces with 1px hairline borders.
pub(super) const ISLAND_RADIUS: u8 = 10;
pub(super) const ISLAND_GAP: i8 = 4;

/// Window chrome (title bar, activity rail, status bar): flat, on the recessed canvas.
pub(super) fn panel_frame(margin: Margin) -> Frame {
    Frame::new().fill(colors().canvas).inner_margin(margin)
}

/// A rounded tool-window surface with a hairline edge.
pub(super) fn island(margin: Margin) -> Frame {
    Frame::new()
        .fill(colors().surface)
        .inner_margin(margin)
        .outer_margin(Margin::same(ISLAND_GAP))
        .corner_radius(ISLAND_RADIUS)
        .stroke(Stroke::new(1.0_f32, colors().border))
}

/// Primary floating center card for the editor, studio, and hub views.
pub(super) fn centre_island(margin: Margin) -> Frame {
    Frame::new()
        .fill(colors().inset)
        .inner_margin(margin)
        .outer_margin(Margin::same(ISLAND_GAP))
        .corner_radius(ISLAND_RADIUS)
        .stroke(Stroke::new(1.0_f32, colors().border))
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

/// Interface icons. Everything except the COVALOS mark is a Lucide icon (ISC) rasterized to a
/// mipmapped mask, so every surface shares one stroke weight, grid and corner language.
// The full interface vocabulary is declared up front so new surfaces reuse it instead of
// drawing ad-hoc glyphs; not every icon is placed yet.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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
    Folder,
    FolderOpen,
    ChevronRight,
    ChevronDown,
    Play,
    Pause,
    Save,
    Bell,
    PanelLeft,
    PanelBottom,
    Detach,
    Dock,
    More,
    Check,
    Alert,
    Send,
    Stop,
    History,
    Commit,
    Diff,
    Sparkles,
    Collapse,
    FilePlus,
    Focus,
    Palette,
    Languages,
    Sliders,
    Undo,
    Trash,
    ExternalLink,
    Copy,
    Compare,
    Grip,
    Layers,
    Back,
    ChevronUp,
    ArrowDown,
    Library,
    Heart,
    Star,
    Download,
    Package,
    Book,
    Offline,
    Globe,
    Blocks,
    Type,
    Chart,
    Image,
    Template,
    Wand,
    Clipboard,
    Shield,
    Warning,
    Spinner,
}

impl Icon {
    pub(super) fn asset(self) -> Option<&'static str> {
        Some(match self {
            Self::Brand => return None,
            Self::Explorer => "lucide-files",
            Self::Search => "lucide-search",
            Self::Git => "lucide-git-branch",
            Self::Hub => "lucide-bot",
            Self::Settings => "lucide-settings",
            Self::Close => "lucide-x",
            Self::Add => "lucide-plus",
            Self::Refresh => "lucide-refresh-cw",
            Self::Minimize => "lucide-minus",
            Self::Maximize => "lucide-square",
            Self::Restore => "lucide-copy",
            Self::Code => "lucide-code",
            Self::Terminal => "lucide-square-terminal",
            Self::Folder => "lucide-folder",
            Self::FolderOpen => "lucide-folder-open",
            Self::ChevronRight => "lucide-chevron-right",
            Self::ChevronDown => "lucide-chevron-down",
            Self::Play => "lucide-play",
            Self::Pause => "lucide-pause",
            Self::Save => "lucide-save",
            Self::Bell => "lucide-bell",
            Self::PanelLeft => "lucide-panel-left",
            Self::PanelBottom => "lucide-panel-bottom",
            Self::Detach => "lucide-picture-in-picture-2",
            Self::Dock => "lucide-arrow-down-to-line",
            Self::More => "lucide-ellipsis",
            Self::Check => "lucide-check",
            Self::Alert => "lucide-circle-alert",
            Self::Send => "lucide-arrow-up",
            Self::Stop => "lucide-circle-stop",
            Self::History => "lucide-history",
            Self::Commit => "lucide-git-commit-horizontal",
            Self::Diff => "lucide-file-diff",
            Self::Sparkles => "lucide-sparkles",
            Self::Collapse => "lucide-chevrons-down-up",
            Self::FilePlus => "lucide-file-plus",
            Self::Focus => "lucide-focus",
            Self::Palette => "lucide-palette",
            Self::Languages => "lucide-languages",
            Self::Sliders => "lucide-sliders-horizontal",
            Self::Undo => "lucide-undo-2",
            Self::Trash => "lucide-trash-2",
            Self::ExternalLink => "lucide-external-link",
            Self::Copy => "lucide-copy",
            Self::Compare => "lucide-git-compare",
            Self::Grip => "lucide-grip-vertical",
            Self::Layers => "lucide-layers",
            Self::Back => "lucide-chevron-left",
            Self::ChevronUp => "lucide-chevron-up",
            Self::ArrowDown => "lucide-chevron-down",
            Self::Library => "lucide-library-big",
            Self::Heart => "lucide-heart",
            Self::Star => "lucide-star",
            Self::Download => "lucide-download",
            Self::Package => "lucide-package",
            Self::Book => "lucide-book-open",
            Self::Offline => "lucide-wifi-off",
            Self::Globe => "lucide-globe",
            Self::Blocks => "lucide-blocks",
            Self::Type => "lucide-type",
            Self::Chart => "lucide-chart-column",
            Self::Image => "lucide-image",
            Self::Template => "lucide-layout-template",
            Self::Wand => "lucide-wand-sparkles",
            Self::Clipboard => "lucide-clipboard-copy",
            Self::Shield => "lucide-shield-check",
            Self::Warning => "lucide-triangle-alert",
            Self::Spinner => "lucide-loader-circle",
        })
    }
}

/// Standard interface icon size in points (Lucide's 24 px grid drawn at 16 pt).
pub(super) const ICON_SIZE: f32 = 16.0;

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
    draw_icon_sized(painter, center, icon, color, ICON_SIZE);
}

pub(super) fn draw_icon_sized(
    painter: &egui::Painter,
    center: egui::Pos2,
    icon: Icon,
    color: Color32,
    size: f32,
) {
    if let Some(name) = icon.asset() {
        super::assets::tinted(
            painter,
            egui::Rect::from_center_size(center, egui::Vec2::splat(size)),
            name,
            color,
        );
        return;
    }
    // Centralized provisional COVALOS mark (refined Bond Prism C). High Contrast uses the
    // monochrome mask tinted with the active accent; other themes render the full-color mark.
    let rect = egui::Rect::from_center_size(center, egui::Vec2::splat(size * 1.15));
    if active() == Theme::HighContrast {
        super::assets::tinted(painter, rect, "covalos_mark_mono", color);
    } else {
        super::assets::tinted(painter, rect, "covalos_mark", Color32::WHITE);
    }
}

pub(super) fn modal_frame() -> Frame {
    Frame::new()
        .fill(colors().surface)
        .inner_margin(Margin::same(20))
        .corner_radius(MODAL_RADIUS)
        .stroke(Stroke::new(1.0_f32, colors().control_border))
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

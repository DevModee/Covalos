//! Web preview: a WebView2 child window placed over a side panel of the
//! editor. egui draws the panel chrome (address, reload, close); the page is
//! drawn by the system WebView2 runtime inside the panel's rectangle.
use eframe::egui::{self, Align, Key, Layout, Margin, RichText, SidePanel, TextEdit};

use super::{Editor, theme::*};

#[derive(Default)]
pub(super) struct Preview {
    pub(super) open: bool,
    url: String,
    address: String,
    /// Logical rectangle the page occupies, from the last frame.
    rect: Option<egui::Rect>,
    navigate: bool,
    reload: bool,
    error: Option<String>,
    #[cfg(windows)]
    view: Option<wry::WebView>,
    #[cfg(windows)]
    context: Option<wry::WebContext>,
}

impl Editor {
    pub(super) fn open_preview(&mut self, url: &str) {
        let preview = &mut self.preview;
        preview.url = url.to_owned();
        preview.address = url.to_owned();
        preview.open = true;
        preview.navigate = true;
        preview.error = None;
    }

    /// Right-hand preview panel (left of the agent dock).
    pub(super) fn preview_panel(&mut self, ctx: &egui::Context) {
        if !self.preview.open {
            self.preview.rect = None;
            return;
        }
        let max = (ctx.content_rect().width() * 0.6).max(320.0);
        let result = SidePanel::right("preview")
            .resizable(true)
            .default_width(self.preferences.preview_width.min(max))
            .width_range(300.0..=max)
            .frame(island(Margin::symmetric(8, 8)))
            .show(ctx, |ui| {
                let preview = &mut self.preview;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if icon_button(ui, Icon::Refresh, tr("Reload"), false).clicked() {
                        preview.reload = true;
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icon_button(ui, Icon::Close, tr("Close preview"), false).clicked() {
                            preview.open = false;
                        }
                        if icon_button(ui, Icon::ExternalLink, tr("Open in the browser"), false)
                            .clicked()
                        {
                            open_external(&preview.url);
                        }
                        if icon_button(ui, Icon::Copy, tr("Copy address"), false).clicked() {
                            ui.ctx().copy_text(preview.url.clone());
                        }
                        let field = ui.add(
                            TextEdit::singleline(&mut preview.address)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(ui.available_width()),
                        );
                        if field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                            let mut url = preview.address.trim().to_owned();
                            if !url.contains("://") {
                                url = format!("http://{url}");
                            }
                            preview.url = url.clone();
                            preview.address = url;
                            preview.navigate = true;
                        }
                    });
                });
                ui.add_space(4.0);
                let rect = ui.available_rect_before_wrap();
                ui.painter().rect_filled(rect, 6, colors().inset);
                let note = preview.error.clone().unwrap_or_else(|| {
                    if cfg!(windows) {
                        tr("Loading the page…").to_owned()
                    } else {
                        tr("The embedded preview needs Windows WebView2. Use Open in the browser.")
                            .to_owned()
                    }
                });
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    note,
                    egui::FontId::proportional(BODY),
                    colors().muted,
                );
                ui.allocate_rect(rect, egui::Sense::hover());
                preview.rect = Some(rect);
                let _ = RichText::new("");
            });
        let width = result.response.rect.width();
        if (width - self.preferences.preview_width).abs() > 1.0 && width < max {
            self.preferences.preview_width = width;
        }
    }

    /// Creates, moves, shows or hides the native page after egui laid out the
    /// frame. Hidden while a modal or another full-screen view is up, because
    /// a native child window always paints above egui.
    #[cfg(windows)]
    pub(super) fn sync_preview(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        let covered = self.hub_open
            || self.studio_open
            || self.pending.is_some()
            || self.settings_open
            || self.about_open
            || self.palette.is_some()
            || self.new_file.is_some()
            || self.file_change.is_some()
            || self.pending_delete.is_some()
            || self.conversation_exit_warning
            || self.hub.agents.chat.add_ai.is_some()
            || self.terminals.confirming();
        let preview = &mut self.preview;
        let show = preview.open && preview.rect.is_some() && !covered;
        if !preview.open && preview.view.is_some() {
            preview.view = None;
            return;
        }
        let Some(rect) = preview.rect else {
            if let Some(view) = &preview.view {
                let _ = view.set_visible(false);
            }
            return;
        };
        let scale = ctx.pixels_per_point();
        let bounds = wry::Rect {
            position: wry::dpi::PhysicalPosition::new(
                (rect.left() * scale).round() as i32,
                (rect.top() * scale).round() as i32,
            )
            .into(),
            size: wry::dpi::PhysicalSize::new(
                (rect.width() * scale).round().max(1.0) as u32,
                (rect.height() * scale).round().max(1.0) as u32,
            )
            .into(),
        };
        if preview.view.is_none() && show {
            if preview.context.is_none() {
                let dir = crate::paths::config_dir().map(|d| d.join("webview"));
                preview.context = Some(wry::WebContext::new(dir));
            }
            let context = preview.context.as_mut().expect("context");
            match wry::WebViewBuilder::new_with_web_context(context)
                .with_url(&preview.url)
                .with_bounds(bounds)
                .build_as_child(frame)
            {
                Ok(view) => {
                    preview.view = Some(view);
                    preview.navigate = false;
                    preview.error = None;
                }
                Err(e) => {
                    preview.error = Some(format!(
                        "{} {e}",
                        tr("WebView2 could not start. Use Open in the browser.")
                    ));
                    preview.open = preview.error.is_some();
                    return;
                }
            }
        }
        let Some(view) = &preview.view else { return };
        let _ = view.set_bounds(bounds);
        let _ = view.set_visible(show);
        if std::mem::take(&mut preview.navigate) {
            let _ = view.load_url(&preview.url);
        }
        if std::mem::take(&mut preview.reload) {
            let _ = view.evaluate_script("location.reload()");
        }
    }

    #[cfg(not(windows))]
    pub(super) fn sync_preview(&mut self, _ctx: &egui::Context, _frame: &eframe::Frame) {}
}

#[cfg(test)]
impl Editor {
    /// The native page exists and reports the address it loaded.
    pub(super) fn harness_preview_url(&self) -> Option<String> {
        #[cfg(windows)]
        {
            self.preview.view.as_ref().and_then(|v| v.url().ok())
        }
        #[cfg(not(windows))]
        {
            None
        }
    }
}

fn open_external(url: &str) {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return;
    }
    #[cfg(windows)]
    {
        let mut command = std::process::Command::new("explorer.exe");
        crate::process::configure(&mut command);
        let _ = command.arg(url).spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}

//! AEROS Clean Code: detach files from the tab strip into floating editor surfaces.
//!
//! A floating editor is a view of the same `Document` the tab used: the same buffer, dirty
//! state, save path and highlighter. Nothing is copied, so docking, closing, saving and
//! exiting go through the regular editor paths and their unsaved-changes guards. Several
//! files can float at once; "Focus" hides the side panels so the code sits on the home
//! background, and docking animates the surface back into the tab strip.
use super::{Editor, Pending, theme::*};
use eframe::egui::{
    self, Align, CornerRadius, Id, Layout, Margin, Rect, Sense, Stroke, TextEdit, TextStyle, Ui,
    text::{CCursor, LayoutJob},
};
use std::path::{Path, PathBuf};

pub(super) const DOCK_SECONDS: f64 = 0.30;

#[derive(Default)]
pub(super) struct Clean {
    pub(super) floating: Vec<Floating>,
    /// Hide side panels so floating editors sit on the background alone.
    pub(super) focus: bool,
    /// Floating document whose editor had keyboard focus last frame.
    pub(super) focused: Option<PathBuf>,
    /// Surfaces travelling back into the tab strip: (from, started at).
    docking: Vec<(Rect, f64)>,
    /// Where docked surfaces land: the top of the editor island.
    pub(super) dock_target: Option<Rect>,
    raise: Option<PathBuf>,
}

pub(super) struct Floating {
    pub(super) path: PathBuf,
    cascade: usize,
    rect: Option<Rect>,
}

impl Clean {
    pub(super) fn is_floating(&self, path: &Path) -> bool {
        self.floating.iter().any(|f| f.path == path)
    }

    #[cfg(test)]
    pub(super) fn active(&self) -> bool {
        !self.floating.is_empty()
    }
}

impl Editor {
    /// Move a document from the tab strip into a floating editor.
    pub(super) fn detach(&mut self, index: usize) {
        let Some(path) = self.documents.get(index).map(|d| d.path.clone()) else {
            return;
        };
        if self.clean.is_floating(&path) {
            self.clean.raise = Some(path);
            return;
        }
        let cascade = self.clean.floating.len();
        self.clean.floating.push(Floating {
            path: path.clone(),
            cascade,
            rect: None,
        });
        self.clean.raise = Some(path);
        if self.active == Some(index) {
            self.active = self.next_docked(index);
            self.cursor = 0;
            self.selection = None;
            self.find_match = None;
        }
    }

    /// Return a floating document to the tab strip and make it the active tab.
    pub(super) fn dock(&mut self, path: &Path, now: f64) {
        if let Some(position) = self.clean.floating.iter().position(|f| f.path == path) {
            let floating = self.clean.floating.remove(position);
            if let Some(rect) = floating.rect {
                self.clean.docking.push((rect, now));
            }
        }
        if let Some(index) = self.documents.iter().position(|d| d.path == path) {
            self.active = None;
            self.activate(index);
        }
        if self.clean.floating.is_empty() {
            self.clean.focus = false;
        }
    }

    pub(super) fn dock_all(&mut self, now: f64) {
        let paths: Vec<PathBuf> = self.clean.floating.iter().map(|f| f.path.clone()).collect();
        for path in paths {
            self.dock(&path, now);
        }
    }

    /// First docked document after `index`, wrapping, or None when everything floats.
    pub(super) fn next_docked(&self, index: usize) -> Option<usize> {
        let n = self.documents.len();
        (1..=n)
            .map(|step| (index + step) % n)
            .find(|&i| i != index && !self.clean.is_floating(&self.documents[i].path))
    }

    /// Focus a floating editor instead of switching tabs when a floating file is opened.
    pub(super) fn raise_floating(&mut self, path: &Path) -> bool {
        if self.clean.is_floating(path) {
            self.clean.raise = Some(path.to_owned());
            true
        } else {
            false
        }
    }

    pub(super) fn floating_editors(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        // Documents closed through the normal paths leave their surfaces too.
        let documents = &self.documents;
        self.clean
            .floating
            .retain(|f| documents.iter().any(|d| d.path == f.path));
        if self.clean.floating.is_empty() {
            self.clean.focus = false;
        }
        self.dock_ghosts(ctx, now);
        let bounds = ctx
            .content_rect()
            .shrink2(egui::vec2(8.0, 0.0))
            .with_min_y(ctx.content_rect().top() + CHROME_HEIGHT + 4.0)
            .with_max_y(ctx.content_rect().bottom() - STATUS_HEIGHT - 4.0);
        let mut focused = None;
        let mut dock = None;
        let mut close = None;
        let raise = self.clean.raise.take();
        let mono = TextStyle::Monospace.resolve(&ctx.style());
        let compact = self.preferences.compact;
        let root = self.root.clone();
        for floating in &mut self.clean.floating {
            let Some(index) = self.documents.iter().position(|d| d.path == floating.path) else {
                continue;
            };
            let doc = &mut self.documents[index];
            let id = Id::new(("floating-editor", &floating.path));
            let size = egui::vec2(
                (bounds.width() * 0.46).clamp(360.0, 760.0),
                (bounds.height() * 0.62).clamp(240.0, 620.0),
            );
            let start = bounds.center() - size / 2.0
                + egui::vec2(28.0, 24.0) * floating.cascade as f32
                + egui::vec2(bounds.width() * 0.08, 0.0);
            let window = egui::Window::new(workspace_name(&doc.path))
                .id(id)
                .title_bar(false)
                .resizable(true)
                .collapsible(false)
                .default_size(size)
                .min_size(egui::vec2(300.0, 180.0))
                .default_pos(start)
                .constrain_to(bounds)
                .fade_in(true)
                .fade_out(true)
                .frame(
                    egui::Frame::new()
                        .fill(colors().inset)
                        .stroke(Stroke::new(1.0_f32, colors().control_border))
                        .corner_radius(CornerRadius::same(14))
                        .shadow(egui::Shadow {
                            offset: [0, 18],
                            blur: 48,
                            spread: 0,
                            color: egui::Color32::from_black_alpha(if active().is_dark() {
                                150
                            } else {
                                60
                            }),
                        })
                        .inner_margin(Margin::ZERO),
                );
            let response = window.show(ctx, |ui| {
                let header = header(ui, doc, root.as_deref());
                if header.dock {
                    dock = Some(floating.path.clone());
                }
                if header.close {
                    close = Some(index);
                }
                if header.focus {
                    self.clean.focus = !self.clean.focus;
                }
                let editor_id = id.with("text");
                let has_focus = code_surface(ui, doc, editor_id, &mono, compact);
                if raise.as_deref() == Some(floating.path.as_path()) {
                    ui.memory_mut(|m| m.request_focus(editor_id));
                }
                if has_focus {
                    focused = Some(floating.path.clone());
                }
            });
            if let Some(response) = response {
                floating.rect = Some(response.response.rect);
                if raise.as_deref() == Some(floating.path.as_path()) {
                    ctx.move_to_top(response.response.layer_id);
                }
            }
        }
        self.clean.focused = focused;
        if let Some(path) = dock {
            self.dock(&path, now);
        }
        if let Some(index) = close {
            self.request(Pending::CloseTab(index), ctx);
        }
    }

    /// Outline that travels from a docked surface to the tab strip and dissolves.
    fn dock_ghosts(&mut self, ctx: &egui::Context, now: f64) {
        let duration = DOCK_SECONDS * f64::from(super::motion::intensity());
        self.clean
            .docking
            .retain(|(_, start)| duration > 0.0 && now - start < duration);
        let Some(target) = self.clean.dock_target else {
            return;
        };
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            Id::new("clean-dock-ghost"),
        ));
        for (from, start) in &self.clean.docking {
            let t = (((now - start) / duration) as f32).clamp(0.0, 1.0);
            let e = egui::emath::easing::cubic_in_out(t);
            let to = Rect::from_min_size(target.min, egui::vec2(target.width().min(220.0), 30.0));
            let rect = Rect::from_min_max(from.min.lerp(to.min, e), from.max.lerp(to.max, e));
            painter.rect_filled(
                rect,
                14.0 - 7.0 * e,
                colors().elevated.gamma_multiply(1.0 - e),
            );
            painter.rect_stroke(
                rect,
                14.0 - 7.0 * e,
                Stroke::new(1.0_f32, colors().accent.gamma_multiply(1.0 - e * 0.6)),
                egui::StrokeKind::Inside,
            );
        }
        if !self.clean.docking.is_empty() {
            ctx.request_repaint();
        }
    }
}

fn workspace_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

struct HeaderActions {
    dock: bool,
    close: bool,
    focus: bool,
}

/// Discreet title strip: file icon, name, folder and dirty state on the left, dock /
/// focus / close on the right. The strip itself only senses hover, so dragging it moves
/// the window.
fn header(ui: &mut Ui, doc: &crate::workspace::Document, root: Option<&Path>) -> HeaderActions {
    let mut actions = HeaderActions {
        dock: false,
        close: false,
        focus: false,
    };
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 36.0), Sense::hover());
    let painter = ui.painter_at(rect);
    super::assets::file(ui, rect.left_center() + egui::vec2(18.0, 0.0), &doc.path);
    let name = workspace_name(&doc.path);
    let galley = ui.fonts_mut(|f| {
        f.layout_no_wrap(
            name.clone(),
            egui::FontId::proportional(BODY),
            colors().text,
        )
    });
    let name_width = galley.size().x;
    painter.galley(
        egui::pos2(rect.left() + 32.0, rect.center().y - galley.size().y / 2.0),
        galley,
        colors().text,
    );
    let folder = doc
        .path
        .parent()
        .map(|p| root.and_then(|r| p.strip_prefix(r).ok()).unwrap_or(p))
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let mut x = rect.left() + 32.0 + name_width + 8.0;
    if doc.dirty {
        painter.circle_filled(egui::pos2(x + 2.0, rect.center().y), 3.5, colors().accent);
        x += 12.0;
    }
    let buttons_left = rect.right() - 3.0 * 28.0 - 8.0;
    if !folder.is_empty() && x < buttons_left - 24.0 {
        painter
            .with_clip_rect(Rect::from_x_y_ranges(
                x..=buttons_left - 8.0,
                rect.y_range(),
            ))
            .text(
                egui::pos2(x, rect.center().y),
                egui::Align2::LEFT_CENTER,
                folder,
                egui::FontId::proportional(SMALL),
                colors().muted,
            );
    }
    let mut ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_x_y_ranges(
                buttons_left..=rect.right() - 6.0,
                rect.y_range(),
            ))
            .layout(Layout::right_to_left(Align::Center)),
    );
    ui.spacing_mut().item_spacing.x = 2.0;
    actions.close = header_button(&mut ui, Icon::Close, "Close tab").clicked();
    actions.dock = header_button(&mut ui, Icon::Dock, "Dock back into tabs").clicked();
    actions.focus = header_button(&mut ui, Icon::Focus, "Focus mode").clicked();
    painter.hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0_f32, colors().border),
    );
    actions
}

fn header_button(ui: &mut Ui, icon: Icon, label: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), Sense::click());
    let hover = super::motion::spring(
        ui.ctx(),
        response.id.with("hover"),
        if response.hovered() { 1.0 } else { 0.0 },
        super::motion::Spring::SNAPPY,
    );
    if hover > 0.01 {
        ui.painter()
            .rect_filled(rect, 6, colors().hover.gamma_multiply(hover));
    }
    draw_icon_sized(
        ui.painter(),
        rect.center(),
        icon,
        super::motion::blend(colors().muted, colors().text, hover),
        14.0,
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    response.on_hover_text(tr(label))
}

/// The floating editor body: same highlighter, gutter and active-line band as the docked
/// editor, editing the document buffer in place. Returns whether it has keyboard focus.
fn code_surface(
    ui: &mut Ui,
    doc: &mut crate::workspace::Document,
    id: Id,
    mono: &egui::FontId,
    compact: bool,
) -> bool {
    let language = doc
        .path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_owned();
    let line_height = mono.size
        * if compact {
            COMPACT_CODE_LINE_HEIGHT
        } else {
            CODE_LINE_HEIGHT
        };
    let mut has_focus = false;
    egui::ScrollArea::both()
        .id_salt(id.with("scroll"))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                let (_, gutter) = ui.allocate_space(egui::vec2(40.0, 1.0));
                let font = mono.clone();
                let mut layouter = |ui: &Ui, buffer: &dyn egui::TextBuffer, _width: f32| {
                    let text = buffer.as_str();
                    let mut job = if language.is_empty() || text.len() > 512 * 1024 {
                        let mut job = LayoutJob::simple(
                            text.to_owned(),
                            font.clone(),
                            colors().text,
                            f32::INFINITY,
                        );
                        for section in &mut job.sections {
                            section.format.line_height = Some(line_height);
                        }
                        job
                    } else {
                        super::syntax::highlight(ui.ctx(), text, &language, font.size, line_height)
                    };
                    job.wrap.max_width = f32::INFINITY;
                    ui.fonts_mut(|fonts| fonts.layout_job(job))
                };
                let band = ui.painter().add(egui::Shape::Noop);
                let output = TextEdit::multiline(&mut doc.text)
                    .id(id)
                    .code_editor()
                    .frame(false)
                    .margin(Margin::symmetric(8, 10))
                    .desired_rows(12)
                    .desired_width(ui.available_width())
                    .layouter(&mut layouter)
                    .show(ui);
                if output.response.changed() {
                    doc.update_dirty();
                }
                has_focus = output.response.has_focus();
                let rows = &output.galley.rows;
                let origin = output.galley_pos;
                let clip = ui.clip_rect();
                let caret = output
                    .cursor_range
                    .map(|c| c.primary.index)
                    .unwrap_or_default();
                let caret = output.galley.pos_from_cursor(CCursor::new(caret));
                let active_row = rows
                    .iter()
                    .position(|row| {
                        caret.center().y >= row.pos.y && caret.center().y < row.pos.y + row.size.y
                    })
                    .unwrap_or(0);
                if has_focus && let Some(row) = rows.get(active_row) {
                    let y = origin.y + row.pos.y;
                    ui.painter().set(
                        band,
                        egui::Shape::rect_filled(
                            Rect::from_min_max(
                                egui::pos2(gutter.left(), y),
                                egui::pos2(clip.right(), y + row.size.y),
                            ),
                            0,
                            super::syntax::current().active_line,
                        ),
                    );
                }
                let first =
                    rows.partition_point(|row| origin.y + row.pos.y + row.size.y < clip.top());
                let last = rows
                    .partition_point(|row| origin.y + row.pos.y < clip.bottom())
                    .min(rows.len());
                for (index, row) in rows.iter().enumerate().take(last).skip(first) {
                    ui.painter().text(
                        egui::pos2(gutter.right() - 6.0, origin.y + row.pos.y),
                        egui::Align2::RIGHT_TOP,
                        (index + 1).to_string(),
                        mono.clone(),
                        if index == active_row && has_focus {
                            colors().secondary
                        } else {
                            colors().disabled
                        },
                    );
                }
            });
        });
    has_focus
}

#[cfg(test)]
mod tests {
    use super::super::preferences::Preferences;
    use super::*;

    #[test]
    fn detached_files_keep_their_buffer_and_dock_back_without_loss() -> std::io::Result<()> {
        let folder = tempfile::tempdir()?;
        let ctx = egui::Context::default();
        let mut editor = Editor::new(&ctx, Some(folder.path().to_owned()), Preferences::default());
        assert!(editor.create_file("a.rs"));
        assert!(editor.create_file("b.rs"));
        editor.documents[1].text = "fn floating() {}\n".into();
        editor.documents[1].update_dirty();
        editor.detach(1);
        assert!(editor.clean.active());
        assert_eq!(editor.active, Some(0));
        // Render with the floating editor present and type through the real TextEdit path.
        for _ in 0..3 {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ctx| editor.render(ctx),
            );
        }
        assert!(editor.clean.floating[0].rect.is_some());
        assert!(editor.documents[1].dirty);
        // Opening the floating file again raises it instead of re-docking.
        let path = editor.documents[1].path.clone();
        editor.open_file(path.clone());
        assert_eq!(editor.active, Some(0));
        editor.dock(&path, 0.0);
        assert!(!editor.clean.active());
        assert_eq!(editor.active, Some(1));
        assert_eq!(editor.documents[1].text, "fn floating() {}\n");
        // Every file floating leaves no docked tab and the home view underneath.
        editor.detach(0);
        editor.detach(1);
        assert_eq!(editor.active, None);
        editor.dock_all(0.0);
        assert_eq!(editor.clean.floating.len(), 0);
        Ok(())
    }
}

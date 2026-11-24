//! Companion surfaces: the top-centre island (compact pill that morphs into the expanded
//! agent card) and toast notifications. Input stays live throughout every transition.
use super::super::{Editor, Pending, theme::*};
use super::{CompanionAction, state_color};
use crate::agent_bridge::{self, State};
use crate::assistant::Provider;
use eframe::egui::{self, Align, Layout, RichText, ScrollArea, Ui};
use std::time::{Duration, Instant};

const ISLAND_COMPACT: egui::Vec2 = egui::vec2(232.0, 21.0);
const ISLAND_WIDTH: f32 = 564.0;

fn provider_of(label: &str) -> Option<Provider> {
    let l = label.to_ascii_lowercase();
    [
        ("claude", Provider::Claude),
        ("codex", Provider::Codex),
        ("gemini", Provider::Gemini),
        ("antigravity", Provider::Antigravity),
    ]
    .into_iter()
    .find(|(k, _)| l.contains(k))
    .map(|(_, p)| p)
}

fn ago(at: Instant) -> String {
    let s = at.elapsed().as_secs();
    match s {
        0..=4 => "now".into(),
        5..=59 => format!("{s}s"),
        60..=3599 => format!("{}m", s / 60),
        _ => format!("{}h", s / 3600),
    }
}

/// A rounded status chip: tinted fill, coloured dot and label.
fn chip(ui: &mut Ui, text: &str, color: egui::Color32) {
    let galley =
        ui.painter()
            .layout_no_wrap(text.to_owned(), egui::FontId::proportional(11.0), color);
    let size = egui::vec2(galley.size().x + 22.0, 20.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 10, color.gamma_multiply(0.14));
    ui.painter()
        .circle_filled(rect.left_center() + egui::vec2(9.0, 0.0), 2.5, color);
    ui.painter().galley(
        rect.left_center() + egui::vec2(15.0, -galley.size().y / 2.0),
        galley,
        color,
    );
}

/// Three-bar activity meter. Bars move while an agent works and rest otherwise.
fn activity(ui: &Ui, center: egui::Pos2, state: State, scale: f32) {
    let t = ui.input(|i| i.time) as f32;
    let live = ui.style().animation_time > 0.0;
    let color = state_color(state);
    for i in 0..3 {
        let h = match state {
            State::Running if live => 0.35 + 0.65 * ((t * 7.0 + i as f32 * 1.3).sin() * 0.5 + 0.5),
            State::Waiting if live => 0.45 + 0.4 * ((t * 3.0).sin() * 0.5 + 0.5),
            State::Running | State::Waiting => 0.7,
            _ => 0.3,
        } * 12.0
            * scale;
        let x = center.x + (i as f32 - 1.0) * 4.5 * scale;
        ui.painter().line_segment(
            [
                egui::pos2(x, center.y - h / 2.0),
                egui::pos2(x, center.y + h / 2.0),
            ],
            egui::Stroke::new(2.4 * scale, color),
        );
    }
    if live && matches!(state, State::Running | State::Waiting) {
        ui.ctx().request_repaint_after(Duration::from_millis(33));
    }
}

/// Indeterminate progress line for running sessions.
fn progress_line(ui: &mut Ui, color: egui::Color32) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 2.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 1, color.gamma_multiply(0.15));
    if ui.style().animation_time > 0.0 {
        let t = (ui.input(|i| i.time) as f32 * 0.6).fract();
        let w = rect.width() * 0.32;
        let x = rect.left() - w + (rect.width() + w) * egui::emath::easing::cubic_in_out(t);
        let bar = egui::Rect::from_min_max(
            egui::pos2(x.max(rect.left()), rect.top()),
            egui::pos2((x + w).min(rect.right()), rect.bottom()),
        );
        if bar.width() > 0.0 {
            ui.painter().rect_filled(bar, 1, color);
        }
    } else {
        ui.painter().rect_filled(rect, 1, color.gamma_multiply(0.6));
    }
}

fn card_frame(attention: bool) -> egui::Frame {
    egui::Frame::new()
        .fill(if attention {
            super::super::motion::blend(colors().elevated, colors().warning, 0.06)
        } else {
            colors().elevated
        })
        .corner_radius(12)
        .inner_margin(egui::Margin::same(14))
        .stroke(egui::Stroke::new(
            1.0_f32,
            if attention {
                colors().warning.gamma_multiply(0.5)
            } else {
                colors().border.gamma_multiply(0.7)
            },
        ))
}

impl Editor {
    fn island_summary(&self) -> (String, State) {
        let (running, waiting) = self.companion.counts();
        if waiting > 0 || self.hub.approval_pending() {
            let n = waiting.max(1);
            (
                trf(
                    if n == 1 {
                        "{n} approval needed"
                    } else {
                        "{n} approvals needed"
                    },
                    &[("n", n.to_string())],
                ),
                State::Waiting,
            )
        } else if running > 0 {
            (
                trf(
                    if running == 1 {
                        "{n} agent working"
                    } else {
                        "{n} agents working"
                    },
                    &[("n", running.to_string())],
                ),
                State::Running,
            )
        } else if let Some(n) = self.companion.history.front() {
            (n.text.clone(), n.state)
        } else {
            (tr("All quiet").into(), State::Idle)
        }
    }

    pub(in super::super) fn companion_panel(&mut self, ui: &mut Ui) {
        let (running, waiting) = self.companion.counts();
        let completed = self
            .companion
            .sessions
            .iter()
            .filter(|s| s.state == State::Completed)
            .count();
        let mood = self.agent_mood(true);
        let mut close = false;
        let mut open_palette_now = false;
        let mut open_hub_now = false;

        // Coucou-style top toolbar inside the Notch: quick mode icons on the left,
        // Luma effort pills + close on the right.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let pill_icon = |ui: &mut Ui, icon: Icon, tip: &str, active: bool| -> egui::Response {
                let (r, resp) = ui.allocate_exact_size(egui::vec2(30.0, 26.0), egui::Sense::click());
                let bg = if active {
                    colors().elevated
                } else if resp.hovered() {
                    colors().hover
                } else {
                    egui::Color32::TRANSPARENT
                };
                ui.painter().rect_filled(r, 13, bg);
                draw_icon_sized(
                    ui.painter(),
                    r.center(),
                    icon,
                    if active || resp.hovered() {
                        colors().text
                    } else {
                        colors().muted
                    },
                    14.0,
                );
                pointer(resp).on_hover_text(tr(tip))
            };
            let _ = pill_icon(ui, Icon::Sparkles, "Luma", true);
            if pill_icon(ui, Icon::Search, "Search files and commands (Ctrl+P)", false).clicked() {
                open_palette_now = true;
            }
            if pill_icon(ui, Icon::Hub, "Open Agent Hub", false).clicked() {
                open_hub_now = true;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_button_sized(ui, Icon::Close, "Close companion", false, 26.0).clicked() {
                    close = true;
                }
                ui.add_space(4.0);
                self.luma_effort_pills(ui);
            });
        });

        ui.add_space(10.0);

        // Coucou-style split hero: Luma expressive status card on the left,
        // quick actions / live status chips on the right.
        let total_w = ui.available_width();
        let left_w = (total_w * 0.56).max(240.0);
        let right_w = (total_w - left_w - 10.0).max(160.0);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            egui::Frame::new()
                .fill(colors().surface)
                .stroke(egui::Stroke::new(1.0_f32, colors().border.gamma_multiply(0.75)))
                .corner_radius(14)
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    ui.set_width((left_w - 24.0).max(160.0));
                    ui.horizontal(|ui| {
                        let (art, _) =
                            ui.allocate_exact_size(egui::vec2(62.0, 62.0), egui::Sense::hover());
                        self.identity.portrait(
                            ui,
                            art,
                            &self.preferences.identity,
                            mood,
                            self.preferences.reduce_motion,
                        );
                        ui.add_space(6.0);
                        ui.vertical(|ui| {
                            ui.add_space(2.0);
                            ui.label(
                                RichText::new(tr("LUMA  ·  LEAD DEVELOPER"))
                                    .size(10.0)
                                    .color(colors().muted),
                            );
                            ui.label(
                                RichText::new(tr(if waiting > 0 {
                                    "Waiting for your OK"
                                } else if running > 0 {
                                    "Work in motion"
                                } else {
                                    "A quiet workspace"
                                }))
                                .size(17.0)
                                .strong(),
                            );
                            ui.add_space(4.0);
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing = egui::vec2(5.0, 4.0);
                                chip(
                                    ui,
                                    &trf("{n} running", &[("n", running.to_string())]),
                                    colors().accent,
                                );
                                if waiting > 0 {
                                    chip(
                                        ui,
                                        &trf("{n} waiting", &[("n", waiting.to_string())]),
                                        colors().warning,
                                    );
                                }
                                if completed > 0 {
                                    chip(
                                        ui,
                                        &trf("{n} done", &[("n", completed.to_string())]),
                                        colors().success,
                                    );
                                }
                            });
                        });
                    });
                });

            egui::Frame::new()
                .fill(colors().surface)
                .stroke(egui::Stroke::new(1.0_f32, colors().border.gamma_multiply(0.75)))
                .corner_radius(14)
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    ui.set_width((right_w - 20.0).max(120.0));
                    ui.spacing_mut().item_spacing.y = 6.0;
                    let pill_row =
                        |ui: &mut Ui, icon: Icon, color: egui::Color32, label: &str| -> bool {
                            let (r, resp) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), 28.0),
                                egui::Sense::click(),
                            );
                            ui.painter().rect(
                                r,
                                10,
                                if resp.hovered() {
                                    colors().hover
                                } else {
                                    colors().elevated
                                },
                                egui::Stroke::new(1.0_f32, color.gamma_multiply(0.32)),
                                egui::StrokeKind::Inside,
                            );
                            draw_icon_sized(
                                ui.painter(),
                                r.left_center() + egui::vec2(14.0, 0.0),
                                icon,
                                color,
                                13.0,
                            );
                            ui.painter().text(
                                r.left_center() + egui::vec2(28.0, 0.0),
                                egui::Align2::LEFT_CENTER,
                                tr(label),
                                egui::FontId::proportional(SMALL),
                                colors().text,
                            );
                            pointer(resp).clicked()
                        };
                    if pill_row(ui, Icon::Search, colors().accent, "Search files (Ctrl+P)") {
                        open_palette_now = true;
                    }
                    if pill_row(ui, Icon::Hub, colors().success, "Agents & sessions") {
                        open_hub_now = true;
                    }
                });
        });

        ui.add_space(10.0);
        self.luma_panel(ui);
        let mut action = None;
        for session in self
            .companion
            .sessions
            .iter()
            .filter(|s| s.state != State::Idle || s.external.is_some())
        {
            ui.push_id(&session.id, |ui| {
                super::super::motion::enter(ui, ui.id().with("card"), super::super::motion::PANEL);
                let color = state_color(session.state);
                let waiting = session.state == State::Waiting;
                card_frame(waiting).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let (tile, _) =
                            ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::hover());
                        match provider_of(&session.label) {
                            Some(p) => super::super::brands::avatar(
                                ui,
                                tile,
                                p,
                                session.state == State::Running,
                            ),
                            None => {
                                ui.painter().rect_filled(tile, 8, colors().inset);
                                activity(ui, tile.center(), session.state, 0.9);
                            }
                        }
                        ui.add_space(4.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&session.label).strong());
                            ui.label(
                                RichText::new(tr(session.state.label()))
                                    .size(SMALL)
                                    .color(color),
                            );
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let (meter, _) = ui
                                .allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                            activity(ui, meter.center(), session.state, 1.0);
                        });
                    });
                    if session.state == State::Running {
                        ui.add_space(8.0);
                        progress_line(ui, color);
                    }
                    if !session.operation.is_empty() {
                        ui.add_space(8.0);
                        egui::Frame::new()
                            .fill(colors().inset)
                            .corner_radius(8)
                            .inner_margin(egui::Margin::symmetric(10, 8))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&session.operation)
                                            .monospace()
                                            .size(12.0)
                                            .color(colors().text),
                                    )
                                    .wrap(),
                                );
                            });
                    }
                    ui.add_space(6.0);
                    if let Some(file) = &session.file {
                        let clean = file.strip_prefix(r"\\?\").unwrap_or(file);
                        ui.add(egui::Label::new(muted(clean)).truncate());
                    }
                    if !session.scope.is_empty() {
                        let clean = session.scope.strip_prefix(r"\\?\").unwrap_or(&session.scope);
                        ui.add(egui::Label::new(muted(clean)).truncate());
                    }
                    if let Some(external) = &session.external {
                        ui.label(muted(format!(
                            "Session {}  ·  {}s ago",
                            external.id,
                            agent_bridge::now_ms().saturating_sub(external.updated_ms) / 1000
                        )));
                        if waiting {
                            ui.add_space(8.0);
                            if external.approvable && external.deadline_ms > agent_bridge::now_ms()
                            {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 8.0;
                                    // Coucou-style red Deny & green Allow pills
                                    let deny = egui::Button::new(
                                        RichText::new(format!("×  {}", tr("Deny")))
                                            .strong()
                                            .color(egui::Color32::from_rgb(255, 224, 224)),
                                    )
                                    .fill(egui::Color32::from_rgb(92, 34, 38))
                                    .stroke(egui::Stroke::NONE)
                                    .corner_radius(15)
                                    .min_size(egui::vec2(104.0, 30.0));
                                    if pointer(ui.add(deny)).clicked() {
                                        action =
                                            Some(CompanionAction::Decide(external.clone(), false));
                                    }
                                    let allow = egui::Button::new(
                                        RichText::new(format!("✓  {}", tr("Allow once")))
                                            .strong()
                                            .color(egui::Color32::from_rgb(10, 32, 16)),
                                    )
                                    .fill(egui::Color32::from_rgb(56, 214, 108))
                                    .stroke(egui::Stroke::NONE)
                                    .corner_radius(15)
                                    .min_size(egui::vec2(120.0, 30.0));
                                    if pointer(ui.add(allow)).clicked() {
                                        action =
                                            Some(CompanionAction::Decide(external.clone(), true));
                                    }
                                });
                            } else {
                                ui.label(muted(tr(
                                    "Answer this request in its original terminal.",
                                )));
                            }
                        }
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if ui
                                .add(egui::Button::new(muted(tr("Copy session ID"))).frame(false))
                                .clicked()
                            {
                                ui.ctx().copy_text(external.id.clone());
                            }
                            if !external.scope.is_empty()
                                && ui
                                    .add(
                                        egui::Button::new(muted(tr("Open workspace ↗")))
                                            .frame(false),
                                    )
                                    .clicked()
                            {
                                action = Some(CompanionAction::Workspace(external.scope.clone()));
                            }
                        });
                    } else if ui
                        .add(egui::Button::new(muted(tr("Open in Agent Hub ↗"))).frame(false))
                        .clicked()
                    {
                        action = Some(CompanionAction::Focus(session.id.clone()));
                    }
                });
                ui.add_space(8.0);
            });
        }
        if self.hub.approval_pending() {
            card_frame(true).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(tr("Buffer change ready for review")).strong());
                ui.add(
                    egui::Label::new(secondary(
                        "The proposed edit is staged in Agent Hub. Nothing has been saved to disk.",
                    ))
                    .wrap(),
                );
                ui.add_space(8.0);
                if ui.add(primary(tr("Review change"))).clicked() {
                    self.hub.focus_approvals();
                    self.hub_open = true;
                    self.companion.open = false;
                }
            });
            ui.add_space(8.0);
        }
        if !self.companion.history.is_empty() {
            ui.add_space(4.0);
            ui.label(
                RichText::new(tr("ACTIVITY"))
                    .size(10.0)
                    .color(colors().muted),
            );
            ui.add_space(4.0);
            for notice in self.companion.history.iter().take(4) {
                ui.horizontal(|ui| {
                    let (dot, _) =
                        ui.allocate_exact_size(egui::vec2(10.0, 16.0), egui::Sense::hover());
                    ui.painter()
                        .circle_filled(dot.center(), 3.0, state_color(notice.state));
                    ui.add(egui::Label::new(secondary(&notice.text)).truncate());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(muted(ago(notice.at)));
                    });
                });
            }
        }
        ui.add_space(8.0);
        // Coucou-style sleek order input anchored at the bottom of the Notch
        self.luma_composer(ui);

        if open_palette_now {
            self.companion.open = false;
            self.open_palette(ui.ctx());
        }
        if open_hub_now {
            self.companion.open = false;
            self.hub_open = true;
            self.studio_open = false;
        }
        if close {
            self.companion.open = false;
        }
        if let Some(action) = action {
            self.companion_action(action, ui.ctx());
        }
    }

    fn companion_action(&mut self, action: CompanionAction, ctx: &egui::Context) {
        match action {
            CompanionAction::Decide(session, allow) => {
                let result = agent_bridge::directory()
                    .ok_or_else(|| std::io::Error::other("No bridge directory"))
                    .and_then(|dir| agent_bridge::decide(&dir, &session, allow));
                match result {
                    Ok(()) => {
                        self.notice =
                            Some((false, if allow { "Allowed once" } else { "Denied" }.into()));
                        for s in &mut self.companion.sessions {
                            if let Some(ext) = &mut s.external
                                && ext.ticket == session.ticket
                            {
                                ext.approvable = false;
                            }
                        }
                    }
                    Err(error) => self.error(format!("Could not answer request. {error}")),
                }
            }
            CompanionAction::Workspace(scope) => {
                let path = std::path::PathBuf::from(scope);
                if path.is_dir() {
                    self.request(Pending::OpenWorkspace(path), ctx);
                } else {
                    self.error(tr("The reported workspace is unavailable."));
                }
            }
            CompanionAction::Focus(id) => {
                self.hub.focus_session(&id);
                self.hub_open = true;
                self.companion.open = false;
            }
        }
    }

    pub(in super::super) fn companion_surface(&mut self, ctx: &egui::Context) {
        let just_opened = self.companion.open && !self.companion.was_open;
        self.companion.was_open = self.companion.open;
        if self.companion.open && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.companion.open = false;
        }
        let (running, waiting) = self.companion.counts();
        let recent = self
            .companion
            .history
            .front()
            .is_some_and(|n| n.at.elapsed() < Duration::from_secs(6));
        let busy = running + waiting > 0 || self.hub.approval_pending();
        let id = egui::Id::new("companion-island");
        // The island opens on a gently damped spring: it retargets mid-flight when the
        // user toggles quickly and settles without a visible bounce.
        let p = super::super::motion::spring(
            ctx,
            id.with("open"),
            if self.companion.open { 1.0 } else { 0.0 },
            super::super::motion::Spring::GENTLE,
        )
        .clamp(0.0, 1.0);
        let eased = p;
        let presence = super::super::motion::value(
            ctx,
            id.with("presence"),
            if busy || recent || self.companion.open {
                1.0
            } else {
                0.0
            },
            super::super::motion::PANEL,
        );
        if presence <= 0.001 && p <= 0.001 {
            self.companion_toast(ctx);
            return;
        }
        let screen = ctx.content_rect();
        let full_width = ISLAND_WIDTH.min(screen.width() - 32.0);
        let target_h: f32 = ctx
            .data(|d| d.get_temp::<f32>(id.with("height")))
            .unwrap_or(320.0)
            .min(screen.height() - CHROME_HEIGHT - 72.0)
            .max(ISLAND_COMPACT.y);
        // Coucou-style top-center Dynamic Notch: anchored flush at the top edge of the
        // window and expanding downward smoothly on a spring.
        let width = egui::lerp(ISLAND_COMPACT.x..=full_width, eased);
        let height = egui::lerp(ISLAND_COMPACT.y..=target_h, eased);
        let top = if eased > 0.05 {
            screen.top() + (1.0 - eased) * 6.0
        } else {
            screen.top() + 6.0 - (1.0 - presence) * 10.0
        };
        let rect = egui::Rect::from_min_size(
            egui::pos2(screen.center().x - width / 2.0, top),
            egui::vec2(width, height),
        );
        let area = egui::Area::new(id)
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .show(ctx, |ui| {
                ui.multiply_opacity(presence.max(p));
                let (outer, response) = ui.allocate_exact_size(rect.size(), egui::Sense::click());
                #[cfg(test)]
                super::super::motion::record(ui, "Companion island", outer);
                let bottom_r = egui::lerp((ISLAND_COMPACT.y / 2.0)..=20.0, eased) as u8;
                let top_r = egui::lerp((ISLAND_COMPACT.y / 2.0)..=0.0, eased) as u8;
                let corner = egui::CornerRadius {
                    nw: top_r,
                    ne: top_r,
                    sw: bottom_r,
                    se: bottom_r,
                };
                ui.painter().add(
                    egui::Shadow {
                        offset: [0, 14],
                        blur: 36,
                        spread: 0,
                        color: egui::Color32::from_black_alpha(150),
                    }
                    .as_shape(outer, corner),
                );
                let notch_fill = if active().is_dark() {
                    egui::Color32::from_rgb(13, 12, 18)
                } else {
                    colors().surface
                };
                ui.painter().rect(
                    outer,
                    corner,
                    notch_fill,
                    egui::Stroke::new(1.0_f32, colors().control_border.gamma_multiply(0.85)),
                    egui::StrokeKind::Inside,
                );
                if eased < 0.999 {
                    let (text, state) = self.island_summary();
                    let pill = egui::Rect::from_min_size(
                        egui::pos2(outer.min.x, outer.max.y - ISLAND_COMPACT.y),
                        ISLAND_COMPACT,
                    );
                    let mut compact = ui.new_child(egui::UiBuilder::new().max_rect(pill));
                    compact.multiply_opacity(1.0 - eased);
                    let mood = self.agent_mood(true);
                    self.identity.portrait(
                        &compact,
                        egui::Rect::from_center_size(
                            pill.left_center() + egui::vec2(15.0, 0.0),
                            egui::vec2(20.0, 20.0),
                        ),
                        &self.preferences.identity,
                        mood,
                        self.preferences.reduce_motion,
                    );
                    let painter = compact
                        .painter()
                        .with_clip_rect(pill.shrink2(egui::vec2(28.0, 0.0)));
                    painter.text(
                        pill.left_center() + egui::vec2(30.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        text,
                        egui::FontId::proportional(SMALL),
                        colors().text,
                    );
                    activity(
                        &compact,
                        pill.right_center() - egui::vec2(16.0, 0.0),
                        state,
                        0.62,
                    );
                    if response.hovered() {
                        ui.painter().rect_stroke(
                            outer,
                            corner,
                            egui::Stroke::new(1.0_f32, colors().accent.gamma_multiply(0.6)),
                            egui::StrokeKind::Inside,
                        );
                    }
                }
                if response.clicked() && !self.companion.open {
                    self.companion.open = true;
                }
                if eased > 0.02 {
                    let inner = outer.shrink(16.0);
                    let mut card = ui.new_child(
                        egui::UiBuilder::new()
                            .id_salt("companion-card")
                            .max_rect(egui::Rect::from_min_size(
                                inner.min,
                                egui::vec2(full_width - 32.0, 4000.0),
                            ))
                            .layout(Layout::top_down(Align::Min)),
                    );
                    card.set_clip_rect(outer.shrink(1.0));
                    card.multiply_opacity(((eased - 0.25) / 0.75).clamp(0.0, 1.0));
                    let scroll = ScrollArea::vertical()
                        .id_salt("companion-scroll")
                        .max_height(screen.height() - CHROME_HEIGHT - 104.0)
                        .show(&mut card, |ui| self.companion_panel(ui));
                    let content = scroll.content_size.y.min(scroll.inner_rect.height()) + 32.0;
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(id.with("height"), content));
                }
                outer
            });
        if self.companion.open
            && !just_opened
            && area.response.clicked_elsewhere()
            && !ctx.input(|i| {
                i.pointer
                    .interact_pos()
                    .is_some_and(|pos| area.inner.contains(pos))
            })
        {
            self.companion.open = false;
        }
        if (p > 0.001 && p < 0.999) || (presence > 0.001 && presence < 0.999) {
            ctx.request_repaint();
        }
        self.companion_toast(ctx);
    }

    fn companion_toast(&mut self, ctx: &egui::Context) {
        if self.preferences.quiet_notifications || self.companion.open {
            return;
        }
        let Some(notice) = self
            .companion
            .history
            .front()
            .filter(|n| n.at.elapsed() < Duration::from_secs(6))
            .cloned()
        else {
            return;
        };
        let age = notice.at.elapsed().as_secs_f32();
        let live = ctx.style().animation_time > 0.0;
        let enter = if live {
            egui::emath::easing::cubic_out((age / super::super::motion::PANEL).min(1.0))
        } else {
            1.0
        };
        let leave = ((6.0 - age) / 0.3).clamp(0.0, 1.0);
        egui::Area::new(egui::Id::new("agent-notification"))
            .order(egui::Order::Foreground)
            .anchor(
                egui::Align2::RIGHT_BOTTOM,
                egui::vec2(-16.0 + (1.0 - enter) * 28.0, -40.0),
            )
            .show(ctx, |ui| {
                ui.multiply_opacity(enter * leave);
                let color = state_color(notice.state);
                let shown = egui::Frame::new()
                    .fill(colors().surface)
                    .corner_radius(12)
                    .stroke(egui::Stroke::new(1.0_f32, colors().control_border))
                    .shadow(egui::Shadow {
                        offset: [0, 8],
                        blur: 24,
                        spread: 0,
                        color: egui::Color32::from_black_alpha(90),
                    })
                    .inner_margin(egui::Margin {
                        left: 14,
                        right: 14,
                        top: 12,
                        bottom: 14,
                    })
                    .show(ui, |ui| {
                        ui.set_width(300.0);
                        ui.horizontal(|ui| {
                            let (tile, _) = ui
                                .allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::hover());
                            match provider_of(&notice.text) {
                                Some(p) => super::super::brands::avatar(ui, tile, p, false),
                                None => {
                                    ui.painter()
                                        .rect_filled(tile, 8, color.gamma_multiply(0.15));
                                    activity(ui, tile.center(), notice.state, 0.8);
                                }
                            }
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(tr(notice.state.label()))
                                        .strong()
                                        .color(color),
                                );
                                ui.add(egui::Label::new(secondary(&notice.text)).truncate());
                            });
                        });
                        ui.add_space(4.0);
                        if ui
                            .add(egui::Button::new(muted(tr("Show agents ↗"))).frame(false))
                            .clicked()
                        {
                            self.companion.open = true;
                        }
                    });
                let r = shown.response.rect;
                let remaining = (1.0 - age / 6.0).clamp(0.0, 1.0);
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(r.left() + 12.0, r.bottom() - 4.0),
                        egui::vec2((r.width() - 24.0) * remaining, 2.0),
                    ),
                    1,
                    color.gamma_multiply(0.7),
                );
            });
        ctx.request_repaint_after(Duration::from_millis(if live { 33 } else { 500 }));
    }
}

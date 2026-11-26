//! Update notice, Settings row and restart flow for auto-update.
//!
//! All network and disk work runs on background threads that report through
//! one channel; the UI thread only drains messages and draws.
use eframe::egui::{self, Align2, Frame, Margin, RichText, Sense, Stroke, Vec2};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

use super::{
    Editor,
    dot_loader::{LoaderPattern, dot_glyph},
    i18n::{tr, trf},
    shell::setting_row,
    theme::{self, CONTROL_RADIUS, Icon, SMALL, STATUS_HEIGHT, colors},
};
use crate::updater::{self, Platform, Release, UpdateState, Updates};

/// The silent startup check waits for the window to settle.
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(3);
/// A long-running session looks again now and then.
const RECHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
const DOT_COUNT: usize = 24;
const DOT_SIZE: f32 = 4.0;
const DOT_PITCH: f32 = 6.0;

enum Msg {
    Checked(Result<Option<Release>, String>),
    Progress(u64),
    Downloaded(Result<PathBuf, String>),
}

pub(super) struct Updater {
    updates: Updates,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    created: Instant,
    last_check: Option<Instant>,
    first_check_started: bool,
    dismissed_loaded: bool,
    /// The running check was started from Settings, so its outcome is shown.
    manual: bool,
    /// A short note under the restart controls (installer opened, etc.).
    note: Option<String>,
}

impl Default for Updater {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            updates: Updates::default(),
            tx,
            rx,
            created: Instant::now(),
            last_check: None,
            first_check_started: false,
            dismissed_loaded: false,
            manual: false,
            note: None,
        }
    }
}

impl Updater {
    /// Applies one background message. Returns true when the dismissed
    /// version changed and should be saved.
    fn apply(&mut self, msg: Msg) -> bool {
        let before = self.updates.dismissed().map(str::to_owned);
        match msg {
            Msg::Checked(result) => {
                self.updates.on_checked(result, self.manual);
                self.manual = false;
            }
            Msg::Progress(bytes) => self.updates.on_progress(bytes),
            Msg::Downloaded(result) => self.updates.on_downloaded(result),
        }
        self.updates.dismissed().map(str::to_owned) != before
    }

    fn start_check(&mut self, ctx: &egui::Context, manual: bool) {
        if !self.updates.on_check_started() {
            return;
        }
        self.manual = manual;
        self.note = None;
        self.last_check = Some(Instant::now());
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        thread::spawn(move || {
            let _ = tx.send(Msg::Checked(updater::check_latest()));
            ctx.request_repaint();
        });
    }

    /// Moves Available to Downloading and returns the release to fetch.
    /// With no installer for this system the state becomes an error.
    fn begin_download(&mut self, platform: Platform, appimage: bool) -> Option<Release> {
        let UpdateState::Available(release) = &self.updates.state else {
            return None;
        };
        let release = release.clone();
        let Some(asset) = updater::select_asset(platform, &release.assets, appimage) else {
            self.updates
                .on_error(tr("No installer for this system.").to_owned());
            return None;
        };
        let total = asset.size;
        self.note = None;
        self.updates.on_download_started(total).then_some(release)
    }

    fn start_download(&mut self, ctx: &egui::Context) {
        let (platform, appimage) = (Platform::current(), updater::running_appimage());
        let Some(release) = self.begin_download(platform, appimage) else {
            return;
        };
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        thread::spawn(move || {
            let progress = {
                let (tx, ctx) = (tx.clone(), ctx.clone());
                move |bytes| {
                    let _ = tx.send(Msg::Progress(bytes));
                    ctx.request_repaint();
                }
            };
            let result = updater::download(&release, platform, appimage, progress);
            let _ = tx.send(Msg::Downloaded(result));
            ctx.request_repaint();
        });
    }

    fn dismiss(&mut self) {
        if self.updates.dismiss() {
            updater::save_dismissed(self.updates.dismissed());
        }
        self.note = None;
    }
}

/// How many of `count` dots are lit at `fraction` (0..=1).
fn lit_dots(fraction: f32, count: usize) -> usize {
    ((fraction.clamp(0.0, 1.0) * count as f32).round() as usize).min(count)
}

fn percent(fraction: f32) -> u32 {
    (fraction.clamp(0.0, 1.0) * 100.0).round() as u32
}

/// A row of small square dots, lit from the left in proportion to `fraction`.
fn dot_progress(ui: &mut egui::Ui, fraction: f32) {
    let colors = colors();
    let size = Vec2::new(
        DOT_COUNT as f32 * DOT_PITCH - (DOT_PITCH - DOT_SIZE),
        DOT_SIZE,
    );
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::ProgressIndicator,
            true,
            "Download progress",
        )
    });
    if !ui.is_rect_visible(rect) {
        return;
    }
    let lit = lit_dots(fraction, DOT_COUNT);
    for index in 0..DOT_COUNT {
        let min = egui::pos2(rect.left() + index as f32 * DOT_PITCH, rect.top());
        let color = if index < lit {
            colors.accent
        } else {
            colors.muted.gamma_multiply(0.3)
        };
        ui.painter().rect_filled(
            egui::Rect::from_min_size(min, Vec2::splat(DOT_SIZE)),
            1.0,
            color,
        );
    }
}

enum Action {
    Update,
    Notes(String),
    Dismiss,
    Restart,
}

impl Editor {
    /// Call once per frame. Starts the silent startup check, collects results
    /// from background jobs and keeps repainting only while one runs.
    pub(super) fn tick_updater(&mut self, ctx: &egui::Context) {
        let u = &mut self.updater;
        if !u.dismissed_loaded {
            u.dismissed_loaded = true;
            u.updates.set_dismissed(updater::load_dismissed());
        }
        let mut save = false;
        while let Ok(msg) = u.rx.try_recv() {
            save |= u.apply(msg);
        }
        if save {
            updater::save_dismissed(u.updates.dismissed());
        }
        if !u.first_check_started {
            let wait = FIRST_CHECK_DELAY.saturating_sub(u.created.elapsed());
            if wait.is_zero() {
                u.first_check_started = true;
                u.start_check(ctx, false);
            } else {
                ctx.request_repaint_after(wait);
            }
        } else if matches!(u.updates.state, UpdateState::Idle | UpdateState::UpToDate)
            && u.last_check.is_some_and(|t| t.elapsed() >= RECHECK_EVERY)
        {
            u.start_check(ctx, false);
        }
        if u.updates.is_busy() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    /// Quits into the installer once nothing unsaved is left.
    fn restart_now(&mut self, ctx: &egui::Context) {
        let UpdateState::ReadyToRestart { installer, .. } = self.updater.updates.state.clone()
        else {
            return;
        };
        if self.documents.iter().any(|d| d.dirty) {
            return;
        }
        let exits = updater::restart_exits(&installer);
        if exits {
            if !self.save_conversation() {
                return;
            }
            self.persist_all_state();
        }
        if let Err(message) = updater::restart_into(&installer) {
            self.updater.updates.on_error(message);
            return;
        }
        if exits {
            self.force_close = true;
            self.chat.run.take();
            self.hub.general_chat.run.take();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            self.updater.note =
                Some(tr("The installer is open. Finish it, then restart Covalos.").to_owned());
        }
    }

    fn run_update_action(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::Update => self.updater.start_download(ctx),
            Action::Notes(url) => ctx.open_url(egui::OpenUrl::new_tab(url)),
            Action::Dismiss => self.updater.dismiss(),
            Action::Restart => self.restart_now(ctx),
        }
    }

    /// The small pill above the status bar. Draws nothing unless there is an
    /// update to announce, a download to show or an error to report.
    pub(super) fn update_notice(&mut self, ctx: &egui::Context) {
        let state = self.updater.updates.state.clone();
        if !matches!(
            state,
            UpdateState::Available(_)
                | UpdateState::Downloading { .. }
                | UpdateState::ReadyToRestart { .. }
                | UpdateState::Error(_)
        ) {
            return;
        }
        let dirty = self.documents.iter().any(|d| d.dirty);
        let fraction = self.updater.updates.fraction();
        let note = self.updater.note.clone();
        let colors = colors();
        let mut action = None;
        egui::Area::new(egui::Id::new("update-notice"))
            .order(egui::Order::Foreground)
            .anchor(
                Align2::RIGHT_BOTTOM,
                Vec2::new(-12.0, -(STATUS_HEIGHT + 10.0)),
            )
            .show(ctx, |ui| {
                Frame::new()
                    .fill(colors.elevated)
                    .stroke(Stroke::new(1.0_f32, colors.border))
                    .corner_radius(CONTROL_RADIUS)
                    .inner_margin(Margin::symmetric(10, 6))
                    .shadow(theme::floating_shadow(theme::active(), 12))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            let label =
                                |text: String| RichText::new(text).size(SMALL).color(colors.text);
                            match &state {
                                UpdateState::Available(release) => {
                                    ui.label(label(trf(
                                        "Update available: {version}",
                                        &[("version", release.tag.clone())],
                                    )));
                                    if ui.add(theme::primary("Update")).clicked() {
                                        action = Some(Action::Update);
                                    }
                                    if ui.button(tr("Release notes")).clicked() {
                                        action = Some(Action::Notes(release.html_url.clone()));
                                    }
                                }
                                UpdateState::Downloading { .. } => {
                                    dot_glyph(ui, 16.0, LoaderPattern::Orbit, colors.accent);
                                    dot_progress(ui, fraction.unwrap_or(0.0));
                                    ui.label(label(match fraction {
                                        Some(f) => trf(
                                            "Downloading {percent}%",
                                            &[("percent", percent(f).to_string())],
                                        ),
                                        None => tr("Downloading").to_owned(),
                                    }));
                                }
                                UpdateState::ReadyToRestart { installer, .. } => {
                                    ui.label(label(tr("Update ready").to_owned()));
                                    if dirty {
                                        ui.label(
                                            RichText::new(tr("Save your files first"))
                                                .size(SMALL)
                                                .color(colors.warning),
                                        );
                                    } else {
                                        let text = if updater::restart_exits(installer) {
                                            "Restart now"
                                        } else {
                                            "Open installer"
                                        };
                                        if ui.add(theme::primary(text)).clicked() {
                                            action = Some(Action::Restart);
                                        }
                                    }
                                    if let Some(note) = &note {
                                        ui.label(theme::muted(note.clone()));
                                    }
                                }
                                UpdateState::Error(message) => {
                                    ui.label(
                                        RichText::new(message).size(SMALL).color(colors.error),
                                    );
                                }
                                _ => {}
                            }
                            if matches!(state, UpdateState::Available(_) | UpdateState::Error(_))
                                && theme::icon_button_sized(
                                    ui,
                                    Icon::Close,
                                    "Dismiss update",
                                    false,
                                    20.0,
                                )
                                .clicked()
                            {
                                action = Some(Action::Dismiss);
                            }
                        });
                    });
            });
        if let Some(action) = action {
            self.run_update_action(ctx, action);
        }
    }

    /// The Updates block of the Settings page.
    pub(super) fn update_settings(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let state = self.updater.updates.state.clone();
        let dirty = self.documents.iter().any(|d| d.dirty);
        let fraction = self.updater.updates.fraction();
        let status = match &state {
            UpdateState::Idle => tr("Covalos checks for updates when it starts.").to_owned(),
            UpdateState::Checking => tr("Checking for updates.").to_owned(),
            UpdateState::UpToDate => tr("You are up to date.").to_owned(),
            UpdateState::Available(r) => {
                trf("{version} is available.", &[("version", r.tag.clone())])
            }
            UpdateState::Downloading { release, .. } => trf(
                "Downloading {version}.",
                &[("version", release.tag.clone())],
            ),
            UpdateState::ReadyToRestart { release, .. } => trf(
                "{version} is ready to install.",
                &[("version", release.tag.clone())],
            ),
            UpdateState::Error(message) => message.clone(),
        };
        let subtitle = format!("{} {}. {status}", tr("Version"), env!("CARGO_PKG_VERSION"));
        let mut check = false;
        let mut action = None;
        setting_row(ui, "Updates", &subtitle, |ui| match &state {
            UpdateState::Checking => {
                dot_glyph(ui, 16.0, LoaderPattern::Orbit, colors().accent);
            }
            UpdateState::Available(_) => {
                if ui.add(theme::primary("Update")).clicked() {
                    action = Some(Action::Update);
                }
            }
            UpdateState::Downloading { .. } => {
                ui.label(theme::muted(match fraction {
                    Some(f) => trf(
                        "Downloading {percent}%",
                        &[("percent", percent(f).to_string())],
                    ),
                    None => tr("Downloading").to_owned(),
                }));
            }
            UpdateState::ReadyToRestart { installer, .. } => {
                let text = if updater::restart_exits(installer) {
                    "Restart now"
                } else {
                    "Open installer"
                };
                if dirty {
                    ui.label(theme::muted("Save your files first"));
                } else if ui.add(theme::primary(text)).clicked() {
                    action = Some(Action::Restart);
                }
            }
            _ => check = ui.button(tr("Check for updates")).clicked(),
        });
        if let UpdateState::Available(release) = &state
            && ui.link(tr("Release notes")).clicked()
        {
            action = Some(Action::Notes(release.html_url.clone()));
        }
        if let Some(note) = &self.updater.note {
            ui.label(theme::muted(note.clone()));
        }
        if check {
            self.updater.start_check(&ctx, true);
        }
        if let Some(action) = action {
            self.run_update_action(&ctx, action);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::updater::{Asset, Version};

    fn release(assets: &[&str]) -> Release {
        Release {
            tag: "v9.0.0".into(),
            version: Version::parse("9.0.0").unwrap(),
            notes: String::new(),
            html_url: "https://github.com/DevModee/Covalos/releases/tag/v9.0.0".into(),
            assets: assets
                .iter()
                .map(|name| Asset {
                    name: (*name).into(),
                    url: format!("https://example.com/{name}"),
                    size: 2000,
                })
                .collect(),
        }
    }

    fn available(updater: &mut Updater, release: Release) {
        updater.updates.on_check_started();
        updater.manual = true;
        updater.apply(Msg::Checked(Ok(Some(release))));
    }

    #[test]
    fn dots_light_up_in_proportion() {
        assert_eq!(lit_dots(0.0, 24), 0);
        assert_eq!(lit_dots(0.5, 24), 12);
        assert_eq!(lit_dots(1.0, 24), 24);
        assert_eq!(lit_dots(7.0, 24), 24, "over 100% is clamped");
        assert_eq!(lit_dots(-1.0, 24), 0);
        assert_eq!(percent(0.424), 42);
        assert_eq!(percent(2.0), 100);
    }

    #[test]
    fn a_silent_failed_check_shows_nothing_but_a_manual_one_does() {
        let mut updater = Updater::default();
        updater.updates.on_check_started();
        updater.apply(Msg::Checked(Err("No public release found".into())));
        assert_eq!(updater.updates.state, UpdateState::Idle);

        updater.updates.on_check_started();
        updater.manual = true;
        updater.apply(Msg::Checked(Err("No public release found".into())));
        assert!(matches!(updater.updates.state, UpdateState::Error(_)));
        assert!(!updater.manual, "the manual flag is spent after one check");
    }

    #[test]
    fn messages_that_arrive_late_are_ignored() {
        let mut updater = Updater::default();
        updater.apply(Msg::Progress(10));
        updater.apply(Msg::Downloaded(Ok(PathBuf::from("x"))));
        updater.apply(Msg::Checked(Ok(None)));
        assert_eq!(updater.updates.state, UpdateState::Idle);
    }

    #[test]
    fn a_download_needs_an_installer_for_this_system() {
        let mut updater = Updater::default();
        assert!(
            updater.begin_download(Platform::current(), false).is_none(),
            "nothing to download while idle"
        );
        available(&mut updater, release(&["README.txt"]));
        assert!(updater.begin_download(Platform::current(), false).is_none());
        assert!(matches!(updater.updates.state, UpdateState::Error(_)));
    }

    #[test]
    fn a_download_starts_with_the_asset_size_and_progress_is_applied() {
        let mut updater = Updater::default();
        let names = [
            "Covalos-Windows-x64-Setup.exe",
            "Covalos-macOS-arm64.dmg",
            "Covalos-macOS-x64.dmg",
            "Covalos-Linux-x86_64.AppImage",
            "Covalos-Linux-amd64.deb",
        ];
        let release = release(&names);
        if updater::select_asset(Platform::current(), &release.assets, false).is_none() {
            return;
        }
        available(&mut updater, release);
        assert!(updater.begin_download(Platform::current(), false).is_some());
        updater.apply(Msg::Progress(500));
        assert_eq!(updater.updates.fraction(), Some(0.25));
        updater.apply(Msg::Downloaded(Ok(PathBuf::from("setup.exe"))));
        assert!(matches!(
            updater.updates.state,
            UpdateState::ReadyToRestart { .. }
        ));
    }

    #[test]
    fn a_manual_check_reports_a_dismissed_version_again() {
        let mut updater = Updater::default();
        updater.updates.set_dismissed(Some("v9.0.0".into()));
        updater.updates.on_check_started();
        assert!(!updater.apply(Msg::Checked(Ok(Some(release(&[]))))));
        assert_eq!(updater.updates.state, UpdateState::Idle);

        updater.updates.on_check_started();
        updater.manual = true;
        assert!(
            updater.apply(Msg::Checked(Ok(Some(release(&[]))))),
            "clearing the dismissed version must be saved"
        );
        assert!(matches!(updater.updates.state, UpdateState::Available(_)));
    }
}

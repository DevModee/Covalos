//! Agent Hub V4/V5 screens: accounts, live sessions with approvals and shared
//! context, and the autonomous engine. Drawn with the existing COVALOS
//! components (section, row, primary, checkbox, Frame surfaces).
use std::{path::PathBuf, sync::Arc};

use covalos_agents::{
    Account, AccountId, ConnectionMethod, ProviderKind, context::EntryKind, permission::Resolution,
};
use covalos_engine::TaskStatus;
use eframe::egui::{self, Align, Frame, Layout, Margin, RichText, ScrollArea, TextEdit, Ui};

use super::{Editor, theme::*};
use crate::agents_runtime::Runtime;

#[derive(Default)]
pub(super) struct State {
    pub(super) runtime: Option<Runtime>,
    open_error: Option<String>,
    provider_selection: Option<AccountId>,
    login: Option<ProviderLogin>,
    pub(super) secret: String,
    pub(super) secret_for: Option<AccountId>,
    #[cfg(all(test, windows))]
    status: Vec<(AccountId, String)>,
    pub(super) selected: Option<usize>,
    note: String,
    pub(super) chat: super::agent_chat::ChatState,
    pub(super) luma: super::luma_ui::LumaChat,
}

impl State {
    pub(super) fn runtime_and_chat(&mut self) -> (&mut Runtime, &mut super::agent_chat::ChatState) {
        (self.runtime.as_mut().expect("runtime"), &mut self.chat)
    }

    pub(super) fn open_error_text(&self) -> Option<String> {
        self.open_error.clone()
    }
}

struct ProviderLogin {
    id: AccountId,
    process: LoginProcess,
    focus: bool,
}

enum LoginProcess {
    Terminal {
        shell: Box<super::pty::Shell>,
        command: String,
        gemini: Option<GeminiLoginWatch>,
    },
    Acp {
        events: std::sync::mpsc::Receiver<LoginEvent>,
        cancel: Arc<std::sync::atomic::AtomicBool>,
        links: Vec<String>,
        verifying: bool,
    },
}

type GeminiStamp = [Option<std::time::SystemTime>; 2];

struct GeminiLoginWatch {
    config: PathBuf,
    observed: GeminiStamp,
    checked: GeminiStamp,
    changed: std::time::Instant,
}

impl GeminiLoginWatch {
    fn new(config: PathBuf) -> Self {
        Self {
            config,
            observed: [None; 2],
            checked: [None; 2],
            changed: std::time::Instant::now(),
        }
    }

    fn has_checked(&self) -> bool {
        self.checked.iter().any(Option::is_some)
    }

    fn should_verify(&mut self) -> bool {
        let stamp = ["oauth_creds.json", "google_accounts.json"].map(|file| {
            std::fs::metadata(self.config.join(file))
                .and_then(|metadata| metadata.modified())
                .ok()
        });
        if stamp != self.observed {
            self.observed = stamp;
            self.changed = std::time::Instant::now();
        }
        if self.observed == self.checked
            || self.changed.elapsed() < std::time::Duration::from_secs(1)
        {
            return false;
        }
        self.checked = self.observed;
        self.observed.iter().any(Option::is_some)
    }
}

#[cfg(test)]
#[test]
fn gemini_verifies_once_after_the_oauth_profile_settles() {
    use std::time::{Duration, Instant};

    let config = tempfile::tempdir().unwrap();
    let mut watch = GeminiLoginWatch::new(config.path().to_path_buf());
    assert!(!watch.should_verify());
    std::fs::write(config.path().join("google_accounts.json"), b"{}").unwrap();
    assert!(!watch.should_verify());
    watch.changed = Instant::now() - Duration::from_secs(2);
    assert!(watch.should_verify());
    assert!(!watch.should_verify());
}

enum LoginEvent {
    Link(String),
    Verifying,
    Done(Result<crate::agent_provider::Catalog, String>),
}

impl Drop for ProviderLogin {
    fn drop(&mut self) {
        if let LoginProcess::Acp { cancel, .. } = &self.process {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

impl State {
    fn poll_login(&mut self) {
        let (Some(login), Some(runtime)) = (&mut self.login, &mut self.runtime) else {
            return;
        };
        use crate::agents_runtime::Connection;
        if runtime.connection_ready(&login.id)
            || runtime.connection.get(&login.id) == Some(&Connection::GeminiMigrationRequired)
        {
            self.login = None;
            return;
        }
        let checking = matches!(
            runtime.connection.get(&login.id),
            Some(Connection::Checking)
        );
        let mut finished = false;
        match &mut login.process {
            LoginProcess::Terminal { shell, gemini, .. } => {
                if gemini.is_some()
                    && crate::agent_provider::gemini_oauth_retired(&shell.contents())
                {
                    runtime.mark_gemini_oauth_retired(&login.id);
                    self.login = None;
                    return;
                }
                finished = shell.process_finished();
                if !finished
                    && !checking
                    && gemini.as_mut().is_some_and(GeminiLoginWatch::should_verify)
                {
                    runtime.refresh_connection(&login.id);
                }
            }
            LoginProcess::Acp {
                events,
                links,
                verifying,
                ..
            } => loop {
                match events.try_recv() {
                    Ok(LoginEvent::Link(link)) if !links.contains(&link) => links.push(link),
                    Ok(LoginEvent::Link(_)) => {}
                    Ok(LoginEvent::Verifying) => {
                        *verifying = true;
                        links.clear();
                    }
                    Ok(LoginEvent::Done(result)) => {
                        runtime.finish_antigravity_login(&login.id, result);
                        self.login = None;
                        return;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        runtime.finish_antigravity_login(
                            &login.id,
                            Err("The provider closed the connection.".into()),
                        );
                        self.login = None;
                        return;
                    }
                }
            },
        }
        if finished {
            if !checking {
                runtime.refresh_connection(&login.id);
            }
            self.login = None;
        }
    }
}

fn add_provider(
    runtime: &mut Runtime,
    selection: &mut Option<AccountId>,
    kind: &ProviderKind,
    name: &str,
    method: ConnectionMethod,
) {
    let index = 1 + runtime
        .profiles
        .accounts()
        .iter()
        .filter(|a| &a.provider == kind && a.method != ConnectionMethod::ApiKey)
        .count();
    let label = if method == ConnectionMethod::SystemCli {
        format!("{name} ({})", tr("PC login"))
    } else {
        format!("{name} {index}")
    };
    let result = new_account_id(kind).and_then(|id| {
        runtime.add_account(Account::new(id.clone(), kind.clone(), label, method))?;
        runtime.refresh_connection(&id);
        Ok(id)
    });
    match result {
        Ok(id) => {
            *selection = Some(id);
            runtime.notice = None;
        }
        Err(error) => runtime.notice = Some((false, error)),
    }
}

fn setup_antigravity(runtime: &mut Runtime, selection: &mut Option<AccountId>) {
    let kind = ProviderKind::Antigravity;
    if let Some(existing) = runtime
        .profiles
        .accounts()
        .iter()
        .find(|a| a.provider == kind && a.method == ConnectionMethod::SystemCli)
    {
        *selection = Some(existing.id.clone());
        runtime.notice = None;
    } else {
        add_provider(
            runtime,
            selection,
            &kind,
            "Antigravity",
            ConnectionMethod::SystemCli,
        );
    }
}

#[cfg(test)]
#[test]
fn gemini_migration_creates_and_reuses_the_supported_antigravity_account() {
    let dir = tempfile::tempdir().unwrap();
    let mut runtime = Runtime::open(dir.path().to_path_buf(), Arc::new(|| {})).unwrap();
    let mut selection = None;
    runtime.notice = Some((false, "stale notice".into()));
    setup_antigravity(&mut runtime, &mut selection);
    let id = selection.take().unwrap();
    let account = runtime.profiles.get(&id).unwrap();
    assert_eq!(account.provider, ProviderKind::Antigravity);
    assert_eq!(account.method, ConnectionMethod::SystemCli);
    assert!(runtime.notice.is_none());

    setup_antigravity(&mut runtime, &mut selection);
    assert_eq!(selection, Some(id));
    assert_eq!(runtime.profiles.accounts().len(), 1);
}

pub(super) fn new_account_id(kind: &ProviderKind) -> Result<AccountId, String> {
    static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sequence = SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    AccountId::new(format!(
        "{}-{:x}-{sequence:x}",
        kind.key(),
        covalos_agents::now_ms()
    ))
    .map_err(|e| e.to_string())
}

fn provider_status(runtime: &Runtime, account: &Account) -> (egui::Color32, &'static str, String) {
    use crate::agents_runtime::Connection;
    if !account.enabled {
        return (colors().muted, "Disabled", String::new());
    }
    if account.provider == ProviderKind::Gemini {
        return (
            colors().warning,
            "Replaced by Antigravity",
            tr("Google moved Gemini CLI to Antigravity. Add Antigravity and remove this account.")
                .to_owned(),
        );
    }
    if runtime.program(&account.provider).is_none() {
        return (colors().error, "Not installed", String::new());
    }
    match runtime.connection.get(&account.id) {
        Some(Connection::SignedIn(identity)) => {
            let mut detail = identity.email.as_ref().map_or_else(
                || tr("Email unavailable from provider").to_owned(),
                |email| trf("Authenticated as {email}", &[("email", email.clone())]),
            );
            detail.push_str(" \u{00b7} ");
            detail.push_str(
                identity
                    .plan
                    .as_deref()
                    .unwrap_or(tr("Plan unavailable from provider")),
            );
            (colors().success, "Authenticated", detail)
        }
        Some(Connection::SignedOut(reason)) => {
            (colors().warning, "Not authenticated", tr(reason).to_owned())
        }
        Some(Connection::GeminiMigrationRequired) => (
            colors().warning,
            "Migration required",
            tr("Gemini CLI personal sign-in moved to Antigravity.").into(),
        ),
        Some(Connection::Unknown(reason)) => {
            (colors().warning, "Status unknown", tr(reason).to_owned())
        }
        Some(Connection::Checking) | None => (colors().muted, "Checking.", String::new()),
        Some(Connection::Key(_)) => (colors().muted, "Status unknown", String::new()),
    }
}

fn status_dot(ui: &mut Ui, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(8.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 3.0, color);
}

fn provider_switch(ui: &mut Ui, enabled: &mut bool) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(egui::vec2(32.0, 20.0), egui::Sense::click());
    if response.clicked() {
        *enabled = !*enabled;
        response.mark_changed();
    }
    let label = tr("Enable provider");
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, *enabled, label)
    });
    let track = rect.shrink(1.0);
    ui.painter().rect_filled(
        track,
        9,
        if *enabled {
            colors().accent
        } else {
            colors().control_border
        },
    );
    if response.has_focus() {
        ui.painter().rect_stroke(
            track.expand(1.0),
            10,
            egui::Stroke::new(1.0_f32, colors().accent_hover),
            egui::StrokeKind::Outside,
        );
    }
    let x = if *enabled {
        track.right() - 9.0
    } else {
        track.left() + 9.0
    };
    ui.painter().circle_filled(
        egui::pos2(x, track.center().y),
        6.0,
        if *enabled {
            colors().canvas
        } else {
            colors().text
        },
    );
    response.on_hover_text(label)
}

fn start_login(
    runtime: &mut Runtime,
    account: &Account,
    ctx: &egui::Context,
) -> Result<ProviderLogin, String> {
    let command = runtime.login_command(&account.id)?;
    let process = if account.provider == ProviderKind::Antigravity {
        let (tx, events) = std::sync::mpsc::channel();
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop = cancel.clone();
        let wake = ctx.clone();
        let cwd = runtime.dir.clone();
        std::thread::spawn(move || {
            let result = crate::agent_provider::antigravity_login(
                command,
                &cwd,
                &stop,
                |url| {
                    let _ = tx.send(LoginEvent::Link(url));
                    wake.request_repaint();
                },
                || {
                    let _ = tx.send(LoginEvent::Verifying);
                    wake.request_repaint();
                },
            );
            let _ = tx.send(LoginEvent::Done(result));
            wake.request_repaint();
        });
        LoginProcess::Acp {
            events,
            cancel,
            links: Vec::new(),
            verifying: false,
        }
    } else {
        let title = match account.provider {
            ProviderKind::ClaudeCode => "claude auth login",
            ProviderKind::Codex => "codex login",
            ProviderKind::Gemini => "gemini",
            _ => "opencode auth login",
        };
        let gemini = (account.provider == ProviderKind::Gemini)
            .then(|| crate::agent_provider::gemini_config(&command))
            .flatten()
            .map(GeminiLoginWatch::new);
        let pty = crate::runtime_install::pty_command(&command, &account.provider)?;
        let shell =
            super::pty::Shell::spawn_command(u32::MAX, runtime.dir.clone(), ctx, pty, true)?;
        LoginProcess::Terminal {
            shell: Box::new(shell),
            command: title.into(),
            gemini,
        }
    };
    runtime.invalidate_connection(&account.id);
    Ok(ProviderLogin {
        id: account.id.clone(),
        process,
        focus: true,
    })
}

fn provider_detail(
    ui: &mut Ui,
    runtime: &mut Runtime,
    current_login: &mut Option<ProviderLogin>,
    selection: &mut Option<AccountId>,
    account: &Account,
) {
    use crate::runtime_install::Progress;
    let (color, status, detail) = provider_status(runtime, account);
    let migration = runtime.connection.get(&account.id)
        == Some(&crate::agents_runtime::Connection::GeminiMigrationRequired);
    let login = current_login
        .as_ref()
        .filter(|login| login.id == account.id);
    let login_active = login.is_some();
    let verified_once = login.is_some_and(|login| matches!(
        &login.process, LoginProcess::Terminal { gemini: Some(watch), .. } if watch.has_checked()
    ));
    let checking = matches!(
        runtime.connection.get(&account.id),
        Some(crate::agents_runtime::Connection::Checking)
    );
    let needs_attention = verified_once && !checking && !detail.is_empty();
    let verifying_acp = login.is_some_and(|login| {
        matches!(
            &login.process,
            LoginProcess::Acp {
                verifying: true,
                ..
            }
        )
    });
    card(ui, |ui| {
        ui.horizontal(|ui| {
            super::agent_chat::provider_mark(ui, &account.provider, 20.0);
            ui.label(heading(&account.label));
        });
        ui.add_space(UNIT);
        ui.horizontal(|ui| {
            let waiting = login_active && !needs_attention;
            status_dot(ui, if waiting { colors().accent } else { color });
            let shown = if waiting {
                if checking || verifying_acp {
                    "Verifying sign-in"
                } else {
                    "Signing in"
                }
            } else {
                status
            };
            ui.label(RichText::new(tr(shown)).strong());
        });
        if (!login_active || needs_attention) && !detail.is_empty() {
            ui.label(secondary(detail));
        }
        ui.add_space(UNIT * 2.0);
        ui.label(label("Display name"));
        let name_id = ui.make_persistent_id(("provider-name", &account.id));
        let mut name = ui
            .ctx()
            .data(|data| data.get_temp::<String>(name_id))
            .unwrap_or_else(|| account.label.clone());
        let edit = ui.add(
            TextEdit::singleline(&mut name)
                .id(name_id)
                .char_limit(64)
                .desired_width(260.0),
        );
        if edit.lost_focus() {
            if !name.trim().is_empty() && name.trim() != account.label {
                let mut updated = runtime
                    .profiles
                    .get(&account.id)
                    .cloned()
                    .unwrap_or_else(|_| account.clone());
                updated.label = name.trim().into();
                if let Err(error) = runtime.update_account(updated) {
                    runtime.notice = Some((false, error));
                }
            }
            ui.ctx().data_mut(|data| data.remove::<String>(name_id));
        } else if edit.changed() {
            ui.ctx().data_mut(|data| data.insert_temp(name_id, name));
        }
        ui.add_space(UNIT * 2.0);
        ui.label(label("Account storage"));
        let antigravity = account.provider == ProviderKind::Antigravity;
        ui.label(secondary(if antigravity {
            "Shared Google sign-in"
        } else if account.method == ConnectionMethod::SystemCli {
            "Use PC login"
        } else {
            "Isolated sign-in"
        }));
        if let Ok(spec) = runtime.registry.get(&account.provider)
            && let Some(var) = &spec.config_dir_var
        {
            ui.label(muted(var));
            let path = if account.method != ConnectionMethod::SystemCli {
                runtime
                    .profiles
                    .isolated_dir(&account.id)
                    .display()
                    .to_string()
            } else {
                match account.provider {
                    ProviderKind::ClaudeCode => "~/.claude",
                    ProviderKind::Codex => "~/.codex",
                    ProviderKind::Gemini | ProviderKind::Antigravity => "~/.gemini",
                    _ => "~/.config",
                }
                .to_owned()
            };
            ui.label(
                RichText::new(path)
                    .monospace()
                    .small()
                    .color(colors().secondary),
            );
        }
        ui.add_space(UNIT * 2.0);
        ui.label(label("Runtime"));
        if let Some(install) = runtime.installing.get(&account.provider) {
            let progress = install
                .progress
                .lock()
                .map(|progress| progress.clone())
                .unwrap_or_default();
            ui.horizontal_wrapped(|ui| {
                busy_indicator(ui);
                let label = match progress {
                    Progress::Preparing => tr("Preparing installation").to_owned(),
                    Progress::Downloading(0) => tr("Downloading runtime").to_owned(),
                    Progress::Downloading(bytes) => trf(
                        "Downloaded {size} MiB",
                        &[("size", format!("{:.1}", bytes as f64 / 1_048_576.0))],
                    ),
                    Progress::Installing => tr("Installing runtime").to_owned(),
                    Progress::Cancelling => tr("Cancelling installation").to_owned(),
                };
                ui.label(secondary(label));
                if ui
                    .add_enabled(
                        !matches!(progress, Progress::Cancelling),
                        egui::Button::new(tr("Cancel installation")),
                    )
                    .clicked()
                {
                    install.cancel();
                }
            });
        } else if let Some(program) = runtime.program(&account.provider) {
            ui.label(
                RichText::new(program.display().to_string())
                    .monospace()
                    .small()
                    .color(colors().muted),
            );
        } else if ui.add(primary("Install runtime")).clicked()
            && let Err(error) = runtime.install_provider(account.provider.clone())
        {
            runtime.notice = Some((false, error));
        }
        ui.add_space(UNIT * 2.0);
        ui.horizontal_wrapped(|ui| {
            if migration {
                let setup = ui.add(primary("Set up Antigravity"));
                #[cfg(test)]
                super::motion::record(ui, "Set up Antigravity", setup.rect);
                if setup.clicked() {
                    setup_antigravity(runtime, selection);
                }
            } else if !login_active
                && ui
                    .add_enabled(
                        account.enabled
                            && (!matches!(
                                account.provider,
                                ProviderKind::Gemini | ProviderKind::Antigravity
                            ) || !checking)
                            && runtime.program(&account.provider).is_some()
                            && current_login.is_none(),
                        primary("Sign in"),
                    )
                    .clicked()
            {
                match start_login(runtime, account, ui.ctx()) {
                    Ok(login) => *current_login = Some(login),
                    Err(error) => runtime.notice = Some((false, error)),
                }
            }
            if (!login_active || needs_attention)
                && ui
                    .add_enabled(
                        account.enabled && !checking,
                        egui::Button::new(tr("Check sign-in")),
                    )
                    .clicked()
            {
                runtime.refresh_connection(&account.id);
            }
            if ui.small_button(tr("Remove")).clicked() {
                if login_active {
                    *current_login = None;
                }
                if let Err(error) = runtime.remove_account(&account.id) {
                    runtime.notice = Some((false, error));
                }
            }
        });
        if current_login
            .as_ref()
            .is_some_and(|login| login.id == account.id)
        {
            ui.add_space(UNIT * 2.0);
            ui.horizontal(|ui| {
                ui.label(label("Provider sign-in"));
                if ui.small_button(tr("Cancel")).clicked() {
                    *current_login = None;
                    runtime.refresh_connection(&account.id);
                }
            });
            if let Some(login) = current_login.as_mut() {
                let links = match &mut login.process {
                    LoginProcess::Terminal { shell, command, .. } => {
                        ui.label(RichText::new(command.as_str()).monospace().small());
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), 158.0),
                            egui::Sense::hover(),
                        );
                        shell.show(ui, rect, std::mem::take(&mut login.focus));
                        shell.urls()
                    }
                    LoginProcess::Acp {
                        links, verifying, ..
                    } => {
                        Frame::new()
                            .fill(colors().inset)
                            .inner_margin(Margin::same(12))
                            .corner_radius(CONTROL_RADIUS)
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new("ACP / authenticate / oauth-personal")
                                        .monospace()
                                        .small(),
                                );
                                ui.label(
                                    RichText::new(tr(if *verifying {
                                        "Checking Antigravity access and models."
                                    } else if links.is_empty() {
                                        "Starting Antigravity sign-in."
                                    } else {
                                        "Waiting for Google sign-in."
                                    }))
                                    .monospace()
                                    .small(),
                                );
                            });
                        if login.focus
                            && let Some(url) = links.first()
                        {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                            login.focus = false;
                        }
                        links.clone()
                    }
                };
                for url in links.iter().rev().take(1) {
                    ui.horizontal(|ui| {
                        ui.hyperlink_to(tr("Open sign-in link"), url);
                        if icon_button(ui, Icon::Copy, "Copy sign-in link", false).clicked() {
                            ui.ctx().copy_text(url.clone());
                        }
                    });
                }
            }
        }
        if let Some(warning) = runtime.catalog_warning(&account.id) {
            ui.label(secondary(tr(warning)));
        }
    });
}

/// Providers offered when adding an account. Gemini CLI is left out: Google moved it to
/// Antigravity. Saved Gemini CLI accounts still load and are marked as replaced.
const PROVIDERS: [(ProviderKind, &str); 4] = [
    (ProviderKind::ClaudeCode, "Claude Code"),
    (ProviderKind::Codex, "OpenAI Codex"),
    (ProviderKind::Antigravity, "Antigravity"),
    (ProviderKind::OpenCode, "OpenCode"),
];
//
// Agent Hub reads like a document: solid panels over a dimmed field, three
// text tones (primary, secondary, metadata) and a 14.5/21 reading size.

const READ: f32 = 14.5;
const READ_LINE: f32 = 21.0;
const META: f32 = 12.5;
const META_LINE: f32 = 18.0;
const LABEL: f32 = 10.5;
const TITLE: f32 = 19.0;
const GUTTER: f32 = 28.0;

mod ink {
    use super::super::{motion::blend, theme::colors};
    use eframe::egui::Color32;

    /// Headlines and agent output: almost white.
    pub(super) fn primary() -> Color32 {
        blend(colors().text, Color32::WHITE, 0.45)
    }
    /// Supporting copy: clearly visible.
    pub(super) fn secondary() -> Color32 {
        blend(colors().secondary, colors().text, 0.45)
    }
    /// Metadata: quiet but readable.
    pub(super) fn meta() -> Color32 {
        blend(colors().muted, colors().secondary, 0.55)
    }
    /// Solid panel fill, a step above the dimmed background.
    pub(super) fn panel() -> Color32 {
        blend(colors().inset, colors().surface, 0.65)
    }
    pub(super) fn raised() -> Color32 {
        blend(colors().surface, colors().elevated, 0.7)
    }
}

fn card(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    Frame::new()
        .fill(ink::panel())
        .stroke(egui::Stroke::new(
            1.0_f32,
            super::motion::blend(colors().border, colors().control_border, 0.35),
        ))
        .inner_margin(Margin::same(18))
        .corner_radius(12)
        .shadow(egui::epaint::Shadow {
            offset: [0, 6],
            blur: 18,
            spread: 0,
            color: egui::Color32::from_black_alpha(70),
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 8.0;
            add(ui);
        });
    ui.add_space(14.0);
}

/// Metadata text: quiet but legible.
fn small(text: impl Into<String>) -> RichText {
    RichText::new(text.into())
        .size(META)
        .line_height(Some(META_LINE))
        .color(ink::meta())
}

/// Reading text for content people actually read.
fn read(text: impl Into<String>) -> RichText {
    RichText::new(text.into())
        .size(READ)
        .line_height(Some(READ_LINE))
        .color(ink::primary())
}

/// Supporting sentence under a heading.
fn support(text: impl Into<String>) -> RichText {
    RichText::new(text.into())
        .size(BODY + 0.5)
        .line_height(Some(19.0))
        .color(ink::secondary())
}

/// Uppercase section label.
fn label(text: &str) -> RichText {
    RichText::new(tr(text).to_uppercase())
        .size(LABEL)
        .strong()
        .extra_letter_spacing(0.9)
        .color(ink::meta())
}

fn heading(text: impl Into<String>) -> RichText {
    RichText::new(text.into())
        .size(TITLE)
        .strong()
        .color(ink::primary())
}

/// Small rounded tag with a tinted fill.
fn badge(ui: &mut Ui, text: &str, color: egui::Color32) {
    Frame::new()
        .fill(color.gamma_multiply(0.14))
        .stroke(egui::Stroke::new(1.0_f32, color.gamma_multiply(0.38)))
        .corner_radius(5)
        .inner_margin(Margin::symmetric(7, 2))
        .show(ui, |ui| {
            // Never wraps: a squeezed badge would stack one letter per line.
            ui.add(
                egui::Label::new(
                    RichText::new(text)
                        .size(LABEL)
                        .strong()
                        .extra_letter_spacing(0.6)
                        .color(color),
                )
                .extend(),
            );
        });
}

/// A real error, shown in full: wrapped, monospace and selectable so it can
/// be read and copied.
pub(super) fn failure_text(ui: &mut Ui, text: &str) {
    Frame::new()
        .fill(colors().error.gamma_multiply(0.08))
        .stroke(egui::Stroke::new(
            1.0_f32,
            colors().error.gamma_multiply(0.3),
        ))
        .corner_radius(8)
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(
                egui::Label::new(
                    RichText::new(text)
                        .monospace()
                        .size(SMALL)
                        .color(ink::primary()),
                )
                .wrap()
                .selectable(true),
            );
        });
}

impl Editor {
    /// Drives the runtime every frame once the Agent Hub opened it.
    pub(super) fn poll_agent_runtime(&mut self, ctx: &egui::Context) {
        self.hub.agents.poll_login();
        if self.hub.agents.login.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
        let Some(runtime) = self.hub.agents.runtime.as_mut() else {
            return;
        };
        if runtime.tick() {
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        } else if self.hub_open {
            ctx.request_repaint_after(std::time::Duration::from_millis(1000));
        }
        self.luma_watch();
    }

    /// Evidence harness: refresh every account's sign-in status line.
    #[cfg(all(test, windows))]
    pub(super) fn evidence_account_status(&mut self) {
        let state = &mut self.hub.agents;
        let Some(runtime) = state.runtime.as_ref() else {
            return;
        };
        state.status = runtime
            .profiles
            .accounts()
            .iter()
            .filter(|a| a.method != ConnectionMethod::ApiKey && a.provider != ProviderKind::Gemini)
            .map(|a| {
                let text = runtime
                    .account_status(&a.id)
                    .unwrap_or_else(|e| format!("Status unavailable: {e}"));
                (a.id.clone(), text)
            })
            .collect();
    }

    pub(super) fn ensure_runtime(&mut self, ctx: &egui::Context) -> bool {
        let state = &mut self.hub.agents;
        if state.runtime.is_some() {
            return true;
        }
        if state.open_error.is_some() {
            return false;
        }
        let Some(dir) = crate::paths::config_dir().map(|d| d.join("agents")) else {
            state.open_error = Some("No user configuration directory.".into());
            return false;
        };
        let wake_ctx = ctx.clone();
        match Runtime::open(dir, Arc::new(move || wake_ctx.request_repaint())) {
            Ok(runtime) => {
                state.runtime = Some(runtime);
                true
            }
            Err(error) => {
                state.open_error = Some(error);
                false
            }
        }
    }

    fn runtime_unavailable(&mut self, ui: &mut Ui) -> bool {
        let ctx = ui.ctx().clone();
        if self.ensure_runtime(&ctx) {
            return false;
        }
        ui.colored_label(
            colors().error,
            self.hub.agents.open_error.clone().unwrap_or_default(),
        );
        true
    }

    fn agent_notice(&mut self, ui: &mut Ui) {
        let Some(runtime) = self.hub.agents.runtime.as_mut() else {
            return;
        };
        if let Some((ok, text)) = runtime.notice.clone() {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(
                    if ok {
                        colors().success
                    } else {
                        colors().warning
                    },
                    text,
                );
                if ui.small_button(tr("Dismiss notice")).clicked() {
                    runtime.notice = None;
                }
            });
            ui.add_space(6.0);
        }
    }

    #[cfg(all(test, windows))]
    pub(super) fn review_gemini_migration(&mut self) {
        let runtime = self.hub.agents.runtime.as_mut().expect("review runtime");
        let id = AccountId::new("gemini-migration-review").unwrap();
        runtime
            .add_account(Account::new(
                id.clone(),
                ProviderKind::Gemini,
                "Gemini CLI",
                ConnectionMethod::SystemCli,
            ))
            .unwrap();
        runtime.detections.insert(
            ProviderKind::Gemini,
            crate::agents_runtime::Detection {
                program: Some(PathBuf::from("gemini-fixture.cmd")),
                version: Some("0.70.0".into()),
                done: true,
                ..Default::default()
            },
        );
        runtime.mark_gemini_oauth_retired(&id);
        self.hub.agents.provider_selection = Some(id);
        self.open_providers();
    }

    pub(super) fn open_providers(&mut self) {
        self.hub.focus_agents("accounts");
        self.hub_open = true;
    }

    pub(super) fn accounts_panel(&mut self, ui: &mut Ui) {
        if self.runtime_unavailable(ui) {
            return;
        }
        self.agent_notice(ui);
        let state = &mut self.hub.agents;
        let runtime = state.runtime.as_mut().expect("runtime");
        let accounts: Vec<_> = runtime
            .profiles
            .accounts()
            .iter()
            .filter(|account| account.method != ConnectionMethod::ApiKey)
            .cloned()
            .collect();
        if !accounts
            .iter()
            .any(|account| Some(&account.id) == state.provider_selection.as_ref())
        {
            state.provider_selection = accounts.first().map(|account| account.id.clone());
        }

        ui.horizontal(|ui| {
            ui.label(RichText::new(tr("Providers")).size(HEADING).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_button(ui, Icon::Refresh, "Refresh", false).clicked() {
                    runtime.redetect();
                    for account in &accounts {
                        if state
                            .login
                            .as_ref()
                            .is_none_or(|login| login.id != account.id)
                        {
                            runtime.refresh_connection(&account.id);
                        }
                    }
                }
            });
        });
        ui.add_space(UNIT * 2.0);
        let height = (ui.available_height() - UNIT).max(120.0);
        let rail = (ui.available_width() * 0.30).clamp(148.0, 245.0);
        let column_gap = UNIT * 3.0;
        let detail_width = (ui.available_width() - rail - column_gap).max(1.0);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = column_gap;
            ui.allocate_ui_with_layout(
                egui::vec2(rail, height),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.set_width(rail);
                    ui.set_max_width(rail);
                    ScrollArea::vertical()
                        .id_salt("provider-rail")
                        .max_height((height - 38.0).max(80.0))
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            for account in &accounts {
                                let selected =
                                    state.provider_selection.as_ref() == Some(&account.id);
                                let (tone, status, _) = provider_status(runtime, account);
                                let version = runtime
                                    .detections
                                    .get(&account.provider)
                                    .and_then(|det| det.version.clone());
                                Frame::new()
                                    .fill(if selected {
                                        colors().accent_bg
                                    } else {
                                        colors().elevated
                                    })
                                    .corner_radius(CONTROL_RADIUS)
                                    .inner_margin(Margin::same(8))
                                    .show(ui, |ui| {
                                        ui.set_width((rail - 26.0).max(100.0));
                                        ui.horizontal(|ui| {
                                            super::agent_chat::provider_mark(
                                                ui,
                                                &account.provider,
                                                16.0,
                                            );
                                            let width = (ui.available_width() - 40.0).max(45.0);
                                            if ui
                                                .add_sized(
                                                    [width, 26.0],
                                                    egui::Button::new(
                                                        RichText::new(&account.label).strong(),
                                                    )
                                                    .frame(false)
                                                    .truncate(),
                                                )
                                                .on_hover_text(&account.label)
                                                .clicked()
                                            {
                                                state.provider_selection = Some(account.id.clone());
                                            }
                                            let mut enabled = account.enabled;
                                            if provider_switch(ui, &mut enabled).changed() {
                                                if !enabled
                                                    && state
                                                        .login
                                                        .as_ref()
                                                        .is_some_and(|login| login.id == account.id)
                                                {
                                                    state.login = None;
                                                }
                                                let mut updated = account.clone();
                                                updated.enabled = enabled;
                                                if let Err(error) = runtime.update_account(updated)
                                                {
                                                    runtime.notice = Some((false, error));
                                                }
                                            }
                                        });
                                        ui.horizontal(|ui| {
                                            status_dot(ui, tone);
                                            ui.add(egui::Label::new(muted(status)).truncate());
                                        });
                                        if let Some(version) = version {
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(version)
                                                        .monospace()
                                                        .small()
                                                        .color(colors().muted),
                                                )
                                                .truncate(),
                                            );
                                        }
                                    });
                            }
                        });
                    ui.menu_button(tr("+ Add provider"), |ui| {
                        for (kind, name) in &PROVIDERS {
                            ui.horizontal(|ui| {
                                super::agent_chat::provider_mark(ui, kind, 16.0);
                                ui.label(RichText::new(*name).strong());
                            });
                            if runtime
                                .registry
                                .get(kind)
                                .is_ok_and(|spec| spec.supports_isolated_login())
                                && ui.button(tr("Isolated sign-in")).clicked()
                            {
                                add_provider(
                                    runtime,
                                    &mut state.provider_selection,
                                    kind,
                                    name,
                                    ConnectionMethod::OfficialLogin,
                                );
                                ui.close();
                            }
                            let existing = accounts.iter().any(|account| {
                                &account.provider == kind
                                    && account.method == ConnectionMethod::SystemCli
                            });
                            let sign_in = if *kind == ProviderKind::Antigravity {
                                "Shared Google sign-in"
                            } else {
                                "Use PC login"
                            };
                            if ui
                                .add_enabled(!existing, egui::Button::new(tr(sign_in)))
                                .clicked()
                            {
                                add_provider(
                                    runtime,
                                    &mut state.provider_selection,
                                    kind,
                                    name,
                                    ConnectionMethod::SystemCli,
                                );
                                ui.close();
                            }
                            ui.separator();
                        }
                    });
                },
            );
            ui.allocate_ui_with_layout(
                egui::vec2(detail_width, height),
                Layout::top_down(Align::Min),
                |ui| {
                    ScrollArea::vertical()
                        .id_salt("provider-detail")
                        .max_height(height)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            if let Some(account) = accounts.iter().find(|account| {
                                Some(&account.id) == state.provider_selection.as_ref()
                            }) {
                                provider_detail(
                                    ui,
                                    runtime,
                                    &mut state.login,
                                    &mut state.provider_selection,
                                    account,
                                );
                            } else {
                                ui.label(
                                    RichText::new(tr("Connect an AI to start"))
                                        .size(LEAD)
                                        .strong(),
                                );
                                ui.label(secondary(
                                    "Add a provider to sign in with your subscription.",
                                ));
                            }
                        });
                },
            );
        });
    }

    pub(super) fn sessions_panel(&mut self, ui: &mut Ui) {
        if self.runtime_unavailable(ui) {
            return;
        }
        // Older callers (and the evidence harness) select a session by index.
        if let Some(index) = self.hub.agents.selected.take()
            && let Some(id) = self
                .hub
                .agents
                .runtime
                .as_ref()
                .and_then(|r| r.sessions.get(index))
                .map(|s| s.meta.id.as_str().to_owned())
        {
            self.hub.agents.chat.page = Some(id);
        }
        let rect = ui.available_rect_before_wrap();
        let side = if rect.width() > 960.0 { 290.0 } else { 0.0 };
        let main = egui::Rect::from_min_max(
            rect.min,
            egui::pos2(
                rect.right() - side - if side > 0.0 { GUTTER } else { 0.0 },
                rect.bottom(),
            ),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(main), |ui| {
            if side == 0.0 {
                self.approvals_strip(ui);
            }
            Frame::new()
                .fill(ink::panel())
                .stroke(egui::Stroke::new(1.0_f32, colors().border))
                .corner_radius(12)
                .inner_margin(Margin::same(14))
                .show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    self.agents_page(ui);
                });
        });
        if side > 0.0 {
            let column =
                egui::Rect::from_min_max(egui::pos2(rect.right() - side, rect.top()), rect.max);
            ui.scope_builder(egui::UiBuilder::new().max_rect(column), |ui| {
                ScrollArea::vertical()
                    .id_salt("agent-side")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.approvals_strip(ui);
                        self.team_run(ui);
                        self.context_panel(ui);
                    });
            });
        }
    }

    fn approvals_strip(&mut self, ui: &mut Ui) {
        let runtime = self.hub.agents.runtime.as_mut().expect("runtime");
        Self::conversation_approvals(ui, runtime, None);
    }

    pub(super) fn conversation_approvals(
        ui: &mut Ui,
        runtime: &mut crate::agents_runtime::Runtime,
        session: Option<&str>,
    ) {
        let pending: Vec<_> = runtime
            .approvals
            .iter()
            .filter(|r| session.is_none_or(|id| r.session == id))
            .cloned()
            .collect();
        if pending.is_empty() {
            return;
        }
        card(ui, |ui| {
            ui.horizontal(|ui| {
                badge(ui, "APPROVALS WAITING", colors().warning);
                ui.label(small(format!("{} pending", pending.len())));
            });
            for request in &pending {
                let title = runtime
                    .sessions
                    .iter()
                    .find(|s| s.meta.id.as_str() == request.session)
                    .map_or_else(|| request.session.clone(), |s| s.meta.title.clone());
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(&title).strong());
                    ui.label(small(format!("· {} · {}", request.tool, request.reason)));
                    let left = request.expires_ms.saturating_sub(covalos_agents::now_ms()) / 1000;
                    ui.label(small(format!("· {left}s")));
                });
                ui.label(
                    RichText::new(crate::agent_gate::redact(&crate::agent_gate::describe(
                        &request.action,
                    )))
                    .monospace()
                    .size(SMALL),
                );
                ui.horizontal(|ui| {
                    if ui.add(primary("Allow once")).clicked() {
                        runtime.answer(request, true, false);
                    }
                    if !request.once_only && ui.button(tr("Allow for session")).clicked() {
                        runtime.answer(request, true, true);
                    }
                    if ui.button(tr("Deny")).clicked() {
                        runtime.answer(request, false, false);
                    }
                });
                ui.add_space(4.0);
            }
        });
    }

    fn context_panel(&mut self, ui: &mut Ui) {
        let state = &mut self.hub.agents;
        let runtime = state.runtime.as_mut().expect("runtime");
        card(ui, |ui| {
            ui.label(label("Shared context"));
            ui.label(support(
                "Results and notes every session can read. Transcripts are never shared; \
                 secret-looking text is refused.",
            ));
            let entries: Vec<_> = runtime.context.since(0).cloned().collect();
            if entries.is_empty() {
                ui.label(small("Nothing shared yet."));
            }
            for entry in entries.iter().rev().take(12) {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let kind = serde_json::to_value(entry.kind)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_uppercase))
                        .unwrap_or_default();
                    badge(ui, &kind, colors().syntax_string);
                    ui.label(small(format!(
                        "#{}{}",
                        entry.seq,
                        if entry.pinned { " · pinned" } else { "" }
                    )));
                });
                ui.add(
                    egui::Label::new(
                        RichText::new(entry.text.chars().take(260).collect::<String>())
                            .size(BODY)
                            .line_height(Some(18.0))
                            .color(ink::secondary()),
                    )
                    .wrap(),
                );
            }
            ui.add_space(4.0);
            ui.add(
                TextEdit::multiline(&mut state.note)
                    .hint_text(tr("Add a decision or contract for all agents"))
                    .desired_rows(2)
                    .desired_width(f32::INFINITY),
            );
            if ui.button(tr("Publish decision")).clicked() && !state.note.trim().is_empty() {
                let text = std::mem::take(&mut state.note);
                if let Err(e) = runtime.publish_note(EntryKind::Decision, &text, true) {
                    runtime.notice = Some((false, e));
                }
            }
        });
    }

    /// The run Luma launched: state, real failure reasons, approvals and
    /// results, straight from the engine.
    pub(super) fn team_run(&mut self, ui: &mut Ui) {
        let state = &mut self.hub.agents;
        let runtime = state.runtime.as_mut().expect("runtime");
        let mut open_session = None;
        if let Some(planning) = &runtime.planning {
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    busy_indicator(ui);
                    ui.label(read(tr("An agent is writing the plan…")));
                });
                ui.label(small(planning.goal.clone()));
                if ui.small_button(tr("Watch its conversation")).clicked() {
                    open_session = Some(planning.session.as_str().to_owned());
                }
            });
        }
        if !runtime.recoverable.is_empty() {
            let running = runtime
                .plan
                .as_ref()
                .is_some_and(|p| !p.engine.plan().state.is_settled());
            card(ui, |ui| {
                ui.label(label("Interrupted plans"));
                let mut recover: Option<PathBuf> = None;
                for journal in &runtime.recoverable {
                    ui.horizontal(|ui| {
                        ui.label(small(
                            journal
                                .file_stem()
                                .map(|s| s.to_string_lossy().into_owned())
                                .unwrap_or_default(),
                        ));
                        if !running && ui.button(tr("Recover")).clicked() {
                            recover = Some(journal.clone());
                        }
                    });
                }
                if let Some(journal) = recover
                    && let Err(e) = runtime.recover_plan(&journal)
                {
                    runtime.notice = Some((false, e));
                }
            });
        }
        let Some(run) = runtime.plan.as_mut() else {
            if runtime.planning.is_none() {
                card(ui, |ui| {
                    ui.label(label("Team run"));
                    ui.label(support(tr(
                        "No team run yet. Click Luma and give it an order; it picks the \
                         agents, model and effort.",
                    )));
                });
            }
            if let Some(id) = open_session {
                state.chat.page = Some(id);
                self.hub.focus_agents("agents");
            }
            return;
        };
        let plan = run.engine.plan().clone();
        card(ui, |ui| {
            badge(ui, plan_label(&plan.state), plan_color(&plan.state));
            ui.add(egui::Label::new(heading(&plan.goal)).wrap());
            if let covalos_engine::PlanState::Failed { reason } = &plan.state {
                failure_text(ui, reason);
            }
            ui.label(small(format!(
                "spent {:.1} min · {} tokens{}",
                plan.spend.wall_ms as f64 / 60_000.0,
                plan.spend.tokens,
                if plan.spend.cost_reported {
                    format!(" · US$ {:.4}", plan.spend.cost_micros as f64 / 1e6)
                } else {
                    String::new()
                }
            )));
            ui.horizontal(|ui| {
                let toggle = if plan.state == covalos_engine::PlanState::Paused {
                    ui.button(tr("Resume"))
                        .clicked()
                        .then(|| run.engine.resume())
                } else if !plan.state.is_settled() {
                    ui.button(tr("Pause")).clicked().then(|| run.engine.pause())
                } else {
                    None
                };
                let cancel = (!plan.state.is_settled() && ui.button(tr("Cancel plan")).clicked())
                    .then(|| run.engine.cancel());
                if let Some(Err(e)) = toggle.or(cancel) {
                    run.error = Some(e.to_string());
                }
            });
            if let Some(error) = &run.error {
                ui.colored_label(colors().error, error);
            }
        });
        let pending: Vec<(String, String)> = run
            .engine
            .pending_approvals()
            .into_iter()
            .map(|(request, task)| {
                (
                    request.ticket.clone(),
                    format!("{task}: {}", crate::agent_gate::describe(&request.action)),
                )
            })
            .collect();
        if !pending.is_empty() {
            card(ui, |ui| {
                badge(ui, "APPROVALS WAITING", colors().warning);
                let mut resolve = None;
                for (ticket, text) in &pending {
                    ui.label(
                        RichText::new(crate::agent_gate::redact(text))
                            .monospace()
                            .size(SMALL),
                    );
                    ui.horizontal(|ui| {
                        if ui.add(primary("Allow once")).clicked() {
                            resolve = Some((ticket.clone(), Resolution::AllowOnce));
                        }
                        if ui.button(tr("Deny")).clicked() {
                            resolve = Some((ticket.clone(), Resolution::Deny));
                        }
                    });
                }
                if let Some((ticket, resolution)) = resolve
                    && let Err(e) = run.engine.resolve(&ticket, resolution)
                {
                    run.error = Some(e.to_string());
                }
            });
        }
        card(ui, |ui| {
            ui.label(label("Tasks"));
            for task in plan.tasks() {
                let status = task_label(&task.status);
                Frame::new()
                    .fill(ink::raised())
                    .corner_radius(8)
                    .inner_margin(Margin::symmetric(10, 7))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let (dot, _) = ui
                                .allocate_exact_size(egui::vec2(10.0, 16.0), egui::Sense::hover());
                            ui.painter()
                                .circle_filled(dot.center(), 4.0, task_color(&task.status));
                            ui.label(
                                RichText::new(&task.spec.title)
                                    .strong()
                                    .color(ink::primary()),
                            );
                        });
                        ui.add(
                            egui::Label::new(small(format!(
                                "{} · {} · attempt {} · {} tokens",
                                task.spec.role,
                                task.account
                                    .as_ref()
                                    .map_or_else(|| "unassigned".into(), ToString::to_string),
                                task.attempts,
                                task.spend.tokens,
                            )))
                            .wrap(),
                        );
                        if matches!(task.status, TaskStatus::Failed { .. }) {
                            failure_text(ui, &status);
                        } else {
                            ui.add(egui::Label::new(small(status)).wrap());
                        }
                    });
                ui.add_space(4.0);
            }
        });
        let context: Vec<_> = run.engine.context().since(0).cloned().collect();
        card(ui, |ui| {
            ui.label(label("Results"));
            if context.is_empty() {
                ui.label(small(tr(
                    "Agents publish their results here as tasks finish.",
                )));
            }
            for entry in context.iter().rev().take(12) {
                egui::CollapsingHeader::new(small(format!(
                    "#{} {:?} · {}",
                    entry.seq,
                    entry.kind,
                    entry.text.chars().take(70).collect::<String>()
                )))
                .id_salt(("plan-result", entry.seq))
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(support(entry.text.clone()))
                            .wrap()
                            .selectable(true),
                    );
                });
            }
            ui.label(small(format!(
                "Journal: {}",
                run.engine.journal_path().display()
            )));
        });
        if let Some(id) = open_session {
            state.chat.page = Some(id);
            self.hub.focus_agents("agents");
        }
    }
}

pub(super) fn task_color(status: &TaskStatus) -> egui::Color32 {
    match status {
        TaskStatus::Succeeded => colors().success,
        TaskStatus::Failed { .. } => colors().error,
        TaskStatus::Running => colors().accent,
        TaskStatus::AwaitingApproval | TaskStatus::Retrying { .. } => colors().warning,
        _ => colors().muted,
    }
}

fn task_label(status: &TaskStatus) -> String {
    match status {
        TaskStatus::Succeeded => "Succeeded".into(),
        TaskStatus::Failed { reason } => format!("Failed: {reason}"),
        TaskStatus::Running => "Running".into(),
        TaskStatus::AwaitingApproval => "Awaiting approval".into(),
        TaskStatus::Retrying { reason, .. } => format!("Retrying: {reason}"),
        TaskStatus::Skipped { because } => format!("Skipped (needs {because})"),
        TaskStatus::Cancelled => "Cancelled".into(),
        TaskStatus::Pending => "Pending".into(),
    }
}

pub(super) fn plan_label(state: &covalos_engine::PlanState) -> &'static str {
    use covalos_engine::PlanState;
    match state {
        PlanState::Running => "RUNNING",
        PlanState::Paused => "PAUSED",
        PlanState::Cancelled => "CANCELLED",
        PlanState::Succeeded => "SUCCEEDED",
        PlanState::Failed { .. } => "FAILED",
    }
}

fn plan_color(state: &covalos_engine::PlanState) -> egui::Color32 {
    use covalos_engine::PlanState;
    match state {
        PlanState::Paused => colors().warning,
        s if s.is_settled() => colors().secondary,
        _ => colors().accent,
    }
}

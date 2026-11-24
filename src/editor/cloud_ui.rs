//! COVALOS account inside the editor: sign-in state, Cloud Config Sync and
//! the phone bridge. Every remote command goes through the same paths as
//! local input, so Luma's effort ceiling and the agent gate still decide.
use covalos_agents::{
    credentials::{CredentialStore, Secret},
    permission::Action,
};
use eframe::egui::{self, RichText, TextEdit, Ui};
use serde_json::{Value, json};
use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

use super::{
    Editor,
    cloud_sync::{CloudPreferences, SyncedConfig},
    i18n::{tr, trf},
    preferences::Preferences,
    shell::{NotificationSource, setting_row},
    theme::*,
};
use crate::{
    agent_gate,
    cloud::{self, Endpoint, Session, SignIn},
    remote_bridge::{Bridge, Command, Event, Incoming},
};

const SYNC_DEBOUNCE: Duration = Duration::from_secs(4);
const SYNC_CHECK: Duration = Duration::from_secs(2);
const PUBLISH_MIN: Duration = Duration::from_millis(1500);
const PUBLISH_MAX: Duration = Duration::from_secs(30);
const RETRY: Duration = Duration::from_secs(60);

enum Job {
    Session(Result<Session, String>),
    Uploaded(Result<SyncedConfig, String>),
    Downloaded {
        manual: bool,
        result: Result<Option<SyncedConfig>, String>,
    },
}

pub(super) struct Cloud {
    store: Box<dyn CredentialStore + Send>,
    session: Option<Session>,
    endpoint: Option<Endpoint>,
    sign_in: Option<SignIn>,
    jobs_tx: Sender<Job>,
    jobs: Receiver<Job>,
    /// Token refresh in flight (startup restore or renewal).
    renewing: bool,
    syncing: bool,
    started: bool,
    retry_at: Option<Instant>,
    status: Option<(bool, String)>,
    /// Last copy known to match the cloud.
    synced: Option<SyncedConfig>,
    sync_due: Option<Instant>,
    sync_checked: Option<Instant>,
    bridge: Option<Bridge>,
    bridge_status: (bool, String),
    published: Option<(Instant, Value)>,
}

impl Default for Cloud {
    fn default() -> Self {
        let (jobs_tx, jobs) = mpsc::channel();
        Self {
            store: cloud::store(),
            session: None,
            endpoint: None,
            sign_in: None,
            jobs_tx,
            jobs,
            renewing: false,
            syncing: false,
            started: false,
            retry_at: None,
            status: None,
            synced: None,
            sync_due: None,
            sync_checked: None,
            bridge: None,
            bridge_status: (false, String::new()),
            published: None,
        }
    }
}

impl Cloud {
    fn spawn(&self, ctx: &egui::Context, work: impl FnOnce() -> Job + Send + 'static) {
        let tx = self.jobs_tx.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let _ = tx.send(work());
            ctx.request_repaint();
        });
    }

    fn renew(&mut self, ctx: &egui::Context, endpoint: Endpoint, refresh: String) {
        self.renewing = true;
        self.spawn(ctx, move || {
            Job::Session(cloud::refresh(&endpoint, &refresh))
        });
    }

    fn signed_in(&self) -> Option<(&Endpoint, &Session)> {
        Some((self.endpoint.as_ref()?, self.session.as_ref()?))
    }

    /// Stores the rotated refresh token and hands the new access token to
    /// the bridge.
    fn keep(&mut self, session: Session) -> Result<(), String> {
        let endpoint = self.endpoint.as_ref().ok_or("No server is set.")?;
        let saved = self
            .store
            .save(
                &endpoint.credential_key(),
                Secret::new(session.refresh.expose()),
            )
            .map_err(|e| format!("Could not save the session in the system store. {e}"));
        if let Some(bridge) = &self.bridge {
            bridge.update_token(session.access.expose());
        }
        self.session = Some(session);
        saved
    }

    fn forget(&mut self) {
        if let Some(endpoint) = &self.endpoint {
            let _ = self.store.delete(&endpoint.credential_key());
        }
        self.session = None;
        self.bridge = None;
        self.synced = None;
        self.sync_due = None;
        self.published = None;
    }
}

impl Editor {
    /// Drives the account every frame: sign-in, token renewal, auto-sync
    /// and the phone bridge.
    pub(super) fn cloud_tick(&mut self, ctx: &egui::Context) {
        self.cloud_start(ctx);
        self.cloud_sign_in_result(ctx);
        self.cloud_jobs(ctx);
        self.cloud_renew(ctx);
        self.cloud_auto_sync(ctx);
        self.cloud_bridge(ctx);
    }

    fn cloud_start(&mut self, ctx: &egui::Context) {
        let cloud = &mut self.cloud;
        let retry = cloud.retry_at.is_some_and(|at| Instant::now() >= at);
        if (cloud.started && !retry) || cloud.session.is_some() || cloud.renewing {
            return;
        }
        cloud.started = true;
        cloud.retry_at = None;
        let Ok(endpoint) = self.preferences.cloud.endpoint() else {
            return;
        };
        let stored = cloud.store.load(&endpoint.credential_key());
        cloud.endpoint = Some(endpoint.clone());
        if let Ok(Some(refresh)) = stored {
            let refresh = refresh.expose().to_owned();
            cloud.renew(ctx, endpoint, refresh);
        }
    }

    fn cloud_sign_in_result(&mut self, ctx: &egui::Context) {
        let Some(result) = self
            .cloud
            .sign_in
            .as_ref()
            .and_then(|s| s.result.try_recv().ok())
        else {
            return;
        };
        self.cloud.sign_in = None;
        match result {
            Ok(session) => {
                let who = session
                    .email
                    .clone()
                    .unwrap_or_else(|| tr("your account").to_owned());
                self.cloud.status = Some(match self.cloud.keep(session) {
                    Ok(()) => (false, trf("Signed in as {who}.", &[("who", who)])),
                    Err(error) => (true, error),
                });
                self.cloud_first_sync(ctx);
            }
            Err(error) => self.cloud.status = Some((true, error)),
        }
    }

    /// After sign-in, pull the cloud copy when auto-sync is on.
    fn cloud_first_sync(&mut self, ctx: &egui::Context) {
        if self.preferences.cloud.auto_sync && self.cloud.synced.is_none() {
            self.cloud_download(ctx, false);
        }
    }

    fn cloud_jobs(&mut self, ctx: &egui::Context) {
        while let Ok(job) = self.cloud.jobs.try_recv() {
            match job {
                Job::Session(Ok(session)) => {
                    self.cloud.renewing = false;
                    if let Err(error) = self.cloud.keep(session) {
                        self.cloud.status = Some((true, error));
                    }
                    self.cloud_first_sync(ctx);
                }
                Job::Session(Err(error)) => {
                    self.cloud.renewing = false;
                    // A refused token means signed out; a network error is retried.
                    if error.contains("(4") {
                        self.cloud.forget();
                        self.cloud.status =
                            Some((true, tr("Your session ended. Sign in again.").to_owned()));
                    } else {
                        self.cloud.session = None;
                        self.cloud.bridge = None;
                        self.cloud.retry_at = Some(Instant::now() + RETRY);
                        self.cloud.status = Some((true, error));
                    }
                }
                Job::Uploaded(result) => {
                    self.cloud.syncing = false;
                    match result {
                        Ok(config) => {
                            self.preferences.cloud.last_synced_ms = config.saved_ms;
                            self.cloud.synced = Some(config);
                            self.cloud.status = Some((false, tr("Settings uploaded.").to_owned()));
                        }
                        Err(error) => self.cloud.status = Some((true, error)),
                    }
                }
                Job::Downloaded { manual, result } => {
                    self.cloud.syncing = false;
                    match result {
                        Ok(Some(remote)) => self.cloud_received(ctx, remote, manual),
                        Ok(None) if manual => {
                            self.cloud.status = Some((
                                false,
                                tr("Nothing saved in the cloud yet. Upload first.").to_owned(),
                            ));
                        }
                        Ok(None) => self.cloud_upload(ctx),
                        Err(error) => self.cloud.status = Some((true, error)),
                    }
                }
            }
        }
    }

    /// Applies the cloud copy when the user asked for it or when it is newer
    /// than what this computer last synced; otherwise uploads local changes.
    fn cloud_received(&mut self, ctx: &egui::Context, remote: SyncedConfig, manual: bool) {
        let local_rules = Preferences::load_agent_rules();
        let local = SyncedConfig::capture(&self.preferences, &local_rules);
        if remote.same_settings(&local) {
            self.preferences.cloud.last_synced_ms = remote.saved_ms;
            self.cloud.synced = Some(remote);
            if manual {
                self.cloud.status = Some((false, tr("Already in sync.").to_owned()));
            }
            return;
        }
        if !manual && remote.saved_ms <= self.preferences.cloud.last_synced_ms {
            self.cloud.synced = Some(remote);
            self.cloud_upload(ctx);
            return;
        }
        let saved_ms = remote.saved_ms;
        self.cloud.synced = Some(remote.clone());
        let rules = remote.apply(&mut self.preferences);
        // Empty cloud rules may mean they were held back as secret; keep local ones.
        if !rules.trim().is_empty()
            && rules != local_rules
            && let Err(error) = Preferences::save_agent_rules(&rules)
        {
            self.error(format!("Could not save agent rules. {error}"));
        }
        ctx.data_mut(|d| d.remove::<bool>(egui::Id::new("settings-agent-rules-loaded")));
        self.preferences.cloud.last_synced_ms = saved_ms;
        set_brightness(self.preferences.brightness);
        install_fonts(
            ctx,
            self.preferences.ubuntu_interface,
            self.preferences.hack_editor,
        );
        install(
            ctx,
            self.preferences.font_size,
            self.preferences.high_contrast,
            self.preferences.theme,
        );
        self.syntax = egui_extras::syntax_highlighting::CodeTheme::dark(self.preferences.font_size);
        self.cloud.status = Some((
            false,
            tr("Settings downloaded from the cloud.").to_owned(),
        ));
    }

    fn cloud_upload(&mut self, ctx: &egui::Context) {
        if self.cloud.syncing {
            self.cloud.sync_due = Some(Instant::now() + SYNC_DEBOUNCE);
            return;
        }
        let Some((endpoint, session)) = self.cloud.signed_in() else {
            return;
        };
        let (endpoint, access, user) = (
            endpoint.clone(),
            session.access.expose().to_owned(),
            session.user_id.clone(),
        );
        let rules = Preferences::load_agent_rules();
        let mut config = SyncedConfig::capture(&self.preferences, &rules);
        if config.dropped_rules(&rules) {
            self.notify(
                NotificationSource::Covalos,
                tr("Agent rules look like they contain a secret, so they were not uploaded."),
                true,
            );
        }
        config.saved_ms = cloud::now_ms();
        self.cloud.syncing = true;
        self.cloud.sync_due = None;
        self.cloud.spawn(ctx, move || {
            Job::Uploaded(
                super::cloud_sync::upload(&endpoint, &access, &user, &config).map(|()| config),
            )
        });
    }

    fn cloud_download(&mut self, ctx: &egui::Context, manual: bool) {
        if self.cloud.syncing {
            return;
        }
        let Some((endpoint, session)) = self.cloud.signed_in() else {
            return;
        };
        let (endpoint, access, user) = (
            endpoint.clone(),
            session.access.expose().to_owned(),
            session.user_id.clone(),
        );
        self.cloud.syncing = true;
        self.cloud.spawn(ctx, move || Job::Downloaded {
            manual,
            result: super::cloud_sync::download(&endpoint, &access, &user),
        });
    }

    fn cloud_renew(&mut self, ctx: &egui::Context) {
        let cloud = &mut self.cloud;
        if cloud.renewing {
            return;
        }
        let Some((endpoint, session)) = cloud.signed_in() else {
            return;
        };
        if session.needs_refresh(cloud::now_ms()) {
            let (endpoint, refresh) = (endpoint.clone(), session.refresh.expose().to_owned());
            cloud.renew(ctx, endpoint, refresh);
        }
    }

    fn cloud_auto_sync(&mut self, ctx: &egui::Context) {
        if !self.preferences.cloud.auto_sync || self.cloud.session.is_none() {
            return;
        }
        let now = Instant::now();
        if let Some(due) = self.cloud.sync_due {
            if now >= due {
                self.cloud_upload(ctx);
            } else {
                ctx.request_repaint_after(due - now);
            }
            return;
        }
        if self.cloud.syncing
            || self.cloud.synced.is_none()
            || self
                .cloud
                .sync_checked
                .is_some_and(|at| now.duration_since(at) < SYNC_CHECK)
        {
            return;
        }
        self.cloud.sync_checked = Some(now);
        let local = SyncedConfig::capture(&self.preferences, &Preferences::load_agent_rules());
        if self
            .cloud
            .synced
            .as_ref()
            .is_some_and(|synced| !synced.same_settings(&local))
        {
            self.cloud.sync_due = Some(now + SYNC_DEBOUNCE);
            ctx.request_repaint_after(SYNC_DEBOUNCE);
        }
    }

    fn cloud_bridge(&mut self, ctx: &egui::Context) {
        let wanted = self.preferences.cloud.remote_bridge && self.cloud.session.is_some();
        if !wanted {
            if self.cloud.bridge.take().is_some() {
                self.cloud.bridge_status = (false, String::new());
                self.cloud.published = None;
            }
            return;
        }
        if self.cloud.bridge.is_none()
            && let Some((endpoint, session)) = self.cloud.signed_in()
        {
            let wake = ctx.clone();
            let bridge = Bridge::start(
                endpoint.clone(),
                &session.user_id,
                session.access.expose().to_owned(),
                move || wake.request_repaint(),
            );
            self.cloud.bridge = Some(bridge);
            self.cloud.bridge_status = (false, tr("Connecting…").to_owned());
        }
        let events = self
            .cloud
            .bridge
            .as_ref()
            .map(Bridge::events)
            .unwrap_or_default();
        for event in events {
            match event {
                Event::Connected => {
                    self.cloud.bridge_status = (false, tr("Connected").to_owned());
                    self.cloud.published = None;
                }
                Event::Disconnected(reason) => self.cloud.bridge_status = (true, reason),
                Event::Command(incoming) => self.remote_command(ctx, incoming),
            }
        }
        self.remote_publish();
        ctx.request_repaint_after(PUBLISH_MIN);
    }

    fn remote_publish(&mut self) {
        if self.cloud.bridge.is_none() {
            return;
        }
        let state = self.remote_state();
        let due = match &self.cloud.published {
            Some((at, last)) => {
                (at.elapsed() >= PUBLISH_MIN && *last != state) || at.elapsed() >= PUBLISH_MAX
            }
            None => true,
        };
        if due && let Some(bridge) = &self.cloud.bridge {
            let mut stamped = state.clone();
            stamped["at_ms"] = json!(cloud::now_ms());
            bridge.publish(stamped);
            self.cloud.published = Some((Instant::now(), state));
        }
    }

    /// Status for the phone. No paths: project and files by name only,
    /// commands and text passed through the same redaction as the audit log.
    fn remote_state(&self) -> Value {
        let allow_approvals = self.preferences.cloud.remote_approvals;
        let mut sessions = Vec::new();
        let mut approvals = Vec::new();
        let mut planning = false;
        if let Some(runtime) = &self.hub.agents.runtime {
            planning = runtime.planning.is_some();
            for session in runtime.sessions.iter().take(40) {
                let meta = &session.meta;
                sessions.push(json!({
                    "id": meta.id.as_str(),
                    "title": agent_gate::redact(&meta.title).chars().take(120).collect::<String>(),
                    "status": meta.status.label(),
                    "running": session.running(),
                    "provider": meta.provider.key(),
                    "model": meta.model,
                    "turns": meta.turns,
                    "pinned": self.preferences.agent_pinned.contains(meta.id.as_str()),
                }));
            }
            for request in runtime.approvals.iter().take(20) {
                approvals.push(json!({
                    "ticket": request.ticket,
                    "session": request.session,
                    "tool": request.tool,
                    "action": remote_action(&request.action),
                    "reason": agent_gate::redact(&request.reason),
                    "sensitive": request.once_only,
                    "can_allow": allow_approvals && !request.once_only,
                    "expires_ms": request.expires_ms,
                }));
            }
        }
        json!({
            "v": 1,
            "app": crate::brand::RELEASE,
            "project": self.root.as_deref().map(crate::workspace::filename),
            "sessions": sessions,
            "approvals": approvals,
            "luma": self.hub.agents.luma.remote_summary(planning),
        })
    }

    fn remote_command(&mut self, ctx: &egui::Context, incoming: Incoming) {
        let Incoming { id, command } = incoming;
        let (ok, message) = match command {
            Command::Refresh => (true, tr("Status sent.").to_owned()),
            Command::LumaOrder { text } => self.remote_luma(&text),
            Command::ChatPrompt { session, text } => self.remote_chat(ctx, &session, &text),
            Command::Approval { ticket, allow } => self.remote_approval(&ticket, allow),
        };
        self.notify(
            NotificationSource::Covalos,
            trf("Phone app: {message}", &[("message", message.clone())]),
            !ok,
        );
        if let Some(bridge) = &self.cloud.bridge {
            bridge.reply(&id, ok, message);
        }
        self.cloud.published = None;
    }

    fn remote_luma(&mut self, text: &str) -> (bool, String) {
        if self.root.is_none() {
            return (
                false,
                tr("Open a project folder on the computer first.").to_owned(),
            );
        }
        // Same path as Luma's own box: the effort ceiling still holds.
        self.luma_order(text);
        if self.hub.agents.luma.is_holding() {
            (
                true,
                tr("Luma needs a higher effort ceiling. Confirm it on the computer.").to_owned(),
            )
        } else {
            (true, tr("Luma took the order.").to_owned())
        }
    }

    fn remote_chat(&mut self, ctx: &egui::Context, session: &str, text: &str) -> (bool, String) {
        if !self.ensure_runtime(ctx) {
            return (false, tr("The agent runtime is unavailable.").to_owned());
        }
        let Some(runtime) = self.hub.agents.runtime.as_mut() else {
            return (false, tr("The agent runtime is unavailable.").to_owned());
        };
        let Some(index) = runtime
            .sessions
            .iter()
            .position(|s| s.meta.id.as_str() == session)
        else {
            return (false, tr("Unknown conversation.").to_owned());
        };
        // The provider's tool calls still pass through the agent gate.
        match runtime.send(index, text) {
            Ok(()) => (true, tr("Message sent.").to_owned()),
            Err(error) => (false, error),
        }
    }

    /// Answers a pending gate request from the phone. Denying always works;
    /// allowing needs the opt-in, never covers sensitive one-time actions and
    /// never remembers the grant.
    fn remote_approval(&mut self, ticket: &str, allow: bool) -> (bool, String) {
        let allow_remote = self.preferences.cloud.remote_approvals;
        let Some(runtime) = self.hub.agents.runtime.as_mut() else {
            return (false, tr("This request is no longer pending.").to_owned());
        };
        let Some(request) = agent_gate::pending(&runtime.gate_dir)
            .into_iter()
            .find(|r| r.ticket == ticket)
        else {
            return (false, tr("This request is no longer pending.").to_owned());
        };
        if let Err(reason) = remote_may_answer(allow, allow_remote, request.once_only) {
            return (false, tr(reason).to_owned());
        }
        runtime.answer(&request, allow, false);
        if allow {
            (true, tr("Allowed once.").to_owned())
        } else {
            (true, tr("Denied.").to_owned())
        }
    }

    fn cloud_begin_sign_in(&mut self, ctx: &egui::Context) {
        let endpoint = match self.preferences.cloud.endpoint() {
            Ok(endpoint) => endpoint,
            Err(error) => {
                self.cloud.status = Some((true, error));
                return;
            }
        };
        let wake = ctx.clone();
        match cloud::begin_sign_in(&endpoint, &self.preferences.cloud.provider, move || {
            wake.request_repaint()
        }) {
            Ok(sign_in) => {
                ctx.open_url(egui::OpenUrl::new_tab(&sign_in.url));
                self.cloud.endpoint = Some(endpoint);
                self.cloud.sign_in = Some(sign_in);
                self.cloud.status = Some((
                    false,
                    tr("Finish signing in in your browser.").to_owned(),
                ));
            }
            Err(error) => self.cloud.status = Some((true, error)),
        }
    }

    fn cloud_sign_out(&mut self) {
        if let Some((endpoint, session)) = self.cloud.signed_in() {
            let (endpoint, access) = (endpoint.clone(), session.access.expose().to_owned());
            thread::spawn(move || cloud::sign_out(&endpoint, &access));
        }
        self.cloud.forget();
        self.cloud.status = Some((false, tr("Signed out.").to_owned()));
    }

    /// Settings → COVALOS account.
    pub(super) fn cloud_settings(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let endpoint = self.preferences.cloud.endpoint();
        let signed_in = self.cloud.session.is_some();
        ui.add(
            egui::Label::new(muted(
                "Sign in to keep your settings in sync and to reach this computer from the COVALOS phone app.",
            ))
            .wrap(),
        );
        ui.add_space(10.0);
        self.cloud_account_row(ui, &ctx, endpoint.is_ok());
        if let Some((error, message)) = &self.cloud.status {
            let color = if *error {
                colors().error
            } else {
                colors().muted
            };
            ui.add(egui::Label::new(RichText::new(message).size(SMALL).color(color)).wrap());
            ui.add_space(8.0);
        }
        if !cloud::store_is_persistent() {
            ui.add(
                egui::Label::new(muted(
                    "This system has no supported credential store, so you stay signed in only until COVALOS closes.",
                ))
                .wrap(),
            );
            ui.add_space(8.0);
        }
        self.cloud_sync_settings(ui, &ctx, signed_in);
        self.cloud_remote_settings(ui, signed_in);
        ui.collapsing(tr("Server"), |ui| {
            match &endpoint {
                Ok(endpoint) => ui.label(muted(trf(
                    "Using {host} from {source}.",
                    &[
                        ("host", endpoint.host().to_owned()),
                        ("source", tr(endpoint.source).to_owned()),
                    ],
                ))),
                Err(error) => ui.label(muted(error.clone())),
            };
            ui.add_space(4.0);
            let locked = signed_in || self.cloud.sign_in.is_some();
            let cloud = &mut self.preferences.cloud;
            ui.add_enabled_ui(!locked, |ui| {
                ui.add(
                    TextEdit::singleline(&mut cloud.server_url)
                        .hint_text(tr("Server address, empty for COVALOS_SUPABASE_URL"))
                        .desired_width(f32::INFINITY)
                        .char_limit(200),
                );
                ui.add(
                    TextEdit::singleline(&mut cloud.anon_key)
                        .hint_text(tr("Public anon key"))
                        .desired_width(f32::INFINITY)
                        .char_limit(400),
                );
            });
            if locked {
                ui.label(muted("Sign out to change the server."));
            }
        });
    }

    fn cloud_account_row(&mut self, ui: &mut Ui, ctx: &egui::Context, configured: bool) {
        if let Some(session) = &self.cloud.session {
            let who = session
                .email
                .clone()
                .unwrap_or_else(|| tr("your account").to_owned());
            let mut sign_out = false;
            setting_row(
                ui,
                "Account",
                &trf("Signed in as {who}.", &[("who", who)]),
                |ui| sign_out = ui.button(tr("Sign out")).clicked(),
            );
            if sign_out {
                self.cloud_sign_out();
            }
            return;
        }
        let waiting = self.cloud.sign_in.is_some();
        let mut start = false;
        let mut cancel = false;
        let provider = &mut self.preferences.cloud.provider;
        setting_row(
            ui,
            "Account",
            "Opens your browser. COVALOS never sees your password.",
            |ui| {
                if waiting {
                    cancel = ui.button(tr("Cancel")).clicked();
                    return;
                }
                start = ui
                    .add_enabled(configured, egui::Button::new(tr("Sign in")))
                    .clicked();
                let name = CloudPreferences::PROVIDERS
                    .iter()
                    .find(|(key, _)| key == provider)
                    .map_or("GitHub", |(_, name)| name);
                egui::ComboBox::from_id_salt("cloud-provider")
                    .selected_text(name)
                    .width(84.0)
                    .show_ui(ui, |ui| {
                        for (key, name) in CloudPreferences::PROVIDERS {
                            ui.selectable_value(provider, key.to_owned(), name);
                        }
                    });
            },
        );
        if start {
            self.cloud_begin_sign_in(ctx);
        }
        if cancel {
            self.cloud.sign_in = None;
            self.cloud.status = Some((false, tr("Sign-in cancelled.").to_owned()));
        }
        if let Some(sign_in) = &self.cloud.sign_in
            && ui.link(tr("Open the sign-in page again")).clicked()
        {
            ctx.open_url(egui::OpenUrl::new_tab(&sign_in.url));
        }
    }

    fn cloud_sync_settings(&mut self, ui: &mut Ui, ctx: &egui::Context, signed_in: bool) {
        ui.label(RichText::new(tr("Cloud Config Sync")).size(BODY).strong());
        ui.add_space(4.0);
        ui.add(
            egui::Label::new(muted(
                "Syncs theme, fonts, density, pinned and favorite chats, agent rules and skills. Folders, files, notes, provider accounts and API keys stay on this computer.",
            ))
            .wrap(),
        );
        ui.add_space(6.0);
        let busy = self.cloud.syncing;
        let mut upload = false;
        let mut download = false;
        ui.horizontal(|ui| {
            upload = ui
                .add_enabled(signed_in && !busy, egui::Button::new(tr("Upload settings")))
                .clicked();
            download = ui
                .add_enabled(
                    signed_in && !busy,
                    egui::Button::new(tr("Download settings")),
                )
                .clicked();
            if busy {
                ui.spinner();
            }
        });
        if upload {
            self.cloud_upload(ctx);
        }
        if download {
            self.cloud_download(ctx, true);
        }
        let was_auto = self.preferences.cloud.auto_sync;
        checkbox(
            ui,
            &mut self.preferences.cloud.auto_sync,
            tr("Sync automatically"),
        );
        if !was_auto && self.preferences.cloud.auto_sync {
            self.cloud_download(ctx, false);
        }
        ui.add_space(12.0);
    }

    fn cloud_remote_settings(&mut self, ui: &mut Ui, signed_in: bool) {
        ui.label(RichText::new(tr("Phone app")).size(BODY).strong());
        ui.add_space(4.0);
        ui.add(
            egui::Label::new(muted(
                "While COVALOS is open, the phone app can see your agents and Luma, send them messages and deny pending requests. Commands follow the same approval rules as this computer.",
            ))
            .wrap(),
        );
        ui.add_space(6.0);
        let cloud = &mut self.preferences.cloud;
        checkbox(
            ui,
            &mut cloud.remote_bridge,
            tr("Allow the phone app to reach this computer"),
        );
        ui.add_enabled_ui(cloud.remote_bridge, |ui| {
            checkbox(
                ui,
                &mut cloud.remote_approvals,
                tr("Let the phone allow pending requests"),
            );
        });
        ui.add(
            egui::Label::new(muted(
                "Sensitive actions are only approved on this computer, and nothing allowed from the phone is remembered.",
            ))
            .wrap(),
        );
        if cloud.remote_bridge {
            let (error, status) = &self.cloud.bridge_status;
            let status = if !signed_in {
                tr("Sign in to connect.").to_owned()
            } else if status.is_empty() {
                tr("Connecting…").to_owned()
            } else {
                status.clone()
            };
            ui.label(
                RichText::new(format!("{}: {status}", tr("Status")))
                    .size(SMALL)
                    .color(if *error {
                        colors().error
                    } else {
                        colors().muted
                    }),
            );
        }
        ui.add_space(12.0);
    }
}

/// The local approval rules applied to a phone answer. Returns why not.
fn remote_may_answer(allow: bool, allow_remote: bool, sensitive: bool) -> Result<(), &'static str> {
    if !allow {
        return Ok(());
    }
    if !allow_remote {
        return Err(
            "Allowing from the phone is off. Turn it on in Settings or answer on the computer.",
        );
    }
    if sensitive {
        return Err("This is a sensitive action. Approve it on the computer.");
    }
    Ok(())
}

/// What a gate request touches, without full paths.
fn remote_action(action: &Action) -> String {
    let name = |path: &std::path::Path| {
        path.file_name().map_or_else(
            || tr("project folder").to_owned(),
            |n| n.to_string_lossy().into_owned(),
        )
    };
    let text = match action {
        Action::Read(path) => format!("read {}", name(path)),
        Action::Write(path) => format!("write {}", name(path)),
        other => agent_gate::describe(other),
    };
    agent_gate::redact(&text).chars().take(300).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn approvals_for_the_phone_show_file_names_not_paths() {
        let write = Action::Write(PathBuf::from("C:/Users/me/work/src/main.rs"));
        assert_eq!(remote_action(&write), "write main.rs");
        let run = Action::Execute(
            "curl -H 'Authorization: Bearer sk-ant-abcdefghijklmnopqrstuvwxyz0123'".into(),
        );
        assert!(!remote_action(&run).contains("sk-ant"));
    }

    #[test]
    fn the_phone_can_always_deny_but_only_allow_what_local_rules_permit() {
        assert!(remote_may_answer(false, false, true).is_ok());
        assert!(remote_may_answer(true, false, false).is_err());
        assert!(remote_may_answer(true, true, true).is_err());
        assert!(remote_may_answer(true, true, false).is_ok());
    }
}

//! Cloud Config Sync: the allowlisted part of the preferences that follows
//! the COVALOS account between computers.
//!
//! Only look-and-feel, agent organization, `agent-rules.md` and skills are
//! synced. Workspace paths, recent folders, notes, tasks, open tabs, provider
//! accounts and API keys never leave the machine, and text that looks like a
//! secret is dropped before upload.
use covalos_agents::context::looks_secret;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeSet;

use super::preferences::Preferences;
use crate::cloud::{self, Endpoint};

const TABLE: &str = "editor_configs";
const VERSION: u32 = 1;
const MAX_PAYLOAD: usize = 512 * 1024;

/// Account settings kept in `preferences.json`. No tokens live here: the
/// refresh token is in the OS credential store.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct CloudPreferences {
    /// Empty means `COVALOS_SUPABASE_URL`.
    pub(super) server_url: String,
    /// Public anon key for `server_url`; empty means `COVALOS_SUPABASE_ANON_KEY`.
    pub(super) anon_key: String,
    /// OAuth provider used for browser sign-in.
    pub(super) provider: String,
    /// Upload changes and pull on start, only after the user opts in.
    pub(super) auto_sync: bool,
    /// Publish session status and take commands from the phone app.
    pub(super) remote_bridge: bool,
    /// Lets the phone allow pending approvals. Denying is always possible.
    pub(super) remote_approvals: bool,
    /// When this computer last pushed or applied the cloud copy.
    pub(super) last_synced_ms: u64,
}

impl Default for CloudPreferences {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            anon_key: String::new(),
            provider: "github".into(),
            auto_sync: false,
            remote_bridge: false,
            remote_approvals: false,
            last_synced_ms: 0,
        }
    }
}

impl CloudPreferences {
    pub(super) const PROVIDERS: [(&'static str, &'static str); 3] = [
        ("github", "GitHub"),
        ("google", "Google"),
        ("discord", "Discord"),
    ];

    pub(super) fn endpoint(&self) -> Result<Endpoint, String> {
        Endpoint::resolve(&self.server_url, &self.anon_key)
    }
}

/// What the cloud copy holds. Every field is safe to leave the machine.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct SyncedConfig {
    pub(super) version: u32,
    pub(super) saved_ms: u64,
    theme: super::theme::Theme,
    syntax: super::syntax::SyntaxStyle,
    language: super::i18n::Language,
    font_size: f32,
    ubuntu_interface: bool,
    hack_editor: bool,
    font_ligatures: bool,
    word_wrap: bool,
    inline_suggestions: bool,
    compact: bool,
    ui_scale: f32,
    explorer_zoom: f32,
    motion: f32,
    brightness: f32,
    reduce_motion: bool,
    high_contrast: bool,
    transitions: super::preferences::Transitions,
    agent_pinned: BTreeSet<String>,
    agent_favorites: BTreeSet<(covalos_agents::AccountId, String)>,
    agent_rules: String,
    skills: Vec<super::skills::Skill>,
}

impl Default for SyncedConfig {
    fn default() -> Self {
        Self::capture(&Preferences::default(), "")
    }
}

impl SyncedConfig {
    pub(super) fn capture(prefs: &Preferences, agent_rules: &str) -> Self {
        let skills = prefs
            .skills
            .iter()
            .filter(|skill| !serde_json::to_string(skill).is_ok_and(|text| looks_secret(&text)))
            .cloned()
            .collect();
        Self {
            version: VERSION,
            saved_ms: 0,
            theme: prefs.theme,
            syntax: prefs.syntax,
            language: prefs.language,
            font_size: prefs.font_size,
            ubuntu_interface: prefs.ubuntu_interface,
            hack_editor: prefs.hack_editor,
            font_ligatures: prefs.font_ligatures,
            word_wrap: prefs.word_wrap,
            inline_suggestions: prefs.inline_suggestions,
            compact: prefs.compact,
            ui_scale: prefs.ui_scale,
            explorer_zoom: prefs.explorer_zoom,
            motion: prefs.motion,
            brightness: prefs.brightness,
            reduce_motion: prefs.reduce_motion,
            high_contrast: prefs.high_contrast,
            transitions: prefs.transitions,
            agent_pinned: prefs.agent_pinned.clone(),
            agent_favorites: prefs.agent_favorites.clone(),
            agent_rules: if looks_secret(agent_rules) {
                String::new()
            } else {
                agent_rules.to_owned()
            },
            skills,
        }
    }

    /// Whether two copies hold the same settings, ignoring the time stamp.
    pub(super) fn same_settings(&self, other: &Self) -> bool {
        // Compared as JSON, the form that is uploaded, so NaN equals NaN.
        let plain = |config: &Self| {
            serde_json::to_value(Self {
                saved_ms: 0,
                ..config.clone()
            })
            .ok()
        };
        plain(self) == plain(other)
    }

    /// True when local rules exist but were held back as secret-looking.
    pub(super) fn dropped_rules(&self, local_rules: &str) -> bool {
        self.agent_rules.is_empty() && !local_rules.trim().is_empty()
    }

    /// Applies the cloud copy. Values are clamped the same way as on load.
    /// Returns the agent rules to write to `agent-rules.md`.
    pub(super) fn apply(self, prefs: &mut Preferences) -> String {
        let clamp = |value: f32, default: f32, (lo, hi): (f32, f32)| {
            if value.is_finite() {
                value.clamp(lo, hi)
            } else {
                default
            }
        };
        prefs.theme = self.theme;
        prefs.syntax = self.syntax;
        prefs.language = self.language;
        prefs.font_size = clamp(self.font_size, 15.0, super::preferences::EDITOR_FONT);
        prefs.ubuntu_interface = self.ubuntu_interface;
        prefs.hack_editor = self.hack_editor;
        prefs.font_ligatures = self.font_ligatures;
        prefs.word_wrap = self.word_wrap;
        prefs.inline_suggestions = self.inline_suggestions;
        prefs.compact = self.compact;
        prefs.ui_scale = clamp(self.ui_scale, 1.0, super::preferences::UI_SCALE);
        prefs.explorer_zoom = clamp(self.explorer_zoom, 1.0, super::preferences::EXPLORER_ZOOM);
        prefs.motion = clamp(self.motion, 1.0, (0.0, 1.0));
        prefs.brightness = clamp(self.brightness, 0.35, (0.0, 1.0));
        prefs.reduce_motion = self.reduce_motion;
        prefs.high_contrast = self.high_contrast;
        prefs.transitions = self.transitions;
        prefs.agent_pinned = self.agent_pinned.into_iter().take(256).collect();
        prefs.agent_favorites = self.agent_favorites.into_iter().take(256).collect();
        prefs.skills = self.skills.into_iter().take(64).collect();
        self.agent_rules
    }
}

pub(super) fn upload(
    endpoint: &Endpoint,
    access: &str,
    user_id: &str,
    config: &SyncedConfig,
) -> Result<(), String> {
    let body = json!([{
        "user_id": user_id,
        "payload": config,
    }])
    .to_string();
    if body.len() > MAX_PAYLOAD {
        return Err("Settings are too large to sync (512 KB limit).".into());
    }
    let (status, text) = cloud::request(
        endpoint,
        Some(access),
        "POST",
        &format!("/rest/v1/{TABLE}?on_conflict=user_id"),
        &[("Prefer", "resolution=merge-duplicates,return=minimal")],
        Some(&body),
    )?;
    if (200..300).contains(&status) {
        Ok(())
    } else {
        Err(cloud::server_error(
            "Could not upload settings",
            status,
            &text,
        ))
    }
}

/// The cloud copy, or `None` when this account has never synced.
pub(super) fn download(
    endpoint: &Endpoint,
    access: &str,
    user_id: &str,
) -> Result<Option<SyncedConfig>, String> {
    let (status, text) = cloud::request(
        endpoint,
        Some(access),
        "GET",
        &format!(
            "/rest/v1/{TABLE}?select=payload&user_id=eq.{}",
            cloud::encode(user_id)
        ),
        &[],
        None,
    )?;
    if status != 200 {
        return Err(cloud::server_error(
            "Could not download settings",
            status,
            &text,
        ));
    }
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(&text).map_err(|_| "The server sent unreadable settings.")?;
    let Some(row) = rows.into_iter().next() else {
        return Ok(None);
    };
    let config: SyncedConfig = serde_json::from_value(row["payload"].clone())
        .map_err(|_| "The cloud settings could not be read.")?;
    if config.version > VERSION {
        return Err("The cloud settings are from a newer COVALOS. Update to sync.".into());
    }
    Ok(Some(config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn only_allowlisted_settings_leave_the_machine() {
        let mut prefs = Preferences::default();
        prefs.last_workspace = Some(PathBuf::from(r"C:\Users\me\secret-project"));
        prefs.recent = vec![PathBuf::from(r"C:\Users\me\secret-project")];
        prefs.agent_pinned.insert("session-1".into());
        prefs.compact = true;
        let config = SyncedConfig::capture(&prefs, "Prefer small commits.");
        let text = serde_json::to_string(&config).unwrap();
        assert!(!text.contains("secret-project"));
        assert!(text.contains("session-1") && text.contains("Prefer small commits."));
        for field in [
            "last_workspace",
            "recent",
            "notes",
            "tasks",
            "agent_account",
        ] {
            assert!(!text.contains(&format!("\"{field}\"")), "{field} leaked");
        }
    }

    #[test]
    fn secret_looking_rules_are_not_uploaded() {
        let rules = "Use key sk-ant-abcdefghijklmnopqrstuvwxyz0123 for tests.";
        let config = SyncedConfig::capture(&Preferences::default(), rules);
        assert!(config.agent_rules.is_empty());
        assert!(config.dropped_rules(rules));
    }

    #[test]
    fn applying_clamps_values_and_keeps_local_only_state() {
        let mut remote = SyncedConfig::capture(&Preferences::default(), "rules");
        remote.font_size = 400.0;
        remote.ui_scale = f32::NAN;
        remote.compact = true;
        let mut local = Preferences::default();
        local.last_workspace = Some(PathBuf::from("/work"));
        let rules = remote.clone().apply(&mut local);
        assert_eq!(rules, "rules");
        assert_eq!(local.font_size, super::super::preferences::EDITOR_FONT.1);
        assert_eq!(local.ui_scale, 1.0);
        assert!(local.compact);
        assert_eq!(local.last_workspace, Some(PathBuf::from("/work")));
        let mut later = remote.clone();
        later.saved_ms = 99;
        assert!(later.same_settings(&remote));
    }

    #[test]
    fn older_payloads_fill_missing_fields_with_defaults() {
        let config: SyncedConfig = serde_json::from_str(r#"{"version":1,"compact":true}"#).unwrap();
        assert!(config.compact);
        assert_eq!(config.font_size, Preferences::default().font_size);
    }
}

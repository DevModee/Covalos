use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Preferences {
    pub(super) theme: super::theme::Theme,
    /// Code colors, chosen independently of the interface theme.
    pub(super) syntax: super::syntax::SyntaxStyle,
    /// Motion intensity from 0 (instant) to 1 (full); Reduced Motion still wins.
    pub(super) motion: f32,
    /// Global surface clarity for dark themes (0.0 darkest .. 1.0 brightest).
    pub(super) brightness: f32,
    /// Shortcut that opens Library Studio, e.g. "Ctrl+Shift+L".
    pub(super) library_shortcut: String,
    /// Explorer zoom, independent from the editor font size and the interface scale.
    pub(super) explorer_zoom: f32,
    /// Whole-interface scale (egui zoom factor); separate from both panel zooms.
    pub(super) ui_scale: f32,
    pub(super) transitions: Transitions,
    pub(super) discord: super::discord::DiscordPreferences,
    pub(super) language: super::i18n::Language,
    pub(super) identity: super::identity::IdentityPreferences,
    pub(super) font_size: f32,
    pub(super) compact: bool,
    pub(super) reduce_motion: bool,
    pub(super) quiet_notifications: bool,
    pub(super) notification_interval: u64,
    pub(super) high_contrast: bool,
    pub(super) assistant_open: bool,
    pub(super) tools_open: bool,
    pub(super) allow_auto_buffer: bool,
    pub(super) setup_completed: bool,
    pub(super) sidebar_width: f32,
    pub(super) assistant_width: f32,
    pub(super) tools_height: f32,
    /// Conversation shown in the right dock, by runtime session id.
    pub(super) agent_session: Option<String>,
    /// Conversations open in their own windows.
    pub(super) agent_detached: Vec<String>,
    pub(super) preview_width: f32,
    pub(super) recent: Vec<PathBuf>,
    notes: Vec<(PathBuf, String)>,
    tasks: Vec<(PathBuf, Vec<super::hub::Task>)>,
    pub(super) skills: Vec<super::skills::Skill>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme: super::theme::Theme::Obsidian,
            syntax: super::syntax::SyntaxStyle::Auto,
            motion: 1.0,
            brightness: 0.35,
            library_shortcut: "Ctrl+Shift+L".into(),
            explorer_zoom: 1.0,
            ui_scale: 1.0,
            transitions: Transitions::Subtle,
            discord: super::discord::DiscordPreferences::default(),
            language: super::i18n::Language::system(),
            identity: super::identity::IdentityPreferences::default(),
            font_size: 15.0,
            compact: false,
            reduce_motion: false,
            quiet_notifications: false,
            notification_interval: 15,
            high_contrast: false,
            assistant_open: true,
            tools_open: false,
            allow_auto_buffer: false,
            setup_completed: false,
            sidebar_width: 220.0,
            assistant_width: 340.0,
            tools_height: 210.0,
            agent_session: None,
            agent_detached: Vec::new(),
            preview_width: 520.0,
            recent: Vec::new(),
            notes: Vec::new(),
            tasks: Vec::new(),
            skills: Vec::new(),
        }
    }
}

pub(super) const EXPLORER_ZOOM: (f32, f32) = (0.8, 1.6);
pub(super) const UI_SCALE: (f32, f32) = (0.8, 1.5);
pub(super) const EDITOR_FONT: (f32, f32) = (11.0, 26.0);

fn clamp_finite(value: f32, default: f32, (lo, hi): (f32, f32)) -> f32 {
    if value.is_finite() {
        value.clamp(lo, hi)
    } else {
        default
    }
}

/// Continuity when moving between Home, a workspace and the first open file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Transitions {
    Off,
    #[default]
    Subtle,
    Expressive,
}

impl Transitions {
    pub(super) const ALL: [Self; 3] = [Self::Off, Self::Subtle, Self::Expressive];
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Subtle => "Subtle",
            Self::Expressive => "Expressive",
        }
    }
    /// Cross-fade length. Off is instant; nothing ever waits on it.
    pub(super) fn seconds(self) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::Subtle => 0.16,
            Self::Expressive => 0.28,
        }
    }
}

impl Preferences {
    pub(super) fn path() -> Option<PathBuf> {
        crate::paths::preferences_path()
    }

    pub(super) fn agent_rules_path() -> Option<PathBuf> {
        Self::path()?.parent().map(|dir| dir.join("agent-rules.md"))
    }

    pub(super) fn load_agent_rules() -> String {
        const MAX_RULES_BYTES: usize = 24 * 1024;
        let Some(path) = Self::agent_rules_path() else {
            return String::new();
        };
        let Ok(raw) = fs::read_to_string(path) else {
            return String::new();
        };
        if raw.len() <= MAX_RULES_BYTES {
            raw
        } else {
            raw[..raw.floor_char_boundary(MAX_RULES_BYTES)].to_owned()
        }
    }

    pub(super) fn save_agent_rules(content: &str) -> io::Result<PathBuf> {
        const MAX_RULES_BYTES: usize = 24 * 1024;
        let path = Self::agent_rules_path()
            .ok_or_else(|| io::Error::other("Missing settings folder."))?;
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::other("Missing settings folder."))?;
        fs::create_dir_all(parent)?;
        let bounded = if content.len() <= MAX_RULES_BYTES {
            content
        } else {
            &content[..content.floor_char_boundary(MAX_RULES_BYTES)]
        };
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        io::Write::write_all(file.as_file_mut(), bounded.as_bytes())?;
        file.as_file().sync_all()?;
        file.persist(&path)?;
        Ok(path)
    }

    pub(super) fn load(path: &Path) -> io::Result<Self> {
        let mut prefs: Self = match fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Self::default(),
            Err(e) => return Err(e),
        };
        prefs.identity.sanitize();
        prefs.notification_interval = prefs.notification_interval.clamp(5, 120);
        prefs.font_size = prefs.font_size.clamp(11.0, 26.0);
        prefs.explorer_zoom = clamp_finite(prefs.explorer_zoom, 1.0, EXPLORER_ZOOM);
        prefs.ui_scale = clamp_finite(prefs.ui_scale, 1.0, UI_SCALE);
        prefs.motion = if prefs.motion.is_finite() {
            prefs.motion.clamp(0.0, 1.0)
        } else {
            1.0
        };
        prefs.brightness = if prefs.brightness.is_finite() {
            prefs.brightness.clamp(0.0, 1.0)
        } else {
            0.35
        };
        prefs.sidebar_width = prefs.sidebar_width.clamp(155.0, 480.0);
        prefs.assistant_width = prefs.assistant_width.clamp(230.0, 520.0);
        prefs.tools_height = prefs.tools_height.clamp(130.0, 420.0);
        prefs.recent.truncate(8);
        Ok(prefs)
    }

    pub(super) fn save(&self, path: &Path) -> io::Result<()> {
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::other("Missing settings folder."))?;
        fs::create_dir_all(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        serde_json::to_writer_pretty(file.as_file_mut(), self)?;
        file.as_file().sync_all()?;
        file.persist(path)?;
        Ok(())
    }

    pub(super) fn tasks(&self, root: &Path) -> Vec<super::hub::Task> {
        self.tasks
            .iter()
            .find(|(path, _)| path == root)
            .map_or_else(Vec::new, |(_, tasks)| tasks.clone())
    }

    pub(super) fn set_tasks(&mut self, root: &Path, tasks: &[super::hub::Task]) {
        self.tasks.retain(|(path, _)| path != root);
        if !tasks.is_empty() {
            self.tasks.insert(
                0,
                (
                    root.to_owned(),
                    tasks[tasks.len().saturating_sub(40)..].to_vec(),
                ),
            );
            self.tasks.truncate(16);
        }
    }

    pub(super) fn note(&self, root: &Path) -> &str {
        self.notes
            .iter()
            .find(|(path, _)| path == root)
            .map_or("", |(_, note)| note.as_str())
    }

    pub(super) fn set_note(&mut self, root: &Path, note: String) {
        self.notes.retain(|(path, _)| path != root);
        if !note.trim().is_empty() {
            self.notes.insert(0, (root.to_owned(), note));
        }
        self.notes.truncate(32);
    }

    pub(super) fn remember(&mut self, root: &Path) {
        self.recent.retain(|p| p != root);
        self.recent.insert(0, root.to_owned());
        self.recent.truncate(8);
    }

    /// Safely import allowlisted UI settings from a known local editor config file.
    pub(super) fn import_from(&mut self, source: ExternalEditor) -> Result<Vec<&'static str>, String> {
        let Some(path) = source.settings_path() else {
            return Err(format!("Could not locate {} settings folder.", source.name()));
        };
        if !path.is_file() {
            return Err(format!("No {} settings file found.", source.name()));
        }
        self.import_from_path(source, &path)
    }

    fn import_from_path(
        &mut self,
        source: ExternalEditor,
        path: &Path,
    ) -> Result<Vec<&'static str>, String> {
        const MAX_IMPORT_BYTES: u64 = 512 * 1024;
        let meta = fs::metadata(path)
            .map_err(|e| format!("Could not read {} settings: {e}", source.name()))?;
        if meta.len() > MAX_IMPORT_BYTES {
            return Err(format!("{} settings file is too large to import.", source.name()));
        }
        let raw = fs::read_to_string(path)
            .map_err(|e| format!("Could not read {} settings: {e}", source.name()))?;
        let cleaned = strip_jsonc(&raw);
        let value: serde_json::Value = serde_json::from_str(&cleaned)
            .map_err(|_| format!("{} settings file is not valid JSON.", source.name()))?;
        let Some(obj) = value.as_object() else {
            return Err(format!("{} settings file must be a JSON object.", source.name()));
        };
        let mut applied = Vec::new();
        let font_key = match source {
            ExternalEditor::Zed => "buffer_font_size",
            _ => "editor.fontSize",
        };
        if let Some(size) = obj.get(font_key).and_then(|v| v.as_f64())
            && size.is_finite()
        {
            self.font_size = (size as f32).clamp(EDITOR_FONT.0, EDITOR_FONT.1);
            applied.push("font size");
        }
        let theme_name = match source {
            ExternalEditor::Zed => obj
                .get("theme")
                .and_then(|v| v.as_str().or_else(|| v.get("mode").and_then(|m| m.as_str()))),
            _ => obj.get("workbench.colorTheme").and_then(|v| v.as_str()),
        };
        if let Some(name) = theme_name {
            let lower = name.to_ascii_lowercase();
            if lower.contains("light") || lower.contains("day") {
                self.theme = super::theme::Theme::Light;
                applied.push("theme");
            } else if lower.contains("high contrast") {
                self.theme = super::theme::Theme::HighContrast;
                applied.push("theme");
            } else if !lower.is_empty() {
                self.theme = super::theme::Theme::Obsidian;
                applied.push("theme");
            }
        }
        if let Some(line_height) = obj.get("editor.lineHeight").and_then(|v| v.as_f64())
            && line_height.is_finite()
            && line_height > 0.0
        {
            self.compact = line_height <= 1.25 || (line_height > 10.0 && line_height <= 20.0);
            applied.push("density");
        }
        if applied.is_empty() {
            return Err(format!("No compatible settings found in {}.", source.name()));
        }
        Ok(applied)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ExternalEditor {
    VsCode,
    Cursor,
    Windsurf,
    Zed,
}

impl ExternalEditor {
    pub(super) const ALL: [Self; 4] = [Self::VsCode, Self::Cursor, Self::Windsurf, Self::Zed];

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::VsCode => "VS Code",
            Self::Cursor => "Cursor",
            Self::Windsurf => "Windsurf",
            Self::Zed => "Zed",
        }
    }

    pub(super) fn settings_path(self) -> Option<PathBuf> {
        #[cfg(windows)]
        let base = std::env::var_os("APPDATA").map(PathBuf::from)?;
        #[cfg(target_os = "macos")]
        let base = std::env::var_os("HOME")
            .map(PathBuf::from)?
            .join("Library/Application Support");
        #[cfg(all(unix, not(target_os = "macos")))]
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;

        Some(match self {
            Self::VsCode => base.join("Code").join("User").join("settings.json"),
            Self::Cursor => base.join("Cursor").join("User").join("settings.json"),
            Self::Windsurf => base.join("Windsurf").join("User").join("settings.json"),
            Self::Zed => {
                #[cfg(target_os = "macos")]
                {
                    std::env::var_os("HOME")
                        .map(PathBuf::from)?
                        .join(".config/zed/settings.json")
                }
                #[cfg(not(target_os = "macos"))]
                {
                    base.join("Zed").join("settings.json")
                }
            }
        })
    }
}

/// Remove `//` comments and trailing commas from JSONC editor configs without touching strings.
fn strip_jsonc(input: &str) -> String {
    let mut no_comments = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;
    while let Some(c) = chars.next() {
        if in_string {
            no_comments.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        if c == '"' {
            in_string = true;
            no_comments.push(c);
        } else if c == '/' && chars.peek() == Some(&'/') {
            for next in chars.by_ref() {
                if next == '\n' {
                    no_comments.push('\n');
                    break;
                }
            }
        } else {
            no_comments.push(c);
        }
    }
    // Strip trailing commas before `}` or `]`.
    let bytes = no_comments.as_bytes();
    let mut out = String::with_capacity(no_comments.len());
    let mut i = 0;
    in_string = false;
    escaped = false;
    while i < bytes.len() {
        let ch = no_comments[i..].chars().next().unwrap();
        let len = ch.len_utf8();
        if in_string {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            i += len;
            continue;
        }
        if ch == '"' {
            in_string = true;
            out.push(ch);
            i += len;
            continue;
        }
        if ch == ',' {
            let rest = no_comments[i + len..].trim_start();
            if rest.starts_with('}') || rest.starts_with(']') {
                i += len;
                continue;
            }
        }
        out.push(ch);
        i += len;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_round_trip_and_bound_legacy_values() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("nested/editor.json");
        let mut prefs = Preferences {
            theme: super::super::theme::Theme::Light,
            high_contrast: true,
            compact: true,
            reduce_motion: true,
            font_size: 18.0,
            tools_open: true,
            ..Default::default()
        };
        prefs.remember(Path::new("project"));
        prefs.remember(Path::new("project"));
        prefs.save(&path)?;
        assert_eq!(Preferences::load(&path)?, prefs);
        fs::write(&path, br#"{"font_size":98}"#)?;
        assert_eq!(Preferences::load(&path)?.font_size, 26.0);
        assert_eq!(Preferences::load(&path)?.assistant_width, 340.0);
        // MF-01: the old default background moves to Dither Field once; a chosen one stays.
        fs::write(&path, br#"{"identity":{"background":"orbital"}}"#)?;
        assert_eq!(
            Preferences::load(&path)?.identity.background,
            super::super::identity::Background::Dither
        );
        fs::write(
            &path,
            br#"{"identity":{"background":"orbital","background_chosen":true}}"#,
        )?;
        assert_eq!(
            Preferences::load(&path)?.identity.background,
            super::super::identity::Background::Orbital
        );
        // MF-05: zooms are independent and clamped.
        fs::write(
            &path,
            br#"{"explorer_zoom":9.0,"ui_scale":0.1,"font_size":12}"#,
        )?;
        let loaded = Preferences::load(&path)?;
        assert_eq!(loaded.explorer_zoom, EXPLORER_ZOOM.1);
        assert_eq!(loaded.ui_scale, UI_SCALE.0);
        assert_eq!(loaded.font_size, 12.0);
        assert_eq!(loaded.transitions, Transitions::Subtle);
        assert!(!loaded.discord.enabled && !loaded.discord.show_project);
        Ok(())
    }

    #[test]
    fn external_editor_import_applies_safe_fields_and_bounds_values() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("settings.json");
        fs::write(
            &path,
            r#"{
                // User editor preferences
                "editor.fontSize": 99,
                "workbench.colorTheme": "Default Light Modern",
                "editor.lineHeight": 18,
                "secret.token": "should-never-be-read",
            }"#,
        )?;
        let mut prefs = Preferences::default();
        let applied = prefs
            .import_from_path(ExternalEditor::VsCode, &path)
            .expect("import valid settings");
        assert_eq!(prefs.font_size, EDITOR_FONT.1);
        assert_eq!(prefs.theme, super::super::theme::Theme::Light);
        assert!(prefs.compact);
        assert!(applied.contains(&"font size"));
        assert!(applied.contains(&"theme"));

        let oversized = dir.path().join("huge.json");
        fs::write(&oversized, vec![b' '; 600 * 1024])?;
        assert!(prefs.import_from_path(ExternalEditor::VsCode, &oversized).is_err());
        Ok(())
    }
}

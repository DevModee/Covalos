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
    pub(super) sidebar_width: f32,
    pub(super) assistant_width: f32,
    pub(super) tools_height: f32,
    pub(super) recent: Vec<PathBuf>,
    notes: Vec<(PathBuf, String)>,
    tasks: Vec<(PathBuf, Vec<super::hub::Task>)>,
    pub(super) skills: Vec<super::skills::Skill>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme: super::theme::Theme::Nocturne,
            syntax: super::syntax::SyntaxStyle::Auto,
            motion: 1.0,
            library_shortcut: "Ctrl+Shift+L".into(),
            explorer_zoom: 1.0,
            ui_scale: 1.0,
            transitions: Transitions::Subtle,
            discord: super::discord::DiscordPreferences::default(),
            language: super::i18n::Language::system(),
            identity: super::identity::IdentityPreferences::default(),
            font_size: 14.0,
            compact: false,
            reduce_motion: false,
            quiet_notifications: false,
            notification_interval: 15,
            high_contrast: false,
            assistant_open: true,
            tools_open: false,
            allow_auto_buffer: false,
            sidebar_width: 220.0,
            assistant_width: 340.0,
            tools_height: 210.0,
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
}

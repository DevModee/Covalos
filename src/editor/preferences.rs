use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Preferences {
    pub(super) theme: super::theme::Theme,
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
            theme: super::theme::Theme::Dark,
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

impl Preferences {
    pub(super) fn path() -> Option<PathBuf> {
        if let Some(path) = std::env::var_os("AEROS_CONFIG_DIR").filter(|p| !p.is_empty()) {
            return Some(PathBuf::from(path).join("editor.json"));
        }
        let home = || std::env::var_os("HOME").map(PathBuf::from);
        let base = if cfg!(windows) {
            std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
        } else if cfg!(target_os = "macos") {
            home().map(|p| p.join("Library/Application Support"))
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| home().map(|p| p.join(".config")))
        };
        base.map(|p| p.join("AerosEditor/editor.json"))
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
        prefs.sidebar_width = prefs.sidebar_width.clamp(155.0, 340.0);
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
        Ok(())
    }
}

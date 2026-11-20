//! Centralized configuration paths and safe, idempotent migration from legacy AEROS data.
//!
//! Resolution order:
//! 1. `COVALOS_CONFIG_DIR` (explicit override; skips automatic legacy migration)
//! 2. `AEROS_CONFIG_DIR` (backwards-compatible override for existing scripts/harnesses)
//! 3. OS standard application directory (`Covalos`), migrating once from `AerosEditor`
//!    without deleting or overwriting existing data.
use serde::Serialize;
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Once,
    time::{SystemTime, UNIX_EPOCH},
};

pub const APP_DIR: &str = "Covalos";
pub const LEGACY_APP_DIR: &str = "AerosEditor";
const MIGRATION_MARKER: &str = ".migrated-from-aeros";
const MAX_MIGRATION_DEPTH: usize = 8;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct MigrationReport {
    pub migrated: bool,
    pub copied_files: usize,
    pub skipped_existing: usize,
}

#[derive(Serialize)]
struct MigrationMarker {
    version: u8,
    from: &'static str,
    migrated_at_unix: u64,
    copied_files: usize,
    skipped_existing: usize,
}

/// Returns the active COVALOS configuration directory.
/// When using the default OS location, runs the one-time legacy migration if needed.
pub fn config_dir() -> Option<PathBuf> {
    if let Some(dir) = explicit_config_dir() {
        return Some(dir);
    }
    let current = default_config_dir()?;
    static MIGRATE_ONCE: Once = Once::new();
    MIGRATE_ONCE.call_once(|| {
        if let Some(legacy) = default_legacy_config_dir() {
            let _ = migrate_directory(&legacy, &current);
        }
        // On Linux, Library Studio previously lived under XDG_CACHE_HOME/AerosEditor/library.
        if !cfg!(windows)
            && !cfg!(target_os = "macos")
            && let Some(legacy_lib) = default_legacy_linux_library_dir()
        {
            let _ = copy_missing_tree(&legacy_lib, &current.join("library"), 0);
        }
    });
    Some(current)
}

pub fn preferences_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("editor.json"))
}

pub fn library_dir() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("library"))
}

pub fn agent_bridge_dir() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("agent-bridge"))
}

fn explicit_config_dir() -> Option<PathBuf> {
    for key in ["COVALOS_CONFIG_DIR", "AEROS_CONFIG_DIR"] {
        if let Some(val) = std::env::var_os(key).filter(|s| !s.is_empty()) {
            return Some(PathBuf::from(val));
        }
    }
    None
}

fn os_config_base() -> Option<PathBuf> {
    let home = || std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        home().map(|p| p.join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| home().map(|p| p.join(".config")))
    }
}

fn default_config_dir() -> Option<PathBuf> {
    let name = if cfg!(windows) || cfg!(target_os = "macos") {
        APP_DIR
    } else {
        "covalos"
    };
    os_config_base().map(|base| base.join(name))
}

fn default_legacy_config_dir() -> Option<PathBuf> {
    os_config_base().map(|base| base.join(LEGACY_APP_DIR))
}

fn default_legacy_linux_library_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .map(|p| p.join(LEGACY_APP_DIR).join("library"))
}

/// Safely copies legacy AEROS user data into `target` once, without ever overwriting
/// existing COVALOS files or modifying/deleting the `legacy` directory.
pub fn migrate_directory(legacy: &Path, target: &Path) -> io::Result<MigrationReport> {
    if !legacy.is_dir() || legacy == target {
        return Ok(MigrationReport::default());
    }
    let marker_path = target.join(MIGRATION_MARKER);
    if marker_path.is_file() {
        return Ok(MigrationReport::default());
    }

    fs::create_dir_all(target)?;
    let (copied_files, skipped_existing) = copy_missing_tree(legacy, target, 0)?;

    let marker = MigrationMarker {
        version: 1,
        from: LEGACY_APP_DIR,
        migrated_at_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
        copied_files,
        skipped_existing,
    };
    let mut temp = tempfile::NamedTempFile::new_in(target)?;
    serde_json::to_writer_pretty(temp.as_file_mut(), &marker)?;
    temp.as_file().sync_all()?;
    temp.persist(&marker_path).map_err(|e| e.error)?;

    Ok(MigrationReport {
        migrated: true,
        copied_files,
        skipped_existing,
    })
}

fn copy_missing_tree(src: &Path, dst: &Path, depth: usize) -> io::Result<(usize, usize)> {
    if depth > MAX_MIGRATION_DEPTH || !src.is_dir() {
        return Ok((0, 0));
    }
    fs::create_dir_all(dst)?;
    let mut copied = 0;
    let mut skipped = 0;
    for entry in fs::read_dir(src)?.flatten() {
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        // Never follow symlinks during profile migration.
        if file_type.is_symlink() {
            continue;
        }
        let name = entry.file_name();
        if name == MIGRATION_MARKER {
            continue;
        }
        let from = entry.path();
        let to = dst.join(&name);
        if file_type.is_dir() {
            let (c, s) = copy_missing_tree(&from, &to, depth + 1)?;
            copied += c;
            skipped += s;
        } else if file_type.is_file() {
            if to.exists() {
                skipped += 1;
            } else {
                fs::copy(&from, &to)?;
                copied += 1;
            }
        }
    }
    Ok((copied, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_install_without_legacy_folder_creates_nothing() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let legacy = dir.path().join("AerosEditor");
        let target = dir.path().join("Covalos");
        let report = migrate_directory(&legacy, &target)?;
        assert_eq!(report, MigrationReport::default());
        assert!(!target.exists());
        Ok(())
    }

    #[test]
    fn migrates_legacy_data_without_overwriting_existing_or_repeating() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let legacy = dir.path().join("AerosEditor");
        let target = dir.path().join("Covalos");

        // Populate legacy AEROS data across settings, conversations, library, and agent-bridge.
        fs::create_dir_all(legacy.join("conversations"))?;
        fs::create_dir_all(legacy.join("library"))?;
        fs::create_dir_all(legacy.join("agent-bridge"))?;
        fs::write(legacy.join("editor.json"), br#"{"font_size":16}"#)?;
        fs::write(
            legacy.join("conversations/abc.json"),
            br#"{"version":1,"messages":[]}"#,
        )?;
        fs::write(
            legacy.join("library/state.json"),
            br#"{"favorites":["aeros/dither-field"]}"#,
        )?;
        fs::write(legacy.join("agent-bridge/session-1.json"), br#"{}"#)?;

        // Pre-create one existing COVALOS file to verify it is never overwritten.
        fs::create_dir_all(&target)?;
        fs::write(target.join("editor.json"), br#"{"font_size":18}"#)?;

        let first = migrate_directory(&legacy, &target)?;
        assert!(first.migrated);
        assert_eq!(first.copied_files, 3);
        assert_eq!(first.skipped_existing, 1);

        // Existing COVALOS settings were preserved intact; missing data was migrated.
        assert_eq!(
            fs::read_to_string(target.join("editor.json"))?,
            r#"{"font_size":18}"#
        );
        assert!(target.join("conversations/abc.json").is_file());
        assert!(target.join("library/state.json").is_file());
        assert!(target.join("agent-bridge/session-1.json").is_file());
        // Legacy directory remains untouched.
        assert!(legacy.join("editor.json").is_file());

        // Second run is a no-op because of the migration marker.
        fs::write(legacy.join("late-file.json"), b"{}")?;
        let second = migrate_directory(&legacy, &target)?;
        assert!(!second.migrated);
        assert!(!target.join("late-file.json").exists());
        Ok(())
    }
}

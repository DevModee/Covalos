use std::{
    ffi::OsStr,
    fs::{self, File},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use regex::{Regex, RegexBuilder};

const MAX_TEXT_BYTES: u64 = 1024 * 1024;
pub const MAX_SEARCH_HITS: usize = 200;

pub struct Entry {
    pub path: PathBuf,
    pub directory: bool,
}

pub fn read_directory(path: &Path) -> io::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for item in fs::read_dir(path)? {
        let item = item?;
        let kind = item.file_type()?;
        if item
            .file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case(".git")
        {
            continue;
        }
        if kind.is_dir() && is_link(&fs::symlink_metadata(item.path())?) {
            continue;
        }
        if kind.is_dir() || kind.is_file() {
            entries.push(Entry {
                path: item.path(),
                directory: kind.is_dir(),
            });
        }
    }
    entries.sort_by_cached_key(|entry| (!entry.directory, filename(&entry.path).to_lowercase()));
    Ok(entries)
}

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        metadata.is_symlink()
    }
}

pub fn filename(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

pub struct Document {
    pub path: PathBuf,
    pub text: String,
    pub dirty: bool,
    original: Vec<u8>,
    saved_text: String,
    bom: bool,
    crlf: bool,
    /// Length and modification time last seen on disk.
    stamp: Option<(u64, std::time::SystemTime)>,
}

/// Result of comparing an open document with its file.
#[derive(Debug, PartialEq, Eq)]
pub enum DiskState {
    Same,
    /// The buffer was clean and now holds the new file contents.
    Reloaded,
    /// The file changed while the buffer has unsaved edits.
    Conflict,
}

fn stamp(path: &Path) -> Option<(u64, std::time::SystemTime)> {
    let meta = fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()?))
}

impl Document {
    pub fn open(path: PathBuf) -> io::Result<Self> {
        let original = read_bytes(&path)?;
        let (raw, bom) = decode_text(&original)?;
        let crlf = raw.contains("\r\n")
            && raw.bytes().filter(|&byte| byte == b'\n').count() == raw.matches("\r\n").count();
        let text = if crlf {
            raw.replace("\r\n", "\n")
        } else {
            raw.to_owned()
        };
        Ok(Self {
            stamp: stamp(&path),
            path,
            saved_text: text.clone(),
            text,
            dirty: false,
            original,
            bom,
            crlf,
        })
    }

    /// Notices edits made outside the editor (agents, git, other tools).
    /// A clean buffer reloads; a dirty one reports a conflict once.
    pub fn sync_from_disk(&mut self) -> DiskState {
        let now = stamp(&self.path);
        if now.is_none() || now == self.stamp {
            return DiskState::Same;
        }
        self.stamp = now;
        let Ok(bytes) = read_bytes(&self.path) else {
            return DiskState::Same;
        };
        if bytes == self.original {
            return DiskState::Same;
        }
        if self.dirty {
            return DiskState::Conflict;
        }
        match Self::open(self.path.clone()) {
            Ok(fresh) => {
                *self = fresh;
                DiskState::Reloaded
            }
            Err(_) => DiskState::Same,
        }
    }

    /// Discards unsaved edits and loads the file as it is on disk.
    pub fn reload(&mut self) -> io::Result<()> {
        *self = Self::open(self.path.clone())?;
        Ok(())
    }

    /// Keeps the buffer and lets the next save replace the disk version.
    pub fn keep_mine(&mut self) {
        if let Ok(bytes) = read_bytes(&self.path) {
            self.original = bytes;
            self.stamp = stamp(&self.path);
        }
    }

    pub fn update_dirty(&mut self) {
        self.dirty = self.text != self.saved_text;
    }

    pub fn save(&mut self) -> io::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        let mut bytes = Vec::new();
        if self.bom {
            bytes.extend_from_slice(b"\xef\xbb\xbf");
        }
        if self.crlf {
            bytes.extend_from_slice(self.text.replace("\n", "\r\n").as_bytes());
        } else {
            bytes.extend_from_slice(self.text.as_bytes());
        }
        if bytes.len() as u64 > MAX_TEXT_BYTES {
            return Err(io::Error::other("The file exceeds the 1 MiB editor limit."));
        }
        let parent = self
            .path
            .parent()
            .ok_or_else(|| io::Error::other("Missing parent folder."))?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(&bytes)?;
        if read_bytes(&self.path)? != self.original {
            return Err(io::Error::other(
                "The file changed on disk. Reopen it before saving.",
            ));
        }
        temporary
            .as_file()
            .set_permissions(fs::metadata(&self.path)?.permissions())?;
        temporary.as_file().sync_all()?;
        temporary.persist(&self.path)?;
        self.saved_text.clone_from(&self.text);
        self.original = bytes;
        self.dirty = false;
        self.stamp = stamp(&self.path);
        Ok(())
    }
}

fn read_bytes(path: &Path) -> io::Result<Vec<u8>> {
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(io::Error::other("Only regular files can be opened."));
    }
    let mut bytes = Vec::new();
    file.take(MAX_TEXT_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_TEXT_BYTES {
        return Err(io::Error::other("The file exceeds the 1 MiB editor limit."));
    }
    Ok(bytes)
}

fn decode_text(bytes: &[u8]) -> io::Result<(&str, bool)> {
    let bom = bytes.strip_prefix(b"\xef\xbb\xbf");
    let text = std::str::from_utf8(bom.unwrap_or(bytes))
        .map_err(|_| io::Error::other("Only UTF-8 text files can be opened."))?;
    if text.contains('\0') {
        return Err(io::Error::other("Binary files cannot be opened."));
    }
    Ok((text, bom.is_some()))
}

pub fn workspace_target(root: &Path, name: &str) -> io::Result<PathBuf> {
    let relative = Path::new(name.trim());
    if relative.as_os_str().is_empty()
        || !relative.components().all(|part| {
            let Component::Normal(name) = part else {
                return false;
            };
            let name = name.to_string_lossy();
            !name.eq_ignore_ascii_case(".git")
                && (!cfg!(windows) || (!name.contains(':') && !name.ends_with(['.', ' '])))
        })
    {
        return Err(io::Error::other("Enter a path inside the workspace."));
    }
    let path = root.join(relative);
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Missing parent folder."))?
        .canonicalize()?;
    if !parent.starts_with(root)
        || parent.strip_prefix(root).is_ok_and(|p| {
            p.components().any(|part| {
                part.as_os_str()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(".git")
            })
        })
    {
        return Err(io::Error::other(
            "The path points outside the editable workspace.",
        ));
    }
    Ok(parent.join(
        path.file_name()
            .ok_or_else(|| io::Error::other("Enter a name."))?,
    ))
}

pub fn create_file(root: &Path, name: &str) -> io::Result<PathBuf> {
    let path = workspace_target(root, name)?;
    let parent = path.parent().expect("workspace target has a parent");
    tempfile::NamedTempFile::new_in(parent)?.persist_noclobber(&path)?;
    Ok(path)
}

pub fn create_folder(root: &Path, name: &str) -> io::Result<PathBuf> {
    let path = workspace_target(root, name)?;
    fs::create_dir(&path)?;
    Ok(path)
}

fn require_managed_entry(root: &Path, path: &Path) -> io::Result<fs::Metadata> {
    let relative = path.strip_prefix(root).map_err(io::Error::other)?;
    let checked = workspace_target(root, &relative.to_string_lossy())?;
    let entry = fs::symlink_metadata(&checked)?;
    if checked != path || is_link(&entry) || (!entry.is_file() && !entry.is_dir()) {
        return Err(io::Error::other(
            "Select a regular file or folder inside the workspace.",
        ));
    }
    Ok(entry)
}

pub fn move_entry(root: &Path, path: &Path, name: &str) -> io::Result<PathBuf> {
    require_managed_entry(root, path)?;
    let target = workspace_target(root, name)?;
    if target == path {
        return Ok(target);
    }
    if target.starts_with(path) {
        return Err(io::Error::other("A folder cannot be moved into itself."));
    }
    rename_without_replacing(path, &target)?;
    Ok(target)
}

#[cfg(windows)]
fn rename_without_replacing(source: &Path, target: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let utf16 = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    let source = utf16(source);
    let target = utf16(target);
    // MoveFileExW with flags 0 fails if the destination exists.
    let moved = unsafe {
        windows_sys::Win32::Storage::FileSystem::MoveFileExW(source.as_ptr(), target.as_ptr(), 0)
    };
    if moved == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn rename_without_replacing(source: &Path, target: &Path) -> io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let source = CString::new(source.as_os_str().as_bytes()).map_err(io::Error::other)?;
    let target = CString::new(target.as_os_str().as_bytes()).map_err(io::Error::other)?;
    #[cfg(target_os = "linux")]
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            target.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    #[cfg(target_os = "macos")]
    let result = unsafe { libc::renamex_np(source.as_ptr(), target.as_ptr(), libc::RENAME_EXCL) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
fn rename_without_replacing(_source: &Path, _target: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Safe moves are unavailable on this platform.",
    ))
}

pub fn delete_entry(root: &Path, path: &Path) -> io::Result<()> {
    let entry = require_managed_entry(root, path)?;
    if entry.is_dir() {
        fs::remove_dir(path)
    } else {
        fs::remove_file(path)
    }
}

pub fn project_files(root: &Path, stop: &AtomicBool) -> Vec<PathBuf> {
    ignore::WalkBuilder::new(root)
        .hidden(false)
        .filter_entry(|entry| {
            !matches!(
                entry.file_name().to_str(),
                Some(".git" | "target" | "node_modules")
            )
        })
        .build()
        .take_while(|_| !stop.load(Ordering::Relaxed))
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
        .take(12_000)
        .map(ignore::DirEntry::into_path)
        .collect()
}

pub fn literal_query(query: &str) -> Regex {
    RegexBuilder::new(&regex::escape(query))
        .case_insensitive(true)
        .build()
        .expect("escaped UI queries are valid regular expressions")
}

pub struct SearchHit {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub text: String,
}

pub fn search(
    root: &Path,
    query: &str,
    cancelled: Arc<AtomicBool>,
    mut hit: impl FnMut(SearchHit) -> bool,
) {
    let matcher = literal_query(query);
    let mut count = 0;
    let walker = ignore::WalkBuilder::new(root)
        .hidden(false)
        .filter_entry(|entry| {
            entry.file_name() != OsStr::new(".git")
                && entry.file_name() != OsStr::new("target")
                && entry.file_name() != OsStr::new("node_modules")
        })
        .build();
    for entry in walker.filter_map(Result::ok) {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let Ok(bytes) = read_bytes(entry.path()) else {
            continue;
        };
        let Ok((text, _)) = decode_text(&bytes) else {
            continue;
        };
        let mut offset = 0;
        for (index, line) in text.lines().enumerate() {
            if let Some(found) = matcher.find(line) {
                if !hit(SearchHit {
                    path: entry.path().to_path_buf(),
                    line: index + 1,
                    column: line[..found.start()].chars().count(),
                    text: line.trim().chars().take(150).collect(),
                }) {
                    return;
                }
                count += 1;
                if count >= MAX_SEARCH_HITS {
                    return;
                }
            }
            offset += line.len();
            if offset > 32768 {
                offset = 0;
                if cancelled.load(Ordering::Relaxed) {
                    return;
                }
            }
        }
    }
}

/// egui cursors count Unicode scalar values. Count directly without allocating a prefix on every frame.
pub fn cursor_position(text: &str, cursor: usize) -> (usize, usize) {
    text.chars()
        .take(cursor)
        .fold((1, 1), |(line, column), ch| {
            if ch == '\n' {
                (line + 1, 1)
            } else {
                (line, column + 1)
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_edits_reload_clean_buffers_and_flag_dirty_ones() -> io::Result<()> {
        let folder = tempfile::tempdir()?;
        let path = folder.path().join("app.tsx");
        fs::write(&path, "export const a = 1;\n")?;
        let mut doc = Document::open(path.clone())?;
        assert_eq!(doc.sync_from_disk(), DiskState::Same);
        fs::write(&path, "export const a = 2; // agent edit\n")?;
        assert_eq!(doc.sync_from_disk(), DiskState::Reloaded);
        assert!(doc.text.contains("agent edit") && !doc.dirty);
        doc.text.push_str("// mine\n");
        doc.update_dirty();
        fs::write(&path, "export const a = 3;\n")?;
        assert_eq!(doc.sync_from_disk(), DiskState::Conflict);
        assert_eq!(doc.sync_from_disk(), DiskState::Same, "reported once");
        assert!(doc.save().is_err(), "save still refuses to clobber");
        doc.keep_mine();
        doc.save()?;
        assert!(fs::read_to_string(&path)?.contains("// mine"));
        Ok(())
    }

    #[test]
    fn preserves_bom_and_crlf_and_rejects_external_changes() -> io::Result<()> {
        let folder = tempfile::tempdir()?;
        let path = folder.path().join("example.txt");
        fs::write(&path, b"\xef\xbb\xbfhello\r\n")?;
        let mut document = Document::open(path.clone())?;
        assert_eq!(document.text, "hello\n");
        document.text.push_str("world\n");
        document.update_dirty();
        document.save()?;
        assert_eq!(fs::read(&path)?, b"\xef\xbb\xbfhello\r\nworld\r\n");
        document.text.push('!');
        document.update_dirty();
        fs::write(&path, "Someone saved new contents.")?;
        assert!(document.save().is_err());
        assert!(document.dirty);
        assert_eq!(fs::read_to_string(path)?, "Someone saved new contents.");
        Ok(())
    }

    #[test]
    fn workspace_mutations_do_not_replace_existing_files_or_touch_git() -> io::Result<()> {
        let folder = tempfile::tempdir()?;
        let root = folder.path().canonicalize()?;
        let dir = create_folder(&root, "assets")?;
        let file = create_file(&root, "assets/file.txt")?;
        let other = create_file(&root, "existing.txt")?;
        fs::write(&other, "preserve")?;
        assert!(create_file(&root, "existing.txt").is_err());
        assert!(move_entry(&root, &file, "existing.txt").is_err());
        assert_eq!(fs::read_to_string(&other)?, "preserve");
        assert!(delete_entry(&root, &dir).is_err());
        let moved = move_entry(&root, &file, "renamed.txt")?;
        assert!(moved.is_file());
        assert!(!file.exists());
        assert!(create_folder(&root, ".GiT").is_err());
        assert!(create_file(&root, "../outside.txt").is_err());
        delete_entry(&root, &dir)?;
        delete_entry(&root, &moved)?;
        Ok(())
    }

    #[test]
    fn cursor_positions_handle_unicode_newlines_and_out_of_range_indices() {
        assert_eq!(cursor_position("", 0), (1, 1));
        assert_eq!(cursor_position("a\ncafé😀\n", 2), (2, 1));
        assert_eq!(cursor_position("a\ncafé😀\n", 7), (2, 6));
        assert_eq!(cursor_position("a\ncafé😀\n", 99), (3, 1));
    }
    #[test]
    #[ignore = "measures large-buffer status position calculation; run in release"]
    fn benchmark_cursor_positions() {
        use std::{hint::black_box, time::Instant};
        let text = "let value = café; // 😀\n".repeat(30_000);
        let cursor = text.chars().count();
        let iterations = 200;
        let start = Instant::now();
        for _ in 0..iterations {
            let before: String = black_box(&text).chars().take(black_box(cursor)).collect();
            black_box((
                before.bytes().filter(|&b| b == b'\n').count() + 1,
                before
                    .rsplit('\n')
                    .next()
                    .map_or(1, |s| s.chars().count() + 1),
            ));
        }
        let original = start.elapsed();
        let start = Instant::now();
        for _ in 0..iterations {
            black_box(cursor_position(black_box(&text), black_box(cursor)));
        }
        let improved = start.elapsed();
        println!(
            "cursor benchmark: bytes={} iterations={} original_ms={:.3} direct_ms={:.3}",
            text.len(),
            iterations,
            original.as_secs_f64() * 1000.0,
            improved.as_secs_f64() * 1000.0
        );
    }
}

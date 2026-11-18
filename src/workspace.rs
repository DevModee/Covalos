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
        if item.file_name() != ".git" && (kind.is_dir() || kind.is_file()) {
            entries.push(Entry {
                path: item.path(),
                directory: kind.is_dir(),
            });
        }
    }
    entries.sort_by_cached_key(|entry| (!entry.directory, filename(&entry.path).to_lowercase()));
    Ok(entries)
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
            path,
            saved_text: text.clone(),
            text,
            dirty: false,
            original,
            bom,
            crlf,
        })
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

pub fn create_file(root: &Path, name: &str) -> io::Result<PathBuf> {
    let relative = Path::new(name.trim());
    if relative.as_os_str().is_empty()
        || !relative
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        || relative
            .components()
            .any(|part| part.as_os_str() == OsStr::new(".git"))
    {
        return Err(io::Error::other(
            "Enter a file path relative to the workspace.",
        ));
    }
    let path = root.join(relative);
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Missing parent folder."))?
        .canonicalize()?;
    if !parent.starts_with(root) {
        return Err(io::Error::other("The path points outside the workspace."));
    }
    let path = parent.join(
        path.file_name()
            .ok_or_else(|| io::Error::other("Enter a file name."))?,
    );
    tempfile::NamedTempFile::new_in(parent)?.persist_noclobber(&path)?;
    Ok(path)
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

#[cfg(test)]
mod tests {
    use super::*;

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
}

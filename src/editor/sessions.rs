use crate::assistant::Provider;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

const MAX_BYTES: usize = 2 * 1024 * 1024;
const HISTORY_BUDGET: usize = 1024 * 1024;

#[derive(Serialize, Deserialize)]
pub(super) struct Conversation {
    version: u8,
    root: PathBuf,
    pub(super) provider: Provider,
    pub(super) session: Option<String>,
    pub(super) draft: String,
    pub(super) messages: Vec<(String, String)>,
    pub(super) interrupted: bool,
}

impl Conversation {
    pub(super) fn new(
        root: &Path,
        provider: Provider,
        session: Option<String>,
        draft: &str,
        messages: impl Iterator<Item = (String, String)>,
        interrupted: bool,
    ) -> Self {
        let mut value = Self {
            version: 1,
            root: root.to_owned(),
            provider,
            session,
            draft: draft.to_owned(),
            messages: messages.collect(),
            interrupted,
        };
        value.bound();
        value
    }

    fn bound(&mut self) {
        self.draft.truncate(self.draft.floor_char_boundary(48_000));
        if self.messages.len() > 100 {
            self.messages.drain(..self.messages.len() - 100);
        }
        let mut bytes = 0;
        for (_, text) in self.messages.iter_mut().rev() {
            let end = text.floor_char_boundary(100_000.min(HISTORY_BUDGET - bytes));
            text.truncate(end);
            bytes += text.len();
        }
        if self.session.as_ref().is_some_and(|id| {
            !id.as_bytes().first().is_some_and(u8::is_ascii_alphanumeric)
                || id.len() > 256
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        }) {
            self.session = None;
        }
    }

    pub(super) fn save(&self, config: &Path) -> io::Result<()> {
        let path = path(config, &self.root)?;
        let parent = path.parent().expect("session directory");
        fs::create_dir_all(parent)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        serde_json::to_writer(temporary.as_file_mut(), self)?;
        if temporary.as_file().metadata()?.len() > MAX_BYTES as u64 {
            return Err(io::Error::other("Conversation exceeds the storage limit."));
        }
        temporary.as_file().sync_all()?;
        temporary.persist(path)?;
        Ok(())
    }

    pub(super) fn load(config: &Path, root: &Path) -> io::Result<Option<Self>> {
        let file = match fs::File::open(path(config, root)?) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let mut bytes = Vec::new();
        file.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > MAX_BYTES {
            return Err(io::Error::other("Conversation exceeds the storage limit."));
        }
        let mut value: Self = serde_json::from_slice(&bytes)?;
        if value.version != 1 || value.root != root {
            return Err(io::Error::other(
                "Conversation belongs to another workspace or version.",
            ));
        }
        value.bound();
        Ok(Some(value))
    }
}

fn path(config: &Path, root: &Path) -> io::Result<PathBuf> {
    // Stable FNV-1a filename; verify the full path on read to reject collisions.
    let hash = serde_json::to_vec(root)?
        .iter()
        .fold(0xcbf29ce484222325u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
    let parent = config
        .parent()
        .ok_or_else(|| io::Error::other("Missing settings directory."))?;
    Ok(parent
        .join("conversations")
        .join(format!("{hash:016x}.json")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histories_are_isolated_bounded_and_cleared_atomically() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let config = dir.path().join("editor.json");
        let a = Path::new("project-a");
        let b = Path::new("project-b");
        let chat = Conversation::new(
            a,
            Provider::Codex,
            Some("session-123".into()),
            "borrador ñ",
            (0..105).map(|i| ("You".into(), format!("{i} café"))),
            true,
        );
        chat.save(&config)?;
        let loaded = Conversation::load(&config, a)?.unwrap();
        assert_eq!(loaded.messages.len(), 100);
        assert_eq!(loaded.messages[0].1, "5 café");
        assert_eq!(loaded.draft, "borrador ñ");
        assert!(loaded.interrupted);
        assert!(Conversation::load(&config, b)?.is_none());
        let invalid = Conversation::new(
            a,
            Provider::Codex,
            Some("--help".into()),
            "",
            std::iter::empty(),
            false,
        );
        assert!(invalid.session.is_none());
        Conversation::new(a, Provider::Codex, None, "", std::iter::empty(), false).save(&config)?;
        assert!(Conversation::load(&config, a)?.unwrap().messages.is_empty());
        Ok(())
    }

    #[test]
    fn malformed_oversized_and_misrouted_history_is_rejected() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let config = dir.path().join("editor.json");
        let root = Path::new("project");
        let value = Conversation::new(root, Provider::Codex, None, "", std::iter::empty(), false);
        value.save(&config)?;
        let file = path(&config, root)?;
        fs::write(&file, b"{invalid")?;
        assert!(Conversation::load(&config, root).is_err());
        fs::write(&file, vec![b' '; MAX_BYTES + 1])?;
        assert!(Conversation::load(&config, root).is_err());
        value.save(&config)?;
        let other = path(&config, Path::new("other"))?;
        fs::copy(file, other)?;
        assert!(Conversation::load(&config, Path::new("other")).is_err());
        Ok(())
    }
}

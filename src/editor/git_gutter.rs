//! Git change markers for the editor gutter: lines added, modified or deleted relative to
//! the file's contents at `HEAD`.
//!
//! `HEAD` contents are read on a background thread (`git show HEAD:path`) and re-read
//! periodically so commits made elsewhere are picked up; the previous copy stays in use
//! while a refresh loads. The line diff runs on the UI thread, cached per buffer revision.
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use eframe::egui;

/// How often the `HEAD` copy of an open file is re-read.
const REFRESH: Duration = Duration::from_secs(8);
/// Above this many line pairs the exact alignment is skipped and the whole changed region
/// is marked as modified.
const MAX_DIFF_CELLS: usize = 4_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mark {
    Added,
    Modified,
    /// Lines were removed just above this line.
    Deleted,
}

#[derive(Default)]
struct Head {
    /// `None` until read, or when the file is not tracked at `HEAD`.
    contents: Option<String>,
    read: Option<Instant>,
    loading: Option<Receiver<Option<String>>>,
}

#[derive(Default)]
pub(super) struct GitGutter {
    heads: HashMap<PathBuf, Head>,
    /// Per file: (digest of buffer and head, marks per buffer line).
    cache: HashMap<PathBuf, (u64, Vec<Option<Mark>>)>,
}

impl GitGutter {
    /// Marks for each line of `text`, or `None` when the file is not tracked at `HEAD`.
    pub(super) fn marks(
        &mut self,
        ctx: &egui::Context,
        root: Option<&Path>,
        path: &Path,
        text: &str,
    ) -> Option<&[Option<Mark>]> {
        let root = root?;
        let head = self.heads.entry(path.to_path_buf()).or_default();
        head.poll(ctx, root, path);
        let contents = head.contents.as_deref()?;
        let key = digest(text, contents);
        if self.cache.get(path).is_none_or(|(k, _)| *k != key) {
            let marks = line_marks(contents, text);
            self.cache.insert(path.to_path_buf(), (key, marks));
        }
        self.cache.get(path).map(|(_, marks)| marks.as_slice())
    }
}

impl Head {
    fn poll(&mut self, ctx: &egui::Context, root: &Path, path: &Path) {
        if let Some(receiver) = &self.loading {
            match receiver.try_recv() {
                Ok(contents) => {
                    self.contents = contents;
                    self.loading = None;
                }
                Err(TryRecvError::Disconnected) => self.loading = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        let due = self.read.is_none_or(|read| read.elapsed() > REFRESH);
        if due && self.loading.is_none() {
            self.read = Some(Instant::now());
            self.loading = Some(spawn_read(
                ctx.clone(),
                root.to_path_buf(),
                path.to_path_buf(),
            ));
        }
    }
}

fn digest(text: &str, head: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    head.hash(&mut hasher);
    hasher.finish()
}

fn spawn_read(ctx: egui::Context, root: PathBuf, path: PathBuf) -> Receiver<Option<String>> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let contents = read_head(&root, &path);
        if sender.send(contents).is_ok() {
            ctx.request_repaint();
        }
    });
    receiver
}

fn read_head(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let spec = format!("HEAD:{}", relative.to_string_lossy().replace('\\', "/"));
    let mut command = Command::new("git");
    command
        .current_dir(root)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(["--no-pager", "show", "--no-textconv", &spec]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    Some(text.replace("\r\n", "\n"))
}

/// Per-line change marks of `new` against `old`.
pub(super) fn line_marks(old: &str, new: &str) -> Vec<Option<Mark>> {
    let old: Vec<&str> = old.split('\n').collect();
    let new: Vec<&str> = new.split('\n').collect();
    let mut marks = vec![None; new.len()];
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let old_mid = &old[prefix..old.len() - suffix];
    let new_mid = &new[prefix..new.len() - suffix];
    for hunk in hunks(old_mid, new_mid) {
        mark_hunk(
            &mut marks,
            prefix + hunk.new_start,
            hunk.removed,
            hunk.added,
        );
    }
    marks
}

struct Hunk {
    new_start: usize,
    removed: usize,
    added: usize,
}

/// Groups of consecutive changes between two line slices, aligned on their longest
/// common subsequence.
fn hunks(old: &[&str], new: &[&str]) -> Vec<Hunk> {
    if old.is_empty() && new.is_empty() {
        return Vec::new();
    }
    if old.is_empty() || new.is_empty() || old.len().saturating_mul(new.len()) > MAX_DIFF_CELLS {
        return vec![Hunk {
            new_start: 0,
            removed: old.len(),
            added: new.len(),
        }];
    }
    let width = new.len() + 1;
    let mut lcs = vec![0u32; (old.len() + 1) * width];
    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            lcs[i * width + j] = if old[i] == new[j] {
                lcs[(i + 1) * width + j + 1] + 1
            } else {
                lcs[(i + 1) * width + j].max(lcs[i * width + j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    let mut current: Option<Hunk> = None;
    while i < old.len() || j < new.len() {
        if i < old.len() && j < new.len() && old[i] == new[j] {
            out.extend(current.take());
            i += 1;
            j += 1;
            continue;
        }
        let hunk = current.get_or_insert(Hunk {
            new_start: j,
            removed: 0,
            added: 0,
        });
        let take_new =
            j < new.len() && (i == old.len() || lcs[i * width + j + 1] >= lcs[(i + 1) * width + j]);
        if take_new {
            hunk.added += 1;
            j += 1;
        } else {
            hunk.removed += 1;
            i += 1;
        }
    }
    out.extend(current);
    out
}

fn mark_hunk(marks: &mut [Option<Mark>], start: usize, removed: usize, added: usize) {
    if added == 0 {
        let at = start.min(marks.len().saturating_sub(1));
        if let Some(slot) = marks.get_mut(at) {
            slot.get_or_insert(Mark::Deleted);
        }
        return;
    }
    for (offset, slot) in marks.iter_mut().skip(start).take(added).enumerate() {
        *slot = Some(if offset < removed {
            Mark::Modified
        } else {
            Mark::Added
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_added_modified_and_deleted_lines() {
        let marks = line_marks("a\nb\nc\nd\ne", "a\nB\nc\nx\ny\ne");
        assert_eq!(
            marks,
            vec![
                None,
                Some(Mark::Modified),
                None,
                Some(Mark::Modified),
                Some(Mark::Added),
                None
            ]
        );
        assert_eq!(
            line_marks("a\nb\nc", "a\nc"),
            vec![None, Some(Mark::Deleted)]
        );
        assert!(
            line_marks("same\ntext", "same\ntext")
                .iter()
                .all(Option::is_none)
        );
    }
}

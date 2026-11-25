//! Inline ghost-text completion for the code editor.
//!
//! After a short typing pause the editor proposes the rest of the current line, taken from
//! the nearest line in the file that starts the same way, or the rest of the current word.
//! The proposal is drawn dimmed after the caret: `Tab` accepts all of it, `Ctrl+Right` or
//! `Alt+]` accepts the next word, and `Esc` or typing something different dismisses it.
//! It stays out of the way while the LSP completion popup is open.
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui::{self, Key, Modifiers, Ui, text::CCursor, text::CCursorRange};

use super::theme::colors;

/// Typing pause before a suggestion is computed.
const DEBOUNCE: Duration = Duration::from_millis(280);
/// Lines scanned around the caret when looking for a matching line.
const SCAN_LINES: usize = 4000;

#[derive(Default)]
pub(super) struct Ghost {
    path: Option<PathBuf>,
    /// Char index the suggestion continues from.
    at: usize,
    text: String,
    /// When the buffer last changed; suggestions wait for `DEBOUNCE` after it.
    edited: Option<Instant>,
    /// Key taken by `keys` this frame, applied by `after_edit`.
    pressed: GhostKey,
}

/// What the user asked the suggestion to do this frame.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum GhostKey {
    #[default]
    None,
    AcceptAll,
    AcceptWord,
    Dismiss,
}

impl Ghost {
    fn visible_for(&self, path: &Path, cursor: usize) -> bool {
        !self.text.is_empty() && self.path.as_deref() == Some(path) && self.at == cursor
    }

    pub(super) fn clear(&mut self) {
        self.text.clear();
        self.path = None;
    }

    /// Takes Tab, Ctrl+Right, Alt+] and Esc before the text editor sees them, but only
    /// while a suggestion is shown at the caret and the LSP popup is closed.
    pub(super) fn keys(
        &mut self,
        ui: &mut Ui,
        id: egui::Id,
        path: &Path,
        cursor: usize,
        popup_open: bool,
    ) {
        if popup_open || !ui.memory(|m| m.has_focus(id)) || !self.visible_for(path, cursor) {
            self.pressed = GhostKey::None;
            return;
        }
        self.pressed = ui.input_mut(|i| {
            if i.consume_key(Modifiers::NONE, Key::Tab) {
                GhostKey::AcceptAll
            } else if i.consume_key(Modifiers::COMMAND, Key::ArrowRight)
                || i.consume_key(Modifiers::ALT, Key::CloseBracket)
            {
                GhostKey::AcceptWord
            } else if i.consume_key(Modifiers::NONE, Key::Escape) {
                GhostKey::Dismiss
            } else {
                GhostKey::None
            }
        });
    }

    /// Updates, applies and paints the suggestion after the text edit ran this frame.
    /// Returns true when it changed the buffer.
    pub(super) fn after_edit(
        &mut self,
        ui: &mut Ui,
        text: &mut String,
        path: &Path,
        id: egui::Id,
        output: &egui::text_edit::TextEditOutput,
        popup_open: bool,
    ) -> bool {
        let key = std::mem::take(&mut self.pressed);
        let cursor = output.cursor_range.map_or(0, |range| range.primary.index);
        let has_selection = output
            .cursor_range
            .is_some_and(|range| range.primary.index != range.secondary.index);
        if output.response.changed() {
            self.follow_typing(text, path, cursor);
            self.edited = Some(Instant::now());
        }
        match key {
            GhostKey::Dismiss => {
                self.clear();
                return false;
            }
            GhostKey::AcceptAll | GhostKey::AcceptWord => {
                let take = if key == GhostKey::AcceptAll {
                    self.text.chars().count()
                } else {
                    next_word_len(&self.text)
                };
                let inserted: String = self.text.chars().take(take).collect();
                insert_at(text, cursor, &inserted);
                let caret = cursor + take;
                self.text = self.text.chars().skip(take).collect();
                self.at = caret;
                move_caret(ui, id, caret);
                return true;
            }
            GhostKey::None => {}
        }
        let focused = output.response.has_focus();
        if !focused || popup_open || has_selection || !at_line_end(text, cursor) {
            self.clear();
            return false;
        }
        if self.visible_for(path, cursor) {
            paint(ui, output, cursor, &self.text);
            return false;
        }
        if self.at != cursor || self.path.as_deref() != Some(path) {
            self.clear();
        }
        let Some(edited) = self.edited else {
            return false;
        };
        let waited = edited.elapsed();
        if waited < DEBOUNCE {
            ui.ctx().request_repaint_after(DEBOUNCE - waited);
            return false;
        }
        self.edited = None;
        if let Some(suggestion) = suggest(text, cursor) {
            self.path = Some(path.to_path_buf());
            self.at = cursor;
            self.text = suggestion;
            paint(ui, output, cursor, &self.text);
        }
        false
    }

    /// Keeps the suggestion while the user types exactly what it proposes.
    fn follow_typing(&mut self, text: &str, path: &Path, cursor: usize) {
        if self.text.is_empty() || self.path.as_deref() != Some(path) || cursor <= self.at {
            self.clear();
            return;
        }
        let typed: String = text.chars().skip(self.at).take(cursor - self.at).collect();
        match self.text.strip_prefix(typed.as_str()) {
            Some(rest) if !rest.is_empty() => {
                self.text = rest.to_owned();
                self.at = cursor;
            }
            _ => self.clear(),
        }
    }
}

fn paint(ui: &Ui, output: &egui::text_edit::TextEditOutput, cursor: usize, ghost: &str) {
    let caret = output
        .galley
        .pos_from_cursor(CCursor::new(cursor))
        .translate(output.galley_pos.to_vec2());
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    ui.painter().text(
        caret.left_top(),
        egui::Align2::LEFT_TOP,
        ghost,
        font,
        colors().muted.gamma_multiply(0.85),
    );
}

fn move_caret(ui: &Ui, id: egui::Id, caret: usize) {
    let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
    state
        .cursor
        .set_char_range(Some(CCursorRange::one(CCursor::new(caret))));
    state.store(ui.ctx(), id);
    ui.ctx().request_repaint();
}

fn byte_at(text: &str, chars: usize) -> usize {
    text.char_indices()
        .nth(chars)
        .map_or(text.len(), |(b, _)| b)
}

fn insert_at(text: &mut String, cursor: usize, inserted: &str) {
    let at = byte_at(text, cursor);
    text.insert_str(at, inserted);
}

/// Ghost text only appears where nothing follows the caret on its line.
fn at_line_end(text: &str, cursor: usize) -> bool {
    let rest = &text[byte_at(text, cursor)..];
    rest.split('\n')
        .next()
        .is_none_or(|tail| tail.trim().is_empty())
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// Length in chars of the next word to accept: leading spaces plus one word, or one run
/// of punctuation.
pub(super) fn next_word_len(text: &str) -> usize {
    let mut chars = text.chars().peekable();
    let mut n = 0;
    while chars.next_if(|c| c.is_whitespace()).is_some() {
        n += 1;
    }
    match chars.peek() {
        Some(&c) if is_word(c) => {
            while chars.next_if(|c| is_word(*c)).is_some() {
                n += 1;
            }
        }
        Some(_) => {
            while chars
                .next_if(|c| !is_word(*c) && !c.is_whitespace())
                .is_some()
            {
                n += 1;
            }
        }
        None => {}
    }
    n.max(1)
}

/// The rest of the current line from the nearest line that starts the same way, or else
/// the rest of the current word from an identifier used elsewhere in the file.
pub(super) fn suggest(text: &str, cursor: usize) -> Option<String> {
    let before = &text[..byte_at(text, cursor)];
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let typed = &before[line_start..];
    let prefix = typed.trim_start();
    if prefix.chars().count() >= 3
        && let Some(rest) = line_completion(text, line_start, prefix)
    {
        return Some(rest);
    }
    word_completion(text, typed)
}

fn line_completion(text: &str, line_start: usize, prefix: &str) -> Option<String> {
    let above = text[..line_start].lines().rev().take(SCAN_LINES);
    let below = text[line_start..].lines().skip(1).take(SCAN_LINES);
    above
        .chain(below)
        .map(str::trim_start)
        .find(|line| line.len() > prefix.len() && line.starts_with(prefix))
        .map(|line| line[prefix.len()..].trim_end().to_owned())
        .filter(|rest| !rest.is_empty())
}

fn word_completion(text: &str, typed: &str) -> Option<String> {
    let partial: String = typed
        .chars()
        .rev()
        .take_while(|c| is_word(*c))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if partial.chars().count() < 2 {
        return None;
    }
    text.split(|c: char| !is_word(c))
        .filter(|word| word.len() > partial.len() && word.starts_with(&partial))
        .max_by_key(|word| text.matches(*word).count().min(64) * 1000 - word.len().min(999))
        .map(|word| word[partial.len()..].to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggests_the_rest_of_a_similar_line_then_a_word() {
        let text = "let total = items.iter().sum();\nlet tot";
        assert_eq!(
            suggest(text, text.chars().count()),
            Some("al = items.iter().sum();".into())
        );
        let text = "fn render_frame() {}\nren";
        assert_eq!(
            suggest(text, text.chars().count()),
            Some("der_frame".into())
        );
        let text = "configuration\nconf";
        assert_eq!(
            suggest(text, text.chars().count()),
            Some("iguration".into())
        );
    }

    #[test]
    fn accepts_word_by_word_and_follows_typing() {
        assert_eq!(next_word_len(" items.iter()"), 6);
        assert_eq!(next_word_len(".iter()"), 1);
        let mut ghost = Ghost {
            path: Some(PathBuf::from("a.rs")),
            at: 3,
            text: "der()".into(),
            edited: None,
            pressed: GhostKey::None,
        };
        ghost.follow_typing("rende", Path::new("a.rs"), 5);
        assert_eq!(ghost.text, "r()");
        ghost.follow_typing("rendex", Path::new("a.rs"), 6);
        assert!(ghost.text.is_empty());
    }

    #[test]
    fn only_shows_at_the_end_of_a_line() {
        assert!(at_line_end("abc\ndef", 3));
        assert!(at_line_end("abc  ", 3));
        assert!(!at_line_end("abc)", 3));
    }
}

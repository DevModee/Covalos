//! Inline ghost-text completion for the code editor.
//!
//! After a short typing pause the editor proposes the rest of the current line, taken from
//! the nearest line in the file that starts the same way (with the block it opens when it
//! ends in a bracket), or the rest of the current word. It shows at the end of a line or
//! before closing punctuation such as an auto-closed `)`. The proposal is drawn dimmed after
//! the caret: `Tab` accepts all of it, `Ctrl+Right` or `Alt+]` accepts the next word, and
//! `Esc` or typing something different dismisses it. It stays out of the way while the
//! completion popup is open.
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui::{self, Key, Modifiers, Ui, text::CCursor, text::CCursorRange};

use super::theme::colors;

/// Typing pause before a suggestion is computed.
const DEBOUNCE: Duration = Duration::from_millis(160);
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
        // `consume_key` ignores extra Shift: check it so Shift+Tab still outdents and
        // Ctrl+Shift+Right still selects a word.
        self.pressed = ui.input_mut(|i| {
            let shift = i.modifiers.shift;
            if !shift && i.consume_key(Modifiers::NONE, Key::Tab) {
                GhostKey::AcceptAll
            } else if !shift
                && (i.consume_key(Modifiers::COMMAND, Key::ArrowRight)
                    || i.consume_key(Modifiers::ALT, Key::CloseBracket))
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
        let Some(tail) =
            closing_tail(text, cursor).filter(|_| focused && !popup_open && !has_selection)
        else {
            self.clear();
            return false;
        };
        let tail = tail.to_owned();
        if self.visible_for(path, cursor) {
            paint(ui, output, cursor, &self.text, &tail);
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
        // With auto-closed pairs after the caret only the current word is completed, so a
        // proposed line never duplicates the closers already there.
        if let Some(suggestion) = suggest(text, cursor, tail.is_empty()) {
            self.path = Some(path.to_path_buf());
            self.at = cursor;
            self.text = suggestion;
            paint(ui, output, cursor, &self.text, &tail);
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

/// Draws the suggestion dimmed at the caret. The rest of the line (closing punctuation)
/// is redrawn after it so the ghost reads as inserted text, and further lines of a block
/// are shown on a panel over the lines below.
fn paint(
    ui: &Ui,
    output: &egui::text_edit::TextEditOutput,
    cursor: usize,
    ghost: &str,
    tail: &str,
) {
    let caret = output
        .galley
        .pos_from_cursor(CCursor::new(cursor))
        .translate(output.galley_pos.to_vec2());
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let painter = ui.painter();
    let dim = colors().muted.gamma_multiply(0.9);
    let mut lines = ghost.split('\n');
    let first = lines.next().unwrap_or_default();
    let first = painter.layout_no_wrap(first.to_owned(), font.clone(), dim);
    if !tail.is_empty() {
        let tail = painter.layout_no_wrap(tail.to_owned(), font.clone(), colors().secondary);
        painter.rect_filled(
            egui::Rect::from_min_size(
                caret.left_top(),
                egui::vec2(first.size().x + tail.size().x + 2.0, caret.height()),
            ),
            0,
            colors().inset,
        );
        painter.galley(
            egui::pos2(caret.left() + first.size().x, caret.top()),
            tail,
            colors().secondary,
        );
    }
    painter.galley(caret.left_top(), first, dim);
    let rest: Vec<&str> = lines.collect();
    if rest.is_empty() {
        return;
    }
    let line_height = caret.height();
    let left = output.galley_pos.x;
    let block = egui::Rect::from_min_max(
        egui::pos2(left - 4.0, caret.bottom()),
        egui::pos2(
            ui.clip_rect().right() - 8.0,
            caret.bottom() + rest.len() as f32 * line_height + 2.0,
        ),
    );
    painter.rect_filled(block, 4, colors().inset);
    painter.rect_filled(
        egui::Rect::from_min_size(block.left_top(), egui::vec2(2.0, block.height())),
        1,
        colors().accent.gamma_multiply(0.5),
    );
    for (i, line) in rest.iter().enumerate() {
        let galley = painter.layout_no_wrap((*line).to_owned(), font.clone(), dim);
        painter.galley(
            egui::pos2(left, caret.bottom() + i as f32 * line_height),
            galley,
            dim,
        );
    }
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

/// What follows the caret on its line when it is only closing punctuation (often pairs
/// the editor closed itself), trimmed; `None` when real code follows.
fn closing_tail(text: &str, cursor: usize) -> Option<&str> {
    let rest = &text[byte_at(text, cursor)..];
    let line = rest.split('\n').next().unwrap_or_default();
    line.chars()
        .all(|c| c.is_whitespace() || matches!(c, ')' | ']' | '}' | '"' | '\'' | '`' | ';' | ','))
        .then(|| line.trim_end())
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

/// The rest of the current line from the nearest line that starts the same way (with the
/// block it opens, when it ends in a bracket), or else the rest of the current word from an
/// identifier used elsewhere in the file. `whole_lines` false limits it to the word.
pub(super) fn suggest(text: &str, cursor: usize, whole_lines: bool) -> Option<String> {
    let before = &text[..byte_at(text, cursor)];
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let typed = &before[line_start..];
    let prefix = typed.trim_start();
    let indent = &typed[..typed.len() - prefix.len()];
    if whole_lines
        && prefix.chars().count() >= 3
        && let Some(rest) = line_completion(text, line_start, prefix, indent)
    {
        return Some(rest);
    }
    word_completion(text, typed)
}

/// Lines a multi-line suggestion may add after the completed one.
const BLOCK_LINES: usize = 12;

fn line_completion(text: &str, line_start: usize, prefix: &str, indent: &str) -> Option<String> {
    let lines: Vec<&str> = text.split('\n').map(|l| l.trim_end_matches('\r')).collect();
    let current = text[..line_start].matches('\n').count();
    let above = (0..current).rev().take(SCAN_LINES);
    let below = (current + 1..lines.len()).take(SCAN_LINES);
    let (i, rest) = above.chain(below).find_map(|i| {
        let trimmed = lines[i].trim_start();
        let rest = trimmed.strip_prefix(prefix)?.trim_end();
        (!rest.is_empty()).then_some((i, rest))
    })?;
    let mut out = rest.to_owned();
    if rest.ends_with(['{', '(', '[']) {
        // Bring the block this line opens, re-indented to the caret's line, when it closes
        // within a few lines.
        let base = lines[i].len() - lines[i].trim_start().len();
        let mut block = String::new();
        for line in lines.iter().skip(i + 1).take(BLOCK_LINES) {
            block.push('\n');
            if line.trim().is_empty() {
                continue;
            }
            let own = line.len() - line.trim_start().len();
            block.push_str(indent);
            block.push_str(line[own.min(base)..].trim_end());
            if own <= base {
                out.push_str(&block);
                break;
            }
        }
    }
    Some(out)
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
            suggest(text, text.chars().count(), true),
            Some("al = items.iter().sum();".into())
        );
        let text = "fn render_frame() {}\nren";
        assert_eq!(
            suggest(text, text.chars().count(), true),
            Some("der_frame".into())
        );
        let text = "configuration\nconf";
        assert_eq!(
            suggest(text, text.chars().count(), true),
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
    fn shows_before_closers_but_not_before_code() {
        assert_eq!(closing_tail("abc\ndef", 3), Some(""));
        assert_eq!(closing_tail("f(ab)", 4), Some(")"));
        assert_eq!(closing_tail("f(ab);  ", 4), Some(");"));
        assert_eq!(closing_tail("abc + d", 3), None);
        let text = "call(conf)\nconfiguration";
        assert_eq!(suggest(text, 9, false), Some("iguration".into()));
    }

    #[test]
    fn suggests_the_block_a_matching_line_opens() {
        let text = "fn a() {\n    if ready {\n        go();\n    }\n}\nfn b() {\n  if re";
        assert_eq!(
            suggest(text, text.chars().count(), true),
            Some("ady {\n      go();\n  }".into())
        );
    }
}

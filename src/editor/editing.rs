//! Keystrokes the code editor handles itself because egui's `TextEdit` does not: auto-closing
//! pairs with overtype and wrapping, pair-aware Backspace, indenting Enter, block Tab and
//! Shift+Tab, line comments (`Ctrl+/`), moving and copying lines (`Alt+Up/Down`, Shift to
//! copy) and new lines below or above (`Ctrl+Enter`, `Ctrl+Shift+Enter`).
//!
//! Every operation is a function over the text and a selection in char indices, tested
//! without a UI. `keys` takes the matching input events before the text edit runs and leaves
//! every other event, in order, to it.
use eframe::egui::{
    self, Event, Key, Ui,
    text::{CCursor, CCursorRange},
};

/// Selection as (anchor, caret) char indices; equal when nothing is selected.
pub(super) type Sel = (usize, usize);

/// How a language comments and opens blocks, from the file extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Syntax {
    line_comment: Option<&'static str>,
    block_comment: Option<(&'static str, &'static str)>,
    /// `'` starts lifetimes or char literals rather than strings: never auto-closed.
    plain_apostrophe: bool,
    /// A trailing `:` opens an indented block (Python, YAML).
    colon_block: bool,
}

pub(super) fn syntax(ext: &str) -> Syntax {
    let ext = ext.to_ascii_lowercase();
    let line_comment = match ext.as_str() {
        "rs" | "ts" | "tsx" | "mts" | "cts" | "js" | "jsx" | "mjs" | "cjs" | "c" | "h" | "cc"
        | "cpp" | "cxx" | "hpp" | "hh" | "cs" | "java" | "kt" | "kts" | "go" | "swift" | "dart"
        | "scala" | "php" | "zig" | "jsonc" | "json5" | "glsl" | "wgsl" | "hlsl" | "scss"
        | "less" | "groovy" | "gradle" | "proto" | "fs" | "sol" => Some("//"),
        "py" | "pyw" | "sh" | "bash" | "zsh" | "fish" | "ps1" | "psm1" | "toml" | "yaml"
        | "yml" | "rb" | "r" | "pl" | "nix" | "cmake" | "mk" | "conf" | "env" | "properties"
        | "tf" | "ex" | "exs" | "jl" | "nim" | "cr" | "gitignore" => Some("#"),
        "sql" | "lua" | "hs" | "elm" => Some("--"),
        "ini" | "asm" | "clj" | "cljs" | "lisp" | "el" | "scm" => Some(";"),
        "tex" | "erl" => Some("%"),
        "bat" | "cmd" => Some("REM"),
        _ => None,
    };
    let block_comment = match ext.as_str() {
        "html" | "htm" | "xml" | "svg" | "md" | "markdown" | "vue" | "svelte" | "xaml" => {
            Some(("<!--", "-->"))
        }
        "css" => Some(("/*", "*/")),
        _ => None,
    };
    Syntax {
        line_comment,
        block_comment,
        plain_apostrophe: matches!(
            ext.as_str(),
            "rs" | "ml" | "mli" | "hs" | "elm" | "clj" | "lisp" | "scm" | "el"
        ),
        colon_block: matches!(ext.as_str(), "py" | "pyw" | "yaml" | "yml" | "nim"),
    }
}

/// The file's indentation unit: a tab when most indented lines use tabs, otherwise the
/// most common step between consecutive indentation levels, or the language's usual width.
pub(super) fn indent_unit(text: &str, ext: &str) -> String {
    let (mut tabs, mut spaced) = (0, 0);
    let mut steps = [0_usize; 9];
    let mut previous = 0;
    for line in text.lines().take(5000) {
        if line.trim().is_empty() {
            continue;
        }
        if line.starts_with('\t') {
            tabs += 1;
            continue;
        }
        let width = line.len() - line.trim_start_matches(' ').len();
        if width > 0 {
            spaced += 1;
        }
        if width > previous && (2..=8).contains(&(width - previous)) {
            steps[width - previous] += 1;
        }
        previous = width;
    }
    if tabs > spaced {
        return "\t".into();
    }
    let usual = match ext.to_ascii_lowercase().as_str() {
        "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "mts" | "cts" | "json" | "jsonc" | "yaml"
        | "yml" | "html" | "htm" | "css" | "scss" | "less" | "vue" | "svelte" | "dart" | "rb"
        | "lua" | "xml" | "svg" | "md" => 2,
        _ => 4,
    };
    let width = (2..=8)
        .max_by_key(|&w| (steps[w], w == usual))
        .filter(|&w| steps[w] > 0)
        .unwrap_or(usual);
    " ".repeat(width)
}

fn byte(text: &str, chars: usize) -> usize {
    text.char_indices()
        .nth(chars)
        .map_or(text.len(), |(b, _)| b)
}

fn char_at(text: &str, index: usize) -> Option<char> {
    text.chars().nth(index)
}

fn ordered((a, b): Sel) -> (usize, usize) {
    (a.min(b), a.max(b))
}

fn closing(open: char) -> Option<char> {
    Some(match open {
        '(' => ')',
        '[' => ']',
        '{' => '}',
        '"' => '"',
        '\'' => '\'',
        '`' => '`',
        _ => return None,
    })
}

fn is_quote(c: char) -> bool {
    matches!(c, '"' | '\'' | '`')
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Char index where the line holding `pos` starts.
fn line_start(text: &str, pos: usize) -> usize {
    let before = &text[..byte(text, pos)];
    before
        .rfind('\n')
        .map_or(0, |b| before[..=b].chars().count())
}

/// Whether `pos` sits inside a `quote` string opened earlier on its line.
fn inside_string(text: &str, pos: usize, quote: char) -> bool {
    let start = byte(text, line_start(text, pos));
    let mut open = false;
    let mut escaped = false;
    for c in text[start..byte(text, pos)].chars() {
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == quote {
            open = !open;
        }
    }
    open
}

/// A typed character: wraps a selection in a pair, steps over a closer that is already
/// there, or inserts the closer after an opener where that cannot break existing code.
pub(super) fn type_char(text: &mut String, sel: Sel, c: char, syn: Syntax) -> Option<Sel> {
    let (s, e) = ordered(sel);
    if s != e {
        let close = closing(c)?;
        let (bs, be) = (byte(text, s), byte(text, e));
        text.insert(be, close);
        text.insert(bs, c);
        return Some(if sel.0 <= sel.1 {
            (s + 1, e + 1)
        } else {
            (e + 1, s + 1)
        });
    }
    let next = char_at(text, s);
    let prev = s.checked_sub(1).and_then(|p| char_at(text, p));
    if matches!(c, ')' | ']' | '}') && next == Some(c)
        || is_quote(c) && next == Some(c) && inside_string(text, s, c)
    {
        return Some((s + 1, s + 1));
    }
    let close = closing(c)?;
    let room = next.is_none_or(|n| {
        n.is_whitespace() || matches!(n, ')' | ']' | '}' | ',' | ';' | ':' | '>' | '.')
    });
    if !room {
        return None;
    }
    if is_quote(c)
        && (prev.is_some_and(|p| is_word(p) || p == c || p == '\\')
            || (c == '\'' && syn.plain_apostrophe)
            || inside_string(text, s, c))
    {
        return None;
    }
    let at = byte(text, s);
    text.insert(at, close);
    text.insert(at, c);
    Some((s + 1, s + 1))
}

/// Backspace between an empty pair deletes both halves; inside space indentation it goes
/// back to the previous indent stop. Anything else is left to the text edit.
pub(super) fn backspace(text: &mut String, sel: Sel, unit: &str) -> Option<Sel> {
    let (s, e) = ordered(sel);
    if s != e || s == 0 {
        return None;
    }
    let prev = char_at(text, s - 1)?;
    if closing(prev).is_some() && char_at(text, s) == closing(prev) {
        let (a, b) = (byte(text, s - 1), byte(text, s + 1));
        text.replace_range(a..b, "");
        return Some((s - 1, s - 1));
    }
    if unit == "\t" {
        return None;
    }
    let start = line_start(text, s);
    let col = s - start;
    let lead = &text[byte(text, start)..byte(text, s)];
    if col < 2 || !lead.bytes().all(|b| b == b' ') {
        return None;
    }
    let remove = (col - 1) % unit.len() + 1;
    if remove < 2 {
        return None;
    }
    let (a, b) = (byte(text, s - remove), byte(text, s));
    text.replace_range(a..b, "");
    Some((s - remove, s - remove))
}

/// Enter: keeps the line's indentation, indents one more level after an opening bracket
/// (or `:` in Python and YAML), splits `{|}` over three lines with the caret in the middle
/// and continues `/** */` doc comments.
pub(super) fn newline(text: &mut String, sel: Sel, unit: &str, syn: Syntax) -> Sel {
    let (mut s, e) = ordered(sel);
    let (mut bs, be) = (byte(text, s), byte(text, e));
    text.replace_range(bs..be, "");
    let ls = text[..bs].rfind('\n').map_or(0, |b| b + 1);
    let indent: String = text[ls..bs]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    if text[ls..bs].trim().is_empty() && bs > ls {
        // A line holding only indentation does not keep trailing spaces.
        s -= text[ls..bs].chars().count();
        text.replace_range(ls..bs, "");
        bs = ls;
    }
    let before = text[ls..bs].trim_end();
    let last = before.chars().last();
    let opens = matches!(last, Some('{' | '[' | '(')) || (syn.colon_block && last == Some(':'));
    let lead = before.trim_start();
    let doc = syn.line_comment == Some("//")
        && (lead.starts_with("/**") || lead.starts_with('*') && !lead.starts_with("*/"))
        && !lead.ends_with("*/");
    let pair = last.and_then(closing).filter(|_| opens);
    let mut insert = format!("\n{indent}");
    let caret;
    if pair.is_some() && text[bs..].chars().next() == pair {
        insert.push_str(unit);
        caret = s + insert.chars().count();
        insert.push('\n');
        insert.push_str(&indent);
    } else {
        if opens {
            insert.push_str(unit);
        } else if doc {
            insert.push_str(if lead.starts_with("/**") { " * " } else { "* " });
        }
        caret = s + insert.chars().count();
    }
    text.insert_str(bs, &insert);
    (caret, caret)
}

/// Ctrl+Enter opens a line below the caret's line, Ctrl+Shift+Enter one above, both
/// indented like it.
pub(super) fn open_line(text: &mut String, sel: Sel, above: bool, unit: &str, syn: Syntax) -> Sel {
    let start = line_start(text, sel.1);
    let line_b = byte(text, start);
    let end_b = text[line_b..].find('\n').map_or(text.len(), |i| line_b + i);
    let line = &text[line_b..end_b];
    let indent: String = line
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    if above {
        text.insert_str(line_b, &format!("{indent}\n"));
        let at = start + indent.chars().count();
        return (at, at);
    }
    let last = line.trim_end().chars().last();
    let extra = matches!(last, Some('{' | '[' | '(')) || (syn.colon_block && last == Some(':'));
    let insert = format!("\n{indent}{}", if extra { unit } else { "" });
    let at = text[..end_b].chars().count() + insert.chars().count();
    text.insert_str(end_b, &insert);
    (at, at)
}

/// First and last line touched by a selection; a selection ending at column 0 does not
/// include that last line.
fn line_span(text: &str, s: usize, e: usize) -> (usize, usize) {
    let line = |p: usize| text[..byte(text, p)].matches('\n').count();
    let first = line(s);
    let mut last = line(e);
    if e > s && last > first && char_at(text, e - 1) == Some('\n') {
        last -= 1;
    }
    (first, last)
}

fn line_starts(lines: &[String]) -> Vec<usize> {
    let mut acc = 0;
    lines
        .iter()
        .map(|line| {
            let start = acc;
            acc += line.chars().count() + 1;
            start
        })
        .collect()
}

/// One rewritten line: its new text and where the change happened (column, char delta).
type LineChange = (String, usize, isize);

/// Rewrites lines `first..=last` and carries the selection through the change.
fn rewrite_lines(
    text: &mut String,
    sel: Sel,
    first: usize,
    last: usize,
    f: impl Fn(&str) -> LineChange,
) -> Sel {
    let mut lines: Vec<String> = text.split('\n').map(str::to_owned).collect();
    let old = line_starts(&lines);
    let mut changes = vec![(0_usize, 0_isize); lines.len()];
    for i in first..=last.min(lines.len() - 1) {
        let (new, at, delta) = f(&lines[i]);
        lines[i] = new;
        changes[i] = (at, delta);
    }
    let new = line_starts(&lines);
    let empty = sel.0 == sel.1;
    let map = |p: usize| {
        let l = old.partition_point(|&start| start <= p).saturating_sub(1);
        let col = p - old[l];
        let (at, delta) = changes[l];
        let col = if delta > 0 {
            if col > at || (col == at && (at > 0 || empty)) {
                col + delta.unsigned_abs()
            } else {
                col
            }
        } else if delta < 0 {
            let removed = delta.unsigned_abs();
            if col >= at + removed {
                col - removed
            } else {
                col.min(at)
            }
        } else {
            col
        };
        new[l] + col.min(lines[l].chars().count())
    };
    let mapped = (map(sel.0), map(sel.1));
    *text = lines.join("\n");
    mapped
}

/// Tab: with no selection, spaces to the next indent stop (or a tab); with a selection,
/// one more indent level on every line it touches.
pub(super) fn indent(text: &mut String, sel: Sel, unit: &str) -> Sel {
    let (s, e) = ordered(sel);
    if s == e {
        let col = s - line_start(text, s);
        let insert = if unit == "\t" {
            "\t".to_owned()
        } else {
            " ".repeat(unit.len() - col % unit.len())
        };
        text.insert_str(byte(text, s), &insert);
        let at = s + insert.chars().count();
        return (at, at);
    }
    let (first, last) = line_span(text, s, e);
    let width = unit.chars().count() as isize;
    rewrite_lines(text, sel, first, last, |line| {
        if line.trim().is_empty() {
            (line.to_owned(), 0, 0)
        } else {
            (format!("{unit}{line}"), 0, width)
        }
    })
}

/// Shift+Tab: one indent level less on every line the selection touches.
pub(super) fn outdent(text: &mut String, sel: Sel, unit: &str) -> Sel {
    let (s, e) = ordered(sel);
    let (first, last) = line_span(text, s, e);
    let width = unit.chars().count().max(1);
    rewrite_lines(text, sel, first, last, |line| {
        let n = if line.starts_with('\t') {
            1
        } else {
            line.chars().take(width).take_while(|c| *c == ' ').count()
        };
        (line[n..].to_owned(), 0, -(n as isize))
    })
}

/// Ctrl+/: comments every touched line at the block's indentation, or uncomments them when
/// all of them already are. Markup and CSS wrap each line in a block comment.
pub(super) fn toggle_comment(text: &mut String, sel: Sel, syn: Syntax) -> Option<Sel> {
    if syn.line_comment.is_none() && syn.block_comment.is_none() {
        return None;
    }
    let (s, e) = ordered(sel);
    let (first, last) = line_span(text, s, e);
    let lines: Vec<&str> = text.split('\n').collect();
    let block: Vec<&str> = lines[first..=last.min(lines.len() - 1)]
        .iter()
        .copied()
        .filter(|l| !l.trim().is_empty())
        .collect();
    if block.is_empty() {
        return Some(sel);
    }
    let col = block
        .iter()
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    if let Some(token) = syn.line_comment {
        let commented = block.iter().all(|l| l.trim_start().starts_with(token));
        return Some(rewrite_lines(text, sel, first, last, |line| {
            if line.trim().is_empty() {
                return (line.to_owned(), 0, 0);
            }
            if commented {
                let at = line.len() - line.trim_start().len();
                let mut n = token.len();
                if line[at + n..].starts_with(' ') {
                    n += 1;
                }
                (
                    format!("{}{}", &line[..at], &line[at + n..]),
                    at,
                    -(n as isize),
                )
            } else {
                (
                    format!("{}{token} {}", &line[..col], &line[col..]),
                    col,
                    token.len() as isize + 1,
                )
            }
        }));
    }
    let (open, close) = syn.block_comment?;
    let commented = block.iter().all(|l| {
        let t = l.trim();
        t.len() >= open.len() + close.len() && t.starts_with(open) && t.ends_with(close)
    });
    Some(rewrite_lines(text, sel, first, last, |line| {
        if line.trim().is_empty() {
            return (line.to_owned(), 0, 0);
        }
        let at = line.len() - line.trim_start().len();
        let trimmed = line.trim();
        if commented {
            let body = &trimmed[open.len()..trimmed.len() - close.len()];
            let inner = body.trim();
            // Positions move by the opening token; the closing one only shortens the line.
            let opening = open.len() + body.len() - body.trim_start().len();
            (format!("{}{inner}", &line[..at]), at, -(opening as isize))
        } else {
            (
                format!("{}{open} {trimmed} {close}", &line[..at]),
                at,
                open.len() as isize + 1,
            )
        }
    }))
}

/// Alt+Up/Down: swaps the touched lines with the line above or below.
pub(super) fn move_lines(text: &mut String, sel: Sel, up: bool) -> Sel {
    let (s, e) = ordered(sel);
    let (first, last) = line_span(text, s, e);
    let mut lines: Vec<String> = text.split('\n').map(str::to_owned).collect();
    let final_empty = lines.len() > 1 && lines.last().is_some_and(String::is_empty);
    let bottom = lines.len() - if final_empty { 2 } else { 1 };
    if (up && first == 0) || (!up && last >= bottom) {
        return sel;
    }
    let shift = if up {
        let moved = lines.remove(first - 1);
        let n = moved.chars().count() + 1;
        lines.insert(last, moved);
        -(n as isize)
    } else {
        let moved = lines.remove(last + 1);
        let n = moved.chars().count() + 1;
        lines.insert(first, moved);
        n as isize
    };
    *text = lines.join("\n");
    let apply = |p: usize| p.saturating_add_signed(shift);
    (apply(sel.0), apply(sel.1))
}

/// Shift+Alt+Up/Down: copies the touched lines above or below; the selection follows the
/// copy that moves.
pub(super) fn copy_lines(text: &mut String, sel: Sel, up: bool) -> Sel {
    let (s, e) = ordered(sel);
    let (first, last) = line_span(text, s, e);
    let mut lines: Vec<String> = text.split('\n').map(str::to_owned).collect();
    let block: Vec<String> = lines[first..=last].to_vec();
    let size: usize = block.iter().map(|l| l.chars().count() + 1).sum();
    for (i, line) in block.into_iter().enumerate() {
        lines.insert(last + 1 + i, line);
    }
    *text = lines.join("\n");
    if up {
        sel
    } else {
        (sel.0 + size, sel.1 + size)
    }
}

/// Applies one input event; `None` leaves it to the text edit.
fn apply(text: &mut String, sel: Sel, event: &Event, syn: Syntax, unit: &str) -> Option<Sel> {
    match event {
        Event::Text(typed) => {
            let mut chars = typed.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            type_char(text, sel, c, syn)
        }
        Event::Key {
            key,
            physical_key,
            pressed: true,
            modifiers: m,
            ..
        } => {
            let ctrl = m.command || m.ctrl;
            let plain = !ctrl && !m.alt;
            match key {
                Key::Enter if ctrl && !m.alt => Some(open_line(text, sel, m.shift, unit, syn)),
                Key::Enter if plain => Some(newline(text, sel, unit, syn)),
                Key::Backspace if plain && !m.shift => backspace(text, sel, unit),
                Key::Tab if plain && m.shift => Some(outdent(text, sel, unit)),
                Key::Tab if plain => Some(indent(text, sel, unit)),
                Key::ArrowUp | Key::ArrowDown if m.alt && !ctrl => {
                    let up = *key == Key::ArrowUp;
                    Some(if m.shift {
                        copy_lines(text, sel, up)
                    } else {
                        move_lines(text, sel, up)
                    })
                }
                // `/` is Shift+7 on Spanish and Latin American layouts.
                _ if ctrl
                    && !m.alt
                    && (*key == Key::Slash
                        || *physical_key == Some(Key::Slash)
                        || (*key == Key::Num7 && m.shift)) =>
                {
                    toggle_comment(text, sel, syn)
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Events after which the remaining ones must reach the text edit unchanged, in order.
fn edits_or_moves(event: &Event) -> bool {
    matches!(
        event,
        Event::Text(_) | Event::Paste(_) | Event::Cut | Event::Key { pressed: true, .. }
    )
}

/// Takes the code-editing keys for the focused editor `id` before its `TextEdit` runs and
/// applies them to `text`. Returns true when it handled a key (the text may have changed).
pub(super) fn keys(ui: &mut Ui, id: egui::Id, text: &mut String, ext: &str) -> bool {
    if !ui.memory(|m| m.has_focus(id)) {
        return false;
    }
    // A text edit that was focused without a click has no cursor yet; like egui, start at
    // the end of the text.
    let mut sel = egui::TextEdit::load_state(ui.ctx(), id)
        .and_then(|s| s.cursor.char_range())
        .map_or_else(
            || {
                let end = text.chars().count();
                (end, end)
            },
            |range| (range.secondary.index, range.primary.index),
        );
    let syn = syntax(ext);
    let mut handled = false;
    let mut unit: Option<String> = None;
    ui.input_mut(|input| {
        if input.events.iter().any(|e| matches!(e, Event::Ime(_))) {
            return;
        }
        let mut kept = Vec::with_capacity(input.events.len());
        let mut open = true;
        for event in std::mem::take(&mut input.events) {
            if open {
                let unit = unit.get_or_insert_with(|| indent_unit(text, ext)).clone();
                if let Some(next) = apply(text, sel, &event, syn, &unit) {
                    sel = next;
                    handled = true;
                    continue;
                }
                open = !edits_or_moves(&event);
            }
            kept.push(event);
        }
        input.events = kept;
    });
    if handled {
        let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
        state.cursor.set_char_range(Some(CCursorRange::two(
            CCursor::new(sel.0),
            CCursor::new(sel.1),
        )));
        state.store(ui.ctx(), id);
        ui.ctx().request_repaint();
    }
    handled
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Text with `|` marking the caret, or `«` and `»` marking anchor and caret.
    fn parse(marked: &str) -> (String, Sel) {
        let mut text = String::new();
        let (mut anchor, mut caret) = (None, None);
        for c in marked.chars() {
            match c {
                '|' | '»' => caret = Some(text.chars().count()),
                '«' => anchor = Some(text.chars().count()),
                _ => text.push(c),
            }
        }
        let caret = caret.expect("marked caret");
        (text, (anchor.unwrap_or(caret), caret))
    }

    fn show(text: &str, (a, c): Sel) -> String {
        let mut out = String::new();
        let chars: Vec<char> = text.chars().collect();
        for i in 0..=chars.len() {
            if a != c && i == a {
                out.push('«');
            }
            if i == c {
                out.push(if a == c { '|' } else { '»' });
            }
            if let Some(ch) = chars.get(i) {
                out.push(*ch);
            }
        }
        out
    }

    fn typed(marked: &str, c: char, ext: &str) -> String {
        let (mut text, sel) = parse(marked);
        match type_char(&mut text, sel, c, syntax(ext)) {
            Some(sel) => show(&text, sel),
            None => "unhandled".into(),
        }
    }

    #[test]
    fn pairs_close_overtype_and_wrap() {
        assert_eq!(typed("foo|", '(', "rs"), "foo(|)");
        assert_eq!(typed("foo(|)", ')', "rs"), "foo()|");
        assert_eq!(typed("let s = |;", '"', "ts"), "let s = \"|\";");
        assert_eq!(typed("\"abc|\"", '"', "ts"), "\"abc\"|");
        assert_eq!(typed("«name»", '"', "ts"), "\"«name»\"");
        assert_eq!(typed("«a + b»", '(', "rs"), "(«a + b»)");
        // Never right before a word, after a letter for quotes, or for Rust lifetimes.
        assert_eq!(typed("|foo", '(', "rs"), "unhandled");
        assert_eq!(typed("don|", '\'', "md"), "unhandled");
        assert_eq!(typed("&|", '\'', "rs"), "unhandled");
        assert_eq!(typed("x = |", '\'', "py"), "x = '|'");
    }

    #[test]
    fn backspace_removes_an_empty_pair_and_indent_stops() {
        let (mut text, sel) = parse("f(|)");
        let sel = backspace(&mut text, sel, "    ").unwrap();
        assert_eq!(show(&text, sel), "f|");
        let (mut text, sel) = parse("        |x");
        let sel = backspace(&mut text, sel, "    ").unwrap();
        assert_eq!(show(&text, sel), "    |x");
        let (mut text, sel) = parse("ab|c");
        assert_eq!(backspace(&mut text, sel, "    "), None);
    }

    #[test]
    fn enter_indents_and_splits_brackets() {
        let rs = syntax("rs");
        let (mut text, sel) = parse("fn main() {|}");
        let sel = newline(&mut text, sel, "    ", rs);
        assert_eq!(show(&text, sel), "fn main() {\n    |\n}");
        let (mut text, sel) = parse("    let a = [|");
        let sel = newline(&mut text, sel, "    ", rs);
        assert_eq!(show(&text, sel), "    let a = [\n        |");
        let (mut text, sel) = parse("def f():|");
        let sel = newline(&mut text, sel, "    ", syntax("py"));
        assert_eq!(show(&text, sel), "def f():\n    |");
        let (mut text, sel) = parse("  /**|");
        let sel = newline(&mut text, sel, "  ", syntax("ts"));
        assert_eq!(show(&text, sel), "  /**\n   * |");
        let (mut text, sel) = parse("    |");
        let sel = newline(&mut text, sel, "    ", rs);
        assert_eq!(show(&text, sel), "\n    |");
    }

    #[test]
    fn tab_and_shift_tab_work_on_whole_lines() {
        let (mut text, sel) = parse("«a\nb»");
        let sel = indent(&mut text, sel, "  ");
        assert_eq!(show(&text, sel), "«  a\n  b»");
        let sel = outdent(&mut text, sel, "  ");
        assert_eq!(show(&text, sel), "«a\nb»");
        let (mut text, sel) = parse("ab|");
        let sel = indent(&mut text, sel, "    ");
        assert_eq!(show(&text, sel), "ab  |");
    }

    #[test]
    fn comments_toggle_per_language() {
        let (mut text, sel) = parse("    «let a = 1;\n    let b = 2;»");
        let sel = toggle_comment(&mut text, sel, syntax("rs")).unwrap();
        assert_eq!(text, "    // let a = 1;\n    // let b = 2;");
        let sel = toggle_comment(&mut text, sel, syntax("rs")).unwrap();
        assert_eq!(show(&text, sel), "    «let a = 1;\n    let b = 2;»");
        let (mut text, sel) = parse("x = 1|");
        let sel = toggle_comment(&mut text, sel, syntax("py")).unwrap();
        assert_eq!(show(&text, sel), "# x = 1|");
        let (mut text, sel) = parse("<p>hi</p>|");
        let sel = toggle_comment(&mut text, sel, syntax("html")).unwrap();
        assert_eq!(text, "<!-- <p>hi</p> -->");
        let sel = toggle_comment(&mut text, sel, syntax("html")).unwrap();
        assert_eq!(show(&text, sel), "<p>hi</p>|");
        assert_eq!(
            toggle_comment(&mut "x".to_owned(), (0, 0), syntax("txt")),
            None
        );
    }

    #[test]
    fn lines_move_and_copy() {
        let (mut text, sel) = parse("a\nb|\nc");
        let sel = move_lines(&mut text, sel, true);
        assert_eq!(show(&text, sel), "b|\na\nc");
        let sel = move_lines(&mut text, sel, false);
        assert_eq!(show(&text, sel), "a\nb|\nc");
        let (mut text, sel) = parse("a\nb|\n");
        let sel = move_lines(&mut text, sel, false);
        assert_eq!(show(&text, sel), "a\nb|\n", "never past the final newline");
        let (mut text, sel) = parse("x|\ny");
        let sel = copy_lines(&mut text, sel, false);
        assert_eq!(show(&text, sel), "x\nx|\ny");
    }

    #[test]
    fn open_line_and_indent_unit() {
        let (mut text, sel) = parse("    if x {|\n    }");
        let sel = open_line(&mut text, sel, false, "    ", syntax("rs"));
        assert_eq!(show(&text, sel), "    if x {\n        |\n    }");
        let (mut text, sel) = parse("a\n  b|");
        let sel = open_line(&mut text, sel, true, "  ", syntax("rs"));
        assert_eq!(show(&text, sel), "a\n  |\n  b");
        assert_eq!(indent_unit("fn a() {\n  b\n    c\n}", "rs"), "  ");
        assert_eq!(indent_unit("a\n\tb\n\tc", "rs"), "\t");
        assert_eq!(indent_unit("", "ts"), "  ");
        assert_eq!(indent_unit("", "rs"), "    ");
    }
}

//! Programming ligatures for the code editor.
//!
//! epaint lays text out glyph by glyph through `ab_glyph`, which never applies OpenType
//! `liga`/`calt` substitutions, so a ligature font alone would render its sequences as
//! separate characters. Instead of reshaping lines (which would move every caret and
//! selection position), this layer keeps the monospace layout untouched: the characters of
//! a recognised sequence are laid out transparent and a single symbol is painted centred
//! over exactly the cells they occupy. Cursor hit-testing, selection and copy keep working
//! on the original characters, and a sequence the caret is inside shows its plain text.
use std::ops::Range;

use eframe::egui::{self, Color32, text::CCursor, text::LayoutJob};

/// Longest sequences first so `===` wins over `==`.
const SEQUENCES: &[(&str, char)] = &[
    ("!==", '≢'),
    ("===", '≡'),
    ("<=>", '⇔'),
    ("=>", '⇒'),
    ("->", '→'),
    ("<-", '←'),
    ("!=", '≠'),
    ("<=", '≤'),
    (">=", '≥'),
    ("::", '∷'),
];

/// Characters that extend an operator; a sequence touching one of them is part of a
/// longer operator (`-->`, `<==`, `:::`) and is left alone.
fn operator_char(c: char) -> bool {
    matches!(c, '=' | '<' | '>' | '!' | '-' | ':')
}

/// Files above this size skip ligatures, like they skip highlighting.
const MAX_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Ligature {
    /// Char range of the source sequence.
    pub(super) chars: Range<usize>,
    /// Byte range of the source sequence.
    pub(super) bytes: Range<usize>,
    pub(super) symbol: char,
}

/// Every ligature in `text`, except the one the caret sits inside.
pub(super) fn find(text: &str, caret: usize) -> Vec<Ligature> {
    if text.len() > MAX_BYTES {
        return Vec::new();
    }
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut found = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let matched = SEQUENCES.iter().find(|(seq, _)| {
            let len = seq.chars().count();
            i + len <= chars.len()
                && seq
                    .chars()
                    .zip(&chars[i..i + len])
                    .all(|(a, (_, b))| a == *b)
                && !(i > 0 && operator_char(chars[i - 1].1))
                && chars.get(i + len).is_none_or(|(_, c)| !operator_char(*c))
        });
        let Some((seq, symbol)) = matched else {
            i += 1;
            continue;
        };
        let len = seq.chars().count();
        let end_byte = chars.get(i + len).map_or(text.len(), |(b, _)| *b);
        if !(i < caret && caret < i + len) {
            found.push(Ligature {
                chars: i..i + len,
                bytes: chars[i].0..end_byte,
                symbol: *symbol,
            });
        }
        i += len;
    }
    found
}

/// Makes the source characters of each ligature transparent, keeping their metrics.
pub(super) fn hide(job: &mut LayoutJob, ligatures: &[Ligature]) {
    if ligatures.is_empty() {
        return;
    }
    let mut sections = Vec::with_capacity(job.sections.len() + ligatures.len() * 2);
    let mut next = ligatures.iter().peekable();
    for section in std::mem::take(&mut job.sections) {
        let mut start = section.byte_range.start;
        let end = section.byte_range.end;
        while let Some(lig) = next.peek() {
            if lig.bytes.end <= start {
                next.next();
                continue;
            }
            if lig.bytes.start >= end {
                break;
            }
            let hidden_start = lig.bytes.start.max(start);
            let hidden_end = lig.bytes.end.min(end);
            if hidden_start > start {
                sections.push(piece(&section, start..hidden_start, None));
            }
            sections.push(piece(
                &section,
                hidden_start..hidden_end,
                Some(Color32::TRANSPARENT),
            ));
            start = hidden_end;
            if lig.bytes.end <= end {
                next.next();
            } else {
                break;
            }
        }
        if start < end {
            sections.push(piece(&section, start..end, None));
        }
    }
    job.sections = sections;
}

/// A slice of `section`, optionally recoloured. Only the first slice keeps the leading
/// space so the split does not shift any glyph.
fn piece(
    section: &egui::text::LayoutSection,
    range: Range<usize>,
    color: Option<Color32>,
) -> egui::text::LayoutSection {
    let mut format = section.format.clone();
    if let Some(color) = color {
        format.color = color;
    }
    egui::text::LayoutSection {
        leading_space: if range.start == section.byte_range.start {
            section.leading_space
        } else {
            0.0
        },
        byte_range: range,
        format,
    }
}

/// Paints each visible ligature symbol centred over its hidden source cells.
pub(super) fn paint(
    painter: &egui::Painter,
    output: &egui::text_edit::TextEditOutput,
    ligatures: &[Ligature],
    font: &egui::FontId,
    color: Color32,
) {
    let clip = painter.clip_rect();
    let origin = output.galley_pos.to_vec2();
    for lig in ligatures {
        let start = output
            .galley
            .pos_from_cursor(CCursor::new(lig.chars.start))
            .translate(origin);
        if start.bottom() < clip.top() || start.top() > clip.bottom() {
            continue;
        }
        let end = output
            .galley
            .pos_from_cursor(CCursor::new(lig.chars.end))
            .translate(origin);
        if (end.top() - start.top()).abs() > 1.0 {
            continue;
        }
        let cells =
            egui::Rect::from_min_max(start.left_top(), egui::pos2(end.left(), start.bottom()));
        let galley = painter.layout_no_wrap(lig.symbol.to_string(), font.clone(), color);
        let scale = (cells.width() / galley.size().x.max(1.0)).min(1.0);
        let galley = if scale < 0.98 {
            painter.layout_no_wrap(
                lig.symbol.to_string(),
                egui::FontId::new(font.size * scale, font.family.clone()),
                color,
            )
        } else {
            galley
        };
        painter.galley(cells.center() - galley.size() / 2.0, galley, color);
    }
}

/// Drops ligatures whose symbol the loaded fonts cannot draw.
pub(super) fn drawable(ctx: &egui::Context, font: &egui::FontId, ligatures: &mut Vec<Ligature>) {
    ctx.fonts_mut(|fonts| ligatures.retain(|lig| fonts.has_glyph(font, lig.symbol)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_sequences_but_not_longer_operators_or_the_one_under_the_caret() {
        let text = "a => b != c === d -- e --> f :: g";
        let found: Vec<char> = find(text, 0).iter().map(|l| l.symbol).collect();
        assert_eq!(found, vec!['⇒', '≠', '≡', '∷']);
        let caret_inside_arrow = 3;
        assert!(
            find(text, caret_inside_arrow)
                .iter()
                .all(|l| l.symbol != '⇒')
        );
        let ligature = &find("x->y", 0)[0];
        assert_eq!(ligature.chars, 1..3);
        assert_eq!(ligature.bytes, 1..3);
    }

    #[test]
    fn hiding_splits_sections_without_moving_text() {
        let text = "a => b";
        let mut job = LayoutJob::simple(
            text.to_owned(),
            egui::FontId::monospace(12.0),
            Color32::WHITE,
            f32::INFINITY,
        );
        hide(&mut job, &find(text, 0));
        let ranges: Vec<_> = job.sections.iter().map(|s| s.byte_range.clone()).collect();
        assert_eq!(ranges, vec![0..2, 2..4, 4..6]);
        assert_eq!(job.sections[1].format.color, Color32::TRANSPARENT);
        assert_eq!(job.text, text);
    }
}

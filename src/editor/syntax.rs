//! AEROS syntax highlighting: syntect grammars, a semantic token classifier and curated
//! palettes that follow the interface theme.
//!
//! Grammars come from syntect's bundled Sublime syntaxes. Scopes are mapped onto a small,
//! stable set of semantic tokens, then a light lexical pass recovers calls, methods,
//! properties, types and constants that the grammars leave as plain identifiers. Each UI
//! theme owns a palette designed for hierarchy: control flow and declarations carry the
//! violet identity, types and functions are cool, literals are warm, comments recede.
use super::theme::Theme;
use eframe::egui::{
    self, Color32, FontId,
    text::{LayoutJob, TextFormat},
    util::cache::{ComputerMut, FrameCache},
};
use std::sync::OnceLock;
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Token {
    Text,
    Keyword,
    Declaration,
    Type,
    Function,
    Method,
    Macro,
    Variable,
    Parameter,
    Property,
    Constant,
    String,
    Escape,
    Number,
    Operator,
    Punctuation,
    Comment,
    Attribute,
    Namespace,
    Tag,
    Lifetime,
    Invalid,
}

#[derive(Clone, Copy)]
pub(super) struct Palette {
    pub(super) text: Color32,
    keyword: Color32,
    declaration: Color32,
    ty: Color32,
    function: Color32,
    method: Color32,
    macro_: Color32,
    parameter: Color32,
    property: Color32,
    constant: Color32,
    string: Color32,
    escape: Color32,
    number: Color32,
    operator: Color32,
    punctuation: Color32,
    comment: Color32,
    attribute: Color32,
    namespace: Color32,
    tag: Color32,
    lifetime: Color32,
    invalid: Color32,
    /// Editor decorations: active line, indent guides and bracket matches.
    pub(super) active_line: Color32,
    pub(super) guide: Color32,
    pub(super) guide_active: Color32,
    pub(super) bracket: Color32,
}

const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

pub(super) fn palette(theme: Theme) -> Palette {
    match theme {
        Theme::Light => Palette {
            text: hex(0x2B2F3A),
            keyword: hex(0x7A3FD1),
            declaration: hex(0x5B4BC4),
            ty: hex(0x0E7C76),
            function: hex(0x1F5FBF),
            method: hex(0x2E6FD0),
            macro_: hex(0xB4530B),
            parameter: hex(0x9A4D2E),
            property: hex(0x3B5B94),
            constant: hex(0x93600A),
            string: hex(0x2F7D32),
            escape: hex(0xB23A8A),
            number: hex(0xB0530F),
            operator: hex(0x0B7285),
            punctuation: hex(0x6B7280),
            comment: hex(0x878C99),
            attribute: hex(0xB0367A),
            namespace: hex(0x4C6A88),
            tag: hex(0xC2334D),
            lifetime: hex(0xC2334D),
            invalid: hex(0xD32F2F),
            active_line: Color32::from_rgba_unmultiplied(122, 63, 209, 14),
            guide: Color32::from_rgba_unmultiplied(40, 44, 60, 40),
            guide_active: Color32::from_rgba_unmultiplied(122, 63, 209, 110),
            bracket: hex(0x7A3FD1),
        },
        Theme::HighContrast => Palette {
            text: hex(0xFFFFFF),
            keyword: hex(0xE3B8FF),
            declaration: hex(0xC9B8FF),
            ty: hex(0x6FF2DD),
            function: hex(0x8EC5FF),
            method: hex(0xA6D2FF),
            macro_: hex(0xFFB070),
            parameter: hex(0xFFC8B0),
            property: hex(0xD8E2FF),
            constant: hex(0xFFD866),
            string: hex(0xA8F08C),
            escape: hex(0xFFB3D9),
            number: hex(0xFFC27A),
            operator: hex(0x7FE7FF),
            punctuation: hex(0xC8CCD8),
            comment: hex(0xA5ADC6),
            attribute: hex(0xFF9CD3),
            namespace: hex(0x9FD3E6),
            tag: hex(0xFF8FA8),
            lifetime: hex(0xFF8FA8),
            invalid: hex(0xFF6B6B),
            active_line: Color32::from_rgba_unmultiplied(160, 140, 255, 26),
            guide: Color32::from_rgba_unmultiplied(160, 170, 200, 70),
            guide_active: Color32::from_rgba_unmultiplied(227, 184, 255, 170),
            bracket: hex(0xE3B8FF),
        },
        // Dark, Midnight and Violet share one palette tuned for graphite canvases.
        _ => Palette {
            text: hex(0xD5DAE6),
            keyword: hex(0xC9A0FF),
            declaration: hex(0x9F8CFF),
            ty: hex(0x6FD6C5),
            function: hex(0x82AAFF),
            method: hex(0x93B7FF),
            macro_: hex(0xF4A261),
            parameter: hex(0xE8B4A0),
            property: hex(0xB8C4F0),
            constant: hex(0xF2C56B),
            string: hex(0xA3D98B),
            escape: hex(0xF0A6CA),
            number: hex(0xF5A97F),
            operator: hex(0x89DDFF),
            punctuation: hex(0x7F889E),
            comment: hex(0x6E7794),
            attribute: hex(0xE88FC7),
            namespace: hex(0x93A4C3),
            tag: hex(0xF38BA8),
            lifetime: hex(0xF38BA8),
            invalid: hex(0xFF6B81),
            active_line: Color32::from_rgba_unmultiplied(150, 130, 255, 12),
            guide: Color32::from_rgba_unmultiplied(140, 150, 180, 34),
            guide_active: Color32::from_rgba_unmultiplied(176, 154, 250, 120),
            bracket: hex(0xC9A0FF),
        },
    }
}

impl Palette {
    fn color(&self, token: Token) -> Color32 {
        match token {
            Token::Text | Token::Variable => self.text,
            Token::Keyword => self.keyword,
            Token::Declaration => self.declaration,
            Token::Type => self.ty,
            Token::Function => self.function,
            Token::Method => self.method,
            Token::Macro => self.macro_,
            Token::Parameter => self.parameter,
            Token::Property => self.property,
            Token::Constant => self.constant,
            Token::String => self.string,
            Token::Escape => self.escape,
            Token::Number => self.number,
            Token::Operator => self.operator,
            Token::Punctuation => self.punctuation,
            Token::Comment => self.comment,
            Token::Attribute => self.attribute,
            Token::Namespace => self.namespace,
            Token::Tag => self.tag,
            Token::Lifetime => self.lifetime,
            Token::Invalid => self.invalid,
        }
    }
}

fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// Grammar for a file extension. TypeScript reuses the JavaScript grammar, which covers
/// its expression syntax; TOML uses the lexical highlighter below.
fn syntax_for(extension: &str) -> Option<&'static SyntaxReference> {
    let ext = match extension {
        "ts" | "tsx" | "mts" | "cts" | "mjs" | "cjs" | "jsx" => "js",
        "jsonc" => "json",
        "csproj" | "fsproj" | "vbproj" | "props" | "targets" | "xaml" | "svg" => "xml",
        "kts" | "gradle" => "groovy",
        other => other,
    };
    syntaxes().find_syntax_by_extension(ext)
}

/// Scope rules, most specific first. A token takes the rule matched by its innermost scope.
const RULES: &[(&str, Token)] = &[
    ("invalid", Token::Invalid),
    ("comment", Token::Comment),
    ("punctuation.definition.comment", Token::Comment),
    ("constant.character.escape", Token::Escape),
    ("string", Token::String),
    ("constant.numeric", Token::Number),
    ("constant.language", Token::Constant),
    ("constant.other", Token::Constant),
    ("constant.character", Token::String),
    ("variable.other.constant", Token::Constant),
    ("support.constant", Token::Constant),
    ("storage.modifier.lifetime", Token::Lifetime),
    ("entity.name.lifetime", Token::Lifetime),
    ("support.macro", Token::Macro),
    ("entity.name.macro", Token::Macro),
    ("meta.annotation", Token::Attribute),
    ("meta.attribute", Token::Attribute),
    ("entity.name.function.decorator", Token::Attribute),
    ("punctuation.definition.annotation", Token::Attribute),
    ("punctuation.definition.attribute", Token::Attribute),
    ("entity.other.attribute-name", Token::Property),
    ("support.type.property-name", Token::Property),
    ("meta.mapping.key", Token::Property),
    ("entity.name.tag", Token::Tag),
    ("entity.name.function", Token::Function),
    ("support.function", Token::Function),
    ("variable.function", Token::Function),
    ("variable.parameter", Token::Parameter),
    ("variable.other.member", Token::Property),
    ("variable.other.property", Token::Property),
    ("variable.other.object.property", Token::Property),
    ("meta.property-name", Token::Property),
    ("entity.name.namespace", Token::Namespace),
    ("entity.name.module", Token::Namespace),
    ("support.other.namespace", Token::Namespace),
    ("entity.name.type", Token::Type),
    ("entity.name.class", Token::Type),
    ("entity.name.struct", Token::Type),
    ("entity.name.enum", Token::Type),
    ("entity.name.trait", Token::Type),
    ("entity.name.interface", Token::Type),
    ("entity.other.inherited-class", Token::Type),
    ("support.type", Token::Type),
    ("support.class", Token::Type),
    ("storage.type.primitive", Token::Type),
    ("storage.type.numeric", Token::Type),
    ("storage.type.string", Token::Type),
    ("storage.type", Token::Declaration),
    ("storage.modifier", Token::Keyword),
    ("keyword.operator", Token::Operator),
    ("keyword.declaration", Token::Declaration),
    ("keyword", Token::Keyword),
    ("punctuation.accessor", Token::Operator),
    ("punctuation.separator.namespace", Token::Operator),
    ("punctuation", Token::Punctuation),
    ("variable.language", Token::Keyword),
    ("variable", Token::Variable),
];

/// TypeScript/JavaScript words the bundled JavaScript grammar can leave unscoped.
const SCRIPT_DECLARATIONS: &[&str] = &[
    "class",
    "interface",
    "type",
    "enum",
    "function",
    "const",
    "let",
    "var",
    "namespace",
];
const SCRIPT_KEYWORDS: &[&str] = &[
    "export",
    "import",
    "from",
    "async",
    "await",
    "return",
    "extends",
    "implements",
    "private",
    "public",
    "protected",
    "readonly",
    "static",
    "abstract",
    "declare",
    "new",
    "this",
    "super",
    "throw",
    "default",
    "as",
    "of",
    "in",
    "instanceof",
    "typeof",
    "keyof",
    "satisfies",
    "get",
    "set",
];

/// Built-in type names that some grammars scope as `storage.type`.
const PRIMITIVES: &[&str] = &[
    "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize", "f32",
    "f64", "bool", "char", "str", "int", "float", "double", "long", "short", "byte", "void",
    "boolean", "string", "number", "any", "unknown", "never", "object", "uint", "decimal", "sbyte",
    "ushort", "ulong", "nint", "nuint", "rune",
];

fn classify(stack: &[Scope], cache: &mut Vec<(Scope, Token)>) -> Token {
    for scope in stack.iter().rev() {
        let token = if let Some((_, t)) = cache.iter().find(|(s, _)| s == scope) {
            *t
        } else {
            let name = scope.build_string();
            let token = RULES
                .iter()
                .find(|(prefix, _)| {
                    name == *prefix
                        || (name.starts_with(prefix)
                            && name.as_bytes().get(prefix.len()) == Some(&b'.'))
                })
                .map_or(Token::Text, |(_, t)| *t);
            cache.push((*scope, token));
            token
        };
        if token != Token::Text {
            return token;
        }
    }
    Token::Text
}

/// Refine plain identifiers that grammars leave unclassified: calls, methods, macros,
/// properties, constants, types and namespaces.
fn refine(spans: &mut Vec<(usize, usize, Token)>, line: &str) {
    let bytes = line.as_bytes();
    let mut out = Vec::with_capacity(spans.len() + 4);
    for &(start, end, token) in spans.iter() {
        if !matches!(token, Token::Text | Token::Variable) {
            out.push((start, end, token));
            continue;
        }
        let mut i = start;
        let mut last = start;
        while i < end {
            let c = bytes[i];
            if !(c.is_ascii_alphabetic() || c == b'_') {
                i += 1;
                continue;
            }
            let s = i;
            while i < end && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let word = &line[s..i];
            let mut j = i;
            while j < bytes.len() && bytes[j] == b' ' {
                j += 1;
            }
            let next = bytes.get(j).copied();
            let prev = line[..s].trim_end().as_bytes().last().copied();
            let refined = if next == Some(b'!') && bytes.get(j + 1) != Some(&b'=') && i == j {
                Token::Macro
            } else if next == Some(b'(') {
                if prev == Some(b'.') {
                    Token::Method
                } else {
                    Token::Function
                }
            } else if prev == Some(b'.') {
                Token::Property
            } else if word.len() > 1
                && word
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            {
                Token::Constant
            } else if word.as_bytes()[0].is_ascii_uppercase() {
                Token::Type
            } else if next == Some(b':') && bytes.get(j + 1) == Some(&b':') {
                Token::Namespace
            } else {
                token
            };
            if refined != token {
                if last < s {
                    out.push((last, s, token));
                }
                let end_word = if refined == Token::Macro { i + 1 } else { i };
                out.push((s, end_word.min(end), refined));
                last = end_word.min(end);
                i = last;
            }
        }
        if last < end {
            out.push((last, end, token));
        }
    }
    *spans = out;
}

/// Minimal TOML lexer: comments, tables, keys, strings, numbers and booleans.
fn toml_line(line: &str) -> Vec<(usize, usize, Token)> {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    if trimmed.starts_with('#') {
        return vec![(0, line.len(), Token::Comment)];
    }
    if trimmed.starts_with('[') {
        return vec![(0, line.len(), Token::Type)];
    }
    let mut spans = Vec::new();
    let value_start = match trimmed.find('=') {
        Some(eq) if !trimmed[..eq].contains('"') || trimmed.starts_with('"') => {
            spans.push((0, indent + eq, Token::Property));
            spans.push((indent + eq, indent + eq + 1, Token::Operator));
            indent + eq + 1
        }
        _ => 0,
    };
    let bytes = line.as_bytes();
    let mut i = value_start;
    let mut last = value_start;
    while i < line.len() {
        let c = bytes[i];
        let rest = &line[i..];
        let (end, token) = if c == b'"' || c == b'\'' {
            let mut j = i + 1;
            while j < line.len() && bytes[j] != c {
                j += if bytes[j] == b'\\' { 2 } else { 1 };
            }
            ((j + 1).min(line.len()), Token::String)
        } else if c == b'#' {
            (line.len(), Token::Comment)
        } else if c.is_ascii_digit()
            || (c == b'-' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit))
        {
            let mut j = i + 1;
            while j < line.len()
                && (bytes[j].is_ascii_alphanumeric() || b"._-:+".contains(&bytes[j]))
            {
                j += 1;
            }
            (j, Token::Number)
        } else if rest.starts_with("true") {
            (i + 4, Token::Constant)
        } else if rest.starts_with("false") {
            (i + 5, Token::Constant)
        } else if b"[]{},".contains(&c) {
            (i + 1, Token::Punctuation)
        } else {
            i += 1;
            continue;
        };
        if last < i {
            spans.push((last, i, Token::Text));
        }
        spans.push((i, end, token));
        i = end;
        last = end;
    }
    if last < line.len() {
        spans.push((last, line.len(), Token::Text));
    }
    spans
}

/// Tokenise one document into `(byte range, token)` spans.
pub(super) fn tokens(code: &str, language: &str) -> Vec<(usize, usize, Token)> {
    let mut all: Vec<(usize, usize, Token)> = Vec::new();
    let mut push = |s: usize, e: usize, t: Token| {
        if let Some(last) = all.last_mut()
            && last.2 == t
            && last.1 == s
        {
            last.1 = e;
        } else if e > s {
            all.push((s, e, t));
        }
    };
    let mut offset = 0;
    if language == "toml" || language == "lock" {
        for line in code.split_inclusive('\n') {
            let body = line.trim_end_matches(['\n', '\r']);
            for (s, e, t) in toml_line(body) {
                push(offset + s, offset + e, t);
            }
            push(offset + body.len(), offset + line.len(), Token::Text);
            offset += line.len();
        }
        return all;
    }
    let Some(syntax) = syntax_for(language) else {
        return vec![(0, code.len(), Token::Text)];
    };
    let mut state = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    let mut cache = Vec::new();
    for line in code.split_inclusive('\n') {
        let Ok(ops) = state.parse_line(line, syntaxes()) else {
            push(offset, offset + line.len(), Token::Text);
            offset += line.len();
            continue;
        };
        let mut spans = Vec::new();
        let mut cursor = 0;
        for (pos, op) in ops {
            if pos > cursor {
                spans.push((cursor, pos, classify(stack.as_slice(), &mut cache)));
                cursor = pos;
            }
            let _ = stack.apply(&op);
        }
        if cursor < line.len() {
            spans.push((cursor, line.len(), classify(stack.as_slice(), &mut cache)));
        }
        refine(&mut spans, line);
        let script = matches!(
            language,
            "js" | "mjs" | "cjs" | "jsx" | "ts" | "tsx" | "mts" | "cts"
        );
        for span in &mut spans {
            let word = &line[span.0..span.1];
            if span.2 == Token::Function && line[..span.0].trim_end().ends_with('.') {
                span.2 = Token::Method;
            } else if (span.2 == Token::Declaration && PRIMITIVES.contains(&word))
                || (span.2 == Token::Constant
                    && word.len() > 1
                    && word.as_bytes()[0].is_ascii_uppercase()
                    && word.bytes().any(|b| b.is_ascii_lowercase()))
            {
                // Primitive names scoped as storage, and PascalCase "constants", are types.
                span.2 = Token::Type;
            } else if script && matches!(span.2, Token::Text | Token::Variable) {
                if SCRIPT_DECLARATIONS.contains(&word) {
                    span.2 = Token::Declaration;
                } else if SCRIPT_KEYWORDS.contains(&word) {
                    span.2 = Token::Keyword;
                } else if PRIMITIVES.contains(&word) && line[..span.0].trim_end().ends_with(':') {
                    span.2 = Token::Type;
                }
            }
        }
        for (s, e, t) in spans {
            push(offset + s, offset + e, t);
        }
        offset += line.len();
    }
    all
}

fn layout(code: &str, language: &str, palette: Palette, size: f32, line_height: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = f32::INFINITY;
    let font = FontId::monospace(size);
    let mut covered = 0;
    for (start, end, token) in tokens(code, language) {
        if start > covered
            && let Some(gap) = code.get(covered..start)
        {
            job.append(gap, 0.0, format(&font, palette.text, line_height));
        }
        let Some(text) = code.get(start..end) else {
            continue;
        };
        let mut f = format(&font, palette.color(token), line_height);
        f.italics = token == Token::Comment;
        if token == Token::Invalid {
            f.underline = egui::Stroke::new(1.0_f32, palette.invalid);
        }
        job.append(text, 0.0, f);
        covered = end;
    }
    if covered < code.len() {
        job.append(
            &code[covered..],
            0.0,
            format(&font, palette.text, line_height),
        );
    }
    job
}

fn format(font: &FontId, color: Color32, line_height: f32) -> TextFormat {
    TextFormat {
        font_id: font.clone(),
        color,
        line_height: Some(line_height),
        ..Default::default()
    }
}

#[derive(Clone, Copy, Hash, PartialEq)]
struct Key<'a> {
    code: &'a str,
    language: &'a str,
    theme: u8,
    size: u32,
    line_height: u32,
}

#[derive(Default)]
struct Highlighter;

impl ComputerMut<&Key<'_>, LayoutJob> for Highlighter {
    fn compute(&mut self, key: &Key<'_>) -> LayoutJob {
        layout(
            key.code,
            key.language,
            palette(Theme::ALL[key.theme as usize]),
            f32::from_bits(key.size),
            f32::from_bits(key.line_height),
        )
    }
}

/// Highlighted, frame-cached layout job for the editor and for code blocks in chat.
pub(super) fn highlight(
    ctx: &egui::Context,
    code: &str,
    language: &str,
    size: f32,
    line_height: f32,
) -> LayoutJob {
    let theme = super::theme::active();
    let key = Key {
        code,
        language,
        theme: Theme::ALL.iter().position(|t| *t == theme).unwrap_or(0) as u8,
        size: size.to_bits(),
        line_height: line_height.to_bits(),
    };
    ctx.memory_mut(|m| {
        m.caches
            .cache::<FrameCache<LayoutJob, Highlighter>>()
            .get(&key)
    })
}

pub(super) fn current() -> Palette {
    palette(super::theme::active())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(code: &str, lang: &str) -> Vec<(String, Token)> {
        tokens(code, lang)
            .into_iter()
            .map(|(s, e, t)| (code[s..e].to_owned(), t))
            .collect()
    }
    fn has(spans: &[(String, Token)], text: &str, token: Token) -> bool {
        spans.iter().any(|(s, t)| s.trim() == text && *t == token)
    }

    #[test]
    fn rust_tokens_carry_semantic_hierarchy() {
        let k = kinds(
            "use std::path::PathBuf;\n/// Doc\n#[derive(Debug)]\nstruct Point { x: i32 }\nfn main() {\n    let p = Point::new(1, 2.5);\n    println!(\"hi {}\\n\", p.x);\n    p.len();\n    const MAX: u8 = 3;\n}\n",
            "rs",
        );
        assert!(
            has(&k, "fn", Token::Declaration) || has(&k, "fn", Token::Keyword),
            "{k:?}"
        );
        assert!(has(&k, "main", Token::Function), "{k:?}");
        assert!(has(&k, "Point", Token::Type), "{k:?}");
        assert!(
            k.iter()
                .any(|(s, t)| s.contains("println") && *t == Token::Macro),
            "{k:?}"
        );
        assert!(
            k.iter()
                .any(|(s, t)| s.contains("hi") && *t == Token::String)
        );
        assert!(has(&k, "\\n", Token::Escape), "{k:?}");
        assert!(has(&k, "2.5", Token::Number));
        assert!(
            k.iter()
                .any(|(s, t)| s.contains("Doc") && *t == Token::Comment)
        );
        assert!(has(&k, "len", Token::Method), "{k:?}");
        assert!(has(&k, "MAX", Token::Constant), "{k:?}");
        assert!(k.iter().any(|(_, t)| *t == Token::Attribute), "{k:?}");
        // Spans cover the source exactly once and in order.
        let spans = tokens("fn a() {}\n", "rs");
        assert!(spans.windows(2).all(|w| w[0].1 <= w[1].0));
    }

    #[test]
    fn python_typescript_and_toml_are_highlighted() {
        let py = kinds(
            "import os\ndef run(path):\n    return os.path.join(path, 'x')  # note\n",
            "py",
        );
        assert!(
            has(&py, "def", Token::Declaration) || has(&py, "def", Token::Keyword),
            "{py:?}"
        );
        assert!(has(&py, "run", Token::Function), "{py:?}");
        assert!(
            py.iter()
                .any(|(s, t)| s.contains("note") && *t == Token::Comment)
        );
        let ts = kinds("const total = items.map((i) => i.price).length;\n", "ts");
        let decl = kinds(
            "export class OrderStore {}\nexport async function load() {}\n",
            "ts",
        );
        assert!(has(&decl, "class", Token::Declaration), "{decl:?}");
        assert!(has(&decl, "export", Token::Keyword), "{decl:?}");
        assert!(has(&decl, "async", Token::Keyword), "{decl:?}");
        assert!(has(&decl, "OrderStore", Token::Type), "{decl:?}");
        assert!(
            has(&ts, "map", Token::Method) || has(&ts, "map", Token::Function),
            "{ts:?}"
        );
        let toml = kinds("[package]\nname = \"aeros\" # id\nlto = true\n", "toml");
        assert!(has(&toml, "[package]", Token::Type), "{toml:?}");
        assert!(has(&toml, "name", Token::Property), "{toml:?}");
        assert!(has(&toml, "\"aeros\"", Token::String), "{toml:?}");
        assert!(has(&toml, "true", Token::Constant), "{toml:?}");
    }

    #[test]
    fn palettes_keep_code_readable() {
        fn lum(c: Color32) -> f64 {
            let f = |v: u8| {
                let x = f64::from(v) / 255.0;
                if x <= 0.04045 {
                    x / 12.92
                } else {
                    ((x + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
        }
        for theme in Theme::ALL {
            let p = palette(theme);
            let bg = theme.colors().canvas;
            for c in [
                p.text,
                p.keyword,
                p.declaration,
                p.ty,
                p.function,
                p.method,
                p.string,
                p.number,
                p.constant,
                p.operator,
                p.macro_,
                p.property,
            ] {
                let (a, b) = (lum(c), lum(bg));
                let ratio = (a.max(b) + 0.05) / (a.min(b) + 0.05);
                assert!(ratio >= 4.5, "{}: {ratio:.2} for {c:?}", theme.name());
            }
        }
    }
}

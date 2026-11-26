//! Language Server Protocol client for the web stack: diagnostics and
//! completion from the standard servers when they are installed
//! (`typescript-language-server`, `vscode-css-language-server`,
//! `vscode-html-language-server`, `vscode-json-language-server`), looked up in
//! the project's `node_modules/.bin` first and then on PATH. Without a server,
//! completion falls back to words from the open file.
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

use eframe::egui::{self, Key, Modifiers, RichText, Ui, text::CCursor, text::CCursorRange};
use serde_json::{Value, json};

use super::theme::*;
use crate::workspace::Document;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(super) enum Lang {
    Ts,
    Css,
    Html,
    Json,
}

impl Lang {
    fn server(self) -> &'static str {
        match self {
            Self::Ts => "typescript-language-server",
            Self::Css => "vscode-css-language-server",
            Self::Html => "vscode-html-language-server",
            Self::Json => "vscode-json-language-server",
        }
    }

    pub(super) fn install_hint(self) -> &'static str {
        match self {
            Self::Ts => "npm install -g typescript-language-server typescript",
            _ => "npm install -g vscode-langservers-extracted",
        }
    }
}

fn language(path: &Path) -> Option<(Lang, &'static str)> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "ts" | "mts" | "cts" => (Lang::Ts, "typescript"),
        "tsx" => (Lang::Ts, "typescriptreact"),
        "js" | "mjs" | "cjs" => (Lang::Ts, "javascript"),
        "jsx" => (Lang::Ts, "javascriptreact"),
        "css" => (Lang::Css, "css"),
        "scss" => (Lang::Css, "scss"),
        "less" => (Lang::Css, "less"),
        "html" | "htm" => (Lang::Html, "html"),
        "json" | "jsonc" => (Lang::Json, "json"),
        _ => return None,
    })
}

/// `node_modules/.bin` of the project, then PATH. A test may point a server
/// at a Node script with `COVALOS_LSP_SCRIPT`.
fn server_command(lang: Lang, root: &Path) -> Option<Command> {
    if let Some(script) = std::env::var_os("COVALOS_LSP_SCRIPT") {
        let mut c = Command::new("node");
        c.arg(script);
        return Some(c);
    }
    let mut path_dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    if lang == Lang::Ts {
        // The project's TypeScript 7+, or with no TypeScript in the project a global one.
        let local = root.join("node_modules");
        let tsc = if local.join("typescript").is_dir() {
            native_typescript(&local, &local.join(".bin"))
        } else {
            path_dirs.iter().find_map(|bin| {
                global_modules(bin).and_then(|modules| native_typescript(&modules, bin))
            })
        };
        if let Some(tsc) = tsc {
            let mut c = Command::new(tsc);
            c.args(["--lsp", "--stdio"]);
            return Some(c);
        }
    }
    let name = lang.server();
    let names: Vec<String> = if cfg!(windows) {
        vec![format!("{name}.cmd"), format!("{name}.exe")]
    } else {
        vec![name.to_owned()]
    };
    let mut dirs = vec![root.join("node_modules").join(".bin")];
    dirs.append(&mut path_dirs);
    let program = dirs
        .iter()
        .flat_map(|d| names.iter().map(move |n| d.join(n)))
        .find(|p| p.is_file())?;
    let mut c = Command::new(program);
    c.arg("--stdio");
    Some(c)
}

/// The global `node_modules` that belongs to an npm bin folder (`%APPDATA%/npm` on
/// Windows, `<prefix>/bin` elsewhere).
fn global_modules(bin: &Path) -> Option<PathBuf> {
    [
        bin.join("node_modules"),
        bin.join("..").join("lib").join("node_modules"),
    ]
    .into_iter()
    .find(|m| m.join("typescript").join("package.json").is_file())
}

fn typescript_major(modules: &Path) -> Option<u32> {
    let manifest = modules.join("typescript").join("package.json");
    let json: Value = serde_json::from_slice(&std::fs::read(manifest).ok()?).ok()?;
    json["version"].as_str()?.split('.').next()?.parse().ok()
}

/// Options for `initialize`. typescript-language-server refuses to start without a
/// TypeScript in the workspace; outside an npm project it gets the one installed globally
/// next to it (`npm i -g typescript-language-server typescript`).
fn init_options(lang: Lang, root: &Path, command: &Command) -> Value {
    if lang != Lang::Ts || root.join("node_modules").join("typescript").is_dir() {
        return json!({});
    }
    Path::new(command.get_program())
        .parent()
        .and_then(global_modules)
        .map(|modules| modules.join("typescript").join("lib").join("tsserver.js"))
        .filter(|tsserver| tsserver.is_file())
        .map_or_else(
            || json!({}),
            |tsserver| json!({"tsserver": {"path": tsserver.to_string_lossy()}}),
        )
}

/// `tsc` from `bin` when the TypeScript in `modules` is 7 or newer (the native compiler,
/// which has no tsserver.js but serves LSP itself).
fn native_typescript(modules: &Path, bin: &Path) -> Option<PathBuf> {
    if typescript_major(modules)? < 7 {
        return None;
    }
    [bin.join("tsc.cmd"), bin.join("tsc.exe"), bin.join("tsc")]
        .into_iter()
        .find(|p| p.is_file())
}

/// Settings answered to `workspace/configuration`, per section.
fn settings(section: &str) -> Value {
    match section {
        "css" | "scss" | "less" => json!({"validate": true, "lint": {}}),
        "html" => json!({"validate": {"scripts": true, "styles": true}}),
        "json" => json!({"validate": {"enable": true}}),
        _ => json!({}),
    }
}

pub(super) fn uri(path: &Path) -> String {
    let path = crate::agents_runtime::plain(path);
    let text = path.to_string_lossy().replace('\\', "/");
    let mut out = String::from("file:///");
    for byte in text.trim_start_matches('/').bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                out.push(byte as char)
            }
            b':' => out.push_str("%3A"),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn path_from_uri(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file:///")?;
    let mut bytes = Vec::new();
    let raw = rest.as_bytes();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'%' && i + 2 < raw.len() {
            let hex = std::str::from_utf8(&raw[i + 1..i + 3]).ok()?;
            bytes.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            bytes.push(raw[i]);
            i += 1;
        }
    }
    let mut text = String::from_utf8(bytes).ok()?;
    // Servers may lower-case the drive letter (`file:///e%3A/...`).
    if cfg!(windows) && text.as_bytes().get(1) == Some(&b':') {
        text[..1].make_ascii_uppercase();
    }
    Some(PathBuf::from(if cfg!(windows) {
        text.replace('/', "\\")
    } else {
        format!("/{text}")
    }))
}

/// (line, UTF-16 column) of a char index.
pub(super) fn position(text: &str, index: usize) -> (u32, u32) {
    let (mut line, mut col) = (0_u32, 0_u32);
    for ch in text.chars().take(index) {
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += ch.len_utf16() as u32;
        }
    }
    (line, col)
}

/// Char index of an LSP position, clamped to the text.
pub(super) fn char_index(text: &str, (line, col): (u32, u32)) -> usize {
    let (mut l, mut c) = (0_u32, 0_u32);
    for (i, ch) in text.chars().enumerate() {
        if l == line && c >= col {
            return i;
        }
        if ch == '\n' {
            if l == line {
                return i;
            }
            l += 1;
            c = 0;
        } else if l == line {
            c += ch.len_utf16() as u32;
        }
    }
    text.chars().count()
}

fn parse_diagnostics(list: &Value) -> Vec<Diagnostic> {
    list.as_array()
        .into_iter()
        .flatten()
        .map(|d| Diagnostic {
            start: pos(&d["range"]["start"]),
            end: pos(&d["range"]["end"]),
            severity: d["severity"].as_u64().unwrap_or(1) as u8,
            message: d["message"].as_str().unwrap_or_default().to_owned(),
            source: d["source"].as_str().unwrap_or_default().to_owned(),
        })
        .collect()
}

fn pos(v: &Value) -> (u32, u32) {
    (
        v["line"].as_u64().unwrap_or(0) as u32,
        v["character"].as_u64().unwrap_or(0) as u32,
    )
}

/// LSP text edits: (start, end, new text), positions as (line, UTF-16 column).
type TextEdits = Vec<((u32, u32), (u32, u32), String)>;

#[derive(Clone, Debug)]
pub(super) struct Diagnostic {
    pub(super) start: (u32, u32),
    pub(super) end: (u32, u32),
    /// 1 error, 2 warning, 3 information, 4 hint.
    pub(super) severity: u8,
    pub(super) message: String,
    pub(super) source: String,
}

/// What a completion item is, shown as a short colored tag in the popup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Function,
    Variable,
    Field,
    Type,
    Module,
    Constant,
    Keyword,
    Snippet,
    Text,
}

impl Kind {
    fn from_lsp(kind: u64) -> Self {
        match kind {
            2..=4 => Self::Function,
            5 | 10 => Self::Field,
            6 => Self::Variable,
            7 | 8 | 13 | 22 | 25 => Self::Type,
            9 => Self::Module,
            12 | 20 | 21 => Self::Constant,
            14 => Self::Keyword,
            15 => Self::Snippet,
            _ => Self::Text,
        }
    }

    fn tag(self) -> &'static str {
        match self {
            Self::Function => "fn",
            Self::Variable => "var",
            Self::Field => "prop",
            Self::Type => "type",
            Self::Module => "mod",
            Self::Constant => "const",
            Self::Keyword => "kw",
            Self::Snippet => "snip",
            Self::Text => "abc",
        }
    }

    fn color(self) -> egui::Color32 {
        match self {
            Self::Function => colors().warning,
            Self::Variable | Self::Field => colors().accent,
            Self::Type | Self::Module => colors().syntax_string,
            Self::Constant => colors().syntax_number,
            Self::Keyword | Self::Snippet | Self::Text => colors().muted,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct Item {
    label: String,
    detail: String,
    insert: String,
    sort: String,
    kind: Kind,
    /// Start of the server's replace range, when it gave one.
    edit_start: Option<(u32, u32)>,
    /// The server's item, sent back to `completionItem/resolve` for its signature and docs.
    raw: Option<Value>,
}

enum Incoming {
    Message(Value),
    Closed,
}

struct Server {
    child: Child,
    out: Sender<Vec<u8>>,
    rx: Receiver<Incoming>,
    next: i64,
    ready: bool,
    queued: Vec<Value>,
    opened: HashMap<PathBuf, (i64, u64)>,
    triggers: Vec<char>,
    completion: Option<(i64, PathBuf)>,
    hover: Option<(i64, PathBuf, usize)>,
    format: Option<(i64, PathBuf)>,
    /// Pending `completionItem/resolve`: request id and the item's label.
    resolve: Option<(i64, String)>,
    /// The server answers `textDocument/diagnostic` (pull) requests.
    pull: bool,
    /// TypeScript 7's own `tsc --lsp` rather than typescript-language-server.
    native: bool,
    diagnostic_requests: HashMap<i64, PathBuf>,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn trace(direction: &str, message: &Value) {
    if std::env::var_os("COVALOS_LSP_TRACE").is_some() {
        let text = message.to_string();
        eprintln!(
            "lsp {direction} {}",
            text.chars().take(300).collect::<String>()
        );
    }
}

fn frame(message: &Value) -> Vec<u8> {
    trace("->", message);
    let body = message.to_string();
    let mut bytes = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    bytes.extend_from_slice(body.as_bytes());
    bytes
}

impl Server {
    fn start(
        mut command: Command,
        root: &Path,
        options: Value,
        wake: egui::Context,
    ) -> Result<Self, String> {
        let native = Path::new(command.get_program())
            .file_stem()
            .is_some_and(|stem| stem == "tsc");
        crate::process::configure(&mut command);
        let mut child = command
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        covalos_engine::process::contain(&child);
        let mut stdin = child.stdin.take().expect("piped");
        let stdout = child.stdout.take().expect("piped");
        let (out, outgoing) = mpsc::channel::<Vec<u8>>();
        thread::spawn(move || {
            for bytes in outgoing {
                if stdin
                    .write_all(&bytes)
                    .and_then(|()| stdin.flush())
                    .is_err()
                {
                    break;
                }
            }
        });
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut length = None;
                loop {
                    let mut line = String::new();
                    match reader.read_line(&mut line) {
                        Ok(0) | Err(_) => {
                            let _ = tx.send(Incoming::Closed);
                            return;
                        }
                        Ok(_) => {}
                    }
                    let line = line.trim();
                    if line.is_empty() {
                        break;
                    }
                    if let Some(n) = line.strip_prefix("Content-Length:") {
                        length = n.trim().parse::<usize>().ok();
                    }
                }
                let Some(length) = length.filter(|n| *n < 64 * 1024 * 1024) else {
                    continue;
                };
                let mut body = vec![0; length];
                if reader.read_exact(&mut body).is_err() {
                    let _ = tx.send(Incoming::Closed);
                    return;
                }
                if let Ok(value) = serde_json::from_slice::<Value>(&body) {
                    trace("<-", &value);
                    let _ = tx.send(Incoming::Message(value));
                    wake.request_repaint();
                }
            }
        });
        let server = Self {
            child,
            out,
            rx,
            next: 1,
            ready: false,
            queued: Vec::new(),
            opened: HashMap::new(),
            triggers: Vec::new(),
            completion: None,
            hover: None,
            format: None,
            resolve: None,
            pull: false,
            native,
            diagnostic_requests: HashMap::new(),
        };
        let root_uri = uri(root);
        let init = json!({
            "jsonrpc": "2.0", "id": 0, "method": "initialize",
            "params": {
                "processId": std::process::id(),
                "rootUri": root_uri,
                "workspaceFolders": [{"uri": root_uri, "name": "workspace"}],
                "capabilities": {
                    "textDocument": {
                        "synchronization": {"didSave": false},
                        "publishDiagnostics": {"relatedInformation": false},
                        "diagnostic": {"dynamicRegistration": false, "relatedDocumentSupport": false},
                        "completion": {"completionItem": {"snippetSupport": false, "labelDetailsSupport": true, "documentationFormat": ["plaintext", "markdown"], "resolveSupport": {"properties": ["detail", "documentation"]}}},
                        "hover": {"contentFormat": ["plaintext", "markdown"]},
                        "formatting": {"dynamicRegistration": false}
                    },
                    "workspace": {"configuration": true, "workspaceFolders": true}
                },
                "initializationOptions": options
            }
        });
        let _ = server.out.send(frame(&init));
        Ok(server)
    }

    fn send(&mut self, message: Value) {
        if self.ready {
            let _ = self.out.send(frame(&message));
        } else {
            self.queued.push(message);
        }
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.send(json!({"jsonrpc": "2.0", "method": method, "params": params}));
    }

    /// Asks a pull-diagnostics server for a document's current diagnostics.
    fn pull_diagnostics(&mut self, path: &Path) {
        if !(self.ready && self.pull) {
            return;
        }
        let id = self.next;
        self.next += 1;
        self.diagnostic_requests.insert(id, path.to_path_buf());
        self.send(json!({
            "jsonrpc": "2.0", "id": id, "method": "textDocument/diagnostic",
            "params": {"textDocument": {"uri": uri(path)}}
        }));
    }
}

enum Slot {
    Running(Box<Server>),
    Missing,
    Failed(String),
}

pub(super) struct Popup {
    path: PathBuf,
    /// Char index where the word being completed starts.
    anchor: usize,
    items: Vec<Item>,
    /// Index into the filtered list, and the first filtered row shown.
    selected: usize,
    scroll: usize,
    server: bool,
}

#[derive(Default)]
pub(super) struct Lsp {
    root: Option<PathBuf>,
    servers: HashMap<Lang, Slot>,
    pub(super) diagnostics: HashMap<PathBuf, Vec<Diagnostic>>,
    popup: Option<Popup>,
    synced: Option<Instant>,
    /// Hover answer: file, char index it describes, text.
    hover: Option<(PathBuf, usize, String)>,
    hover_asked: Option<(PathBuf, usize)>,
    /// Formatting edits waiting to be applied: file, (start, end, text).
    formatted: Option<(PathBuf, TextEdits)>,
    /// Signature and documentation of a resolved completion item: label, detail, docs.
    resolved: Option<(String, String, String)>,
    /// Where the completion popup was drawn last frame, so a click on it keeps it open.
    popup_rect: Option<egui::Rect>,
    pub(super) format_note: Option<String>,
}

fn digest(text: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

/// Whether `-` belongs to words in this file (CSS properties, HTML attributes).
fn dashed(path: &Path) -> bool {
    matches!(
        language(path),
        Some((Lang::Css | Lang::Html | Lang::Json, _))
    )
}

fn is_ident(c: char, dash: bool) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$' || (dash && c == '-')
}

fn word_start(text: &str, cursor: usize, dash: bool) -> usize {
    let chars: Vec<char> = text.chars().take(cursor).collect();
    let mut start = chars.len();
    while start > 0 && is_ident(chars[start - 1], dash) {
        start -= 1;
    }
    start
}

/// Keywords offered alongside the file's own symbols.
fn keywords(path: &Path) -> &'static [&'static str] {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match ext.as_str() {
        "rs" => &[
            "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
            "false", "for", "impl", "loop", "match", "move", "mut", "pub", "return", "self",
            "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "where",
            "while", "Option", "Result", "Some", "None", "Ok", "Err", "String", "Vec", "Box",
        ],
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "mts" | "cts" => &[
            "async",
            "await",
            "break",
            "case",
            "catch",
            "class",
            "const",
            "continue",
            "default",
            "export",
            "extends",
            "false",
            "finally",
            "function",
            "import",
            "interface",
            "let",
            "null",
            "return",
            "switch",
            "this",
            "throw",
            "true",
            "type",
            "typeof",
            "undefined",
            "while",
        ],
        "py" | "pyw" => &[
            "async", "await", "break", "class", "continue", "def", "elif", "else", "except",
            "False", "finally", "import", "lambda", "None", "pass", "raise", "return", "self",
            "True", "while", "with", "yield",
        ],
        "go" => &[
            "break",
            "chan",
            "const",
            "continue",
            "defer",
            "func",
            "import",
            "interface",
            "map",
            "package",
            "range",
            "return",
            "select",
            "struct",
            "switch",
            "type",
            "var",
        ],
        _ => &[],
    }
}

/// The kind a buffer identifier most likely has, from the word before it and the character
/// after it.
fn infer_kind(word: &str, before: &str, member: bool, called: bool) -> Kind {
    match before {
        "fn" | "function" | "def" | "func" | "fun" => return Kind::Function,
        "struct" | "enum" | "class" | "interface" | "type" | "trait" | "union" => {
            return Kind::Type;
        }
        "mod" | "namespace" | "package" => return Kind::Module,
        _ => {}
    }
    let upper = word.chars().any(char::is_alphabetic)
        && word.chars().all(|c| !c.is_alphabetic() || c.is_uppercase());
    if called {
        Kind::Function
    } else if member {
        Kind::Field
    } else if upper && word.chars().count() > 1 {
        Kind::Constant
    } else if word.starts_with(char::is_uppercase) {
        Kind::Type
    } else if matches!(before, "let" | "const" | "var" | "mut" | "static") {
        Kind::Variable
    } else {
        Kind::Text
    }
}

/// Symbols from the open file with an inferred kind, most used first. After `.` or `::`
/// (`member`) only names that followed one of those in the file are offered; otherwise the
/// language's keywords join them.
fn words(text: &str, path: &Path, member: bool) -> Vec<Item> {
    let dash = dashed(path);
    let mut found: HashMap<&str, (Kind, usize, bool)> = HashMap::new();
    let mut previous_word = "";
    let mut start = None;

    for (i, c) in text
        .char_indices()
        .chain(std::iter::once((text.len(), ' ')))
    {
        if is_ident(c, dash) && i < text.len() {
            start.get_or_insert(i);
            continue;
        }
        let Some(s) = start.take() else { continue };
        let word = &text[s..i];
        let lead = text[..s].trim_end_matches([' ', '\t']).as_bytes();
        let after_member = lead.ends_with(b".") || lead.ends_with(b"::");
        let called = text[i..].trim_start_matches([' ', '\t']).starts_with('(');
        if word.chars().count() >= 2 && !word.starts_with(|c: char| c.is_ascii_digit()) {
            let kind = infer_kind(word, previous_word, after_member, called);
            let entry = found.entry(word).or_insert((kind, 0, false));
            entry.1 += 1;
            entry.2 |= after_member;
            if entry.0 == Kind::Text || kind != Kind::Text && entry.0 == Kind::Field {
                entry.0 = kind;
            }
        }
        previous_word = word;
    }
    let mut items: Vec<(usize, Item)> = found
        .into_iter()
        .filter(|(_, (_, _, after_member))| !member || *after_member)
        .map(|(word, (kind, uses, _))| {
            (
                uses,
                Item {
                    label: word.to_owned(),
                    detail: String::new(),
                    insert: word.to_owned(),
                    sort: word.to_lowercase(),
                    kind,
                    edit_start: None,
                    raw: None,
                },
            )
        })
        .collect();
    if !member {
        for keyword in keywords(path) {
            if !items.iter().any(|(_, item)| item.label == *keyword) {
                items.push((
                    0,
                    Item {
                        label: (*keyword).to_owned(),
                        detail: String::new(),
                        insert: (*keyword).to_owned(),
                        sort: keyword.to_lowercase(),
                        kind: Kind::Keyword,
                        edit_start: None,
                        raw: None,
                    },
                ));
            }
        }
    }
    items.sort_by(|(a, x), (b, y)| b.cmp(a).then_with(|| x.sort.cmp(&y.sort)));
    items.into_iter().take(400).map(|(_, item)| item).collect()
}

fn completion_items(result: &Value) -> Vec<Item> {
    let list = result
        .get("items")
        .and_then(Value::as_array)
        .or_else(|| result.as_array());
    let mut items: Vec<Item> = list
        .into_iter()
        .flatten()
        .take(20_000)
        .map(|item| {
            let label = item["label"].as_str().unwrap_or_default().to_owned();
            let insert = item["textEdit"]["newText"]
                .as_str()
                .or_else(|| item["insertText"].as_str())
                .unwrap_or(&label)
                .to_owned();
            let range = if item["textEdit"]["range"].is_object() {
                &item["textEdit"]["range"]
            } else {
                &item["textEdit"]["replace"]
            };
            let detail = [
                &item["detail"],
                &item["labelDetails"]["detail"],
                &item["labelDetails"]["description"],
            ]
            .into_iter()
            .find_map(|v| v.as_str().filter(|s| !s.trim().is_empty()))
            .unwrap_or_default()
            .trim()
            .to_owned();
            Item {
                detail,
                sort: item["sortText"].as_str().unwrap_or(&label).to_owned(),
                kind: Kind::from_lsp(item["kind"].as_u64().unwrap_or(1)),
                edit_start: range.is_object().then(|| pos(&range["start"])),
                raw: Some(item.clone()),
                insert,
                label,
            }
        })
        .collect();
    items.sort_by(|a, b| a.sort.cmp(&b.sort));
    items
}

/// How well `label` matches the typed `prefix`: 0 exact-case prefix, 1 any-case prefix,
/// 2 the prefix's letters in order (`gtUsr` for `getUser`), `None` no match.
fn rank(label: &str, prefix: &str) -> Option<u8> {
    if prefix.is_empty() || label.starts_with(prefix) {
        return Some(0);
    }
    let lower = label.to_lowercase();
    let wanted = prefix.to_lowercase();
    if lower.starts_with(&wanted) {
        return Some(1);
    }
    let mut chars = lower.chars();
    let first = wanted.chars().next()?;
    (lower.starts_with(first) && wanted.chars().all(|w| chars.any(|c| c == w))).then_some(2)
}

impl Lsp {
    fn reset_root(&mut self, root: Option<&Path>) {
        if self.root.as_deref() != root {
            self.servers.clear();
            self.diagnostics.clear();
            self.popup = None;
            self.root = root.map(Path::to_path_buf);
        }
    }

    /// Opens, updates and closes documents on their servers (throttled).
    pub(super) fn sync(&mut self, root: Option<&Path>, docs: &[Document], ctx: &egui::Context) {
        self.reset_root(root);
        self.poll();
        let Some(root) = self.root.clone() else {
            return;
        };
        if self
            .synced
            .is_some_and(|t| t.elapsed() < Duration::from_millis(300))
        {
            return;
        }
        self.synced = Some(Instant::now());
        for doc in docs {
            let Some((lang, id)) = language(&doc.path) else {
                continue;
            };
            let slot =
                self.servers
                    .entry(lang)
                    .or_insert_with(|| match server_command(lang, &root) {
                        None => Slot::Missing,
                        Some(command) => {
                            let options = init_options(lang, &root, &command);
                            match Server::start(command, &root, options, ctx.clone()) {
                                Ok(server) => Slot::Running(Box::new(server)),
                                Err(e) => Slot::Failed(e),
                            }
                        }
                    });
            let Slot::Running(server) = slot else {
                continue;
            };
            let hash = digest(&doc.text);
            match server.opened.get(&doc.path).copied() {
                None => {
                    server.opened.insert(doc.path.clone(), (1, hash));
                    server.notify(
                        "textDocument/didOpen",
                        json!({"textDocument": {"uri": uri(&doc.path), "languageId": id, "version": 1, "text": doc.text}}),
                    );
                    server.pull_diagnostics(&doc.path);
                }
                Some((version, old)) if old != hash => {
                    server.opened.insert(doc.path.clone(), (version + 1, hash));
                    server.notify(
                        "textDocument/didChange",
                        json!({"textDocument": {"uri": uri(&doc.path), "version": version + 1}, "contentChanges": [{"text": doc.text}]}),
                    );
                    server.pull_diagnostics(&doc.path);
                }
                Some(_) => {}
            }
        }
        for slot in self.servers.values_mut() {
            let Slot::Running(server) = slot else {
                continue;
            };
            let closed: Vec<PathBuf> = server
                .opened
                .keys()
                .filter(|p| !docs.iter().any(|d| &d.path == *p))
                .cloned()
                .collect();
            for path in closed {
                server.opened.remove(&path);
                server.notify(
                    "textDocument/didClose",
                    json!({"textDocument": {"uri": uri(&path)}}),
                );
                self.diagnostics
                    .remove(&crate::agents_runtime::plain(&path));
            }
        }
    }

    fn poll(&mut self) {
        let mut diagnostics = Vec::new();
        let mut completions = Vec::new();
        let mut hovers = Vec::new();
        let mut formats = Vec::new();
        let mut resolved = Vec::new();
        for (lang, slot) in &mut self.servers {
            let Slot::Running(server) = slot else {
                continue;
            };
            let mut closed = false;
            let mut failed: Option<String> = None;
            let messages: Vec<Incoming> = server.rx.try_iter().collect();
            for message in messages {
                let Incoming::Message(msg) = message else {
                    closed = true;
                    continue;
                };
                let method = msg["method"].as_str().unwrap_or_default();
                if msg["id"] == 0 && method.is_empty() && msg.get("error").is_some() {
                    let reason = msg["error"]["message"]
                        .as_str()
                        .unwrap_or("initialize failed");
                    failed = Some(reason.chars().take(300).collect::<String>());
                } else if msg["id"] == 0 && method.is_empty() {
                    server.ready = true;
                    server.pull = msg["result"]["capabilities"]
                        .get("diagnosticProvider")
                        .is_some();
                    server.triggers =
                        msg["result"]["capabilities"]["completionProvider"]["triggerCharacters"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|c| c.as_str()?.chars().next())
                            .collect();
                    let _ = server.out.send(frame(
                        &json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}),
                    ));
                    for queued in std::mem::take(&mut server.queued) {
                        let _ = server.out.send(frame(&queued));
                    }
                    let opened: Vec<PathBuf> = server.opened.keys().cloned().collect();
                    for path in opened {
                        server.pull_diagnostics(&path);
                    }
                } else if let Some(path) = msg["id"]
                    .as_i64()
                    .filter(|_| method.is_empty())
                    .and_then(|id| server.diagnostic_requests.remove(&id))
                {
                    if msg["result"]["kind"] != "unchanged" {
                        let list = parse_diagnostics(&msg["result"]["items"]);
                        diagnostics.push((path, list));
                    }
                } else if method == "textDocument/publishDiagnostics" {
                    let params = &msg["params"];
                    if let Some(path) = params["uri"].as_str().and_then(path_from_uri) {
                        diagnostics.push((path, parse_diagnostics(&params["diagnostics"])));
                    }
                } else if !method.is_empty() && msg.get("id").is_some() {
                    // Server-to-client requests must be answered or some servers stall.
                    let result = if method == "workspace/configuration" {
                        Value::Array(
                            msg["params"]["items"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .map(|item| settings(item["section"].as_str().unwrap_or_default()))
                                .collect(),
                        )
                    } else {
                        Value::Null
                    };
                    let _ = server.out.send(frame(
                        &json!({"jsonrpc": "2.0", "id": msg["id"], "result": result}),
                    ));
                } else if let Some((id, path)) = &server.completion
                    && msg["id"].as_i64() == Some(*id)
                {
                    completions.push((path.clone(), completion_items(&msg["result"])));
                    server.completion = None;
                } else if let Some((id, label)) = &server.resolve
                    && msg["id"].as_i64() == Some(*id)
                {
                    let result = &msg["result"];
                    let docs = &result["documentation"];
                    let docs = docs
                        .as_str()
                        .or_else(|| docs["value"].as_str())
                        .unwrap_or_default();
                    resolved.push((
                        label.clone(),
                        result["detail"]
                            .as_str()
                            .unwrap_or_default()
                            .trim()
                            .to_owned(),
                        hover_text(&Value::String(docs.to_owned())),
                    ));
                    server.resolve = None;
                } else if let Some((id, path, index)) = &server.hover
                    && msg["id"].as_i64() == Some(*id)
                {
                    hovers.push((path.clone(), *index, hover_text(&msg["result"]["contents"])));
                    server.hover = None;
                } else if let Some((id, path)) = &server.format
                    && msg["id"].as_i64() == Some(*id)
                {
                    let edits = msg["result"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|e| {
                            (
                                pos(&e["range"]["start"]),
                                pos(&e["range"]["end"]),
                                e["newText"].as_str().unwrap_or_default().to_owned(),
                            )
                        })
                        .collect::<Vec<_>>();
                    formats.push((path.clone(), edits, msg.get("error").is_some()));
                    server.format = None;
                }
            }
            if let Some(reason) = failed {
                *slot = Slot::Failed(reason);
            } else if closed {
                *slot = Slot::Failed(format!("{} stopped", lang.server()));
            }
        }
        for (path, list) in diagnostics {
            // Keyed by the plain path: editor paths may carry the verbatim prefix.
            self.diagnostics
                .insert(crate::agents_runtime::plain(&path), list);
        }
        for (path, index, text) in hovers {
            self.hover = (!text.trim().is_empty()).then_some((path, index, text));
        }
        for (path, edits, failed) in formats {
            if failed {
                self.format_note = Some("The language server could not format this file.".into());
            } else if edits.is_empty() {
                self.format_note = Some("Already formatted.".into());
            } else {
                self.formatted = Some((path, edits));
            }
        }
        for (path, items) in completions {
            if let Some(popup) = &mut self.popup
                && popup.path == path
                && popup.server
            {
                popup.items = items;
                popup.selected = 0;
                popup.scroll = 0;
            }
        }
        if let Some(last) = resolved.pop() {
            self.resolved = Some(last);
        }
    }

    /// Server state for the status line and the Problems view.
    pub(super) fn state(&self, path: &Path) -> Option<(Lang, &'static str)> {
        let (lang, _) = language(path)?;
        Some((
            lang,
            match self.servers.get(&lang) {
                Some(Slot::Running(s)) if s.ready => "ready",
                Some(Slot::Running(_)) => "starting",
                Some(Slot::Failed(_)) => "stopped",
                Some(Slot::Missing) => "missing",
                None => "idle",
            },
        ))
    }

    /// Whether TypeScript files are served by TypeScript 7's native `tsc --lsp`.
    pub(super) fn native_typescript(&self) -> bool {
        matches!(self.servers.get(&Lang::Ts), Some(Slot::Running(s)) if s.native)
    }

    fn failure(&self, lang: Lang) -> Option<&str> {
        match self.servers.get(&lang) {
            Some(Slot::Failed(reason)) => Some(reason),
            _ => None,
        }
    }

    pub(super) fn counts(&self) -> (usize, usize) {
        let all = self.diagnostics.values().flatten();
        let errors = all.clone().filter(|d| d.severity == 1).count();
        (errors, all.filter(|d| d.severity == 2).count())
    }

    /// Opens the popup and asks the server; without one, the file's symbols fill it. After
    /// `.` or `::` (`member`) the buffer offers only names used after those.
    fn trigger(&mut self, doc: &Document, cursor: usize, member: bool) {
        let anchor = word_start(&doc.text, cursor, dashed(&doc.path));
        let mut server_asked = false;
        if let Some((lang, _)) = language(&doc.path)
            && let Some(Slot::Running(server)) = self.servers.get_mut(&lang)
            && server.ready
            && server.opened.contains_key(&doc.path)
        {
            let hash = digest(&doc.text);
            if let Some((version, old)) = server.opened.get(&doc.path).copied()
                && old != hash
            {
                server.opened.insert(doc.path.clone(), (version + 1, hash));
                server.notify(
                    "textDocument/didChange",
                    json!({"textDocument": {"uri": uri(&doc.path), "version": version + 1}, "contentChanges": [{"text": doc.text}]}),
                );
            }
            let id = server.next;
            server.next += 1;
            let (line, character) = position(&doc.text, cursor);
            server.send(json!({
                "jsonrpc": "2.0", "id": id, "method": "textDocument/completion",
                "params": {"textDocument": {"uri": uri(&doc.path)}, "position": {"line": line, "character": character}}
            }));
            server.completion = Some((id, doc.path.clone()));
            server_asked = true;
        }
        self.resolved = None;
        self.popup = Some(Popup {
            path: doc.path.clone(),
            anchor,
            items: if server_asked {
                Vec::new()
            } else {
                words(&doc.text, &doc.path, member)
            },
            selected: 0,
            scroll: 0,
            server: server_asked,
        });
    }

    /// Asks the server for the signature and docs of the highlighted item, once per item.
    fn request_resolve(&mut self, path: &Path, item: &Item) {
        let Some(raw) = item.raw.clone() else { return };
        if self
            .resolved
            .as_ref()
            .is_some_and(|(label, _, _)| label == &item.label)
        {
            return;
        }
        let label = item.label.clone();
        let Some(server) = self.server_for(path) else {
            return;
        };
        if server.resolve.as_ref().is_some_and(|(_, l)| l == &label) {
            return;
        }
        let id = server.next;
        server.next += 1;
        server.send(
            json!({"jsonrpc": "2.0", "id": id, "method": "completionItem/resolve", "params": raw}),
        );
        server.resolve = Some((id, label));
    }

    /// Whether a ready language server answers completion for `path`.
    fn has_server(&self, path: &Path) -> bool {
        language(path).is_some_and(|(lang, _)| {
            matches!(self.servers.get(&lang), Some(Slot::Running(s)) if s.ready && s.opened.contains_key(path))
        })
    }

    /// Whether the completion popup is showing items for `path`.
    pub(super) fn popup_open(&self, path: &Path) -> bool {
        self.popup
            .as_ref()
            .is_some_and(|p| p.path == path && !p.items.is_empty())
    }

    /// The ready server for a document, if any.
    fn server_for(&mut self, path: &Path) -> Option<&mut Server> {
        let (lang, _) = language(path)?;
        match self.servers.get_mut(&lang) {
            Some(Slot::Running(server)) if server.ready && server.opened.contains_key(path) => {
                Some(server)
            }
            _ => None,
        }
    }

    fn request_hover(&mut self, doc: &Document, index: usize) {
        if self.hover_asked == Some((doc.path.clone(), index)) {
            return;
        }
        self.hover_asked = Some((doc.path.clone(), index));
        self.hover = None;
        let (line, character) = position(&doc.text, index);
        let Some(server) = self.server_for(&doc.path) else {
            return;
        };
        let id = server.next;
        server.next += 1;
        server.send(json!({
            "jsonrpc": "2.0", "id": id, "method": "textDocument/hover",
            "params": {"textDocument": {"uri": uri(&doc.path)}, "position": {"line": line, "character": character}}
        }));
        server.hover = Some((id, doc.path.clone(), index));
    }

    fn request_format(&mut self, doc: &Document) {
        let Some(server) = self.server_for(&doc.path) else {
            self.format_note = Some("No language server can format this file.".into());
            return;
        };
        let id = server.next;
        server.next += 1;
        server.send(json!({
            "jsonrpc": "2.0", "id": id, "method": "textDocument/formatting",
            "params": {"textDocument": {"uri": uri(&doc.path)}, "options": {"tabSize": 2, "insertSpaces": true}}
        }));
        server.format = Some((id, doc.path.clone()));
    }

    fn triggers(&self, path: &Path) -> Vec<char> {
        language(path)
            .and_then(|(lang, _)| match self.servers.get(&lang) {
                Some(Slot::Running(s)) => Some(s.triggers.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }
}

/// Keys the completion popup takes before the text editor sees them.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum PopupKey {
    None,
    Up,
    Down,
    Accept,
    Close,
    PageUp,
    PageDown,
    Trigger,
    Format,
}

pub(super) fn popup_keys(ui: &mut Ui, lsp: &Lsp, id: egui::Id, path: &Path) -> PopupKey {
    if !ui.memory(|m| m.has_focus(id)) {
        return PopupKey::None;
    }
    if ui.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Space)) {
        return PopupKey::Trigger;
    }
    if ui.input_mut(|i| i.consume_key(Modifiers::SHIFT | Modifiers::ALT, Key::F)) {
        return PopupKey::Format;
    }
    let open = lsp
        .popup
        .as_ref()
        .is_some_and(|p| p.path == path && !p.items.is_empty());
    if !open {
        return PopupKey::None;
    }
    ui.input_mut(|i| {
        if i.consume_key(Modifiers::NONE, Key::ArrowDown) {
            PopupKey::Down
        } else if i.consume_key(Modifiers::NONE, Key::ArrowUp) {
            PopupKey::Up
        } else if i.consume_key(Modifiers::NONE, Key::PageDown) {
            PopupKey::PageDown
        } else if i.consume_key(Modifiers::NONE, Key::PageUp) {
            PopupKey::PageUp
        } else if i.consume_key(Modifiers::NONE, Key::Enter)
            || i.consume_key(Modifiers::NONE, Key::Tab)
        {
            PopupKey::Accept
        } else if i.consume_key(Modifiers::NONE, Key::Escape) {
            PopupKey::Close
        } else {
            PopupKey::None
        }
    })
}

/// Rows the completion popup shows at once.
const POPUP_ROWS: usize = 10;
const POPUP_ROW: f32 = 22.0;
const POPUP_WIDTH: f32 = 380.0;

/// Completion, triggers and diagnostics for the code editor, after the text
/// edit was laid out this frame.
pub(super) fn after_edit(
    ui: &mut Ui,
    lsp: &mut Lsp,
    doc: &mut Document,
    id: egui::Id,
    output: &egui::text_edit::TextEditOutput,
    key: PopupKey,
) {
    let cursor = output.cursor_range.map_or(0, |range| range.primary.index);
    if key == PopupKey::Trigger {
        let member = matches!(
            doc.text.chars().nth(cursor.saturating_sub(1)),
            Some('.' | ':')
        );
        lsp.trigger(doc, cursor, member);
    }
    if output.response.changed() {
        let before: Vec<char> = doc.text.chars().take(cursor).collect();
        let typed = before.last().copied();
        let member = typed == Some('.')
            || (typed == Some(':') && before.len() >= 2 && before[before.len() - 2] == ':');
        let dash = dashed(&doc.path);
        let server = lsp.has_server(&doc.path);
        let word = cursor - word_start(&doc.text, cursor, dash);
        if let Some(c) = typed
            && (member
                || lsp.triggers(&doc.path).contains(&c)
                || (is_ident(c, dash) && lsp.popup.is_none() && word >= if server { 1 } else { 2 }))
        {
            lsp.trigger(doc, cursor, member);
        }
    }
    if key == PopupKey::Format {
        lsp.request_format(doc);
    }
    if let Some((path, edits)) = lsp.formatted.take() {
        if path == doc.path {
            apply_edits(doc, edits);
            ui.ctx().request_repaint();
        } else {
            lsp.formatted = Some((path, edits));
        }
    }
    let on_diagnostic = paint_diagnostics(ui, lsp, doc, output);
    hover(ui, lsp, doc, output, on_diagnostic);
    let pointer_on_popup = ui
        .ctx()
        .pointer_interact_pos()
        .zip(lsp.popup_rect)
        .is_some_and(|(p, r)| r.contains(p));
    lsp.popup_rect = None;
    let Some(popup) = &mut lsp.popup else { return };
    if popup.path != doc.path {
        return;
    }
    let prefix: String = if cursor >= popup.anchor {
        doc.text
            .chars()
            .skip(popup.anchor)
            .take(cursor - popup.anchor)
            .collect()
    } else {
        String::new()
    };
    let dash = dashed(&doc.path);
    if cursor < popup.anchor
        || key == PopupKey::Close
        || (!output.response.has_focus() && !pointer_on_popup)
        || prefix.chars().any(|c| !is_ident(c, dash))
    {
        lsp.popup = None;
        return;
    }
    let mut visible: Vec<(u8, usize)> = popup
        .items
        .iter()
        .enumerate()
        .filter_map(|(i, item)| rank(&item.label, &prefix).map(|r| (r, i)))
        .collect();
    visible.sort_by_key(|&(r, i)| (r, i));
    visible.truncate(300);
    // Nothing to offer once the word is typed out in full.
    let done = visible.len() == 1 && popup.items[visible[0].1].label == prefix;
    if visible.is_empty() || done {
        if !popup.server || !popup.items.is_empty() {
            lsp.popup = None;
        }
        return;
    }
    let last = visible.len() - 1;
    popup.selected = match key {
        PopupKey::Down if popup.selected >= last => 0,
        PopupKey::Down => popup.selected + 1,
        PopupKey::Up if popup.selected == 0 => last,
        PopupKey::Up => popup.selected - 1,
        PopupKey::PageDown => (popup.selected + POPUP_ROWS).min(last),
        PopupKey::PageUp => popup.selected.saturating_sub(POPUP_ROWS),
        _ => popup.selected.min(last),
    };
    if popup.selected < popup.scroll {
        popup.scroll = popup.selected;
    } else if popup.selected >= popup.scroll + POPUP_ROWS {
        popup.scroll = popup.selected + 1 - POPUP_ROWS;
    }
    popup.scroll = popup.scroll.min(visible.len().saturating_sub(POPUP_ROWS));
    let chosen = popup.items[visible[popup.selected].1].clone();
    let mut accept = (key == PopupKey::Accept).then_some(visible[popup.selected].1);
    let rows = visible.len().min(POPUP_ROWS);
    let (detail, docs) = match &lsp.resolved {
        Some((label, detail, docs)) if label == &chosen.label => (
            if detail.is_empty() {
                chosen.detail.clone()
            } else {
                detail.clone()
            },
            docs.clone(),
        ),
        _ => (chosen.detail.clone(), String::new()),
    };
    let footer = !detail.is_empty() || !docs.is_empty();
    let height = rows as f32 * POPUP_ROW + 10.0 + if footer { 76.0 } else { 0.0 };
    let caret = output
        .galley
        .pos_from_cursor(CCursor::new(cursor))
        .translate(output.galley_pos.to_vec2());
    let screen = ui.ctx().content_rect();
    let x = caret
        .left()
        .min(screen.right() - POPUP_WIDTH - 8.0)
        .max(screen.left() + 4.0);
    // Below the caret line, or above it when the popup would run off the window.
    let y = if caret.bottom() + 2.0 + height > screen.bottom() - 8.0 {
        caret.top() - 2.0 - height
    } else {
        caret.bottom() + 2.0
    };
    let font = egui::FontId::monospace(12.5);
    let tag_font = egui::FontId::monospace(10.0);
    let small = egui::FontId::proportional(SMALL);
    let area = egui::Area::new(id.with("completion"))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(x, y))
        .show(ui.ctx(), |ui| {
            egui::Frame::new()
                .fill(colors().elevated)
                .stroke(egui::Stroke::new(1.0_f32, colors().control_border))
                .corner_radius(8)
                .shadow(floating_shadow(active(), 18))
                .inner_margin(egui::Margin::same(4))
                .show(ui, |ui| {
                    ui.set_width(POPUP_WIDTH - 8.0);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for (row, &(_, i)) in visible.iter().enumerate().skip(popup.scroll).take(rows) {
                        let item = &popup.items[i];
                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), POPUP_ROW),
                            egui::Sense::click(),
                        );
                        let painter = ui.painter_at(rect);
                        if row == popup.selected {
                            painter.rect_filled(rect, 5, colors().accent_bg);
                        } else if response.hovered() {
                            painter.rect_filled(rect, 5, colors().hover);
                        }
                        let tag = egui::Rect::from_min_size(
                            rect.left_center() + egui::vec2(4.0, -8.0),
                            egui::vec2(36.0, 16.0),
                        );
                        let color = item.kind.color();
                        painter.rect_filled(tag, 4, color.gamma_multiply(0.16));
                        painter.text(
                            tag.center(),
                            egui::Align2::CENTER_CENTER,
                            item.kind.tag(),
                            tag_font.clone(),
                            color,
                        );
                        // The typed part of the name in the accent color.
                        let matched = if rank(&item.label, &prefix).is_some_and(|r| r < 2) {
                            prefix.chars().count()
                        } else {
                            0
                        };
                        let head: String = item.label.chars().take(matched).collect();
                        let tail: String = item.label.chars().skip(matched).collect();
                        let label_x = tag.right() + 8.0;
                        let mut job = egui::text::LayoutJob::default();
                        job.append(
                            &head,
                            0.0,
                            egui::TextFormat::simple(font.clone(), colors().accent),
                        );
                        job.append(
                            &tail,
                            0.0,
                            egui::TextFormat::simple(font.clone(), colors().text),
                        );
                        job.wrap = egui::text::TextWrapping::truncate_at_width(
                            rect.right() - 8.0 - label_x,
                        );
                        let galley = ui.fonts_mut(|f| f.layout_job(job));
                        let label_w = galley.size().x;
                        painter.galley(
                            egui::pos2(label_x, rect.center().y - galley.size().y / 2.0),
                            galley,
                            colors().text,
                        );
                        let room = rect.right() - 8.0 - (label_x + label_w + 16.0);
                        if !item.detail.is_empty() && room > 40.0 {
                            let mut job = egui::text::LayoutJob::simple_singleline(
                                item.detail.replace('\n', " "),
                                small.clone(),
                                colors().muted,
                            );
                            job.wrap = egui::text::TextWrapping::truncate_at_width(room);
                            let detail = ui.fonts_mut(|f| f.layout_job(job));
                            painter.galley(
                                egui::pos2(
                                    rect.right() - 8.0 - detail.size().x,
                                    rect.center().y - detail.size().y / 2.0,
                                ),
                                detail,
                                colors().muted,
                            );
                        }
                        if response.clicked() {
                            accept = Some(i);
                        }
                    }
                    if footer {
                        ui.add_space(4.0);
                        let (line, _) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), 1.0),
                            egui::Sense::hover(),
                        );
                        ui.painter().rect_filled(line, 0, colors().border);
                        ui.add_space(6.0);
                        egui::ScrollArea::vertical()
                            .id_salt("completion-docs")
                            .max_height(62.0)
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing.y = 3.0;
                                if !detail.is_empty() {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&detail)
                                                .monospace()
                                                .size(12.0)
                                                .color(colors().text),
                                        )
                                        .wrap(),
                                    );
                                }
                                if !docs.is_empty() {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&docs)
                                                .size(SMALL)
                                                .color(colors().secondary),
                                        )
                                        .wrap(),
                                    );
                                }
                            });
                    }
                });
        });
    lsp.popup_rect = Some(area.response.rect);
    if lsp.popup.as_ref().is_some_and(|p| p.server) && accept.is_none() {
        let path = doc.path.clone();
        lsp.request_resolve(&path, &chosen);
    }
    let Some(i) = accept else { return };
    let Some(popup) = lsp.popup.take() else {
        return;
    };
    let item = &popup.items[i];
    // The server's range start when it has one (it can begin before the typed word).
    let anchor = item
        .edit_start
        .map(|start| char_index(&doc.text, start))
        .filter(|start| *start <= cursor)
        .unwrap_or(popup.anchor);
    let start = doc
        .text
        .char_indices()
        .nth(anchor)
        .map_or(doc.text.len(), |(b, _)| b);
    let end = doc
        .text
        .char_indices()
        .nth(cursor)
        .map_or(doc.text.len(), |(b, _)| b);
    doc.text.replace_range(start..end, &item.insert);
    doc.update_dirty();
    let caret = anchor + item.insert.chars().count();
    let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
    state
        .cursor
        .set_char_range(Some(CCursorRange::one(CCursor::new(caret))));
    state.store(ui.ctx(), id);
    lsp.resolved = None;
    // A click on the popup took focus from the editor: give it back.
    ui.memory_mut(|m| m.request_focus(id));
    ui.ctx().request_repaint();
}

fn paint_diagnostics(
    ui: &mut Ui,
    lsp: &Lsp,
    doc: &Document,
    output: &egui::text_edit::TextEditOutput,
) -> bool {
    let mut shown = false;
    let Some(list) = lsp
        .diagnostics
        .get(&crate::agents_runtime::plain(&doc.path))
    else {
        return false;
    };
    let origin = output.galley_pos.to_vec2();
    let pointer = ui.ctx().pointer_hover_pos();
    for (n, diag) in list.iter().take(300).enumerate() {
        let start = char_index(&doc.text, diag.start);
        let mut end = char_index(&doc.text, diag.end).max(start + 1);
        let a = output
            .galley
            .pos_from_cursor(CCursor::new(start))
            .translate(origin);
        let mut b = output
            .galley
            .pos_from_cursor(CCursor::new(end))
            .translate(origin);
        if (b.top() - a.top()).abs() > 1.0 {
            // Multi-line range: underline the first line only.
            end = doc
                .text
                .chars()
                .skip(start)
                .position(|c| c == '\n')
                .map_or(end, |p| start + p);
            b = output
                .galley
                .pos_from_cursor(CCursor::new(end.max(start + 1)))
                .translate(origin);
        }
        let color = match diag.severity {
            1 => colors().error,
            2 => colors().warning,
            _ => colors().accent,
        };
        let y = a.bottom() - 1.0;
        let (x0, x1) = (a.left(), b.left().max(a.left() + 6.0));
        let mut points = Vec::new();
        let mut x = x0;
        let mut up = true;
        while x <= x1 {
            points.push(egui::pos2(x, if up { y - 1.5 } else { y + 0.5 }));
            x += 2.5;
            up = !up;
        }
        ui.painter()
            .add(egui::Shape::line(points, egui::Stroke::new(1.2_f32, color)));
        let rect = egui::Rect::from_min_max(egui::pos2(x0, a.top()), egui::pos2(x1, a.bottom()));
        if pointer.is_some_and(|p| rect.contains(p)) {
            shown = true;
            egui::Tooltip::always_open(
                ui.ctx().clone(),
                ui.layer_id(),
                id_for(n),
                egui::PopupAnchor::Pointer,
            )
            .show(|ui| {
                ui.set_max_width(420.0);
                ui.label(RichText::new(&diag.message).color(colors().text));
                if !diag.source.is_empty() {
                    ui.label(
                        RichText::new(&diag.source)
                            .size(SMALL)
                            .color(colors().muted),
                    );
                }
            });
        }
    }
    shown
}

/// Type information under a resting pointer, from `textDocument/hover`.
fn hover(
    ui: &mut Ui,
    lsp: &mut Lsp,
    doc: &Document,
    output: &egui::text_edit::TextEditOutput,
    on_diagnostic: bool,
) {
    let Some(pointer) = ui.ctx().pointer_hover_pos() else {
        return;
    };
    if on_diagnostic || !output.response.rect.contains(pointer) || lsp.popup.is_some() {
        return;
    }
    let resting = ui.input(|i| i.pointer.time_since_last_movement()) > 0.45;
    let local = pointer - output.galley_pos;
    let index = output.galley.cursor_from_pos(local).index;
    let start = word_start(&doc.text, index, dashed(&doc.path));
    let over_word = doc
        .text
        .chars()
        .nth(index)
        .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$');
    if !resting || !over_word {
        return;
    }
    lsp.request_hover(doc, start.max(index.min(start + 1)));
    let Some((path, _, text)) = &lsp.hover else {
        ui.ctx().request_repaint_after(Duration::from_millis(100));
        return;
    };
    if path != &doc.path {
        return;
    }
    egui::Tooltip::always_open(
        ui.ctx().clone(),
        ui.layer_id(),
        egui::Id::new("lsp-hover"),
        egui::PopupAnchor::Pointer,
    )
    .show(|ui| {
        ui.set_max_width(520.0);
        for (i, part) in text.split("\n---\n").enumerate() {
            if i > 0 {
                ui.separator();
            }
            ui.label(
                RichText::new(part.trim())
                    .monospace()
                    .size(12.0)
                    .color(colors().text),
            );
        }
    });
}

/// Hover contents in any of the LSP shapes, without markdown fences.
fn hover_text(contents: &Value) -> String {
    let piece = |v: &Value| -> String {
        v.as_str()
            .map(str::to_owned)
            .or_else(|| v["value"].as_str().map(str::to_owned))
            .unwrap_or_default()
    };
    let text = match contents {
        Value::Array(items) => items.iter().map(piece).collect::<Vec<_>>().join("\n---\n"),
        other => piece(other),
    };
    let fence = "`".repeat(3);
    text.lines()
        .filter(|l| !l.trim_start().starts_with(&fence))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .chars()
        .take(1500)
        .collect()
}

/// Applies LSP text edits (ranges in the original text) from last to first.
fn apply_edits(doc: &mut Document, mut edits: TextEdits) {
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.0));
    for (start, end, text) in edits {
        let a = char_index(&doc.text, start);
        let b = char_index(&doc.text, end).max(a);
        let byte = |i: usize| {
            doc.text
                .char_indices()
                .nth(i)
                .map_or(doc.text.len(), |(b, _)| b)
        };
        let (a, b) = (byte(a), byte(b));
        doc.text.replace_range(a..b, &text);
    }
    doc.update_dirty();
}

fn id_for(n: usize) -> egui::Id {
    egui::Id::new(("diagnostic-tip", n))
}

impl super::Editor {
    /// Keeps language servers in step with the open documents.
    pub(super) fn lsp_sync(&mut self, ctx: &egui::Context) {
        self.lsp.sync(self.root.as_deref(), &self.documents, ctx);
        if let Some(note) = self.lsp.format_note.take() {
            self.notice = Some((false, tr(&note).to_owned()));
        }
    }

    /// Problems view: every diagnostic the servers reported for open files.
    pub(super) fn problems_view(&mut self, ui: &mut Ui) {
        let active = self
            .active
            .and_then(|i| self.documents.get(i))
            .map(|d| d.path.clone());
        if let Some(path) = &active
            && let Some((lang, state)) = self.lsp.state(path)
        {
            let native = lang == Lang::Ts && self.lsp.native_typescript();
            let name = if native {
                "TypeScript 7 (tsc --lsp)"
            } else {
                lang.server()
            };
            ui.horizontal(|ui| {
                let text = match state {
                    "ready" => format!("{} {}", name, tr("is running")),
                    "starting" => format!("{} {}", name, tr("is starting…")),
                    "missing" => format!(
                        "{} {} · {}",
                        name,
                        tr("is not installed"),
                        lang.install_hint()
                    ),
                    "stopped" => format!(
                        "{} {}: {}",
                        name,
                        tr("stopped"),
                        self.lsp.failure(lang).unwrap_or_default()
                    ),
                    _ => String::new(),
                };
                ui.label(RichText::new(text).size(SMALL).color(colors().muted));
            });
        }
        let mut open = None;
        egui::ScrollArea::vertical()
            .id_salt("problems")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut paths: Vec<&PathBuf> = self.lsp.diagnostics.keys().collect();
                paths.sort();
                let mut any = false;
                for path in paths {
                    let list = &self.lsp.diagnostics[path];
                    if list.is_empty() {
                        continue;
                    }
                    any = true;
                    let name = self
                        .root
                        .as_deref()
                        .and_then(|r| path.strip_prefix(crate::agents_runtime::plain(r)).ok())
                        .unwrap_or(path)
                        .display()
                        .to_string();
                    ui.label(RichText::new(name).strong().size(SMALL));
                    for diag in list.iter().take(200) {
                        let (icon, color) = match diag.severity {
                            1 => ("error", colors().error),
                            2 => ("warning", colors().warning),
                            _ => ("info", colors().accent),
                        };
                        let row = ui.horizontal(|ui| {
                            ui.label(RichText::new(icon).size(SMALL).color(color));
                            ui.label(
                                RichText::new(format!("{}:{}", diag.start.0 + 1, diag.start.1 + 1))
                                    .monospace()
                                    .size(SMALL)
                                    .color(colors().muted),
                            );
                            ui.add(
                                egui::Label::new(RichText::new(&diag.message).size(SMALL))
                                    .truncate(),
                            )
                        });
                        if row.inner.interact(egui::Sense::click()).clicked() {
                            open = Some((path.clone(), diag.start, diag.end));
                        }
                    }
                }
                if !any {
                    ui.label(RichText::new(tr("No problems reported.")).color(colors().muted));
                }
            });
        if let Some((path, start, end)) = open {
            self.open_file(path.clone());
            if let Some(doc) = self.documents.iter().find(|d| d.path == path) {
                let a = char_index(&doc.text, start);
                let b = char_index(&doc.text, end);
                self.selection = Some((a, b.max(a)));
                self.focus_editor = true;
            }
        }
    }
}

#[cfg(all(test, windows))]
impl Lsp {
    pub(super) fn harness_ready(&self, path: &Path) -> bool {
        self.state(path).is_some_and(|(_, state)| state == "ready")
    }

    pub(super) fn harness_has(&self, path: &Path, needle: &str) -> bool {
        self.diagnostics
            .get(&crate::agents_runtime::plain(path))
            .is_some_and(|l| l.iter().any(|d| d.message.contains(needle)))
    }

    pub(super) fn harness_popup_items(&self) -> usize {
        self.popup.as_ref().map_or(0, |p| p.items.len())
    }

    pub(super) fn harness_close_popup(&mut self) {
        self.popup = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_round_trip_through_utf16() {
        let text = "const a = 1;\nconst é = \"😀x\";\n";
        let idx = text.chars().position(|c| c == 'x').unwrap();
        let p = position(text, idx);
        assert_eq!(p, (1, 13));
        assert_eq!(char_index(text, p), idx);
        assert_eq!(char_index(text, (0, 99)), 12, "clamped to the line end");
        if cfg!(windows) {
            assert_eq!(
                path_from_uri("file:///e%3A/a/b.ts").unwrap(),
                PathBuf::from(r"E:\a\b.ts")
            );
        }
        let path = PathBuf::from(r"E:\my site\src\App.tsx");
        let u = uri(&path);
        assert!(u.starts_with("file:///E%3A/my%20site/src/App.tsx"), "{u}");
        if cfg!(windows) {
            assert_eq!(path_from_uri(&u).unwrap(), path);
        }
    }

    #[test]
    fn buffer_symbols_carry_kinds_and_rank_by_match() {
        let text = "struct Config { port: u16 }\nfn load_config() -> Config {}\nconst MAX_PORT: u16 = 1;\nlet total = cfg.port + cfg.port;\n";
        let path = Path::new("main.rs");
        let all = words(text, path, false);
        let kind = |label: &str| all.iter().find(|i| i.label == label).map(|i| i.kind);
        assert_eq!(kind("Config"), Some(Kind::Type));
        assert_eq!(kind("load_config"), Some(Kind::Function));
        assert_eq!(kind("MAX_PORT"), Some(Kind::Constant));
        assert_eq!(kind("total"), Some(Kind::Variable));
        assert_eq!(
            kind("match"),
            Some(Kind::Keyword),
            "language keywords join in"
        );
        let members: Vec<String> = words(text, path, true)
            .into_iter()
            .map(|i| i.label)
            .collect();
        assert_eq!(members, ["port"], "after `.` only names used as members");
        assert_eq!(rank("useState", "use"), Some(0));
        assert_eq!(rank("UseState", "use"), Some(1));
        assert_eq!(rank("getUser", "gtus"), Some(2));
        assert_eq!(
            rank("getUser", "usr"),
            None,
            "subsequences start at the first letter"
        );
        assert_eq!(word_start("a-b", 3, false), 2);
        assert_eq!(word_start("margin-top", 10, true), 0);
    }

    /// Talks to a minimal Node LSP server: initialize, didOpen, diagnostics
    /// and completion over the real stdio framing.
    #[test]
    fn diagnostics_and_completion_from_a_stdio_server() {
        if Command::new("node").arg("--version").output().is_err() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("server.js");
        std::fs::write(&script, FAKE_SERVER).unwrap();
        let root = dir.path().to_path_buf();
        let doc_path = root.join("App.tsx");
        std::fs::write(&doc_path, "const total = 1;\ntot").unwrap();
        let doc = Document::open(doc_path.clone()).unwrap();
        let mut command = Command::new("node");
        command.arg(&script);
        let ctx = egui::Context::default();
        let mut lsp = Lsp {
            root: Some(root.clone()),
            ..Lsp::default()
        };
        lsp.servers.insert(
            Lang::Ts,
            Slot::Running(Box::new(
                Server::start(command, &root, json!({}), ctx.clone()).unwrap(),
            )),
        );
        let docs = vec![doc];
        let until = Instant::now() + Duration::from_secs(20);
        while lsp.diagnostics.get(&doc_path).is_none_or(Vec::is_empty) && Instant::now() < until {
            lsp.synced = None;
            lsp.sync(Some(&root), &docs, &ctx);
            thread::sleep(Duration::from_millis(30));
        }
        let diags = &lsp.diagnostics[&doc_path];
        assert_eq!(diags[0].message, "Cannot find name 'tot'.");
        assert_eq!(diags[0].start, (1, 0));
        lsp.trigger(&docs[0], docs[0].text.chars().count(), false);
        let until = Instant::now() + Duration::from_secs(10);
        while lsp.popup.as_ref().is_none_or(|p| p.items.is_empty()) && Instant::now() < until {
            lsp.poll();
            thread::sleep(Duration::from_millis(20));
        }
        let popup = lsp.popup.as_ref().unwrap();
        assert!(popup.server);
        assert_eq!(popup.items[0].label, "total");
        assert_eq!(popup.anchor, 17);
    }

    /// The real TypeScript server on a throwaway project: member completion has kinds and
    /// the highlighted item resolves to its signature.
    #[test]
    #[ignore = "needs typescript-language-server on PATH"]
    fn real_typescript_completion_resolves_signatures() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("probe.ts");
        std::fs::write(&file, "const s = \"x\";\ns.toUp").unwrap();
        let ctx = egui::Context::default();
        let docs = vec![Document::open(file).unwrap()];
        let mut lsp = Lsp::default();
        let until = Instant::now() + Duration::from_secs(60);
        while !lsp.has_server(&docs[0].path) && Instant::now() < until {
            lsp.synced = None;
            lsp.sync(Some(dir.path()), &docs, &ctx);
            thread::sleep(Duration::from_millis(50));
        }
        lsp.trigger(&docs[0], docs[0].text.chars().count(), true);
        while lsp.popup.as_ref().is_none_or(|p| p.items.is_empty()) && Instant::now() < until {
            lsp.poll();
            thread::sleep(Duration::from_millis(20));
        }
        let item = lsp
            .popup
            .as_ref()
            .expect("popup")
            .items
            .iter()
            .find(|i| i.label == "toUpperCase")
            .cloned()
            .expect("toUpperCase offered");
        assert_eq!(item.kind, Kind::Function);
        lsp.request_resolve(&docs[0].path, &item);
        while lsp.resolved.is_none() && Instant::now() < until {
            lsp.poll();
            thread::sleep(Duration::from_millis(20));
        }
        let (label, detail, _) = lsp.resolved.expect("resolved");
        assert_eq!(label, "toUpperCase");
        assert!(detail.contains("toUpperCase(): string"), "{detail}");
    }

    /// Real servers on a Vite React-TS project (COVALOS_LSP_PROJECT): the
    /// servers are found the same way the editor finds them.
    #[test]
    #[ignore = "needs typescript-language-server and vscode-langservers-extracted"]
    fn real_servers_on_a_react_project() {
        let root =
            PathBuf::from(std::env::var_os("COVALOS_LSP_PROJECT").expect("COVALOS_LSP_PROJECT"));
        let ctx = egui::Context::default();
        let mut app = Document::open(root.join("src").join("App.tsx")).unwrap();
        app.text
            .push_str("\nconst broken: number = \"text\";\nconst probe = useSt");
        let mut css = Document::open(root.join("src").join("App.css")).unwrap();
        css.text.push_str("\n.probe { colr: red; }\n");
        let docs = vec![app, css];
        let mut lsp = Lsp::default();
        let started = Instant::now();
        let has = |lsp: &Lsp, path: &Path, needle: &str| {
            lsp.diagnostics
                .get(path)
                .is_some_and(|l| l.iter().any(|d| d.message.contains(needle)))
        };
        while !(has(&lsp, &docs[0].path, "not assignable") && has(&lsp, &docs[1].path, "colr"))
            && started.elapsed() < Duration::from_secs(90)
        {
            lsp.synced = None;
            lsp.sync(Some(&root), &docs, &ctx);
            thread::sleep(Duration::from_millis(100));
        }
        for doc in &docs {
            eprintln!("{} -> {:?}", doc.path.display(), lsp.state(&doc.path));
            for d in lsp.diagnostics.get(&doc.path).into_iter().flatten() {
                eprintln!("  {:?} sev {} {}", d.start, d.severity, d.message);
            }
        }
        eprintln!("diagnostics after {:?}", started.elapsed());
        for (path, list) in &lsp.diagnostics {
            eprintln!("key {} ({} items)", path.display(), list.len());
        }
        assert!(
            has(&lsp, &docs[0].path, "not assignable"),
            "no TypeScript diagnostic"
        );
        assert!(has(&lsp, &docs[1].path, "colr"), "no CSS diagnostic");
        let cursor = docs[0].text.chars().count();
        lsp.trigger(&docs[0], cursor, false);
        let until = Instant::now() + Duration::from_secs(30);
        while lsp.popup.as_ref().is_none_or(|p| p.items.is_empty()) && Instant::now() < until {
            lsp.poll();
            thread::sleep(Duration::from_millis(20));
        }
        let popup = lsp.popup.as_ref().expect("popup");
        let labels: Vec<&str> = popup
            .items
            .iter()
            .map(|i| i.label.as_str())
            .filter(|l| l.starts_with("useSt"))
            .collect();
        eprintln!(
            "{} completion items, first {:?}",
            labels.len(),
            &labels[..labels.len().min(8)]
        );
        assert!(popup.server);
        assert!(labels.contains(&"useState"), "useState not offered");

        // Hover over `useState` in the import line.
        let index = docs[0].text.find("useState").unwrap() + 2;
        lsp.request_hover(&docs[0], index);
        let until = Instant::now() + Duration::from_secs(20);
        while lsp.hover.is_none() && Instant::now() < until {
            lsp.poll();
            thread::sleep(Duration::from_millis(20));
        }
        let hover = lsp.hover.clone().expect("hover").2;
        eprintln!("hover: {hover}");
        assert!(hover.contains("useState"), "{hover}");

        // Formatting returns edits for badly spaced code.
        let mut ugly = Document::open(root.join("src").join("main.tsx")).unwrap();
        ugly.text.push_str(
            "
const   spaced={a:1,   b:2}
",
        );
        let docs = vec![ugly];
        lsp.synced = None;
        lsp.sync(Some(&root), &docs, &ctx);
        thread::sleep(Duration::from_millis(500));
        lsp.poll();
        lsp.request_format(&docs[0]);
        let until = Instant::now() + Duration::from_secs(20);
        while lsp.formatted.is_none() && lsp.format_note.is_none() && Instant::now() < until {
            lsp.poll();
            thread::sleep(Duration::from_millis(20));
        }
        let mut doc = docs.into_iter().next().unwrap();
        let (_, edits) = lsp.formatted.take().expect("format edits");
        apply_edits(&mut doc, edits);
        assert!(
            doc.text.contains("const spaced = { a: 1, b: 2 }"),
            "{}",
            doc.text
        );
    }

    const FAKE_SERVER: &str = r#"
let buf = Buffer.alloc(0);
const send = (m) => { const b = Buffer.from(JSON.stringify(m)); process.stdout.write(`Content-Length: ${b.length}\r\n\r\n`); process.stdout.write(b); };
process.stdin.on('data', (d) => {
  buf = Buffer.concat([buf, d]);
  for (;;) {
    const h = buf.indexOf('\r\n\r\n'); if (h < 0) return;
    const len = parseInt(/Content-Length: (\d+)/.exec(buf.slice(0, h).toString())[1]);
    if (buf.length < h + 4 + len) return;
    const msg = JSON.parse(buf.slice(h + 4, h + 4 + len).toString()); buf = buf.slice(h + 4 + len);
    if (msg.method === 'initialize') send({ jsonrpc: '2.0', id: msg.id, result: { capabilities: { completionProvider: { triggerCharacters: ['.'] } } } });
    if (msg.method === 'textDocument/didOpen') send({ jsonrpc: '2.0', method: 'textDocument/publishDiagnostics', params: { uri: msg.params.textDocument.uri, diagnostics: [{ range: { start: { line: 1, character: 0 }, end: { line: 1, character: 3 } }, severity: 1, message: "Cannot find name 'tot'.", source: 'ts' }] } });
    if (msg.method === 'textDocument/completion') send({ jsonrpc: '2.0', id: msg.id, result: { isIncomplete: false, items: [{ label: 'total', detail: 'const total: 1', sortText: '1' }, { label: 'toString', sortText: '2' }] } });
  }
});
"#;
}

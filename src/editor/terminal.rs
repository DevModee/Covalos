//! Terminal tabs running real PowerShell processes, the TCP ports those
//! processes open, and Cloudflare quick tunnels for a chosen port.
//!
//! Each command is its own `powershell.exe` process started in the tab's
//! directory (`cd` is tracked by the tab), so a long-running dev server keeps
//! one tab busy while other tabs stay usable. Every process joins the Covalos
//! job object, so nothing outlives the editor.
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use eframe::egui::{self, Align, Key, Layout, Margin, RichText, ScrollArea, TextEdit, Ui};

use super::{Editor, theme::*};

const MAX_LOG: usize = 200_000;

enum Event {
    Output(String),
    Finished(String),
}

pub(super) struct Job {
    events: mpsc::Receiver<Event>,
    stop: Arc<AtomicBool>,
    pid: Arc<AtomicU32>,
    started: Instant,
    command: String,
}

impl Drop for Job {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Job {
    fn pid(&self) -> Option<u32> {
        Some(self.pid.load(Ordering::Relaxed)).filter(|p| *p != 0)
    }
}

struct Term {
    id: u32,
    cwd: PathBuf,
    log: String,
    input: String,
    job: Option<Job>,
    history: Vec<String>,
    recall: Option<usize>,
    /// Local URLs the processes printed (dev servers announce them).
    urls: Vec<String>,
}

pub(super) struct Port {
    pub(super) port: u16,
    pub(super) address: String,
    pub(super) pid: u32,
    pub(super) process: String,
    /// Terminal tab whose process tree owns the listener.
    pub(super) owner: Option<u32>,
}

struct Tunnel {
    port: u16,
    url: Option<String>,
    log: String,
    job: Job,
    done: bool,
}

#[derive(Default)]
pub(super) struct Terminals {
    tabs: Vec<Term>,
    active: usize,
    second: Option<usize>,
    next: u32,
    pub(super) ports: Vec<Port>,
    ports_at: Option<Instant>,
    show_all_ports: bool,
    tunnels: Vec<Tunnel>,
    confirm_tunnel: Option<u16>,
    cloudflared: Option<Option<PathBuf>>,
    focus_input: bool,
}

fn append(log: &mut String, text: &str) {
    log.push_str(text);
    if log.len() > MAX_LOG {
        let end = log.ceil_char_boundary(log.len() - MAX_LOG);
        log.drain(..end);
    }
}

fn strip_ansi(text: &str) -> String {
    use std::sync::OnceLock;
    static ANSI: OnceLock<regex::Regex> = OnceLock::new();
    let re = ANSI.get_or_init(|| {
        regex::Regex::new(
            r"\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(\x07|\x1b\\)|\x1b[()][A-Za-z0-9]|\r",
        )
        .expect("static regex")
    });
    re.replace_all(text, "").into_owned()
}

/// `http://localhost:5173/`-style addresses printed by a process.
fn local_urls(text: &str) -> Vec<String> {
    use std::sync::OnceLock;
    static URL: OnceLock<regex::Regex> = OnceLock::new();
    let re = URL.get_or_init(|| {
        regex::Regex::new(
            r"https?://(?:localhost|127\.0\.0\.1|\[::1\]|0\.0\.0\.0)(?::\d{2,5})?(?:/[^\s'\x22<>]*)?",
        )
        .expect("static regex")
    });
    re.find_iter(text)
        .map(|m| {
            m.as_str()
                .replace("0.0.0.0", "localhost")
                .trim_end_matches(['.', ',', ')'])
                .to_owned()
        })
        .collect()
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn powershell(cwd: &Path, script: &str) -> Command {
    let full = format!(
        "$ProgressPreference='SilentlyContinue'; [Console]::OutputEncoding=[Text.Encoding]::UTF8; {script}"
    );
    let utf16: Vec<u8> = full.encode_utf16().flat_map(u16::to_le_bytes).collect();
    #[cfg(windows)]
    let mut command = {
        let mut c = Command::new("powershell.exe");
        c.args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
        ])
        .args(["-EncodedCommand", &base64(&utf16)]);
        c
    };
    #[cfg(not(windows))]
    let mut command = {
        let _ = utf16;
        let mut c = Command::new("sh");
        c.args(["-c", script]);
        c
    };
    command
        .current_dir(cwd)
        .env("BROWSER", "none")
        .env("FORCE_COLOR", "0");
    command
}

fn spawn(mut command: Command, label: String, ctx: egui::Context) -> Job {
    let (tx, events) = mpsc::sync_channel(512);
    let stop = Arc::new(AtomicBool::new(false));
    let pid = Arc::new(AtomicU32::new(0));
    let (thread_stop, thread_pid) = (stop.clone(), pid.clone());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP
        command.creation_flags(0x0800_0000 | 0x0000_0200);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    thread::spawn(move || {
        let send = |event| {
            let _ = tx.send(event);
            ctx.request_repaint();
        };
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                send(Event::Finished(format!(
                    "Could not start the process. {error}"
                )));
                return;
            }
        };
        covalos_engine::process::contain(&child);
        thread_pid.store(child.id(), Ordering::Relaxed);
        let read = |reader: Box<dyn Read + Send>| {
            let tx = tx.clone();
            let ctx = ctx.clone();
            thread::spawn(move || {
                let mut reader = BufReader::new(reader);
                loop {
                    let mut bytes = Vec::new();
                    match (&mut reader).take(8192).read_until(b'\n', &mut bytes) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {
                            let text = strip_ansi(&String::from_utf8_lossy(&bytes));
                            if tx.send(Event::Output(text)).is_err() {
                                break;
                            }
                            ctx.request_repaint();
                        }
                    }
                }
            })
        };
        let out = read(Box::new(child.stdout.take().expect("piped")));
        let err = read(Box::new(child.stderr.take().expect("piped")));
        let status = loop {
            if thread_stop.load(Ordering::Relaxed) {
                covalos_engine::process::kill_tree(&mut child);
                break "Stopped.".to_owned();
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    break match status.code() {
                        Some(0) => String::new(),
                        Some(code) => format!("Exited with code {code}."),
                        None => "Exited.".to_owned(),
                    };
                }
                Ok(None) => thread::sleep(Duration::from_millis(40)),
                Err(e) => {
                    covalos_engine::process::kill_tree(&mut child);
                    break format!("Could not wait for the process. {e}");
                }
            }
        };
        for _ in 0..50 {
            if out.is_finished() && err.is_finished() {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        send(Event::Finished(status));
    });
    Job {
        events,
        stop,
        pid,
        started: Instant::now(),
        command: label,
    }
}

impl Term {
    fn new(id: u32, cwd: PathBuf) -> Self {
        Self {
            id,
            cwd,
            log: String::new(),
            input: String::new(),
            job: None,
            history: Vec::new(),
            recall: None,
            urls: Vec::new(),
        }
    }

    fn title(&self) -> String {
        match &self.job {
            Some(job) => job
                .command
                .split_whitespace()
                .take(3)
                .collect::<Vec<_>>()
                .join(" "),
            None => self.cwd.file_name().map_or_else(
                || "PowerShell".to_owned(),
                |n| n.to_string_lossy().into_owned(),
            ),
        }
    }

    fn poll(&mut self) {
        let Some(job) = &self.job else { return };
        let mut finished = None;
        for event in job.events.try_iter().take(512) {
            match event {
                Event::Output(text) => {
                    for url in local_urls(&text) {
                        if !self.urls.contains(&url) {
                            self.urls.push(url);
                        }
                    }
                    append(&mut self.log, &text);
                }
                Event::Finished(status) => {
                    finished = Some(status);
                    break;
                }
            }
        }
        if let Some(status) = finished {
            if !status.is_empty() {
                append(&mut self.log, &format!("{status}\n"));
            }
            self.job = None;
        }
    }

    fn run(&mut self, ctx: &egui::Context) {
        let line = std::mem::take(&mut self.input);
        let script = line.trim();
        self.recall = None;
        if script.is_empty() {
            return;
        }
        if self.history.last().is_none_or(|h| h != script) {
            self.history.push(script.to_owned());
        }
        append(
            &mut self.log,
            &format!("PS {}> {script}\n", self.cwd.display()),
        );
        if matches!(script, "cls" | "clear" | "Clear-Host") {
            self.log.clear();
            return;
        }
        let target = script
            .strip_prefix("cd ")
            .or_else(|| script.strip_prefix("Set-Location "))
            .or_else(|| script.strip_prefix("sl "))
            .map(|t| t.trim().trim_matches(['"', '\'']));
        if let Some(target) = target.or((script == "cd..").then_some("..")) {
            let next = if target == "~" {
                std::env::var_os("USERPROFILE").map_or_else(|| self.cwd.clone(), PathBuf::from)
            } else {
                self.cwd.join(target)
            };
            match std::fs::canonicalize(&next) {
                Ok(path) if path.is_dir() => {
                    self.cwd = crate::agents_runtime::plain(&path);
                }
                _ => append(&mut self.log, &format!("Cannot find path '{target}'.\n")),
            }
            return;
        }
        self.urls.clear();
        self.job = Some(spawn(
            powershell(&self.cwd, script),
            script.to_owned(),
            ctx.clone(),
        ));
    }
}

// ------------------------------------------------------------------ ports

/// Listening TCP sockets with their owning process (Windows IP Helper API).
#[cfg(windows)]
fn listeners() -> Vec<(u16, String, u32)> {
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_OWNER_PID,
        TCP_TABLE_OWNER_PID_LISTENER,
    };
    const AF_INET: u32 = 2;
    const AF_INET6: u32 = 23;
    let mut out = Vec::new();
    for family in [AF_INET, AF_INET6] {
        let mut size = 0_u32;
        // SAFETY: a null buffer asks for the required size.
        unsafe {
            GetExtendedTcpTable(
                std::ptr::null_mut(),
                &mut size,
                0,
                family,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            )
        };
        if size == 0 {
            continue;
        }
        let mut buffer = vec![0_u8; size as usize + 4096];
        size = buffer.len() as u32;
        // SAFETY: the buffer holds at least `size` bytes and outlives the call.
        let status = unsafe {
            GetExtendedTcpTable(
                buffer.as_mut_ptr().cast(),
                &mut size,
                0,
                family,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            )
        };
        if status != 0 {
            continue;
        }
        let count = u32::from_ne_bytes(buffer[..4].try_into().expect("4 bytes")) as usize;
        let rows = &buffer[4..];
        for i in 0..count {
            if family == AF_INET {
                let width = std::mem::size_of::<MIB_TCPROW_OWNER_PID>();
                let Some(raw) = rows.get(i * width..(i + 1) * width) else {
                    break;
                };
                // SAFETY: `raw` holds one complete row; read_unaligned tolerates alignment.
                let row: MIB_TCPROW_OWNER_PID =
                    unsafe { std::ptr::read_unaligned(raw.as_ptr().cast()) };
                let port = u16::from_be(row.dwLocalPort as u16);
                let ip = std::net::Ipv4Addr::from(u32::from_be(row.dwLocalAddr));
                out.push((port, ip.to_string(), row.dwOwningPid));
            } else {
                let width = std::mem::size_of::<MIB_TCP6ROW_OWNER_PID>();
                let Some(raw) = rows.get(i * width..(i + 1) * width) else {
                    break;
                };
                // SAFETY: as above.
                let row: MIB_TCP6ROW_OWNER_PID =
                    unsafe { std::ptr::read_unaligned(raw.as_ptr().cast()) };
                let port = u16::from_be(row.dwLocalPort as u16);
                let ip = std::net::Ipv6Addr::from(row.ucLocalAddr);
                out.push((port, format!("[{ip}]"), row.dwOwningPid));
            }
        }
    }
    out.sort();
    out.dedup_by(|a, b| a.0 == b.0 && a.2 == b.2);
    out
}

#[cfg(not(windows))]
fn listeners() -> Vec<(u16, String, u32)> {
    Vec::new()
}

/// pid → (parent pid, executable name).
#[cfg(windows)]
fn processes() -> HashMap<u32, (u32, String)> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
        System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS,
        },
    };
    let mut map = HashMap::new();
    // SAFETY: plain Win32 snapshot walk; the handle is closed below.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return map;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut entry) != 0;
        while ok {
            let len = entry
                .szExeFile
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
            map.insert(entry.th32ProcessID, (entry.th32ParentProcessID, name));
            ok = Process32NextW(snap, &mut entry) != 0;
        }
        CloseHandle(snap);
    }
    map
}

#[cfg(not(windows))]
fn processes() -> HashMap<u32, (u32, String)> {
    HashMap::new()
}

const SYSTEM: &[&str] = &[
    "System",
    "svchost.exe",
    "lsass.exe",
    "wininit.exe",
    "services.exe",
    "spoolsv.exe",
    "Idle",
    "smss.exe",
    "csrss.exe",
];

fn cloudflared() -> Option<PathBuf> {
    let installed = ["ProgramFiles(x86)", "ProgramFiles"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(|p| PathBuf::from(p).join("cloudflared").join("cloudflared.exe"));
    let on_path = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .map(|d| {
            d.join(if cfg!(windows) {
                "cloudflared.exe"
            } else {
                "cloudflared"
            })
        });
    installed.chain(on_path).find(|p| p.is_file())
}

fn trycloudflare(text: &str) -> Option<String> {
    let start = text.find("https://")?;
    let rest = &text[start..];
    let end = rest
        .find(|c: char| c.is_whitespace() || c == '|')
        .unwrap_or(rest.len());
    let url = &rest[..end];
    url.contains(".trycloudflare.com").then(|| url.to_owned())
}

impl Terminals {
    pub(super) fn poll(&mut self) {
        for term in &mut self.tabs {
            term.poll();
        }
        for tunnel in &mut self.tunnels {
            for event in tunnel.job.events.try_iter().take(256) {
                match event {
                    Event::Output(text) => {
                        if tunnel.url.is_none() {
                            tunnel.url = trycloudflare(&text);
                        }
                        append(&mut tunnel.log, &text);
                    }
                    Event::Finished(_) => tunnel.done = true,
                }
            }
        }
        self.tunnels.retain(|t| !t.done);
    }

    pub(super) fn confirming(&self) -> bool {
        self.confirm_tunnel.is_some()
    }

    pub(super) fn busy(&self) -> bool {
        self.tabs.iter().any(|t| t.job.is_some()) || !self.tunnels.is_empty()
    }

    fn refresh_ports(&mut self) {
        self.ports_at = Some(Instant::now());
        let table = processes();
        let roots: Vec<(u32, u32)> = self
            .tabs
            .iter()
            .filter_map(|t| t.job.as_ref().and_then(Job::pid).map(|p| (p, t.id)))
            .collect();
        let owner = |mut pid: u32| {
            for _ in 0..32 {
                if let Some((_, tab)) = roots.iter().find(|(root, _)| *root == pid) {
                    return Some(*tab);
                }
                match table.get(&pid) {
                    Some((parent, _)) if *parent != 0 && *parent != pid => pid = *parent,
                    _ => return None,
                }
            }
            None
        };
        self.ports = listeners()
            .into_iter()
            .map(|(port, address, pid)| Port {
                port,
                address,
                pid,
                process: table
                    .get(&pid)
                    .map_or_else(|| format!("pid {pid}"), |(_, n)| n.clone()),
                owner: owner(pid),
            })
            .collect();
    }

    fn ensure_tab(&mut self, root: &Path) {
        if self.tabs.is_empty() {
            self.next += 1;
            self.tabs.push(Term::new(self.next, root.to_path_buf()));
            self.active = 0;
        }
    }

    /// A new project resets idle tabs to its folder; running ones keep going.
    pub(super) fn workspace_changed(&mut self, root: Option<&Path>) {
        if let Some(root) = root {
            for tab in self.tabs.iter_mut().filter(|t| t.job.is_none()) {
                tab.cwd = root.to_path_buf();
            }
        }
    }
}

impl Editor {
    /// Terminal section of the tools panel: tabs, output and the prompt.
    pub(super) fn terminal_view(&mut self, ui: &mut Ui) {
        let Some(root) = self.root.clone() else {
            ui.label(muted(tr("Open a project to run commands.")));
            return;
        };
        let ctx = ui.ctx().clone();
        let terms = &mut self.terminals;
        terms.ensure_tab(&root);
        let mut close = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (i, term) in terms.tabs.iter().enumerate() {
                let running = term.job.is_some();
                let text = format!("{}{}", if running { "● " } else { "" }, term.title());
                let selected = i == terms.active || Some(i) == terms.second;
                let response = ui
                    .selectable_label(selected, RichText::new(text).size(SMALL))
                    .on_hover_text(term.cwd.display().to_string());
                if response.clicked() {
                    terms.active = i;
                    terms.focus_input = true;
                }
                if response.middle_clicked() {
                    close = Some(i);
                }
            }
            if ui
                .small_button("+")
                .on_hover_text(tr("New terminal"))
                .clicked()
            {
                terms.next += 1;
                let cwd = terms
                    .tabs
                    .get(terms.active)
                    .map_or(root.clone(), |t| t.cwd.clone());
                terms.tabs.push(Term::new(terms.next, cwd));
                terms.active = terms.tabs.len() - 1;
                terms.focus_input = true;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .small_button(tr("Close"))
                    .on_hover_text(tr("Close this terminal and stop its process"))
                    .clicked()
                {
                    close = Some(terms.active);
                }
                if terms.tabs.len() > 1 {
                    let split = terms.second.is_some();
                    if ui
                        .selectable_label(split, RichText::new(tr("Split")).size(SMALL))
                        .on_hover_text(tr("Show two terminals side by side"))
                        .clicked()
                    {
                        terms.second = if split {
                            None
                        } else {
                            Some(if terms.active == 0 {
                                1
                            } else {
                                terms.active - 1
                            })
                        };
                    }
                }
            });
        });
        if let Some(i) = close {
            terms.tabs.remove(i);
            terms.second = None;
            terms.active = terms.active.min(terms.tabs.len().saturating_sub(1));
            terms.ensure_tab(&root);
        }
        if terms
            .second
            .is_some_and(|s| s >= terms.tabs.len() || s == terms.active)
        {
            terms.second = None;
        }
        let focus = std::mem::take(&mut terms.focus_input);
        let active = terms.active;
        let preview = match terms.second {
            Some(second) => {
                let (a, b) = if second < active {
                    let (left, right) = terms.tabs.split_at_mut(active);
                    (&mut left[second], &mut right[0])
                } else {
                    let (left, right) = terms.tabs.split_at_mut(second);
                    (&mut right[0], &mut left[active])
                };
                let mut found = None;
                ui.columns(2, |cols| {
                    found = term_pane(&mut cols[0], a, &ctx, false);
                    found = term_pane(&mut cols[1], b, &ctx, focus).or(found.take());
                });
                found
            }
            None => term_pane(ui, &mut terms.tabs[active], &ctx, focus),
        };
        if let Some(url) = preview {
            self.open_preview(&url);
        }
    }

    /// Ports section: listeners, ownership, preview, link, stop and tunnels.
    pub(super) fn ports_view(&mut self, ui: &mut Ui) {
        let terms = &mut self.terminals;
        if terms
            .ports_at
            .is_none_or(|t| t.elapsed() > Duration::from_secs(2))
        {
            terms.refresh_ports();
        }
        ui.ctx().request_repaint_after(Duration::from_secs(2));
        if terms.cloudflared.is_none() {
            terms.cloudflared = Some(cloudflared());
        }
        let has_cloudflared = terms.cloudflared.as_ref().is_some_and(Option::is_some);
        let mut preview = None;
        let mut stop_tab = None;
        ui.horizontal(|ui| {
            checkbox(ui, &mut terms.show_all_ports, tr("Show system ports"));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                match terms.cloudflared.as_ref().and_then(Option::as_ref) {
                    Some(path) => {
                        ui.label(muted(format!("cloudflared · {}", path.display())).size(SMALL));
                    }
                    None => {
                        ui.label(
                            muted(tr(
                                "cloudflared not installed · winget install Cloudflare.cloudflared",
                            ))
                            .size(SMALL),
                        );
                    }
                }
            });
        });
        ScrollArea::vertical()
            .id_salt("ports")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for tunnel in &mut terms.tunnels {
                    egui::Frame::new()
                        .fill(colors().accent_bg)
                        .corner_radius(8)
                        .inner_margin(Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "{} {}",
                                        tr("Public link for port"),
                                        tunnel.port
                                    ))
                                    .strong(),
                                );
                                match &tunnel.url {
                                    Some(url) => {
                                        ui.label(
                                            RichText::new(url)
                                                .monospace()
                                                .size(SMALL)
                                                .color(colors().text),
                                        );
                                        if ui.small_button(tr("Copy")).clicked() {
                                            ui.ctx().copy_text(url.clone());
                                        }
                                    }
                                    None => {
                                        busy_indicator(ui);
                                        ui.label(muted(tr("Creating the temporary link…")));
                                    }
                                }
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.button(tr("Stop tunnel")).clicked() {
                                        tunnel.job.stop.store(true, Ordering::Relaxed);
                                    }
                                });
                            });
                            if tunnel.url.is_none() && !tunnel.log.is_empty() {
                                let tail: Vec<&str> = tunnel.log.lines().rev().take(2).collect();
                                ui.label(muted(tail.join(" · ")).size(SMALL));
                            }
                        });
                    ui.add_space(4.0);
                }
                let mut rows: Vec<&Port> = terms
                    .ports
                    .iter()
                    .filter(|p| {
                        terms.show_all_ports
                            || p.owner.is_some()
                            || (p.port >= 1024 && !SYSTEM.contains(&p.process.as_str()))
                    })
                    .collect();
                rows.sort_by_key(|p| (p.owner.is_none(), p.port));
                if rows.is_empty() {
                    ui.label(muted(tr(
                        "No listening ports. Run a dev server (npm run dev) in a terminal.",
                    )));
                    return;
                }
                egui::Grid::new("ports-grid")
                    .num_columns(5)
                    .spacing([16.0, 6.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for title in ["Port", "Address", "Process", "Status", ""] {
                            ui.label(muted(tr(title)).size(SMALL));
                        }
                        ui.end_row();
                        for port in rows {
                            let url = format!("http://localhost:{}", port.port);
                            ui.label(RichText::new(port.port.to_string()).monospace().strong());
                            ui.label(RichText::new(&port.address).monospace().size(SMALL));
                            ui.label(
                                RichText::new(format!("{} · {}", port.process, port.pid))
                                    .size(SMALL),
                            );
                            match port.owner {
                                Some(tab) => ui.label(
                                    RichText::new(format!("Covalos · terminal {tab}"))
                                        .size(SMALL)
                                        .color(colors().success),
                                ),
                                None => ui.label(muted(tr("External")).size(SMALL)),
                            };
                            ui.horizontal(|ui| {
                                if ui.small_button(tr("Preview")).clicked() {
                                    preview = Some(url.clone());
                                }
                                if ui.small_button(tr("Copy link")).clicked() {
                                    ui.ctx().copy_text(url.clone());
                                }
                                let tunneled = terms.tunnels.iter().any(|t| t.port == port.port);
                                if has_cloudflared
                                    && !tunneled
                                    && ui
                                        .small_button(tr("Share…"))
                                        .on_hover_text(tr(
                                            "Create a temporary public link with Cloudflare Tunnel",
                                        ))
                                        .clicked()
                                {
                                    terms.confirm_tunnel = Some(port.port);
                                }
                                if let Some(tab) = port.owner
                                    && ui
                                        .small_button(tr("Stop"))
                                        .on_hover_text(tr(
                                            "Stop the terminal process that owns this port",
                                        ))
                                        .clicked()
                                {
                                    stop_tab = Some(tab);
                                }
                            });
                            ui.end_row();
                        }
                    });
            });
        if let Some(tab) = stop_tab
            && let Some(job) = terms
                .tabs
                .iter()
                .find(|t| t.id == tab)
                .and_then(|t| t.job.as_ref())
        {
            job.stop.store(true, Ordering::Relaxed);
        }
        if let Some(url) = preview {
            self.open_preview(&url);
        }
    }

    /// Confirmation before anything is published through a tunnel.
    pub(super) fn tunnel_modal(&mut self, ctx: &egui::Context) {
        let Some(port) = self.terminals.confirm_tunnel else {
            return;
        };
        let mut answer = None;
        let modal = egui::Modal::new(egui::Id::new("tunnel-confirm"))
            .frame(modal_frame())
            .show(ctx, |ui| {
                ui.set_width(440.0);
                ui.label(
                    RichText::new(tr("Share this port publicly?"))
                        .size(HEADING)
                        .strong(),
                );
                ui.add_space(6.0);
                ui.label(secondary(format!(
                    "{} http://localhost:{port}. {}",
                    tr("Cloudflare creates a temporary trycloudflare.com address that forwards to"),
                    tr("Anyone with the link can open it until you stop the tunnel. No Cloudflare account is used.")
                )));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.add(primary(tr("Create link"))).clicked() {
                        answer = Some(true);
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        answer = Some(false);
                    }
                });
            });
        if modal.should_close() && answer.is_none() {
            answer = Some(false);
        }
        let Some(yes) = answer else { return };
        self.terminals.confirm_tunnel = None;
        if yes {
            self.start_tunnel(port, ctx);
        }
    }

    /// Starts `cloudflared tunnel --url` for a port the user confirmed.
    fn start_tunnel(&mut self, port: u16, ctx: &egui::Context) {
        let Some(Some(program)) = self.terminals.cloudflared.clone() else {
            return;
        };
        let mut command = Command::new(program);
        command.args([
            "tunnel",
            "--no-autoupdate",
            "--url",
            &format!("http://localhost:{port}"),
        ]);
        let job = spawn(command, format!("cloudflared {port}"), ctx.clone());
        self.terminals.tunnels.push(Tunnel {
            port,
            url: None,
            log: String::new(),
            job,
            done: false,
        });
    }
}

/// One terminal: scrollback, a status line and the prompt. Returns a URL the
/// user asked to preview.
fn term_pane(ui: &mut Ui, term: &mut Term, ctx: &egui::Context, focus: bool) -> Option<String> {
    let mut preview = None;
    let input_height = 30.0;
    let status_height = if term.job.is_some() || !term.urls.is_empty() {
        26.0
    } else {
        0.0
    };
    let height = (ui.available_height() - input_height - status_height).max(40.0);
    egui::Frame::new()
        .fill(colors().inset)
        .corner_radius(6)
        .inner_margin(Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ScrollArea::vertical()
                .id_salt(("term-log", term.id))
                .max_height(height - 12.0)
                .stick_to_bottom(true)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if term.log.is_empty() {
                        ui.label(
                            muted(tr(
                                "PowerShell · type a command, e.g. npm install or npm run dev",
                            ))
                            .size(SMALL),
                        );
                    }
                    ui.add(
                        egui::Label::new(
                            RichText::new(&term.log)
                                .monospace()
                                .size(12.0)
                                .color(colors().text),
                        )
                        .wrap()
                        .selectable(true),
                    );
                });
        });
    if status_height > 0.0 {
        ui.horizontal(|ui| {
            if let Some(job) = &term.job {
                busy_indicator(ui);
                let secs = job.started.elapsed().as_secs();
                ui.label(
                    muted(format!("{} · {}:{:02}", job.command, secs / 60, secs % 60)).size(SMALL),
                );
            }
            for url in term.urls.iter().take(2) {
                if ui
                    .small_button(format!("{} {url}", tr("Preview")))
                    .on_hover_text(tr("Open in the Covalos preview"))
                    .clicked()
                {
                    preview = Some(url.clone());
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if let Some(job) = &term.job
                    && ui
                        .button(tr("Stop"))
                        .on_hover_text(tr("Stop the process and its children"))
                        .clicked()
                {
                    job.stop.store(true, Ordering::Relaxed);
                }
            });
        });
    }
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("PS>")
                .monospace()
                .size(12.0)
                .color(colors().accent),
        );
        let enabled = term.job.is_none();
        let id = egui::Id::new(("term-input", term.id));
        let history_key = if enabled && ui.memory(|m| m.has_focus(id)) {
            ui.input(|i| {
                if i.key_pressed(Key::ArrowUp) {
                    Some(true)
                } else if i.key_pressed(Key::ArrowDown) {
                    Some(false)
                } else {
                    None
                }
            })
        } else {
            None
        };
        if let Some(up) = history_key
            && !term.history.is_empty()
        {
            let last = term.history.len() - 1;
            let next = match (term.recall, up) {
                (None, true) => Some(last),
                (Some(i), true) => Some(i.saturating_sub(1)),
                (Some(i), false) if i < last => Some(i + 1),
                _ => None,
            };
            term.recall = next;
            term.input = next.map(|i| term.history[i].clone()).unwrap_or_default();
        }
        let hint = if enabled {
            term.cwd.display().to_string()
        } else {
            tr("Running… open another tab with + to keep working").to_owned()
        };
        let response = ui.add_enabled(
            enabled,
            TextEdit::singleline(&mut term.input)
                .id(id)
                .font(egui::TextStyle::Monospace)
                .hint_text(hint)
                .desired_width(f32::INFINITY)
                .char_limit(8192),
        );
        if focus {
            response.request_focus();
        }
        if enabled && response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
            term.run(ctx);
            response.request_focus();
        }
    });
    preview
}

#[cfg(test)]
impl Editor {
    pub(super) fn harness_run(&mut self, command: &str, ctx: &egui::Context) {
        let root = self.root.clone().expect("project");
        let terms = &mut self.terminals;
        terms.next += 1;
        let mut term = Term::new(terms.next, root);
        term.input = command.to_owned();
        term.run(ctx);
        terms.tabs.push(term);
        terms.active = terms.tabs.len() - 1;
    }

    pub(super) fn harness_url(&self) -> Option<String> {
        self.terminals
            .tabs
            .iter()
            .flat_map(|t| t.urls.iter())
            .next()
            .cloned()
    }

    pub(super) fn harness_ask_tunnel(&mut self, port: u16) {
        self.terminals.cloudflared = Some(cloudflared());
        self.terminals.confirm_tunnel = Some(port);
    }

    /// What the "Create link" button does after the confirmation.
    pub(super) fn harness_confirm_tunnel(&mut self, ctx: &egui::Context) {
        if let Some(port) = self.terminals.confirm_tunnel.take() {
            self.start_tunnel(port, ctx);
        }
    }

    pub(super) fn harness_tunnel_url(&self) -> Option<String> {
        self.terminals.tunnels.iter().find_map(|t| t.url.clone())
    }

    pub(super) fn harness_stop_all(&mut self) {
        for tunnel in &self.terminals.tunnels {
            tunnel.job.stop.store(true, Ordering::Relaxed);
        }
        for tab in &self.terminals.tabs {
            if let Some(job) = &tab.job {
                job.stop.store(true, Ordering::Relaxed);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_and_ansi_are_parsed_from_dev_server_output() {
        let line = "\x1b[32m  ➜  Local:\x1b[0m   http://localhost:5173/\r\n  Network: http://0.0.0.0:3000.";
        let clean = strip_ansi(line);
        assert!(!clean.contains('\x1b'));
        assert_eq!(
            local_urls(&clean),
            ["http://localhost:5173/", "http://localhost:3000"]
        );
        assert_eq!(
            trycloudflare("INF |  https://quiet-river-12.trycloudflare.com  |"),
            Some("https://quiet-river-12.trycloudflare.com".into())
        );
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"Man"), "TWFu");
    }

    #[cfg(windows)]
    #[test]
    fn powershell_runs_in_the_tab_directory_and_cd_is_tracked() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("web")).unwrap();
        let ctx = egui::Context::default();
        let mut term = Term::new(1, dir.path().to_path_buf());
        term.input = "cd web".into();
        term.run(&ctx);
        assert!(term.cwd.ends_with("web"), "{}", term.cwd.display());
        term.input = "Write-Output \"$((Get-Location).Path)|ok\"".into();
        term.run(&ctx);
        let until = Instant::now() + Duration::from_secs(30);
        while term.job.is_some() && Instant::now() < until {
            term.poll();
            thread::sleep(Duration::from_millis(20));
        }
        assert!(term.log.contains("web|ok"), "{}", term.log);
    }

    #[cfg(windows)]
    #[test]
    fn a_listening_port_is_attributed_to_its_terminal() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let me = std::process::id();
        let found = listeners();
        assert!(
            found.iter().any(|(p, _, pid)| *p == port && *pid == me),
            "port {port} not listed for pid {me}"
        );
        assert!(processes().contains_key(&me));
    }
}

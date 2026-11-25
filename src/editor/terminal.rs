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
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use eframe::egui::{self, Align, Layout, Margin, RichText, ScrollArea, Ui};

use super::{Editor, theme::*};

const MAX_LOG: usize = 200_000;

enum Event {
    Output(String),
    Finished(String),
}

pub(super) struct Job {
    events: mpsc::Receiver<Event>,
    stop: Arc<AtomicBool>,
}

impl Drop for Job {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
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
    tabs: Vec<super::pty::Shell>,
    error: Option<String>,
    preview: Option<String>,
    busy_at: Option<Instant>,
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

pub(super) fn strip_ansi(text: &str) -> String {
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
pub(super) fn local_urls(text: &str) -> Vec<String> {
    use std::sync::OnceLock;
    static URL: OnceLock<regex::Regex> = OnceLock::new();
    let re = URL.get_or_init(|| {
        regex::Regex::new(
            r"https?://(?:localhost|127\.0\.0\.1|\[::1\]|0\.0\.0\.0)(?::\d{2,5})(?:/[^\s'\x22<>]*)?",
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

fn spawn(mut command: Command, ctx: egui::Context) -> Job {
    let (tx, events) = mpsc::sync_channel(512);
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
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
    Job { events, stop }
}

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
        if !self.tabs.is_empty()
            && self
                .busy_at
                .is_none_or(|t| t.elapsed() > Duration::from_millis(900))
        {
            self.busy_at = Some(Instant::now());
            let table = processes();
            for tab in &mut self.tabs {
                let Some(pid) = tab.pid() else { continue };
                tab.busy = table
                    .iter()
                    .find(|(_, (parent, name))| *parent == pid && name != "conhost.exe")
                    .map(|(_, (_, name))| name.clone());
            }
        }
        self.tabs.retain(|t| !t.exited());
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len().saturating_sub(1);
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
                    Event::Finished(status) => {
                        append(&mut tunnel.log, &status);
                        tunnel.done = true;
                    }
                }
            }
        }
        self.tunnels.retain(|t| !t.done);
    }

    #[cfg(windows)]
    pub(super) fn confirming(&self) -> bool {
        self.confirm_tunnel.is_some()
    }

    pub(super) fn busy(&self) -> bool {
        self.tabs.iter().any(|t| t.busy.is_some()) || !self.tunnels.is_empty()
    }

    fn refresh_ports(&mut self) {
        self.ports_at = Some(Instant::now());
        let table = processes();
        let roots: Vec<(u32, u32)> = self
            .tabs
            .iter()
            .filter_map(|t| t.pid().map(|p| (p, t.id)))
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

    fn ensure_tab(&mut self, root: &Path, ctx: &egui::Context) {
        if self.tabs.is_empty() && self.error.is_none() && self.next == 0 {
            self.open_tab(root.to_path_buf(), ctx);
        }
    }

    pub(super) fn active_directory(&self) -> Option<&Path> {
        self.tabs.get(self.active).map(|tab| tab.cwd.as_path())
    }

    pub(super) fn open_tab(&mut self, cwd: PathBuf, ctx: &egui::Context) {
        self.next += 1;
        match super::pty::Shell::spawn(self.next, cwd, ctx) {
            Ok(shell) => {
                self.tabs.push(shell);
                self.active = self.tabs.len() - 1;
                self.focus_input = true;
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// A new project closes idle shells; busy ones (servers) keep running.
    pub(super) fn workspace_changed(&mut self, _root: Option<&Path>) {
        self.tabs.retain(|t| t.busy.is_some());
        self.active = 0;
        self.second = None;
        if self.tabs.is_empty() {
            self.next = 0;
        }
    }
}

impl Editor {
    /// Terminal section of the tools panel: tabs of interactive shells.
    pub(super) fn terminal_view(&mut self, ui: &mut Ui) {
        let Some(root) = self
            .root
            .clone()
            .or_else(|| self.editor_directory())
            .or_else(|| self.terminals.active_directory().map(Path::to_owned))
        else {
            ui.label(muted(tr("Open a project to run commands.")));
            return;
        };
        let ctx = ui.ctx().clone();
        let terms = &mut self.terminals;
        terms.ensure_tab(&root, &ctx);
        let mut close = None;
        let mut add = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (i, term) in terms.tabs.iter().enumerate() {
                let text = format!(
                    "{}{}",
                    if term.busy.is_some() { "• " } else { "" },
                    term.title()
                );
                let selected = i == terms.active || Some(i) == terms.second;
                let btn = egui::Button::new(RichText::new(text).size(SMALL).color(if selected {
                    colors().text
                } else {
                    colors().secondary
                }))
                .fill(if selected {
                    colors().elevated
                } else {
                    egui::Color32::TRANSPARENT
                })
                .stroke(if selected {
                    egui::Stroke::new(1.0_f32, colors().border)
                } else {
                    egui::Stroke::NONE
                })
                .corner_radius(10);
                let response = ui.add(btn).on_hover_text(term.cwd.display().to_string());
                if response.clicked() {
                    terms.active = i;
                    terms.focus_input = true;
                }
                if response.middle_clicked() {
                    close = Some(i);
                }
            }
            if icon_button(ui, Icon::Add, "New terminal", false).clicked() {
                add = true;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                if !terms.tabs.is_empty() && icon_button(ui, Icon::Close, "Close", false).clicked()
                {
                    close = Some(terms.active);
                }
                if terms.tabs.len() > 1 {
                    let split = terms.second.is_some();
                    if icon_button(ui, Icon::Layers, "Split", split).clicked() {
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
                if let Some(term) = terms.tabs.get(terms.active) {
                    if icon_button(ui, Icon::Copy, "Copy output", false).clicked() {
                        ui.ctx().copy_text(term.contents());
                    }
                    if icon_button(ui, Icon::Stop, "Interrupt the running program", false).clicked()
                    {
                        term.interrupt();
                        terms.focus_input = true;
                    }
                    for url in term.urls().into_iter().rev().take(2) {
                        let preview_btn = egui::Button::new(
                            RichText::new(format!("{} {url}", tr("Preview")))
                                .size(SMALL)
                                .color(colors().accent),
                        )
                        .fill(colors().elevated)
                        .stroke(egui::Stroke::new(1.0_f32, colors().border))
                        .corner_radius(8);
                        if ui
                            .add(preview_btn)
                            .on_hover_text(tr("Open in the Covalos preview"))
                            .clicked()
                        {
                            terms.preview = Some(url);
                        }
                    }
                }
            });
        });
        if add {
            terms.open_tab(root.clone(), &ctx);
        }
        if let Some(i) = close.filter(|i| *i < terms.tabs.len()) {
            terms.tabs.remove(i);
            terms.second = None;
            terms.active = terms.active.min(terms.tabs.len().saturating_sub(1));
        }
        if terms
            .second
            .is_some_and(|s| s >= terms.tabs.len() || s == terms.active)
        {
            terms.second = None;
        }
        if let Some(error) = &terms.error {
            ui.colored_label(colors().error, error.clone());
        }
        if terms.tabs.is_empty() {
            if ui.button(tr("New terminal")).clicked() {
                terms.open_tab(root, &ctx);
            }
            return;
        }
        let focus = std::mem::take(&mut terms.focus_input);
        ui.add_space(4.0);
        let rect = ui.available_rect_before_wrap();
        let active = terms.active;
        match terms.second {
            Some(second) => {
                let gap = 8.0;
                let half = (rect.width() - gap) / 2.0;
                let left = egui::Rect::from_min_size(rect.min, egui::vec2(half, rect.height()));
                let right =
                    egui::Rect::from_min_max(egui::pos2(left.right() + gap, rect.top()), rect.max);
                terms.tabs[second].show(ui, left, false);
                terms.tabs[active].show(ui, right, focus);
            }
            None => terms.tabs[active].show(ui, rect, focus),
        }
        ui.allocate_rect(rect, egui::Sense::hover());
        if let Some(url) = self.terminals.preview.take() {
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
            && let Some(term) = terms.tabs.iter().find(|t| t.id == tab)
        {
            term.interrupt();
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
        let job = spawn(command, ctx.clone());
        self.terminals.tunnels.push(Tunnel {
            port,
            url: None,
            log: String::new(),
            job,
            done: false,
        });
    }
}

#[cfg(all(test, windows))]
impl Editor {
    pub(super) fn harness_open_terminal(&mut self, ctx: &egui::Context) {
        let root = self.root.clone().expect("project");
        self.terminals.open_tab(root, ctx);
    }

    pub(super) fn harness_prompt_ready(&self) -> bool {
        self.terminals
            .tabs
            .last()
            .is_some_and(|t| t.contents().contains("PS "))
    }

    pub(super) fn harness_type(&mut self, text: &str) {
        if let Some(term) = self.terminals.tabs.last() {
            term.write(text.as_bytes());
        }
    }

    pub(super) fn harness_url(&self) -> Option<String> {
        self.terminals.tabs.iter().flat_map(|t| t.urls()).next()
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
        self.terminals.tabs.clear();
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

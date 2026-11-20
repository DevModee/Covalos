//! One interactive terminal tab: a persistent PowerShell running inside a
//! pseudo console (ConPTY on Windows, via `portable-pty`), parsed into a
//! screen grid by `vt100`, and drawn with egui. Keys go straight to the
//! shell, so prompts, line editing, Ctrl+C and full-screen programs work.
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use eframe::egui::{
    self, Color32, Event, FontId, Key, Rect, Sense, Ui, text::LayoutJob, text::TextFormat,
};
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};

use super::theme::colors;

const SCROLLBACK: usize = 5_000;
const FONT: f32 = 12.5;

type Writer = Arc<Mutex<Box<dyn Write + Send>>>;

pub(super) struct Shell {
    pub(super) id: u32,
    pub(super) cwd: PathBuf,
    parser: Arc<Mutex<vt100::Parser>>,
    writer: Writer,
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    pid: Option<u32>,
    exited: Arc<AtomicBool>,
    urls: Arc<Mutex<Vec<String>>>,
    size: (u16, u16),
    scroll: f32,
    /// A program started by the shell is running (set from the process table).
    pub(super) busy: Option<String>,
}

impl Drop for Shell {
    fn drop(&mut self) {
        self.kill();
    }
}

/// Every shell joins one kill-on-close job, so a crash never leaves dev
/// servers running.
#[cfg(windows)]
fn contain(pid: u32) {
    use std::sync::OnceLock;
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::{
            JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                SetInformationJobObject,
            },
            Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE},
        },
    };
    static JOB: OnceLock<usize> = OnceLock::new();
    let job = *JOB.get_or_init(|| {
        // SAFETY: plain Win32 calls; the job handle lives for the whole process.
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            job as usize
        }
    });
    if job == 0 {
        return;
    }
    // SAFETY: the process handle is closed right after the assignment.
    unsafe {
        let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
        if !process.is_null() {
            AssignProcessToJobObject(job as _, process);
            CloseHandle(process);
        }
    }
}

#[cfg(not(windows))]
fn contain(_pid: u32) {}

fn send(writer: &Writer, bytes: &[u8]) {
    if let Ok(mut w) = writer.lock() {
        let _ = w.write_all(bytes);
        let _ = w.flush();
    }
}

fn shell_command(cwd: &Path) -> CommandBuilder {
    #[cfg(windows)]
    let mut cmd = if let Ok(shell) = std::env::var("COVALOS_SHELL") {
        CommandBuilder::new(shell)
    } else {
        let mut c = CommandBuilder::new("powershell.exe");
        c.args(["-NoLogo", "-ExecutionPolicy", "Bypass"]);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = CommandBuilder::new(std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into()));
    cmd.cwd(cwd);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("BROWSER", "none");
    cmd
}

impl Shell {
    pub(super) fn spawn(id: u32, cwd: PathBuf, ctx: &egui::Context) -> Result<Self, String> {
        // "Ignore Ctrl+C" is inherited from whoever launched Covalos (any parent
        // started with CREATE_NEW_PROCESS_GROUP). Clear it before the shell is
        // created so the shell and its programs receive Ctrl+C.
        #[cfg(windows)]
        // SAFETY: plain Win32 call with a null handler.
        unsafe {
            windows_sys::Win32::System::Console::SetConsoleCtrlHandler(None, 0);
        }
        let size = (30_u16, 100_u16);
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: size.0,
                cols: size.1,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("Could not open a pseudo console: {e}"))?;
        let child = pair
            .slave
            .spawn_command(shell_command(&crate::agents_runtime::plain(&cwd)))
            .map_err(|e| format!("Could not start PowerShell: {e}"))?;
        drop(pair.slave);
        let pid = child.process_id();
        if let Some(pid) = pid {
            contain(pid);
        }
        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer: Writer = Arc::new(Mutex::new(
            pair.master.take_writer().map_err(|e| e.to_string())?,
        ));
        let parser = Arc::new(Mutex::new(vt100::Parser::new(size.0, size.1, SCROLLBACK)));
        let exited = Arc::new(AtomicBool::new(false));
        let urls = Arc::new(Mutex::new(Vec::new()));
        {
            let (parser, writer, exited, urls, ctx) = (
                parser.clone(),
                writer.clone(),
                exited.clone(),
                urls.clone(),
                ctx.clone(),
            );
            let drawn = Arc::new(AtomicBool::new(false));
            thread::spawn(move || {
                let mut buffer = [0_u8; 16 * 1024];
                let mut tail = String::new();
                loop {
                    let n = match reader.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    let chunk = &buffer[..n];
                    if std::env::var_os("COVALOS_PTY_TRACE").is_some() {
                        eprintln!("pty <- {:?}", String::from_utf8_lossy(chunk));
                    }
                    // ConPTY may ask where the cursor is before it starts drawing.
                    if chunk.windows(4).any(|w| w == b"\x1b[6n") {
                        // Answered once, a moment later: ConPTY asks before it
                        // reads its input, and an early reply is lost.
                        let (row, col) = parser
                            .lock()
                            .map(|p| p.screen().cursor_position())
                            .unwrap_or((0, 0));
                        let reply = format!("\x1b[{};{}R", row + 1, col + 1);
                        send(&writer, reply.as_bytes());
                        // ConPTY (cursor inheritance) often keeps waiting after the
                        // first answer and later echoes it as typed text. If nothing
                        // is drawn within a second, answer again; once the prompt is
                        // up, Esc clears the stray text from the input line.
                        let (writer, drawn) = (writer.clone(), drawn.clone());
                        thread::spawn(move || {
                            thread::sleep(std::time::Duration::from_millis(1000));
                            if drawn.load(Ordering::Relaxed) {
                                return;
                            }
                            send(&writer, reply.as_bytes());
                            for _ in 0..100 {
                                if drawn.load(Ordering::Relaxed) {
                                    thread::sleep(std::time::Duration::from_millis(300));
                                    send(&writer, b"\x1b");
                                    return;
                                }
                                thread::sleep(std::time::Duration::from_millis(50));
                            }
                        });
                    }
                    if chunk != b"\x1b[6n" {
                        drawn.store(true, Ordering::Relaxed);
                    }
                    if let Ok(mut p) = parser.lock() {
                        p.process(chunk);
                    }
                    let text = super::terminal::strip_ansi(&String::from_utf8_lossy(chunk));
                    tail.push_str(&text);
                    if let Ok(mut list) = urls.lock() {
                        for url in super::terminal::local_urls(&tail) {
                            if !list.contains(&url) {
                                list.push(url);
                            }
                        }
                    }
                    let keep = tail.len().saturating_sub(256);
                    let keep = tail.ceil_char_boundary(keep);
                    tail.drain(..keep);
                    ctx.request_repaint();
                }
                exited.store(true, Ordering::Relaxed);
                ctx.request_repaint();
            });
        }
        Ok(Self {
            id,
            cwd,
            parser,
            writer,
            master: pair.master,
            child,
            pid,
            exited,
            urls,
            size,
            scroll: 0.0,
            busy: None,
        })
    }

    pub(super) fn pid(&self) -> Option<u32> {
        self.pid
    }

    pub(super) fn exited(&self) -> bool {
        self.exited.load(Ordering::Relaxed)
    }

    pub(super) fn urls(&self) -> Vec<String> {
        self.urls.lock().map(|u| u.clone()).unwrap_or_default()
    }

    pub(super) fn write(&self, bytes: &[u8]) {
        if let Ok(mut w) = self.writer.lock() {
            let _ = w.write_all(bytes);
            let _ = w.flush();
        }
    }

    /// Ctrl+C, as the keyboard would send it.
    pub(super) fn interrupt(&self) {
        self.write(b"\x03");
    }

    /// Screen text, including what scrolled off.
    pub(super) fn contents(&self) -> String {
        self.parser
            .lock()
            .map(|p| p.screen().contents())
            .unwrap_or_default()
    }

    pub(super) fn title(&self) -> String {
        if let Some(program) = &self.busy {
            return program.trim_end_matches(".exe").to_owned();
        }
        self.cwd.file_name().map_or_else(
            || "PowerShell".to_owned(),
            |n| n.to_string_lossy().into_owned(),
        )
    }

    pub(super) fn kill(&mut self) {
        if let Some(pid) = self.pid.take() {
            #[cfg(windows)]
            {
                let mut command = std::process::Command::new("taskkill.exe");
                crate::process::configure(&mut command);
                let _ = command
                    .args(["/PID", &pid.to_string(), "/T", "/F"])
                    .output();
            }
            #[cfg(not(windows))]
            let _ = pid;
            let _ = self.child.kill();
        }
    }

    fn resize(&mut self, rows: u16, cols: u16) {
        if (rows, cols) == self.size || rows < 2 || cols < 10 {
            return;
        }
        self.size = (rows, cols);
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
        if let Ok(mut p) = self.parser.lock() {
            p.set_size(rows, cols);
        }
    }

    /// Draws the screen into `rect` and forwards keyboard input while focused.
    pub(super) fn show(&mut self, ui: &mut Ui, rect: Rect, focus: bool) {
        let font = FontId::monospace(FONT);
        let (cell_w, cell_h) = ui.fonts_mut(|f| (f.glyph_width(&font, 'M'), f.row_height(&font)));
        let inner = rect.shrink2(egui::vec2(8.0, 6.0));
        let cols = (inner.width() / cell_w).floor().clamp(10.0, 500.0) as u16;
        let rows = (inner.height() / cell_h).floor().clamp(2.0, 300.0) as u16;
        self.resize(rows, cols);

        let id = egui::Id::new(("pty", self.id));
        let response = ui.interact(rect, id, Sense::click());
        if response.clicked() || focus {
            response.request_focus();
        }
        let focused = response.has_focus();
        if focused {
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    id,
                    egui::EventFilter {
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: true,
                    },
                )
            });
            self.input(ui);
        }
        if response.hovered() {
            let dy = ui.input(|i| i.smooth_scroll_delta.y);
            if dy != 0.0 {
                self.scroll = (self.scroll + dy / cell_h).max(0.0);
            }
        }

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 6, colors().inset);
        if focused {
            painter.rect_stroke(
                rect,
                6,
                egui::Stroke::new(1.0_f32, colors().accent.gamma_multiply(0.45)),
                egui::StrokeKind::Inside,
            );
        }
        let Ok(mut parser) = self.parser.lock() else {
            return;
        };
        parser.set_scrollback(self.scroll.round() as usize);
        let applied = parser.screen().scrollback() as f32;
        self.scroll = self.scroll.min(applied.max(0.0) + 0.99);
        let screen = parser.screen();
        let (rows, cols) = screen.size();
        for row in 0..rows {
            let y = inner.top() + f32::from(row) * cell_h;
            let mut job = LayoutJob::default();
            for col in 0..cols {
                let Some(cell) = screen.cell(row, col) else {
                    continue;
                };
                if cell.is_wide_continuation() {
                    continue;
                }
                let (mut fg, mut bg) = (color(cell.fgcolor(), true), color(cell.bgcolor(), false));
                if cell.inverse() {
                    std::mem::swap(&mut fg, &mut bg);
                    fg = fg.or(Some(colors().inset));
                    bg = bg.or(Some(colors().text));
                }
                if let Some(bg) = bg {
                    let x = inner.left() + f32::from(col) * cell_w;
                    let width = if cell.is_wide() { 2.0 } else { 1.0 } * cell_w;
                    painter.rect_filled(
                        Rect::from_min_size(egui::pos2(x, y), egui::vec2(width, cell_h)),
                        0,
                        bg,
                    );
                }
                let text = if cell.has_contents() {
                    cell.contents()
                } else {
                    " ".to_owned()
                };
                let mut fg = fg.unwrap_or(colors().text);
                if cell.bold() {
                    fg = super::motion::blend(fg, Color32::WHITE, 0.25);
                }
                job.append(
                    &text,
                    0.0,
                    TextFormat {
                        font_id: font.clone(),
                        color: fg,
                        ..Default::default()
                    },
                );
            }
            let galley = ui.fonts_mut(|f| f.layout_job(job));
            painter.galley(egui::pos2(inner.left(), y), galley, colors().text);
        }
        if focused && applied == 0.0 && !screen.hide_cursor() {
            let (row, col) = screen.cursor_position();
            let at = egui::pos2(
                inner.left() + f32::from(col) * cell_w,
                inner.top() + f32::from(row) * cell_h,
            );
            painter.rect_filled(
                Rect::from_min_size(at, egui::vec2(cell_w, cell_h)),
                1,
                colors().caret.gamma_multiply(0.7),
            );
        }
        if applied > 0.0 {
            painter.text(
                rect.right_top() + egui::vec2(-10.0, 6.0),
                egui::Align2::RIGHT_TOP,
                format!("↑ {applied:.0}"),
                FontId::proportional(11.0),
                colors().muted,
            );
        }
    }

    fn input(&mut self, ui: &mut Ui) {
        let events = ui.input(|i| i.events.clone());
        let mut bytes: Vec<u8> = Vec::new();
        for event in events {
            match event {
                Event::Text(text) => bytes.extend_from_slice(text.as_bytes()),
                Event::Paste(text) => bytes
                    .extend_from_slice(text.replace("\r\n", "\r").replace('\n', "\r").as_bytes()),
                Event::Copy => bytes.push(0x03),
                Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    if let Some(seq) = key_bytes(key, modifiers) {
                        if key == Key::PageUp && modifiers.shift {
                            self.scroll += f32::from(self.size.0);
                            continue;
                        }
                        if key == Key::PageDown && modifiers.shift {
                            self.scroll = (self.scroll - f32::from(self.size.0)).max(0.0);
                            continue;
                        }
                        bytes.extend_from_slice(&seq);
                    }
                }
                _ => {}
            }
        }
        if !bytes.is_empty() {
            self.scroll = 0.0;
            self.write(&bytes);
        }
    }
}

/// Bytes a VT terminal sends for a key; `None` for keys handled elsewhere.
fn key_bytes(key: Key, m: egui::Modifiers) -> Option<Vec<u8>> {
    let seq: &[u8] = match key {
        Key::Enter => b"\r",
        Key::Backspace => b"\x7f",
        Key::Tab if m.shift => b"\x1b[Z",
        Key::Tab => b"\t",
        Key::Escape => b"\x1b",
        Key::ArrowUp => b"\x1b[A",
        Key::ArrowDown => b"\x1b[B",
        Key::ArrowRight if m.ctrl => b"\x1b[1;5C",
        Key::ArrowLeft if m.ctrl => b"\x1b[1;5D",
        Key::ArrowRight => b"\x1b[C",
        Key::ArrowLeft => b"\x1b[D",
        Key::Home => b"\x1b[H",
        Key::End => b"\x1b[F",
        Key::Delete => b"\x1b[3~",
        Key::Insert => b"\x1b[2~",
        Key::PageUp => b"\x1b[5~",
        Key::PageDown => b"\x1b[6~",
        _ if m.ctrl && !m.alt => {
            // Ctrl+C/V/X arrive as Copy/Paste/Cut events.
            let name = key.name();
            let letter = name
                .chars()
                .next()
                .filter(|c| name.len() == 1 && c.is_ascii_alphabetic())?;
            if matches!(letter, 'C' | 'V' | 'X') {
                return None;
            }
            return Some(vec![(letter as u8) & 0x1f]);
        }
        _ => return None,
    };
    Some(seq.to_vec())
}

/// Terminal colors mapped onto the Nocturne palette for the 16 base colors.
fn color(c: vt100::Color, fg: bool) -> Option<Color32> {
    const BASE: [(u8, u8, u8); 16] = [
        (40, 42, 54),
        (240, 98, 120),
        (120, 214, 160),
        (240, 200, 110),
        (120, 160, 255),
        (196, 146, 255),
        (110, 210, 230),
        (205, 208, 222),
        (110, 114, 140),
        (255, 128, 146),
        (150, 236, 186),
        (255, 220, 140),
        (150, 186, 255),
        (216, 176, 255),
        (140, 228, 244),
        (245, 246, 252),
    ];
    match c {
        vt100::Color::Default => None,
        vt100::Color::Idx(i) if i < 16 => {
            let (r, g, b) = BASE[usize::from(i)];
            if !fg && i == 0 {
                return None;
            }
            Some(Color32::from_rgb(r, g, b))
        }
        vt100::Color::Idx(i) if i < 232 => {
            let i = i - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            Some(Color32::from_rgb(
                level(i / 36),
                level((i / 6) % 6),
                level(i % 6),
            ))
        }
        vt100::Color::Idx(i) => {
            let v = 8 + (i - 232) * 10;
            Some(Color32::from_rgb(v, v, v))
        }
        vt100::Color::Rgb(r, g, b) => Some(Color32::from_rgb(r, g, b)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn wait_for(shell: &Shell, needle: &str, secs: u64) -> bool {
        let until = Instant::now() + Duration::from_secs(secs);
        while Instant::now() < until {
            if shell.contents().contains(needle) {
                return true;
            }
            thread::sleep(Duration::from_millis(50));
        }
        false
    }

    /// A real PowerShell in a pseudo console: a prompt, typed commands,
    /// Ctrl+C on a running program, and `cd` handled by the shell itself.
    #[cfg(windows)]
    #[test]
    fn powershell_is_interactive_and_ctrl_c_interrupts() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("web")).unwrap();
        let ctx = egui::Context::default();
        let mut shell = Shell::spawn(1, dir.path().to_path_buf(), &ctx).unwrap();
        assert!(
            wait_for(&shell, "PS ", 30),
            "no prompt: {}",
            shell.contents()
        );
        shell.write(b"cd web; Write-Output ('co'+'valos-'+(Split-Path -Leaf (Get-Location)))\r");
        assert!(wait_for(&shell, "covalos-web", 30), "{}", shell.contents());
        shell.write(b"ping -n 60 127.0.0.1\r");
        assert!(wait_for(&shell, "127.0.0.1", 30), "{}", shell.contents());
        shell.interrupt();
        assert!(wait_for(&shell, "Control-C", 30), "{}", shell.contents());
        // The console drops input typed while it handles Ctrl+C.
        thread::sleep(Duration::from_millis(800));
        shell.write(b"Write-Output ('after'+'-ctrl-c')\r");
        assert!(wait_for(&shell, "after-ctrl-c", 30), "{}", shell.contents());
        shell.write(
            b"node -e \"console.log('  Local:   http://localhost:' + (5100 + 99) + '/')\"\r",
        );
        assert!(
            wait_for(&shell, "localhost:5199", 30),
            "{}",
            shell.contents()
        );
        thread::sleep(Duration::from_millis(200));
        assert_eq!(
            shell.urls(),
            ["http://localhost:5199/"],
            "{}",
            shell.contents()
        );
        shell.resize(20, 60);
        assert_eq!(shell.parser.lock().unwrap().screen().size(), (20, 60));
        shell.kill();
    }

    #[test]
    fn keys_map_to_vt_sequences() {
        let none = egui::Modifiers::NONE;
        assert_eq!(key_bytes(Key::Enter, none).unwrap(), b"\r");
        assert_eq!(key_bytes(Key::ArrowUp, none).unwrap(), b"\x1b[A");
        assert_eq!(key_bytes(Key::L, egui::Modifiers::CTRL).unwrap(), [0x0c]);
        assert!(
            key_bytes(Key::C, egui::Modifiers::CTRL).is_none(),
            "Copy event sends ^C"
        );
        assert_eq!(
            color(vt100::Color::Idx(196), true),
            Some(Color32::from_rgb(255, 0, 0))
        );
    }
}

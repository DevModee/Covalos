use std::{
    io::{BufRead, BufReader, Read},
    path::Path,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use eframe::egui::{
    self, Align, Context, Key, Layout, Margin, RichText, ScrollArea, TextEdit, TopBottomPanel, Ui,
};

use super::{Editor, theme::*};
use crate::process::{configure as hidden, terminate};

const MAX_LOG: usize = 140_000;

#[derive(Clone, Copy, PartialEq, Default)]
enum Tab {
    #[default]
    Terminal,
    Output,
    AgentLog,
}

#[derive(Default)]
pub(super) struct Tools {
    tab: Tab,
    command: String,
    terminal: String,
    output: String,
    agent_log: String,
    process: Option<Job>,
    git: Option<Job>,
    git_view: String,
}

impl Tools {
    pub(super) fn log_agent(&mut self, event: &str) {
        append(&mut self.agent_log, event);
        append(&mut self.agent_log, "\n");
    }

    pub(super) fn workspace_changed(&mut self) {
        *self = Self::default();
    }

    pub(super) fn poll(&mut self) {
        drain(&mut self.process, &mut self.terminal);
        drain(&mut self.git, &mut self.git_view);
    }
}

enum Event {
    Output(String),
    Finished(String),
}

struct Job {
    events: mpsc::Receiver<Event>,
    stop: Arc<AtomicBool>,
}

impl Drop for Job {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn append(log: &mut String, text: &str) {
    log.push_str(text);
    if log.len() > MAX_LOG {
        let end = log.ceil_char_boundary(log.len() - MAX_LOG);
        log.drain(..end);
    }
}

fn drain(job: &mut Option<Job>, text: &mut String) {
    let Some(run) = job else { return };
    for event in run.events.try_iter().take(256) {
        match event {
            Event::Output(output) => append(text, &output),
            Event::Finished(status) => {
                append(text, &status);
                *job = None;
                return;
            }
        }
    }
}

fn worker(mut command: Command, ctx: Context, show_status: bool) -> Job {
    let (sender, events) = mpsc::sync_channel(256);
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    hidden(&mut command);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    thread::spawn(move || {
        let send = |event| {
            let _ = sender.send(event);
            ctx.request_repaint();
        };
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                send(Event::Finished(format!(
                    "Could not start command. {error}\n"
                )));
                return;
            }
        };
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let read = |reader: Box<dyn Read + Send>| {
            let sender = sender.clone();
            let ctx = ctx.clone();
            thread::spawn(move || {
                let mut reader = BufReader::new(reader);
                loop {
                    let mut bytes = Vec::new();
                    let Ok(count) = (&mut reader).take(4096).read_until(b'\n', &mut bytes) else {
                        break;
                    };
                    if count == 0
                        || sender
                            .send(Event::Output(String::from_utf8_lossy(&bytes).into_owned()))
                            .is_err()
                    {
                        break;
                    }
                    ctx.request_repaint();
                }
            })
        };
        let stdout = read(Box::new(stdout));
        let stderr = read(Box::new(stderr));
        let status = loop {
            if thread_stop.load(Ordering::Relaxed) {
                terminate(&mut child);
                break "\nStopped.\n".into();
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    break if show_status || !status.success() {
                        format!("\n{status}\n")
                    } else {
                        String::new()
                    };
                }
                Ok(None) => thread::sleep(Duration::from_millis(35)),
                Err(error) => {
                    terminate(&mut child);
                    break format!("\nCould not wait for command. {error}\n");
                }
            }
        };
        // Some commands deliberately launch background jobs that inherit the pipes.
        // Allow a short grace period for normal output without hanging the editor.
        for _ in 0..40 {
            if stdout.is_finished() && stderr.is_finished() {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        send(Event::Finished(status));
    });
    Job { events, stop }
}

fn user_command(root: &Path, script: &str) -> Command {
    #[cfg(windows)]
    let mut command = {
        use std::os::windows::process::CommandExt;
        let mut c = Command::new("cmd.exe");
        c.args(["/d", "/s", "/c"]).raw_arg(format!("\"{script}\""));
        c
    };
    #[cfg(not(windows))]
    let mut command = {
        let mut c = Command::new("sh");
        c.args(["-c", script]);
        c
    };
    command.current_dir(root);
    command
}

impl Editor {
    pub(super) fn git_refresh(&mut self, ctx: &Context, diff: Option<bool>) {
        let Some(root) = &self.root else { return };
        let mut command = Command::new("git");
        command
            .current_dir(root)
            .env("GIT_OPTIONAL_LOCKS", "0")
            .args([
                "--no-pager",
                "-c",
                "color.ui=false",
                "-c",
                "core.quotePath=false",
            ]);
        match diff {
            None => {
                command.args(["status", "--short", "--branch"]);
            }
            Some(staged) => {
                command.args(["diff", "--no-ext-diff", "--no-textconv"]);
                if staged {
                    command.arg("--cached");
                }
                command.arg("--");
            }
        }
        self.tools.git_view.clear();
        self.tools.git = Some(worker(command, ctx.clone(), false));
    }

    pub(super) fn git_panel(&mut self, ui: &mut Ui) {
        ui.label(RichText::new("Source control").strong().size(17.0));
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button("Status").clicked() {
                self.git_refresh(ui.ctx(), None);
            }
            if ui.button("Diff").clicked() {
                self.git_refresh(ui.ctx(), Some(false));
            }
            if ui.button("Staged").clicked() {
                self.git_refresh(ui.ctx(), Some(true));
            }
        });
        ui.separator();
        ScrollArea::vertical().id_salt("git-output").show(ui, |ui| {
            if self.tools.git_view.is_empty() {
                ui.label(muted(if self.tools.git.is_some() {
                    "Reading Git…"
                } else {
                    "No changes to display."
                }));
            }
            ui.add(
                egui::Label::new(RichText::new(&self.tools.git_view).monospace().small())
                    .wrap()
                    .selectable(true),
            );
        });
    }

    fn run_terminal(&mut self, ctx: &Context) {
        let Some(root) = &self.root else { return };
        let script = std::mem::take(&mut self.tools.command);
        if script.trim().is_empty() {
            return;
        }
        append(&mut self.tools.terminal, &format!("\n> {script}\n"));
        self.tools.process = Some(worker(user_command(root, &script), ctx.clone(), true));
    }

    pub(super) fn tools_panel(&mut self, ctx: &Context) {
        let max_height = (ctx.content_rect().height() * 0.47).clamp(160.0, 420.0);
        let panel = TopBottomPanel::bottom("tools")
            .resizable(true)
            .default_height(self.preferences.tools_height.min(max_height))
            .height_range(130.0..=max_height)
            .frame(panel_frame(Margin::symmetric(12, 8)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (_, mark) = ui.allocate_space(egui::vec2(20.0, 28.0));
                    draw_icon(ui.painter(), mark.center(), Icon::Terminal, MUTED);
                    for (tab, label) in [
                        (Tab::Terminal, "Terminal"),
                        (Tab::Output, "Output"),
                        (Tab::AgentLog, "Agent log"),
                    ] {
                        if ui.selectable_label(self.tools.tab == tab, label).clicked() {
                            self.tools.tab = tab;
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Hide").clicked() {
                            self.preferences.tools_open = false;
                        }
                        if let Some(job) = &self.tools.process
                            && self.tools.tab == Tab::Terminal
                            && ui.button("Stop").clicked()
                        {
                            job.stop.store(true, Ordering::Relaxed);
                        }
                    });
                });
                ui.separator();
                let is_terminal = self.tools.tab == Tab::Terminal;
                let log = match self.tools.tab {
                    Tab::Terminal => &self.tools.terminal,
                    Tab::Output => &self.tools.output,
                    Tab::AgentLog => &self.tools.agent_log,
                };
                let height =
                    (ui.available_height() - if is_terminal { 36.0 } else { 0.0 }).max(18.0);
                ScrollArea::vertical()
                    .id_salt(("tool-log", self.tools.tab as u8))
                    .max_height(height)
                    .stick_to_bottom(true)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if log.is_empty() {
                            ui.label(muted(match self.tools.tab {
                                Tab::Terminal if self.root.is_none() => {
                                    "Open a project to run commands."
                                }
                                Tab::Terminal => "Run a command from the project folder.",
                                Tab::Output => "Editor operations appear here.",
                                Tab::AgentLog => "Agent activity appears here.",
                            }));
                        }
                        ui.add(
                            egui::Label::new(RichText::new(log).monospace().small())
                                .wrap()
                                .selectable(true),
                        );
                    });
                if is_terminal {
                    let can_run = self.root.is_some() && self.tools.process.is_none();
                    let input = ui.add_enabled(
                        can_run,
                        TextEdit::singleline(&mut self.tools.command)
                            .font(egui::TextStyle::Monospace)
                            .hint_text("Enter a command…")
                            .desired_width(f32::INFINITY)
                            .char_limit(4096),
                    );
                    if can_run
                        && input.lost_focus()
                        && ui.input(|input| input.key_pressed(Key::Enter))
                    {
                        self.run_terminal(ctx);
                    }
                }
            });
        let height = panel.response.rect.height();
        if height < max_height {
            self.preferences.tools_height = height;
        }
    }

    pub(super) fn editor_log(&mut self, message: &str) {
        append(&mut self.tools.output, message);
        append(&mut self.tools.output, "\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_runs_at_project_root_and_captures_stdout() -> std::io::Result<()> {
        let folder = tempfile::tempdir()?;
        let mut log = String::new();
        let script = if cfg!(windows) {
            "echo AEROS_READY && cd"
        } else {
            "echo AEROS_READY && pwd"
        };
        let mut job = Some(worker(
            user_command(folder.path(), script),
            Context::default(),
            true,
        ));
        let until = std::time::Instant::now() + Duration::from_secs(5);
        while job.is_some() && std::time::Instant::now() < until {
            drain(&mut job, &mut log);
            thread::sleep(Duration::from_millis(10));
        }
        assert!(job.is_none(), "{log}");
        assert!(log.contains("AEROS_READY"), "{log}");
        assert!(
            log.contains(folder.path().file_name().unwrap().to_str().unwrap()),
            "{log}"
        );
        Ok(())
    }
}

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
    git_mode: GitMode,
    git_selected: Option<String>,
    /// Last complete `git status` output, kept while a diff is shown.
    git_status: String,
}

/// What the Git job last produced: the working-tree summary or one file's diff.
#[derive(Clone, Default, PartialEq)]
enum GitMode {
    #[default]
    Status,
    Diff {
        path: Option<String>,
        staged: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Group {
    Conflicts,
    Staged,
    Changes,
    Untracked,
}

impl Group {
    fn title(self) -> &'static str {
        match self {
            Self::Conflicts => "Merge conflicts",
            Self::Staged => "Staged changes",
            Self::Changes => "Changes",
            Self::Untracked => "Untracked files",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Change {
    pub(super) group: Group,
    pub(super) code: char,
    pub(super) path: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Status {
    pub(super) branch: Option<String>,
    pub(super) upstream: Option<String>,
    pub(super) ahead: u32,
    pub(super) behind: u32,
    pub(super) changes: Vec<Change>,
}

/// Parse `git status --porcelain=v1 --branch` into grouped changes. A file that is both
/// staged and modified appears in both groups, like in IntelliJ and VS Code.
pub(super) fn parse_status(text: &str) -> Status {
    let mut status = Status::default();
    for line in text.lines() {
        if let Some(head) = line.strip_prefix("## ") {
            let (names, counts) = head.split_once(" [").unwrap_or((head, ""));
            let (branch, upstream) = names.split_once("...").unwrap_or((names, ""));
            let branch = branch.trim_start_matches("No commits yet on ");
            status.branch = Some(branch.to_owned());
            status.upstream = (!upstream.is_empty()).then(|| upstream.to_owned());
            for part in counts.trim_end_matches(']').split(", ") {
                if let Some(n) = part.strip_prefix("ahead ") {
                    status.ahead = n.parse().unwrap_or(0);
                } else if let Some(n) = part.strip_prefix("behind ") {
                    status.behind = n.parse().unwrap_or(0);
                }
            }
            continue;
        }
        let mut chars = line.chars();
        let (Some(x), Some(y)) = (chars.next(), chars.next()) else {
            continue;
        };
        let Some(path) = line.get(3..).filter(|p| !p.is_empty()) else {
            continue;
        };
        let path = path
            .rsplit_once(" -> ")
            .map_or(path, |(_, new)| new)
            .trim_matches('"')
            .to_owned();
        let conflict = x == 'U' || y == 'U' || (x == 'A' && y == 'A') || (x == 'D' && y == 'D');
        if conflict {
            status.changes.push(Change {
                group: Group::Conflicts,
                code: 'U',
                path,
            });
            continue;
        }
        if x == '?' {
            status.changes.push(Change {
                group: Group::Untracked,
                code: 'U',
                path,
            });
            continue;
        }
        if x != ' ' && x != '!' {
            status.changes.push(Change {
                group: Group::Staged,
                code: x,
                path: path.clone(),
            });
        }
        if y != ' ' && y != '!' {
            status.changes.push(Change {
                group: Group::Changes,
                code: y,
                path,
            });
        }
    }
    status
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
        if self.git.is_none()
            && self.git_mode == GitMode::Status
            && self.git_status != self.git_view
        {
            self.git_status.clone_from(&self.git_view);
        }
    }

    /// A file diff is open and owns the editor area.
    pub(super) fn showing_diff(&self) -> bool {
        matches!(self.git_mode, GitMode::Diff { .. })
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
                command.args([
                    "status",
                    "--porcelain=v1",
                    "--branch",
                    "--untracked-files=all",
                ]);
                self.tools.git_mode = GitMode::Status;
            }
            Some(staged) => {
                command.args(["diff", "--no-ext-diff", "--no-textconv"]);
                if staged {
                    command.arg("--cached");
                }
                command.arg("--");
                self.tools.git_mode = GitMode::Diff { path: None, staged };
            }
        }
        self.tools.git_view.clear();
        self.tools.git = Some(worker(command, ctx.clone(), false));
    }

    /// Show one file's diff (working tree or staged) inside the Git panel.
    pub(super) fn git_diff_file(&mut self, ctx: &Context, path: &str, staged: bool) {
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
            ])
            .args(["diff", "--no-ext-diff", "--no-textconv"]);
        if staged {
            command.arg("--cached");
        }
        command.args(["--", path]);
        self.tools.git_view.clear();
        self.tools.git_mode = GitMode::Diff {
            path: Some(path.to_owned()),
            staged,
        };
        self.tools.git = Some(worker(command, ctx.clone(), false));
    }

    /// The last Git job finished and produced output.
    #[cfg(test)]
    pub(super) fn git_ready(&self) -> bool {
        self.tools.git.is_none() && !self.tools.git_view.is_empty()
    }

    pub(super) fn git_panel(&mut self, ui: &mut Ui) {
        if self.tools.git.is_none()
            && self.tools.git_view.is_empty()
            && self.tools.git_status.is_empty()
            && self.root.is_some()
        {
            self.git_refresh(ui.ctx(), None);
        }
        self.git_status_view(ui);
    }

    /// The selected file's diff, shown in the editor area like an IntelliJ diff tab.
    pub(super) fn git_diff_surface(&mut self, ui: &mut Ui) {
        if let GitMode::Diff { path, staged } = self.tools.git_mode.clone() {
            egui::Frame::new()
                .inner_margin(Margin::symmetric(14, 10))
                .show(ui, |ui| self.git_diff_view(ui, path.as_deref(), staged));
        }
    }

    fn git_status_view(&mut self, ui: &mut Ui) {
        let status = parse_status(if self.tools.git_mode == GitMode::Status {
            &self.tools.git_view
        } else {
            &self.tools.git_status
        });
        let not_repository = self.tools.git.is_none()
            && status.branch.is_none()
            && self.tools.git_view.contains("not a git repository");
        // Branch header: branch chip, upstream drift and a refresh action.
        ui.horizontal(|ui| {
            let (_, mark) = ui.allocate_space(egui::vec2(18.0, 24.0));
            draw_icon_sized(
                ui.painter(),
                mark.center(),
                Icon::Git,
                colors().accent,
                15.0,
            );
            let branch = status.branch.clone().unwrap_or_else(|| {
                if not_repository {
                    tr("No repository").to_owned()
                } else {
                    "—".into()
                }
            });
            ui.add(
                egui::Label::new(RichText::new(branch).strong().color(colors().text)).truncate(),
            );
            if status.ahead > 0 {
                ui.label(
                    RichText::new(format!("↑{}", status.ahead))
                        .small()
                        .color(colors().success),
                );
            }
            if status.behind > 0 {
                ui.label(
                    RichText::new(format!("↓{}", status.behind))
                        .small()
                        .color(colors().warning),
                );
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_button(ui, Icon::Refresh, "Refresh changes", false).clicked() {
                    self.git_refresh(ui.ctx(), None);
                }
                if self.tools.git.is_some() {
                    busy_indicator(ui);
                }
            });
        });
        if let Some(upstream) = &status.upstream {
            ui.label(muted(format!("{} {upstream}", tr("Tracking"))));
        }
        ui.add_space(6.0);
        if not_repository {
            ui.label(muted("This folder is not a Git repository."));
            return;
        }
        if status.changes.is_empty() && self.tools.git.is_none() {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                let (_, mark) = ui.allocate_space(egui::vec2(28.0, 28.0));
                draw_icon_sized(
                    ui.painter(),
                    mark.center(),
                    Icon::Check,
                    colors().success,
                    22.0,
                );
                ui.label(RichText::new(tr("Working tree clean")).color(colors().secondary));
            });
            return;
        }
        let mut open = None;
        let mut diff = None;
        ScrollArea::vertical()
            .id_salt("git-changes")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for group in [
                    Group::Conflicts,
                    Group::Staged,
                    Group::Changes,
                    Group::Untracked,
                ] {
                    let items: Vec<&Change> =
                        status.changes.iter().filter(|c| c.group == group).collect();
                    if items.is_empty() {
                        continue;
                    }
                    let id = ui.make_persistent_id(("git-group", group as u8));
                    let state = egui::collapsing_header::CollapsingState::load_with_default_open(
                        ui.ctx(),
                        id,
                        true,
                    );
                    let expanded = state.is_open();
                    let header = ui.horizontal(|ui| {
                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), 24.0),
                            egui::Sense::click(),
                        );
                        if response.hovered() {
                            ui.painter().rect_filled(rect, 6, colors().hover);
                        }
                        draw_icon_sized(
                            ui.painter(),
                            rect.left_center() + egui::vec2(10.0, 0.0),
                            if expanded {
                                Icon::ChevronDown
                            } else {
                                Icon::ChevronRight
                            },
                            colors().muted,
                            13.0,
                        );
                        ui.painter().text(
                            rect.left_center() + egui::vec2(22.0, 0.0),
                            egui::Align2::LEFT_CENTER,
                            tr(group.title()),
                            egui::FontId::proportional(SMALL),
                            colors().secondary,
                        );
                        let badge = items.len().to_string();
                        let badge_rect = egui::Rect::from_center_size(
                            rect.right_center() - egui::vec2(16.0, 0.0),
                            egui::vec2(10.0 + 7.0 * badge.len() as f32, 16.0),
                        );
                        ui.painter().rect_filled(badge_rect, 8, colors().elevated);
                        ui.painter().text(
                            badge_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            badge,
                            egui::FontId::proportional(11.0),
                            colors().muted,
                        );
                        response
                    });
                    if header.inner.clicked() {
                        let mut state = state;
                        state.toggle(ui);
                        state.store(ui.ctx());
                    }
                    if !expanded {
                        continue;
                    }
                    for change in items {
                        let selected = self.tools.git_selected.as_deref() == Some(&change.path);
                        let response = change_row(ui, change, selected);
                        // Single click previews the diff; double click opens the file.
                        if response.clicked() {
                            self.tools.git_selected = Some(change.path.clone());
                            if change.group == Group::Untracked {
                                open = Some(change.path.clone());
                            } else {
                                diff = Some((change.path.clone(), change.group == Group::Staged));
                            }
                        }
                        if response.double_clicked() && change.code != 'D' {
                            open = Some(change.path.clone());
                        }
                        response.context_menu(|ui| {
                            if change.group != Group::Untracked
                                && ui.button(tr("Show diff")).clicked()
                            {
                                diff = Some((change.path.clone(), change.group == Group::Staged));
                                ui.close();
                            }
                            if change.code != 'D' && ui.button(tr("Open file")).clicked() {
                                open = Some(change.path.clone());
                                ui.close();
                            }
                            if ui.button(tr("Copy path")).clicked() {
                                ui.ctx().copy_text(change.path.clone());
                                ui.close();
                            }
                        });
                    }
                    ui.add_space(6.0);
                }
            });
        if let Some(path) = open
            && let Some(root) = self.root.clone()
        {
            self.open_file(root.join(path));
        }
        if let Some((path, staged)) = diff {
            self.git_diff_file(ui.ctx(), &path, staged);
        }
    }

    fn git_diff_view(&mut self, ui: &mut Ui, path: Option<&str>, staged: bool) {
        let mut back = false;
        ui.horizontal(|ui| {
            if icon_button(ui, Icon::Close, "Close diff", false).clicked() {
                back = true;
            }
            let name = path.map_or_else(
                || tr(if staged { "Staged changes" } else { "Changes" }).to_owned(),
                |p| p.rsplit(['/', '\\']).next().unwrap_or(p).to_owned(),
            );
            ui.vertical(|ui| {
                ui.add(egui::Label::new(RichText::new(name).strong()).truncate());
                ui.label(muted(if staged { "Staged" } else { "Working tree" }));
            });
        });
        ui.add_space(4.0);
        let mut added = 0;
        let mut removed = 0;
        for line in self.tools.git_view.lines() {
            if line.starts_with('+') && !line.starts_with("+++") {
                added += 1;
            } else if line.starts_with('-') && !line.starts_with("---") {
                removed += 1;
            }
        }
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("+{added}"))
                    .small()
                    .color(colors().success),
            );
            ui.label(
                RichText::new(format!("−{removed}"))
                    .small()
                    .color(colors().error),
            );
        });
        ui.add_space(4.0);
        egui::Frame::new()
            .fill(colors().inset)
            .corner_radius(8)
            .inner_margin(Margin::same(8))
            .show(ui, |ui| {
                ScrollArea::both()
                    .id_salt("git-diff")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.tools.git_view.trim().is_empty() {
                            ui.label(muted(if self.tools.git.is_some() {
                                "Reading Git…"
                            } else {
                                "No differences."
                            }));
                        }
                        let font = egui::FontId::monospace(12.0);
                        let mut job = egui::text::LayoutJob::default();
                        for line in self
                            .tools
                            .git_view
                            .lines()
                            .filter(|l| !l.starts_with("warning: "))
                            .take(4000)
                        {
                            let (color, background) = if line.starts_with("@@") {
                                (colors().accent, egui::Color32::TRANSPARENT)
                            } else if line.starts_with("+++")
                                || line.starts_with("---")
                                || line.starts_with("diff ")
                                || line.starts_with("index ")
                            {
                                (colors().muted, egui::Color32::TRANSPARENT)
                            } else if line.starts_with('+') {
                                (colors().success, colors().success.gamma_multiply(0.10))
                            } else if line.starts_with('-') {
                                (colors().error, colors().error.gamma_multiply(0.10))
                            } else {
                                (colors().secondary, egui::Color32::TRANSPARENT)
                            };
                            job.append(
                                &format!("{line}\n"),
                                0.0,
                                egui::TextFormat {
                                    font_id: font.clone(),
                                    color,
                                    background,
                                    line_height: Some(17.0),
                                    ..Default::default()
                                },
                            );
                        }
                        ui.add(egui::Label::new(job).extend().selectable(true));
                    });
            });
        if back {
            self.tools.git_selected = None;
            self.git_refresh(ui.ctx(), None);
        }
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
            .frame(super::theme::island(Margin::symmetric(12, 8)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (_, mark) = ui.allocate_space(egui::vec2(20.0, 28.0));
                    draw_icon(ui.painter(), mark.center(), Icon::Terminal, colors().muted);
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
                        if ui.button(tr("Hide")).clicked() {
                            self.preferences.tools_open = false;
                        }
                        if let Some(job) = &self.tools.process
                            && self.tools.tab == Tab::Terminal
                            && ui.button(tr("Stop")).clicked()
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
                            .hint_text(tr("Enter a command…"))
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

/// One changed file: Material file icon, the file name in full strength, its folder dimmed
/// and truncated, and a colored status letter aligned on the right.
fn change_row(ui: &mut Ui, change: &Change, selected: bool) -> egui::Response {
    let height = if ui.spacing().interact_size.y < 26.0 {
        24.0
    } else {
        26.0
    };
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::click(),
    );
    let hover = super::motion::spring(
        ui.ctx(),
        response.id.with("hover"),
        if response.hovered() { 1.0 } else { 0.0 },
        super::motion::Spring::SNAPPY,
    );
    let painter = ui.painter_at(rect);
    if selected {
        painter.rect_filled(rect, 6, colors().accent_bg);
    } else if hover > 0.01 {
        painter.rect_filled(rect, 6, colors().hover.gamma_multiply(hover));
    }
    let path = std::path::Path::new(&change.path);
    super::assets::paint(
        ui,
        egui::Rect::from_center_size(
            rect.left_center() + egui::vec2(24.0, 0.0),
            egui::vec2(16.0, 16.0),
        ),
        super::assets::file_name(path),
    );
    let name = path
        .file_name()
        .map_or_else(|| change.path.clone(), |n| n.to_string_lossy().into_owned());
    let folder = path
        .parent()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    let (letter, color) = match change.code {
        'M' => ('M', colors().warning),
        'A' => ('A', colors().success),
        'D' => ('D', colors().error),
        'R' => ('R', colors().accent),
        'C' => ('C', colors().accent),
        'U' if change.group == Group::Conflicts => ('!', colors().error),
        'U' => ('U', colors().success),
        other => (other, colors().muted),
    };
    let text_left = rect.left() + 38.0;
    let text_right = rect.right() - 26.0;
    let name_galley = ui.fonts_mut(|f| {
        f.layout_no_wrap(
            name.clone(),
            egui::FontId::proportional(BODY),
            if change.code == 'D' {
                colors().muted
            } else {
                colors().text
            },
        )
    });
    let name_width = name_galley.size().x.min((text_right - text_left).max(0.0));
    painter
        .with_clip_rect(egui::Rect::from_x_y_ranges(
            text_left..=text_right,
            rect.y_range(),
        ))
        .galley(
            egui::pos2(text_left, rect.center().y - name_galley.size().y / 2.0),
            name_galley,
            colors().text,
        );
    if change.code == 'D' {
        painter.hline(
            text_left..=text_left + name_width,
            rect.center().y + 0.5,
            egui::Stroke::new(1.0_f32, colors().muted),
        );
    }
    if !folder.is_empty() && text_left + name_width + 8.0 < text_right {
        let folder_rect = egui::Rect::from_x_y_ranges(
            (text_left + name_width + 8.0)..=text_right,
            rect.y_range(),
        );
        let galley = ui.fonts_mut(|f| {
            f.layout_job(egui::text::LayoutJob {
                wrap: egui::text::TextWrapping {
                    max_width: folder_rect.width(),
                    max_rows: 1,
                    break_anywhere: true,
                    overflow_character: Some('…'),
                },
                ..egui::text::LayoutJob::simple_singleline(
                    folder,
                    egui::FontId::proportional(SMALL),
                    colors().muted,
                )
            })
        });
        painter.galley(
            egui::pos2(folder_rect.left(), rect.center().y - galley.size().y / 2.0),
            galley,
            colors().muted,
        );
    }
    painter.text(
        rect.right_center() - egui::vec2(12.0, 0.0),
        egui::Align2::CENTER_CENTER,
        letter,
        egui::FontId::monospace(12.0),
        color,
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::SelectableLabel, true, &change.path)
    });
    response.on_hover_text(&change.path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn porcelain_status_is_grouped_by_state() {
        let status = parse_status(
            "## main...origin/main [ahead 2, behind 1]\nM  src/lib.rs\n M README.md\nMM src/app.rs\nA  new.rs\n D gone.rs\nR  old.rs -> src/renamed.rs\nUU conflict.rs\n?? notes/todo.md\n",
        );
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!(status.upstream.as_deref(), Some("origin/main"));
        assert_eq!((status.ahead, status.behind), (2, 1));
        let count = |g| status.changes.iter().filter(|c| c.group == g).count();
        assert_eq!(count(Group::Staged), 4);
        assert_eq!(count(Group::Changes), 3);
        assert_eq!(count(Group::Conflicts), 1);
        assert_eq!(count(Group::Untracked), 1);
        assert!(
            status
                .changes
                .iter()
                .any(|c| c.path == "src/renamed.rs" && c.code == 'R')
        );
        assert_eq!(
            parse_status("## No commits yet on main\n")
                .branch
                .as_deref(),
            Some("main")
        );
    }

    #[test]
    fn shell_runs_at_project_root_and_captures_stdout() -> std::io::Result<()> {
        let folder = tempfile::tempdir()?;
        let mut log = String::new();
        let script = if cfg!(windows) {
            "echo COVALOS_READY && cd"
        } else {
            "echo COVALOS_READY && pwd"
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
        assert!(log.contains("COVALOS_READY"), "{log}");
        assert!(
            log.contains(folder.path().file_name().unwrap().to_str().unwrap()),
            "{log}"
        );
        Ok(())
    }
}

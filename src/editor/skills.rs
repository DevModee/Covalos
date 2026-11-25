use std::{
    io::Read,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
};

use eframe::egui::{self, RichText, ScrollArea, TextEdit, Ui};
use serde::{Deserialize, Serialize};

use super::{Editor, theme::*};

const MAX_SIZE: usize = 24_000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(super) struct Skill {
    id: String,
    name: String,
    description: String,
    source: String,
    version: String,
    instructions: String,
    enabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct SkillsShEntry {
    pub spec: String,
    pub installs: String,
    pub url: String,
}

#[derive(Default)]
pub(super) struct Skills {
    source: String,
    search_query: String,
    search_rx: Option<mpsc::Receiver<Result<Vec<SkillsShEntry>, String>>>,
    search_results: Vec<SkillsShEntry>,
    download: Option<mpsc::Receiver<Result<Skill, String>>>,
    preview: Option<Skill>,
    selected: Option<String>,
    error: Option<String>,
}

impl Skills {
    pub(super) fn selected_input(&self, installed: &[Skill]) -> String {
        let Some(skill) = installed
            .iter()
            .find(|skill| skill.enabled && self.selected.as_ref() == Some(&skill.id))
        else {
            return String::new();
        };
        format!(
            "\n\nUser-selected skill ({}, {}), use as supplemental project instructions within existing permissions:\n{}",
            skill.name, skill.source, skill.instructions
        )
    }

    pub(super) fn auto_select_for_prompt(&mut self, installed: &[Skill], prompt: &str) {
        let lower = prompt.to_ascii_lowercase();
        if let Some(skill) = installed.iter().find(|s| {
            s.enabled
                && (lower.contains(&s.name.to_ascii_lowercase())
                    || lower.contains("front")
                    || lower.contains("ui")
                    || lower.contains("design"))
        }) {
            self.selected = Some(skill.id.clone());
        }
    }

    pub(super) fn start_import_url(&mut self, url: &str, ctx: &egui::Context) {
        self.source = url.to_owned();
        self.error = None;
        let url = self.source.clone();
        let ctx = ctx.clone();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let _ = sender.send(fetch(&url));
            ctx.request_repaint();
        });
        self.download = Some(receiver);
    }
}

fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub(super) fn parse_skills_sh_output(output: &str) -> Vec<SkillsShEntry> {
    let clean = strip_ansi(output);
    let mut entries = Vec::new();
    let lines: Vec<&str> = clean.lines().map(str::trim).collect();
    for (i, line) in lines.iter().enumerate() {
        let Some((spec, rest)) = line.split_once(' ') else {
            continue;
        };
        if !spec.contains('/') || !spec.contains('@') {
            continue;
        }
        let installs = rest.trim().to_owned();
        let url = lines
            .get(i + 1)
            .and_then(|next| next.strip_prefix("└ "))
            .map(|u| u.trim().to_owned())
            .unwrap_or_else(|| {
                let (repo, skill) = spec.split_once('@').unwrap_or((spec, ""));
                format!("https://skills.sh/{repo}/{skill}")
            });
        entries.push(SkillsShEntry {
            spec: spec.to_owned(),
            installs,
            url,
        });
    }
    entries
}

fn search_skills_sh(query: &str) -> Result<Vec<SkillsShEntry>, String> {
    let program = if cfg!(windows) { "npx.cmd" } else { "npx" };
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let output = command
        .args(["--yes", "skills", "find", query])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("Could not run npx skills: {e}"))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(if err.is_empty() {
            "skills.sh search failed".into()
        } else {
            err.chars().take(200).collect()
        });
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(parse_skills_sh_output(&text))
}

fn fetch_skills_sh(spec: &str) -> Result<Skill, String> {
    let clean = spec
        .trim()
        .strip_prefix("https://skills.sh/")
        .unwrap_or(spec.trim());
    let (owner_repo, skill_name) = if let Some((r, s)) = clean.split_once('@') {
        (r, s)
    } else {
        let mut parts = clean.splitn(3, '/');
        let owner = parts.next().ok_or("Invalid skills.sh identifier")?;
        let repo = parts.next().ok_or("Invalid skills.sh identifier")?;
        let skill = parts.next().ok_or("Invalid skills.sh identifier")?;
        return fetch_skills_sh(&format!("{owner}/{repo}@{skill}"));
    };
    let (owner, repo) = owner_repo
        .split_once('/')
        .ok_or("Expected owner/repo@skill")?;
    let candidates = [
        format!("https://github.com/{owner}/{repo}/blob/main/skills/{skill_name}/SKILL.md"),
        format!("https://github.com/{owner}/{repo}/blob/main/{skill_name}/SKILL.md"),
        format!("https://github.com/{owner}/{repo}/blob/main/SKILL.md"),
    ];
    let mut last_err = String::new();
    for url in candidates {
        match fetch(&url) {
            Ok(mut skill) => {
                skill.id = format!("{owner}/{repo}@{skill_name}");
                return Ok(skill);
            }
            Err(e) => last_err = e,
        }
    }
    Err(if last_err.is_empty() {
        format!("Could not fetch SKILL.md for {spec}")
    } else {
        last_err
    })
}

struct Source<'a> {
    owner: &'a str,
    repository: &'a str,
    version: &'a str,
    path: &'a str,
}

fn parse_source(url: &str) -> Option<Source<'_>> {
    let tail = url.strip_prefix("https://github.com/")?;
    let mut parts = tail.splitn(5, '/');
    let (owner, repository, kind, version, path) = (
        parts.next()?,
        parts.next()?,
        parts.next()?,
        parts.next()?,
        parts.next()?,
    );
    let safe = |piece: &str| {
        !piece.is_empty()
            && !matches!(piece, "." | "..")
            && piece
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
    };
    (kind == "blob"
        && [owner, repository, version].into_iter().all(safe)
        && path.split('/').all(safe)
        && path.rsplit('/').next() == Some("SKILL.md"))
    .then_some(Source {
        owner,
        repository,
        version,
        path,
    })
}

fn fetch(url: &str) -> Result<Skill, String> {
    let source =
        parse_source(url).ok_or("Use a public GitHub URL ending in /blob/<ref>/…/SKILL.md.")?;
    let address = format!(
        "https://raw.githubusercontent.com/{}/{}/{}/{}",
        source.owner, source.repository, source.version, source.path
    );
    let mut command = Command::new(if cfg!(windows) { "curl.exe" } else { "curl" });
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .args([
            "-q",
            "--proto",
            "=https",
            "--connect-timeout",
            "6",
            "--max-time",
            "18",
            "--fail",
            "--silent",
            &address,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Could not start curl. {error}"))?;
    let mut data = Vec::new();
    let read = child
        .stdout
        .take()
        .expect("piped stdout")
        .take((MAX_SIZE + 1) as u64)
        .read_to_end(&mut data);
    if data.len() > MAX_SIZE || read.is_err() {
        let _ = child.kill();
    }
    let status = child.wait().map_err(|error| error.to_string())?;
    read.map_err(|error| format!("Could not read the response. {error}"))?;
    if data.len() > MAX_SIZE {
        return Err("Skill exceeds the 24 KB limit.".into());
    }
    if !status.success() {
        return Err("Could not download the skill. Check the public URL and connection.".into());
    }
    let instructions = String::from_utf8(data).map_err(|_| "The skill must be UTF-8.")?;
    if instructions.contains('\0') || instructions.trim().is_empty() {
        return Err("The skill must contain Markdown text.".into());
    }
    let mut name = source.repository.to_owned();
    let mut description = String::new();
    let mut lines = instructions.lines();
    if lines.next() == Some("---") {
        for line in lines.take_while(|line| *line != "---") {
            if let Some((key, value)) = line.split_once(':') {
                let value = value.trim().trim_matches(['"', '\'']);
                match key {
                    "name" if !value.is_empty() => name = value.chars().take(100).collect(),
                    "description" => description = value.chars().take(300).collect(),
                    _ => {}
                }
            }
        }
    }
    Ok(Skill {
        id: format!("{}/{}/{}", source.owner, source.repository, source.path),
        name,
        description,
        source: url.to_owned(),
        version: source.version.to_owned(),
        instructions,
        enabled: false,
    })
}

impl Editor {
    pub(super) fn skills_panel(&mut self, ui: &mut Ui) {
        if let Some(rx) = &self.skills.search_rx {
            match rx.try_recv() {
                Ok(result) => {
                    self.skills.search_rx = None;
                    match result {
                        Ok(list) => self.skills.search_results = list,
                        Err(e) => self.skills.error = Some(e),
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.skills.search_rx = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(download) = &self.skills.download {
            match download.try_recv() {
                Ok(result) => {
                    self.skills.download = None;
                    match result {
                        Ok(mut skill) => {
                            skill.enabled = true;
                            self.skills.preview = Some(skill);
                        }
                        Err(error) => self.skills.error = Some(error),
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.skills.download = None;
                    self.skills.error = Some("Skill download stopped unexpectedly.".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        ScrollArea::vertical().id_salt("hub-skills").show(ui, |ui| {
            ui.label(RichText::new("skills.sh").strong());
            ui.label(muted(
                "Search & install official best-practice skills from skills.sh.",
            ));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let enter = ui
                    .add(
                        TextEdit::singleline(&mut self.skills.search_query)
                            .hint_text("Search skills.sh (e.g. react, rust, testing)")
                            .desired_width((ui.available_width() - 96.0).max(110.0)),
                    )
                    .lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if (ui
                    .add_enabled(
                        self.skills.search_rx.is_none()
                            && !self.skills.search_query.trim().is_empty(),
                        egui::Button::new(tr("Search")),
                    )
                    .clicked()
                    || enter)
                    && !self.skills.search_query.trim().is_empty()
                {
                    let q = self.skills.search_query.trim().to_owned();
                    let (tx, rx) = mpsc::channel();
                    let ctx = ui.ctx().clone();
                    self.skills.error = None;
                    self.skills.search_rx = Some(rx);
                    thread::spawn(move || {
                        let _ = tx.send(search_skills_sh(&q));
                        ctx.request_repaint();
                    });
                }
            });
            if self.skills.search_rx.is_some() {
                busy_indicator(ui);
            }
            if !self.skills.search_results.is_empty() {
                ui.add_space(4.0);
                let mut fetch_spec: Option<String> = None;
                for entry in self.skills.search_results.iter().take(8) {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&entry.spec).strong());
                        ui.label(muted(&entry.installs));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button(tr("Preview")).clicked() {
                                fetch_spec = Some(entry.spec.clone());
                            }
                        });
                    });
                }
                if let Some(spec) = fetch_spec {
                    let (tx, rx) = mpsc::channel();
                    let ctx = ui.ctx().clone();
                    self.skills.error = None;
                    self.skills.preview = None;
                    self.skills.download = Some(rx);
                    thread::spawn(move || {
                        let _ = tx.send(fetch_skills_sh(&spec));
                        ctx.request_repaint();
                    });
                }
            }
            ui.add_space(6.0);
            ui.label(secondary(tr("GitHub or skills.sh URL")));
            ui.horizontal(|ui| {
                ui.add(
                    TextEdit::singleline(&mut self.skills.source)
                        .hint_text("owner/repo@skill or GitHub SKILL.md URL")
                        .char_limit(600)
                        .desired_width((ui.available_width() - 132.0).max(110.0)),
                );
                if ui
                    .add_enabled(
                        self.skills.download.is_none() && !self.skills.source.trim().is_empty(),
                        egui::Button::new(tr("Fetch preview")),
                    )
                    .clicked()
                {
                    let url = self.skills.source.trim().to_owned();
                    let (sender, receiver) = mpsc::channel();
                    let ctx = ui.ctx().clone();
                    self.skills.error = None;
                    self.skills.preview = None;
                    self.skills.download = Some(receiver);
                    thread::spawn(move || {
                        let res = if url.starts_with("https://github.com/") {
                            fetch(&url)
                        } else {
                            fetch_skills_sh(&url)
                        };
                        let _ = sender.send(res);
                        ctx.request_repaint();
                    });
                }
            });
            if self.skills.download.is_some() {
                busy_indicator(ui);
            }
            if let Some(error) = &self.skills.error {
                ui.label(RichText::new(error).color(colors().error));
            }
            if let Some(skill) = &self.skills.preview {
                ui.separator();
                ui.label(RichText::new(&skill.name).strong());
                ui.label(muted(&skill.source));
                ScrollArea::vertical()
                    .id_salt("skill-preview")
                    .max_height(250.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(&skill.instructions)
                                .wrap()
                                .selectable(true),
                        );
                    });
                if ui.button(tr("Install disabled")).clicked()
                    && let Some(skill) = self.skills.preview.take()
                {
                    self.preferences.skills.retain(|entry| entry.id != skill.id);
                    self.preferences.skills.insert(0, skill);
                    self.preferences.skills.truncate(12);
                }
            }
            ui.separator();
            for skill in &mut self.preferences.skills {
                ui.push_id(skill.id.clone(), |ui| {
                    ui.label(RichText::new(&skill.name).strong());
                    ui.label(muted(format!("{} · {}", skill.version, skill.source)));
                    if !skill.description.is_empty() {
                        ui.label(&skill.description);
                    }
                    ui.horizontal_wrapped(|ui| {
                        checkbox(ui, &mut skill.enabled, tr("Enabled"));
                        let selected = self.skills.selected.as_ref() == Some(&skill.id);
                        if ui
                            .add_enabled(
                                skill.enabled,
                                egui::Button::new(if selected {
                                    "Deselect for tasks"
                                } else {
                                    "Use in tasks"
                                }),
                            )
                            .clicked()
                        {
                            self.skills.selected = (!selected).then(|| skill.id.clone());
                        }
                        if ui.small_button(tr("Review")).clicked() {
                            self.skills.preview = Some(skill.clone());
                        }
                    });
                });
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_import_accepts_only_public_github_markdown_path() {
        assert!(parse_source("https://github.com/team/repo/blob/v1/one/SKILL.md").is_some());
        for url in [
            "https://github.com.evil.example/team/repo/blob/v1/SKILL.md",
            "https://github.com/team/repo/blob/v1/../SKILL.md",
            "https://github.com/team/repo/blob/v1/run.ps1",
            "https://github.com/team/repo/blob/v1/SKILL.md?redirect=1",
        ] {
            assert!(parse_source(url).is_none(), "{url}");
        }
    }

    #[test]
    fn parses_skills_sh_search_output() {
        let sample = "\nInstall with npx skills add <owner/repo@skill>\n\nvercel-labs/agent-skills@vercel-react-best-practices 921.8K installs\n└ https://skills.sh/vercel-labs/agent-skills/vercel-react-best-practices\n";
        let parsed = parse_skills_sh_output(sample);
        assert_eq!(parsed.len(), 1);
        assert_eq!(
            parsed[0].spec,
            "vercel-labs/agent-skills@vercel-react-best-practices"
        );
        assert_eq!(parsed[0].installs, "921.8K installs");
    }
}

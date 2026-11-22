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

#[derive(Default)]
pub(super) struct Skills {
    source: String,
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
        if let Some(download) = &self.skills.download {
            match download.try_recv() {
                Ok(result) => {
                    self.skills.download = None;
                    match result {
                        Ok(skill) => self.skills.preview = Some(skill),
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
            ui.label(muted(
                "Review GitHub instructions before enabling them for tasks.",
            ));
            ui.label(secondary(tr("GitHub skill URL")));
            ui.horizontal(|ui| {
                ui.add(
                    TextEdit::singleline(&mut self.skills.source)
                        .hint_text("https://github.com/owner/repo/blob/ref/SKILL.md")
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
                        let _ = sender.send(fetch(&url));
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
}

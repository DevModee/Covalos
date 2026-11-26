//! Guided Mode (lyric-style synced code-along) with deterministic dependency ordering.
//!
//! Instead of pasting finished files from an AI response, Guided Mode turns an AI plan
//! into atomic, dependency-ordered steps so the user never types an import or reference
//! to a file that does not exist yet.
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use eframe::egui::{self, Align, Color32, Frame, Id, Layout, Margin, RichText};
use serde::{Deserialize, Serialize};

use super::i18n::tr;
use super::theme::{BODY, LEAD, SMALL, colors};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum FlowOrder {
    /// Start from the entry file up to the first missing dependency, pause to write
    /// that dependency file, then return and import it cleanly.
    #[default]
    Natural,
    /// Write leaf dependency files first, then assemble the entry file last.
    BottomUp,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum MatchMode {
    /// Tolerates minor whitespace, quote style, and trailing semicolon differences.
    #[default]
    Flexible,
    /// Requires exact trimmed line text.
    Strict,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum GuidedStyle {
    /// Short explanations per line, call-flow hover, "Explain another way" and XP.
    #[default]
    Learn,
    /// Code-along only: the lines to type, without explanation cards.
    CodeOnly,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct GuidedPreferences {
    pub(super) flow_order: FlowOrder,
    pub(super) match_mode: MatchMode,
    pub(super) style: GuidedStyle,
    pub(super) show_why: bool,
    /// Expanded explanation of what the active line or block does.
    pub(super) show_detail: bool,
    pub(super) floating: bool,
    /// Open Guided Mode in an independent native OS window (movable across monitors).
    pub(super) detached: bool,
    /// Font size scale percentage for the Guided teleprompter (85..=180).
    pub(super) font_scale_pct: u16,
    /// Show translucent bracket-colored hover explanations over code blocks in the editor.
    pub(super) learn_hover: bool,
    /// Learning points earned by completing guided lines and steps.
    pub(super) xp: u32,
    /// Current streak of consecutive guided lines/steps completed without skipping.
    pub(super) streak: u32,
}

impl Default for GuidedPreferences {
    fn default() -> Self {
        Self {
            flow_order: FlowOrder::Natural,
            match_mode: MatchMode::Flexible,
            style: GuidedStyle::Learn,
            show_why: true,
            show_detail: true,
            floating: true,
            detached: false,
            font_scale_pct: 100,
            learn_hover: true,
            xp: 0,
            streak: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum StepAction {
    #[default]
    CreateFile,
    WriteBlock,
    InsertImport,
    AppendBlock,
}

impl<'de> Deserialize<'de> for StepAction {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.trim().to_ascii_lowercase().as_str() {
            "create_file" | "create" | "new_file" => Self::CreateFile,
            "insert_import" | "import" => Self::InsertImport,
            "append_block" | "append" => Self::AppendBlock,
            _ => Self::WriteBlock,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct GuidedStep {
    pub(super) path: String,
    #[serde(default)]
    pub(super) action: StepAction,
    #[serde(default)]
    pub(super) lines: Vec<String>,
    #[serde(default)]
    pub(super) requires: Vec<String>,
    #[serde(default)]
    pub(super) why: Option<String>,
    /// Longer explanation of the whole block, shown on demand.
    #[serde(default)]
    pub(super) detail: Option<String>,
    /// One explanation per line, aligned with `lines`; missing entries fall back to a
    /// description inferred from the line itself.
    #[serde(default)]
    pub(super) notes: Vec<String>,
    /// Call-flow notes explaining which function calls which and how they connect.
    #[serde(default)]
    pub(super) calls: Vec<String>,
}

impl GuidedStep {
    /// Splits the step at `at`, keeping per-line notes aligned with their lines.
    fn split_at(&self, at: usize) -> (Self, Self) {
        let note_split = at.min(self.notes.len());
        let head = Self {
            lines: self.lines[..at].to_vec(),
            notes: self.notes[..note_split].to_vec(),
            requires: Vec::new(),
            ..self.clone()
        };
        let tail = Self {
            lines: self.lines[at..].to_vec(),
            notes: self.notes[note_split..].to_vec(),
            ..self.clone()
        };
        (head, tail)
    }

    /// The explanation for one line: the plan's note when present, otherwise inferred.
    pub(super) fn line_note(&self, index: usize) -> String {
        self.notes
            .get(index)
            .map(|note| note.trim())
            .filter(|note| !note.is_empty())
            .map_or_else(
                || describe_line(self.lines.get(index).map_or("", String::as_str)).to_owned(),
                str::to_owned,
            )
    }
}

/// Plain-language description of what a code line does, for steps without notes.
pub(super) fn describe_line(line: &str) -> &'static str {
    let line = line.trim();
    let starts = |prefixes: &[&str]| prefixes.iter().any(|p| line.starts_with(p));
    let key = if line.is_empty() {
        "Blank line that separates blocks for readability."
    } else if starts(&["//", "#!", "/*", "*", "<!--"])
        || (line.starts_with('#') && !line.starts_with("#[") && !line.starts_with("#include"))
    {
        "Comment for readers; it does not run."
    } else if starts(&[
        "import ",
        "use ",
        "from ",
        "#include",
        "extern crate",
        "@import",
        "@use",
    ]) || line.contains("require(")
    {
        "Imports a dependency so this file can use it."
    } else if starts(&["@", "#["]) {
        "Directive or decorator that configures the code that follows."
    } else if starts(&[
        "class ",
        "struct ",
        "interface ",
        "type ",
        "enum ",
        "pub struct ",
        "pub enum ",
        "export interface ",
        "export type ",
    ]) {
        "Declares a type that describes the shape of data."
    } else if starts(&[
        "function ",
        "fn ",
        "def ",
        "pub fn ",
        "async ",
        "export function ",
        "export default function ",
        "export async function ",
    ]) || (line.contains("=>") && line.ends_with('{'))
    {
        "Declares a function: a named, reusable block of logic."
    } else if starts(&["return", "yield"]) {
        "Returns a value to whoever called this code."
    } else if starts(&[
        "if ", "if(", "else", "} else", "match ", "switch", "case ", "elif ",
    ]) {
        "Branches: runs code only when a condition holds."
    } else if starts(&["for ", "for(", "while ", "loop", "do "])
        || line.contains(".map(")
        || line.contains(".forEach(")
    {
        "Loops over items, repeating the block for each one."
    } else if starts(&["}", ")", "]", "</"]) {
        "Closes the block opened above."
    } else if line.starts_with('<') {
        "Markup: describes an element rendered on screen."
    } else if line.starts_with("--")
        || (line.contains(':') && line.ends_with(';') && !line.contains('('))
    {
        "Sets a style property or design token value."
    } else if line.starts_with("export ") {
        "Exports a value so other files can import it."
    } else if starts(&[
        "let ",
        "const ",
        "var ",
        "static ",
        "pub const ",
        "pub static ",
    ]) {
        "Stores a value in a named variable."
    } else if line.ends_with('{') {
        "Opens a block; the lines below belong to it."
    } else {
        "Statement that runs as part of this step."
    };
    tr(key)
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct GuidedPlan {
    pub(super) title: String,
    pub(super) steps: Vec<GuidedStep>,
}

/// Live progress snapshot returned when syncing with the active editor buffer.
#[derive(Clone, Debug, Default)]
pub(super) struct SyncSnapshot {
    pub(super) on_target_file: bool,
    /// Character match ratio `[0.0, 1.0]` for the current target line.
    pub(super) line_progress: f32,
    pub(super) step_complete: bool,
    pub(super) all_complete: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct GuidedSession {
    pub(super) plan: GuidedPlan,
    pub(super) step_index: usize,
    pub(super) line_index: usize,
    pub(super) completed_steps: Vec<bool>,
    pub(super) window_open: bool,
}

/// Runtime controller held by `Editor`.
#[derive(Default)]
pub(super) struct GuidedController {
    /// Toggle in composers: when true, outgoing prompts request a `covalos-guided` plan.
    pub(super) armed: bool,
    /// Whether the lyric-style guide window is currently visible.
    pub(super) open: bool,
    /// Active guided session, if a plan has been loaded.
    pub(super) session: Option<GuidedSession>,
    /// Agent Hub session ID we are watching for an incoming `covalos-guided` block.
    pub(super) watching_session: Option<String>,
    /// Last live sync snapshot for rendering the karaoke highlight bar.
    pub(super) sync: SyncSnapshot,
    /// Transient notice shown inside the guide window.
    pub(super) hint: Option<String>,
    /// Prompt or URL (repo / aprenderust.dev / lab) drafted inside the Guided launcher.
    pub(super) prompt_draft: String,
    /// Last XP reward earned, for a subtle feedback pill.
    pub(super) last_xp: Option<(u32, std::time::Instant)>,
    /// Last time workspace files on disk were reconciled against the guided plan.
    disk_synced_at: Option<std::time::Instant>,
}

impl GuidedController {
    pub(super) fn load_plan(&mut self, plan: GuidedPlan, workspace: Option<&Path>) {
        if plan.steps.is_empty() {
            return;
        }
        let count = plan.steps.len();
        let mut session = GuidedSession {
            plan,
            step_index: 0,
            line_index: 0,
            completed_steps: vec![false; count],
            window_open: true,
        };
        session.skip_blank_lines();
        self.session = Some(session);
        self.watching_session = None;
        self.open = true;
        self.hint = None;
        self.save_to_disk(workspace);
    }

    pub(super) fn restore_for_workspace(&mut self, workspace: &Path) {
        if let Some(path) = persistence_path(workspace)
            && let Ok(raw) = std::fs::read_to_string(path)
            && let Ok(mut session) = serde_json::from_str::<GuidedSession>(&raw)
            && !session.plan.steps.is_empty()
        {
            session
                .plan
                .steps
                .retain(|s| sanitize_relative_path(&s.path).is_some());
            if !session.plan.steps.is_empty() {
                session.step_index = session.step_index.min(session.plan.steps.len() - 1);
                session
                    .completed_steps
                    .resize(session.plan.steps.len(), false);
                session.skip_blank_lines();
                let should_open = session.window_open;
                self.session = Some(session);
                self.open = should_open;
            }
        }
    }

    pub(super) fn save_to_disk(&mut self, workspace: Option<&Path>) {
        let (Some(root), Some(session)) = (workspace, self.session.as_mut()) else {
            return;
        };
        session.window_open = self.open;
        if let Some(path) = persistence_path(root) {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(json) = serde_json::to_string_pretty(session) {
                let _ = std::fs::write(path, json);
            }
        }
    }

    pub(super) fn clear(&mut self, workspace: Option<&Path>) {
        self.session = None;
        self.open = false;
        self.watching_session = None;
        self.sync = SyncSnapshot::default();
        self.hint = None;
        if let Some(root) = workspace
            && let Some(path) = persistence_path(root)
        {
            let _ = std::fs::remove_file(path);
        }
    }

    pub(super) fn sync_with_editor(
        &mut self,
        active_rel_path: Option<&str>,
        buffer_text: &str,
        mode: MatchMode,
        workspace: Option<&Path>,
    ) {
        if let Some(root) = workspace
            && self
                .disk_synced_at
                .is_none_or(|t| t.elapsed() >= std::time::Duration::from_millis(1500))
        {
            self.disk_synced_at = Some(std::time::Instant::now());
            self.reconcile_with_disk(root, mode);
        }
        let Some(session) = self.session.as_mut() else {
            self.sync = SyncSnapshot::default();
            return;
        };
        let prev = (session.step_index, session.line_index);
        self.sync = session.sync_with_buffer(active_rel_path, buffer_text, mode);
        if (session.step_index, session.line_index) != prev {
            self.save_to_disk(workspace);
        }
    }

    /// Reconciles every step in the plan against the real files on disk so work completed
    /// externally (e.g. by Claude or manual edits) automatically marks steps done and
    /// advances the active step instead of getting stuck on stale lines.
    pub(super) fn reconcile_with_disk(&mut self, workspace: &Path, mode: MatchMode) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let mut changed = false;
        for (idx, step) in session.plan.steps.iter().enumerate() {
            if session.completed_steps.get(idx).copied().unwrap_or(false) {
                continue;
            }
            let full = workspace.join(&step.path);
            let Ok(contents) = std::fs::read_to_string(&full) else {
                continue;
            };
            let disk_lines: Vec<&str> = contents.lines().collect();
            let non_blank: Vec<&String> =
                step.lines.iter().filter(|l| !l.trim().is_empty()).collect();
            if non_blank.is_empty() {
                continue;
            }
            let matched = non_blank
                .iter()
                .filter(|target| disk_lines.iter().any(|l| lines_match(l, target, mode)))
                .count();
            // Step is complete if all lines match, or if the file was already written
            // externally with >= 60% of the step's lines (or substantially larger code).
            let done = matched == non_blank.len()
                || (matched * 5 >= non_blank.len() * 3)
                || (contents.len() > 240 && disk_lines.len() >= non_blank.len() * 2);
            if done && idx < session.completed_steps.len() {
                session.completed_steps[idx] = true;
                changed = true;
            }
        }
        // If the current step is now completed on disk, advance to the first incomplete step.
        if session
            .completed_steps
            .get(session.step_index)
            .copied()
            .unwrap_or(false)
        {
            if let Some(next_idx) = session.completed_steps.iter().position(|&done| !done) {
                session.step_index = next_idx;
                session.line_index = 0;
                session.skip_blank_lines();
                changed = true;
            } else if let Some(step) = session.plan.steps.get(session.step_index) {
                session.line_index = step.lines.len();
                changed = true;
            }
        }
        if changed {
            self.save_to_disk(Some(workspace));
        }
    }

    pub(super) fn advance_step(&mut self, workspace: Option<&Path>) -> Option<String> {
        let session = self.session.as_mut()?;
        if session.step_index < session.completed_steps.len() {
            session.completed_steps[session.step_index] = true;
        }
        if session.step_index + 1 < session.plan.steps.len() {
            session.step_index += 1;
            session.line_index = 0;
            session.skip_blank_lines();
            let next_path = session.plan.steps[session.step_index].path.clone();
            self.save_to_disk(workspace);
            Some(next_path)
        } else {
            self.save_to_disk(workspace);
            None
        }
    }

    pub(super) fn prev_step(&mut self, workspace: Option<&Path>) -> Option<String> {
        let session = self.session.as_mut()?;
        if session.step_index > 0 {
            session.step_index -= 1;
            session.line_index = 0;
            session.skip_blank_lines();
            let path = session.plan.steps[session.step_index].path.clone();
            self.save_to_disk(workspace);
            Some(path)
        } else {
            None
        }
    }

    pub(super) fn jump_to_step(
        &mut self,
        index: usize,
        workspace: Option<&Path>,
    ) -> Option<String> {
        let session = self.session.as_mut()?;
        if index < session.plan.steps.len() {
            session.step_index = index;
            session.line_index = 0;
            session.skip_blank_lines();
            let path = session.plan.steps[session.step_index].path.clone();
            self.save_to_disk(workspace);
            Some(path)
        } else {
            None
        }
    }

    pub(super) fn skip_line(&mut self, workspace: Option<&Path>) {
        if let Some(session) = self.session.as_mut()
            && let Some(step) = session.plan.steps.get(session.step_index)
            && session.line_index < step.lines.len()
        {
            session.line_index += 1;
            session.skip_blank_lines();
            self.save_to_disk(workspace);
        }
    }
}

impl GuidedSession {
    fn skip_blank_lines(&mut self) {
        if let Some(step) = self.plan.steps.get(self.step_index) {
            while self.line_index < step.lines.len()
                && step.lines[self.line_index].trim().is_empty()
            {
                self.line_index += 1;
            }
        }
    }

    pub(super) fn sync_with_buffer(
        &mut self,
        active_rel_path: Option<&str>,
        buffer_text: &str,
        mode: MatchMode,
    ) -> SyncSnapshot {
        let Some(step) = self.plan.steps.get(self.step_index) else {
            return SyncSnapshot {
                all_complete: true,
                ..Default::default()
            };
        };
        let on_target_file = active_rel_path
            .map(|p| paths_equal(p, &step.path))
            .unwrap_or(false);
        if !on_target_file {
            return SyncSnapshot {
                on_target_file: false,
                line_progress: 0.0,
                step_complete: self.line_index >= step.lines.len(),
                all_complete: self.completed_steps.iter().all(|&c| c),
            };
        }

        let buf_lines: Vec<&str> = buffer_text.lines().collect();
        let mut best_progress = 0.0_f32;

        loop {
            self.skip_blank_lines();
            let Some(target) = self
                .plan
                .steps
                .get(self.step_index)
                .and_then(|s| s.lines.get(self.line_index))
            else {
                break;
            };
            let matched_exact = buf_lines.iter().any(|l| lines_match(l, target, mode));
            if matched_exact {
                self.line_index += 1;
                best_progress = 1.0;
                continue;
            }
            best_progress = buf_lines
                .iter()
                .map(|l| line_prefix_ratio(l, target, mode))
                .fold(0.0_f32, f32::max);
            break;
        }

        let step_len = self
            .plan
            .steps
            .get(self.step_index)
            .map_or(0, |s| s.lines.len());
        let step_complete = self.line_index >= step_len;
        if step_complete && self.step_index < self.completed_steps.len() {
            self.completed_steps[self.step_index] = true;
        }
        let all_complete =
            !self.completed_steps.is_empty() && self.completed_steps.iter().all(|&c| c);

        SyncSnapshot {
            on_target_file: true,
            line_progress: if step_complete { 1.0 } else { best_progress },
            step_complete,
            all_complete,
        }
    }
}

/// Wraps a user's request (prompt, repo link, or course URL like aprenderust.dev) so the
/// AI returns a dependency-ordered `covalos-guided` JSON plan tailored to Learn or CodeOnly.
pub(super) fn build_guided_prompt_with_style(
    user_request: &str,
    flow: FlowOrder,
    style: GuidedStyle,
) -> String {
    let order_rule = match flow {
        FlowOrder::Natural => {
            "Use natural developer order: start with the entry file up to the point where it needs another module/component/route, pause to create that dependency file first, then return to the entry file to add the import and use it."
        }
        FlowOrder::BottomUp => {
            "Use bottom-up dependency order: write leaf types, helpers, and components first, then write the entry file that imports them last."
        }
    };
    let has_link = user_request.contains("http://")
        || user_request.contains("https://")
        || user_request.contains("aprenderust.dev")
        || user_request.contains("github.com/");
    let source_hint = if has_link {
        "The user provided a course/lab/repo reference link. Turn its key concepts or exercises into a welcoming, hands-on interactive practice plan in this workspace."
    } else {
        "Turn the user's request into a clear, welcoming step-by-step practice plan."
    };
    let pedagogy_rule = match style {
        GuidedStyle::Learn => {
            "Teaching style: warm, approachable, never heavy or academic. Use short plain-language sentences and everyday analogies in `why`, `detail`, `notes`, and `calls` so anyone learns faster than in a university lecture."
        }
        GuidedStyle::CodeOnly => {
            "Code-only style: keep `why` and `notes` ultra-concise (1 short phrase) and focus on clean, idiomatic code lines."
        }
    };
    format!(
        "{user_request}\n\n\
         [COVALOS GUIDED MODE]\n\
         {source_hint}\n\
         {pedagogy_rule}\n\
         Do NOT edit files directly. Instead, respond with a short 2-line overview and one fenced code block tagged `covalos-guided` containing valid JSON with this exact schema:\n\
         ```covalos-guided\n\
         {{\n\
           \"title\": \"Short project/task title\",\n\
           \"steps\": [\n\
             {{\n\
               \"path\": \"relative/path/to/file.ext\",\n\
               \"action\": \"create_file\",\n\
               \"requires\": [],\n\
               \"why\": \"One short friendly sentence explaining why we write this block now\",\n\
               \"detail\": \"Two to three light, easy-to-read sentences on what this block does and how it fits\",\n\
               \"lines\": [\"line 1\", \"line 2\"],\n\
               \"notes\": [\"What line 1 does in plain language\", \"What line 2 does\"],\n\
               \"calls\": [\"caller() -> callee(): how and why it calls it\"]\n\
             }}\n\
           ]\n\
         }}\n\
         ```\n\
         Rules:\n\
         1. {order_rule}\n\
         2. NEVER import or reference a local file before an earlier step has created it.\n\
         3. Keep each step focused (4 to 16 lines) and use relative paths only.\n\
         4. `notes` holds exactly one short plain-language explanation per entry in `lines`.\n\
         5. `calls` explains which functions/hooks/methods this block calls and how data flows between them."
    )
}

/// Extracts and validates a `GuidedPlan` from an assistant markdown response.
pub(super) fn extract_plan_from_text(text: &str, flow: FlowOrder) -> Option<GuidedPlan> {
    let json_block = find_fenced_block(text, "covalos-guided")
        .or_else(|| find_fenced_block(text, "json").filter(|b| b.contains("\"steps\"")))?;
    let mut plan: GuidedPlan = serde_json::from_str(json_block.trim()).ok()?;
    let mut clean_steps = Vec::with_capacity(plan.steps.len());
    for mut step in plan.steps {
        let Some(clean_path) = sanitize_relative_path(&step.path) else {
            continue;
        };
        if step.lines.iter().all(|l| l.trim().is_empty()) {
            continue;
        }
        step.path = clean_path;
        step.requires = step
            .requires
            .into_iter()
            .filter_map(|r| sanitize_relative_path(&r))
            .collect();
        clean_steps.push(step);
    }
    if clean_steps.is_empty() {
        return None;
    }
    if plan.title.trim().is_empty() {
        plan.title = "Guided session".to_owned();
    }
    plan.steps = normalize_plan_order(clean_steps, flow);
    Some(plan)
}

fn find_fenced_block<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let needle = format!("```{tag}");
    let mut search_from = 0;
    let mut last_match = None;
    while let Some(pos) = text[search_from..].find(&needle) {
        let start = search_from + pos + needle.len();
        let after_header = text[start..].find('\n').map(|n| start + n + 1)?;
        if let Some(end_rel) = text[after_header..].find("```") {
            let block = &text[after_header..after_header + end_rel];
            last_match = Some(block);
            search_from = after_header + end_rel + 3;
        } else {
            break;
        }
    }
    last_match
}

/// Rejects absolute paths, drive letters, parent traversal (`..`), NUL bytes, and `.git`.
pub(super) fn sanitize_relative_path(raw: &str) -> Option<String> {
    let trimmed = raw.trim().replace('\\', "/");
    if trimmed.is_empty() || trimmed.contains('\0') || trimmed.starts_with('/') {
        return None;
    }
    let mut parts = Vec::new();
    for comp in Path::new(&trimmed).components() {
        match comp {
            Component::Normal(seg) => {
                let s = seg.to_str()?;
                if s.is_empty() || s.eq_ignore_ascii_case(".git") || s.contains(':') {
                    return None;
                }
                parts.push(s.to_owned());
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

/// Deterministically reorders (and splits in `FlowOrder::Natural`) steps so no step ever
/// imports a local workspace file that has not been created by an earlier step.
pub(super) fn normalize_plan_order(steps: Vec<GuidedStep>, flow: FlowOrder) -> Vec<GuidedStep> {
    if steps.len() <= 1 {
        return steps;
    }
    let known_files: Vec<String> = steps
        .iter()
        .map(|s| s.path.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    match flow {
        FlowOrder::BottomUp => reorder_bottom_up(steps, &known_files),
        FlowOrder::Natural => reorder_natural_jit(steps, &known_files),
    }
}

fn reorder_bottom_up(steps: Vec<GuidedStep>, known_files: &[String]) -> Vec<GuidedStep> {
    let mut created: HashSet<String> = HashSet::new();
    let mut remaining: Vec<GuidedStep> = steps;
    let mut out = Vec::with_capacity(remaining.len());

    while !remaining.is_empty() {
        let ready_idx = remaining.iter().position(|step| {
            let deps = step_dependencies(step, known_files);
            deps.iter().all(|d| d == &step.path || created.contains(d))
        });
        let idx = ready_idx.unwrap_or(0);
        let step = remaining.remove(idx);
        created.insert(step.path.clone());
        out.push(step);
    }
    out
}

fn reorder_natural_jit(steps: Vec<GuidedStep>, known_files: &[String]) -> Vec<GuidedStep> {
    let mut pending: Vec<Option<GuidedStep>> = steps.into_iter().map(Some).collect();
    let mut created: HashSet<String> = HashSet::new();
    let mut visiting: HashSet<String> = HashSet::new();
    let mut out = Vec::new();

    for i in 0..pending.len() {
        if let Some(step) = pending[i].take() {
            emit_natural(
                step,
                &mut pending,
                known_files,
                &mut created,
                &mut visiting,
                &mut out,
            );
        }
    }
    out
}

fn emit_natural(
    step: GuidedStep,
    pending: &mut [Option<GuidedStep>],
    known_files: &[String],
    created: &mut HashSet<String>,
    visiting: &mut HashSet<String>,
    out: &mut Vec<GuidedStep>,
) {
    // Check line-by-line if a line references a local file that hasn't been created yet.
    let mut first_missing: Option<(usize, String)> = None;
    for (idx, line) in step.lines.iter().enumerate() {
        for dep in extract_line_deps(line, &step.path, known_files) {
            if dep != step.path && !created.contains(&dep) && !visiting.contains(&dep) {
                first_missing = Some((idx, dep));
                break;
            }
        }
        if first_missing.is_some() {
            break;
        }
    }

    if let Some((split_at, missing_dep)) = first_missing {
        // 1. Emit lines before the missing import (if any) so the user starts naturally.
        let (prefix, mut remainder) = step.split_at(split_at);
        if split_at > 0 && prefix.lines.iter().any(|l| !l.trim().is_empty()) {
            out.push(prefix);
            created.insert(step.path.clone());
        }

        // 2. Pull forward and emit all pending steps that create `missing_dep`.
        visiting.insert(step.path.clone());
        let mut dep_steps = Vec::new();
        for slot in pending.iter_mut() {
            if slot.as_ref().is_some_and(|s| s.path == missing_dep)
                && let Some(dep_step) = slot.take()
            {
                dep_steps.push(dep_step);
            }
        }
        for dep_step in dep_steps {
            emit_natural(dep_step, pending, known_files, created, visiting, out);
        }
        created.insert(missing_dep);
        visiting.remove(&step.path);

        // 3. Resume the rest of the original step (starting at the import line).
        if split_at > 0 {
            remainder.action = StepAction::InsertImport;
        }
        emit_natural(remainder, pending, known_files, created, visiting, out);
        return;
    }

    // Also honour explicit `requires` that aren't created yet.
    for req in &step.requires {
        if req != &step.path && !created.contains(req) && !visiting.contains(req) {
            visiting.insert(step.path.clone());
            let mut dep_steps = Vec::new();
            for slot in pending.iter_mut() {
                if slot.as_ref().is_some_and(|s| &s.path == req)
                    && let Some(dep_step) = slot.take()
                {
                    dep_steps.push(dep_step);
                }
            }
            for dep_step in dep_steps {
                emit_natural(dep_step, pending, known_files, created, visiting, out);
            }
            created.insert(req.clone());
            visiting.remove(&step.path);
        }
    }

    created.insert(step.path.clone());
    out.push(step);
}

fn step_dependencies(step: &GuidedStep, known_files: &[String]) -> Vec<String> {
    let mut deps: Vec<String> = step.requires.clone();
    for line in &step.lines {
        for d in extract_line_deps(line, &step.path, known_files) {
            if !deps.contains(&d) {
                deps.push(d);
            }
        }
    }
    deps
}

fn extract_line_deps(line: &str, current_file: &str, known_files: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let trimmed = line.trim();
    // JS / TS: `import ... from './foo'` or `require('./foo')`
    if trimmed.starts_with("import ")
        || trimmed.contains("require(")
        || trimmed.starts_with("export ")
    {
        for quote in ['\'', '"'] {
            let mut rest = trimmed;
            while let Some(start) = rest.find(quote) {
                let after = &rest[start + 1..];
                if let Some(end) = after.find(quote) {
                    let spec = &after[..end];
                    if (spec.starts_with('.') || spec.starts_with("@/"))
                        && let Some(resolved) =
                            resolve_relative_import(current_file, spec, known_files)
                    {
                        out.push(resolved);
                    }
                    rest = &after[end + 1..];
                } else {
                    break;
                }
            }
        }
    }
    // Rust: `mod foo;`
    if let Some(rest) = trimmed.strip_prefix("mod ")
        && let Some(name) = rest.strip_suffix(';')
    {
        let module = name.trim();
        if let Some(resolved) =
            resolve_relative_import(current_file, &format!("./{module}"), known_files)
        {
            out.push(resolved);
        }
    }
    // Python: `from .foo import ...`
    if let Some(rest) = trimmed.strip_prefix("from .") {
        let module = rest.split_whitespace().next().unwrap_or("");
        if !module.is_empty()
            && let Some(resolved) =
                resolve_relative_import(current_file, &format!("./{module}"), known_files)
        {
            out.push(resolved);
        }
    }
    out
}

fn resolve_relative_import(
    current_file: &str,
    spec: &str,
    known_files: &[String],
) -> Option<String> {
    let base_dir = if let Some(stripped) = spec.strip_prefix("@/") {
        return match_known_file(&format!("src/{stripped}"), known_files)
            .or_else(|| match_known_file(stripped, known_files));
    } else {
        Path::new(current_file)
            .parent()
            .unwrap_or_else(|| Path::new(""))
    };
    let joined = normalize_joined(base_dir, spec);
    match_known_file(&joined, known_files)
}

fn normalize_joined(base: &Path, spec: &str) -> String {
    let mut stack: Vec<String> = base
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => s.to_str().map(str::to_owned),
            _ => None,
        })
        .collect();
    for seg in spec.replace('\\', "/").split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                stack.pop();
            }
            other => stack.push(other.to_owned()),
        }
    }
    stack.join("/")
}

fn match_known_file(candidate: &str, known_files: &[String]) -> Option<String> {
    let exact = known_files.iter().find(|k| paths_equal(k, candidate));
    if let Some(f) = exact {
        return Some(f.clone());
    }
    const EXTS: &[&str] = &[
        ".ts",
        ".tsx",
        ".js",
        ".jsx",
        ".rs",
        ".py",
        ".css",
        "/index.ts",
        "/index.tsx",
        "/index.js",
        "/mod.rs",
    ];
    for ext in EXTS {
        let with_ext = format!("{candidate}{ext}");
        if let Some(f) = known_files.iter().find(|k| paths_equal(k, &with_ext)) {
            return Some(f.clone());
        }
    }
    None
}

fn paths_equal(a: &str, b: &str) -> bool {
    a.replace('\\', "/")
        .eq_ignore_ascii_case(&b.replace('\\', "/"))
}

fn normalize_line(line: &str, mode: MatchMode) -> String {
    match mode {
        MatchMode::Strict => line.trim().to_owned(),
        MatchMode::Flexible => {
            let mut out = String::with_capacity(line.len());
            let mut prev_space = false;
            for ch in line.trim().chars() {
                let mapped = if ch == '\'' { '"' } else { ch };
                if mapped.is_whitespace() {
                    if !prev_space {
                        out.push(' ');
                        prev_space = true;
                    }
                } else {
                    out.push(mapped);
                    prev_space = false;
                }
            }
            out.trim_end_matches(';').trim().to_owned()
        }
    }
}

fn lines_match(actual: &str, target: &str, mode: MatchMode) -> bool {
    let a = normalize_line(actual, mode);
    let t = normalize_line(target, mode);
    !t.is_empty() && a == t
}

fn line_prefix_ratio(actual: &str, target: &str, mode: MatchMode) -> f32 {
    let a = normalize_line(actual, mode);
    let t = normalize_line(target, mode);
    if t.is_empty() || a.is_empty() {
        return 0.0;
    }
    let matched = a
        .chars()
        .zip(t.chars())
        .take_while(|(ac, tc)| ac == tc)
        .count();
    let total = t.chars().count().max(1);
    (matched as f32 / total as f32).clamp(0.0, 1.0)
}

fn persistence_path(workspace: &Path) -> Option<PathBuf> {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in workspace.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    let mut dir = crate::paths::config_dir()?;
    dir.push("guided");
    dir.push(format!("{hash:016x}.json"));
    Some(dir)
}

/// Relative workspace path helper for matching open buffers against guided steps.
pub(super) fn relative_workspace_path(root: &Path, file: &Path) -> Option<String> {
    let rel = file.strip_prefix(root).ok()?;
    sanitize_relative_path(&rel.to_string_lossy())
}

/// Lookup progress counts `(completed_steps, total_steps)`.
pub(super) fn step_progress_counts(session: &GuidedSession) -> (usize, usize) {
    let done = session.completed_steps.iter().filter(|&&c| c).count();
    let total = session.plan.steps.len();
    (done, total)
}

impl super::Editor {
    /// Polls any watched Agent Hub session for a `covalos-guided` plan and syncs the
    /// active document buffer with the current lyric line.
    pub(super) fn tick_guided(&mut self) {
        if let Some(watch_id) = self.guided.watching_session.clone() {
            let mut finished_without_plan = false;
            let found = self.hub.agents.runtime.as_ref().and_then(|rt| {
                let session = rt
                    .sessions
                    .iter()
                    .find(|s| s.meta.id.as_str() == watch_id)?;
                if session.running() {
                    return None;
                }
                let flow = self.preferences.guided.flow_order;
                let plan = session
                    .meta
                    .log
                    .iter()
                    .rev()
                    .filter(|l| l.kind == "agent")
                    .find_map(|l| extract_plan_from_text(&l.text, flow));
                if plan.is_none() {
                    finished_without_plan = true;
                }
                plan
            });
            if let Some(plan) = found {
                let first_path = plan.steps.first().map(|s| s.path.clone());
                self.guided.watching_session = None;
                self.guided.load_plan(plan, self.root.as_deref());
                if let Some(path) = first_path {
                    self.open_guided_file(&path);
                }
            } else if finished_without_plan {
                self.guided.watching_session = None;
            }
        }

        if self.guided.session.is_none() {
            return;
        }
        let active_pair = self.active.and_then(|i| self.documents.get(i)).map(|doc| {
            let rel = self
                .root
                .as_deref()
                .and_then(|root| relative_workspace_path(root, &doc.path));
            (rel, doc.text.clone())
        });
        let (rel, text) = active_pair.unwrap_or((None, String::new()));
        let mode = self.preferences.guided.match_mode;
        let root = self.root.clone();
        let before = self.guided.session.as_ref().map(|s| {
            (
                s.step_index,
                s.line_index,
                s.completed_steps.iter().filter(|&&c| c).count(),
            )
        });
        self.guided
            .sync_with_editor(rel.as_deref(), &text, mode, root.as_deref());
        let after = self.guided.session.as_ref().map(|s| {
            (
                s.step_index,
                s.line_index,
                s.completed_steps.iter().filter(|&&c| c).count(),
            )
        });
        if let (Some((bs, bl, bd)), Some((as_, al, ad))) = (before, after) {
            let mut gained = 0u32;
            if as_ == bs && al > bl {
                gained += ((al - bl) as u32) * 5;
            }
            if ad > bd {
                gained += ((ad - bd) as u32) * 25;
            }
            if gained > 0 {
                self.preferences.guided.xp = self.preferences.guided.xp.saturating_add(gained);
                self.preferences.guided.streak = self.preferences.guided.streak.saturating_add(1);
                self.guided.last_xp = Some((gained, std::time::Instant::now()));
            }
        }
    }

    /// Safely opens (or creates empty inside the workspace) the file for a guided step.
    pub(super) fn open_guided_file(&mut self, rel_path: &str) {
        let Some(clean) = sanitize_relative_path(rel_path) else {
            self.notice = Some((true, tr("Invalid guided file path.").to_owned()));
            return;
        };
        let Some(root) = self.root.clone() else {
            self.notice = Some((true, tr("Open a project folder").to_owned()));
            return;
        };
        let full = root.join(&clean);
        if !full.exists() {
            if let Some(parent) = full.parent()
                && let Err(e) = std::fs::create_dir_all(parent)
            {
                self.notice = Some((true, format!("Could not create directory: {e}")));
                return;
            }
            if let Err(e) = std::fs::write(&full, "") {
                self.notice = Some((true, format!("Could not create file: {e}")));
                return;
            }
            self.directories.clear();
        }
        self.open_file(full);
    }

    /// Renders the Studio / Teleprompter Guided Mode surface (or the Learn / Code-only
    /// launcher when no session is active), either floating inside the editor or detached
    /// into an independent native OS window movable across monitors.
    pub(super) fn guided_overlay(&mut self, ctx: &egui::Context) {
        if !self.guided.open {
            return;
        }
        let session_clone = self.guided.session.clone();
        let sync = self.guided.sync.clone();
        let mut open = self.guided.open;
        let mut actions = GuidedUiActions::default();
        let mut launch_prompt: Option<String> = None;

        let prefs = &self.preferences.guided;
        let recent_xp = self
            .guided
            .last_xp
            .filter(|(_, at)| at.elapsed() < std::time::Duration::from_secs(4))
            .map(|(pts, _)| pts);
        let view = GuidedViewConfig {
            style: prefs.style,
            show_why: prefs.show_why,
            show_detail: prefs.show_detail,
            match_mode: prefs.match_mode,
            detached: prefs.detached,
            learn_hover: prefs.learn_hover,
            font_scale: (f32::from(prefs.font_scale_pct.clamp(85, 180)) / 100.0),
            xp: prefs.xp,
            streak: prefs.streak,
            recent_xp,
            watching: self.guided.watching_session.is_some(),
        };
        let mut draft = std::mem::take(&mut self.guided.prompt_draft);

        let show_contents = |ui: &mut egui::Ui,
                             draft: &mut String,
                             actions: &mut GuidedUiActions,
                             launch_prompt: &mut Option<String>| {
            if let Some(session) = &session_clone
                && session.plan.steps.get(session.step_index).is_some()
            {
                render_guided_body(ui, session, &sync, &view, actions);
            } else {
                render_guided_launcher(ui, draft, &view, actions, launch_prompt);
            }
        };

        if view.detached {
            let viewport_id = egui::ViewportId::from_hash_of("covalos-guided-viewport");
            let title = match &session_clone {
                Some(s) => format!("{} — {}", tr("Guided Mode"), s.plan.title),
                None => tr("Guided Mode").to_owned(),
            };
            let builder = egui::ViewportBuilder::default()
                .with_title(title)
                .with_inner_size([840.0, 580.0])
                .with_min_inner_size([380.0, 340.0])
                .with_resizable(true);
            ctx.show_viewport_immediate(viewport_id, builder, |vctx, _class| {
                if vctx.input(|i| i.viewport().close_requested()) {
                    open = false;
                }
                egui::CentralPanel::default()
                    .frame(
                        Frame::new()
                            .fill(colors().canvas)
                            .inner_margin(Margin::same(18)),
                    )
                    .show(vctx, |ui| {
                        show_contents(ui, &mut draft, &mut actions, &mut launch_prompt);
                    });
            });
        } else {
            let (anchor, pivot) = self.guided_anchor(ctx);
            let screen = ctx.content_rect();
            let safe_rect = egui::Rect::from_min_max(
                egui::pos2(screen.left() + 8.0, screen.top() + super::theme::CHROME_HEIGHT + 8.0),
                egui::pos2(screen.right() - 8.0, screen.bottom() - super::theme::STATUS_HEIGHT - 8.0),
            );
            let max_h = (safe_rect.height() - 48.0).max(220.0);
            egui::Window::new(tr("Guided Mode"))
                .id(Id::new("covalos-guided-lyrics"))
                .open(&mut open)
                .resizable(true)
                .collapsible(true)
                .constrain(true)
                .constrain_to(safe_rect)
                .default_width(500.0)
                .min_width(360.0)
                .max_height(max_h)
                .pivot(pivot)
                .default_pos(anchor)
                .frame(
                    Frame::new()
                        .fill(colors().surface)
                        .stroke(egui::Stroke::new(1.0_f32, colors().control_border))
                        .corner_radius(16)
                        .inner_margin(Margin::same(16))
                        .shadow(super::theme::floating_shadow(super::theme::active(), 28)),
                )
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, true])
                        .max_height(max_h)
                        .show(ui, |ui| {
                            show_contents(ui, &mut draft, &mut actions, &mut launch_prompt);
                        });
                });
        }
        self.guided.prompt_draft = draft;

        let prev_open = self.guided.open;
        self.guided.open = open;
        if prev_open != open {
            self.guided.save_to_disk(self.root.as_deref());
        }
        if let Some(style) = actions.set_style {
            self.preferences.guided.style = style;
        }
        if actions.toggle_detached {
            self.preferences.guided.detached = !self.preferences.guided.detached;
        }
        if actions.font_delta != 0 {
            let next = (i32::from(self.preferences.guided.font_scale_pct) + actions.font_delta)
                .clamp(85, 180) as u16;
            self.preferences.guided.font_scale_pct = next;
        }
        if actions.toggle_why {
            self.preferences.guided.show_why = !self.preferences.guided.show_why;
        }
        if actions.toggle_detail {
            self.preferences.guided.show_detail = !self.preferences.guided.show_detail;
        }
        if actions.toggle_learn_hover {
            self.preferences.guided.learn_hover = !self.preferences.guided.learn_hover;
        }
        if actions.toggle_mode {
            self.preferences.guided.match_mode = match self.preferences.guided.match_mode {
                MatchMode::Flexible => MatchMode::Strict,
                MatchMode::Strict => MatchMode::Flexible,
            };
        }
        if let Some(req) = launch_prompt {
            let outgoing = build_guided_prompt_with_style(
                &req,
                self.preferences.guided.flow_order,
                self.preferences.guided.style,
            );
            if let Some(session_id) = self.start_chat_from_home(ctx, &outgoing) {
                self.guided.watching_session = Some(session_id);
                self.guided.prompt_draft.clear();
            }
        }
        if let Some(reexplain) = actions.reexplain_prompt {
            self.preferences.assistant_open = true;
            let _ = self.start_chat_from_home(ctx, &reexplain);
        }
        if let Some(path) = actions.open_path {
            self.open_guided_file(&path);
        }
        if let Some(idx) = actions.jump_step
            && let Some(path) = self.guided.jump_to_step(idx, self.root.as_deref())
        {
            self.open_guided_file(&path);
        }
        if actions.prev_now
            && let Some(path) = self.guided.prev_step(self.root.as_deref())
        {
            self.open_guided_file(&path);
        }
        if actions.skip_line_now {
            self.preferences.guided.streak = 0;
            self.guided.skip_line(self.root.as_deref());
        }
        if actions.advance_now
            && let Some(path) = self.guided.advance_step(self.root.as_deref())
        {
            self.open_guided_file(&path);
        }
        if actions.clear_now {
            self.guided.clear(self.root.as_deref());
        }
    }

    /// Default spot for the Guided window: the bottom-right corner of the editor area,
    /// clear of the title bar controls and of the agent dock on the right.
    fn guided_anchor(&self, ctx: &egui::Context) -> (egui::Pos2, egui::Align2) {
        let screen = ctx.content_rect();
        let dock = if self.preferences.assistant_open {
            self.preferences.assistant_width + 8.0
        } else {
            0.0
        };
        let right = (screen.right() - dock - 16.0).max(screen.left() + 360.0);
        let bottom = screen.bottom() - super::theme::STATUS_HEIGHT - 16.0;
        (egui::pos2(right, bottom), egui::Align2::RIGHT_BOTTOM)
    }
}

struct GuidedViewConfig {
    style: GuidedStyle,
    show_why: bool,
    show_detail: bool,
    match_mode: MatchMode,
    detached: bool,
    learn_hover: bool,
    font_scale: f32,
    xp: u32,
    streak: u32,
    recent_xp: Option<u32>,
    watching: bool,
}

#[derive(Default)]
struct GuidedUiActions {
    set_style: Option<GuidedStyle>,
    open_path: Option<String>,
    jump_step: Option<usize>,
    reexplain_prompt: Option<String>,
    advance_now: bool,
    prev_now: bool,
    skip_line_now: bool,
    clear_now: bool,
    toggle_mode: bool,
    toggle_why: bool,
    toggle_detail: bool,
    toggle_detached: bool,
    toggle_learn_hover: bool,
    font_delta: i32,
}

/// Guided Studio launcher, shown when no guided session is active yet:
/// choose Learn vs Code-only, paste a prompt or link (e.g. aprenderust.dev or GitHub repo),
/// or click a quick-start preset.
fn render_guided_launcher(
    ui: &mut egui::Ui,
    draft: &mut String,
    view: &GuidedViewConfig,
    actions: &mut GuidedUiActions,
    launch_prompt: &mut Option<String>,
) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(tr("Guided Learning Studio"))
                .size(LEAD)
                .strong()
                .color(colors().text),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let detach_lbl = if view.detached {
                tr("Dock ↙")
            } else {
                tr("Pop out ↗")
            };
            if pill(ui, detach_lbl, view.detached).clicked() {
                actions.toggle_detached = true;
            }
            xp_badge(ui, view.xp, view.streak, view.recent_xp);
        });
    });
    ui.add_space(4.0);
    ui.add(
        egui::Label::new(
            RichText::new(tr(
                "Describe what to build or paste a repository or course link. Covalos turns it into steps you type yourself.",
            ))
            .size(SMALL)
            .color(colors().muted),
        )
        .wrap(),
    );
    ui.add_space(12.0);

    // Two clean mode selector cards: Learn vs Code-only
    let gap = 8.0;
    let card_w = ((ui.available_width() - gap) * 0.5).max(140.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for (style, title, desc) in [
            (
                GuidedStyle::Learn,
                "Learn mode",
                "Explanations per line, call flow on hover, XP and streaks",
            ),
            (
                GuidedStyle::CodeOnly,
                "Code only",
                "Type the code line by line, without explanations",
            ),
        ] {
            let selected = view.style == style;
            let (rect, resp) =
                ui.allocate_exact_size(egui::vec2(card_w, 56.0), egui::Sense::click());
            ui.painter().rect(
                rect,
                11,
                if selected {
                    colors().accent_bg
                } else if resp.hovered() {
                    colors().hover
                } else {
                    colors().elevated
                },
                egui::Stroke::new(
                    1.0_f32,
                    if selected {
                        colors().accent.gamma_multiply(0.75)
                    } else {
                        colors().border
                    },
                ),
                egui::StrokeKind::Inside,
            );
            let inner = rect.shrink2(egui::vec2(10.0, 8.0));
            ui.painter().with_clip_rect(inner).text(
                inner.left_top(),
                egui::Align2::LEFT_TOP,
                tr(title),
                egui::FontId::proportional(BODY),
                if selected {
                    colors().accent
                } else {
                    colors().text
                },
            );
            ui.painter().with_clip_rect(inner).text(
                inner.left_top() + egui::vec2(0.0, 20.0),
                egui::Align2::LEFT_TOP,
                tr(desc),
                egui::FontId::proportional(11.0),
                colors().muted,
            );
            if super::theme::pointer(resp).clicked() {
                actions.set_style = Some(style);
            }
        }
    });

    ui.add_space(12.0);
    // Prompt or URL input card
    let input_id = ui.make_persistent_id("guided-launcher-input");
    let focused = ui.memory(|m| m.has_focus(input_id));
    let mut submit = false;
    Frame::new()
        .fill(colors().inset)
        .stroke(egui::Stroke::new(
            1.0_f32,
            if focused {
                colors().accent
            } else {
                colors().border
            },
        ))
        .corner_radius(12)
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if focused && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
                submit = true;
            }
            ui.add(
                egui::TextEdit::multiline(draft)
                    .id(input_id)
                    .frame(false)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY)
                    .hint_text(tr(
                        "Paste a link (e.g. aprenderust.dev, GitHub repo) or write what you want to learn…",
                    )),
            );
            ui.add_space(6.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let ready = !draft.trim().is_empty() && !view.watching;
                let lbl = if view.watching {
                    tr("Building lesson…")
                } else {
                    tr("Start lesson →")
                };
                if super::theme::pointer(ui.add_enabled(ready, super::theme::primary(lbl)))
                    .clicked()
                {
                    submit = true;
                }
            });
        });
    if submit && !draft.trim().is_empty() && !view.watching {
        *launch_prompt = Some(draft.trim().to_owned());
    }

    ui.add_space(12.0);
    ui.label(
        RichText::new(tr("Quick start"))
            .size(11.0)
            .strong()
            .color(colors().muted),
    );
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
        for (label, prompt) in [
            (
                "Rust · aprenderust.dev",
                "Basado en el curso aprenderust.dev de midudev, armame una lección práctica paso a paso para aprender los fundamentos de Rust (variables, mutabilidad, ownership y structs) con ejercicios cortos y explicaciones súper claras.",
            ),
            (
                "Learn this repo",
                "Analizá este proyecto y armame un recorrido guiado paso a paso explicando qué hace cada archivo principal, por qué está estructurado así y qué función llama a qué.",
            ),
            (
                "Hands-on lab",
                "Creame un laboratorio práctico corto para aprender construyendo una mini funcionalidad real en este proyecto, paso a paso y con analogías fáciles.",
            ),
        ] {
            if pill(ui, tr(label), false).clicked() && !view.watching {
                *launch_prompt = Some(prompt.to_owned());
            }
        }
    });
}

/// Compact gamified XP + streak badge.
fn xp_badge(ui: &mut egui::Ui, xp: u32, streak: u32, recent_xp: Option<u32>) {
    Frame::new()
        .fill(colors().accent_bg)
        .stroke(egui::Stroke::new(
            1.0_f32,
            colors().accent.gamma_multiply(0.45),
        ))
        .corner_radius(10)
        .inner_margin(Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                ui.label(
                    RichText::new(format!("★ {xp} XP"))
                        .size(11.0)
                        .strong()
                        .color(colors().accent),
                );
                if streak > 1 {
                    ui.label(
                        RichText::new(format!("· x{streak}"))
                            .size(11.0)
                            .color(colors().warning),
                    );
                }
                if let Some(pts) = recent_xp {
                    ui.label(
                        RichText::new(format!("+{pts}"))
                            .size(11.0)
                            .strong()
                            .color(colors().success),
                    );
                }
            });
        });
}

fn render_guided_body(
    ui: &mut egui::Ui,
    session: &GuidedSession,
    sync: &SyncSnapshot,
    view: &GuidedViewConfig,
    actions: &mut GuidedUiActions,
) {
    let step_idx = session.step_index;
    let line_idx = session.line_index;
    let Some(step) = session.plan.steps.get(step_idx) else {
        return;
    };
    let total_steps = session.plan.steps.len();
    let (done_steps, _) = step_progress_counts(session);
    let next_step_path = session.plan.steps.get(step_idx + 1).map(|s| s.path.clone());
    let wide = ui.available_width() >= 680.0;

    // Header row: title + XP badge + zoom + pop-out / dock
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            let detach_lbl = if view.detached {
                tr("Dock ↙")
            } else {
                tr("Pop out ↗")
            };
            let detach_tip = if view.detached {
                tr("Dock Guided Mode back inside the editor window")
            } else {
                tr("Open Guided Mode in its own window so you can move or maximize it on another screen")
            };
            if pill(ui, detach_lbl, view.detached)
                .on_hover_text(detach_tip)
                .clicked()
            {
                actions.toggle_detached = true;
            }
            if pill(ui, "A+", false)
                .on_hover_text(tr("Increase Guided Mode text size"))
                .clicked()
            {
                actions.font_delta = 15;
            }
            if pill(ui, "A-", false)
                .on_hover_text(tr("Decrease Guided Mode text size"))
                .clicked()
            {
                actions.font_delta = -15;
            }
            xp_badge(ui, view.xp, view.streak, view.recent_xp);
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(&session.plan.title)
                            .size(LEAD * view.font_scale.min(1.35))
                            .strong()
                            .color(colors().text),
                    )
                    .truncate(),
                );
            });
        });
    });

    ui.add_space(6.0);
    // Meta & mode toggles row: Learn vs Code-only + learning helpers
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let mode_lbl = match view.match_mode {
                MatchMode::Flexible => tr("Flexible"),
                MatchMode::Strict => tr("Strict"),
            };
            if pill(ui, mode_lbl, false)
                .on_hover_text(tr("Switch between flexible and strict line matching"))
                .clicked()
            {
                actions.toggle_mode = true;
            }
            if view.style == GuidedStyle::Learn {
                if pill(ui, tr("Learn hover"), view.learn_hover)
                    .on_hover_text(tr(
                        "Highlight code blocks on hover in the editor to explain what they do and what calls what",
                    ))
                    .clicked()
                {
                    actions.toggle_learn_hover = true;
                }
                if pill(ui, tr("Detail"), view.show_detail)
                    .on_hover_text(tr("Explain what each line of the active block does"))
                    .clicked()
                {
                    actions.toggle_detail = true;
                }
            }
            let code_only = view.style == GuidedStyle::CodeOnly;
            if pill(ui, tr("Code only"), code_only)
                .on_hover_text(tr("Code-along without explanation cards"))
                .clicked()
            {
                actions.set_style = Some(GuidedStyle::CodeOnly);
            }
            if pill(ui, tr("Learn mode"), !code_only)
                .on_hover_text(tr("Explanations, call flow on hover and XP"))
                .clicked()
            {
                actions.set_style = Some(GuidedStyle::Learn);
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(format!(
                            "{} {}/{} · {}",
                            tr("Step"),
                            step_idx + 1,
                            total_steps,
                            step.path
                        ))
                        .size(SMALL)
                        .color(colors().secondary),
                    )
                    .truncate(),
                )
                .on_hover_text(&step.path);
            });
        });
    });

    ui.add_space(6.0);
    let (prog_rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 4.0), egui::Sense::hover());
    ui.painter().rect_filled(prog_rect, 2, colors().hover);
    let step_ratio = (done_steps as f32 / total_steps.max(1) as f32).clamp(0.0, 1.0);
    if step_ratio > 0.0 {
        let fill = egui::Rect::from_min_size(
            prog_rect.min,
            egui::vec2(prog_rect.width() * step_ratio, prog_rect.height()),
        );
        ui.painter().rect_filled(fill, 2, colors().accent);
    }

    ui.add_space(10.0);

    if wide {
        let total_w = ui.available_width();
        let roadmap_w = (total_w * 0.30).clamp(200.0, 270.0);
        let main_w = (total_w - roadmap_w - 14.0).max(320.0);
        let avail_h = ui.available_height().max(200.0);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            ui.allocate_ui_with_layout(
                egui::vec2(roadmap_w, avail_h),
                Layout::top_down(Align::Min),
                |ui| {
                    step_roadmap(ui, session, actions);
                },
            );
            ui.allocate_ui_with_layout(
                egui::vec2(main_w, avail_h),
                Layout::top_down(Align::Min),
                |ui| {
                    teleprompter_column(
                        ui,
                        step,
                        step_idx,
                        line_idx,
                        sync,
                        view,
                        next_step_path.as_deref(),
                        actions,
                    );
                },
            );
        });
    } else {
        teleprompter_column(
            ui,
            step,
            step_idx,
            line_idx,
            sync,
            view,
            next_step_path.as_deref(),
            actions,
        );
    }
}

/// Sidebar roadmap shown automatically when Guided Mode is wide (e.g. on a second monitor).
fn step_roadmap(ui: &mut egui::Ui, session: &GuidedSession, actions: &mut GuidedUiActions) {
    Frame::new()
        .fill(colors().surface)
        .stroke(egui::Stroke::new(1.0_f32, colors().border))
        .corner_radius(11)
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(tr("Steps"))
                    .size(SMALL)
                    .strong()
                    .color(colors().muted),
            );
            ui.add_space(6.0);
            egui::ScrollArea::vertical()
                .id_salt("guided-roadmap-scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    for (idx, step) in session.plan.steps.iter().enumerate() {
                        let is_active = idx == session.step_index;
                        let is_done = session.completed_steps.get(idx).copied().unwrap_or(false);
                        let file_name = Path::new(&step.path)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(&step.path);
                        let (rect, resp) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), 38.0),
                            egui::Sense::click(),
                        );
                        let bg = if is_active {
                            colors().accent_bg
                        } else if resp.hovered() {
                            colors().hover
                        } else {
                            Color32::TRANSPARENT
                        };
                        ui.painter().rect_filled(rect, 8, bg);
                        if is_active {
                            ui.painter().rect_stroke(
                                rect,
                                8,
                                egui::Stroke::new(1.0_f32, colors().accent.gamma_multiply(0.55)),
                                egui::StrokeKind::Inside,
                            );
                        }
                        let badge_center = rect.left_center() + egui::vec2(14.0, 0.0);
                        if is_done {
                            ui.painter().circle_filled(
                                badge_center,
                                8.0,
                                colors().success.gamma_multiply(0.22),
                            );
                            ui.painter().text(
                                badge_center,
                                egui::Align2::CENTER_CENTER,
                                "✓",
                                egui::FontId::proportional(11.0),
                                colors().success,
                            );
                        } else {
                            ui.painter().circle_filled(
                                badge_center,
                                8.0,
                                if is_active {
                                    colors().accent
                                } else {
                                    colors().elevated
                                },
                            );
                            ui.painter().text(
                                badge_center,
                                egui::Align2::CENTER_CENTER,
                                format!("{}", idx + 1),
                                egui::FontId::proportional(10.5),
                                if is_active {
                                    colors().canvas
                                } else {
                                    colors().muted
                                },
                            );
                        }
                        let text_rect = egui::Rect::from_min_max(
                            rect.left_top() + egui::vec2(28.0, 4.0),
                            rect.right_bottom() - egui::vec2(6.0, 4.0),
                        );
                        let p = ui.painter().with_clip_rect(text_rect);
                        p.text(
                            text_rect.left_top(),
                            egui::Align2::LEFT_TOP,
                            file_name,
                            egui::FontId::proportional(SMALL),
                            if is_active {
                                colors().text
                            } else if is_done {
                                colors().secondary
                            } else {
                                colors().muted
                            },
                        );
                        p.text(
                            text_rect.left_bottom(),
                            egui::Align2::LEFT_BOTTOM,
                            &step.path,
                            egui::FontId::proportional(10.5),
                            colors().muted,
                        );
                        if super::theme::pointer(resp)
                            .on_hover_text(&step.path)
                            .clicked()
                        {
                            actions.jump_step = Some(idx);
                        }
                    }
                });
        });
}

#[allow(clippy::too_many_arguments)]
fn teleprompter_column(
    ui: &mut egui::Ui,
    step: &GuidedStep,
    step_idx: usize,
    line_idx: usize,
    sync: &SyncSnapshot,
    view: &GuidedViewConfig,
    next_step_path: Option<&str>,
    actions: &mut GuidedUiActions,
) {
    // Target file prompt when user is on another tab
    if !sync.on_target_file {
        Frame::new()
            .fill(colors().accent_bg)
            .stroke(egui::Stroke::new(
                1.0_f32,
                colors().accent.gamma_multiply(0.45),
            ))
            .corner_radius(9)
            .inner_margin(Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(format!("{}: {}", tr("Target file"), step.path))
                            .size(SMALL)
                            .color(colors().text),
                    );
                    if super::theme::pointer(
                        ui.add(
                            egui::Button::new(
                                RichText::new(tr("Open file →"))
                                    .size(SMALL)
                                    .strong()
                                    .color(colors().canvas),
                            )
                            .fill(colors().accent)
                            .corner_radius(7),
                        ),
                    )
                    .clicked()
                    {
                        actions.open_path = Some(step.path.clone());
                    }
                });
            });
        ui.add_space(8.0);
    }

    // Optional friendly 1-line explanation (shown in Learn mode)
    if view.style == GuidedStyle::Learn
        && view.show_why
        && let Some(why) = step.why.as_deref().filter(|w| !w.trim().is_empty())
    {
        Frame::new()
            .fill(colors().elevated)
            .corner_radius(10)
            .inner_margin(Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add(
                    egui::Label::new(
                        RichText::new(why)
                            .size(SMALL * view.font_scale.min(1.25))
                            .color(colors().secondary),
                    )
                    .wrap(),
                );
            });
        ui.add_space(8.0);
    }

    // Lyrics-style synced code block with syntax colors & karaoke active line
    let lang = Path::new(&step.path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let context_before = if view.detached { 3 } else { 2 };
    let context_after = if view.detached { 6 } else { 4 };
    let code_small = (SMALL * view.font_scale).clamp(11.0, 22.0);
    let code_active = ((BODY + 1.5) * view.font_scale).clamp(13.0, 27.0);

    Frame::new()
        .fill(colors().inset)
        .corner_radius(11)
        .stroke(egui::Stroke::new(1.0_f32, colors().border))
        .inner_margin(Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let start = line_idx.saturating_sub(context_before);
            let end = (line_idx + context_after).min(step.lines.len());
            for (idx, code_line) in step.lines.iter().enumerate().take(end).skip(start) {
                if idx < line_idx {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("✓")
                                .monospace()
                                .size(code_small)
                                .color(colors().success.gamma_multiply(0.85)),
                        );
                        ui.add(
                            egui::Label::new(
                                RichText::new(code_line)
                                    .monospace()
                                    .size(code_small)
                                    .color(colors().muted),
                            )
                            .wrap(),
                        );
                    });
                } else if idx == line_idx && !sync.step_complete {
                    ui.add_space(5.0);
                    active_line_card(ui, code_line, sync.line_progress, code_active);
                    ui.add_space(5.0);
                } else {
                    let job = if !lang.is_empty() {
                        let mut job = super::syntax::highlight(
                            ui.ctx(),
                            code_line,
                            lang,
                            code_small,
                            code_small * 1.32,
                        );
                        for sec in &mut job.sections {
                            sec.format.color = sec.format.color.gamma_multiply(0.68);
                        }
                        job
                    } else {
                        egui::text::LayoutJob::simple(
                            code_line.to_owned(),
                            egui::FontId::monospace(code_small),
                            colors().secondary.gamma_multiply(0.75),
                            f32::INFINITY,
                        )
                    };
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("{:>2}", idx + 1))
                                .monospace()
                                .size((code_small - 1.5).max(9.5))
                                .color(colors().disabled),
                        );
                        ui.add(egui::Label::new(job).wrap());
                    });
                }
            }
            if sync.step_complete {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(if sync.all_complete {
                        tr("All guided steps completed!")
                    } else {
                        tr("Step completed!")
                    })
                    .size(BODY * view.font_scale.min(1.3))
                    .strong()
                    .color(colors().success),
                );
            }
        });

    if view.style == GuidedStyle::Learn && view.show_detail {
        ui.add_space(10.0);
        if detail_card(ui, step, line_idx, sync.step_complete) {
            let active_line = step.lines.get(line_idx).map_or("", String::as_str);
            let block = step.lines.join("\n");
            actions.reexplain_prompt = Some(format!(
                "Explicame de otra forma, súper simple y con una analogía de la vida real (sin lenguaje académico pesado), qué hace este bloque de `{}` y en especial la línea `{}`:\n```\n{}\n```",
                step.path, active_line, block
            ));
        }
    }

    ui.add_space(10.0);
    let next_label = next_step_path.map(|next_path| {
        if next_path == step.path {
            tr("Next step →").to_owned()
        } else {
            format!("{}: {} →", tr("Next file"), next_path)
        }
    });
    let action = footer(
        ui,
        step_idx > 0,
        !sync.step_complete,
        next_label.as_deref(),
        sync.step_complete,
    );
    match action {
        Some(FooterAction::Prev) => actions.prev_now = true,
        Some(FooterAction::SkipLine) => actions.skip_line_now = true,
        Some(FooterAction::Finish) => actions.clear_now = true,
        Some(FooterAction::Next) => actions.advance_now = true,
        None => {}
    }
}

/// Footer buttons of the Guided window.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FooterAction {
    Prev,
    SkipLine,
    Finish,
    Next,
}

/// Compact toggle pill used in the Guided header.
fn pill(ui: &mut egui::Ui, label: &str, active: bool) -> egui::Response {
    let response = ui.add(
        egui::Button::new(RichText::new(label).size(11.0).color(if active {
            colors().accent
        } else {
            colors().secondary
        }))
        .fill(if active {
            colors().accent_bg
        } else {
            Color32::TRANSPARENT
        })
        .stroke(egui::Stroke::new(1.0_f32, colors().border))
        .corner_radius(6),
    );
    super::theme::pointer(response)
}

/// The line being typed now, large, with karaoke character-by-character lighting,
/// a subtle copy button, and its live match progress underneath.
fn active_line_card(ui: &mut egui::Ui, code_line: &str, progress: f32, font_size: f32) {
    let clamped = progress.clamp(0.0, 1.0);
    let chars: Vec<char> = code_line.chars().collect();
    let trimmed_start = code_line.len() - code_line.trim_start().len();
    let leading_chars = code_line[..trimmed_start].chars().count();
    let content_chars = chars.len().saturating_sub(leading_chars);
    let matched_content = ((content_chars as f32) * clamped).round() as usize;
    let split_char = (leading_chars + matched_content).min(chars.len());
    let split_byte = code_line
        .char_indices()
        .nth(split_char)
        .map_or(code_line.len(), |(b, _)| b);
    let typed_part = &code_line[..split_byte];
    let rest_part = &code_line[split_byte..];

    let mut job = egui::text::LayoutJob::default();
    let font = egui::FontId::monospace(font_size);
    if !typed_part.is_empty() && clamped > 0.01 {
        job.append(
            typed_part,
            0.0,
            egui::text::TextFormat {
                font_id: font.clone(),
                color: colors().success,
                ..Default::default()
            },
        );
    } else if !typed_part.is_empty() {
        job.append(
            typed_part,
            0.0,
            egui::text::TextFormat {
                font_id: font.clone(),
                color: colors().text,
                ..Default::default()
            },
        );
    }
    if !rest_part.is_empty() {
        job.append(
            rest_part,
            0.0,
            egui::text::TextFormat {
                font_id: font,
                color: colors().text,
                ..Default::default()
            },
        );
    }

    Frame::new()
        .fill(colors().accent_bg)
        .stroke(egui::Stroke::new(
            1.0_f32,
            colors().accent.gamma_multiply(0.55),
        ))
        .corner_radius(9)
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                    if super::theme::icon_button_sized(
                        ui,
                        super::theme::Icon::Copy,
                        "Copy line",
                        false,
                        22.0,
                    )
                    .clicked()
                    {
                        ui.ctx().copy_text(code_line.to_owned());
                    }
                    ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                        ui.add(egui::Label::new(job).wrap());
                    });
                });
            });
            ui.add_space(6.0);
            let (bar, _) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), 3.5), egui::Sense::hover());
            ui.painter().rect_filled(bar, 2, colors().hover);
            if clamped > 0.01 {
                let filled = egui::Rect::from_min_size(
                    bar.min,
                    egui::vec2(bar.width() * clamped, bar.height()),
                );
                ui.painter().rect_filled(filled, 2, colors().accent);
            }
        });
}

/// On-demand learning explanation: what the active line does, why the block exists,
/// which functions/methods it calls, and a button to ask for a simpler alternative explanation.
/// Returns `true` when the user clicks "Explain another way".
fn detail_card(ui: &mut egui::Ui, step: &GuidedStep, line_idx: usize, complete: bool) -> bool {
    let mut reexplain = false;
    let calls: Vec<String> = if !step.calls.is_empty() {
        step.calls.clone()
    } else {
        analyze_block_calls(&step.lines, "")
    };
    Frame::new()
        .fill(colors().elevated)
        .stroke(egui::Stroke::new(
            1.0_f32,
            colors().border.gamma_multiply(0.75),
        ))
        .corner_radius(12)
        .inner_margin(Margin::symmetric(14, 11))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(tr("This line"))
                        .size(SMALL)
                        .strong()
                        .color(colors().accent),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if pill(ui, tr("Explain another way ↻"), false)
                        .on_hover_text(tr(
                            "Ask the AI to explain this block in simpler words with an analogy",
                        ))
                        .clicked()
                    {
                        reexplain = true;
                    }
                });
            });
            if !complete && line_idx < step.lines.len() {
                ui.add_space(3.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(step.line_note(line_idx))
                            .size(SMALL)
                            .color(colors().text),
                    )
                    .wrap(),
                );
            }
            if let Some(detail) = step.detail.as_deref().filter(|d| !d.trim().is_empty()) {
                ui.add_space(7.0);
                ui.label(
                    RichText::new(tr("This block"))
                        .size(SMALL)
                        .strong()
                        .color(colors().secondary),
                );
                ui.add(
                    egui::Label::new(RichText::new(detail).size(SMALL).color(colors().text)).wrap(),
                );
            }
            if !calls.is_empty() {
                ui.add_space(7.0);
                ui.label(
                    RichText::new(tr("Call flow"))
                        .size(SMALL)
                        .strong()
                        .color(colors().secondary),
                );
                for item in calls.iter().take(5) {
                    ui.add(
                        egui::Label::new(
                            RichText::new(format!("→  {item}"))
                                .size(SMALL)
                                .color(colors().text),
                        )
                        .wrap(),
                    );
                }
            }
        });
    reexplain
}

/// Interactive learning metadata for a code block hovered in the editor during Guided Mode.
pub(super) struct LearnBlockInfo {
    /// 0-based inclusive line range `(start_line, end_line)` in the active editor buffer.
    pub(super) line_range: (usize, usize),
    pub(super) title: String,
    pub(super) line_summary: String,
    pub(super) why: Option<String>,
    pub(super) detail: Option<String>,
    pub(super) calls: Vec<String>,
}

/// Returns the learning block + explanation for `hovered_line` (0-based) in the active
/// document while a Guided Mode session is active and `learn_hover` is enabled.
pub(super) fn learn_block_for_hover(
    session: Option<&GuidedSession>,
    root: Option<&Path>,
    learn_hover: bool,
    mode: MatchMode,
    doc_path: &Path,
    buffer_text: &str,
    hovered_line: usize,
) -> Option<LearnBlockInfo> {
    if !learn_hover {
        return None;
    }
    let session = session?;
    let rel = root.and_then(|r| relative_workspace_path(r, doc_path));
    let buf_lines: Vec<&str> = buffer_text.lines().collect();
    let hovered_text = buf_lines.get(hovered_line).copied().unwrap_or("");
    if hovered_text.trim().is_empty() {
        return None;
    }

    // 1. Try to match a specific guided step belonging to this file that covers `hovered_line`.
    if let Some(rel_path) = rel.as_deref() {
        for (idx, step) in session.plan.steps.iter().enumerate() {
            if !paths_equal(rel_path, &step.path) {
                continue;
            }
            if let Some((start, end)) = find_step_line_range(buffer_text, step, mode)
                && hovered_line >= start
                && hovered_line <= end
            {
                let step_line_note = step
                    .lines
                    .iter()
                    .enumerate()
                    .find(|(_, l)| lines_match(hovered_text, l, mode))
                    .map(|(li, _)| step.line_note(li))
                    .unwrap_or_else(|| describe_line(hovered_text).to_owned());
                let calls = if !step.calls.is_empty() {
                    step.calls.clone()
                } else {
                    analyze_block_calls(&step.lines, buffer_text)
                };
                return Some(LearnBlockInfo {
                    line_range: (start, end),
                    title: format!(
                        "{} {}/{} · {}",
                        tr("Step"),
                        idx + 1,
                        session.plan.steps.len(),
                        step.path
                    ),
                    line_summary: step_line_note,
                    why: step.why.clone(),
                    detail: step.detail.clone(),
                    calls,
                });
            }
        }
    }

    // 2. Fallback: highlight the enclosing `{ ... }` block (or compact 3-line slice)
    // around `hovered_line` and explain what it declares, does and calls.
    let (start, end) = find_brace_block_range(buffer_text, hovered_line)?;
    let slice: Vec<String> = buf_lines
        .iter()
        .skip(start)
        .take(end - start + 1)
        .map(|s| (*s).to_owned())
        .collect();
    let header = slice
        .first()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("Block");
    let calls = analyze_block_calls(&slice, buffer_text);
    let declared = extract_declared_name(header);
    let title = if let Some(name) = declared {
        format!("{} · {name}", tr("Code block"))
    } else {
        format!("{} · L{}–{}", tr("Code block"), start + 1, end + 1)
    };
    Some(LearnBlockInfo {
        line_range: (start, end),
        title,
        line_summary: describe_line(hovered_text).to_owned(),
        why: Some(describe_line(header).to_owned()),
        detail: None,
        calls,
    })
}

/// Finds the 0-based inclusive `(start_line, end_line)` in `buffer_text` spanned by `step`.
pub(super) fn find_step_line_range(
    buffer_text: &str,
    step: &GuidedStep,
    mode: MatchMode,
) -> Option<(usize, usize)> {
    let buf_lines: Vec<&str> = buffer_text.lines().collect();
    let mut first: Option<usize> = None;
    let mut last: Option<usize> = None;
    for target in step.lines.iter().filter(|l| !l.trim().is_empty()) {
        if let Some(pos) = buf_lines.iter().position(|l| lines_match(l, target, mode)) {
            first = Some(first.map_or(pos, |f| f.min(pos)));
            last = Some(last.map_or(pos, |l| l.max(pos)));
        }
    }
    first.zip(last)
}

/// Finds the enclosing `{ ... }` block around `line_idx` (0-based), bounded so huge files
/// don't highlight 500 lines at once.
pub(super) fn find_brace_block_range(buffer_text: &str, line_idx: usize) -> Option<(usize, usize)> {
    let lines: Vec<&str> = buffer_text.lines().collect();
    if line_idx >= lines.len() {
        return None;
    }
    // Walk upward to find the nearest unmatched `{` opening this block.
    let mut depth: i32 = 0;
    let mut start = line_idx;
    let mut found_open = false;
    for i in (line_idx.saturating_sub(40)..=line_idx).rev() {
        let l = lines[i];
        let opens = l.chars().filter(|&c| c == '{').count() as i32;
        let closes = l.chars().filter(|&c| c == '}').count() as i32;
        if i == line_idx && opens > closes {
            start = i;
            found_open = true;
            break;
        }
        depth += closes - opens;
        if depth < 0 {
            start = i;
            found_open = true;
            break;
        }
    }
    if !found_open {
        let lo = line_idx.saturating_sub(1);
        let hi = (line_idx + 1).min(lines.len().saturating_sub(1));
        return Some((lo, hi));
    }
    // Walk downward from `start` until braces balance.
    let mut balance: i32 = 0;
    let max_end = (start + 45).min(lines.len().saturating_sub(1));
    let mut end = start;
    for (i, l) in lines.iter().enumerate().take(max_end + 1).skip(start) {
        balance += l.chars().filter(|&c| c == '{').count() as i32;
        balance -= l.chars().filter(|&c| c == '}').count() as i32;
        end = i;
        if balance <= 0 && i > start {
            break;
        }
    }
    if line_idx >= start && line_idx <= end {
        Some((start, end))
    } else {
        Some((line_idx, line_idx))
    }
}

/// Extracts a concise function/component/type name from a block header line.
fn extract_declared_name(header: &str) -> Option<String> {
    let keywords = [
        "export default async function ",
        "export async function ",
        "export default function ",
        "export function ",
        "async function ",
        "function ",
        "pub async fn ",
        "pub fn ",
        "fn ",
        "def ",
        "class ",
        "struct ",
        "const ",
    ];
    for kw in keywords {
        if let Some(rest) = header.strip_prefix(kw) {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

/// Lightweight static call-flow analyzer: detects which functions/methods a block calls
/// and notes whether each callee is defined locally, imported, or a method/hook call.
pub(super) fn analyze_block_calls(lines: &[String], file_text: &str) -> Vec<String> {
    const IGNORED: &[&str] = &[
        "if", "for", "while", "match", "switch", "catch", "return", "sizeof", "typeof", "await",
        "Some", "None", "Ok", "Err", "vec", "format", "println", "eprintln", "assert",
    ];
    let caller = lines
        .iter()
        .find_map(|l| extract_declared_name(l.trim()))
        .unwrap_or_else(|| tr("This block").to_owned());
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with("/*") {
            continue;
        }
        let bytes = trimmed.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'(' && i > 0 {
                let mut start = i;
                while start > 0
                    && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_')
                {
                    start -= 1;
                }
                if start < i {
                    let name = &trimmed[start..i];
                    let is_decl = trimmed.starts_with(&format!("fn {name}("))
                        || trimmed.starts_with(&format!("pub fn {name}("))
                        || trimmed.starts_with(&format!("function {name}("))
                        || trimmed.starts_with(&format!("export function {name}("))
                        || trimmed.starts_with(&format!("export async function {name}("))
                        || trimmed.starts_with(&format!("def {name}("));
                    if !is_decl
                        && name.len() >= 2
                        && !IGNORED.contains(&name)
                        && seen.insert(name.to_owned())
                    {
                        let is_method = start > 0 && bytes[start - 1] == b'.';
                        let role = if name.starts_with("use")
                            && name.chars().nth(3).is_some_and(char::is_uppercase)
                        {
                            tr("React hook")
                        } else if is_method {
                            tr("method call")
                        } else if is_imported(name, file_text) {
                            tr("imported function")
                        } else {
                            tr("function call")
                        };
                        out.push(format!("{caller} → {name}() ({role})"));
                        if out.len() >= 6 {
                            return out;
                        }
                    }
                }
            }
            i += 1;
        }
    }
    out
}

/// Whether `name` is brought in by an import line (`import`, `use`, `from … import`
/// or `require`) rather than defined in the file. Matches whole identifiers only.
fn is_imported(name: &str, file_text: &str) -> bool {
    file_text.lines().any(|line| {
        let line = line.trim_start();
        let is_import = ["import ", "use ", "pub use ", "from "]
            .iter()
            .any(|prefix| line.starts_with(prefix))
            || line.contains("require(");
        is_import
            && line
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .any(|word| word == name)
    })
}

/// Width a footer button needs for `text` at the footer size, padding included.
fn button_width(ui: &egui::Ui, text: &str, strong: bool) -> f32 {
    let mut rich = RichText::new(text).size(SMALL);
    if strong {
        rich = rich.strong();
    }
    let galley = egui::WidgetText::from(rich).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Button,
    );
    galley.size().x + 2.0 * ui.spacing().button_padding.x
}

/// Prev / Skip line / Finish on the left and the next-step button on the right. When the
/// window is too narrow for one row, the next-step button moves to its own full-width row
/// and wraps its label instead of clipping any button.
fn footer(
    ui: &mut egui::Ui,
    can_prev: bool,
    can_skip: bool,
    next: Option<&str>,
    step_complete: bool,
) -> Option<FooterAction> {
    let mut action = None;
    let spacing = ui.spacing().item_spacing.x;
    let left: Vec<(&str, bool)> = [
        (tr("← Prev"), true),
        (tr("Skip line"), can_skip),
        (tr("Finish"), true),
    ]
    .into_iter()
    .filter(|(_, shown)| *shown)
    .collect();
    let left_width: f32 = left
        .iter()
        .map(|(text, _)| button_width(ui, text, false) + spacing)
        .sum();
    let next_width = next.map_or(0.0, |text| button_width(ui, text, true));
    let one_row = left_width + next_width <= ui.available_width();
    ui.horizontal_wrapped(|ui| {
        if super::theme::pointer(ui.add_enabled(
            can_prev,
            egui::Button::new(RichText::new(tr("← Prev")).size(SMALL)),
        ))
        .clicked()
        {
            action = Some(FooterAction::Prev);
        }
        if can_skip
            && super::theme::pointer(ui.add(egui::Button::new(
                RichText::new(tr("Skip line")).size(SMALL),
            )))
            .on_hover_text(tr("Mark the current line as typed and move on"))
            .clicked()
        {
            action = Some(FooterAction::SkipLine);
        }
        if super::theme::pointer(
            ui.add(egui::Button::new(
                RichText::new(tr("Finish"))
                    .size(SMALL)
                    .color(colors().muted),
            )),
        )
        .on_hover_text(tr("End this guided session"))
        .clicked()
        {
            action = Some(FooterAction::Finish);
        }
        if one_row && let Some(text) = next {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if next_button(ui, text, step_complete, false).clicked() {
                    action = Some(FooterAction::Next);
                }
            });
        }
    });
    if !one_row && let Some(text) = next {
        ui.add_space(4.0);
        if next_button(ui, text, step_complete, true).clicked() {
            action = Some(FooterAction::Next);
        }
    }
    action
}

fn next_button(ui: &mut egui::Ui, text: &str, step_complete: bool, full: bool) -> egui::Response {
    let mut button = egui::Button::new(RichText::new(text).size(SMALL).strong().color(
        if step_complete {
            colors().canvas
        } else {
            colors().text
        },
    ))
    .fill(if step_complete {
        colors().accent
    } else {
        colors().hover
    })
    .corner_radius(7);
    if full {
        button = button
            .wrap()
            .min_size(egui::vec2(ui.available_width(), 0.0));
    }
    super::theme::pointer(ui.add(button))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_rejects_traversal_absolute_and_git_paths() {
        assert_eq!(
            sanitize_relative_path("src\\components\\Hero.tsx"),
            Some("src/components/Hero.tsx".into())
        );
        assert_eq!(sanitize_relative_path("../secret.ts"), None);
        assert_eq!(sanitize_relative_path("C:\\Windows\\win.ini"), None);
        assert_eq!(sanitize_relative_path("/etc/passwd"), None);
        assert_eq!(sanitize_relative_path(".git/config"), None);
    }

    #[test]
    fn natural_order_splits_entry_file_before_missing_import() {
        let steps = vec![
            GuidedStep {
                path: "src/main.tsx".into(),
                action: StepAction::CreateFile,
                lines: vec![
                    "// App entry".into(),
                    "import { Home } from './routes/home';".into(),
                    "export function main() { return Home(); }".into(),
                ],
                requires: vec![],
                why: Some("Start entry file".into()),
                detail: None,
                notes: Vec::new(),
                calls: Vec::new(),
            },
            GuidedStep {
                path: "src/routes/home.tsx".into(),
                action: StepAction::CreateFile,
                lines: vec!["export function Home() { return 'Hi'; }".into()],
                requires: vec![],
                why: Some("Create home route".into()),
                detail: None,
                notes: Vec::new(),
                calls: Vec::new(),
            },
        ];

        let ordered = normalize_plan_order(steps, FlowOrder::Natural);
        assert_eq!(ordered.len(), 3);
        assert_eq!(ordered[0].path, "src/main.tsx");
        assert_eq!(ordered[0].lines, vec!["// App entry"]);
        assert_eq!(ordered[1].path, "src/routes/home.tsx");
        assert_eq!(ordered[2].path, "src/main.tsx");
        assert!(ordered[2].lines[0].contains("./routes/home"));
    }

    #[test]
    fn bottom_up_order_places_dependencies_first() {
        let steps = vec![
            GuidedStep {
                path: "src/app.ts".into(),
                action: StepAction::CreateFile,
                lines: vec!["import { sum } from './math';".into()],
                requires: vec![],
                why: None,
                detail: None,
                notes: Vec::new(),
                calls: Vec::new(),
            },
            GuidedStep {
                path: "src/math.ts".into(),
                action: StepAction::CreateFile,
                lines: vec!["export const sum = (a: number, b: number) => a + b;".into()],
                requires: vec![],
                why: None,
                detail: None,
                notes: Vec::new(),
                calls: Vec::new(),
            },
        ];
        let ordered = normalize_plan_order(steps, FlowOrder::BottomUp);
        assert_eq!(ordered[0].path, "src/math.ts");
        assert_eq!(ordered[1].path, "src/app.ts");
    }

    #[test]
    fn extract_and_sync_advances_lyrics_flexibly() {
        let reply = r#"Here is your plan:
```covalos-guided
{
  "title": "Landing page",
  "steps": [
    {
      "path": "src/main.ts",
      "action": "create_file",
      "requires": [],
      "why": "Greet the user",
      "lines": [
        "const title = 'Covalos';",
        "console.log(title);"
      ]
    }
  ]
}
```"#;
        let plan = extract_plan_from_text(reply, FlowOrder::Natural).expect("plan parsed");
        let mut session = GuidedSession {
            completed_steps: vec![false; plan.steps.len()],
            plan,
            step_index: 0,
            line_index: 0,
            window_open: true,
        };
        let snap1 = session.sync_with_buffer(
            Some("src/main.ts"),
            "const   title = \"Covalos\"\nconsole.lo",
            MatchMode::Flexible,
        );
        assert!(snap1.on_target_file);
        assert_eq!(session.line_index, 1);
        assert!(snap1.line_progress > 0.4 && snap1.line_progress < 1.0);

        let snap2 = session.sync_with_buffer(
            Some("src/main.ts"),
            "const title = \"Covalos\";\nconsole.log(title);",
            MatchMode::Flexible,
        );
        assert!(snap2.step_complete);
        assert!(snap2.all_complete);
    }

    #[test]
    fn splitting_a_step_keeps_line_notes_aligned() {
        let step = GuidedStep {
            path: "app/page.tsx".into(),
            action: StepAction::CreateFile,
            lines: vec!["const a = 1;".into(), "import X from './x';".into()],
            requires: Vec::new(),
            why: None,
            detail: Some("Entry".into()),
            notes: vec!["first".into(), "second".into()],
            calls: Vec::new(),
        };
        let (head, tail) = step.split_at(1);
        assert_eq!(head.notes, vec!["first".to_owned()]);
        assert_eq!(tail.notes, vec!["second".to_owned()]);
        assert_eq!(tail.line_note(0), "second");
        assert_eq!(
            describe_line("import X from './x';"),
            "Imports a dependency so this file can use it."
        );
        assert_eq!(
            GuidedStep {
                notes: Vec::new(),
                ..step
            }
            .line_note(0),
            "Stores a value in a named variable."
        );
    }

    #[test]
    fn learn_helpers_detect_brace_blocks_and_call_flow() {
        let code = "import { saveUser } from './db';\n\nexport async function register(email: string) {\n    const clean = email.trim();\n    return saveUser(clean);\n}\n";
        assert_eq!(find_brace_block_range(code, 3), Some((2, 5)));
        let lines: Vec<String> = code.lines().skip(2).take(4).map(str::to_owned).collect();
        let calls = analyze_block_calls(&lines, code);
        assert!(
            calls
                .iter()
                .any(|c| c.contains("register") && c.contains("trim()"))
        );
        assert!(
            calls
                .iter()
                .any(|c| c.contains("saveUser()") && c.contains(tr("imported function")))
        );
        // `trim` appears nowhere in an import line, so it must not be labelled as imported
        // just because the file has imports.
        assert!(
            calls
                .iter()
                .any(|c| c.contains("trim()") && c.contains(tr("method call")))
        );
        let local = "import { db } from './db';\nfn helper() {}\nfn run() {\n    helper();\n}\n";
        let lines: Vec<String> = local.lines().skip(2).map(str::to_owned).collect();
        let calls = analyze_block_calls(&lines, local);
        assert!(
            calls
                .iter()
                .any(|c| c.contains("helper()") && c.contains(tr("function call")))
        );
    }
}

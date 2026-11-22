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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct GuidedPreferences {
    pub(super) flow_order: FlowOrder,
    pub(super) match_mode: MatchMode,
    pub(super) show_why: bool,
    pub(super) floating: bool,
}

impl Default for GuidedPreferences {
    fn default() -> Self {
        Self {
            flow_order: FlowOrder::Natural,
            match_mode: MatchMode::Flexible,
            show_why: true,
            floating: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum StepAction {
    #[default]
    CreateFile,
    WriteBlock,
    InsertImport,
    AppendBlock,
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
            session.plan.steps.retain(|s| sanitize_relative_path(&s.path).is_some());
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
        let all_complete = !self.completed_steps.is_empty()
            && self.completed_steps.iter().all(|&c| c);

        SyncSnapshot {
            on_target_file: true,
            line_progress: if step_complete { 1.0 } else { best_progress },
            step_complete,
            all_complete,
        }
    }
}

/// Wraps a user's request so the AI returns a dependency-ordered `covalos-guided` JSON plan
/// instead of dumping disconnected files.
pub(super) fn build_guided_prompt(user_request: &str, flow: FlowOrder) -> String {
    let order_rule = match flow {
        FlowOrder::Natural => {
            "Use natural developer order: start with the entry file up to the point where it needs another module/component/route, pause to create that dependency file first, then return to the entry file to add the import and use it."
        }
        FlowOrder::BottomUp => {
            "Use bottom-up dependency order: write leaf types, helpers, and components first, then write the entry file that imports them last."
        }
    };
    format!(
        "{user_request}\n\n\
         [COVALOS GUIDED MODE]\n\
         Do NOT edit files directly. Instead, respond with a short 2-line overview and one fenced code block tagged `covalos-guided` containing valid JSON with this exact schema:\n\
         ```covalos-guided\n\
         {{\n\
           \"title\": \"Short project/task title\",\n\
           \"steps\": [\n\
             {{\n\
               \"path\": \"relative/path/to/file.ext\",\n\
               \"action\": \"create_file\",\n\
               \"requires\": [],\n\
               \"why\": \"One short sentence explaining why we write this block now\",\n\
               \"lines\": [\"line 1\", \"line 2\"]\n\
             }}\n\
           ]\n\
         }}\n\
         ```\n\
         Rules:\n\
         1. {order_rule}\n\
         2. NEVER import or reference a local file before an earlier step has created it.\n\
         3. Keep each step focused (4 to 20 lines) and use relative paths only."
    )
}

/// Extracts and validates a `GuidedPlan` from an assistant markdown response.
pub(super) fn extract_plan_from_text(text: &str, flow: FlowOrder) -> Option<GuidedPlan> {
    let json_block = find_fenced_block(text, "covalos-guided")
        .or_else(|| {
            find_fenced_block(text, "json").filter(|b| b.contains("\"steps\""))
        })?;
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
            emit_natural(step, &mut pending, known_files, &mut created, &mut visiting, &mut out);
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
        if split_at > 0 {
            let prefix_lines = step.lines[..split_at].to_vec();
            if prefix_lines.iter().any(|l| !l.trim().is_empty()) {
                out.push(GuidedStep {
                    path: step.path.clone(),
                    action: step.action,
                    lines: prefix_lines,
                    requires: Vec::new(),
                    why: step.why.clone(),
                });
                created.insert(step.path.clone());
            }
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
        let remainder = GuidedStep {
            path: step.path,
            action: if split_at > 0 {
                StepAction::InsertImport
            } else {
                step.action
            },
            lines: step.lines[split_at..].to_vec(),
            requires: step.requires,
            why: step.why,
        };
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
    if trimmed.starts_with("import ") || trimmed.contains("require(") || trimmed.starts_with("export ")
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
        Path::new(current_file).parent().unwrap_or_else(|| Path::new(""))
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
        ".ts", ".tsx", ".js", ".jsx", ".rs", ".py", ".css", "/index.ts", "/index.tsx",
        "/index.js", "/mod.rs",
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
    a.replace('\\', "/").eq_ignore_ascii_case(&b.replace('\\', "/"))
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
            let found = self.hub.agents.runtime.as_ref().and_then(|rt| {
                let session = rt
                    .sessions
                    .iter()
                    .find(|s| s.meta.id.as_str() == watch_id)?;
                if session.running() {
                    return None;
                }
                let flow = self.preferences.guided.flow_order;
                session
                    .meta
                    .log
                    .iter()
                    .rev()
                    .filter(|l| l.kind == "agent")
                    .find_map(|l| extract_plan_from_text(&l.text, flow))
            });
            if let Some(plan) = found {
                let first_path = plan.steps.first().map(|s| s.path.clone());
                self.guided.watching_session = None;
                self.guided.load_plan(plan, self.root.as_deref());
                if let Some(path) = first_path {
                    self.open_guided_file(&path);
                }
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
        self.guided
            .sync_with_editor(rel.as_deref(), &text, mode, root.as_deref());
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

    /// Renders the minimalist Spotify-Lyrics-style Guided Mode floating window.
    pub(super) fn guided_overlay(&mut self, ctx: &egui::Context) {
        if !self.guided.open {
            return;
        }
        let Some(session_clone) = self.guided.session.clone() else {
            self.guided.open = false;
            return;
        };
        let step_idx = session_clone.step_index;
        let line_idx = session_clone.line_index;
        let Some(step) = session_clone.plan.steps.get(step_idx) else {
            return;
        };
        let total_steps = session_clone.plan.steps.len();
        let (done_steps, _) = step_progress_counts(&session_clone);
        let sync = self.guided.sync.clone();
        let next_step_path = session_clone
            .plan
            .steps
            .get(step_idx + 1)
            .map(|s| s.path.clone());

        let mut open = self.guided.open;
        let mut open_path: Option<String> = None;
        let mut advance_now = false;
        let mut prev_now = false;
        let mut skip_line_now = false;
        let mut clear_now = false;
        let mut toggle_mode = false;
        let mut toggle_why = false;

        let show_why = self.preferences.guided.show_why;
        let match_mode = self.preferences.guided.match_mode;

        egui::Window::new(tr("Guided Mode"))
            .id(Id::new("covalos-guided-lyrics"))
            .open(&mut open)
            .resizable(true)
            .collapsible(true)
            .default_width(440.0)
            .default_pos(egui::pos2(
                (ctx.content_rect().width() - 480.0).max(40.0),
                68.0,
            ))
            .frame(
                Frame::new()
                    .fill(colors().surface.gamma_multiply(0.97))
                    .stroke(egui::Stroke::new(1.0_f32, colors().border))
                    .corner_radius(14)
                    .inner_margin(Margin::same(14))
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 10],
                        blur: 28,
                        spread: 0,
                        color: Color32::from_black_alpha(120),
                    }),
            )
            .show(ctx, |ui| {
                // Header: Title, step counter & quick pills
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(&session_clone.plan.title)
                            .size(LEAD)
                            .strong()
                            .color(colors().text),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let mode_lbl = match match_mode {
                            MatchMode::Flexible => tr("Flexible"),
                            MatchMode::Strict => tr("Strict"),
                        };
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new(mode_lbl).size(11.0).color(colors().secondary),
                                )
                                .corner_radius(6),
                            )
                            .clicked()
                        {
                            toggle_mode = true;
                        }
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new(tr("Why?")).size(11.0).color(if show_why {
                                        colors().accent
                                    } else {
                                        colors().muted
                                    }),
                                )
                                .fill(if show_why {
                                    colors().accent_bg
                                } else {
                                    Color32::TRANSPARENT
                                })
                                .corner_radius(6),
                            )
                            .clicked()
                        {
                            toggle_why = true;
                        }
                    });
                });

                // Step & file progress bar
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "{} {}/{} · {}",
                            tr("Step"),
                            step_idx + 1,
                            total_steps,
                            step.path
                        ))
                        .size(SMALL)
                        .color(colors().secondary),
                    );
                });
                ui.add_space(4.0);
                let (prog_rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), 3.0),
                    egui::Sense::hover(),
                );
                ui.painter().rect_filled(prog_rect, 2, colors().elevated);
                let step_ratio = (done_steps as f32 / total_steps.max(1) as f32).clamp(0.0, 1.0);
                if step_ratio > 0.0 {
                    let fill = egui::Rect::from_min_size(
                        prog_rect.min,
                        egui::vec2(prog_rect.width() * step_ratio, prog_rect.height()),
                    );
                    ui.painter().rect_filled(fill, 2, colors().accent);
                }

                // Target file prompt when user is on another tab
                if !sync.on_target_file {
                    ui.add_space(10.0);
                    Frame::new()
                        .fill(colors().accent_bg)
                        .corner_radius(9)
                        .inner_margin(Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!("{}: {}", tr("Target file"), step.path))
                                        .size(SMALL)
                                        .color(colors().text),
                                );
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui
                                        .add(
                                            egui::Button::new(
                                                RichText::new(tr("Open file →"))
                                                    .size(SMALL)
                                                    .strong()
                                                    .color(colors().canvas),
                                            )
                                            .fill(colors().accent)
                                            .corner_radius(7),
                                        )
                                        .clicked()
                                    {
                                        open_path = Some(step.path.clone());
                                    }
                                });
                            });
                        });
                }

                // Optional 1-line explanation
                if show_why
                    && let Some(why) = step.why.as_deref().filter(|w| !w.trim().is_empty())
                {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(why)
                            .size(SMALL)
                            .italics()
                            .color(colors().secondary),
                    );
                }

                // Lyrics-style synced code block
                ui.add_space(10.0);
                Frame::new()
                    .fill(colors().canvas.gamma_multiply(0.85))
                    .corner_radius(10)
                    .stroke(egui::Stroke::new(1.0_f32, colors().border))
                    .inner_margin(Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let start = line_idx.saturating_sub(2);
                        let end = (line_idx + 4).min(step.lines.len());
                        for (idx, code_line) in
                            step.lines.iter().enumerate().take(end).skip(start)
                        {
                            if idx < line_idx {
                                ui.label(
                                    RichText::new(format!("✓  {code_line}"))
                                        .monospace()
                                        .size(SMALL)
                                        .color(colors().muted),
                                );
                            } else if idx == line_idx && !sync.step_complete {
                                ui.add_space(4.0);
                                Frame::new()
                                    .fill(colors().accent_bg)
                                    .corner_radius(7)
                                    .inner_margin(Margin::symmetric(8, 6))
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.label(
                                            RichText::new(code_line)
                                                .monospace()
                                                .size(BODY + 1.0)
                                                .strong()
                                                .color(colors().text),
                                        );
                                        ui.add_space(4.0);
                                        let (bar, _) = ui.allocate_exact_size(
                                            egui::vec2(ui.available_width(), 3.0),
                                            egui::Sense::hover(),
                                        );
                                        ui.painter().rect_filled(bar, 2, colors().elevated);
                                        if sync.line_progress > 0.01 {
                                            let filled = egui::Rect::from_min_size(
                                                bar.min,
                                                egui::vec2(
                                                    bar.width() * sync.line_progress,
                                                    bar.height(),
                                                ),
                                            );
                                            ui.painter().rect_filled(filled, 2, colors().accent);
                                        }
                                    });
                                ui.add_space(4.0);
                            } else {
                                ui.label(
                                    RichText::new(format!("   {code_line}"))
                                        .monospace()
                                        .size(SMALL)
                                        .color(colors().secondary.gamma_multiply(0.75)),
                                );
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
                                .size(BODY)
                                .strong()
                                .color(colors().success),
                            );
                        }
                    });

                // Footer controls
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            step_idx > 0,
                            egui::Button::new(RichText::new(tr("← Prev")).size(SMALL)),
                        )
                        .clicked()
                    {
                        prev_now = true;
                    }
                    if !sync.step_complete
                        && ui
                            .add(egui::Button::new(
                                RichText::new(tr("Skip line")).size(SMALL),
                            ))
                            .clicked()
                    {
                        skip_line_now = true;
                    }
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(tr("Finish")).size(SMALL).color(colors().muted),
                            )
                            .frame(false),
                        )
                        .clicked()
                    {
                        clear_now = true;
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some(next_path) = &next_step_path {
                            let label = if next_path == &step.path {
                                tr("Next step →").to_owned()
                            } else {
                                format!("{}: {} →", tr("Next file"), next_path)
                            };
                            let btn = egui::Button::new(
                                RichText::new(label)
                                    .size(SMALL)
                                    .strong()
                                    .color(if sync.step_complete {
                                        colors().canvas
                                    } else {
                                        colors().text
                                    }),
                            )
                            .fill(if sync.step_complete {
                                colors().accent
                            } else {
                                colors().elevated
                            })
                            .corner_radius(7);
                            if ui.add(btn).clicked() {
                                advance_now = true;
                            }
                        }
                    });
                });
            });

        let prev_open = self.guided.open;
        self.guided.open = open;
        if prev_open != open {
            self.guided.save_to_disk(self.root.as_deref());
        }
        if toggle_why {
            self.preferences.guided.show_why = !self.preferences.guided.show_why;
        }
        if toggle_mode {
            self.preferences.guided.match_mode = match self.preferences.guided.match_mode {
                MatchMode::Flexible => MatchMode::Strict,
                MatchMode::Strict => MatchMode::Flexible,
            };
        }
        if let Some(path) = open_path {
            self.open_guided_file(&path);
        }
        if prev_now
            && let Some(path) = self.guided.prev_step(self.root.as_deref())
        {
            self.open_guided_file(&path);
        }
        if skip_line_now {
            self.guided.skip_line(self.root.as_deref());
        }
        if advance_now
            && let Some(path) = self.guided.advance_step(self.root.as_deref())
        {
            self.open_guided_file(&path);
        }
        if clear_now {
            self.guided.clear(self.root.as_deref());
        }
    }
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
            },
            GuidedStep {
                path: "src/routes/home.tsx".into(),
                action: StepAction::CreateFile,
                lines: vec!["export function Home() { return 'Hi'; }".into()],
                requires: vec![],
                why: Some("Create home route".into()),
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
            },
            GuidedStep {
                path: "src/math.ts".into(),
                action: StepAction::CreateFile,
                lines: vec!["export const sum = (a: number, b: number) => a + b;".into()],
                requires: vec![],
                why: None,
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
}

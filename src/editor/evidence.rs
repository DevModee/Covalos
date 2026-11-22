//! Visual evidence harness. Drives the real COVALOS window through every screen named in
//! the review brief and writes PNG captures plus a manifest into
//! `artifacts/visual-review/`. Input is injected through eframe's raw input hook into
//! a native window; screenshots come from the OpenGL framebuffer.
//!
//! Run (PowerShell):
//!   $env:COVALOS_EVIDENCE_DIR = "artifacts/visual-review"
//!   cargo test --release capture_visual_review -- --ignored
//! Optional: COVALOS_EVIDENCE_WIDTH / _HEIGHT / _SCALE, COVALOS_EVIDENCE_SET=matrix (subset),
//! COVALOS_TEST_EXE (path to covalos.exe, enables the approval fixture).
use super::{Editor, Sidebar, identity, preferences::Preferences, theme};
use eframe::egui::{
    self, Event, Key, Modifiers, PointerButton, UserData, ViewportBuilder, ViewportCommand,
};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

type Act = fn(&mut Editor, &egui::Context, &Path);

enum Step {
    Act(Act),
    Click(&'static str),
    Move(f32, f32),
    Tap(f32, f32),
    Key(Key, Modifiers),
    Wait(f32),
    Shot(&'static str),
    /// Save an upscaled crop around a recorded control from the next screenshot.
    Crop(&'static str, &'static str, f32),
    Until(fn(&Editor) -> bool, &'static str),
    /// Like `Until`, with a longer limit in seconds for real provider turns.
    UntilFor(fn(&Editor) -> bool, &'static str, u64),
}
use Step::*;

fn theme(editor: &mut Editor, ctx: &egui::Context, t: theme::Theme) {
    editor.preferences.theme = t;
    theme::install(
        ctx,
        editor.preferences.font_size,
        editor.preferences.high_contrast,
        t,
    );
}

fn caret_after(editor: &mut Editor, needle: &str) {
    if let Some(index) = editor.active {
        let text = &editor.documents[index].text;
        if let Some(byte) = text.find(needle) {
            let chars = text[..byte + needle.len()].chars().count();
            editor.selection = Some((chars, chars));
            editor.focus_editor = true;
        }
    }
}

fn home(editor: &mut Editor) {
    editor.documents.clear();
    editor.active = None;
    editor.hub_open = false;
    editor.settings_open = false;
    editor.companion.open = false;
    editor.palette = None;
}

fn background(editor: &mut Editor, b: identity::Background) {
    home(editor);
    // Hide the Codex panel so the whole field is visible.
    editor.preferences.assistant_open = false;
    editor.preferences.identity.background = b;
}

fn open_git(editor: &mut Editor, ctx: &egui::Context) {
    editor.sidebar = Sidebar::Git;
    editor.git_refresh(ctx, None);
}

fn detach_named(editor: &mut Editor, name: &str) {
    if let Some(index) = editor
        .documents
        .iter()
        .position(|d| d.path.file_name().is_some_and(|n| n == name))
    {
        editor.detach(index);
    }
}

fn steps(matrix: bool) -> Vec<Step> {
    use identity::{Background as B, Mood as M};
    let mut s = vec![
        Act(|e, c, _| {
            theme(e, c, theme::Theme::Nocturne);
            e.preferences.language = super::i18n::Language::English;
        }),
        Move(0.70, 0.30),
        Wait(0.8),
        Shot("01-home/home-nocturne-dither-field"),
        Click("Folder src"),
        Click("Folder web"),
        Click("Folder tools"),
        Wait(0.4),
        Shot("02-explorer/explorer-expanded"),
        Click("File main.rs"),
        Act(|e, _, _| caret_after(e, "fn summarize(")),
        Wait(0.5),
        Shot("03-editor/rust/editor-rust-nocturne"),
        // MF-05: Explorer zoom leaves the code untouched, editor zoom leaves the tree.
        Act(|e, _, _| e.preferences.explorer_zoom = 1.4),
        Wait(0.4),
        Shot("02-explorer/explorer-zoom-140-code-unchanged"),
        Act(|e, c, _| {
            e.preferences.explorer_zoom = 1.0;
            e.preferences.font_size = 18.0;
            theme::install(c, 18.0, false, e.preferences.theme);
        }),
        Wait(0.4),
        Shot("03-editor/rust/editor-zoom-18pt-explorer-unchanged"),
        Act(|e, c, _| {
            e.preferences.font_size = 14.0;
            theme::install(c, 14.0, false, e.preferences.theme);
        }),
        Act(|e, c, _| open_git(e, c)),
        Until(|e| e.git_ready(), "git status"),
        Wait(0.3),
        Shot("04-git/git-grouped-changes"),
    ];
    if matrix {
        s.extend([
            Act(|e, _, _| {
                e.sidebar = Sidebar::Files;
                detach_named(e, "main.rs");
            }),
            Wait(0.6),
            Shot("05-clean-code/clean-floating-single"),
            Act(|e, c, _| e.dock_all(c.input(|i| i.time))),
            Act(|e, _, _| {
                e.preferences.language = super::i18n::Language::Spanish;
                e.settings_open = true;
            }),
            Wait(0.5),
            Shot("10-settings/settings-es"),
        ]);
        return s;
    }
    // Git: one file's diff, then back to the grouped list.
    s.extend([
        Act(|e, c, _| e.git_diff_file(c, "src/main.rs", false)),
        Until(|e| e.git_ready(), "git diff"),
        Wait(0.3),
        Shot("04-git/git-diff-main-rs"),
        Act(|e, c, _| e.git_diff_file(c, "web/app.ts", true)),
        Until(|e| e.git_ready(), "staged diff"),
        Wait(0.3),
        Shot("04-git/git-diff-staged-app-ts"),
        Act(|e, _, _| e.sidebar = Sidebar::Files),
        // Editor languages and density.
        Click("File app.ts"),
        Act(|e, _, _| caret_after(e, "async function loadOrders(")),
        Wait(0.5),
        Shot("03-editor/typescript/editor-typescript-nocturne"),
        Click("File analyze.py"),
        Act(|e, _, _| caret_after(e, "def summarize(")),
        Wait(0.5),
        Shot("03-editor/python/editor-python-nocturne"),
        Click("Tab main.rs"),
        Act(|e, _, _| e.preferences.compact = true),
        Wait(0.5),
        Shot("03-editor/rust/editor-rust-compact"),
        Act(|e, _, _| {
            e.preferences.compact = false;
            e.preferences.syntax = super::syntax::SyntaxStyle::Graphite;
        }),
        Wait(0.4),
        Shot("03-editor/rust/editor-rust-graphite-syntax-before"),
        Act(|e, _, _| e.preferences.syntax = super::syntax::SyntaxStyle::Auto),
        // Tab spring: three frames of the selection pill travelling.
        Click("Tab analyze.py"),
        Wait(0.03),
        Shot("11-motion/tab-spring-1"),
        Wait(0.06),
        Shot("11-motion/tab-spring-2"),
        Wait(0.4),
        Shot("11-motion/tab-spring-3-settled"),
        // Clean Code: detach, several surfaces, real typing, focus mode, dock back.
        Click("Tab main.rs"),
        Wait(0.2),
        Click("Detach to floating editor"),
        Wait(0.05),
        Shot("11-motion/clean-open-1"),
        Wait(0.5),
        Shot("05-clean-code/clean-floating-main-rs"),
        Act(|e, _, _| detach_named(e, "app.ts")),
        Wait(0.6),
        Shot("05-clean-code/clean-two-surfaces"),
        Act(|e, _, _| {
            let name = std::ffi::OsStr::new("app.ts");
            if let Some(doc) = e
                .documents
                .iter_mut()
                .find(|d| d.path.file_name() == Some(name))
            {
                doc.text.insert_str(0, "// edited in a floating editor\n");
                doc.update_dirty();
            }
        }),
        Wait(0.3),
        Shot("05-clean-code/clean-edit-dirty"),
        Act(|e, _, _| e.clean.focus = true),
        Move(0.12, 0.85),
        Wait(0.7),
        Shot("05-clean-code/clean-focus-mode"),
        Act(|e, c, _| e.dock_all(c.input(|i| i.time))),
        Wait(0.08),
        Shot("11-motion/clean-dock-1"),
        Wait(0.12),
        Shot("11-motion/clean-dock-2"),
        Wait(0.5),
        Shot("05-clean-code/clean-docked-back"),
        // Agent Hub and the Codex panel with the official Blossom.
        Act(|e, _, _| {
            e.hub_open = true;
            e.review_fixture("conversation");
        }),
        Wait(0.5),
        Shot("06-agent-hub/agent-hub-conversation"),
        Act(|e, _, _| e.review_fixture("tasks")),
        Wait(0.4),
        Shot("06-agent-hub/agent-hub-tasks"),
        Act(|e, _, _| {
            e.hub_open = false;
            e.chat.messages.clear();
        }),
        Wait(0.4),
        Shot("06-agent-hub/codex-panel-openai-mark"),
        // Companion spring.
        Act(|e, _, _| e.companion.open = true),
        Wait(0.04),
        Shot("11-motion/companion-spring-1"),
        Wait(0.08),
        Shot("11-motion/companion-spring-2"),
        Wait(0.5),
        Shot("07-companion/companion-expanded"),
        Key(Key::Escape, Modifiers::NONE),
        Act(|e, _, _| e.companion.clear_external()),
        Wait(0.5),
    ]);
    let moods: [(Act, &'static str); 4] = [
        (
            |e, _, _| e.mood_override = Some(M::Idle),
            "08-luma/luma-idle",
        ),
        (
            |e, _, _| e.mood_override = Some(M::Thinking),
            "08-luma/luma-thinking",
        ),
        (
            |e, _, _| e.mood_override = Some(M::Success),
            "08-luma/luma-success",
        ),
        (
            |e, _, _| e.mood_override = Some(M::Approval),
            "08-luma/luma-approval",
        ),
    ];
    for (act, name) in moods {
        s.extend([Act(act), Wait(0.3), Crop(name, "Luma", 5.0)]);
    }
    s.extend([
        Act(|e, _, _| e.mood_override = None),
        Move(0.06, 0.88),
        Wait(0.3),
        Crop("08-luma/luma-hover-look", "Luma", 5.0),
        Act(|e, _, _| e.companion.open = false),
    ]);
    let themes: [(Act, &'static str); 6] = [
        (
            |e, c, _| theme(e, c, theme::Theme::Nocturne),
            "09-themes/editor-nocturne",
        ),
        (
            |e, c, _| theme(e, c, theme::Theme::Dark),
            "09-themes/editor-graphite",
        ),
        (
            |e, c, _| theme(e, c, theme::Theme::Midnight),
            "09-themes/editor-midnight",
        ),
        (
            |e, c, _| theme(e, c, theme::Theme::Violet),
            "09-themes/editor-violet",
        ),
        (
            |e, c, _| theme(e, c, theme::Theme::Light),
            "09-themes/editor-light",
        ),
        (
            |e, c, _| theme(e, c, theme::Theme::HighContrast),
            "09-themes/editor-high-contrast",
        ),
    ];
    for (act, name) in themes {
        s.extend([Act(act), Wait(0.4), Shot(name)]);
    }
    // Home background: the reference interactions (trail, click ripple) on a real window.
    s.extend([
        Act(|e, c, _| {
            theme(e, c, theme::Theme::Light);
            background(e, B::Dither);
        }),
        Move(0.80, 0.35),
        Wait(0.6),
        Shot("09-themes/home-light-dither"),
        Act(|e, c, _| theme(e, c, theme::Theme::Nocturne)),
        Move(0.30, 0.96),
        Wait(1.2),
        Shot("01-home/dither-field-rest"),
        Move(0.58, 0.22),
        Move(0.64, 0.30),
        Move(0.70, 0.40),
        Move(0.76, 0.52),
        Move(0.82, 0.62),
        Wait(0.02),
        Shot("01-home/dither-field-cursor-trail"),
        Wait(1.0),
        Shot("01-home/dither-field-trail-faded"),
        Tap(0.78, 0.45),
        Wait(0.12),
        Shot("11-motion/dither-ripple-1"),
        Wait(0.35),
        Shot("11-motion/dither-ripple-2"),
        Wait(0.6),
        Shot("11-motion/dither-ripple-3"),
        Act(|e, _, _| background(e, B::Bloom)),
        Move(0.30, 0.96),
        Wait(0.8),
        Shot("01-home/dither-bloom"),
        Act(|e, _, _| background(e, B::Orbital)),
        Wait(0.6),
        Shot("01-home/orbital-legacy"),
        Act(|e, _, _| {
            background(e, B::Dither);
            e.preferences.assistant_open = true;
            e.settings_open = true;
        }),
        Wait(0.5),
        Shot("10-settings/settings-appearance-en"),
        Act(|e, _, _| e.preferences.language = super::i18n::Language::Spanish),
        Wait(0.4),
        Shot("10-settings/settings-es"),
        Act(|e, _, _| {
            e.settings_open = false;
            e.preferences.language = super::i18n::Language::English;
        }),
        Act(|e, _, _| e.preferences.reduce_motion = true),
        Tap(0.78, 0.45),
        Wait(0.4),
        Shot("11-motion/reduced-motion-no-ripple"),
        Act(|e, _, _| e.preferences.reduce_motion = false),
    ]);
    s
}

thread_local! {
    static HOOK: std::cell::RefCell<Option<std::process::Child>> =
        const { std::cell::RefCell::new(None) };
}

struct Harness {
    editor: Editor,
    root: PathBuf,
    steps: Vec<Step>,
    stage: usize,
    at: Instant,
    delay: f32,
    queued: Vec<Event>,
    requested: bool,
    output: PathBuf,
    suffix: String,
    scale: f32,
    deadline: Instant,
    manifest: Vec<serde_json::Value>,
    last_action: Instant,
    render_ms: Vec<f64>,
}

impl eframe::App for Harness {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, input: &mut egui::RawInput) {
        input.focused = true;
        input.events.append(&mut self.queued);
    }
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        assert!(
            Instant::now() < self.deadline,
            "evidence capture timed out at step {}",
            self.stage
        );
        if (ctx.pixels_per_point() - self.scale).abs() > 0.01 {
            ctx.set_pixels_per_point(self.scale);
        }
        let start = Instant::now();
        self.editor.update(ctx, frame);
        self.render_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        let image = ctx.input(|i| {
            i.events.iter().find_map(|event| match event {
                Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image
            && self.requested
        {
            let (name, crop) = match self.steps[self.stage] {
                Shot(name) => (name, None),
                Crop(name, target, zoom) => (name, Some((target, zoom))),
                _ => unreachable!(),
            };
            let file = format!("{name}{}.png", self.suffix);
            let path = self.output.join(&file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let mut img = image::RgbaImage::from_raw(
                image.size[0] as u32,
                image.size[1] as u32,
                image.pixels.iter().flat_map(|p| p.to_array()).collect(),
            )
            .unwrap();
            if let Some((target, zoom)) = crop {
                let rect = targets(ctx)
                    .get(target)
                    .copied()
                    .unwrap_or_else(|| panic!("missing crop target {target}"));
                let ppp = ctx.pixels_per_point();
                let r = rect.expand(14.0);
                let x = (r.min.x * ppp).max(0.0) as u32;
                let y = (r.min.y * ppp).max(0.0) as u32;
                let w = ((r.width() * ppp) as u32).min(img.width() - x);
                let h = ((r.height() * ppp) as u32).min(img.height() - y);
                let cropped = image::imageops::crop_imm(&img, x, y, w, h).to_image();
                img = image::imageops::resize(
                    &cropped,
                    (w as f32 * zoom) as u32,
                    (h as f32 * zoom) as u32,
                    image::imageops::FilterType::Lanczos3,
                );
            }
            img.save(&path).unwrap();
            self.manifest.push(serde_json::json!({
                "file": file,
                "theme": self.editor.preferences.theme.name(),
                "language": self.editor.preferences.language.native_name(),
                "pixels": [img.width(), img.height()],
                "window_points": [ctx.content_rect().width(), ctx.content_rect().height()],
                "pixels_per_point": ctx.pixels_per_point(),
                "ms_since_last_action": self.last_action.elapsed().as_millis() as u64,
                "upscaled_crop": crop.is_some(),
            }));
            self.stage += 1;
            self.requested = false;
            self.delay = 0.0;
            self.at = Instant::now();
        }
        if self.stage == self.steps.len() {
            self.finish(ctx);
            return;
        }
        if self.requested || self.at.elapsed().as_secs_f32() < self.delay {
            ctx.request_repaint_after(Duration::from_millis(8));
            return;
        }
        let size = ctx.content_rect().size();
        match &self.steps[self.stage] {
            Act(f) => {
                f(&mut self.editor, ctx, &self.root);
                self.last_action = Instant::now();
            }
            Click(label) => {
                let rect = targets(ctx)
                    .get(*label)
                    .copied()
                    .unwrap_or_else(|| panic!("missing control {label} at step {}", self.stage));
                self.tap(rect.center());
            }
            Move(x, y) => self
                .queued
                .push(Event::PointerMoved(egui::pos2(x * size.x, y * size.y))),
            Tap(x, y) => self.tap(egui::pos2(x * size.x, y * size.y)),
            Key(key, modifiers) => {
                for pressed in [true, false] {
                    self.queued.push(Event::Key {
                        key: *key,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers: *modifiers,
                    });
                }
                self.last_action = Instant::now();
            }
            Wait(seconds) => {
                self.delay = *seconds;
                self.stage += 1;
                self.at = Instant::now();
                ctx.request_repaint();
                return;
            }
            Shot(_) | Crop(..) => {
                ctx.send_viewport_cmd(ViewportCommand::Screenshot(UserData::default()));
                self.requested = true;
                ctx.request_repaint();
                return;
            }
            Until(check, what) => {
                if !check(&self.editor) {
                    assert!(
                        self.at.elapsed() < Duration::from_secs(20),
                        "timed out waiting for {what}"
                    );
                    ctx.request_repaint_after(Duration::from_millis(30));
                    return;
                }
            }
            UntilFor(check, what, secs) => {
                if !check(&self.editor) {
                    assert!(
                        self.at.elapsed() < Duration::from_secs(*secs),
                        "timed out waiting for {what}"
                    );
                    ctx.request_repaint_after(Duration::from_millis(100));
                    return;
                }
            }
        }
        self.delay = 0.12;
        self.stage += 1;
        self.at = Instant::now();
        ctx.request_repaint();
    }
}

fn targets(ctx: &egui::Context) -> std::collections::HashMap<String, egui::Rect> {
    ctx.data(|d| d.get_temp(egui::Id::new("native-review-targets")))
        .unwrap_or_default()
}

impl Harness {
    fn tap(&mut self, pos: egui::Pos2) {
        self.queued.push(Event::PointerMoved(pos));
        for pressed in [true, false] {
            self.queued.push(Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
        }
        self.last_action = Instant::now();
    }
    fn finish(&mut self, ctx: &egui::Context) {
        HOOK.with(|h| {
            if let Some(mut child) = h.borrow_mut().take() {
                let _ = child.kill();
            }
        });
        self.render_ms.sort_by(f64::total_cmp);
        let n = self.render_ms.len();
        let pct = |p: f64| self.render_ms[((n - 1) as f64 * p).round() as usize];
        let report = serde_json::json!({
            "captures": self.manifest,
            "ui_build_cpu_ms": {"samples": n, "p50": pct(0.5), "p95": pct(0.95), "max": pct(1.0)},
            "input": "eframe raw_input_hook in a native window (not OS-level SendInput)",
        });
        std::fs::write(
            self.output
                .join("reports")
                .join(format!("manifest{}.json", self.suffix)),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        self.editor.force_close = true;
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }
}

#[test]
#[ignore = "opens a native window and writes the visual evidence set"]
fn capture_visual_review() -> Result<(), Box<dyn std::error::Error>> {
    use winit::platform::windows::EventLoopBuilderExtWindows;
    let output = PathBuf::from(
        std::env::var_os("COVALOS_EVIDENCE_DIR")
            .or_else(|| std::env::var_os("AEROS_EVIDENCE_DIR"))
            .ok_or("set COVALOS_EVIDENCE_DIR")?,
    );
    std::fs::create_dir_all(output.join("reports"))?;
    let env = |k: &str, legacy: &str, d: f32| {
        std::env::var(k)
            .or_else(|_| std::env::var(legacy))
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(d)
    };
    let (width, height, scale) = (
        env("COVALOS_EVIDENCE_WIDTH", "AEROS_EVIDENCE_WIDTH", 1366.0),
        env("COVALOS_EVIDENCE_HEIGHT", "AEROS_EVIDENCE_HEIGHT", 768.0),
        env("COVALOS_EVIDENCE_SCALE", "AEROS_EVIDENCE_SCALE", 1.0),
    );
    let matrix = std::env::var("COVALOS_EVIDENCE_SET")
        .or_else(|_| std::env::var("AEROS_EVIDENCE_SET"))
        .as_deref()
        == Ok("matrix");
    let suffix = format!(
        "-{}x{}@{}",
        width as u32,
        height as u32,
        (scale * 100.0).round() as u32
    );
    let fixture = tempfile::tempdir()?;
    // Keep preferences and the agent bridge out of the real user profile; the hook child
    // process inherits this.
    let config = fixture.path().join("config");
    std::fs::create_dir_all(&config)?;
    // SAFETY: set before any thread of this test reads the environment.
    unsafe { std::env::set_var("COVALOS_CONFIG_DIR", &config) };
    let root = fixture.path().join("orbit-ledger");
    write_fixture(&root)?;
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([width, height])
            .with_decorations(false)
            .with_icon(crate::brand::window_icon())
            .with_title("Covalos visual evidence"),
        event_loop_builder: Some(Box::new(|b| {
            b.with_any_thread(true);
        })),
        ..Default::default()
    };
    eframe::run_native(
        "Covalos visual evidence",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_pixels_per_point(scale);
            let mut preferences = Preferences::default();
            preferences.language = super::i18n::Language::English;
            preferences.recent = vec![root.with_file_name("covalos-site")];
            let mut editor = Editor::new(&cc.egui_ctx, Some(root.clone()), preferences);
            editor.sidebar = Sidebar::Files;
            Ok(Box::new(Harness {
                editor,
                root,
                steps: steps(matrix),
                stage: 0,
                at: Instant::now(),
                delay: 0.8,
                queued: Vec::new(),
                requested: false,
                output,
                suffix,
                scale,
                deadline: Instant::now() + Duration::from_secs(420),
                manifest: Vec::new(),
                last_action: Instant::now(),
                render_ms: Vec::new(),
            }))
        }),
    )?;
    drop(fixture);
    Ok(())
}

fn library_steps() -> Vec<Step> {
    use super::studio::View;
    use crate::library::Category;
    vec![
        Act(|e, c, _| {
            theme(e, c, theme::Theme::Nocturne);
            e.preferences.language = super::i18n::Language::English;
            e.preferences.assistant_open = false;
            e.open_library();
            e.studio.focus_search = false;
        }),
        Wait(0.6),
        Shot("01-offline-before-sync"),
        Act(|e, c, _| e.studio.start_sync(c)),
        Until(|e| !e.studio.syncing(), "catalog sync"),
        Wait(0.8),
        Shot("02-discover"),
        Act(|e, _, _| e.studio.query.text = "animated button".into()),
        Wait(0.5),
        Shot("03-search-animated-button"),
        Act(|e, _, _| e.studio.query.text = "react chart".into()),
        Wait(0.4),
        Shot("04-search-react-chart"),
        Act(|e, _, _| {
            e.studio.query.text.clear();
            e.studio.view = View::Category(Category::Backgrounds);
            e.studio.query.free_only = true;
        }),
        Wait(0.5),
        Shot("05-category-backgrounds-free"),
        Act(|e, _, _| {
            e.studio.query.free_only = false;
            e.studio.view = View::Discover;
            e.studio.select("covalos/dither-field");
            e.library_open_playground_tab();
        }),
        Wait(0.8),
        Shot("06-native-dither-playground"),
        Act(|e, _, _| {
            e.studio.select("covalos/spring-smooth");
            e.library_open_playground_tab();
        }),
        Wait(0.5),
        Shot("07-native-spring-playground"),
        Act(|e, _, _| e.studio.select("shadcn/button")),
        Wait(0.5),
        Shot("08-shadcn-button-overview"),
        Act(|e, _, _| e.library_open_code_tab()),
        Wait(0.3),
        Click("Load source"),
        Until(|e| e.library_item_loaded(), "registry item"),
        Wait(0.4),
        Shot("09-shadcn-button-source"),
        Act(|e, _, _| e.library_open_install_tab()),
        Wait(0.3),
        Click("Add to project"),
        Until(|e| e.library_review_ready(), "install plan"),
        Wait(0.4),
        Shot("10-install-review"),
        Act(|e, _, _| {
            e.studio.select("react-bits/BlurText");
        }),
        Wait(0.5),
        Shot("11-commons-clause-entry"),
        Act(|e, _, _| {
            e.studio.view = View::Sources;
            e.studio.selected = None;
        }),
        Wait(0.5),
        Shot("12-sources-licenses"),
        Act(|e, _, _| {
            e.preferences.language = super::i18n::Language::Spanish;
            e.studio.view = View::Discover;
        }),
        Wait(0.5),
        Shot("13-discover-es"),
    ]
}

#[test]
#[ignore = "opens a native window, syncs the official registries and writes Library Studio evidence"]
fn capture_library_review() -> Result<(), Box<dyn std::error::Error>> {
    use winit::platform::windows::EventLoopBuilderExtWindows;
    let output = PathBuf::from(
        std::env::var_os("COVALOS_EVIDENCE_DIR")
            .or_else(|| std::env::var_os("AEROS_EVIDENCE_DIR"))
            .ok_or("set COVALOS_EVIDENCE_DIR")?,
    );
    std::fs::create_dir_all(output.join("reports"))?;
    let fixture = tempfile::tempdir()?;
    let config = fixture.path().join("config");
    std::fs::create_dir_all(&config)?;
    // SAFETY: set before any thread of this test reads the environment.
    unsafe { std::env::set_var("COVALOS_CONFIG_DIR", &config) };
    // A temporary React + Tailwind + shadcn workspace; nothing is installed into it.
    let root = fixture.path().join("react-app");
    std::fs::create_dir_all(root.join("src/components/ui"))?;
    std::fs::write(
        root.join("package.json"),
        r#"{"name":"react-app","private":true,"dependencies":{"react":"^19.0.0","react-dom":"^19.0.0"},"devDependencies":{"typescript":"^5.6.0","tailwindcss":"^4.0.0","@tailwindcss/vite":"^4.0.0","vite":"^6.0.0"}}"#,
    )?;
    std::fs::write(
        root.join("tsconfig.json"),
        r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["./src/*"]}}}"#,
    )?;
    std::fs::write(
        root.join("components.json"),
        r#"{"style":"new-york","tsx":true,"aliases":{"components":"@/components","ui":"@/components/ui","utils":"@/lib/utils"}}"#,
    )?;
    std::fs::write(root.join("package-lock.json"), "{}")?;
    std::fs::write(root.join("src/main.tsx"), "export {};\n")?;
    let (width, height) = (1366.0, 768.0);
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([width, height])
            .with_decorations(false)
            .with_icon(crate::brand::window_icon())
            .with_title("Covalos Library Studio evidence"),
        event_loop_builder: Some(Box::new(|b| {
            b.with_any_thread(true);
        })),
        ..Default::default()
    };
    eframe::run_native(
        "Covalos Library Studio evidence",
        options,
        Box::new(move |cc| {
            let mut preferences = Preferences::default();
            preferences.language = super::i18n::Language::English;
            let editor = Editor::new(&cc.egui_ctx, Some(root.clone()), preferences);
            Ok(Box::new(Harness {
                editor,
                root,
                steps: library_steps(),
                stage: 0,
                at: Instant::now(),
                delay: 0.8,
                queued: Vec::new(),
                requested: false,
                output,
                suffix: "-1366x768@100".into(),
                scale: 1.0,
                deadline: Instant::now() + Duration::from_secs(300),
                manifest: Vec::new(),
                last_action: Instant::now(),
                render_ms: Vec::new(),
            }))
        }),
    )?;
    drop(fixture);
    Ok(())
}

fn write_fixture(root: &Path) -> std::io::Result<()> {
    for dir in [
        "src",
        "web",
        "tools",
        "tests",
        "docs",
        "assets",
        ".github/workflows",
        "target",
    ] {
        std::fs::create_dir_all(root.join(dir))?;
    }
    let files: [(&str, &str); 15] = [
        ("src/main.rs", RUST),
        ("web/app.ts", TYPESCRIPT),
        ("tools/analyze.py", PYTHON),
        (
            "Cargo.toml",
            "[package]\nname = \"orbit-ledger\"\nversion = \"0.4.2\"\nedition = \"2024\"\n\n# Runtime\n[dependencies]\nserde = { version = \"1\", features = [\"derive\"] }\nrayon = \"1.10\"\n\n[profile.release]\nlto = true\ncodegen-units = 1\n",
        ),
        (
            "package.json",
            "{\n  \"name\": \"orbit-ledger-web\",\n  \"private\": true,\n  \"scripts\": { \"dev\": \"vite\" }\n}\n",
        ),
        (
            "tsconfig.json",
            "{\n  \"compilerOptions\": { \"strict\": true, \"target\": \"ES2022\" }\n}\n",
        ),
        (
            "README.md",
            "# Orbit Ledger\n\nA small ledger used to review Covalos.\n",
        ),
        (".gitignore", "/target\nnode_modules/\n"),
        ("Dockerfile", "FROM rust:1.89\nWORKDIR /app\nCOPY . .\n"),
        ("LICENSE", "MIT\n"),
        ("tests/ledger.rs", "#[test]\nfn balances() {}\n"),
        ("docs/guide.md", "# Guide\n"),
        (
            "assets/logo.svg",
            "<svg xmlns=\"http://www.w3.org/2000/svg\"/>\n",
        ),
        (".github/workflows/ci.yml", "name: ci\non: [push]\n"),
        ("OrbitLedger.sln", "Microsoft Visual Studio Solution File\n"),
    ];
    for (path, text) in files {
        std::fs::write(root.join(path), text)?;
    }
    // A real repository with staged, modified, deleted and untracked files for the Git view.
    let git = |args: &[&str]| -> std::io::Result<()> {
        let status = std::process::Command::new("git")
            .current_dir(root)
            .args([
                "-c",
                "core.autocrlf=false",
                "-c",
                "user.name=Covalos Fixture",
            ])
            .args([
                "-c",
                "user.email=fixture@example.invalid",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other(format!("git {args:?} failed")))
        }
    };
    git(&["init", "-q", "-b", "main"])?;
    git(&["add", "-A"])?;
    git(&["commit", "-q", "-m", "Initial ledger"])?;
    let main = root.join("src/main.rs");
    let text = std::fs::read_to_string(&main)?.replace(
        "    println!(\"accounts: {}\", totals.len());",
        "    let largest = totals.values().cloned().fold(0.0, f64::max);
    println!(\"accounts: {} largest: {largest:.2}\", totals.len());",
    );
    std::fs::write(&main, text)?;
    let app = root.join("web/app.ts");
    let text = std::fs::read_to_string(&app)?.replace("page = 1", "page = 1, signal?: AbortSignal");
    std::fs::write(&app, text)?;
    git(&["add", "web/app.ts"])?;
    std::fs::remove_file(root.join("docs/guide.md"))?;
    std::fs::create_dir_all(root.join("notes"))?;
    std::fs::write(
        root.join("notes/todo.md"),
        "- reconcile fees
",
    )?;
    std::fs::write(
        root.join("README.md"),
        "# Orbit Ledger

A small ledger used to review Covalos.
Now with largest balance.
",
    )?;
    Ok(())
}

const RUST: &str = r#"use std::collections::HashMap;
use std::fmt;

/// Maximum number of entries kept in memory.
const MAX_ENTRIES: usize = 4_096;

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Credit,
    Debit { reason: String },
}

#[derive(Debug, Clone)]
pub struct Entry<'a> {
    pub account: &'a str,
    pub amount: f64,
    pub kind: Kind,
}

impl fmt::Display for Entry<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:>12} {:+.2}", self.account, self.amount)
    }
}

/// Sum balances per account, ignoring empty names.
pub fn summarize(entries: &[Entry<'_>]) -> HashMap<String, f64> {
    let mut totals = HashMap::with_capacity(entries.len().min(MAX_ENTRIES));
    for entry in entries.iter().filter(|e| !e.account.is_empty()) {
        let sign = match &entry.kind {
            Kind::Credit => 1.0,
            Kind::Debit { .. } => -1.0,
        };
        *totals.entry(entry.account.to_owned()).or_insert(0.0) += sign * entry.amount;
    }
    totals
}

fn main() {
    let entries = vec![
        Entry { account: "orbit", amount: 120.5, kind: Kind::Credit },
        Entry { account: "orbit", amount: 20.0, kind: Kind::Debit { reason: "fees".into() } },
    ];
    let totals = summarize(&entries);
    println!("accounts: {}", totals.len());
}
"#;

const TYPESCRIPT: &str = r#"import { formatCurrency } from "./format";

export interface Order {
  id: string;
  total: number;
  status: "open" | "paid" | "refunded";
}

const API_URL = "https://example.invalid/api";

export class OrderStore {
  private readonly cache = new Map<string, Order>();

  constructor(private readonly pageSize = 50) {}

  get size(): number {
    return this.cache.size;
  }
}

/** Load paid orders and return a short report. */
export async function loadOrders(store: OrderStore, page = 1): Promise<string[]> {
  const response = await fetch(`${API_URL}/orders?page=${page}`);
  if (!response.ok) {
    throw new Error(`Request failed: ${response.status}`);
  }
  const orders: Order[] = await response.json();
  return orders
    .filter((order) => order.status === "paid")
    .map((order) => `${order.id}: ${formatCurrency(order.total)}`);
}
"#;

const PYTHON: &str = r#"from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path

THRESHOLD = 0.75


@dataclass
class Sample:
    name: str
    values: list[float] = field(default_factory=list)

    @property
    def mean(self) -> float:
        return sum(self.values) / len(self.values) if self.values else 0.0


def summarize(samples: list[Sample], *, strict: bool = False) -> dict[str, float]:
    """Return the mean of every sample above the threshold."""
    result = {}
    for sample in samples:
        if strict and not sample.values:
            raise ValueError(f"empty sample: {sample.name}")
        if sample.mean >= THRESHOLD:
            result[sample.name] = round(sample.mean, 3)
    return result


if __name__ == "__main__":
    data = json.loads(Path("samples.json").read_text())
    print(summarize([Sample(**item) for item in data]))
"#;

fn agents(editor: &mut Editor) -> &mut crate::agents_runtime::Runtime {
    editor
        .hub
        .agents
        .runtime
        .as_mut()
        .expect("agent runtime open")
}

fn agents_ref(editor: &Editor) -> Option<&crate::agents_runtime::Runtime> {
    editor.hub.agents.runtime.as_ref()
}

fn session_index(editor: &Editor, title: &str) -> Option<usize> {
    agents_ref(editor)?
        .sessions
        .iter()
        .position(|s| s.meta.title == title)
}

fn idle(editor: &Editor, title: &str) -> bool {
    session_index(editor, title)
        .and_then(|i| agents_ref(editor).map(|r| !r.sessions[i].running()))
        .unwrap_or(false)
}

fn select(editor: &mut Editor, title: &str) {
    editor.hub.agents.selected = session_index(editor, title);
}

fn add_account(
    editor: &mut Editor,
    id: &str,
    provider: covalos_agents::ProviderKind,
    label: &str,
    method: covalos_agents::ConnectionMethod,
) {
    let account = covalos_agents::Account::new(
        covalos_agents::AccountId::new(id).unwrap(),
        provider,
        label,
        method,
    );
    agents(editor).add_account(account).unwrap();
}

fn start(
    editor: &mut Editor,
    root: &Path,
    title: &str,
    worktree: bool,
    preset: covalos_agents::permission::Preset,
    prompt: &str,
) {
    let runtime = agents(editor);
    let index = runtime
        .create_session(crate::agents_runtime::NewSession {
            title: title.into(),
            account: covalos_agents::AccountId::new("codex-main").unwrap(),
            model: None,
            root: root.to_path_buf(),
            worktree,
            preset,
            allow_safe_commands: true,
            share_context: true,
        })
        .unwrap();
    runtime.send(index, prompt).unwrap();
    editor.hub.agents.selected = Some(index);
}

const A: &str = "Agent A - add mul()";
const B: &str = "Agent B - add tests";
const C: &str = "Agent C - reads shared context";
const D: &str = "Agent D - cancelled";

fn agent_steps() -> Vec<Step> {
    use covalos_agents::{ConnectionMethod::*, ProviderKind::*, permission::Preset};
    vec![
        Act(|e, c, _| {
            e.hub_open = true;
            e.hub.focus_agents("accounts");
            e.ensure_runtime(c);
        }),
        UntilFor(
            |e| {
                agents_ref(e).is_some_and(|r| {
                    !r.detections.is_empty() && r.detections.values().all(|d| d.done)
                })
            },
            "provider detection",
            90,
        ),
        Wait(0.6),
        Shot("agents/01-providers-detected"),
        Act(|e, _, _| {
            add_account(
                e,
                "codex-main",
                Codex,
                "Codex - ChatGPT (system CLI)",
                SystemCli,
            );
            add_account(
                e,
                "codex-isolated",
                Codex,
                "Codex - second account (isolated)",
                OfficialLogin,
            );
            add_account(
                e,
                "claude-main",
                ClaudeCode,
                "Claude - system CLI",
                SystemCli,
            );
            add_account(
                e,
                "claude-work",
                ClaudeCode,
                "Claude - work (isolated)",
                OfficialLogin,
            );
            add_account(
                e,
                "claude-alt",
                ClaudeCode,
                "Claude - alt (isolated)",
                OfficialLogin,
            );
            e.evidence_account_status();
        }),
        Wait(0.6),
        Shot("agents/02-accounts-multi"),
        // Goal 2: two agents, distinct tasks, independent worktrees, in parallel.
        Act(|e, _, root| {
            e.hub.focus_agents("sessions");
            start(
                e,
                root,
                A,
                true,
                Preset::Worktree,
                "In calc.py add a function mul(a, b) that returns a * b. Edit only calc.py. \
                 Reply with one short sentence.",
            );
            start(
                e,
                root,
                B,
                true,
                Preset::Worktree,
                "Create test_calc.py with two pytest tests, one for add and one for sub from \
                 calc.py. Create only that file. Reply with one short sentence.",
            );
            select(e, A);
        }),
        Wait(4.0),
        Shot("agents/03-two-agents-running"),
        UntilFor(|e| idle(e, A) && idle(e, B), "agents A and B", 420),
        Act(|e, _, _| select(e, A)),
        Wait(0.6),
        Shot("agents/04-agent-a-finished"),
        Act(|e, _, _| select(e, B)),
        Wait(0.6),
        Shot("agents/05-agent-b-finished"),
        // Shared context: a third agent reads what A and B published.
        Act(|e, _, root| {
            start(
                e,
                root,
                C,
                false,
                Preset::ReadOnly,
                "Using only the shared Covalos context above, say in one sentence what the \
                 other agents reported. Do not read or change files.",
            );
        }),
        UntilFor(|e| idle(e, C), "agent C", 240),
        Wait(0.6),
        Shot("agents/06-shared-context"),
        // Cancellation.
        Act(|e, _, root| {
            start(
                e,
                root,
                D,
                false,
                Preset::ReadOnly,
                "Explain calc.py line by line in exhaustive detail with many examples.",
            );
        }),
        Wait(5.0),
        Act(|e, _, _| {
            let i = session_index(e, D).unwrap();
            agents(e).cancel(i);
        }),
        UntilFor(|e| idle(e, D), "cancellation", 60),
        Wait(0.6),
        Shot("agents/07-cancelled"),
        // Recovery: the UI state goes away mid-turn and a fresh runtime reopens it.
        Act(|e, _, _| {
            let i = session_index(e, C).unwrap();
            agents(e)
                .send(i, "Reply with the single word READY.")
                .unwrap();
        }),
        Wait(1.0),
        Act(|e, c, _| {
            e.hub.agents.runtime = None;
            e.ensure_runtime(c);
            select(e, C);
        }),
        Wait(0.8),
        Shot("agents/08-recovered-interrupted"),
        Act(|e, _, _| {
            let i = session_index(e, C).unwrap();
            agents(e)
                .send(
                    i,
                    "We were interrupted. Reply with the single word RESUMED.",
                )
                .unwrap();
        }),
        UntilFor(|e| idle(e, C), "resumed agent C", 240),
        Wait(0.6),
        Shot("agents/09-resumed"),
        // Goal 3: a small objective through the autonomous engine.
        Act(|e, _, root| {
            e.hub.focus_agents("autonomy");
            let budget = covalos_engine::budget::Budget {
                wall_ms: Some(10 * 60_000),
                tokens: None,
                cost_micros: None,
            };
            agents(e)
                .start_plan(
                    "Review calc.py and publish one concrete improvement as a review entry.",
                    crate::agents_runtime::PlanMode::Single {
                        role: "reviewer".into(),
                    },
                    root,
                    crate::agents_runtime::PlanSetup {
                        worktrees: true,
                        budget,
                        max_parallel: 1,
                        pins: Default::default(),
                    },
                )
                .unwrap();
        }),
        Wait(3.0),
        Shot("agents/10-autonomous-running"),
        UntilFor(
            |e| {
                agents_ref(e)
                    .and_then(|r| r.plan.as_ref())
                    .is_some_and(|p| p.engine.plan().state.is_settled())
            },
            "autonomous plan",
            600,
        ),
        Wait(0.6),
        Shot("agents/11-autonomous-settled"),
        Act(|e, _, root| write_agent_report(e, root)),
    ]
}

fn git_out(dir: &Path, args: &[&str]) -> String {
    std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default()
}

fn write_agent_report(editor: &mut Editor, root: &Path) {
    let output = PathBuf::from(std::env::var_os("COVALOS_EVIDENCE_DIR").unwrap());
    let runtime = agents(editor);
    let sessions: Vec<_> = runtime
        .sessions
        .iter()
        .map(|s| {
            let m = &s.meta;
            serde_json::json!({
                "title": m.title, "status": m.status.label(), "turns": m.turns,
                "provider_session": m.provider_session, "model": m.model,
                "workdir": m.workdir, "branch": m.worktree_branch,
                "tokens": [m.input_tokens, m.output_tokens],
                "git_status": git_out(&m.workdir, &["status", "--porcelain"]),
                "log": m.log.iter().map(|l| format!("[{}] {}", l.kind, l.text)).collect::<Vec<_>>(),
                "gate_audit": runtime.audit(&m.id),
            })
        })
        .collect();
    let context: Vec<_> = runtime.context.since(0).cloned().collect();
    let plan = runtime.plan.as_ref().map(|run| {
        let plan = run.engine.plan();
        serde_json::json!({
            "goal": plan.goal, "state": plan.state, "spend": plan.spend,
            "tasks": plan.tasks().collect::<Vec<_>>(),
            "context": run.engine.context().since(0).cloned().collect::<Vec<_>>(),
            "journal": run.engine.journal_path(),
            "error": run.error,
        })
    });
    let detections: Vec<_> = runtime
        .detections
        .iter()
        .map(|(k, d)| {
            serde_json::json!({
                "provider": k.key(), "program": d.program, "version": d.version, "login": d.login,
            })
        })
        .collect();
    let accounts: Vec<_> = runtime
        .profiles
        .accounts()
        .iter()
        .map(|a| {
            serde_json::json!({
                "id": a.id, "provider": a.provider.key(), "method": a.method,
                "status": runtime.account_status(&a.id).unwrap_or_else(|e| e),
            })
        })
        .collect();
    let report = serde_json::json!({
        "project": root,
        "main_status": git_out(root, &["status", "--porcelain"]),
        "worktrees": git_out(root, &["worktree", "list"]),
        "credential_backend": runtime.credential_backend,
        "detections": detections,
        "accounts": accounts,
        "sessions": sessions,
        "shared_context": context,
        "plan": plan,
    });
    std::fs::create_dir_all(output.join("reports")).unwrap();
    std::fs::write(
        output.join("reports/agent-hub-report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "runs real provider CLIs (uses a small amount of quota) and writes Agent Hub evidence"]
fn capture_agent_hub_review() -> Result<(), Box<dyn std::error::Error>> {
    use winit::platform::windows::EventLoopBuilderExtWindows;
    let output =
        PathBuf::from(std::env::var_os("COVALOS_EVIDENCE_DIR").ok_or("set COVALOS_EVIDENCE_DIR")?);
    let sandbox = PathBuf::from(
        std::env::var_os("COVALOS_AGENT_SANDBOX").ok_or("set COVALOS_AGENT_SANDBOX")?,
    );
    std::fs::create_dir_all(output.join("reports"))?;
    // COVALOS_AGENT_RUN keeps the project, worktrees and config for inspection.
    let fixture = tempfile::tempdir()?;
    let base = std::env::var_os("COVALOS_AGENT_RUN")
        .map(PathBuf::from)
        .unwrap_or_else(|| fixture.path().to_path_buf());
    assert!(
        !base.join("agent-sandbox").exists(),
        "COVALOS_AGENT_RUN must point to a fresh folder"
    );
    let config = base.join("config");
    std::fs::create_dir_all(&config)?;
    // SAFETY: set before any thread of this test reads the environment.
    unsafe { std::env::set_var("COVALOS_CONFIG_DIR", &config) };
    let root = base.join("agent-sandbox");
    let cloned = std::process::Command::new("git")
        .args(["clone", "-q"])
        .arg(&sandbox)
        .arg(&root)
        .status()?;
    assert!(cloned.success(), "git clone of the sandbox failed");
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([1366.0, 768.0])
            .with_decorations(false)
            .with_icon(crate::brand::window_icon())
            .with_title("Covalos Agent Hub evidence"),
        event_loop_builder: Some(Box::new(|b| {
            b.with_any_thread(true);
        })),
        ..Default::default()
    };
    eframe::run_native(
        "Covalos Agent Hub evidence",
        options,
        Box::new(move |cc| {
            let mut preferences = Preferences::default();
            preferences.language = super::i18n::Language::English;
            let editor = Editor::new(&cc.egui_ctx, Some(root.clone()), preferences);
            Ok(Box::new(Harness {
                editor,
                root,
                steps: agent_steps(),
                stage: 0,
                at: Instant::now(),
                delay: 0.8,
                queued: Vec::new(),
                requested: false,
                output,
                suffix: "-1366x768@100".into(),
                scale: 1.0,
                deadline: Instant::now() + Duration::from_secs(40 * 60),
                manifest: Vec::new(),
                last_action: Instant::now(),
                render_ms: Vec::new(),
            }))
        }),
    )?;
    // Keep sessions.json, the gate audit and plan journals next to the evidence.
    let output = PathBuf::from(std::env::var_os("COVALOS_EVIDENCE_DIR").unwrap());
    let _ = std::process::Command::new("robocopy")
        .arg(config.join("agents"))
        .arg(output.join("config-snapshot"))
        .args(["/E", "/NFL", "/NDL", "/NJH", "/NJS", "/XD", "worktrees"])
        .status();
    drop(fixture);
    Ok(())
}

fn replay_steps() -> Vec<Step> {
    vec![
        Act(|e, c, _| {
            e.hub_open = true;
            e.hub.focus_agents("accounts");
            e.ensure_runtime(c);
            e.evidence_account_status();
        }),
        UntilFor(
            |e| {
                agents_ref(e).is_some_and(|r| {
                    !r.detections.is_empty() && r.detections.values().all(|d| d.done)
                })
            },
            "provider detection",
            90,
        ),
        Wait(0.6),
        Shot("replay/01-accounts"),
        Act(|e, _, _| {
            e.hub.focus_agents("sessions");
            select(e, A);
        }),
        Wait(0.6),
        Shot("replay/02-session-a"),
        Act(|e, _, _| select(e, B)),
        Wait(0.6),
        Shot("replay/03-session-b"),
        Act(|e, _, _| select(e, C)),
        Wait(0.6),
        Shot("replay/04-session-c-recovered"),
        Act(|e, _, _| select(e, D)),
        Wait(0.6),
        Shot("replay/05-session-d-cancelled"),
        Act(|e, _, _| e.hub.focus_agents("autonomy")),
        Wait(0.6),
        Shot("replay/06-autonomy"),
        Act(|e, _, root| {
            e.hub_open = false;
            e.preferences.assistant_open = true;
            e.preferences.assistant_width = 420.0;
            let index = session_index(e, A).expect("session A");
            let id = agents_ref(e).expect("runtime").sessions[index]
                .meta
                .id
                .as_str()
                .to_owned();
            e.hub.agents.chat.dock = Some(id);
            if let Some(file) = ["calc.py", "README.md"]
                .iter()
                .map(|f| root.join(f))
                .find(|p| p.exists())
            {
                e.open_file(file);
            }
        }),
        Wait(0.8),
        Shot("replay/07-editor-dock"),
        Act(|e, _, _| e.hub.agents.chat.add_ai = Some(Default::default())),
        Wait(0.6),
        Shot("replay/08-add-ai"),
    ]
}

/// One real engine objective on a recorded run (COVALOS_AGENT_ENGINE=1).
fn engine_steps() -> Vec<Step> {
    vec![
        Act(|e, c, _| {
            e.hub_open = true;
            e.hub.focus_agents("autonomy");
            e.ensure_runtime(c);
        }),
        UntilFor(
            |e| {
                agents_ref(e).is_some_and(|r| {
                    !r.detections.is_empty() && r.detections.values().all(|d| d.done)
                })
            },
            "provider detection",
            90,
        ),
        Act(|e, _, root| {
            let budget = covalos_engine::budget::Budget {
                wall_ms: Some(10 * 60_000),
                tokens: None,
                cost_micros: None,
            };
            agents(e)
                .start_plan(
                    "Review calc.py and publish one concrete improvement as a review entry.",
                    crate::agents_runtime::PlanMode::Single {
                        role: "reviewer".into(),
                    },
                    root,
                    crate::agents_runtime::PlanSetup {
                        worktrees: true,
                        budget,
                        max_parallel: 1,
                        pins: Default::default(),
                    },
                )
                .unwrap();
        }),
        Wait(3.0),
        Shot("engine/01-autonomous-running"),
        UntilFor(
            |e| {
                agents_ref(e)
                    .and_then(|r| r.plan.as_ref())
                    .is_some_and(|p| p.engine.plan().state.is_settled())
            },
            "autonomous plan",
            600,
        ),
        Wait(0.6),
        Shot("engine/02-autonomous-settled"),
        Act(|e, _, root| write_agent_report(e, root)),
    ]
}

/// Title of the tour window; the capture below looks the window up by it.
const TOUR_TITLE: &str = "Covalos alpha tour capture";

/// Captures only the tour window's own pixels with `PrintWindow`
/// (PW_RENDERFULLCONTENT, which includes the WebView2 preview). Unlike a
/// screen-region copy, other windows on top can never end up in the image.
fn os_capture(_ctx: &egui::Context, path: &Path) {
    const SCRIPT: &str = r#"
Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'
using System;
using System.Drawing;
using System.Runtime.InteropServices;
public static class WindowShot {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int L, T, R, B; }
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern IntPtr FindWindow(string c, string t);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out Rect r);
    [DllImport("user32.dll")] static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
    public static string Save(string title, string path) {
        IntPtr h = FindWindow(null, title);
        if (h == IntPtr.Zero) return "window not found";
        Rect r; GetWindowRect(h, out r);
        using (var b = new Bitmap(r.R - r.L, r.B - r.T)) {
            using (var g = Graphics.FromImage(b)) {
                IntPtr dc = g.GetHdc();
                bool ok = PrintWindow(h, dc, 2);
                g.ReleaseHdc(dc);
                if (!ok) return "PrintWindow failed";
            }
            b.Save(path, System.Drawing.Imaging.ImageFormat.Png);
        }
        return "ok";
    }
}
'@
[WindowShot]::Save($args[0], $args[1])
"#;
    let _ = std::fs::create_dir_all(path.parent().unwrap_or(path));
    let script = std::env::temp_dir().join("covalos-window-shot.ps1");
    if std::fs::write(&script, SCRIPT).is_err() {
        return;
    }
    // Not awaited: PrintWindow asks this window's own UI thread to paint,
    // so the harness must keep running frames while the script captures.
    let _ = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&script)
        .arg(TOUR_TITLE)
        .arg(path)
        .spawn();
}

fn app_tsx(e: &Editor) -> Option<usize> {
    e.documents.iter().position(|d| d.path.ends_with("App.tsx"))
}

/// Puts `line` after the first line of App.tsx (in memory only) and moves
/// the caret to its end.
fn inject(e: &mut Editor, line: &str) {
    let Some(i) = app_tsx(e) else { return };
    let doc = &mut e.documents[i];
    let first = doc.text.find('\n').map_or(doc.text.len(), |n| n + 1);
    doc.text.insert_str(first, &format!("{line}\n"));
    doc.update_dirty();
    let caret = doc.text[..first].chars().count() + line.chars().count();
    e.active = Some(i);
    e.selection = Some((caret, caret));
    e.focus_editor = true;
}

fn uninject(e: &mut Editor, line: &str) {
    let Some(i) = app_tsx(e) else { return };
    let doc = &mut e.documents[i];
    doc.text = doc.text.replacen(&format!("{line}\n"), "", 1);
    doc.update_dirty();
}

const BAD: &str = "const count: number = \"five\";";
const PROBE: &str = "const probe = useSt";

/// Alpha tour on a real Vite React-TS project (COVALOS_LSP_PROJECT) with a
/// recorded agents config (COVALOS_AGENT_RUN): welcome, editor, LSP squiggle,
/// Problems, completion, fix, one small read-only Codex turn in the dock,
/// npm run dev with the preview, and the orchestrator. Nothing is saved.
fn tour_steps() -> Vec<Step> {
    vec![
        Wait(1.2),
        Shot("promo/01-welcome"),
        Act(|e, _, root| {
            e.open_workspace(root.to_path_buf());
            e.open_file(root.join("src").join("App.tsx"));
        }),
        UntilFor(
            |e| app_tsx(e).is_some_and(|i| e.lsp.harness_ready(&e.documents[i].path)),
            "TypeScript server",
            90,
        ),
        Wait(1.5),
        Shot("promo/02-editor-react"),
        Act(|e, _, _| {
            inject(e, BAD);
            e.show_tool("problems");
        }),
        UntilFor(
            |e| {
                app_tsx(e)
                    .is_some_and(|i| e.lsp.harness_has(&e.documents[i].path, "not assignable"))
            },
            "type error diagnostic",
            60,
        ),
        Wait(1.0),
        Shot("lsp/01-squiggle-and-problems"),
        Act(|e, _, _| {
            uninject(e, BAD);
            inject(e, PROBE);
        }),
        Wait(0.8),
        Key(Key::Space, Modifiers::CTRL),
        UntilFor(|e| e.lsp.harness_popup_items() > 0, "completion items", 30),
        Wait(0.8),
        Shot("lsp/02-completion-popup"),
        Act(|e, _, _| {
            e.lsp.harness_close_popup();
            uninject(e, PROBE);
        }),
        UntilFor(
            |e| {
                app_tsx(e).is_some_and(|i| {
                    let p = &e.documents[i].path;
                    !e.lsp.harness_has(p, "not assignable") && !e.lsp.harness_has(p, "useSt")
                })
            },
            "diagnostics cleared after the fix",
            60,
        ),
        Wait(0.8),
        Shot("lsp/03-fixed"),
        Act(|e, c, _| {
            e.ensure_runtime(c);
        }),
        UntilFor(
            |e| {
                agents_ref(e).is_some_and(|r| {
                    !r.detections.is_empty() && r.detections.values().all(|d| d.done)
                })
            },
            "provider detection",
            90,
        ),
        Act(|e, _, root| {
            use covalos_agents::{AccountId, permission::Preset};
            if std::env::var_os("COVALOS_TOUR_SKIP_AGENT").is_some() {
                return;
            }
            let r = agents(e);
            let index = r
                .create_session(crate::agents_runtime::NewSession {
                    title: "Explain App.tsx".into(),
                    account: AccountId::new("codex-main").unwrap(),
                    model: None,
                    root: root.to_path_buf(),
                    worktree: false,
                    preset: Preset::ReadOnly,
                    allow_safe_commands: false,
                    share_context: false,
                })
                .unwrap();
            r.send(
                index,
                "Active file in the editor: src/App.tsx\n\nIn two short sentences, what does this component render? Do not edit files.",
            )
            .unwrap();
            let id = r.sessions[index].meta.id.as_str().to_owned();
            e.hub.agents.chat.restored = true;
            e.hub.agents.chat.dock = Some(id);
            e.preferences.assistant_open = true;
            e.preferences.assistant_width = 430.0;
            e.preferences.tools_open = false;
        }),
        UntilFor(
            |e| agents_ref(e).is_some_and(|r| r.sessions.last().is_some_and(|s| !s.running())),
            "Codex turn",
            300,
        ),
        Wait(1.0),
        Shot("promo/03-agents-beside-code"),
        Act(|e, c, _| {
            e.preferences.assistant_open = false;
            e.show_tool("terminal");
            e.preferences.tools_height = 300.0;
            e.harness_open_terminal(c);
        }),
        UntilFor(|e| e.harness_prompt_ready(), "PowerShell prompt", 60),
        Wait(0.6),
        Act(|e, _, _| e.harness_type("npm run dev\r")),
        UntilFor(|e| e.harness_url().is_some(), "Vite address", 120),
        Act(|e, _, _| {
            let url = e.harness_url().unwrap();
            e.open_preview(&url);
        }),
        UntilFor(
            |e| e.harness_preview_url().is_some(),
            "WebView2 preview",
            60,
        ),
        Wait(3.0),
        Act(|_, c, _| {
            let out = std::env::var_os("COVALOS_EVIDENCE_DIR")
                .map(PathBuf::from)
                .unwrap();
            os_capture(c, &out.join("promo").join("04-terminal-preview.png"));
        }),
        Wait(6.0),
        Shot("promo/04-terminal-preview-gl"),
        Act(|e, _, _| {
            e.preview.open = false;
            e.hub_open = true;
            e.hub.focus_agents("orchestrator");
        }),
        Wait(1.2),
        Shot("promo/05-orchestrator"),
        Act(|e, _, _| {
            e.hub_open = false;
            e.harness_stop_all();
        }),
        UntilFor(|e| !e.terminals.busy(), "processes stopped", 30),
    ]
}

#[test]
#[ignore = "opens a native window, runs the TypeScript server, Vite and one Codex turn"]
fn capture_alpha_tour() -> Result<(), Box<dyn std::error::Error>> {
    use winit::platform::windows::EventLoopBuilderExtWindows;
    let output =
        PathBuf::from(std::env::var_os("COVALOS_EVIDENCE_DIR").ok_or("set COVALOS_EVIDENCE_DIR")?);
    std::fs::create_dir_all(output.join("reports"))?;
    let root =
        PathBuf::from(std::env::var_os("COVALOS_LSP_PROJECT").ok_or("set COVALOS_LSP_PROJECT")?);
    let base = PathBuf::from(std::env::var_os("COVALOS_AGENT_RUN").ok_or("set COVALOS_AGENT_RUN")?);
    // SAFETY: set before any thread of this test reads the environment.
    unsafe { std::env::set_var("COVALOS_CONFIG_DIR", base.join("config")) };
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([1440.0, 860.0])
            .with_position([40.0, 40.0])
            .with_decorations(false)
            .with_title(TOUR_TITLE),
        event_loop_builder: Some(Box::new(|b| {
            b.with_any_thread(true);
        })),
        ..Default::default()
    };
    eframe::run_native(
        "Covalos alpha tour",
        options,
        Box::new(move |cc| {
            let mut preferences = Preferences::default();
            preferences.language = super::i18n::Language::English;
            preferences.assistant_open = false;
            let editor = Editor::new(&cc.egui_ctx, None, preferences);
            Ok(Box::new(Harness {
                editor,
                root,
                steps: tour_steps(),
                stage: 0,
                at: Instant::now(),
                delay: 0.8,
                queued: Vec::new(),
                requested: false,
                output,
                suffix: String::new(),
                scale: 1.0,
                deadline: Instant::now() + Duration::from_secs(900),
                manifest: Vec::new(),
                last_action: Instant::now(),
                render_ms: Vec::new(),
            }))
        }),
    )?;
    Ok(())
}

/// Web workflow on a throwaway Node project: dev server from a terminal tab,
/// its port in Ports, the WebView2 preview, and a confirmed Cloudflare tunnel.
fn web_steps() -> Vec<Step> {
    vec![
        Act(|e, c, _| {
            e.show_tool("terminal");
            e.harness_open_terminal(c);
        }),
        UntilFor(|e| e.harness_prompt_ready(), "PowerShell prompt", 60),
        Wait(0.6),
        Act(|e, _, _| e.harness_type("npm run dev\r")),
        UntilFor(|e| e.harness_url().is_some(), "dev server address", 90),
        Wait(1.0),
        Shot("web/01-terminal-dev-server"),
        Act(|e, _, _| e.show_tool("ports")),
        UntilFor(
            |e| {
                e.terminals
                    .ports
                    .iter()
                    .any(|p| p.port == 5179 && p.owner.is_some())
            },
            "port 5179 owned by a Covalos terminal",
            30,
        ),
        Wait(0.8),
        Shot("web/02-ports"),
        Act(|e, _, _| e.open_preview("http://localhost:5179/")),
        UntilFor(
            |e| e.harness_preview_url().is_some_and(|u| u.contains("5179")),
            "WebView2 preview",
            60,
        ),
        Wait(1.5),
        Act(|e, _, _| eprintln!("preview loaded {:?}", e.harness_preview_url())),
        Shot("web/03-preview-panel"),
        Act(|e, _, _| e.harness_ask_tunnel(5179)),
        Wait(0.8),
        Shot("web/04-tunnel-confirm"),
        Act(|e, c, _| e.harness_confirm_tunnel(c)),
        UntilFor(
            |e| e.harness_tunnel_url().is_some(),
            "trycloudflare address",
            90,
        ),
        Wait(0.8),
        Act(|e, _, _| eprintln!("tunnel {:?}", e.harness_tunnel_url())),
        Shot("web/05-tunnel-live"),
        Act(|e, _, _| e.harness_stop_all()),
        UntilFor(|e| !e.terminals.busy(), "processes stopped", 30),
        Wait(0.5),
        Shot("web/06-stopped"),
    ]
}

#[test]
#[ignore = "opens a native window, runs node and cloudflared"]
fn capture_web_workflow() -> Result<(), Box<dyn std::error::Error>> {
    use winit::platform::windows::EventLoopBuilderExtWindows;
    let output =
        PathBuf::from(std::env::var_os("COVALOS_EVIDENCE_DIR").ok_or("set COVALOS_EVIDENCE_DIR")?);
    std::fs::create_dir_all(output.join("reports"))?;
    let project = tempfile::tempdir()?;
    let root = crate::agents_runtime::plain(&std::fs::canonicalize(project.path())?);
    std::fs::write(
        root.join("package.json"),
        r#"{"name":"covalos-web-check","private":true,"scripts":{"dev":"node server.js"}}"#,
    )?;
    std::fs::write(
        root.join("server.js"),
        "const http = require('http');\nconst port = 5179;\nhttp.createServer((q, r) => {\n  r.writeHead(200, {'content-type': 'text/html'});\n  r.end('<h1>Covalos preview check</h1>');\n}).listen(port, () => console.log(`  Local:   http://localhost:${port}/`));\n",
    )?;
    std::fs::write(root.join("index.html"), "<h1>Covalos</h1>\n")?;
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([1366.0, 768.0])
            .with_decorations(false)
            .with_title("Covalos web workflow"),
        event_loop_builder: Some(Box::new(|b| {
            b.with_any_thread(true);
        })),
        ..Default::default()
    };
    let run_root = root.clone();
    eframe::run_native(
        "Covalos web workflow",
        options,
        Box::new(move |cc| {
            let mut preferences = Preferences::default();
            preferences.language = super::i18n::Language::English;
            preferences.assistant_open = false;
            let mut editor = Editor::new(&cc.egui_ctx, Some(run_root.clone()), preferences);
            editor.open_file(run_root.join("server.js"));
            Ok(Box::new(Harness {
                editor,
                root: run_root,
                steps: web_steps(),
                stage: 0,
                at: Instant::now(),
                delay: 0.8,
                queued: Vec::new(),
                requested: false,
                output,
                suffix: String::new(),
                scale: 1.0,
                deadline: Instant::now() + Duration::from_secs(600),
                manifest: Vec::new(),
                last_action: Instant::now(),
                render_ms: Vec::new(),
            }))
        }),
    )?;
    Ok(())
}

/// Re-renders a finished real run (no provider turns) to review layout at
/// several window sizes and zoom levels.
#[test]
#[ignore = "opens a native window over a recorded Agent Hub run"]
fn capture_agent_hub_replay() -> Result<(), Box<dyn std::error::Error>> {
    use winit::platform::windows::EventLoopBuilderExtWindows;
    let output =
        PathBuf::from(std::env::var_os("COVALOS_EVIDENCE_DIR").ok_or("set COVALOS_EVIDENCE_DIR")?);
    let base = PathBuf::from(std::env::var_os("COVALOS_AGENT_RUN").ok_or("set COVALOS_AGENT_RUN")?);
    let env = |k: &str, d: f32| {
        std::env::var(k)
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(d)
    };
    let (width, height, scale) = (
        env("COVALOS_EVIDENCE_WIDTH", 1366.0),
        env("COVALOS_EVIDENCE_HEIGHT", 768.0),
        env("COVALOS_EVIDENCE_SCALE", 1.0),
    );
    // SAFETY: set before any thread of this test reads the environment.
    unsafe { std::env::set_var("COVALOS_CONFIG_DIR", base.join("config")) };
    let root = base.join("agent-sandbox");
    let suffix = format!(
        "-{}x{}@{}",
        width as u32,
        height as u32,
        (scale * 100.0).round() as u32
    );
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([width, height])
            .with_decorations(false)
            .with_title("Covalos Agent Hub replay"),
        event_loop_builder: Some(Box::new(|b| {
            b.with_any_thread(true);
        })),
        ..Default::default()
    };
    eframe::run_native(
        "Covalos Agent Hub replay",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_pixels_per_point(scale);
            let mut preferences = Preferences::default();
            preferences.language = super::i18n::Language::English;
            let editor = Editor::new(&cc.egui_ctx, Some(root.clone()), preferences);
            Ok(Box::new(Harness {
                editor,
                root,
                steps: if std::env::var_os("COVALOS_AGENT_ENGINE").is_some() {
                    engine_steps()
                } else {
                    replay_steps()
                },
                stage: 0,
                at: Instant::now(),
                delay: 0.8,
                queued: Vec::new(),
                requested: false,
                output,
                suffix,
                scale,
                deadline: Instant::now() + Duration::from_secs(900),
                manifest: Vec::new(),
                last_action: Instant::now(),
                render_ms: Vec::new(),
            }))
        }),
    )?;
    Ok(())
}

fn welcome_steps() -> Vec<Step> {
    vec![
        Wait(1.0),
        Shot("welcome/01-no-project"),
        Click("Home Agents"),
        Wait(0.8),
        Shot("welcome/02-agents-from-welcome"),
        Act(|e, _, _| {
            e.hub_open = false;
            e.sidebar = Sidebar::Files;
            e.sidebar_requested = true;
        }),
        Wait(0.6),
        Shot("welcome/03-explorer-on-demand"),
        Act(|e, _, root| {
            e.sidebar_requested = false;
            e.open_workspace(root.to_path_buf());
        }),
        Wait(1.0),
        Shot("welcome/04-project-open-explorer-restored"),
    ]
}

/// Welcome screen without a project, quick actions and Explorer restore.
#[test]
#[ignore = "opens a native window and writes welcome evidence"]
fn capture_welcome_review() -> Result<(), Box<dyn std::error::Error>> {
    use winit::platform::windows::EventLoopBuilderExtWindows;
    let output =
        PathBuf::from(std::env::var_os("COVALOS_EVIDENCE_DIR").ok_or("set COVALOS_EVIDENCE_DIR")?);
    std::fs::create_dir_all(output.join("reports"))?;
    let fixture = tempfile::tempdir()?;
    let config = fixture.path().join("config");
    std::fs::create_dir_all(&config)?;
    // SAFETY: set before any thread of this test reads the environment.
    unsafe { std::env::set_var("COVALOS_CONFIG_DIR", &config) };
    let root = fixture.path().join("orbit-ledger");
    write_fixture(&root)?;
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([1366.0, 768.0])
            .with_decorations(false)
            .with_title("Covalos welcome evidence"),
        event_loop_builder: Some(Box::new(|b| {
            b.with_any_thread(true);
        })),
        ..Default::default()
    };
    eframe::run_native(
        "Covalos welcome evidence",
        options,
        Box::new(move |cc| {
            let mut preferences = Preferences::default();
            preferences.language = super::i18n::Language::English;
            let editor = Editor::new(&cc.egui_ctx, None, preferences);
            Ok(Box::new(Harness {
                editor,
                root,
                steps: welcome_steps(),
                stage: 0,
                at: Instant::now(),
                delay: 0.8,
                queued: Vec::new(),
                requested: false,
                output,
                suffix: "-1366x768@100".into(),
                scale: 1.0,
                deadline: Instant::now() + Duration::from_secs(120),
                manifest: Vec::new(),
                last_action: Instant::now(),
                render_ms: Vec::new(),
            }))
        }),
    )?;
    drop(fixture);
    Ok(())
}

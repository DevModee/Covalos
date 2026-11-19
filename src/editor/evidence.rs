//! Visual evidence harness. Drives the real AEROS window through every screen named in
//! the review brief and writes PNG captures plus a manifest into
//! `artifacts/visual-review/`. Input is injected through eframe's raw input hook into
//! a native window; screenshots come from the OpenGL framebuffer.
//!
//! Run (PowerShell):
//!   $env:AEROS_EVIDENCE_DIR = "artifacts/visual-review"
//!   cargo test --release capture_visual_review -- --ignored
//! Optional: AEROS_EVIDENCE_WIDTH / _HEIGHT / _SCALE, AEROS_EVIDENCE_SET=matrix (subset),
//! AEROS_TEST_EXE (path to aeros-editor.exe, enables the approval fixture).
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

fn background(editor: &mut Editor, b: identity::Background, p: identity::Pattern) {
    home(editor);
    // Hide the Codex panel so the whole field is visible.
    editor.preferences.assistant_open = false;
    editor.preferences.identity.background = b;
    editor.preferences.identity.pattern = p;
}

fn steps(matrix: bool) -> Vec<Step> {
    use identity::{Background as B, Mood as M, Pattern as P};
    let mut s = vec![
        Act(|e, c, _| {
            theme(e, c, theme::Theme::Dark);
            e.preferences.language = super::i18n::Language::English;
        }),
        Move(0.62, 0.40),
        Wait(0.6),
        Shot("01-home/home-dark-orbital"),
        Move(0.32, 0.43),
        Wait(0.4),
        Shot("01-home/home-card-hover"),
        Click("Folder src"),
        Click("Folder web"),
        Click("Folder tools"),
        Wait(0.4),
        Shot("02-explorer/explorer-expanded"),
        Click("File main.rs"),
        Act(|e, _, _| caret_after(e, "fn summarize(")),
        Wait(0.5),
        Shot("03-editor/rust/editor-rust-dark"),
    ];
    if matrix {
        s.extend([
            Act(|e, _, _| {
                e.hub_open = true;
                e.review_fixture("conversation");
            }),
            Wait(0.5),
            Shot("04-agent-hub/agent-hub-dark"),
            Act(|e, _, _| {
                e.hub_open = false;
                e.preferences.language = super::i18n::Language::Spanish;
                e.settings_open = true;
            }),
            Wait(0.5),
            Shot("09-settings-es/settings-es"),
        ]);
        return s;
    }
    s.extend([
        Click("File app.ts"),
        Act(|e, _, _| caret_after(e, "async function loadOrders(")),
        Wait(0.5),
        Shot("03-editor/typescript/editor-typescript-dark"),
        Click("File analyze.py"),
        Act(|e, _, _| caret_after(e, "def summarize(")),
        Wait(0.5),
        Shot("03-editor/python/editor-python-dark"),
        Click("File Cargo.toml"),
        Wait(0.4),
        Shot("03-editor/rust/editor-toml-dark"),
        Click("Tab main.rs"),
        Wait(0.03),
        Shot("10-motion/tab-switch-1-early"),
        Wait(0.08),
        Shot("10-motion/tab-switch-2-mid"),
        Wait(0.4),
        Shot("10-motion/tab-switch-3-settled"),
        Key(Key::P, Modifiers::COMMAND),
        Wait(0.02),
        Shot("10-motion/palette-1-opening"),
        Wait(0.35),
        Shot("03-editor/command-palette"),
        Key(Key::Escape, Modifiers::NONE),
        Act(|e, _, _| {
            e.hub_open = true;
            e.review_fixture("conversation");
        }),
        Wait(0.5),
        Shot("04-agent-hub/agent-hub-conversation-dark"),
        Act(|e, _, _| e.review_fixture("tasks")),
        Wait(0.4),
        Shot("04-agent-hub/agent-hub-tasks-dark"),
        Act(|e, _, _| {
            e.hub_open = false;
            e.chat.messages.clear();
        }),
        Wait(0.4),
        Shot("04-agent-hub/codex-panel-empty-dark"),
        Act(|e, c, root| {
            if let Some(exe) = std::env::var_os("AEROS_TEST_EXE") {
                use std::io::Write;
                e.companion.start(c);
                let mut child = std::process::Command::new(exe)
                    .arg("--agent-hook")
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::null())
                    .spawn()
                    .expect("spawn test hook");
                let payload = serde_json::json!({"session_id":"evidence-fixture","hook_event_name":"PermissionRequest","cwd":root,"tool_name":"Edit","tool_input":{"file_path":"src/main.rs","old_string":"FIXTURE","new_string":"FIXTURE — no tool is executed"}});
                child
                    .stdin
                    .take()
                    .unwrap()
                    .write_all(payload.to_string().as_bytes())
                    .unwrap();
                HOOK.with(|h| *h.borrow_mut() = Some(child));
            }
        }),
        Until(
            |e| {
                std::env::var_os("AEROS_TEST_EXE").is_none()
                    || e.companion.sessions.iter().any(|s| {
                        s.state == crate::agent_bridge::State::Waiting
                    })
            },
            "approval fixture",
        ),
        Wait(0.5),
        Shot("05-companion/companion-compact-approval"),
        Act(|e, _, _| e.companion.open = true),
        Wait(0.03),
        Shot("10-motion/companion-open-1-early"),
        Wait(0.06),
        Shot("10-motion/companion-open-2-mid"),
        Wait(0.5),
        Shot("05-companion/companion-expanded-approval"),
        Key(Key::Escape, Modifiers::NONE),
        Wait(0.05),
        Shot("10-motion/companion-close-1-early"),
        Wait(0.5),
        Act(|e, _, _| {
            HOOK.with(|h| {
                if let Some(mut child) = h.borrow_mut().take() {
                    let _ = child.kill();
                }
            });
            e.companion.clear_external();
        }),
        Wait(0.6),
    ]);
    let moods: [(Act, &'static str); 8] = [
        (
            |e, _, _| e.mood_override = Some(M::Idle),
            "06-mascot/luma-idle",
        ),
        (
            |e, _, _| e.mood_override = Some(M::Thinking),
            "06-mascot/luma-thinking",
        ),
        (
            |e, _, _| e.mood_override = Some(M::Coding),
            "06-mascot/luma-coding",
        ),
        (
            |e, _, _| e.mood_override = Some(M::Searching),
            "06-mascot/luma-searching",
        ),
        (
            |e, _, _| e.mood_override = Some(M::Approval),
            "06-mascot/luma-waiting-approval",
        ),
        (
            |e, _, _| e.mood_override = Some(M::Success),
            "06-mascot/luma-success",
        ),
        (
            |e, _, _| e.mood_override = Some(M::Error),
            "06-mascot/luma-error",
        ),
        (
            |e, _, _| e.mood_override = Some(M::Sleeping),
            "06-mascot/luma-sleeping",
        ),
    ];
    for (act, name) in moods {
        s.extend([Act(act), Wait(0.3), Crop(name, "Luma", 5.0)]);
    }
    s.extend([
        Act(|e, _, _| e.mood_override = None),
        Move(0.06, 0.88),
        Wait(0.3),
        Crop("06-mascot/luma-hover", "Luma", 5.0),
        Click("Luma"),
        Wait(0.12),
        Crop("06-mascot/luma-click-bounce", "Luma", 5.0),
        Act(|e, _, _| e.companion.open = false),
        Wait(0.4),
    ]);
    let themes: [(Act, &'static str); 5] = [
        (
            |e, c, _| theme(e, c, theme::Theme::Dark),
            "07-themes/editor-aeros-dark",
        ),
        (
            |e, c, _| theme(e, c, theme::Theme::Midnight),
            "07-themes/editor-aeros-midnight",
        ),
        (
            |e, c, _| theme(e, c, theme::Theme::Violet),
            "07-themes/editor-aeros-violet",
        ),
        (
            |e, c, _| theme(e, c, theme::Theme::Light),
            "07-themes/editor-aeros-light",
        ),
        (
            |e, c, _| theme(e, c, theme::Theme::HighContrast),
            "07-themes/editor-high-contrast",
        ),
    ];
    for (act, name) in themes {
        s.extend([Act(act), Wait(0.4), Shot(name)]);
    }
    s.extend([
        Act(|e, _, _| home(e)),
        Wait(0.4),
        Shot("07-themes/home-high-contrast"),
        Act(|e, c, _| theme(e, c, theme::Theme::Light)),
        Wait(0.4),
        Shot("07-themes/home-aeros-light"),
        Act(|e, c, _| theme(e, c, theme::Theme::Dark)),
    ]);
    let scenes: [(Act, &'static str); 10] = [
        (
            |e, _, _| background(e, B::Orbital, P::Waves),
            "08-ascii-backgrounds/orbital",
        ),
        (
            |e, _, _| background(e, B::Neural, P::Waves),
            "08-ascii-backgrounds/neural",
        ),
        (
            |e, _, _| background(e, B::Void, P::Waves),
            "08-ascii-backgrounds/void",
        ),
        (
            |e, _, _| background(e, B::Minimal, P::Waves),
            "08-ascii-backgrounds/minimal",
        ),
        (
            |e, _, _| background(e, B::Ascii, P::Waves),
            "08-ascii-backgrounds/ascii-nebula",
        ),
        (
            |e, _, _| background(e, B::Ascii, P::Geometry),
            "08-ascii-backgrounds/ascii-prism",
        ),
        (
            |e, _, _| background(e, B::Ascii, P::Particles),
            "08-ascii-backgrounds/ascii-drift",
        ),
        (
            |e, _, _| background(e, B::Ascii, P::Orbits),
            "08-ascii-backgrounds/ascii-orbit",
        ),
        (
            |e, _, _| background(e, B::Ascii, P::Network),
            "08-ascii-backgrounds/ascii-contour",
        ),
        (
            |e, _, _| background(e, B::Ascii, P::Sequence),
            "08-ascii-backgrounds/ascii-signal",
        ),
    ];
    for (act, name) in scenes {
        s.extend([Act(act), Move(0.84, 0.30), Wait(0.7), Shot(name)]);
    }
    // Pointer and click targets stay in empty field space, away from Home controls.
    s.extend([
        Act(|e, _, _| background(e, B::Ascii, P::Geometry)),
        Move(0.30, 0.95),
        Wait(0.5),
        Shot("10-motion/ascii-prism-rest"),
        Move(0.86, 0.42),
        Wait(0.08),
        Shot("10-motion/ascii-prism-lens-entering"),
        Wait(0.5),
        Shot("10-motion/ascii-prism-lens"),
        Tap(0.86, 0.42),
        Wait(0.12),
        Shot("10-motion/ascii-prism-click-wave-1"),
        Wait(0.25),
        Shot("10-motion/ascii-prism-click-wave-2"),
        Act(|e, _, _| background(e, B::Ascii, P::Waves)),
        Act(|e, c, _| theme(e, c, theme::Theme::Light)),
        Move(0.84, 0.30),
        Wait(0.6),
        Shot("08-ascii-backgrounds/ascii-nebula-light"),
        Act(|e, c, _| theme(e, c, theme::Theme::Dark)),
        Act(|e, _, _| {
            background(e, B::Orbital, P::Waves);
            e.preferences.assistant_open = true;
        }),
        Act(|e, _, _| {
            e.preferences.language = super::i18n::Language::Spanish;
            e.settings_open = true;
        }),
        Wait(0.5),
        Shot("09-settings-es/settings-es"),
        Act(|e, _, _| e.settings_open = false),
        Wait(0.3),
        Shot("09-settings-es/home-es"),
        Click("File main.rs"),
        Wait(0.4),
        Shot("09-settings-es/editor-es"),
        Act(|e, _, _| {
            e.hub_open = true;
            e.review_fixture("conversation");
        }),
        Wait(0.4),
        Shot("09-settings-es/agent-hub-es"),
        Act(|e, _, _| {
            e.hub_open = false;
            e.companion.open = true;
        }),
        Wait(0.5),
        Shot("09-settings-es/companion-es"),
        Act(|e, _, _| {
            e.companion.open = false;
            e.preferences.language = super::i18n::Language::English;
        }),
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
    let output =
        PathBuf::from(std::env::var_os("AEROS_EVIDENCE_DIR").ok_or("set AEROS_EVIDENCE_DIR")?);
    std::fs::create_dir_all(output.join("reports"))?;
    let env = |k: &str, d: f32| {
        std::env::var(k)
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(d)
    };
    let (width, height, scale) = (
        env("AEROS_EVIDENCE_WIDTH", 1366.0),
        env("AEROS_EVIDENCE_HEIGHT", 768.0),
        env("AEROS_EVIDENCE_SCALE", 1.0),
    );
    let matrix = std::env::var("AEROS_EVIDENCE_SET").as_deref() == Ok("matrix");
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
    unsafe { std::env::set_var("AEROS_CONFIG_DIR", &config) };
    let root = fixture.path().join("orbit-ledger");
    write_fixture(&root)?;
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([width, height])
            .with_decorations(false)
            .with_title("AEROS visual evidence"),
        event_loop_builder: Some(Box::new(|b| {
            b.with_any_thread(true);
        })),
        ..Default::default()
    };
    eframe::run_native(
        "AEROS visual evidence",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_pixels_per_point(scale);
            let mut preferences = Preferences::default();
            preferences.language = super::i18n::Language::English;
            preferences.recent = vec![root.with_file_name("aeros-site")];
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
                deadline: Instant::now() + Duration::from_secs(240),
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
            "# Orbit Ledger\n\nA small ledger used to review AEROS.\n",
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

use super::*;
use std::time::{Duration, Instant};

#[test]
#[ignore = "opens native windows to capture the real application at the requested pixel size and scale"]
#[cfg(windows)]
fn capture_design_review() -> Result<(), Box<dyn std::error::Error>> {
    use eframe::egui::{Event, UserData, ViewportBuilder, ViewportCommand};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    const SCREENS: &[&str] = &[
        "home",
        "editor",
        "conversation",
        "quick-access",
        "agent-hub",
        "tasks",
        "memory",
        "skills",
        "settings",
        "file-dialog",
        "terminal",
        "source-control",
        "companion",
        "motion-start",
        "motion-end",
    ];

    struct Review {
        editor: Editor,
        root: PathBuf,
        output: PathBuf,
        scale: f32,
        stage: usize,
        settle: usize,
        requested: bool,
        hook: Option<std::process::Child>,
        deadline: Instant,
    }
    impl Drop for Review {
        fn drop(&mut self) {
            if let Some(mut child) = self.hook.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
    impl Review {
        fn enter(&mut self, ctx: &egui::Context) {
            self.editor.palette = None;
            self.editor.settings_open = false;
            self.editor.new_file = None;
            self.editor.hub_open = false;
            self.editor.preferences.tools_open = false;
            self.editor.sidebar = Sidebar::Files;
            match self.stage {
                0 => {}
                1 => {
                    self.editor.open_file(self.root.join("src/main.rs"));
                    self.editor.open_file(self.root.join("Cargo.toml"));
                    self.editor.activate(0);
                    self.editor.documents[1].text.push('\n');
                    self.editor.documents[1].update_dirty();
                }
                2 => self.editor.review_fixture("conversation"),
                3 => self.editor.open_palette(ctx),
                4..=7 => {
                    self.editor.hub_open = true;
                    self.editor.review_fixture(SCREENS[self.stage]);
                }
                8 => self.editor.settings_open = true,
                9 => self.editor.new_file = Some("src/window.rs".into()),
                10 => self.editor.preferences.tools_open = true,
                11 => {
                    self.editor.sidebar = Sidebar::Git;
                    self.editor.git_refresh(ctx, None);
                }
                12 => {
                    self.editor.companion.open = true;
                    if let Some(exe) = std::env::var_os("AEROS_TEST_EXE") {
                        use std::io::Write;
                        self.editor.companion.start(ctx);
                        let mut child = std::process::Command::new(exe)
                            .arg("--agent-hook")
                            .stdin(std::process::Stdio::piped())
                            .stdout(std::process::Stdio::null())
                            .spawn()
                            .expect("spawn test hook");
                        let payload = serde_json::json!({"session_id":"native-contract-test","hook_event_name":"PermissionRequest","cwd":self.root,"tool_name":"Write","tool_input":{"file_path":"fixture-only.rs","content":"INTEGRATION TEST — no tool is executed"}});
                        child
                            .stdin
                            .take()
                            .unwrap()
                            .write_all(payload.to_string().as_bytes())
                            .unwrap();
                        self.hook = Some(child);
                    }
                }
                13 | 14 => {
                    self.editor.companion.open = false;
                    self.editor.documents.clear();
                    self.editor.active = None;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                _ => {}
            }
            self.settle = 0;
            self.requested = false;
        }
    }
    impl eframe::App for Review {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            assert!(
                Instant::now() < self.deadline,
                "native screenshot timed out"
            );
            if self.stage == 0 && self.settle == 0 {
                ctx.set_pixels_per_point(self.scale);
            }
            let screenshot = ctx.input(|input| {
                input.events.iter().find_map(|event| {
                    if let Event::Screenshot { image, .. } = event {
                        Some(image.clone())
                    } else {
                        None
                    }
                })
            });
            if let Some(image) = screenshot {
                let bytes = image
                    .pixels
                    .iter()
                    .flat_map(|pixel| pixel.to_array())
                    .collect::<Vec<_>>();
                image::save_buffer(
                    self.output.join(format!("{}.png", SCREENS[self.stage])),
                    &bytes,
                    image.size[0] as u32,
                    image.size[1] as u32,
                    image::ColorType::Rgba8,
                )
                .expect("write native review frame");
                if self.stage == 12 && self.hook.is_some() {
                    if let Some(dir) = crate::agent_bridge::directory() {
                        for session in crate::agent_bridge::read_sessions(&dir) {
                            if session.id == "native-contract-test" && session.approvable {
                                crate::agent_bridge::decide(&dir, &session, false)
                                    .expect("deny test hook");
                            }
                        }
                    }
                    if let Some(mut child) = self.hook.take() {
                        let until = Instant::now() + Duration::from_secs(2);
                        while child.try_wait().unwrap().is_none() && Instant::now() < until {
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        if child.try_wait().unwrap().is_none() {
                            let _ = child.kill();
                        }
                        let _ = child.wait();
                    }
                }
                self.stage += 1;
                if self.stage == SCREENS.len() {
                    self.editor.force_close = true;
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                    return;
                }
                self.enter(ctx);
            }
            self.editor.render(ctx);
            self.settle += 1;
            let settle_frames = if self.stage == 12 || self.stage == 14 {
                65
            } else {
                5
            };
            if self.settle > settle_frames && !self.requested {
                ctx.send_viewport_cmd(ViewportCommand::Screenshot(UserData::default()));
                self.requested = true;
            }
            ctx.request_repaint_after(Duration::from_millis(40));
        }
    }
    let output = std::env::var_os("AEROS_REVIEW_DIR")
        .map(PathBuf::from)
        .ok_or("set AEROS_REVIEW_DIR")?;
    std::fs::create_dir_all(&output)?;
    let width = std::env::var("AEROS_REVIEW_WIDTH")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(1366.0);
    let height = std::env::var("AEROS_REVIEW_HEIGHT")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(768.0);
    let scale = std::env::var("AEROS_REVIEW_SCALE")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(1.0);
    let fixture = tempfile::tempdir()?;
    let root = fixture.path().join("aeros-workspace");
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(root.join("src/main.rs"), SAMPLE)?;
    std::fs::write(root.join("src/workspace.rs"), "// Workspace operations\n")?;
    std::fs::write(root.join("README.md"), "# Aeros workspace\n")?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"aeros-workspace\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([width, height])
            .with_decorations(false)
            .with_title("AEROS visual review"),
        event_loop_builder: Some(Box::new(|builder| {
            builder.with_any_thread(true);
        })),
        ..Default::default()
    };
    eframe::run_native(
        "AEROS visual review",
        options,
        Box::new(move |cc| {
            let requested_theme =
                std::env::var("AEROS_REVIEW_THEME").unwrap_or_else(|_| "dark".into());
            let mut preferences = Preferences::default();
            preferences.theme =
                serde_json::from_str(&format!("\"{requested_theme}\"")).unwrap_or_default();
            if let Ok(background) = std::env::var("AEROS_REVIEW_BACKGROUND") {
                preferences.identity.background =
                    serde_json::from_str(&format!("\"{background}\"")).unwrap_or_default();
            }
            preferences.reduce_motion = std::env::var("AEROS_REVIEW_REDUCE").as_deref() == Ok("1");
            preferences.compact = std::env::var("AEROS_REVIEW_COMPACT").as_deref() == Ok("1");
            let mut editor = Editor::new(&cc.egui_ctx, Some(root.clone()), preferences);
            if std::env::var_os("AEROS_TEST_EXE").is_some() {
                editor.companion.start(&cc.egui_ctx);
            }
            editor
                .preferences
                .recent
                .push(root.parent().unwrap().join("aeros-series"));
            Ok(Box::new(Review {
                editor,
                root,
                output,
                scale,
                stage: 0,
                settle: 0,
                requested: false,
                hook: None,
                deadline: Instant::now() + Duration::from_secs(90),
            }))
        }),
    )?;
    Ok(())
}
const SAMPLE: &str = r#"use std::path::PathBuf;
use eframe::egui::{self, Context, ViewportBuilder};

/// Open the workspace in a native window.
fn main() -> eframe::Result {
    let root = std::env::args_os()
        .nth(1)
        .map(PathBuf::from);

    let viewport = ViewportBuilder::default()
        .with_inner_size([1366.0, 768.0])
        .with_min_inner_size([920.0, 600.0]);

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "AEROS Editor",
        options,
        Box::new(move |cc| {
            let editor = Editor::new(&cc.egui_ctx, root);
            Ok(Box::new(editor))
        }),
    )
}

fn workspace_label(root: Option<&PathBuf>) -> String {
    root.map(|path| path.display().to_string())
        .unwrap_or_else(|| "Open a workspace".into())
}
"#;

#[test]
#[ignore = "opens a native window to inspect a persisted recovery fixture; no provider is called"]
#[cfg(windows)]
fn capture_recovered_conversation() -> Result<(), Box<dyn std::error::Error>> {
    use eframe::egui::{Event, UserData, ViewportBuilder, ViewportCommand};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    struct Recovery {
        editor: Editor,
        output: PathBuf,
        frames: usize,
        requested: bool,
        deadline: Instant,
    }
    impl eframe::App for Recovery {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            assert!(
                Instant::now() < self.deadline,
                "recovery screenshot timed out"
            );
            let image = ctx.input(|i| {
                i.events.iter().find_map(|event| match event {
                    Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(image) = image {
                let bytes: Vec<_> = image
                    .pixels
                    .iter()
                    .flat_map(|pixel| pixel.to_array())
                    .collect();
                image::save_buffer(
                    &self.output,
                    &bytes,
                    image.size[0] as u32,
                    image.size[1] as u32,
                    image::ColorType::Rgba8,
                )
                .expect("save recovery screenshot");
                self.editor.force_close = true;
                ctx.send_viewport_cmd(ViewportCommand::Close);
                return;
            }
            self.editor.render(ctx);
            self.frames += 1;
            if self.frames > 6 && !self.requested {
                self.requested = true;
                ctx.send_viewport_cmd(ViewportCommand::Screenshot(UserData::default()));
            }
            ctx.request_repaint_after(Duration::from_millis(60));
        }
    }
    let output = PathBuf::from(
        std::env::var_os("AEROS_RECOVERY_CAPTURE").ok_or("set AEROS_RECOVERY_CAPTURE")?,
    );
    std::fs::create_dir_all(output.parent().unwrap())?;
    let fixture = tempfile::tempdir()?;
    let root = fixture.path().join("recovery-workspace");
    std::fs::create_dir_all(&root)?;
    let root = root.canonicalize()?;
    let config = fixture.path().join("profile/editor.json");
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([1366.0, 768.0])
            .with_decorations(false)
            .with_title("AEROS recovery verification"),
        event_loop_builder: Some(Box::new(|b| {
            b.with_any_thread(true);
        })),
        ..Default::default()
    };
    eframe::run_native(
        "AEROS recovery verification",
        options,
        Box::new(move |cc| {
            let mut original =
                Editor::new(&cc.egui_ctx, Some(root.clone()), Preferences::default());
            original.preferences_path = Some(config.clone());
            original.chat.push(
                "You",
                "Keep my workspace conversation after restarting the editor.".into(),
            );
            original.chat.push(
                "System",
                "Recovery verification fixture. No AI provider was called.".into(),
            );
            original.chat.prompt = "Continue from this conversation".into();
            assert!(original.save_conversation());
            drop(original);
            let mut saved = sessions::Conversation::load(&config, &root)
                .unwrap()
                .unwrap();
            saved.interrupted = true;
            saved.save(&config).unwrap();
            let mut editor = Editor::new(&cc.egui_ctx, Some(root), Preferences::default());
            editor.preferences_path = Some(config);
            editor.restore_conversation();
            assert_eq!(editor.chat.messages.len(), 3);
            assert!(editor.chat.run.is_none());
            assert!(!editor.hub.approval_pending());
            assert!(editor.hub.workflow.is_none());
            editor.hub_open = true;
            Ok(Box::new(Recovery {
                editor,
                output,
                frames: 0,
                requested: false,
                deadline: Instant::now() + Duration::from_secs(30),
            }))
        }),
    )?;
    Ok(())
}

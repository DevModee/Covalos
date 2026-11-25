use super::*;
use std::time::{Duration, Instant};

#[test]
#[ignore = "opens native windows to capture the real application at the requested pixel size and scale"]
#[cfg(windows)]
fn capture_design_review() -> Result<(), Box<dyn std::error::Error>> {
    use eframe::egui::{
        Event, Modifiers, PointerButton, UserData, ViewportBuilder, ViewportCommand,
    };
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
        "beta-providers",
        "beta-model-picker",
        "beta-chat-home",
        "beta-chat-docked",
        "gemini-migration",
        "antigravity-setup",
        "usage-limits",
        "document-context-menu",
        "explorer-create-folder",
        "explorer-create-file",
        "terminal-at-folder",
        "home-context-menu",
        "explorer-name-collision",
        "explorer-cancel",
        "chat-waiting",
    ];

    struct Review {
        editor: Editor,
        root: PathBuf,
        output: PathBuf,
        scale: f32,
        entry_width: f32,
        stage: usize,
        settle: usize,
        requested: bool,
        hook: Option<std::process::Child>,
        approval_seen: Option<Instant>,
        deadline: Instant,
        queued: Vec<Event>,
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
            if self.stage != 28 {
                self.editor.entry_draft = None;
            }
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
                9 => {
                    self.entry_width = self.editor.preferences.sidebar_width;
                    self.editor.begin_entry(
                        files::EntryKind::File,
                        self.root.join("src").canonicalize().unwrap(),
                        None,
                    );
                    self.editor.entry_draft.as_mut().unwrap().name = "window.rs".into();
                }
                10 => self.editor.preferences.tools_open = true,
                11 => {
                    self.editor.sidebar = Sidebar::Git;
                    self.editor.git_refresh(ctx, None);
                }
                12 => {
                    self.editor.companion.open = true;
                    if let Some(exe) = std::env::var_os("COVALOS_TEST_EXE")
                        .or_else(|| std::env::var_os("AEROS_TEST_EXE"))
                    {
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
                15 => {
                    assert!(self.editor.ensure_runtime(ctx));
                    self.editor
                        .hub
                        .agents
                        .runtime
                        .as_mut()
                        .unwrap()
                        .seed_review_accounts();
                    self.editor.open_providers();
                }
                16 => {
                    self.editor.documents.clear();
                    self.editor.active = None;
                    self.editor.hub.agents.chat.dock = None;
                    self.editor.restore_chat_layout();
                }
                17 => {
                    egui::Popup::close_all(ctx);
                    let runtime = self.editor.hub.agents.runtime.as_mut().unwrap();
                    let account = runtime.seed_review_accounts();
                    let id = runtime.seed_review_conversation(self.root.clone(), account);
                    self.editor.preferences.assistant_open = true;
                    self.editor.hub.agents.chat.dock = Some(id);
                    assert!(self.editor.in_session_home());
                }
                18 => {
                    let id = self.editor.hub.agents.chat.dock.clone();
                    self.editor.open_file(self.root.join("src/main.rs"));
                    assert!(!self.editor.in_session_home());
                    assert_eq!(self.editor.hub.agents.chat.dock, id);
                }
                19 => self.editor.review_gemini_migration(),
                20 => self.editor.open_providers(),
                21 => {
                    let rt = self.editor.hub.agents.runtime.as_mut().unwrap();
                    let account = rt.seed_review_accounts();
                    rt.limits.insert(
                        account,
                        Ok(vec![
                            crate::agent_provider::LimitWindow {
                                label: "Session".into(),
                                left: 100.0,
                                resets_at: None,
                            },
                            crate::agent_provider::LimitWindow {
                                label: "Weekly".into(),
                                left: 22.0,
                                resets_at: Some(
                                    time::OffsetDateTime::now_utc().unix_timestamp() + 5460,
                                ),
                            },
                        ]),
                    );
                    ctx.data_mut(|d| d.insert_temp(egui::Id::new("agent-usage-open"), true));
                }
                22 => {
                    ctx.data_mut(|data| data.insert_temp(egui::Id::new("agent-usage-open"), false));
                    self.editor.preferences.assistant_open = false;
                    self.editor.sidebar_open = true;
                    self.editor.open_file(self.root.join("src/main.rs"));
                }
                26 => {
                    self.editor.documents.clear();
                    self.editor.active = None;
                    self.editor.hub.agents.chat.dock = None;
                }
                27 => egui::Popup::close_all(ctx),
                29 => {
                    self.editor.sidebar = Sidebar::Chats;
                    self.editor.sidebar_open = true;
                    self.editor.documents.clear();
                    self.editor.active = None;
                    let runtime = self.editor.hub.agents.runtime.as_mut().unwrap();
                    let account = runtime.seed_review_accounts();
                    let id = runtime.seed_review_conversation(self.root.clone(), account);
                    runtime.review_waiting(&id);
                    self.editor.hub.agents.chat.dock = Some(id);
                    self.editor.preferences.assistant_open = true;
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
            self.approval_seen = None;
        }
    }
    impl eframe::App for Review {
        fn raw_input_hook(&mut self, _: &egui::Context, input: &mut egui::RawInput) {
            input.focused = true;
            input.events.append(&mut self.queued);
        }
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
                    if let Event::Screenshot {
                        image, user_data, ..
                    } = event
                        && user_data.data.is_none()
                    {
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
            if matches!((self.stage, self.settle), (23, 12) | (24, 17)) {
                assert!(
                    self.editor.entry_draft.is_some(),
                    "expected inline naming field at stage {}",
                    self.stage
                );
                self.queued.push(Event::Text(
                    if self.stage == 23 { "views" } else { "lib.rs" }.into(),
                ));
            }
            if matches!((self.stage, self.settle), (23, 17) | (24, 21)) {
                self.queued.push(Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                });
                self.queued.push(Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                });
            }
            if (self.stage, self.settle) == (27, 17) {
                self.queued.push(Event::Text("README.md".into()));
            }
            if matches!((self.stage, self.settle), (27, 22) | (28, 6)) {
                let key = if self.stage == 27 {
                    egui::Key::Enter
                } else {
                    egui::Key::Escape
                };
                for pressed in [true, false] {
                    self.queued.push(Event::Key {
                        key,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers: Modifiers::NONE,
                    });
                }
            }
            if let Some(control) = match (self.stage, self.settle) {
                (26, 6) => Some("Home background"),
                (27, 5) => Some("Explorer background"),
                (27, 11) => Some("New file"),
                (22, 6) => Some("Document text"),
                (23, 5) => Some("New folder"),
                (24, 5) => Some("Folder src"),
                (24, 11) => Some("New file"),
                (25, 5) => Some("Document text"),
                (25, 12) => Some("New terminal here"),
                (16, 6) => Some("Model picker"),
                (20, 6) => Some("Set up Antigravity"),
                _ => None,
            } {
                let pos = ctx
                    .data(|data| {
                        data.get_temp::<std::collections::HashMap<String, egui::Rect>>(
                            egui::Id::new("native-review-targets"),
                        )
                    })
                    .and_then(|targets| targets.get(control).map(egui::Rect::center))
                    .expect("review control");
                self.queued.push(Event::PointerMoved(pos));
                for pressed in [true, false] {
                    self.queued.push(Event::PointerButton {
                        pos,
                        button: if matches!(
                            (self.stage, self.settle),
                            (22, 6) | (24, 5) | (25, 5) | (26, 6) | (27, 5)
                        ) {
                            PointerButton::Secondary
                        } else {
                            PointerButton::Primary
                        },
                        pressed,
                        modifiers: Modifiers::NONE,
                    });
                }
            }
            let settle_frames = if self.stage == 12 || self.stage == 14 {
                65
            } else {
                30
            };
            if self.stage == 12 && self.hook.is_some() && self.editor.companion.counts().1 > 0 {
                self.approval_seen.get_or_insert_with(Instant::now);
            }
            let approval_ready = self.stage != 12
                || self.hook.is_none()
                || self
                    .approval_seen
                    .is_some_and(|at| at.elapsed() > Duration::from_millis(350));
            if self.settle > settle_frames && !self.requested && approval_ready {
                match self.stage {
                    9 => assert!(
                        self.editor.preferences.sidebar_width <= self.entry_width + 1.0,
                        "inline naming resized sidebar from {} to {}",
                        self.entry_width,
                        self.editor.preferences.sidebar_width
                    ),
                    26 => assert!(egui::Popup::is_any_open(ctx), "missing Home context menu"),
                    27 => {
                        assert!(
                            self.editor.entry_draft.is_some(),
                            "duplicate name must keep field open"
                        );
                        assert_eq!(
                            std::fs::read_to_string(self.root.join("README.md")).unwrap(),
                            "# Covalos workspace\n"
                        );
                    }
                    28 => assert!(
                        self.editor.entry_draft.is_none(),
                        "Escape must cancel inline naming"
                    ),
                    22 => assert!(
                        egui::Popup::is_any_open(ctx),
                        "missing document context menu"
                    ),
                    23 => assert!(
                        self.root.join("src/views").is_dir(),
                        "new folder must be relative to active file"
                    ),
                    24 => {
                        let expected = self
                            .root
                            .join("src/lib.rs")
                            .canonicalize()
                            .expect("file created in selected folder");
                        assert_eq!(
                            self.editor.documents[self.editor.active.unwrap()].path,
                            expected
                        );
                    }
                    25 => {
                        assert!(self.editor.preferences.tools_open);
                        assert_eq!(
                            self.editor
                                .terminals
                                .active_directory()
                                .unwrap()
                                .canonicalize()
                                .unwrap(),
                            self.root.join("src").canonicalize().unwrap()
                        );
                    }
                    _ => {}
                }
                if self.stage == 8 {
                    assert!(
                        ctx.data(
                            |data| data.get_temp::<bool>(egui::Id::new("review-settings-blurred"))
                        )
                        .unwrap_or(false),
                        "settings rendered without the blurred snapshot"
                    );
                }
                if self.stage == 20 {
                    let runtime = self.editor.hub.agents.runtime.as_ref().unwrap();
                    assert!(runtime.profiles.accounts().iter().any(|a| {
                        a.provider == covalos_agents::ProviderKind::Antigravity
                            && a.method == covalos_agents::ConnectionMethod::SystemCli
                    }));
                    assert!(runtime.notice.is_none());
                }
                ctx.send_viewport_cmd(ViewportCommand::Screenshot(UserData::default()));
                self.requested = true;
            }
            ctx.request_repaint_after(Duration::from_millis(40));
        }
    }
    let output = std::env::var_os("COVALOS_REVIEW_DIR")
        .or_else(|| std::env::var_os("AEROS_REVIEW_DIR"))
        .map(PathBuf::from)
        .ok_or("set COVALOS_REVIEW_DIR")?;
    std::fs::create_dir_all(&output)?;
    let width = std::env::var("COVALOS_REVIEW_WIDTH")
        .or_else(|_| std::env::var("AEROS_REVIEW_WIDTH"))
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(1366.0);
    let height = std::env::var("COVALOS_REVIEW_HEIGHT")
        .or_else(|_| std::env::var("AEROS_REVIEW_HEIGHT"))
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(768.0);
    let scale = std::env::var("COVALOS_REVIEW_SCALE")
        .or_else(|_| std::env::var("AEROS_REVIEW_SCALE"))
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(1.0);
    let fixture = tempfile::tempdir()?;
    let config = fixture.path().join("config");
    std::fs::create_dir_all(&config)?;
    // SAFETY: set before any thread of this test reads the environment.
    unsafe { std::env::set_var("COVALOS_CONFIG_DIR", &config) };
    let root = fixture.path().join("covalos-workspace");
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(root.join("src/main.rs"), SAMPLE)?;
    std::fs::write(root.join("src/workspace.rs"), "// Workspace operations\n")?;
    std::fs::write(root.join("README.md"), "# Covalos workspace\n")?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"covalos-workspace\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([width, height])
            .with_decorations(false)
            .with_title("Covalos visual review"),
        event_loop_builder: Some(Box::new(|builder| {
            builder.with_any_thread(true);
        })),
        ..Default::default()
    };
    eframe::run_native(
        "Covalos visual review",
        options,
        Box::new(move |cc| {
            let requested_theme = std::env::var("COVALOS_REVIEW_THEME")
                .or_else(|_| std::env::var("AEROS_REVIEW_THEME"))
                .unwrap_or_else(|_| "nocturne".into());
            let mut preferences = Preferences::default();
            preferences.theme =
                serde_json::from_str(&format!("\"{requested_theme}\"")).unwrap_or_default();
            if let Ok(background) = std::env::var("COVALOS_REVIEW_BACKGROUND")
                .or_else(|_| std::env::var("AEROS_REVIEW_BACKGROUND"))
            {
                preferences.identity.background =
                    serde_json::from_str(&format!("\"{background}\"")).unwrap_or_default();
            }
            preferences.reduce_motion = std::env::var("COVALOS_REVIEW_REDUCE")
                .or_else(|_| std::env::var("AEROS_REVIEW_REDUCE"))
                .as_deref()
                == Ok("1");
            preferences.compact = std::env::var("COVALOS_REVIEW_COMPACT")
                .or_else(|_| std::env::var("AEROS_REVIEW_COMPACT"))
                .as_deref()
                == Ok("1");
            let mut editor = Editor::new(&cc.egui_ctx, Some(root.clone()), preferences);
            if std::env::var_os("COVALOS_TEST_EXE")
                .or_else(|| std::env::var_os("AEROS_TEST_EXE"))
                .is_some()
            {
                editor.companion.start(&cc.egui_ctx);
            }
            editor
                .preferences
                .recent
                .push(root.parent().unwrap().join("covalos-series"));
            Ok(Box::new(Review {
                editor,
                root,
                output,
                scale,
                entry_width: 0.0,
                stage: 0,
                settle: 0,
                requested: false,
                hook: None,
                approval_seen: None,
                queued: Vec::new(),
                deadline: Instant::now() + Duration::from_secs(130),
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
        "Covalos",
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
                    Event::Screenshot {
                        image, user_data, ..
                    } if user_data.data.is_none() => Some(image.clone()),
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
        std::env::var_os("COVALOS_RECOVERY_CAPTURE")
            .or_else(|| std::env::var_os("AEROS_RECOVERY_CAPTURE"))
            .ok_or("set COVALOS_RECOVERY_CAPTURE")?,
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
            .with_title("Covalos recovery verification"),
        event_loop_builder: Some(Box::new(|b| {
            b.with_any_thread(true);
        })),
        ..Default::default()
    };
    eframe::run_native(
        "Covalos recovery verification",
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

#[test]
#[ignore = "native pointer/keyboard integration and motion captures; uses only a temporary project"]
#[cfg(windows)]
fn capture_precision_interactions() -> Result<(), Box<dyn std::error::Error>> {
    use eframe::egui::{
        Event, Key, Modifiers, PointerButton, UserData, ViewportBuilder, ViewportCommand,
    };
    use winit::platform::windows::EventLoopBuilderExtWindows;
    enum Action {
        Click(&'static str),
        Key(Key, bool),
        Text(&'static str),
        Move(egui::Pos2),
        Tap(egui::Pos2),
        Capture(&'static str),
        Check(u8),
        DismissFixtureCompanion,
    }
    use Action::*;
    let steps = vec![
        Capture("home-rest"),
        Click("Folder src"),
        Check(1),
        Click("File main.rs"),
        Check(2),
        Text("// native interaction verification\n"),
        Key(Key::S, true),
        Check(3),
        Key(Key::F, true),
        Text("PathBuf"),
        Check(4),
        Capture("find-and-editor"),
        Key(Key::Escape, false),
        Click("File Cargo.toml"),
        Check(5),
        Capture("tab-before"),
        Click("Tab main.rs"),
        Capture("tab-moving"),
        Capture("tab-settled"),
        Key(Key::P, true),
        Capture("palette-open"),
        Key(Key::Escape, false),
        Move(egui::pos2(88.0, 680.0)),
        Capture("luma-hover"),
        Click("Luma"),
        Check(6),
        Capture("companion"),
        DismissFixtureCompanion,
        Click("Settings"),
        Click("Theme Covalos Light"),
        Check(7),
        Capture("light-settings"),
        Click("Settings"),
        Key(Key::W, true),
        Key(Key::W, true),
        Click("Settings"),
        Click("Background selector"),
        Click("Background Neural"),
        Check(8),
        Click("Settings"),
        Move(egui::pos2(420.0, 290.0)),
        Capture("neural-left"),
        Move(egui::pos2(880.0, 480.0)),
        Capture("neural-right"),
        Click("Settings"),
        Click("Background selector"),
        Click("Background ASCII Flow"),
        Check(9),
        Click("Settings"),
        Move(egui::pos2(420.0, 290.0)),
        Capture("ascii-left"),
        Move(egui::pos2(880.0, 480.0)),
        Capture("ascii-right"),
        Tap(egui::pos2(760.0, 560.0)),
        Capture("ascii-ripple"),
        Move(egui::pos2(480.0, 310.0)),
        Capture("home-card-hover"),
        Move(egui::pos2(88.0, 690.0)),
        Capture("luma-hover-home"),
        Click("Luma"),
        Capture("companion-opening"),
        Capture("companion-open"),
        Key(Key::Escape, false),
        Capture("companion-closing"),
    ];
    struct Interaction {
        editor: Editor,
        steps: Vec<Action>,
        stage: usize,
        at: Instant,
        queued: Vec<Event>,
        requested: bool,
        output: PathBuf,
        deadline: Instant,
        fixture: tempfile::TempDir,
        render_ms: Vec<f64>,
        input_ms: Vec<f64>,
        input_at: Option<Instant>,
    }
    impl eframe::App for Interaction {
        fn raw_input_hook(&mut self, _ctx: &egui::Context, input: &mut egui::RawInput) {
            input.focused = true;
            if !self.queued.is_empty() {
                self.input_at = Some(Instant::now());
            }
            input.events.append(&mut self.queued);
        }
        fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
            assert!(
                Instant::now() < self.deadline,
                "native interaction timed out at {}",
                self.stage
            );
            let render_start = Instant::now();
            self.editor.update(ctx, frame);
            self.render_ms
                .push(render_start.elapsed().as_secs_f64() * 1000.0);
            if let Some(input_at) = self.input_at.take() {
                self.input_ms
                    .push(input_at.elapsed().as_secs_f64() * 1000.0);
            }
            let image = ctx.input(|i| {
                i.events.iter().find_map(|event| {
                    if let Event::Screenshot {
                        image, user_data, ..
                    } = event
                        && user_data.data.is_none()
                    {
                        Some(image.clone())
                    } else {
                        None
                    }
                })
            });
            if let Some(image) = image
                && let Capture(name) = self.steps[self.stage]
            {
                let bytes: Vec<_> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                image::save_buffer(
                    self.output.join(format!("{name}.png")),
                    &bytes,
                    image.size[0] as u32,
                    image.size[1] as u32,
                    image::ColorType::Rgba8,
                )
                .unwrap();
                self.stage += 1;
                self.at = Instant::now();
                self.requested = false;
            }
            if self.stage == self.steps.len() {
                self.render_ms.sort_by(f64::total_cmp);
                self.input_ms.sort_by(f64::total_cmp);
                let percentile = |v: &[f64], p: f64| v[((v.len() - 1) as f64 * p).round() as usize];
                let metrics = serde_json::json!({"input_source":"scripted native RawInput hook; desktop automation unavailable","render_samples":self.render_ms.len(),"render_cpu_ms_p50":percentile(&self.render_ms,0.5),"render_cpu_ms_p95":percentile(&self.render_ms,0.95),"input_to_cpu_paint_ms_p50":percentile(&self.input_ms,0.5),"input_to_cpu_paint_ms_p95":percentile(&self.input_ms,0.95),"physical_input_latency":"not measured","gpu_presentation_latency":"not measured"});
                std::fs::write(
                    self.output.join("interaction-metrics.json"),
                    serde_json::to_vec_pretty(&metrics).unwrap(),
                )
                .unwrap();
                self.editor.force_close = true;
                ctx.send_viewport_cmd(ViewportCommand::Close);
                return;
            }
            let delay = match self.steps[self.stage] {
                Capture("tab-moving") => 0.035,
                Capture("tab-settled") => 0.4,
                Capture("ascii-ripple") => 0.18,
                Capture("companion-opening") | Capture("companion-closing") => 0.07,
                _ => 0.35,
            };
            if self.at.elapsed().as_secs_f32() < delay || self.requested {
                ctx.request_repaint_after(Duration::from_millis(16));
                return;
            }
            match self.steps[self.stage] {
                Click(label) => {
                    let rect = ctx
                        .data(|d| {
                            d.get_temp::<std::collections::HashMap<String, egui::Rect>>(
                                egui::Id::new("native-review-targets"),
                            )
                        })
                        .and_then(|m| m.get(label).copied())
                        .unwrap_or_else(|| {
                            panic!("missing rendered control {label} at {}", self.stage)
                        });
                    let pos = rect.center();
                    self.queued.extend([
                        Event::PointerMoved(pos),
                        Event::PointerButton {
                            pos,
                            button: PointerButton::Primary,
                            pressed: true,
                            modifiers: Modifiers::NONE,
                        },
                        Event::PointerButton {
                            pos,
                            button: PointerButton::Primary,
                            pressed: false,
                            modifiers: Modifiers::NONE,
                        },
                    ]);
                }
                Key(key, command) => {
                    let modifiers = if command {
                        Modifiers::COMMAND
                    } else {
                        Modifiers::NONE
                    };
                    self.queued.push(Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers,
                    });
                    self.queued.push(Event::Key {
                        key,
                        physical_key: None,
                        pressed: false,
                        repeat: false,
                        modifiers,
                    });
                }
                Text(text) => self.queued.push(Event::Text(text.into())),
                Move(pos) => self.queued.push(Event::PointerMoved(pos)),
                Tap(pos) => self.queued.extend([
                    Event::PointerMoved(pos),
                    Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed: true,
                        modifiers: Modifiers::NONE,
                    },
                    Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed: false,
                        modifiers: Modifiers::NONE,
                    },
                ]),
                Capture(_) => {
                    ctx.send_viewport_cmd(ViewportCommand::Screenshot(UserData::default()));
                    self.requested = true;
                    ctx.request_repaint();
                    return;
                }
                Check(n) => match n {
                    1 => assert!(self.editor.directories.keys().any(|p| p.ends_with("src"))),
                    2 => assert!(
                        self.editor
                            .documents
                            .iter()
                            .any(|d| d.path.ends_with("main.rs"))
                    ),
                    3 => {
                        let doc = &self.editor.documents[self.editor.active.unwrap()];
                        assert!(!doc.dirty);
                        assert!(
                            std::fs::read_to_string(&doc.path)
                                .unwrap()
                                .contains("native interaction verification")
                        );
                    }
                    4 => assert!(self.editor.find_match.is_some()),
                    5 => assert!(
                        self.editor.documents[self.editor.active.unwrap()]
                            .path
                            .ends_with("Cargo.toml")
                    ),
                    6 => assert!(self.editor.companion.open),
                    7 => assert_eq!(self.editor.preferences.theme, theme::Theme::Light),
                    8 => assert_eq!(
                        self.editor.preferences.identity.background,
                        identity::Background::Neural
                    ),
                    9 => assert_eq!(
                        self.editor.preferences.identity.background,
                        identity::Background::Ascii
                    ),
                    _ => unreachable!(),
                },
                DismissFixtureCompanion => self.editor.companion.open = false,
            }
            self.stage += 1;
            self.at = Instant::now();
            ctx.request_repaint();
            let _ = &self.fixture;
        }
    }
    let output = PathBuf::from(
        std::env::var_os("COVALOS_INTERACTION_DIR")
            .or_else(|| std::env::var_os("AEROS_INTERACTION_DIR"))
            .ok_or("set COVALOS_INTERACTION_DIR")?,
    );
    std::fs::create_dir_all(&output)?;
    let fixture = tempfile::tempdir()?;
    let config = fixture.path().join("profile");
    std::fs::create_dir_all(&config)?;
    // SAFETY: set before any thread of this test reads the environment.
    unsafe { std::env::set_var("COVALOS_CONFIG_DIR", &config) };
    let root = fixture.path().join("covalos-project");
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(root.join("src/main.rs"), SAMPLE)?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"covalos-project\"\n",
    )?;
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([1366.0, 768.0])
            .with_decorations(false)
            .with_title("Covalos Precision Interaction Review"),
        event_loop_builder: Some(Box::new(|b| {
            b.with_any_thread(true);
        })),
        ..Default::default()
    };
    eframe::run_native(
        "Covalos Precision Interaction Review",
        options,
        Box::new(move |cc| {
            let mut editor = Editor::new(&cc.egui_ctx, Some(root), Preferences::default());
            editor.preferences_path = Some(fixture.path().join("profile/editor.json"));
            Ok(Box::new(Interaction {
                editor,
                steps,
                stage: 0,
                at: Instant::now(),
                queued: Vec::new(),
                requested: false,
                output,
                deadline: Instant::now() + Duration::from_secs(80),
                fixture,
                render_ms: Vec::new(),
                input_ms: Vec::new(),
                input_at: None,
            }))
        }),
    )?;
    Ok(())
}

/// Captures for the README and the website: the real application rendering a small
/// fixture project. Agent sessions, Luma's log and the approval are fixtures; no CLI or
/// provider is called. `COVALOS_SITE_DIR` is the output folder; `COVALOS_SITE_SCALE`
/// (default 1.4, which fits a 1920x1080 screen) multiplies the 1366x768 layout into pixels.
#[test]
#[ignore = "opens a native window and writes the website and README captures"]
fn capture_site_screens() -> Result<(), Box<dyn std::error::Error>> {
    use super::guided::{GuidedPlan, GuidedStep, StepAction};
    use crate::agent_gate::GateRequest;
    use crate::agents_runtime::{ClaudeModes, NewSession};
    use covalos_agents::permission::{Action, Preset};
    use eframe::egui::{Event, UserData, ViewportBuilder, ViewportCommand};
    use winit::platform::windows::EventLoopBuilderExtWindows;

    const SHOTS: &[&str] = &[
        "editor-nocturne",
        "guided-learn",
        "command-search",
        "home-nocturne",
        // Last: its agents stay alive and would show up in the other captures.
        "luma-notch",
    ];
    const SETTLE_FRAMES: usize = 45;

    struct Site {
        editor: Editor,
        root: PathBuf,
        output: PathBuf,
        scale: f32,
        size: [u32; 2],
        stage: usize,
        settle: usize,
        started: bool,
        requested: bool,
        deadline: Instant,
    }
    impl Site {
        fn enter(&mut self, ctx: &egui::Context) {
            self.editor.palette = None;
            self.editor.companion.open = false;
            self.editor.guided.open = false;
            self.editor.sidebar = Sidebar::Files;
            self.editor.sidebar_open = true;
            match SHOTS[self.stage] {
                "editor-nocturne" => {
                    self.editor.preferences.minimap = true;
                    self.editor.open_file(self.root.join("src/models.rs"));
                    self.editor.open_file(self.root.join("src/main.rs"));
                    self.editor.open_file(self.root.join("src/report.rs"));
                }
                "luma-notch" => {
                    // The chat panel would show the fixture account's placeholder model.
                    self.editor.preferences.assistant_open = false;
                    self.editor.open_file(self.root.join("src/report.rs"));
                    assert!(self.editor.ensure_runtime(ctx));
                    let runtime = self.editor.hub.agents.runtime.as_mut().unwrap();
                    runtime.seed_review_accounts();
                    let mut first = None;
                    for (title, account) in [
                        ("Fix the refund sign in summarize()", "claude-review"),
                        ("Add a regression test for refunds", "codex-review"),
                    ] {
                        let index = runtime
                            .create_session(NewSession {
                                title: title.into(),
                                account: covalos_agents::AccountId::new(account).unwrap(),
                                model: None,
                                effort: None,
                                claude_modes: ClaudeModes::default(),
                                root: self.root.clone(),
                                worktree: false,
                                preset: Preset::Supervised,
                                allow_safe_commands: false,
                                share_context: false,
                            })
                            .unwrap();
                        let id = runtime.sessions[index].meta.id.to_string();
                        runtime.review_waiting(&id);
                        first.get_or_insert(id);
                    }
                    // Pending approvals are read from the gate folder, as the hook writes them.
                    let session = first.unwrap();
                    let request = GateRequest {
                        ticket: "site-capture".into(),
                        session: session.clone(),
                        tool: "Bash".into(),
                        action: Action::Execute("cargo test --workspace".into()),
                        reason: "Runs the test suite".into(),
                        once_only: false,
                        created_ms: 0,
                        expires_ms: u64::MAX,
                    };
                    let dir = runtime.gate_dir.join(&session);
                    std::fs::create_dir_all(&dir).unwrap();
                    std::fs::write(
                        dir.join("request-site-capture.json"),
                        serde_json::to_vec(&request).unwrap(),
                    )
                    .unwrap();
                    self.editor.preferences.luma_effort = crate::luma::Effort::High;
                    self.editor.hub.agents.luma.review_order();
                    self.editor.companion.open = true;
                    ctx.data_mut(|d| d.insert_temp(egui::Id::new("luma-notch-tab"), 1u8));
                }
                "guided-learn" => {
                    let root = self.root.clone();
                    self.editor.preferences.guided.xp = 140;
                    self.editor.preferences.guided.streak = 6;
                    self.editor.guided.load_plan(lesson(), Some(&root));
                    self.editor.open_file(root.join("src/ledger.rs"));
                }
                "command-search" => {
                    self.editor.open_palette(ctx);
                    self.editor
                        .palette
                        .as_mut()
                        .unwrap()
                        .set_query("notifications");
                }
                "home-nocturne" => {
                    self.editor.documents.clear();
                    self.editor.active = None;
                }
                _ => unreachable!(),
            }
            self.settle = 0;
            self.requested = false;
        }
    }
    impl eframe::App for Site {
        fn raw_input_hook(&mut self, _: &egui::Context, input: &mut egui::RawInput) {
            input.focused = true;
            // The real pointer would trigger hovers and tooltips wherever it happens to be.
            input.events.retain(|event| {
                !matches!(
                    event,
                    Event::PointerMoved(_) | Event::MouseMoved(_) | Event::PointerButton { .. }
                )
            });
        }
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            assert!(Instant::now() < self.deadline, "site capture timed out");
            if !self.started {
                // Size the window in physical pixels whatever the monitor scale is,
                // then lay the interface out at 1366x768 points.
                let native = ctx.native_pixels_per_point().unwrap_or(1.0);
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(
                    self.size[0] as f32 / native,
                    self.size[1] as f32 / native,
                )));
                ctx.set_pixels_per_point(self.scale);
                self.started = true;
                self.enter(ctx);
            }
            let shot = ctx.input(|input| {
                input.events.iter().find_map(|event| match event {
                    Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(image) = shot {
                assert_eq!(
                    [image.size[0] as u32, image.size[1] as u32],
                    self.size,
                    "the window did not reach the capture size; lower COVALOS_SITE_SCALE"
                );
                let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                image::save_buffer(
                    self.output.join(format!("{}.png", SHOTS[self.stage])),
                    &bytes,
                    self.size[0],
                    self.size[1],
                    image::ColorType::Rgba8,
                )
                .expect("write site capture");
                self.stage += 1;
                if self.stage == SHOTS.len() {
                    self.editor.force_close = true;
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                    return;
                }
                self.enter(ctx);
            }
            self.editor.render(ctx);
            self.settle += 1;
            if self.settle >= SETTLE_FRAMES && !self.requested {
                ctx.send_viewport_cmd(ViewportCommand::Screenshot(UserData::default()));
                self.requested = true;
            }
            ctx.request_repaint_after(Duration::from_millis(40));
        }
    }

    fn step(
        path: &str,
        action: StepAction,
        requires: &[&str],
        why: &str,
        detail: &str,
        lines: &[(&str, &str)],
        calls: &[&str],
    ) -> GuidedStep {
        GuidedStep {
            path: path.into(),
            action,
            lines: lines.iter().map(|(line, _)| (*line).into()).collect(),
            requires: requires.iter().map(|r| (*r).into()).collect(),
            why: Some(why.into()),
            detail: Some(detail.into()),
            notes: lines.iter().map(|(_, note)| (*note).into()).collect(),
            calls: calls.iter().map(|c| (*c).into()).collect(),
        }
    }
    fn lesson() -> GuidedPlan {
        GuidedPlan {
            title: "Monthly totals for a small ledger".into(),
            steps: vec![
                step(
                    "src/models.rs",
                    StepAction::CreateFile,
                    &[],
                    "The ledger needs a shape for each entry before anything can add them up.",
                    "An enum lists the kinds of movement; a struct groups the fields every entry has.",
                    MODELS,
                    &[],
                ),
                step(
                    "src/ledger.rs",
                    StepAction::CreateFile,
                    &["src/models.rs"],
                    "Totals live in their own module so main.rs only asks for them.",
                    "summarize walks every entry once, turns its kind into a sign and adds the signed amount to its account.",
                    LEDGER,
                    &[
                        "summarize → HashMap::new() (creates the totals map)",
                        "summarize → entry() / or_insert() (finds or starts an account's total)",
                    ],
                ),
                step(
                    "src/main.rs",
                    StepAction::WriteBlock,
                    &["src/ledger.rs"],
                    "main builds a few entries and prints what summarize returns.",
                    "The entry point only wires the pieces together.",
                    &[
                        (
                            "mod ledger;",
                            "Declares the ledger module from src/ledger.rs.",
                        ),
                        (
                            "mod models;",
                            "Declares the models module from src/models.rs.",
                        ),
                    ],
                    &[],
                ),
            ],
        }
    }
    const MODELS: &[(&str, &str)] = &[
        (
            "#[derive(Debug, Clone, Copy)]",
            "Lets Kind be printed and copied freely.",
        ),
        (
            "pub enum Kind {",
            "The three kinds of movement an entry can be.",
        ),
        ("    Credit,", "Money coming in."),
        ("    Debit,", "Money going out."),
        (
            "    Refund,",
            "Money returned to a customer, so it also goes out.",
        ),
        ("}", "Closes the enum."),
        ("", ""),
        ("pub struct Entry {", "One line of the ledger."),
        (
            "    pub account: String,",
            "Which account the money belongs to.",
        ),
        (
            "    pub amount: f64,",
            "How much, always positive; the kind gives the sign.",
        ),
        ("    pub kind: Kind,", "Credit, debit or refund."),
        ("}", "Closes the struct."),
    ];
    const LEDGER: &[(&str, &str)] = &[
        (
            "use std::collections::HashMap;",
            "Brings in the map type used for the totals.",
        ),
        (
            "use crate::models::{Entry, Kind};",
            "Uses the types written in the previous step.",
        ),
        ("", ""),
        (
            "/// Balance per account, counting each refund once.",
            "A doc comment: it shows up in editor hovers.",
        ),
        (
            "pub fn summarize(entries: &[Entry]) -> HashMap<String, f64> {",
            "Takes a slice of entries and returns one total per account.",
        ),
        (
            "    let mut totals = HashMap::new();",
            "An empty map that will collect the totals; mut because it changes.",
        ),
        ("    for entry in entries {", "Visits every entry once."),
        (
            "        let sign = match entry.kind {",
            "match picks a value for each kind of entry.",
        ),
        (
            "            Kind::Credit => 1.0,",
            "Credits add to the balance.",
        ),
        (
            "            Kind::Debit | Kind::Refund => -1.0,",
            "Debits and refunds subtract. One arm, so a refund is counted once.",
        ),
        ("        };", "Ends the match; sign is now 1.0 or -1.0."),
        (
            "        *totals.entry(entry.account.clone()).or_insert(0.0) += sign * entry.amount;",
            "Finds the account's total (starting at 0) and adds the signed amount.",
        ),
        ("    }", "Ends the loop."),
        ("    totals", "The last expression is the return value."),
        ("}", "Closes the function."),
    ];
    const REPORT: &str = r#"
/// Accounts sorted by balance, largest first, for the monthly report.
pub fn ranked(totals: &HashMap<String, f64>) -> Vec<(&str, f64)> {
    let mut rows: Vec<(&str, f64)> = totals
        .iter()
        .map(|(account, total)| (account.as_str(), *total))
        .collect();
    rows.sort_by(|a, b| b.1.total_cmp(&a.1));
    rows
}

/// One aligned line per account, with the sign always shown.
pub fn render(totals: &HashMap<String, f64>) -> String {
    ranked(totals)
        .into_iter()
        .map(|(account, total)| format!("{account:>12}  {total:+.2}\n"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Entry, Kind};

    #[test]
    fn a_refund_is_counted_once() {
        let entries = [
            Entry { account: "orbit".into(), amount: 120.0, kind: Kind::Credit },
            Entry { account: "orbit".into(), amount: 20.0, kind: Kind::Refund },
        ];
        assert_eq!(summarize(&entries)["orbit"], 100.0);
    }
}
"#;

    let output = PathBuf::from(std::env::var_os("COVALOS_SITE_DIR").ok_or("set COVALOS_SITE_DIR")?);
    std::fs::create_dir_all(&output)?;
    let scale = std::env::var("COVALOS_SITE_SCALE")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(1.4);
    let size = [
        (1366.0 * scale).round() as u32,
        (768.0 * scale).round() as u32,
    ];

    let fixture = tempfile::tempdir()?;
    let config = fixture.path().join("config");
    std::fs::create_dir_all(&config)?;
    // SAFETY: set before any thread of this test reads the environment.
    unsafe { std::env::set_var("COVALOS_CONFIG_DIR", &config) };
    // The project path shows up in agent cards, so captures meant for publishing should
    // set COVALOS_SITE_ROOT to a neutral folder (it must not exist; it is removed after).
    let site_root = std::env::var_os("COVALOS_SITE_ROOT").map(PathBuf::from);
    if let Some(path) = &site_root {
        assert!(!path.exists(), "COVALOS_SITE_ROOT must not exist yet");
    }
    let root = site_root
        .clone()
        .unwrap_or_else(|| fixture.path().join("orbit-ledger"));
    for dir in ["src", "tests", "docs"] {
        std::fs::create_dir_all(root.join(dir))?;
    }
    let join = |lines: &[(&str, &str)]| {
        lines
            .iter()
            .map(|(line, _)| format!("{line}\n"))
            .collect::<String>()
    };
    std::fs::write(root.join("src/models.rs"), join(MODELS))?;
    // The ledger step is half typed, so Guided Mode resumes on the loop.
    std::fs::write(root.join("src/ledger.rs"), join(&LEDGER[..6]))?;
    std::fs::write(root.join("src/main.rs"), SAMPLE)?;
    std::fs::write(
        root.join("src/report.rs"),
        format!("{}{REPORT}", join(LEDGER)),
    )?;
    std::fs::write(
        root.join("src/notifications.rs"),
        "// Sends the monthly report by email.\n",
    )?;
    std::fs::write(
        root.join("tests/ledger.rs"),
        "// Regression tests for the ledger.\n",
    )?;
    std::fs::write(root.join("README.md"), "# orbit-ledger\n")?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"orbit-ledger\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;

    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([size[0] as f32, size[1] as f32])
            .with_position([0.0, 0.0])
            .with_decorations(false)
            .with_title("Covalos site capture"),
        event_loop_builder: Some(Box::new(|builder| {
            builder.with_any_thread(true);
        })),
        ..Default::default()
    };
    eframe::run_native(
        "Covalos site capture",
        options,
        Box::new(move |cc| {
            let mut preferences = Preferences::default();
            preferences.theme = serde_json::from_str("\"nocturne\"").unwrap_or_default();
            preferences.language = super::i18n::Language::English;
            let editor = Editor::new(&cc.egui_ctx, Some(root.clone()), preferences);
            Ok(Box::new(Site {
                editor,
                root,
                output,
                scale,
                size,
                stage: 0,
                settle: 0,
                started: false,
                requested: false,
                deadline: Instant::now() + Duration::from_secs(120),
            }))
        }),
    )?;
    if let Some(path) = site_root {
        std::fs::remove_dir_all(path)?;
    }
    Ok(())
}

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
    ];

    struct Review {
        editor: Editor,
        root: PathBuf,
        output: PathBuf,
        scale: f32,
        stage: usize,
        settle: usize,
        requested: bool,
        deadline: Instant,
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
            if self.settle > 5 && !self.requested {
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
            let mut editor = Editor::new(&cc.egui_ctx, Some(root.clone()), Preferences::default());
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
                deadline: Instant::now() + Duration::from_secs(45),
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

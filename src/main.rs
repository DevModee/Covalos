#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod agent_bridge;
mod assistant;
mod brand;
mod editor;
mod library;
mod paths;
mod process;
mod workspace;

use eframe::egui;

fn main() -> eframe::Result {
    if std::env::args().nth(1).as_deref() == Some("--agent-hook") {
        if let Err(error) = agent_bridge::hook_entry() {
            eprintln!("{} hook: {error}", brand::NAME);
        }
        return Ok(());
    }
    if std::env::args().nth(1).as_deref() == Some("--library") {
        // Structured Library Studio tools for agents; prints JSON and exits.
        let args: Vec<String> = std::env::args().skip(2).collect();
        std::process::exit(library::tool_main(&args));
    }
    let root = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(brand::TITLE)
            .with_icon(brand::window_icon())
            .with_inner_size([1390.0, 860.0])
            .with_decorations(false)
            .with_min_inner_size([800.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        editor::TITLE,
        options,
        Box::new(move |cc| {
            Ok(Box::new(editor::Editor::from_preferences(
                &cc.egui_ctx,
                root,
            )))
        }),
    )
}

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod assistant;
mod editor;
mod process;
mod workspace;

use eframe::egui;

fn main() -> eframe::Result {
    let root = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1390.0, 860.0])
            .with_min_inner_size([920.0, 600.0]),
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

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod ui;
use eframe::egui;
fn main() -> eframe::Result {
    if std::env::args().any(|s| {
        s == "--playlists-preview"
            || s == "--web-preview"
            || s == "--grouping-preview"
            || s == "--smoke-test"
            || s == "--popup-test"
            || s == "--music-test"
            || s == "--batch-tags-preview"
            || s == "--tray-test"
    }) {
        let directory = std::env::current_dir().unwrap().join(
            if std::env::args().any(|s| s == "--web-preview") {
                "artifacts/web-data"
            } else if std::env::args().any(|s| s == "--tray-test") {
                "artifacts/tray-data"
            } else if std::env::args().any(|s| s == "--playlists-preview") {
                "artifacts/playlists-data"
            } else if std::env::args().any(|s| s == "--grouping-preview") {
                "artifacts/grouping-data"
            } else if std::env::args().any(|s| s == "--batch-tags-preview") {
                "artifacts/batch-tags-data"
            } else {
                "artifacts/smoke-data"
            },
        );
        // No threads have been started yet; set the test directory before eframe starts.
        unsafe { std::env::set_var("SEQUOIA_DATA_DIR", directory) };
    }
    let Some(instance) = sequoia::instance::Instance::start(
        &sequoia::library::data_dir(),
        sequoia::instance::launch_paths(),
    )
        .map_err(|error| eframe::Error::AppCreation(error.into()))?
    else {
        return Ok(());
    };
    eframe::run_native(
        "Sequoia",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1100., 720.])
                .with_min_inner_size([960., 580.])
                .with_decorations(false)
                .with_resizable(true)
                .with_drag_and_drop(true)
                .with_icon(sequoia::icons::app_icon()),
            // Application identity belongs to the OS shell; the workspace contains tools only.
            renderer: eframe::Renderer::Glow,
            ..Default::default()
        },
        Box::new(move |cc| Ok(Box::new(ui::Player::new(cc, instance)))),
    )
}

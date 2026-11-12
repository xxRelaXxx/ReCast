#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod engine;
mod protocol;
mod softcam;
mod theme;
mod ui;

use eframe::egui;
use engine::Engine;
use std::sync::Arc;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 780.0])
            .with_min_inner_size([860.0, 560.0])
            .with_title("Rewebcam"),
        ..Default::default()
    };

    eframe::run_native(
        "Rewebcam",
        options,
        Box::new(|cc| {
            let engine = Arc::new(Engine::new());
            Ok(Box::new(ui::RewebcamApp::new(cc, engine)))
        }),
    )
}

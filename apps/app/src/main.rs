#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod control;
mod engine;
mod pairing;
mod protocol;
mod softcam;
mod theme;
mod ui;
mod window;

use eframe::egui;
use engine::Engine;
use std::sync::Arc;

/// A UTF-16 string for the Windows API.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Window icon, pre-rendered as 256 × 256 RGBA so the app needs no image
/// decoder at startup.
fn window_icon() -> egui::IconData {
    const SIDE: u32 = 256;
    egui::IconData {
        rgba: include_bytes!("../assets/icon.rgba").to_vec(),
        width: SIDE,
        height: SIDE,
    }
}

fn main() -> eframe::Result<()> {
    // Only one ReCast can listen for the phone, so a second copy is closed
    // right away instead of showing a window that never connects.
    if std::net::TcpListener::bind(("0.0.0.0", 47777)).is_err() {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            let log = std::path::Path::new(&local)
                .join("ReCast")
                .join("launcher.log");
            let _ = std::fs::write(
                log,
                "ReCast is already running. The second copy was closed.\n",
            );
        }
        std::process::exit(0);
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 780.0])
            .with_min_inner_size([860.0, 560.0])
            .with_title("ReCast")
            .with_decorations(false)
            .with_resizable(true)
            .with_icon(window_icon()),
        ..Default::default()
    };

    eframe::run_native(
        "ReCast",
        options,
        Box::new(|cc| {
            let engine = Arc::new(Engine::new());

            // The command line tool talks to the app through this channel.
            if let Err(error) = control::start(engine.clone()) {
                engine.log(format!("The control channel is not available: {error}"));
            }

            Ok(Box::new(ui::ReCastApp::new(cc, engine)))
        }),
    )
}

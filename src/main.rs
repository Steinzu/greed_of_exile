#![windows_subsystem = "windows"]

mod api;
mod app;
mod models;
mod storage;

use app::GreedOfExileApp;
use eframe::egui;

fn load_icon() -> std::sync::Arc<egui::IconData> {
    let image = image::load_from_memory(include_bytes!("../logo.png"))
        .expect("Failed to load logo.png")
        .into_rgba8();
    let (width, height) = image.dimensions();
    std::sync::Arc::new(egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    })
}

fn main() -> eframe::Result<()> {
    let rt = tokio::runtime::Runtime::new().expect("Failed to build Tokio runtime");
    // Keep the runtime alive for the entire duration of the program and make it
    // the ambient runtime for the current thread so that tokio::spawn works
    // inside egui's synchronous update loop.
    let _rt_guard = rt.enter();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([850.0, 650.0])
            .with_icon(load_icon()),
        ..Default::default()
    };

    eframe::run_native(
        "Greed of Exile - Steins",
        options,
        Box::new(|cc| Ok(Box::new(GreedOfExileApp::new(cc)))),
    )
}

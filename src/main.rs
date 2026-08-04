//! Pane - a transparent window into your system.
//!
//! GPU-accelerated native GUI. Lightweight, performant, single binary.

#![windows_subsystem = "windows"]

mod app;
mod collect;
mod config;
mod gui;
mod metrics;

fn load_icon() -> Option<egui::IconData> {
    let bytes = include_bytes!("../assets/logo.png");
    let img = image::load_from_memory(bytes).ok()?.into_rgba8();
    let (w, h) = img.dimensions();
    Some(egui::IconData {
        rgba: img.into_raw(),
        width: w,
        height: h,
    })
}

use eframe::egui;

/// Register a stable AppUserModelID so Windows can associate this process
/// with the app for taskbar grouping and icon display. Without it the
/// taskbar shows a generic icon for the running window.
#[cfg(target_os = "windows")]
fn set_app_user_model_id(app_id: &str) {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;

    let wide: Vec<u16> = app_id.encode_utf16().chain(std::iter::once(0)).collect();
    let _ = unsafe { SetCurrentProcessExplicitAppUserModelID(PCWSTR(wide.as_ptr())) };
}

fn main() -> eframe::Result<()> {
    #[cfg(target_os = "windows")]
    set_app_user_model_id("TxsharDev.Pane");

    let cfg = config::Config::load();

    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size([cfg.window_width, cfg.window_height])
        .with_min_inner_size([800.0, 500.0])
        .with_title("Pane");

    if let Some(icon) = load_icon() {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Pane",
        options,
        Box::new(move |cc| Ok(Box::new(gui::PaneApp::new(cc, cfg)))),
    )
}

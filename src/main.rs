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

use eframe::{egui, egui_wgpu};

/// Parse `--gpu <name-substring>` from the command line. Returns None when absent.
fn cli_gpu_flag() -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--gpu" {
            return args.next();
        }
    }
    None
}

/// Enumerate wgpu adapter names for the render-GPU picker, deduplicated across
/// backends. Returns an empty list if no adapters are found, in which case
/// selection falls back to automatic.
fn enumerate_render_gpus() -> Vec<String> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let mut names: Vec<String> = Vec::new();
    for adapter in instance.enumerate_adapters(wgpu::Backends::PRIMARY) {
        let name = adapter.get_info().name;
        if !name.is_empty() && !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// Build an egui-wgpu adapter selector that matches `choice` as a
/// case-insensitive substring of the adapter name. Falls back to the first
/// surface-compatible adapter when nothing matches, so a stale config entry
/// or bad --gpu value never prevents startup.
fn make_adapter_selector(choice: String) -> egui_wgpu::NativeAdapterSelectorMethod {
    let choice = choice.to_lowercase();
    std::sync::Arc::new(move |adapters, surface| {
        let compatible =
            |a: &wgpu::Adapter| surface.is_none_or(|s| a.is_surface_supported(s));
        adapters
            .iter()
            .filter(|a| compatible(a))
            .find(|a| a.get_info().name.to_lowercase().contains(&choice))
            .or_else(|| adapters.iter().find(|a| compatible(a)))
            .cloned()
            .ok_or_else(|| "no compatible wgpu adapter found".to_string())
    })
}

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
    set_app_user_model_id("ALIA.Pane");

    let cfg = config::Config::load();

    // Render-GPU choice: CLI flag > saved config > automatic.
    let render_gpus = enumerate_render_gpus();
    let gpu_choice = cli_gpu_flag().or_else(|| cfg.render_gpu.clone());

    let mut wgpu_options = egui_wgpu::WgpuConfiguration::default();
    if let Some(choice) = gpu_choice
        && let egui_wgpu::WgpuSetup::CreateNew(setup) = &mut wgpu_options.wgpu_setup
    {
        setup.native_adapter_selector = Some(make_adapter_selector(choice));
    }

    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size([cfg.window_width, cfg.window_height])
        .with_min_inner_size([800.0, 500.0])
        .with_title("Pane");

    if let Some(icon) = load_icon() {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }

    let options = eframe::NativeOptions {
        viewport,
        wgpu_options,
        ..Default::default()
    };

    eframe::run_native(
        "Pane",
        options,
        Box::new(move |cc| Ok(Box::new(gui::PaneApp::new(cc, cfg, render_gpus)))),
    )
}

//! Pane GUI - egui/eframe based native window.

pub mod theme;
pub mod panels;
pub mod widgets;

use std::sync::mpsc;
use std::time::Duration;

use eframe::egui;

use crate::app::{App, Panel};
use crate::collect;
use crate::config::Config;
use crate::metrics::gpu;
use crate::metrics::gpu::pdh::PdhGpuCollector;
use crate::metrics::system::SystemCollector;
use theme::ThemeMode;

struct MetricSnapshot {
    app: App,
}

/// Commands sent from UI thread to the background metric thread.
pub enum GpuCommand {
    SetPowerLimit { gpu_index: usize, watts: f64 },
}

pub struct PaneApp {
    app: App,
    rx: mpsc::Receiver<MetricSnapshot>,
    cmd_tx: mpsc::Sender<GpuCommand>,
    loading: bool,
    sidebar_width: f32,
    theme_mode: ThemeMode,
    config: Config,
    /// Adapter names detected at startup, for the render-GPU picker.
    render_gpus: Vec<String>,
    /// Set when the render-GPU choice changed this session (applies after restart).
    render_gpu_changed: bool,
}

impl PaneApp {
    pub fn new(cc: &eframe::CreationContext<'_>, config: Config, render_gpus: Vec<String>) -> Self {
        // Load custom Airstrike font for branding
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "airstrike".to_owned(),
            std::sync::Arc::new(egui::FontData::from_static(
                include_bytes!("../../assets/pane-font.ttf"),
            )),
        );
        fonts.families.entry(egui::FontFamily::Name("Airstrike".into()))
            .or_default()
            .push("airstrike".to_owned());
        // Fallback to default proportional for missing glyphs
        fonts.families.entry(egui::FontFamily::Name("Airstrike".into()))
            .or_default()
            .push("Ubuntu-Light".to_owned());
        cc.egui_ctx.set_fonts(fonts);

        let mode = config.theme_mode();
        theme::apply(&cc.egui_ctx, mode);

        let refresh_ms = config.refresh_ms;
        let tick_rate = Duration::from_millis(refresh_ms);
        let mut app = App::new(tick_rate);
        app.selected_gpu = config.selected_gpu;

        let (tx, rx) = mpsc::channel();
        let (cmd_tx, cmd_rx) = mpsc::channel::<GpuCommand>();

        std::thread::spawn(move || {
            let mut sys = SystemCollector::new();
            let mut gpu_backend = gpu::create_backend();
            let mut pdh = PdhGpuCollector::new();
            let mut local_app = App::new(Duration::from_millis(refresh_ms));

            loop {
                // Process any pending GPU commands
                while let Ok(cmd) = cmd_rx.try_recv() {
                    match cmd {
                        GpuCommand::SetPowerLimit { gpu_index, watts } => {
                            if let Err(e) = gpu_backend.set_power_limit(gpu_index, watts) {
                                local_app.status_msg = Some((e, true));
                            } else {
                                local_app.status_msg = Some((format!("Power limit set to {:.0}W", watts), false));
                            }
                        }
                    }
                }

                collect::collect_metrics(&mut local_app, &mut sys, &mut gpu_backend, &mut pdh);
                let _ = tx.send(MetricSnapshot {
                    app: local_app.clone(),
                });
                std::thread::sleep(Duration::from_millis(refresh_ms));
            }
        });

        Self {
            app,
            rx,
            cmd_tx,
            loading: true,
            sidebar_width: config.sidebar_width,
            theme_mode: mode,
            config,
            render_gpus,
            render_gpu_changed: false,
        }
    }

}

impl eframe::App for PaneApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(snapshot) = self.rx.try_recv() {
            let panel = self.app.active_panel;
            let sel_gpu = self.app.selected_gpu;
            let controls = self.app.gpu_controls.clone();
            let sort = self.app.sort_column;
            let asc = self.app.sort_ascending;
            let filter = self.app.filter.clone();
            let sel_proc = self.app.process_selected;
            let confirm = self.app.confirm_kill;
            let status = self.app.status_msg.clone();

            self.app = snapshot.app;
            self.app.active_panel = panel;
            self.app.selected_gpu = sel_gpu;
            self.app.gpu_controls = controls;
            self.app.sort_column = sort;
            self.app.sort_ascending = asc;
            self.app.filter = filter;
            self.app.process_selected = sel_proc;
            self.app.confirm_kill = confirm;
            // Only preserve local status if background didn't set one
            if self.app.status_msg.is_none() {
                self.app.status_msg = status;
            }

            if self.loading {
                self.loading = false;
            }
        }

        // Save window size when resized
        if let Some(rect) = ctx.input(|i| i.viewport().inner_rect) {
            let w = rect.width();
            let h = rect.height();
            if (w - self.config.window_width).abs() > 10.0 || (h - self.config.window_height).abs() > 10.0 {
                self.config.window_width = w;
                self.config.window_height = h;
                self.config.save();
            }
        }

        ctx.request_repaint_after(Duration::from_millis(self.config.refresh_ms));

        if self.loading {
            self.draw_loading(ctx);
            return;
        }

        self.draw_main(ctx);
    }
}

impl PaneApp {
    fn draw_loading(&self, ctx: &egui::Context) {
        let p = theme::p();
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() / 3.0);
                ui.label(egui::RichText::new("PANE")
                    .font(egui::FontId::new(80.0, egui::FontFamily::Name("Airstrike".into())))
                    .color(p.accent));
                ui.add_space(12.0);
                ui.label(egui::RichText::new("A transparent window into your system").size(16.0).color(p.dim));
                ui.add_space(32.0);
                ui.spinner();
                ui.add_space(12.0);
                ui.label(egui::RichText::new("Initializing...").size(12.0).color(p.dim).monospace());
            });
        });
    }

    fn draw_main(&mut self, ctx: &egui::Context) {
        let p = theme::p();

        egui::SidePanel::left("sidebar")
            .exact_width(self.sidebar_width)
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_space(20.0);

                // Logo - Airstrike font
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new("PANE")
                        .font(egui::FontId::new(32.0, egui::FontFamily::Name("Airstrike".into())))
                        .color(p.accent));
                    ui.label(egui::RichText::new("SYSTEM MONITOR").size(9.0).color(p.dim));
                });

                ui.add_space(12.0);

                // Theme toggle - full width button
                let theme_text = format!("Theme: {}", self.theme_mode.label());
                if ui.add_sized(
                    [ui.available_width(), 22.0],
                    egui::Button::new(egui::RichText::new(&theme_text).size(10.0).color(p.dim))
                        .fill(p.card_bg)
                        .corner_radius(egui::CornerRadius::same(4)),
                ).clicked() {
                    self.theme_mode = self.theme_mode.next();
                    theme::apply(ctx, self.theme_mode);
                    self.config.theme = self.theme_mode.label().to_lowercase();
                    self.config.save();
                }

                // Render GPU picker - the adapter is fixed at startup, so a
                // change here only takes effect on the next launch.
                if !self.render_gpus.is_empty() {
                    ui.add_space(4.0);
                    let current = self.config.render_gpu.as_deref().unwrap_or("Automatic");
                    let mut pick: Option<Option<String>> = None;
                    egui::ComboBox::from_id_salt("render_gpu")
                        .width(ui.available_width())
                        .selected_text(egui::RichText::new(format!("Render: {}", current.replace("NVIDIA GeForce ", ""))).size(10.0).color(p.dim))
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(self.config.render_gpu.is_none(), "Automatic").clicked() {
                                pick = Some(None);
                            }
                            for name in &self.render_gpus {
                                let selected = self.config.render_gpu.as_deref() == Some(name.as_str());
                                if ui.selectable_label(selected, name).clicked() {
                                    pick = Some(Some(name.clone()));
                                }
                            }
                        });
                    if let Some(choice) = pick
                        && choice != self.config.render_gpu
                    {
                        self.config.render_gpu = choice;
                        self.config.save();
                        self.render_gpu_changed = true;
                    }
                    if self.render_gpu_changed {
                        ui.label(egui::RichText::new("Applies after restart").size(9.0).color(p.dim));
                    }
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                let panels = [
                    (Panel::Dashboard, "Dashboard"),
                    (Panel::Gpu, "GPU"),
                    (Panel::Cpu, "CPU"),
                    (Panel::Memory, "Memory"),
                    (Panel::Disk, "Disk"),
                    (Panel::Network, "Network"),
                    (Panel::Processes, "Processes"),
                    (Panel::GpuControl, "GPU Control"),
                    (Panel::VramCalc, "VRAM Calc"),
                    (Panel::Snapshot, "Snapshot"),
                ];

                for (panel, label) in panels {
                    let is_active = self.app.active_panel == panel;
                    let text = egui::RichText::new(label)
                        .size(13.0)
                        .color(if is_active { p.accent } else { p.text });

                    let response = ui.add_sized(
                        [ui.available_width(), 28.0],
                        egui::SelectableLabel::new(is_active, text),
                    );
                    if response.clicked() {
                        self.app.active_panel = panel;
                    }
                }

                if self.app.gpus.len() > 1 {
                    ui.add_space(16.0);
                    ui.separator();
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("GPU Select").size(11.0).color(p.dim));
                    for (i, gpu_m) in self.app.gpus.iter().enumerate() {
                        let short_name = gpu_m.name.replace("NVIDIA GeForce ", "");
                        let response = ui.selectable_label(
                            self.app.selected_gpu == i,
                            egui::RichText::new(format!("  {} {}", if self.app.selected_gpu == i { ">" } else { " " }, short_name)).size(12.0),
                        );
                        if response.clicked() {
                            self.app.selected_gpu = i;
                        }
                    }
                }

                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("Built by ALIA Labs").size(10.0).color(p.dim));
                    ui.hyperlink_to(egui::RichText::new("alialabs.org").size(10.0), "https://alialabs.org");
                });
            });

        // Collect command sender for panels that need it
        let cmd_tx = self.cmd_tx.clone();

        egui::CentralPanel::default().show(ctx, |ui| {
            match self.app.active_panel {
                Panel::Dashboard => panels::dashboard::draw(ui, &self.app),
                Panel::Gpu => panels::gpu::draw(ui, &mut self.app),
                Panel::Cpu => panels::cpu::draw(ui, &self.app),
                Panel::Memory => panels::memory::draw(ui, &self.app),
                Panel::Disk => panels::disk::draw(ui, &self.app),
                Panel::Network => panels::network::draw(ui, &self.app),
                Panel::Processes => panels::processes::draw(ui, &mut self.app),
                Panel::GpuControl => panels::gpu_control::draw(ui, &mut self.app, &cmd_tx),
                Panel::VramCalc => panels::vram_calc::draw(ui, &self.app),
                Panel::Snapshot => panels::snapshot::draw(ui, &self.app),
            }
        });
    }
}

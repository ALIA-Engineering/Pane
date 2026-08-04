//! GPU Control panel - fan, power limit, clock offsets with sliders.
//!
//! Power limit control is wired to NVML (requires admin).
//! Fan speed and clock offsets are NOT implemented. The sliders are shown
//! read-only and disabled; changing them does nothing. Implementing them
//! needs NVAPI (undocumented, privileged), which Pane does not link against.

use std::sync::mpsc;
use eframe::egui;
use crate::app::{App, GpuControl};
use crate::collect;
use crate::gui::{theme, widgets};
use crate::gui::GpuCommand;

pub fn draw(ui: &mut egui::Ui, app: &mut App, cmd_tx: &mpsc::Sender<GpuCommand>) {
    let p = theme::p();

    if app.gpus.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(egui::RichText::new("No GPU detected").color(p.dim));
        });
        return;
    }

    while app.gpu_controls.len() <= app.selected_gpu {
        app.gpu_controls.push(GpuControl::new());
    }

    let gpu = &app.gpus[app.selected_gpu.min(app.gpus.len() - 1)];
    let elevated = collect::is_elevated();

    egui::ScrollArea::vertical().show(ui, |ui| {
        let short = gpu.name.replace("NVIDIA GeForce ", "");
        widgets::section_header(ui, &format!("{} - Control Panel", short));

        // Admin warning banner
        if !elevated {
            egui::Frame::NONE
                .fill(p.yellow.gamma_multiply(0.1))
                .corner_radius(egui::CornerRadius::same(6))
                .stroke(egui::Stroke::new(1.0_f32, p.yellow))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("!").size(14.0).color(p.yellow).strong());
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new("Not running as administrator").size(12.0).color(p.yellow).strong());
                            ui.label(egui::RichText::new("GPU controls require admin privileges. Right-click pane.exe and select 'Run as administrator' to enable controls.").size(10.0).color(p.dim));
                        });
                    });
                });
            ui.add_space(8.0);
        }

        // Live stats bar
        ui.horizontal(|ui| {
            if let Some(t) = gpu.temp_core {
                ui.label(egui::RichText::new(format!("{}C", t)).size(14.0).color(theme::temp_color(t)).strong());
                ui.separator();
            }
            if let Some(w) = gpu.power_watts {
                ui.label(egui::RichText::new(format!("{:.0}W", w)).size(14.0).color(p.yellow).strong());
                ui.separator();
            }
            ui.label(egui::RichText::new(format!("{:.0}%", gpu.utilization)).size(14.0).color(theme::usage_color(gpu.utilization)).strong());
            if let Some(core) = gpu.clock_core_mhz {
                ui.separator();
                ui.label(egui::RichText::new(format!("{} MHz", core)).size(12.0).color(p.dim));
            }
        });

        ui.add_space(16.0);

        let ctrl = &mut app.gpu_controls[app.selected_gpu];

        // Power Limit (NVML - functional)
        egui::Frame::NONE
            .fill(p.panel_bg)
            .corner_radius(egui::CornerRadius::same(6))
            .stroke(egui::Stroke::new(1.0_f32, if elevated { p.accent } else { p.border }))
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Power Limit").size(13.0).color(p.text).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if elevated {
                            ui.label(egui::RichText::new("NVML").size(9.0).color(p.green));
                        } else {
                            ui.label(egui::RichText::new("Needs admin").size(9.0).color(p.yellow));
                        }
                    });
                });
                let default_limit = gpu.power_limit.unwrap_or(300.0);
                let mut val = ctrl.power_limit_watts.unwrap_or(default_limit) as f32;
                let max = (default_limit * 1.15) as f32;
                ui.add_enabled(elevated, egui::Slider::new(&mut val, 100.0..=max).suffix("W"));
                ctrl.power_limit_watts = Some(val as f64);
                ui.label(egui::RichText::new(format!("Default: {:.0}W | Max: {:.0}W", default_limit, max)).size(10.0).color(p.dim));
            });

        ui.add_space(8.0);

        // Fan Speed - display only, not implemented (needs NVAPI)
        egui::Frame::NONE
            .fill(p.panel_bg)
            .corner_radius(egui::CornerRadius::same(6))
            .stroke(egui::Stroke::new(1.0_f32, p.border.gamma_multiply(0.5)))
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Fan Speed").size(13.0).color(p.dim).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new("Not implemented").size(9.0).color(p.dim));
                    });
                });
                ui.label(egui::RichText::new(format!("Current: {}%", gpu.fan_rpm.unwrap_or(0))).size(11.0).color(p.dim));
            });

        ui.add_space(8.0);

        // Core Clock Offset - display only, not implemented (needs NVAPI)
        egui::Frame::NONE
            .fill(p.panel_bg)
            .corner_radius(egui::CornerRadius::same(6))
            .stroke(egui::Stroke::new(1.0_f32, p.border.gamma_multiply(0.5)))
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Core Clock Offset").size(13.0).color(p.dim).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new("Not implemented").size(9.0).color(p.dim));
                    });
                });
                if let Some(core) = gpu.clock_core_mhz {
                    ui.label(egui::RichText::new(format!("Current: {} MHz", core)).size(11.0).color(p.dim));
                }
            });

        ui.add_space(8.0);

        // Memory Clock Offset - display only, not implemented (needs NVAPI)
        egui::Frame::NONE
            .fill(p.panel_bg)
            .corner_radius(egui::CornerRadius::same(6))
            .stroke(egui::Stroke::new(1.0_f32, p.border.gamma_multiply(0.5)))
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Memory Clock Offset").size(13.0).color(p.dim).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new("Not implemented").size(9.0).color(p.dim));
                    });
                });
                if let Some(mem) = gpu.clock_mem_mhz {
                    ui.label(egui::RichText::new(format!("Current: {} MHz", mem)).size(11.0).color(p.dim));
                }
            });

        ui.add_space(16.0);

        // Apply / Reset
        ui.horizontal(|ui| {
            let apply = ui.add_enabled(
                elevated,
                egui::Button::new(egui::RichText::new("Apply Power Limit").size(13.0).color(if elevated { p.accent } else { p.dim })),
            );
            if apply.clicked() && let Some(watts) = ctrl.power_limit_watts {
                let _ = cmd_tx.send(GpuCommand::SetPowerLimit {
                    gpu_index: app.selected_gpu,
                    watts,
                });
            }
            if !elevated {
                apply.on_hover_text("Run Pane as administrator to use GPU controls");
            }

            if ui.button(egui::RichText::new("Reset").size(13.0)).clicked() {
                *ctrl = GpuControl::new();
            }
        });
    });
}

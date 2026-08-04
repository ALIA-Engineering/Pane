//! GPU detail panel - metrics, charts, and GPU processes.

use eframe::egui;
use crate::app::{App, GpuProcessKind};
use crate::collect;
use crate::gui::{theme, widgets};

pub fn draw(ui: &mut egui::Ui, app: &mut App) {
    let p = theme::p();

    if app.gpus.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(egui::RichText::new("No GPU detected").color(p.dim));
        });
        return;
    }

    let gpu_idx = app.selected_gpu.min(app.gpus.len() - 1);
    let gpu = app.gpus[gpu_idx].clone();

    egui::ScrollArea::vertical().show(ui, |ui| {
        widgets::section_header(ui, &gpu.name);

        // Utilization
        widgets::chart(ui, &gpu.utilization_history.data, theme::usage_color(gpu.utilization), 90.0, "Utilization", "%", Some(100.0));

        ui.add_space(8.0);

        // VRAM
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("VRAM").size(12.0).color(p.dim));
            ui.label(egui::RichText::new(format!(
                "{} / {} ({:.0}%)",
                widgets::format_bytes(gpu.vram_used),
                widgets::format_bytes(gpu.vram_total),
                gpu.vram_pct()
            )).size(12.0).color(theme::usage_color(gpu.vram_pct())));
        });
        widgets::progress_bar(ui, gpu.vram_pct() / 100.0, theme::usage_color(gpu.vram_pct()), 6.0);
        ui.add_space(2.0);
        widgets::chart(ui, &gpu.vram_history.data, p.accent, 50.0, "", "%", Some(100.0));

        ui.add_space(12.0);

        // Hardware + Thermals columns
        ui.columns(2, |cols| {
            widgets::section_header(&mut cols[0], "Hardware");

            if let Some(watts) = gpu.power_watts {
                let limit = gpu.power_limit.map(|l| format!(" / {:.0}W limit", l)).unwrap_or_default();
                widgets::copyable_value(&mut cols[0], "Power", &format!("{:.0}W", watts), &format!("{:.1}W{}", watts, limit), "Current board power draw", p.yellow);
                widgets::chart(&mut cols[0], &gpu.power_history.data, p.yellow, 40.0, "", "W", None);
                cols[0].add_space(4.0);
            }
            if let Some(core) = gpu.clock_core_mhz {
                widgets::copiable(&mut cols[0], "Core Clock", &format!("{} MHz", core), "Graphics clock frequency", p.text);
            }
            if let Some(mem) = gpu.clock_mem_mhz {
                widgets::copiable(&mut cols[0], "Mem Clock", &format!("{} MHz", mem), "Memory clock frequency", p.text);
            }

            widgets::section_header(&mut cols[1], "Thermals & IO");
            if let Some(temp) = gpu.temp_core {
                widgets::copiable(&mut cols[1], "Core Temp", &format!("{}C", temp), "GPU die temperature", theme::temp_color(temp));
                if let Some(hs) = gpu.temp_hotspot {
                    widgets::copiable(&mut cols[1], "Hotspot", &format!("{}C", hs), "Hottest point on die", theme::temp_color(hs));
                }
                widgets::chart(&mut cols[1], &gpu.temp_history.data, p.red, 40.0, "", "C", None);
                cols[1].add_space(4.0);
            }
            if let Some(fan) = gpu.fan_rpm {
                widgets::copiable(&mut cols[1], "Fan", &format!("{}%", fan), "Fan speed percentage", p.text);
            }
            if let (Some(tx), Some(rx)) = (gpu.pcie_tx_bytes_sec, gpu.pcie_rx_bytes_sec) {
                cols[1].add_space(4.0);
                widgets::copyable_value(&mut cols[1], "PCIe TX", &widgets::format_rate(tx), &format!("{} bytes/s", tx), "Host to device", p.accent);
                widgets::copyable_value(&mut cols[1], "PCIe RX", &widgets::format_rate(rx), &format!("{} bytes/s", rx), "Device to host", p.accent);
            }
        });

        // GPU Processes section
        ui.add_space(12.0);
        widgets::section_header(ui, &format!("GPU Processes ({})", gpu.processes.len()));

        if gpu.processes.is_empty() {
            ui.label(egui::RichText::new("No processes using this GPU").size(11.0).color(p.dim));
        } else {
            // Kill confirmation
            if let Some(pid) = app.confirm_kill {
                egui::Frame::NONE
                    .fill(p.red.gamma_multiply(0.1))
                    .corner_radius(egui::CornerRadius::same(6))
                    .stroke(egui::Stroke::new(1.0_f32, p.red))
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(format!("End PID {}?", pid)).color(p.red).strong());
                            if ui.button(egui::RichText::new("Close").color(p.yellow)).clicked() {
                                match collect::close_process(pid) {
                                    Ok(()) => app.status_msg = Some((format!("PID {} closed", pid), false)),
                                    Err(e) => app.status_msg = Some((e, true)),
                                }
                                app.confirm_kill = None;
                            }
                            if ui.button(egui::RichText::new("Force Kill").color(p.red).strong()).clicked() {
                                match collect::kill_process(pid) {
                                    Ok(()) => app.status_msg = Some((format!("PID {} killed", pid), false)),
                                    Err(e) => app.status_msg = Some((e, true)),
                                }
                                app.confirm_kill = None;
                            }
                            if ui.button("Cancel").clicked() {
                                app.confirm_kill = None;
                            }
                        });
                        if !collect::is_elevated() {
                            ui.label(egui::RichText::new("Not running as admin - some processes cannot be killed").size(10.0).color(p.dim));
                        }
                    });
                ui.add_space(4.0);
            }

            egui_extras::TableBuilder::new(ui)
                .striped(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(egui_extras::Column::exact(55.0))    // PID
                .column(egui_extras::Column::remainder())     // Name
                .column(egui_extras::Column::exact(55.0))    // Type
                .column(egui_extras::Column::exact(80.0))    // VRAM
                .column(egui_extras::Column::exact(70.0))    // Actions
                .header(22.0, |mut header| {
                    header.col(|ui| { ui.label(egui::RichText::new("PID").size(10.0).color(p.accent).strong()); });
                    header.col(|ui| { ui.label(egui::RichText::new("Name").size(10.0).color(p.accent).strong()); });
                    header.col(|ui| { ui.label(egui::RichText::new("Type").size(10.0).color(p.accent).strong()); });
                    header.col(|ui| { ui.label(egui::RichText::new("VRAM").size(10.0).color(p.accent).strong()); });
                    header.col(|_ui| {});
                })
                .body(|body| {
                    let procs = gpu.processes.clone();
                    body.rows(20.0, procs.len(), |mut row| {
                        let proc = &procs[row.index()];
                        let pid = proc.pid;

                        row.col(|ui| {
                            ui.label(egui::RichText::new(pid.to_string()).size(11.0).color(p.dim).monospace());
                        });
                        row.col(|ui| {
                            ui.label(egui::RichText::new(&proc.name).size(11.0));
                        });
                        row.col(|ui| {
                            let (label, color) = match proc.kind {
                                GpuProcessKind::Graphics => ("GFX", p.green),
                                GpuProcessKind::Compute => ("CMP", p.accent),
                            };
                            ui.label(egui::RichText::new(label).size(10.0).color(color).strong());
                        });
                        row.col(|ui| {
                            ui.label(egui::RichText::new(widgets::format_bytes(proc.used_gpu_memory)).size(11.0).color(p.accent));
                        });
                        row.col(|ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 2.0;
                                let search = ui.add(
                                    egui::Button::new(egui::RichText::new("?").size(9.0).color(p.accent))
                                        .fill(egui::Color32::TRANSPARENT)
                                        .corner_radius(egui::CornerRadius::same(8)),
                                );
                                if search.clicked() {
                                    let query = format!("what is {} Windows process", proc.name);
                                    let url = format!("https://www.google.com/search?q={}", query.replace(' ', "+"));
                                    let _ = open::that(&url);
                                }
                                search.on_hover_text(format!("Search: what is {}?", proc.name));

                                let close_btn = ui.add(
                                    egui::Button::new(egui::RichText::new("x").size(9.0).color(p.red))
                                        .fill(egui::Color32::TRANSPARENT)
                                        .corner_radius(egui::CornerRadius::same(8)),
                                );
                                if close_btn.clicked() {
                                    app.confirm_kill = Some(pid);
                                }
                                close_btn.on_hover_text(format!("End {}", proc.name));
                            });
                        });
                    });
                });
        }
    });
}

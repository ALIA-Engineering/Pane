//! Dashboard - everything at a glance.

use eframe::egui;
use crate::app::App;
use crate::gui::{theme, widgets};

pub fn draw(ui: &mut egui::Ui, app: &App) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        // GPU cards
        widgets::section_header(ui, "GPUs");
        for (i, gpu) in app.gpus.iter().enumerate() {
            egui::Frame::NONE
                .fill(theme::p().panel_bg)
                .corner_radius(egui::CornerRadius::same(6))
                .stroke(egui::Stroke::new(
                    if i == app.selected_gpu { 1.5_f32 } else { 1.0_f32 },
                    if i == app.selected_gpu { theme::p().accent } else { theme::p().border },
                ))
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());

                    ui.horizontal(|ui| {
                        let short = gpu.name.replace("NVIDIA GeForce ", "");
                        ui.label(egui::RichText::new(&short).size(14.0).color(theme::p().text).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if let Some(t) = gpu.temp_core {
                                ui.label(egui::RichText::new(format!("{}C", t)).size(12.0).color(theme::temp_color(t)));
                            }
                            if let Some(w) = gpu.power_watts {
                                ui.label(egui::RichText::new(format!("{:.0}W", w)).size(12.0).color(theme::p().dim));
                            }
                            ui.label(
                                egui::RichText::new(format!("{:.0}%", gpu.utilization))
                                    .size(18.0)
                                    .color(theme::usage_color(gpu.utilization))
                                    .strong(),
                            );
                        });
                    });

                    ui.add_space(4.0);
                    widgets::chart(ui, &gpu.utilization_history.data, theme::usage_color(gpu.utilization), 50.0, "", "%", Some(100.0));

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("VRAM").size(10.0).color(theme::p().dim));
                        ui.label(egui::RichText::new(format!(
                            "{} / {}",
                            widgets::format_bytes(gpu.vram_used),
                            widgets::format_bytes(gpu.vram_total),
                        )).size(10.0).color(theme::p().text));
                    });
                    widgets::progress_bar(ui, gpu.vram_pct() / 100.0, theme::usage_color(gpu.vram_pct()), 3.0);
                });
            ui.add_space(4.0);
        }

        ui.add_space(8.0);

        // CPU + RAM
        ui.columns(2, |cols| {
            widgets::section_header(&mut cols[0], "CPU");
            let cpu_name = if app.cpu.name.len() > 30 {
                format!("{}...", &app.cpu.name[..27])
            } else {
                app.cpu.name.clone()
            };
            widgets::stat_card(
                &mut cols[0],
                &cpu_name,
                &format!("{:.1}%", app.cpu.total_usage),
                Some(app.cpu.total_usage),
                theme::usage_color(app.cpu.total_usage),
            );
            cols[0].add_space(4.0);
            widgets::chart(&mut cols[0], &app.cpu.total_history.data, theme::usage_color(app.cpu.total_usage), 55.0, "", "%", Some(100.0));

            widgets::section_header(&mut cols[1], "Memory");
            let mem_pct = if app.memory.total_bytes > 0 {
                (app.memory.used_bytes as f64 / app.memory.total_bytes as f64) * 100.0
            } else { 0.0 };
            widgets::stat_card(
                &mut cols[1],
                &format!("{} / {}", widgets::format_bytes(app.memory.used_bytes), widgets::format_bytes(app.memory.total_bytes)),
                &format!("{:.1}%", mem_pct),
                Some(mem_pct),
                theme::usage_color(mem_pct),
            );
            cols[1].add_space(4.0);
            widgets::chart(&mut cols[1], &app.memory.usage_history.data, theme::usage_color(mem_pct), 55.0, "", "%", Some(100.0));
        });

        ui.add_space(8.0);

        // Disk + Network
        ui.columns(2, |cols| {
            widgets::section_header(&mut cols[0], "Disk");
            for d in app.disks.iter().take(4) {
                let pct = if d.total_bytes > 0 { (d.used_bytes as f64 / d.total_bytes as f64) * 100.0 } else { 0.0 };
                cols[0].horizontal(|ui| {
                    ui.label(egui::RichText::new(&d.mount).size(11.0).color(theme::p().text));
                    ui.label(egui::RichText::new(format!("{:.0}%", pct)).size(11.0).color(theme::usage_color(pct)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(format!("W:{}", widgets::format_rate(d.write_bytes_sec))).size(9.0).color(theme::p().magenta));
                        ui.label(egui::RichText::new(format!("R:{}", widgets::format_rate(d.read_bytes_sec))).size(9.0).color(theme::p().green));
                    });
                });
                widgets::progress_bar(&mut cols[0], pct / 100.0, theme::usage_color(pct), 2.0);
                cols[0].add_space(2.0);
            }

            widgets::section_header(&mut cols[1], "Network");
            let active_nets: Vec<_> = app.networks.iter()
                .filter(|n| n.rx_bytes_sec > 0 || n.tx_bytes_sec > 0 || n.total_rx > 1024)
                .take(4)
                .collect();

            if active_nets.is_empty() {
                cols[1].label(egui::RichText::new("No active interfaces").size(11.0).color(theme::p().dim));
            } else {
                for n in active_nets {
                    cols[1].horizontal(|ui| {
                        let short_name = if n.name.len() > 18 { &n.name[..18] } else { &n.name };
                        ui.label(egui::RichText::new(short_name).size(11.0).color(theme::p().text));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new(widgets::format_rate(n.tx_bytes_sec)).size(10.0).color(theme::p().magenta));
                            ui.label(egui::RichText::new("up").size(9.0).color(theme::p().dim));
                            ui.label(egui::RichText::new(widgets::format_rate(n.rx_bytes_sec)).size(10.0).color(theme::p().green));
                            ui.label(egui::RichText::new("dn").size(9.0).color(theme::p().dim));
                        });
                    });
                    cols[1].add_space(2.0);
                }
            }
        });

        ui.add_space(8.0);

        // Top processes
        widgets::section_header(ui, "Top Processes");
        let procs = app.sorted_processes();
        egui_extras::TableBuilder::new(ui)
            .striped(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(egui_extras::Column::exact(60.0))
            .column(egui_extras::Column::remainder())
            .column(egui_extras::Column::exact(60.0))
            .column(egui_extras::Column::exact(80.0))
            .header(20.0, |mut header| {
                header.col(|ui| { ui.label(egui::RichText::new("PID").size(10.0).color(theme::p().accent).strong()); });
                header.col(|ui| { ui.label(egui::RichText::new("Name").size(10.0).color(theme::p().accent).strong()); });
                header.col(|ui| { ui.label(egui::RichText::new("CPU%").size(10.0).color(theme::p().accent).strong()); });
                header.col(|ui| { ui.label(egui::RichText::new("Memory").size(10.0).color(theme::p().accent).strong()); });
            })
            .body(|body| {
                body.rows(18.0, procs.len().min(12), |mut row| {
                    let p = procs[row.index()];
                    row.col(|ui| { ui.label(egui::RichText::new(p.pid.to_string()).size(10.0).color(theme::p().dim).monospace()); });
                    row.col(|ui| { ui.label(egui::RichText::new(&p.name).size(11.0)); });
                    row.col(|ui| { ui.label(egui::RichText::new(format!("{:.1}", p.cpu_usage)).size(11.0).color(theme::usage_color(p.cpu_usage))); });
                    row.col(|ui| { ui.label(egui::RichText::new(widgets::format_bytes(p.memory_bytes)).size(10.0)); });
                });
            });
    });
}

//! Process table with sort, filter, search, and close/kill.

use eframe::egui::{self, Color32};
use crate::app::{App, SortColumn};
use crate::collect;
use crate::gui::{theme, widgets};

/// Open the default browser to search for a process name.
fn search_process(name: &str) {
    let query = format!("what is {} Windows process", name);
    let url = format!("https://www.google.com/search?q={}", urlenccode(&query));
    let _ = open::that(&url);
}

/// Minimal URL encoding for the search query.
fn urlenccode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                out.push_str(&format!("{:02X}", b));
            }
        }
    }
    out
}

pub fn draw(ui: &mut egui::Ui, app: &mut App) {
    let p = theme::p();

    // Filter bar
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Filter").size(12.0).color(p.dim));
        ui.add_sized(
            [200.0, 20.0],
            egui::TextEdit::singleline(&mut app.filter).hint_text("Search processes..."),
        );
        ui.label(egui::RichText::new(format!("{} total", app.processes.len())).size(11.0).color(p.dim));
    });

    ui.add_space(6.0);

    // Sort pills
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Sort by").size(11.0).color(p.dim));
        ui.add_space(4.0);

        let columns = [
            (SortColumn::Pid, "PID"),
            (SortColumn::Name, "Name"),
            (SortColumn::Cpu, "CPU"),
            (SortColumn::Memory, "Mem"),
            (SortColumn::GpuUtil, "GPU"),
            (SortColumn::GpuVram, "VRAM"),
        ];

        for (col, name) in columns {
            let is_active = app.sort_column == col;
            let label = if is_active {
                format!("{} {}", name, if app.sort_ascending { "^" } else { "v" })
            } else {
                name.to_string()
            };

            let btn = ui.add(
                egui::Button::new(
                    egui::RichText::new(&label)
                        .size(11.0)
                        .color(if is_active { p.accent } else { p.text })
                        .strong(),
                )
                .fill(if is_active { p.accent.gamma_multiply(0.15) } else { p.panel_bg })
                .stroke(egui::Stroke::new(
                    if is_active { 1.0_f32 } else { 0.5_f32 },
                    if is_active { p.accent } else { p.border },
                ))
                .corner_radius(egui::CornerRadius::same(12)),
            );

            if btn.clicked() {
                if app.sort_column == col {
                    app.sort_ascending = !app.sort_ascending;
                } else {
                    app.sort_column = col;
                    app.sort_ascending = false;
                }
            }
        }
    });

    ui.add_space(6.0);

    // Status message (success/error feedback)
    if let Some((msg, is_err)) = app.status_msg.clone() {
        let color = if is_err { p.red } else { p.green };
        egui::Frame::NONE
            .fill(color.gamma_multiply(0.1))
            .corner_radius(egui::CornerRadius::same(6))
            .stroke(egui::Stroke::new(1.0_f32, color))
            .inner_margin(egui::Margin::same(6))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&msg).size(12.0).color(color));
                    if ui.small_button("Dismiss").clicked() {
                        app.status_msg = None;
                    }
                });
            });
        ui.add_space(4.0);
    }

    // Kill confirmation
    if let Some(pid) = app.confirm_kill {
        let elevated = collect::is_elevated();
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
                            Ok(()) => app.status_msg = Some(( format!("PID {} closed", pid), false )),
                            Err(e) => app.status_msg = Some(( e, true )),
                        }
                        app.confirm_kill = None;
                    }
                    if ui.button(egui::RichText::new("Force Kill").color(p.red).strong()).clicked() {
                        match collect::kill_process(pid) {
                            Ok(()) => app.status_msg = Some(( format!("PID {} killed", pid), false )),
                            Err(e) => app.status_msg = Some(( e, true )),
                        }
                        app.confirm_kill = None;
                    }
                    if ui.button("Cancel").clicked() {
                        app.confirm_kill = None;
                    }
                });
                if !elevated {
                    ui.label(egui::RichText::new("Not running as admin - some processes cannot be killed").size(10.0).color(p.dim));
                }
            });
        ui.add_space(4.0);
    }

    // Process table
    let procs = app.sorted_processes();
    let procs_cloned: Vec<_> = procs.iter().map(|p| (*p).clone()).collect();

    egui_extras::TableBuilder::new(ui)
        .striped(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(egui_extras::Column::exact(55.0))    // PID
        .column(egui_extras::Column::remainder())     // Name
        .column(egui_extras::Column::exact(55.0))    // CPU
        .column(egui_extras::Column::exact(75.0))    // Memory
        .column(egui_extras::Column::exact(55.0))    // GPU%
        .column(egui_extras::Column::exact(75.0))    // VRAM
        .column(egui_extras::Column::exact(70.0))    // Actions
        .header(24.0, |mut header| {
            let hdr = |ui: &mut egui::Ui, text: &str| {
                ui.label(egui::RichText::new(text).size(11.0).color(p.accent).strong());
            };
            header.col(|ui| hdr(ui, "PID"));
            header.col(|ui| hdr(ui, "Name"));
            header.col(|ui| hdr(ui, "CPU%"));
            header.col(|ui| hdr(ui, "Memory"));
            header.col(|ui| hdr(ui, "GPU%"));
            header.col(|ui| hdr(ui, "VRAM"));
            header.col(|_ui| {});
        })
        .body(|body| {
            body.rows(20.0, procs_cloned.len(), |mut row| {
                let proc = &procs_cloned[row.index()];
                let pid = proc.pid;
                let name = proc.name.clone();

                row.col(|ui| {
                    ui.label(egui::RichText::new(pid.to_string()).size(11.0).color(p.dim).monospace());
                });
                row.col(|ui| {
                    ui.label(egui::RichText::new(&name).size(11.0));
                });
                row.col(|ui| {
                    ui.label(egui::RichText::new(format!("{:.1}", proc.cpu_usage)).size(11.0).color(theme::usage_color(proc.cpu_usage)));
                });
                row.col(|ui| {
                    ui.label(egui::RichText::new(widgets::format_bytes(proc.memory_bytes)).size(11.0));
                });
                row.col(|ui| {
                    let text = proc.gpu_util.map(|v| format!("{:.1}", v)).unwrap_or_else(|| "-".into());
                    ui.label(egui::RichText::new(text).size(11.0).color(p.accent));
                });
                row.col(|ui| {
                    let text = proc.gpu_vram.map(widgets::format_bytes).unwrap_or_else(|| "-".into());
                    ui.label(egui::RichText::new(text).size(11.0).color(p.accent));
                });
                row.col(|ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;

                        // Search button
                        let search = ui.add(
                            egui::Button::new(egui::RichText::new("?").size(10.0).color(p.accent))
                                .fill(Color32::TRANSPARENT)
                                .corner_radius(egui::CornerRadius::same(10)),
                        );
                        if search.clicked() {
                            search_process(&name);
                        }
                        search.on_hover_text(format!("Search: what is {}?", name));

                        // Kill button
                        let kill = ui.add(
                            egui::Button::new(egui::RichText::new("x").size(10.0).color(p.red))
                                .fill(Color32::TRANSPARENT)
                                .corner_radius(egui::CornerRadius::same(10)),
                        );
                        if kill.clicked() {
                            app.confirm_kill = Some(pid);
                        }
                        kill.on_hover_text(format!("End {}", name));
                    });
                });
            });
        });
}

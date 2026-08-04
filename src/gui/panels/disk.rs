//! Disk detail panel.

use eframe::egui;
use crate::app::App;
use crate::gui::{theme, widgets};

pub fn draw(ui: &mut egui::Ui, app: &App) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        widgets::section_header(ui, "Disks");

        for d in &app.disks {
            let pct = if d.total_bytes > 0 { (d.used_bytes as f64 / d.total_bytes as f64) * 100.0 } else { 0.0 };

            egui::Frame::NONE
                .fill(theme::p().panel_bg)
                .corner_radius(egui::CornerRadius::same(6))
                .stroke(egui::Stroke::new(1.0_f32, theme::p().border))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("{} ({})", d.name, d.mount)).size(13.0).color(theme::p().text).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new(format!("{:.0}%", pct)).color(theme::usage_color(pct)));
                            ui.label(egui::RichText::new(format!("{} / {}", widgets::format_bytes(d.used_bytes), widgets::format_bytes(d.total_bytes))).size(11.0).color(theme::p().dim));
                        });
                    });
                    widgets::progress_bar(ui, pct / 100.0, theme::usage_color(pct), 4.0);
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("Read: {}", widgets::format_rate(d.read_bytes_sec))).size(11.0).color(theme::p().green));
                        ui.label(egui::RichText::new(format!("Write: {}", widgets::format_rate(d.write_bytes_sec))).size(11.0).color(theme::p().magenta));
                    });
                });
            ui.add_space(4.0);
        }
    });
}

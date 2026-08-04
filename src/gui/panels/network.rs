//! Network detail panel.

use eframe::egui;
use crate::app::App;
use crate::gui::{theme, widgets};

pub fn draw(ui: &mut egui::Ui, app: &App) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        widgets::section_header(ui, "Network Interfaces");

        for n in &app.networks {
            egui::Frame::NONE
                .fill(theme::p().panel_bg)
                .corner_radius(egui::CornerRadius::same(6))
                .stroke(egui::Stroke::new(1.0_f32, theme::p().border))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(egui::RichText::new(&n.name).size(13.0).color(theme::p().text).strong());
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("\u{2193} {}", widgets::format_rate(n.rx_bytes_sec))).color(theme::p().green));
                        ui.label(egui::RichText::new(format!("\u{2191} {}", widgets::format_rate(n.tx_bytes_sec))).color(theme::p().magenta));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new(format!("Total: \u{2193}{} \u{2191}{}",
                                widgets::format_bytes(n.total_rx), widgets::format_bytes(n.total_tx)
                            )).size(10.0).color(theme::p().dim));
                        });
                    });
                });
            ui.add_space(4.0);
        }
    });
}

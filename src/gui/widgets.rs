//! Reusable GUI widgets for Pane.

use eframe::egui::{self, Color32, CornerRadius, Rect, Sense, Stroke, Vec2};
use super::theme;

// ── Stat card ──────────────────────────────────────────────────────────────

pub fn stat_card(ui: &mut egui::Ui, label: &str, value: &str, pct: Option<f64>, color: Color32) {
    let p = theme::p();
    egui::Frame::NONE
        .fill(p.card_bg)
        .corner_radius(CornerRadius::same(8))
        .stroke(Stroke::new(1.0_f32, p.border))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(egui::RichText::new(label).size(11.0).color(p.dim));
            ui.label(egui::RichText::new(value).size(26.0).color(color).strong().monospace());
            if let Some(pct) = pct {
                progress_bar(ui, pct / 100.0, color, 4.0);
            }
        });
}

// ── Progress bar ───────────────────────────────────────────────────────────

pub fn progress_bar(ui: &mut egui::Ui, ratio: f64, color: Color32, height: f32) {
    let p = theme::p();
    let available = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(available, height), Sense::hover());
    let cr = CornerRadius::same((height / 2.0) as u8);

    let painter = ui.painter();
    painter.rect_filled(rect, cr, p.bar_bg);
    let fill_w = (rect.width() * ratio.clamp(0.0, 1.0) as f32).max(1.0);
    let fill_rect = Rect::from_min_size(rect.min, Vec2::new(fill_w, height));
    painter.rect_filled(fill_rect, cr, color);
}

// ── Chart ──────────────────────────────────────────────────────────────────

pub fn chart(ui: &mut egui::Ui, data: &[f64], color: Color32, height: f32, label: &str, unit: &str, max_override: Option<f64>) {
    let p = theme::p();
    let available = ui.available_width();

    if !label.is_empty() {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(label).size(11.0).color(p.dim));
            if let Some(last) = data.last() {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(format!("{:.1}{}", last, unit)).size(11.0).color(color).strong());
                });
            }
        });
    }

    let (rect, _) = ui.allocate_exact_size(Vec2::new(available, height), Sense::hover());
    if data.is_empty() { return; }

    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(4), p.chart_bg);

    let max = max_override.unwrap_or_else(|| data.iter().cloned().fold(f64::NEG_INFINITY, f64::max).max(1.0));
    let h = rect.height();
    let w = rect.width();

    // Grid lines
    for frac in [0.25, 0.5, 0.75] {
        let y = rect.bottom() - h * frac as f32;
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            Stroke::new(0.5_f32, p.chart_grid),
        );
    }

    let start_idx = if data.len() as f32 > w { data.len() - w as usize } else { 0 };
    let visible = &data[start_idx..];
    let step = if visible.len() > 1 { w / (visible.len() - 1) as f32 } else { 0.0 };

    let mut line_points = Vec::with_capacity(visible.len());
    let mut fill_points = Vec::with_capacity(visible.len() + 2);

    for (i, &val) in visible.iter().enumerate() {
        let x = rect.left() + i as f32 * step;
        let y = rect.bottom() - (val / max).clamp(0.0, 1.0) as f32 * h;
        line_points.push(egui::pos2(x, y));
        fill_points.push(egui::pos2(x, y));
    }

    if let (Some(first), Some(last)) = (fill_points.first(), fill_points.last()) {
        let mut polygon = fill_points.clone();
        polygon.push(egui::pos2(last.x, rect.bottom()));
        polygon.push(egui::pos2(first.x, rect.bottom()));
        let fill_color = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 25);
        painter.add(egui::Shape::convex_polygon(polygon, fill_color, Stroke::NONE));
    }

    if line_points.len() >= 2 {
        painter.add(egui::Shape::line(line_points, Stroke::new(1.5_f32, color)));
    }

    // Y-axis labels
    painter.text(
        egui::pos2(rect.left() + 4.0, rect.top() + 2.0),
        egui::Align2::LEFT_TOP,
        format!("{:.0}{}", max, unit),
        egui::FontId::new(10.0, egui::FontFamily::Monospace),
        p.dim,
    );
    painter.text(
        egui::pos2(rect.left() + 4.0, rect.bottom() - 11.0),
        egui::Align2::LEFT_TOP,
        format!("0{}", unit),
        egui::FontId::new(10.0, egui::FontFamily::Monospace),
        p.dim,
    );
}

// ── Section header ─────────────────────────────────────────────────────────

pub fn section_header(ui: &mut egui::Ui, title: &str) {
    let p = theme::p();
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        let rect = ui.allocate_exact_size(Vec2::new(3.0, 18.0), Sense::hover()).0;
        ui.painter().rect_filled(rect, CornerRadius::same(1), p.accent);
        ui.label(egui::RichText::new(title).size(15.0).color(p.text).strong());
    });
    ui.add_space(6.0);
}

// ── Copyable values ────────────────────────────────────────────────────────

pub fn copyable_value(ui: &mut egui::Ui, label: &str, display: &str, copy_text: &str, tooltip: &str, color: Color32) {
    let p = theme::p();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).size(11.0).color(p.dim));
        let response = ui.add(
            egui::Label::new(egui::RichText::new(display).size(13.0).color(color))
                .sense(Sense::click()),
        );
        if response.clicked() {
            ui.ctx().copy_text(copy_text.to_string());
        }
        let hover = if response.clicked() {
            "Copied!".to_string()
        } else if tooltip.is_empty() {
            format!("{}\nClick to copy", copy_text)
        } else {
            format!("{}\n{}\nClick to copy", copy_text, tooltip)
        };
        response.on_hover_text(hover);
    });
}

pub fn copiable(ui: &mut egui::Ui, label: &str, value: &str, tooltip: &str, color: Color32) {
    copyable_value(ui, label, value, value, tooltip, color);
}

// ── Formatters ─────────────────────────────────────────────────────────────

pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;
    if bytes >= TB { format!("{:.1} TB", bytes as f64 / TB as f64) }
    else if bytes >= GB { format!("{:.1} GB", bytes as f64 / GB as f64) }
    else if bytes >= MB { format!("{:.1} MB", bytes as f64 / MB as f64) }
    else if bytes >= KB { format!("{:.1} KB", bytes as f64 / KB as f64) }
    else { format!("{} B", bytes) }
}

pub fn format_rate(bytes_sec: u64) -> String {
    format!("{}/s", format_bytes(bytes_sec))
}

#[cfg(test)]
mod tests {
    use super::{format_bytes, format_rate};

    #[test]
    fn formats_binary_units() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(format_bytes(32 * 1024 * 1024 * 1024), "32.0 GB");
        assert_eq!(format_bytes(1024u64.pow(4)), "1.0 TB");
    }

    #[test]
    fn boundaries_pick_the_larger_unit() {
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1024 * 1024 - 1), "1024.0 KB");
    }

    #[test]
    fn rate_appends_per_second() {
        assert_eq!(format_rate(1024), "1.0 KB/s");
        assert_eq!(format_rate(0), "0 B/s");
    }
}

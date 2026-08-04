//! Pane theme - dark, light, and system mode.

use std::sync::atomic::{AtomicU8, Ordering};
use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, Visuals};

static ACTIVE_MODE: AtomicU8 = AtomicU8::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Dark,
    Light,
    System,
}

impl ThemeMode {
    pub fn label(self) -> &'static str {
        match self { ThemeMode::Dark => "Dark", ThemeMode::Light => "Light", ThemeMode::System => "System" }
    }
    pub fn next(self) -> Self {
        match self { ThemeMode::Dark => ThemeMode::Light, ThemeMode::Light => ThemeMode::System, ThemeMode::System => ThemeMode::Dark }
    }
    pub fn resolved(self) -> Self {
        match self { ThemeMode::System => ThemeMode::Dark, other => other }
    }
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub accent: Color32,
    #[allow(dead_code)]
    pub accent_dim: Color32,
    pub green: Color32,
    pub yellow: Color32,
    pub red: Color32,
    pub magenta: Color32,
    pub text: Color32,
    pub dim: Color32,
    pub bg: Color32,
    pub panel_bg: Color32,
    pub card_bg: Color32,
    pub border: Color32,
    pub chart_bg: Color32,
    pub chart_grid: Color32,
    pub bar_bg: Color32,
    pub hover_bg: Color32,
    pub active_bg: Color32,
}

// Refined dark palette - deeper blacks, richer accent, better contrast
static DARK_PAL: Palette = Palette {
    accent:     Color32::from_rgb(56, 189, 248),   // sky blue - more modern than pure cyan
    accent_dim: Color32::from_rgb(30, 100, 140),
    green:      Color32::from_rgb(74, 222, 128),    // emerald
    yellow:     Color32::from_rgb(250, 204, 21),    // amber
    red:        Color32::from_rgb(248, 113, 113),   // rose
    magenta:    Color32::from_rgb(192, 132, 252),   // violet
    text:       Color32::from_rgb(240, 240, 245),
    dim:        Color32::from_rgb(115, 115, 130),
    bg:         Color32::from_rgb(13, 13, 18),      // near black
    panel_bg:   Color32::from_rgb(18, 18, 26),
    card_bg:    Color32::from_rgb(24, 24, 36),
    border:     Color32::from_rgb(40, 40, 56),
    chart_bg:   Color32::from_rgb(16, 16, 24),
    chart_grid: Color32::from_rgb(32, 32, 46),
    bar_bg:     Color32::from_rgb(32, 32, 46),
    hover_bg:   Color32::from_rgb(36, 36, 52),
    active_bg:  Color32::from_rgb(44, 44, 62),
};

static LIGHT_PAL: Palette = Palette {
    accent:     Color32::from_rgb(2, 132, 199),     // deeper sky
    accent_dim: Color32::from_rgb(120, 180, 210),
    green:      Color32::from_rgb(22, 163, 74),
    yellow:     Color32::from_rgb(202, 138, 4),
    red:        Color32::from_rgb(220, 38, 38),
    magenta:    Color32::from_rgb(147, 51, 234),
    text:       Color32::from_rgb(15, 15, 25),
    dim:        Color32::from_rgb(100, 100, 120),
    bg:         Color32::from_rgb(248, 249, 252),
    panel_bg:   Color32::from_rgb(255, 255, 255),
    card_bg:    Color32::from_rgb(243, 244, 248),
    border:     Color32::from_rgb(220, 222, 230),
    chart_bg:   Color32::from_rgb(240, 241, 246),
    chart_grid: Color32::from_rgb(225, 226, 234),
    bar_bg:     Color32::from_rgb(228, 230, 238),
    hover_bg:   Color32::from_rgb(235, 236, 242),
    active_bg:  Color32::from_rgb(225, 226, 234),
};

pub fn p() -> &'static Palette {
    if ACTIVE_MODE.load(Ordering::Relaxed) == 1 { &LIGHT_PAL } else { &DARK_PAL }
}

pub fn usage_color(pct: f64) -> Color32 {
    let p = p();
    if pct > 90.0 { p.red } else if pct > 70.0 { p.yellow } else { p.green }
}

pub fn temp_color(temp: u32) -> Color32 {
    let p = p();
    if temp > 80 { p.red } else if temp > 65 { p.yellow } else { p.green }
}

pub fn apply(ctx: &egui::Context, mode: ThemeMode) {
    let is_light = mode.resolved() == ThemeMode::Light;
    ACTIVE_MODE.store(if is_light { 1 } else { 0 }, Ordering::Relaxed);

    let pal = p();
    let mut visuals = if is_light { Visuals::light() } else { Visuals::dark() };

    visuals.override_text_color = Some(pal.text);
    visuals.panel_fill = pal.bg;
    visuals.window_fill = pal.bg;
    visuals.extreme_bg_color = pal.card_bg;
    visuals.faint_bg_color = pal.card_bg;

    let cr = CornerRadius::same(6);

    visuals.widgets.noninteractive.bg_fill = pal.panel_bg;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, pal.dim);
    visuals.widgets.noninteractive.corner_radius = cr;

    visuals.widgets.inactive.bg_fill = pal.card_bg;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, pal.text);
    visuals.widgets.inactive.corner_radius = cr;

    visuals.widgets.hovered.bg_fill = pal.hover_bg;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.5_f32, pal.accent);
    visuals.widgets.hovered.corner_radius = cr;

    visuals.widgets.active.bg_fill = pal.active_bg;
    visuals.widgets.active.fg_stroke = Stroke::new(1.5_f32, pal.accent);
    visuals.widgets.active.corner_radius = cr;

    visuals.selection.bg_fill = pal.accent.gamma_multiply(0.12);
    visuals.selection.stroke = Stroke::new(1.0_f32, pal.accent);

    visuals.window_corner_radius = CornerRadius::same(8);
    visuals.window_stroke = Stroke::new(1.0_f32, pal.border);
    visuals.striped = true;

    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.text_styles.insert(egui::TextStyle::Body, FontId::new(14.0, FontFamily::Proportional));
    style.text_styles.insert(egui::TextStyle::Small, FontId::new(11.5, FontFamily::Proportional));
    style.text_styles.insert(egui::TextStyle::Heading, FontId::new(20.0, FontFamily::Proportional));
    style.text_styles.insert(egui::TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace));
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(12.0, 5.0);
    ctx.set_style(style);
}

//! GPU Control Panel - adjust fan speed, power limit, clock offsets.
//!
//! Arrow keys to navigate rows, Left/Right to adjust values.
//! Only the power limit is applied, via NVML. Fan and clock rows are display
//! only - Pane has no NVAPI/ADL integration.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, ControlRow};
use super::{ACCENT, DIM, BORDER_STYLE};

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    if app.gpus.is_empty() {
        let msg = Paragraph::new("No GPU detected.")
            .style(Style::default().fg(DIM))
            .block(Block::default().borders(Borders::ALL).title(" GPU Control ").border_style(BORDER_STYLE));
        f.render_widget(msg, area);
        return;
    }

    let gpu = &app.gpus[app.selected_gpu.min(app.gpus.len() - 1)];
    let ctrl = app.gpu_controls.get(app.selected_gpu);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // GPU name header
            Constraint::Length(12), // Control sliders
            Constraint::Min(0),    // Live stats
        ])
        .split(area);

    // Header with GPU name and current status
    let header = Paragraph::new(Line::from(vec![
        Span::styled(format!(" {} ", gpu.name), Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Span::styled(
            format!("  {:.0}%  {}C  {:.0}W",
                gpu.utilization,
                gpu.temp_core.unwrap_or(0),
                gpu.power_watts.unwrap_or(0.0),
            ),
            Style::default().fg(Color::White),
        ),
    ]))
    .block(Block::default().borders(Borders::ALL).border_style(BORDER_STYLE));
    f.render_widget(header, chunks[0]);

    // Control sliders
    let row_style = |row: ControlRow| -> Style {
        if row == app.control_row {
            Style::default().fg(Color::White).bg(Color::DarkGray).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        }
    };
    let arrow = |row: ControlRow| -> &'static str {
        if row == app.control_row { " > " } else { "   " }
    };

    let fan_val = ctrl.map(|c| {
        if c.fan_auto { "Auto".to_string() } else {
            format!("{}%", c.fan_speed_pct.unwrap_or(0))
        }
    }).unwrap_or_else(|| "Auto".to_string());

    let power_current = gpu.power_limit.unwrap_or(0.0);
    let power_val = ctrl
        .and_then(|c| c.power_limit_watts)
        .map(|w| format!("{:.0}W", w))
        .unwrap_or_else(|| format!("{:.0}W (default)", power_current));

    let core_offset = ctrl.map(|c| c.clock_offset_mhz).unwrap_or(0);
    let mem_offset = ctrl.map(|c| c.mem_offset_mhz).unwrap_or(0);

    let slider_width = 30usize;

    let fan_pct = ctrl.and_then(|c| c.fan_speed_pct).unwrap_or(50) as f64;
    let fan_bar = make_slider(fan_pct, 100.0, slider_width);

    let power_pct = ctrl.and_then(|c| c.power_limit_watts).unwrap_or(power_current);
    let power_max = gpu.power_limit.unwrap_or(450.0) * 1.15; // ~115% for OC headroom
    let power_bar = make_slider(power_pct, power_max, slider_width);

    let core_bar = make_offset_slider(core_offset, 500, slider_width);
    let mem_bar = make_offset_slider(mem_offset, 2000, slider_width);

    let lines = vec![
        Line::from(vec![
            Span::styled(arrow(ControlRow::FanSpeed), Style::default().fg(ACCENT)),
            Span::styled("Fan Speed   ", row_style(ControlRow::FanSpeed)),
            Span::styled(format!(" {} ", fan_bar), Style::default().fg(Color::Cyan)),
            Span::styled(format!(" {}", fan_val), row_style(ControlRow::FanSpeed)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(arrow(ControlRow::PowerLimit), Style::default().fg(ACCENT)),
            Span::styled("Power Limit ", row_style(ControlRow::PowerLimit)),
            Span::styled(format!(" {} ", power_bar), Style::default().fg(Color::Yellow)),
            Span::styled(format!(" {}", power_val), row_style(ControlRow::PowerLimit)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(arrow(ControlRow::CoreClock), Style::default().fg(ACCENT)),
            Span::styled("Core Offset ", row_style(ControlRow::CoreClock)),
            Span::styled(format!(" {} ", core_bar), Style::default().fg(Color::Green)),
            Span::styled(format!(" {:+} MHz", core_offset), row_style(ControlRow::CoreClock)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(arrow(ControlRow::MemClock), Style::default().fg(ACCENT)),
            Span::styled("Mem Offset  ", row_style(ControlRow::MemClock)),
            Span::styled(format!(" {} ", mem_bar), Style::default().fg(Color::Magenta)),
            Span::styled(format!(" {:+} MHz", mem_offset), row_style(ControlRow::MemClock)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("   \u{2190}/\u{2192}: adjust   \u{2191}/\u{2193}: select   Enter: apply   Esc: reset", Style::default().fg(DIM)),
        ]),
    ];

    let controls = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Controls ")
                .title_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))
                .border_style(BORDER_STYLE),
        );
    f.render_widget(controls, chunks[1]);

    // Live stats at bottom
    let mut stats = vec![
        Line::from(vec![
            Span::styled("Current Clocks  ", Style::default().fg(DIM)),
            Span::styled(
                format!("Core: {} MHz  Mem: {} MHz",
                    gpu.clock_core_mhz.unwrap_or(0),
                    gpu.clock_mem_mhz.unwrap_or(0)),
                Style::default().fg(Color::White),
            ),
        ]),
    ];
    if let Some(temp) = gpu.temp_core {
        let hotspot = gpu.temp_hotspot.map(|h| format!("  Hotspot: {}C", h)).unwrap_or_default();
        stats.push(Line::from(vec![
            Span::styled("Temperature     ", Style::default().fg(DIM)),
            Span::styled(format!("Core: {}C{}", temp, hotspot), Style::default().fg(
                if temp > 80 { Color::Red } else if temp > 65 { Color::Yellow } else { Color::Green }
            )),
        ]));
    }
    if let Some(fan) = gpu.fan_rpm {
        stats.push(Line::from(vec![
            Span::styled("Fan             ", Style::default().fg(DIM)),
            Span::styled(format!("{}%", fan), Style::default().fg(Color::White)),
        ]));
    }

    let stats_widget = Paragraph::new(stats)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Live Stats ")
                .title_style(Style::default().fg(ACCENT))
                .border_style(BORDER_STYLE),
        );
    f.render_widget(stats_widget, chunks[2]);
}

/// Render a 0-max slider bar using block characters.
fn make_slider(value: f64, max: f64, width: usize) -> String {
    let ratio = (value / max).clamp(0.0, 1.0);
    let filled = (ratio * width as f64) as usize;
    format!("{}{}", "\u{2588}".repeat(filled), "\u{2591}".repeat(width - filled))
}

/// Render a centered offset slider (-max to +max) with 0 in the middle.
fn make_offset_slider(value: i32, max: i32, width: usize) -> String {
    let center = width / 2;
    let ratio = (value as f64 / max as f64).clamp(-1.0, 1.0);
    let offset = (ratio * center as f64) as i32;

    let mut bar: Vec<char> = vec!['\u{2591}'; width];
    bar[center] = '|'; // center marker

    if offset >= 0 {
        for cell in bar.iter_mut().take((center + offset as usize).min(width)).skip(center) {
            *cell = '\u{2588}';
        }
    } else {
        for cell in bar.iter_mut().take(center).skip((center as i32 + offset).max(0) as usize) {
            *cell = '\u{2588}';
        }
    }

    bar.into_iter().collect()
}

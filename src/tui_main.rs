//! Pane - a transparent window into your system.
//!
//! Entry point: sets up terminal, runs the event loop, cleans up on exit.

mod app;
mod collect;
mod metrics;
mod ui;

use std::io;
use std::time::Duration;

use color_eyre::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::{App, GpuControl, Panel};
use metrics::gpu;
use metrics::gpu::pdh::PdhGpuCollector;
use metrics::system::SystemCollector;

fn main() -> Result<()> {
    color_eyre::install()?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let tick_rate = Duration::from_millis(250);
    let mut app = App::new(tick_rate);
    let mut sys = SystemCollector::new();
    let mut gpu_backend = gpu::create_backend();
    let mut pdh = PdhGpuCollector::new();

    collect::collect_metrics(&mut app, &mut sys, &mut gpu_backend, &mut pdh);

    let result = run_loop(&mut terminal, &mut app, &mut sys, &mut gpu_backend, &mut pdh);

    // Cleanup - always restore terminal state
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    sys: &mut SystemCollector,
    gpu_backend: &mut Box<dyn gpu::GpuBackend>,
    pdh: &mut PdhGpuCollector,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        let timeout = app
            .tick_rate
            .checked_sub(app.last_tick.elapsed())
            .unwrap_or(Duration::ZERO);

        if event::poll(timeout)?
            && let Event::Key(key) = event::read()?
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            handle_input(app, key.code);
        }

        if app.last_tick.elapsed() >= app.tick_rate {
            collect::collect_metrics(app, sys, gpu_backend, pdh);
            app.last_tick = std::time::Instant::now();
        }

        if !app.running {
            return Ok(());
        }
    }
}

fn handle_input(app: &mut App, key: KeyCode) {
    // Kill confirmation mode
    if app.confirm_kill.is_some() {
        match key {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(pid) = app.confirm_kill.take()
                    && let Err(e) = collect::kill_process(pid)
                {
                    app.status_msg = Some((e, true));
                }
            }
            _ => { app.confirm_kill = None; }
        }
        return;
    }

    // Filter mode
    if app.filtering {
        match key {
            KeyCode::Esc => {
                app.filtering = false;
                app.filter.clear();
            }
            KeyCode::Enter => { app.filtering = false; }
            KeyCode::Backspace => { app.filter.pop(); }
            KeyCode::Char(c) => { app.filter.push(c); }
            _ => {}
        }
        return;
    }

    // GPU Control panel has its own keybindings
    if app.active_panel == Panel::GpuControl {
        match key {
            KeyCode::Char('q') | KeyCode::Esc => app.quit(),
            KeyCode::Tab => app.next_panel(),
            KeyCode::Up => { app.control_row = app.control_row.prev(); }
            KeyCode::Down => { app.control_row = app.control_row.next(); }
            KeyCode::Left => adjust_gpu_control(app, -1),
            KeyCode::Right => adjust_gpu_control(app, 1),
            // Panel jumps
            KeyCode::Char('h') => app.active_panel = Panel::Dashboard,
            KeyCode::Char('g') => app.active_panel = Panel::Gpu,
            KeyCode::Char('c') => app.active_panel = Panel::Cpu,
            KeyCode::Char('m') => app.active_panel = Panel::Memory,
            KeyCode::Char('d') => app.active_panel = Panel::Disk,
            KeyCode::Char('n') => app.active_panel = Panel::Network,
            KeyCode::Char('p') => app.active_panel = Panel::Processes,
            KeyCode::Char('x') => app.active_panel = Panel::GpuControl,
            _ => {}
        }
        return;
    }

    // Normal mode
    match key {
        KeyCode::Char('q') | KeyCode::Esc => app.quit(),
        KeyCode::Tab => app.next_panel(),

        // Panel jumps
        KeyCode::Char('h') => app.active_panel = Panel::Dashboard,
        KeyCode::Char('g') => app.active_panel = Panel::Gpu,
        KeyCode::Char('c') => app.active_panel = Panel::Cpu,
        KeyCode::Char('m') => app.active_panel = Panel::Memory,
        KeyCode::Char('d') => app.active_panel = Panel::Disk,
        KeyCode::Char('n') => app.active_panel = Panel::Network,
        KeyCode::Char('p') => app.active_panel = Panel::Processes,
        KeyCode::Char('x') => app.active_panel = Panel::GpuControl,

        // Process controls
        KeyCode::Char('s') => app.cycle_sort(),
        KeyCode::Char('/') => {
            app.filtering = true;
            app.filter.clear();
        }
        KeyCode::Char('k') => {
            if app.active_panel == Panel::Processes {
                let procs = app.sorted_processes();
                if let Some(p) = procs.get(app.process_selected) {
                    app.confirm_kill = Some(p.pid);
                }
            }
        }

        // Scrolling
        KeyCode::Up => {
            app.process_selected = app.process_selected.saturating_sub(1);
            if app.process_selected < app.process_scroll {
                app.process_scroll = app.process_selected;
            }
        }
        KeyCode::Down => {
            let max = app.sorted_processes().len().saturating_sub(1);
            app.process_selected = (app.process_selected + 1).min(max);
        }

        // GPU select
        KeyCode::Char('1') => app.selected_gpu = 0,
        KeyCode::Char('2') if app.gpus.len() > 1 => app.selected_gpu = 1,
        _ => {}
    }
}

/// Adjust GPU control values - direction is -1 (left) or +1 (right).
fn adjust_gpu_control(app: &mut App, direction: i32) {
    // Ensure we have a control entry for this GPU
    while app.gpu_controls.len() <= app.selected_gpu {
        app.gpu_controls.push(GpuControl::new());
    }

    let ctrl = &mut app.gpu_controls[app.selected_gpu];

    match app.control_row {
        app::ControlRow::FanSpeed => {
            if ctrl.fan_auto && direction > 0 {
                ctrl.fan_auto = false;
                ctrl.fan_speed_pct = Some(50);
            } else if !ctrl.fan_auto {
                let current = ctrl.fan_speed_pct.unwrap_or(50) as i32;
                let new_val = (current + direction * 5).clamp(0, 100);
                if new_val == 0 && direction < 0 {
                    ctrl.fan_auto = true;
                    ctrl.fan_speed_pct = None;
                } else {
                    ctrl.fan_speed_pct = Some(new_val as u32);
                }
            }
        }
        app::ControlRow::PowerLimit => {
            let gpu = app.gpus.get(app.selected_gpu);
            let default_limit = gpu.and_then(|g| g.power_limit).unwrap_or(300.0);
            let current = ctrl.power_limit_watts.unwrap_or(default_limit);
            let new_val = (current + direction as f64 * 10.0).clamp(100.0, default_limit * 1.15);
            ctrl.power_limit_watts = Some(new_val);
        }
        app::ControlRow::CoreClock => {
            ctrl.clock_offset_mhz = (ctrl.clock_offset_mhz + direction * 15).clamp(-500, 500);
        }
        app::ControlRow::MemClock => {
            ctrl.mem_offset_mhz = (ctrl.mem_offset_mhz + direction * 50).clamp(-2000, 2000);
        }
    }
}


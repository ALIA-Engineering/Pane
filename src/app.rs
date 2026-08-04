//! Application state for Pane.
//!
//! All data flows through `App` - metric collectors write into it,
//! the UI reads from it. No direct coupling between collectors and renderers.

#![allow(dead_code)]

use std::time::{Duration, Instant};

/// UI panels - Dashboard is the default overview, others are detail views.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Dashboard,
    Gpu,
    Cpu,
    Memory,
    Disk,
    Network,
    Processes,
    GpuControl,
    VramCalc,
    Snapshot,
}

impl Panel {
    pub fn next(self) -> Self {
        match self {
            Panel::Dashboard => Panel::Gpu,
            Panel::Gpu => Panel::Cpu,
            Panel::Cpu => Panel::Memory,
            Panel::Memory => Panel::Disk,
            Panel::Disk => Panel::Network,
            Panel::Network => Panel::Processes,
            Panel::Processes => Panel::GpuControl,
            Panel::GpuControl => Panel::VramCalc,
            Panel::VramCalc => Panel::Snapshot,
            Panel::Snapshot => Panel::Dashboard,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Panel::Dashboard => "Dashboard",
            Panel::Gpu => "GPU",
            Panel::Cpu => "CPU",
            Panel::Memory => "Memory",
            Panel::Disk => "Disk",
            Panel::Network => "Network",
            Panel::Processes => "Processes",
            Panel::GpuControl => "GPU Ctrl",
            Panel::VramCalc => "VRAM Calc",
            Panel::Snapshot => "Snapshot",
        }
    }
}

const HISTORY_LEN: usize = 200;

/// Ring buffer for sparkline/graph history. Stores the last N samples.
#[derive(Debug, Clone)]
pub struct History {
    pub data: Vec<f64>,
    capacity: usize,
}

impl History {
    pub fn new() -> Self {
        Self {
            data: Vec::with_capacity(HISTORY_LEN),
            capacity: HISTORY_LEN,
        }
    }

    pub fn push(&mut self, value: f64) {
        if self.data.len() >= self.capacity {
            self.data.remove(0);
        }
        self.data.push(value);
    }

    /// Get the latest value, or 0.0 if empty.
    pub fn last(&self) -> f64 {
        self.data.last().copied().unwrap_or(0.0)
    }
}

#[derive(Debug, Clone)]
pub struct CpuCore {
    pub usage: f64,
    pub freq_mhz: u64,
    pub history: History,
}

#[derive(Debug, Clone)]
pub struct CpuMetrics {
    pub total_usage: f64,
    pub total_history: History,
    pub cores: Vec<CpuCore>,
    pub name: String,
    pub physical_cores: usize,
    pub logical_cores: usize,
}

#[derive(Debug, Clone)]
pub struct MemMetrics {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub swap_total: u64,
    pub swap_used: u64,
    pub usage_history: History,
}

#[derive(Debug, Clone)]
pub struct DiskInfo {
    pub name: String,
    pub mount: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub read_bytes_sec: u64,
    pub write_bytes_sec: u64,
}

#[derive(Debug, Clone)]
pub struct NetInterface {
    pub name: String,
    pub rx_bytes_sec: u64,
    pub tx_bytes_sec: u64,
    pub total_rx: u64,
    pub total_tx: u64,
}

#[derive(Debug, Clone)]
pub struct GpuMetrics {
    pub name: String,
    pub utilization: f64,
    pub utilization_history: History,
    pub vram_used: u64,
    pub vram_total: u64,
    pub vram_history: History,
    pub temp_core: Option<u32>,
    pub temp_hotspot: Option<u32>,
    pub temp_vram: Option<u32>,
    pub temp_history: History,
    pub power_watts: Option<f64>,
    pub power_limit: Option<f64>,
    pub power_history: History,
    pub fan_rpm: Option<u32>,
    pub clock_core_mhz: Option<u32>,
    pub clock_mem_mhz: Option<u32>,
    pub pcie_tx_bytes_sec: Option<u64>,
    pub pcie_rx_bytes_sec: Option<u64>,
    pub processes: Vec<GpuProcessInfo>,
}

/// A process using this GPU - PID, name, type, VRAM.
#[derive(Debug, Clone)]
pub struct GpuProcessInfo {
    pub pid: u32,
    pub name: String,
    pub used_gpu_memory: u64,
    pub kind: GpuProcessKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuProcessKind {
    Graphics,
    Compute,
}

impl GpuMetrics {
    pub fn vram_pct(&self) -> f64 {
        if self.vram_total > 0 {
            (self.vram_used as f64 / self.vram_total as f64) * 100.0
        } else {
            0.0
        }
    }
}

/// GPU control state - adjustable values for fan, power, clocks.
#[derive(Debug, Clone)]
pub struct GpuControl {
    pub fan_speed_pct: Option<u32>,    // None = auto, Some = manual override
    pub power_limit_watts: Option<f64>,
    pub clock_offset_mhz: i32,        // Core clock offset (+/-)
    pub mem_offset_mhz: i32,          // Memory clock offset (+/-)
    pub fan_auto: bool,
}

impl GpuControl {
    pub fn new() -> Self {
        Self {
            fan_speed_pct: None,
            power_limit_watts: None,
            clock_offset_mhz: 0,
            mem_offset_mhz: 0,
            fan_auto: true,
        }
    }
}

/// Which control row is selected in the GPU Control panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlRow {
    FanSpeed,
    PowerLimit,
    CoreClock,
    MemClock,
}

impl ControlRow {
    pub fn next(self) -> Self {
        match self {
            ControlRow::FanSpeed => ControlRow::PowerLimit,
            ControlRow::PowerLimit => ControlRow::CoreClock,
            ControlRow::CoreClock => ControlRow::MemClock,
            ControlRow::MemClock => ControlRow::FanSpeed,
        }
    }
    pub fn prev(self) -> Self {
        match self {
            ControlRow::FanSpeed => ControlRow::MemClock,
            ControlRow::PowerLimit => ControlRow::FanSpeed,
            ControlRow::CoreClock => ControlRow::PowerLimit,
            ControlRow::MemClock => ControlRow::CoreClock,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortColumn {
    Pid,
    Name,
    Cpu,
    Memory,
    GpuUtil,
    GpuVram,
}

impl SortColumn {
    pub fn next(self) -> Self {
        match self {
            SortColumn::Pid => SortColumn::Name,
            SortColumn::Name => SortColumn::Cpu,
            SortColumn::Cpu => SortColumn::Memory,
            SortColumn::Memory => SortColumn::GpuUtil,
            SortColumn::GpuUtil => SortColumn::GpuVram,
            SortColumn::GpuVram => SortColumn::Pid,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cpu_usage: f64,
    pub memory_bytes: u64,
    pub gpu_util: Option<f64>,
    pub gpu_vram: Option<u64>,
}

#[derive(Clone)]
pub struct App {
    pub running: bool,
    pub active_panel: Panel,
    pub cpu: CpuMetrics,
    pub memory: MemMetrics,
    pub disks: Vec<DiskInfo>,
    pub networks: Vec<NetInterface>,
    pub gpus: Vec<GpuMetrics>,
    pub gpu_controls: Vec<GpuControl>,
    pub control_row: ControlRow,
    pub processes: Vec<ProcessInfo>,
    pub selected_gpu: usize,
    pub sort_column: SortColumn,
    pub sort_ascending: bool,
    pub process_scroll: usize,
    pub process_selected: usize,
    pub filter: String,
    pub filtering: bool,
    pub tick_rate: Duration,
    pub last_tick: Instant,
    pub confirm_kill: Option<u32>,
    pub status_msg: Option<(String, bool)>, // (message, is_error)
}

impl App {
    pub fn new(tick_rate: Duration) -> Self {
        Self {
            running: true,
            active_panel: Panel::Dashboard,
            cpu: CpuMetrics {
                total_usage: 0.0,
                total_history: History::new(),
                cores: Vec::new(),
                name: String::new(),
                physical_cores: 0,
                logical_cores: 0,
            },
            memory: MemMetrics {
                total_bytes: 0,
                used_bytes: 0,
                swap_total: 0,
                swap_used: 0,
                usage_history: History::new(),
            },
            disks: Vec::new(),
            networks: Vec::new(),
            gpus: Vec::new(),
            gpu_controls: Vec::new(),
            control_row: ControlRow::FanSpeed,
            processes: Vec::new(),
            selected_gpu: 0,
            sort_column: SortColumn::Cpu,
            sort_ascending: false,
            process_scroll: 0,
            process_selected: 0,
            filter: String::new(),
            filtering: false,
            tick_rate,
            last_tick: Instant::now(),
            confirm_kill: None,
            status_msg: None,
        }
    }

    pub fn quit(&mut self) {
        self.running = false;
    }

    pub fn next_panel(&mut self) {
        self.active_panel = self.active_panel.next();
    }

    pub fn cycle_sort(&mut self) {
        self.sort_column = self.sort_column.next();
    }

    pub fn sorted_processes(&self) -> Vec<&ProcessInfo> {
        let mut procs: Vec<&ProcessInfo> = self.processes.iter().collect();

        if !self.filter.is_empty() {
            let f = self.filter.to_lowercase();
            procs.retain(|p| p.name.to_lowercase().contains(&f));
        }

        procs.sort_by(|a, b| {
            let ord = match self.sort_column {
                SortColumn::Pid => a.pid.cmp(&b.pid),
                SortColumn::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortColumn::Cpu => a.cpu_usage.partial_cmp(&b.cpu_usage).unwrap_or(std::cmp::Ordering::Equal),
                SortColumn::Memory => a.memory_bytes.cmp(&b.memory_bytes),
                SortColumn::GpuUtil => {
                    let av = a.gpu_util.unwrap_or(0.0);
                    let bv = b.gpu_util.unwrap_or(0.0);
                    av.partial_cmp(&bv).unwrap_or(std::cmp::Ordering::Equal)
                }
                SortColumn::GpuVram => {
                    let av = a.gpu_vram.unwrap_or(0);
                    let bv = b.gpu_vram.unwrap_or(0);
                    av.cmp(&bv)
                }
            };
            if self.sort_ascending { ord } else { ord.reverse() }
        });

        procs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32, name: &str, cpu: f64, mem: u64, util: Option<f64>, vram: Option<u64>) -> ProcessInfo {
        ProcessInfo {
            pid,
            name: name.into(),
            cpu_usage: cpu,
            memory_bytes: mem,
            gpu_util: util,
            gpu_vram: vram,
        }
    }

    fn app_with(procs: Vec<ProcessInfo>) -> App {
        let mut app = App::new(Duration::from_millis(500));
        app.processes = procs;
        app
    }

    fn sample() -> Vec<ProcessInfo> {
        vec![
            proc(100, "chrome.exe", 5.0, 800, Some(3.0), Some(3 * 1024 * 1024 * 1024)),
            proc(200, "Ollama.exe", 1.0, 400, Some(90.0), Some(20 * 1024 * 1024 * 1024)),
            proc(300, "idle", 0.0, 100, None, None),
        ]
    }

    #[test]
    fn history_is_a_bounded_ring_buffer() {
        let mut h = History::new();
        for i in 0..(HISTORY_LEN + 50) {
            h.push(i as f64);
        }
        assert_eq!(h.data.len(), HISTORY_LEN);
        // Oldest samples were dropped; newest is last.
        assert_eq!(h.last(), (HISTORY_LEN + 49) as f64);
        assert_eq!(h.data[0], 50.0);
    }

    #[test]
    fn empty_history_last_is_zero() {
        assert_eq!(History::new().last(), 0.0);
    }

    #[test]
    fn vram_pct_is_safe_when_total_unknown() {
        let mut g = GpuMetrics {
            name: "RTX 5090".into(),
            utilization: 0.0,
            utilization_history: History::new(),
            vram_used: 8 * 1024 * 1024 * 1024,
            vram_total: 32 * 1024 * 1024 * 1024,
            vram_history: History::new(),
            temp_core: None,
            temp_hotspot: None,
            temp_vram: None,
            temp_history: History::new(),
            power_watts: None,
            power_limit: None,
            power_history: History::new(),
            fan_rpm: None,
            clock_core_mhz: None,
            clock_mem_mhz: None,
            pcie_tx_bytes_sec: None,
            pcie_rx_bytes_sec: None,
            processes: Vec::new(),
        };
        assert!((g.vram_pct() - 25.0).abs() < 1e-9);
        // Driver not reporting total must not divide by zero.
        g.vram_total = 0;
        assert_eq!(g.vram_pct(), 0.0);
    }

    #[test]
    fn sorts_by_gpu_vram_descending_by_default() {
        let app = app_with(sample());
        let mut app = app;
        app.sort_column = SortColumn::GpuVram;
        app.sort_ascending = false;
        let pids: Vec<u32> = app.sorted_processes().iter().map(|p| p.pid).collect();
        assert_eq!(pids, vec![200, 100, 300]);
    }

    #[test]
    fn sorts_by_gpu_util_ascending_treats_none_as_zero() {
        let mut app = app_with(sample());
        app.sort_column = SortColumn::GpuUtil;
        app.sort_ascending = true;
        let pids: Vec<u32> = app.sorted_processes().iter().map(|p| p.pid).collect();
        assert_eq!(pids, vec![300, 100, 200]);
    }

    #[test]
    fn sorts_by_name_case_insensitively() {
        let mut app = app_with(sample());
        app.sort_column = SortColumn::Name;
        app.sort_ascending = true;
        let names: Vec<&str> = app.sorted_processes().iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["chrome.exe", "idle", "Ollama.exe"]);
    }

    #[test]
    fn filter_is_case_insensitive_substring() {
        let mut app = app_with(sample());
        app.filter = "OLLAMA".into();
        let pids: Vec<u32> = app.sorted_processes().iter().map(|p| p.pid).collect();
        assert_eq!(pids, vec![200]);

        app.filter = "nomatch".into();
        assert!(app.sorted_processes().is_empty());

        app.filter.clear();
        assert_eq!(app.sorted_processes().len(), 3);
    }

    #[test]
    fn sorting_nan_cpu_does_not_panic() {
        let mut app = app_with(vec![
            proc(1, "a", f64::NAN, 1, None, None),
            proc(2, "b", 5.0, 2, None, None),
        ]);
        app.sort_column = SortColumn::Cpu;
        assert_eq!(app.sorted_processes().len(), 2);
    }

    #[test]
    fn panel_cycle_visits_every_panel_once() {
        let mut seen = Vec::new();
        let mut p = Panel::Dashboard;
        for _ in 0..10 {
            seen.push(p);
            p = p.next();
        }
        assert_eq!(p, Panel::Dashboard, "cycle should close");
        seen.sort_by_key(|p| p.label());
        seen.dedup_by_key(|p| p.label());
        assert_eq!(seen.len(), 10, "every panel should appear exactly once");
    }

    #[test]
    fn sort_column_cycle_closes() {
        let mut c = SortColumn::Pid;
        for _ in 0..6 {
            c = c.next();
        }
        assert_eq!(c, SortColumn::Pid);
    }

    #[test]
    fn control_row_next_and_prev_are_inverses() {
        for row in [
            ControlRow::FanSpeed,
            ControlRow::PowerLimit,
            ControlRow::CoreClock,
            ControlRow::MemClock,
        ] {
            assert_eq!(row.next().prev(), row);
        }
    }
}

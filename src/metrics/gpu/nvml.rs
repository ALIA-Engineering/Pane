//! NVIDIA GPU metrics via NVML (NVIDIA Management Library).
//!
//! Provides device-level metrics: utilization, VRAM, temperature, power,
//! clocks, PCIe throughput, and fan speed. Works on both Windows and Linux.
//!
//! Note: On Windows (WDDM mode), per-process VRAM from NVML returns
//! NOT_AVAILABLE. Per-process GPU data comes from PDH instead (see pdh.rs).

use std::sync::OnceLock;

use crate::app::{GpuMetrics, GpuProcessInfo, GpuProcessKind, History};
use super::GpuBackend;

/// Lazily initialized NVML instance - lives for the process lifetime.
static NVML: OnceLock<nvml_wrapper::Nvml> = OnceLock::new();

pub struct NvmlBackend {
    devices: Vec<NvmlDevice>,
}

struct NvmlDevice {
    index: u32,
    metrics: GpuMetrics,
}

impl NvmlBackend {
    pub fn try_new() -> Option<Self> {
        let nvml = match nvml_wrapper::Nvml::init() {
            Ok(n) => {
                // Store in OnceLock for later use
                let _ = NVML.set(n);
                NVML.get()?
            }
            Err(_) => return None,
        };

        let count = nvml.device_count().ok()?;
        if count == 0 {
            return None;
        }

        let mut devices = Vec::new();
        for i in 0..count {
            let handle = match nvml.device_by_index(i) {
                Ok(h) => h,
                Err(_) => continue,
            };
            let name = handle.name().unwrap_or_else(|_| format!("GPU {}", i));
            let vram_total = handle.memory_info().map(|m| m.total).unwrap_or(0);

            devices.push(NvmlDevice {
                index: i,
                metrics: GpuMetrics {
                    name,
                    utilization: 0.0,
                    utilization_history: History::new(),
                    vram_used: 0,
                    vram_total,
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
                },
            });
        }

        if devices.is_empty() {
            return None;
        }

        Some(Self { devices })
    }
}

impl GpuBackend for NvmlBackend {
    fn refresh(&mut self) {
        let nvml = match NVML.get() {
            Some(n) => n,
            None => return,
        };

        // One sysinfo snapshot per tick, shared by every device/PID lookup.
        // The old code built a fresh System per process name, ~120 times per
        // second at 500 ms refresh across 30 processes.
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

        for dev in &mut self.devices {
            let h = match nvml.device_by_index(dev.index) {
                Ok(h) => h,
                Err(_) => continue,
            };

            // Utilization (% of time GPU was busy)
            if let Ok(util) = h.utilization_rates() {
                dev.metrics.utilization = util.gpu as f64;
                dev.metrics.utilization_history.push(util.gpu as f64);
            }

            // VRAM (device-level, always works)
            if let Ok(mem) = h.memory_info() {
                dev.metrics.vram_used = mem.used;
                dev.metrics.vram_total = mem.total;
            }

            // Temperature
            dev.metrics.temp_core = h
                .temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu)
                .ok();

            // Power (NVML returns milliwatts)
            dev.metrics.power_watts = h.power_usage().ok().map(|mw| mw as f64 / 1000.0);
            dev.metrics.power_limit = h
                .power_management_limit()
                .ok()
                .map(|mw| mw as f64 / 1000.0);

            // Clocks
            dev.metrics.clock_core_mhz = h
                .clock_info(nvml_wrapper::enum_wrappers::device::Clock::Graphics)
                .ok();
            dev.metrics.clock_mem_mhz = h
                .clock_info(nvml_wrapper::enum_wrappers::device::Clock::Memory)
                .ok();

            // PCIe throughput (NVML returns KB/s, we want bytes/sec)
            dev.metrics.pcie_tx_bytes_sec = h
                .pcie_throughput(nvml_wrapper::enum_wrappers::device::PcieUtilCounter::Send)
                .ok()
                .map(|kb| kb as u64 * 1024);
            dev.metrics.pcie_rx_bytes_sec = h
                .pcie_throughput(nvml_wrapper::enum_wrappers::device::PcieUtilCounter::Receive)
                .ok()
                .map(|kb| kb as u64 * 1024);

            // Fan speed (NVML gives percentage, not RPM)
            dev.metrics.fan_rpm = h.fan_speed(0).ok();

            // Per-GPU processes. NVML reports graphics and compute contexts
            // separately and the same PID can appear in both lists.
            let graphics: Vec<(u32, u64)> = h
                .running_graphics_processes()
                .unwrap_or_default()
                .into_iter()
                .map(|p| (p.pid, extract_gpu_mem(p.used_gpu_memory)))
                .collect();
            let compute: Vec<(u32, u64)> = h
                .running_compute_processes()
                .unwrap_or_default()
                .into_iter()
                .map(|p| (p.pid, extract_gpu_mem(p.used_gpu_memory)))
                .collect();

            dev.metrics.processes = merge_processes(&graphics, &compute, |pid| process_name(&sys, pid));
        }
    }

    fn metrics(&self) -> Vec<GpuMetrics> {
        self.devices.iter().map(|d| d.metrics.clone()).collect()
    }

    fn set_power_limit(&mut self, gpu_index: usize, watts: f64) -> Result<(), String> {
        let nvml = NVML.get().ok_or("NVML not initialized")?;
        let dev = self.devices.get(gpu_index).ok_or("Invalid GPU index")?;
        let mut handle = nvml.device_by_index(dev.index).map_err(|e| format!("Device error: {}", e))?;

        let milliwatts = (watts * 1000.0) as u32;
        handle.set_power_management_limit(milliwatts)
            .map_err(|e| format!("Failed to set power limit: {} (requires admin)", e))
    }
}

/// Merge NVML's graphics and compute process lists into one table.
///
/// - A PID present in both lists is reported once, as `Graphics`
///   (the graphics context is the one the user sees on screen).
/// - Duplicate PIDs *within* a list are collapsed, summing memory: NVML can
///   report one entry per context.
/// - Result is sorted by VRAM descending, then PID ascending for stability.
///
/// Split out from the NVML call path so it can be unit tested without a GPU.
fn merge_processes(
    graphics: &[(u32, u64)],
    compute: &[(u32, u64)],
    resolve_name: impl Fn(u32) -> String,
) -> Vec<GpuProcessInfo> {
    let mut out: Vec<GpuProcessInfo> = Vec::new();

    let push = |pid: u32, mem: u64, kind: GpuProcessKind, out: &mut Vec<GpuProcessInfo>| {
        if let Some(existing) = out.iter_mut().find(|p: &&mut GpuProcessInfo| p.pid == pid) {
            existing.used_gpu_memory = existing.used_gpu_memory.saturating_add(mem);
            return;
        }
        out.push(GpuProcessInfo {
            pid,
            name: resolve_name(pid),
            used_gpu_memory: mem,
            kind,
        });
    };

    for &(pid, mem) in graphics {
        push(pid, mem, GpuProcessKind::Graphics, &mut out);
    }
    for &(pid, mem) in compute {
        push(pid, mem, GpuProcessKind::Compute, &mut out);
    }

    out.sort_by(|a, b| {
        b.used_gpu_memory
            .cmp(&a.used_gpu_memory)
            .then(a.pid.cmp(&b.pid))
    });
    out
}

/// Extract GPU memory usage from NVML's enum type.
fn extract_gpu_mem(mem: nvml_wrapper::enums::device::UsedGpuMemory) -> u64 {
    match mem {
        nvml_wrapper::enums::device::UsedGpuMemory::Used(bytes) => bytes,
        nvml_wrapper::enums::device::UsedGpuMemory::Unavailable => 0,
    }
}

/// Resolve PID to process name from a sysinfo snapshot refreshed once per tick.
fn process_name(sys: &sysinfo::System, pid: u32) -> String {
    use sysinfo::Pid;
    sys.process(Pid::from_u32(pid))
        .map(|p| p.name().to_string_lossy().to_string())
        .unwrap_or_else(|| format!("PID {}", pid))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvml_wrapper::enums::device::UsedGpuMemory;

    fn stub_name(pid: u32) -> String {
        format!("proc{pid}")
    }

    #[test]
    fn extract_gpu_mem_handles_both_variants() {
        assert_eq!(extract_gpu_mem(UsedGpuMemory::Used(4_294_967_296)), 4_294_967_296);
        assert_eq!(extract_gpu_mem(UsedGpuMemory::Used(0)), 0);
        // WDDM consumer GPUs return Unavailable - must degrade to 0, not panic.
        assert_eq!(extract_gpu_mem(UsedGpuMemory::Unavailable), 0);
    }

    #[test]
    fn merge_sorts_by_vram_descending() {
        let procs = merge_processes(&[(10, 1024), (11, 8192), (12, 4096)], &[], stub_name);
        assert_eq!(
            procs.iter().map(|p| p.pid).collect::<Vec<_>>(),
            vec![11, 12, 10]
        );
        assert_eq!(procs[0].name, "proc11");
    }

    #[test]
    fn merge_dedups_pid_present_in_both_lists() {
        let procs = merge_processes(&[(7, 2048)], &[(7, 9999), (8, 512)], stub_name);
        assert_eq!(procs.len(), 2);
        let seven = procs.iter().find(|p| p.pid == 7).unwrap();
        // Same PID in both lists is one context reported twice by NVML,
        // so memory accumulates and the kind stays Graphics.
        assert_eq!(seven.kind, GpuProcessKind::Graphics);
        assert_eq!(seven.used_gpu_memory, 2048 + 9999);
    }

    #[test]
    fn merge_labels_compute_only_pids() {
        let procs = merge_processes(&[], &[(42, 1)], stub_name);
        assert_eq!(procs[0].kind, GpuProcessKind::Compute);
    }

    #[test]
    fn merge_handles_empty_input() {
        assert!(merge_processes(&[], &[], stub_name).is_empty());
    }

    #[test]
    fn merge_ties_break_on_pid() {
        let procs = merge_processes(&[(30, 100), (10, 100), (20, 100)], &[], stub_name);
        assert_eq!(
            procs.iter().map(|p| p.pid).collect::<Vec<_>>(),
            vec![10, 20, 30]
        );
    }

    #[test]
    fn merge_saturates_on_overflow() {
        let procs = merge_processes(&[(1, u64::MAX)], &[(1, 4096)], stub_name);
        assert_eq!(procs[0].used_gpu_memory, u64::MAX);
    }

    /// Live check against the real driver. Ignored by default.
    #[test]
    #[ignore = "requires an NVIDIA GPU + driver; run with --ignored"]
    fn live_nvml_reports_devices() {
        let mut backend = NvmlBackend::try_new().expect("NVML init failed");
        backend.refresh();
        let metrics = backend.metrics();
        assert!(!metrics.is_empty());
        for (i, m) in metrics.iter().enumerate() {
            println!(
                "GPU {i}: {} | util {:.0}% | vram {:.1}/{:.1} GiB | temp {:?}C | power {:?}W | limit {:?}W | procs {}",
                m.name,
                m.utilization,
                m.vram_used as f64 / (1024.0 * 1024.0 * 1024.0),
                m.vram_total as f64 / (1024.0 * 1024.0 * 1024.0),
                m.temp_core,
                m.power_watts.map(|w| w.round()),
                m.power_limit.map(|w| w.round()),
                m.processes.len(),
            );
            assert!(m.vram_total > 0, "vram_total should be known");
            assert!((0.0..=100.0).contains(&m.vram_pct()));
        }
    }
}

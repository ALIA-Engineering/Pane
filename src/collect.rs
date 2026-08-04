//! Shared metric collection logic - used by both GUI and TUI.
//!
//! Pulls data from system collectors and GPU backends into App state.

use crate::app::{App, GpuControl};
use crate::metrics::gpu::GpuBackend;
use crate::metrics::gpu::pdh::PdhGpuCollector;
use crate::metrics::system::SystemCollector;

/// Refresh all metrics and update app state. Called once per tick.
pub fn collect_metrics(
    app: &mut App,
    sys: &mut SystemCollector,
    gpu_backend: &mut Box<dyn GpuBackend>,
    pdh: &mut PdhGpuCollector,
) {
    sys.refresh();
    sys.cpu(&mut app.cpu);
    sys.memory(&mut app.memory);
    app.disks = sys.disks();
    app.networks = sys.networks();
    app.processes = sys.processes();
    sys.mark_tick();

    // Per-process GPU metrics (PDH on Windows, no-op elsewhere)
    pdh.refresh();
    let gpu_usage = pdh.per_process();
    for proc in &mut app.processes {
        if let Some(usage) = gpu_usage.get(&proc.pid) {
            if usage.utilization > 0.01 {
                proc.gpu_util = Some(usage.utilization);
            }
            let total_vram = usage.dedicated_vram + usage.shared_vram;
            if total_vram > 0 {
                proc.gpu_vram = Some(total_vram);
            }
        }
    }

    gpu_backend.refresh();
    let gpu_metrics = gpu_backend.metrics();

    if app.gpus.len() != gpu_metrics.len() {
        app.gpus = gpu_metrics;
    } else {
        for (i, new) in gpu_metrics.into_iter().enumerate() {
            let g = &mut app.gpus[i];
            g.utilization = new.utilization;
            g.utilization_history.push(new.utilization);
            g.vram_used = new.vram_used;
            g.vram_total = new.vram_total;
            g.vram_history.push(new.vram_pct());
            g.temp_core = new.temp_core;
            g.temp_hotspot = new.temp_hotspot;
            g.temp_vram = new.temp_vram;
            if let Some(t) = new.temp_core {
                g.temp_history.push(t as f64);
            }
            g.power_watts = new.power_watts;
            g.power_limit = new.power_limit;
            if let Some(w) = new.power_watts {
                g.power_history.push(w);
            }
            g.fan_rpm = new.fan_rpm;
            g.clock_core_mhz = new.clock_core_mhz;
            g.clock_mem_mhz = new.clock_mem_mhz;
            g.pcie_tx_bytes_sec = new.pcie_tx_bytes_sec;
            g.pcie_rx_bytes_sec = new.pcie_rx_bytes_sec;
            g.processes = new.processes;
        }
    }

    while app.gpu_controls.len() < app.gpus.len() {
        app.gpu_controls.push(GpuControl::new());
    }
}

/// Create a Command that won't show a console window on Windows GUI apps.
#[cfg(windows)]
fn hidden_cmd(program: &str) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let mut cmd = std::process::Command::new(program);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

#[cfg(not(windows))]
fn hidden_cmd(program: &str) -> std::process::Command {
    std::process::Command::new(program)
}

/// Check if the current process is running with admin/elevated privileges.
#[allow(dead_code)] // GUI-only; the TUI has no control panel
pub fn is_elevated() -> bool {
    #[cfg(windows)]
    {
        hidden_cmd("net")
            .args(["session"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        // Check if running as root by trying to read a root-only file
        std::fs::metadata("/root").is_ok()
    }
}

/// Graceful close - asks the process to exit cleanly.
#[allow(dead_code)] // GUI-only
pub fn close_process(pid: u32) -> Result<(), String> {
    #[cfg(windows)]
    {
        let output = hidden_cmd("taskkill")
            .args(["/PID", &pid.to_string()])
            .output()
            .map_err(|e| e.to_string())?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("Access is denied") {
                Err("Access denied - run Pane as administrator".into())
            } else {
                Err(stderr.trim().to_string())
            }
        }
    }
    #[cfg(not(windows))]
    {
        let output = hidden_cmd("kill")
            .args(["-15", &pid.to_string()])
            .output()
            .map_err(|e| e.to_string())?;

        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }
}

/// Force kill - immediately terminates the process.
/// Returns Ok(()) on success, Err(message) on failure.
pub fn kill_process(pid: u32) -> Result<(), String> {
    #[cfg(windows)]
    {
        let output = hidden_cmd("taskkill")
            .args(["/F", "/PID", &pid.to_string()])
            .output()
            .map_err(|e| e.to_string())?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("Access is denied") {
                Err("Access denied - run Pane as administrator".into())
            } else {
                Err(stderr.trim().to_string())
            }
        }
    }
    #[cfg(not(windows))]
    {
        let output = hidden_cmd("kill")
            .args(["-9", &pid.to_string()])
            .output()
            .map_err(|e| e.to_string())?;

        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }
}

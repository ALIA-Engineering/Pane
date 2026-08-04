use crate::app::GpuMetrics;

pub mod pdh;

pub mod nvml;

pub trait GpuBackend {
    fn refresh(&mut self);
    fn metrics(&self) -> Vec<GpuMetrics>;

    /// Set power limit in watts for a GPU. Returns error message on failure.
    #[allow(dead_code)] // GUI-only; the TUI has no control panel
    fn set_power_limit(&mut self, _gpu_index: usize, _watts: f64) -> Result<(), String> {
        Err("Not supported on this backend".into())
    }
}

/// Detect available GPU backends and return the best one
pub fn create_backend() -> Box<dyn GpuBackend> {
    // Try NVML first (works on Windows + Linux for NVIDIA)
    if let Some(backend) = nvml::NvmlBackend::try_new() {
        return Box::new(backend);
    }

    // Fallback: no GPU monitoring
    Box::new(NoGpu)
}

struct NoGpu;

impl GpuBackend for NoGpu {
    fn refresh(&mut self) {}
    fn metrics(&self) -> Vec<GpuMetrics> {
        Vec::new()
    }
}

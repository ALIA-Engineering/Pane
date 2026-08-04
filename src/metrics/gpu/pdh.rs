//! Windows Performance Counter (PDH) backend for per-process GPU metrics.
//!
//! This is the same data source Task Manager uses for its GPU columns.
//! Vendor-agnostic (NVIDIA, AMD, Intel), no admin elevation required.
//!
//! Counter paths (English names, via `PdhAddEnglishCounterW`):
//! - `\GPU Engine(pid_XXXX_..._engtype_YYY)\Utilization Percentage`
//! - `\GPU Process Memory(pid_XXXX_...)\Dedicated Usage`
//! - `\GPU Process Memory(pid_XXXX_...)\Shared Usage`
//!
//! The primary implementation calls the PDH API directly through `windows-rs`.
//! If PDH initialisation fails for any reason we fall back to shelling out to
//! PowerShell's `Get-Counter`, which is much slower (~2.8 s per refresh) but
//! needs no special support.

use std::collections::HashMap;

/// Per-process GPU usage, aggregated across all engines and adapters.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProcessGpuUsage {
    pub pid: u32,
    /// Highest single-engine utilization percentage seen for this process.
    pub utilization: f64,
    /// Dedicated (on-card) VRAM in bytes.
    pub dedicated_vram: u64,
    /// Shared (system) GPU memory in bytes.
    pub shared_vram: u64,
}

/// Which backend actually produced the data (for diagnostics / UI labelling).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // `Unavailable` is only constructed on non-Windows targets
pub enum PdhSource {
    /// Native PDH via windows-rs.
    NativePdh,
    /// PowerShell `Get-Counter` fallback.
    PowerShell,
    /// Not available on this platform.
    Unavailable,
}

impl PdhSource {
    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            PdhSource::NativePdh => "PDH",
            PdhSource::PowerShell => "PowerShell",
            PdhSource::Unavailable => "unavailable",
        }
    }
}

// ---------------------------------------------------------------------------
// Instance-name parsing (pure, platform independent, unit tested)
// ---------------------------------------------------------------------------

/// A parsed `GPU Engine` / `GPU Process Memory` counter instance name.
///
/// Real examples:
/// - `pid_12345_luid_0x00000000_0x0000D1B5_phys_0`
/// - `pid_12345_luid_0x00000000_0x0000D1B5_phys_0_eng_3_engtype_3D`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuInstance {
    pub pid: u32,
    /// The `luid_<hi>_<lo>` portion, lowercased, without the `luid_` prefix.
    pub luid: Option<String>,
    /// Physical adapter index from `phys_<N>`.
    pub phys: Option<u32>,
    /// Engine index from `eng_<N>`.
    pub eng: Option<u32>,
    /// Engine type from `engtype_<NAME>` (e.g. `3d`, `copy`, `video_decode`).
    pub engtype: Option<String>,
}

/// Parse a GPU counter instance name.
///
/// Returns `None` if the name does not start with `pid_` followed by digits.
/// Missing trailing segments are tolerated so that driver revisions which add
/// new fields do not break parsing.
pub fn parse_gpu_instance(name: &str) -> Option<GpuInstance> {
    let lower = name.trim().to_ascii_lowercase();
    let rest = lower.strip_prefix("pid_")?;

    // PID: leading run of digits, terminated by `_` or end of string.
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    if !matches!(rest.as_bytes().get(digits.len()), None | Some(b'_')) {
        return None;
    }
    let pid: u32 = digits.parse().ok()?;

    let parts: Vec<&str> = lower.split('_').collect();
    let mut inst = GpuInstance {
        pid,
        luid: None,
        phys: None,
        eng: None,
        engtype: None,
    };

    let mut i = 0;
    while i < parts.len() {
        match parts[i] {
            // luid is two underscore-separated hex words: luid_0xAAAA_0xBBBB
            "luid" if i + 2 < parts.len() => {
                inst.luid = Some(format!("{}_{}", parts[i + 1], parts[i + 2]));
                i += 3;
            }
            "phys" if i + 1 < parts.len() => {
                inst.phys = parts[i + 1].parse().ok();
                i += 2;
            }
            "eng" if i + 1 < parts.len() => {
                inst.eng = parts[i + 1].parse().ok();
                i += 2;
            }
            // engtype is the last field and may itself contain underscores.
            "engtype" if i + 1 < parts.len() => {
                inst.engtype = Some(parts[i + 1..].join("_"));
                break;
            }
            _ => i += 1,
        }
    }

    Some(inst)
}

/// Extract just the PID from an instance name.
#[allow(dead_code)]
pub fn extract_pid(instance: &str) -> Option<u32> {
    parse_gpu_instance(instance).map(|i| i.pid)
}

/// Extract the instance name from a full counter *path* such as
/// `\\HOST\GPU Process Memory(pid_1234_luid_..._phys_0)\Dedicated Usage`.
///
/// PowerShell's `Get-Counter` reports `.Path` (not `.InstanceName`) for the
/// memory counters, so the instance has to be unwrapped from the parentheses
/// before it can be parsed.
pub fn instance_from_path(path: &str) -> Option<&str> {
    let open = path.find('(')?;
    let close = path[open + 1..].find(')')? + open + 1;
    let inner = &path[open + 1..close];
    if inner.is_empty() { None } else { Some(inner) }
}

/// Extract a PID from a full counter *path* such as
/// `\\HOST\GPU Process Memory(pid_1234_luid_..._phys_0)\Dedicated Usage`.
#[allow(dead_code)]
pub fn extract_pid_from_path(path: &str) -> Option<u32> {
    let lower = path.to_ascii_lowercase();
    let start = lower.find("pid_")? + 4;
    let digits: String = lower[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok()
}

/// Which GPU Process Memory counter a path refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryCounter {
    Dedicated,
    Shared,
    Local,
    NonLocal,
    Other,
}

/// Classify the counter name at the end of a PDH counter path.
pub fn classify_memory_counter(path: &str) -> MemoryCounter {
    let lower = path.to_ascii_lowercase();
    // Match on the trailing counter name only, after the instance parens.
    let tail = match lower.rfind(')') {
        Some(idx) => lower[idx..].to_string(),
        None => lower,
    };
    if tail.contains("dedicated usage") {
        MemoryCounter::Dedicated
    } else if tail.contains("shared usage") {
        MemoryCounter::Shared
    } else if tail.contains("non local usage") {
        MemoryCounter::NonLocal
    } else if tail.contains("local usage") {
        MemoryCounter::Local
    } else {
        MemoryCounter::Other
    }
}

/// Fold a single `(instance, utilization)` sample into the aggregate map.
///
/// Utilization is reported per engine; a process may be busy on several
/// engines at once. We report the *maximum* single-engine value, which is what
/// Task Manager's "GPU" column shows.
pub fn fold_utilization(map: &mut HashMap<u32, ProcessGpuUsage>, instance: &str, value: f64) {
    if !value.is_finite() || value < 0.0 {
        return;
    }
    let Some(inst) = parse_gpu_instance(instance) else {
        return;
    };
    let entry = map.entry(inst.pid).or_insert(ProcessGpuUsage {
        pid: inst.pid,
        ..Default::default()
    });
    if value > entry.utilization {
        entry.utilization = value;
    }
}

/// Fold a single memory sample into the aggregate map.
///
/// A process can hold allocations on several adapters (dual-GPU machines emit
/// one instance per `phys_N`), so memory is **summed** across instances.
/// Counters other than Dedicated/Shared are ignored to avoid double counting.
pub fn fold_memory(
    map: &mut HashMap<u32, ProcessGpuUsage>,
    instance: &str,
    counter: MemoryCounter,
    bytes: u64,
) {
    if !matches!(counter, MemoryCounter::Dedicated | MemoryCounter::Shared) {
        return;
    }
    let Some(inst) = parse_gpu_instance(instance) else {
        return;
    };
    let entry = map.entry(inst.pid).or_insert(ProcessGpuUsage {
        pid: inst.pid,
        ..Default::default()
    });
    match counter {
        MemoryCounter::Dedicated => {
            entry.dedicated_vram = entry.dedicated_vram.saturating_add(bytes)
        }
        MemoryCounter::Shared => entry.shared_vram = entry.shared_vram.saturating_add(bytes),
        _ => {}
    }
}

/// Parse one `instance=value` line as emitted by the PowerShell fallback.
pub fn parse_ps_pair(line: &str) -> Option<(&str, f64)> {
    let (lhs, rhs) = line.rsplit_once('=')?;
    let value: f64 = rhs.trim().parse().ok()?;
    Some((lhs.trim(), value))
}

// ---------------------------------------------------------------------------
// Native PDH backend (Windows only)
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
mod windows_pdh {
    use super::{MemoryCounter, ProcessGpuUsage, fold_memory, fold_utilization};
    use std::collections::HashMap;
    use windows::Win32::System::Performance::{
        PDH_FMT, PDH_FMT_COUNTERVALUE, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_FMT_LARGE,
        PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA, PdhAddEnglishCounterW, PdhCloseQuery,
        PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW,
    };
    use windows::core::PCWSTR;

    const ERROR_SUCCESS: u32 = 0;
    /// Not exposed by windows-rs 0.62; documented value from `pdh.h`.
    const PDH_FMT_NOCAP100: u32 = 0x0000_8000;
    /// Typical machines have a few hundred GPU Engine instances.
    const INITIAL_BUFFER_ITEMS: usize = 512;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Owned PDH query handle. Closes on drop.
    pub struct PdhQuery {
        query: PDH_HQUERY,
        engine_util: PDH_HCOUNTER,
        mem_dedicated: PDH_HCOUNTER,
        mem_shared: PDH_HCOUNTER,
    }

    // The query handle is only ever touched from the thread that owns the
    // collector; PDH itself is thread safe for distinct queries.
    unsafe impl Send for PdhQuery {}

    impl PdhQuery {
        pub fn new() -> Result<Self, String> {
            unsafe {
                let mut query = PDH_HQUERY::default();
                let rc = PdhOpenQueryW(PCWSTR::null(), 0, &mut query);
                if rc != ERROR_SUCCESS {
                    return Err(format!("PdhOpenQueryW failed: 0x{rc:08X}"));
                }

                let add = |path: &str| -> Result<PDH_HCOUNTER, String> {
                    let mut counter = PDH_HCOUNTER::default();
                    let w = wide(path);
                    let rc = PdhAddEnglishCounterW(query, PCWSTR(w.as_ptr()), 0, &mut counter);
                    if rc != ERROR_SUCCESS {
                        return Err(format!("PdhAddEnglishCounterW({path}) failed: 0x{rc:08X}"));
                    }
                    Ok(counter)
                };

                let counters = (|| {
                    Ok::<_, String>((
                        add(r"\GPU Engine(*)\Utilization Percentage")?,
                        add(r"\GPU Process Memory(*)\Dedicated Usage")?,
                        add(r"\GPU Process Memory(*)\Shared Usage")?,
                    ))
                })();

                let (engine_util, mem_dedicated, mem_shared) = match counters {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = PdhCloseQuery(query);
                        return Err(e);
                    }
                };

                // Rate counters (Utilization Percentage) need a baseline sample.
                let rc = PdhCollectQueryData(query);
                if rc != ERROR_SUCCESS {
                    let _ = PdhCloseQuery(query);
                    return Err(format!("initial PdhCollectQueryData failed: 0x{rc:08X}"));
                }

                Ok(Self {
                    query,
                    engine_util,
                    mem_dedicated,
                    mem_shared,
                })
            }
        }

        /// Collect one sample and return the aggregated per-process map.
        pub fn collect(&mut self) -> Result<HashMap<u32, ProcessGpuUsage>, String> {
            let rc = unsafe { PdhCollectQueryData(self.query) };
            if rc != ERROR_SUCCESS {
                return Err(format!("PdhCollectQueryData failed: 0x{rc:08X}"));
            }

            let mut map: HashMap<u32, ProcessGpuUsage> = HashMap::new();

            // Utilization Percentage can legitimately exceed 100 across
            // engines; NOCAP100 keeps PDH from clamping it.
            let util_fmt = PDH_FMT(PDH_FMT_DOUBLE.0 | PDH_FMT_NOCAP100);
            for (name, value) in read_array_f64(self.engine_util, util_fmt) {
                fold_utilization(&mut map, &name, value);
            }

            for (name, value) in read_array_i64(self.mem_dedicated) {
                fold_memory(
                    &mut map,
                    &name,
                    MemoryCounter::Dedicated,
                    value.max(0) as u64,
                );
            }
            for (name, value) in read_array_i64(self.mem_shared) {
                fold_memory(&mut map, &name, MemoryCounter::Shared, value.max(0) as u64);
            }

            Ok(map)
        }
    }

    impl Drop for PdhQuery {
        fn drop(&mut self) {
            unsafe {
                let _ = PdhCloseQuery(self.query);
            }
        }
    }

    /// Fetch the formatted counter array for `counter`.
    ///
    /// PDH writes an array of `PDH_FMT_COUNTERVALUE_ITEM_W` followed by the
    /// instance name strings into a single caller-supplied buffer, so the
    /// buffer must be sized in bytes (not items) from the `PDH_MORE_DATA`
    /// probe. We over-allocate a `Vec<PDH_FMT_COUNTERVALUE_ITEM_W>` to get the
    /// right alignment for the item array.
    fn read_array(counter: PDH_HCOUNTER, format: PDH_FMT) -> Vec<(String, PDH_FMT_COUNTERVALUE)> {
        const ITEM: usize = std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>();
        let mut out = Vec::new();
        unsafe {
            let mut buffer: Vec<PDH_FMT_COUNTERVALUE_ITEM_W> =
                Vec::with_capacity(INITIAL_BUFFER_ITEMS);
            let mut size = (buffer.capacity() * ITEM) as u32;
            let mut count: u32 = 0;

            let mut rc = PdhGetFormattedCounterArrayW(
                counter,
                format,
                &mut size,
                &mut count,
                Some(buffer.as_mut_ptr()),
            );

            if rc == PDH_MORE_DATA {
                let items = (size as usize).div_ceil(ITEM) + 1;
                buffer = Vec::with_capacity(items);
                size = (buffer.capacity() * ITEM) as u32;
                count = 0;
                rc = PdhGetFormattedCounterArrayW(
                    counter,
                    format,
                    &mut size,
                    &mut count,
                    Some(buffer.as_mut_ptr()),
                );
            }

            if rc != ERROR_SUCCESS || count == 0 {
                return out;
            }

            let items = std::slice::from_raw_parts(buffer.as_ptr(), count as usize);
            for item in items {
                if item.szName.is_null() {
                    continue;
                }
                let Ok(name) = item.szName.to_string() else {
                    continue;
                };
                out.push((name, item.FmtValue));
            }
        }
        out
    }

    fn read_array_f64(counter: PDH_HCOUNTER, format: PDH_FMT) -> Vec<(String, f64)> {
        read_array(counter, format)
            .into_iter()
            .filter(|(_, v)| v.CStatus == ERROR_SUCCESS)
            .map(|(n, v)| (n, unsafe { v.Anonymous.doubleValue }))
            .collect()
    }

    fn read_array_i64(counter: PDH_HCOUNTER) -> Vec<(String, i64)> {
        read_array(counter, PDH_FMT_LARGE)
            .into_iter()
            .filter(|(_, v)| v.CStatus == ERROR_SUCCESS)
            .map(|(n, v)| (n, unsafe { v.Anonymous.largeValue }))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// PowerShell fallback (Windows only)
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
mod powershell {
    use super::{
        ProcessGpuUsage, classify_memory_counter, fold_memory, fold_utilization, instance_from_path,
        parse_ps_pair,
    };
    use std::collections::HashMap;
    use std::process::Command;

    /// Hide the console window when spawning children from a GUI app.
    fn hidden_command(program: &str) -> Command {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut cmd = Command::new(program);
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd
    }

    pub fn collect() -> HashMap<u32, ProcessGpuUsage> {
        let mut map: HashMap<u32, ProcessGpuUsage> = HashMap::new();

        if let Ok(output) = hidden_command("powershell")
            .args([
                "-NoProfile", "-NonInteractive", "-Command",
                "(Get-Counter '\\GPU Engine(*)\\Utilization Percentage').CounterSamples | ForEach-Object { $_.InstanceName + '=' + $_.CookedValue.ToString('F2') }",
            ])
            .output()
            && output.status.success()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if let Some((instance, value)) = parse_ps_pair(line) {
                    fold_utilization(&mut map, instance, value);
                }
            }
        }

        if let Ok(output) = hidden_command("powershell")
            .args([
                "-NoProfile", "-NonInteractive", "-Command",
                "(Get-Counter '\\GPU Process Memory(*)\\Dedicated Usage','\\GPU Process Memory(*)\\Shared Usage').CounterSamples | ForEach-Object { $_.Path + '=' + $_.CookedValue.ToString('F0') }",
            ])
            .output()
            && output.status.success()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if let Some((path, value)) = parse_ps_pair(line)
                    && let Some(instance) = instance_from_path(path)
                {
                    let counter = classify_memory_counter(path);
                    fold_memory(&mut map, instance, counter, value.max(0.0) as u64);
                }
            }
        }

        map
    }
}

// ---------------------------------------------------------------------------
// Public collector
// ---------------------------------------------------------------------------

/// Collects per-process GPU utilization and memory.
///
/// Uses the native PDH API when available, otherwise falls back to PowerShell
/// `Get-Counter`. On non-Windows platforms this is an inert stub.
pub struct PdhGpuCollector {
    data: HashMap<u32, ProcessGpuUsage>,
    source: PdhSource,
    #[cfg(target_os = "windows")]
    query: Option<windows_pdh::PdhQuery>,
    init_error: Option<String>,
}

impl Default for PdhGpuCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl PdhGpuCollector {
    #[cfg(target_os = "windows")]
    pub fn new() -> Self {
        match windows_pdh::PdhQuery::new() {
            Ok(q) => Self {
                data: HashMap::new(),
                source: PdhSource::NativePdh,
                query: Some(q),
                init_error: None,
            },
            Err(e) => Self {
                data: HashMap::new(),
                source: PdhSource::PowerShell,
                query: None,
                init_error: Some(e),
            },
        }
    }

    #[cfg(not(target_os = "windows"))]
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            source: PdhSource::Unavailable,
            init_error: None,
        }
    }

    /// Which backend produced the most recent data.
    #[allow(dead_code)]
    pub fn source(&self) -> PdhSource {
        self.source
    }

    /// Reason PDH could not be used, if it could not.
    #[allow(dead_code)]
    pub fn init_error(&self) -> Option<&str> {
        self.init_error.as_deref()
    }

    #[cfg(target_os = "windows")]
    pub fn refresh(&mut self) {
        if let Some(q) = self.query.as_mut() {
            match q.collect() {
                Ok(map) => {
                    self.data = map;
                    self.source = PdhSource::NativePdh;
                    return;
                }
                Err(e) => {
                    // Treat as permanent: drop the query and fall back.
                    self.init_error = Some(e);
                    self.query = None;
                }
            }
        }
        self.data = powershell::collect();
        self.source = PdhSource::PowerShell;
    }

    #[cfg(not(target_os = "windows"))]
    pub fn refresh(&mut self) {}

    pub fn per_process(&self) -> &HashMap<u32, ProcessGpuUsage> {
        &self.data
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_engine_instance() {
        let inst =
            parse_gpu_instance("pid_12345_luid_0x00000000_0x0000D1B5_phys_0_eng_3_engtype_3D")
                .expect("should parse");
        assert_eq!(inst.pid, 12345);
        assert_eq!(inst.luid.as_deref(), Some("0x00000000_0x0000d1b5"));
        assert_eq!(inst.phys, Some(0));
        assert_eq!(inst.eng, Some(3));
        assert_eq!(inst.engtype.as_deref(), Some("3d"));
    }

    #[test]
    fn parses_process_memory_instance() {
        let inst =
            parse_gpu_instance("pid_4242_luid_0x00000000_0x0000ABCD_phys_1").expect("should parse");
        assert_eq!(inst.pid, 4242);
        assert_eq!(inst.phys, Some(1));
        assert_eq!(inst.eng, None);
        assert_eq!(inst.engtype, None);
    }

    #[test]
    fn parses_multiword_engtype() {
        let inst =
            parse_gpu_instance("pid_1_luid_0x0_0x1_phys_0_eng_5_engtype_Video_Decode").unwrap();
        assert_eq!(inst.engtype.as_deref(), Some("video_decode"));
    }

    #[test]
    fn tolerates_missing_trailing_fields() {
        let inst = parse_gpu_instance("pid_777").unwrap();
        assert_eq!(inst.pid, 777);
        assert_eq!(inst.luid, None);
        assert_eq!(inst.phys, None);
    }

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(parse_gpu_instance("  pid_55_phys_0  ").unwrap().pid, 55);
    }

    #[test]
    fn rejects_malformed_instances() {
        let bad = [
            "",
            "   ",
            "pid_",
            "pid_abc",
            "pid__luid_0x0_0x1",
            "_pid_1234",
            "engtype_3D",
            "Total",
            "luid_0x0_0x1_phys_0",
            "pid-1234_phys_0",
            "pid_12ab_phys_0",
        ];
        for case in bad {
            assert_eq!(parse_gpu_instance(case), None, "should reject {case:?}");
        }
    }

    #[test]
    fn pid_overflow_is_rejected() {
        assert_eq!(parse_gpu_instance("pid_99999999999999999999_phys_0"), None);
    }

    #[test]
    fn extract_pid_matches_parse() {
        assert_eq!(extract_pid("pid_31337_luid_0x0_0x1_phys_1"), Some(31337));
        assert_eq!(extract_pid("Total"), None);
    }

    #[test]
    fn extract_pid_from_path_handles_full_paths() {
        let path = r"\\DESKTOP\GPU Process Memory(pid_9012_luid_0x00000000_0x0000D1B5_phys_0)\Dedicated Usage";
        assert_eq!(extract_pid_from_path(path), Some(9012));
        assert_eq!(
            extract_pid_from_path(r"\\HOST\Processor(_Total)\% Idle Time"),
            None
        );
        assert_eq!(extract_pid_from_path(""), None);
        assert_eq!(extract_pid_from_path("pid_x"), None);
    }

    #[test]
    fn unwraps_instance_from_counter_path() {
        // Regression: the PowerShell fallback reports .Path, not .InstanceName.
        // Feeding the raw path to the instance parser silently dropped all
        // memory samples.
        let path = r"\\DESKTOP\gpu process memory(pid_9012_luid_0x0_0xd1b5_phys_0)\dedicated usage";
        assert_eq!(
            instance_from_path(path),
            Some("pid_9012_luid_0x0_0xd1b5_phys_0")
        );
        assert_eq!(parse_gpu_instance(instance_from_path(path).unwrap()).unwrap().pid, 9012);
        // A raw path must NOT parse as an instance name.
        assert_eq!(parse_gpu_instance(path), None);

        assert_eq!(instance_from_path("no parens here"), None);
        assert_eq!(instance_from_path(r"\obj()\counter"), None);
        assert_eq!(instance_from_path(""), None);
        assert_eq!(instance_from_path(r"\Processor(_Total)\% Idle"), Some("_Total"));
    }

    #[test]
    fn powershell_memory_path_round_trip() {
        let line = r"\\HOST\gpu process memory(pid_555_luid_0x0_0x1_phys_1)\shared usage=2048";
        let (path, value) = parse_ps_pair(line).unwrap();
        let instance = instance_from_path(path).unwrap();
        let mut map = HashMap::new();
        fold_memory(&mut map, instance, classify_memory_counter(path), value as u64);
        assert_eq!(map[&555].shared_vram, 2048);
        assert_eq!(map[&555].dedicated_vram, 0);
    }

    #[test]
    fn classifies_memory_counters() {
        let base = "(pid_1_luid_0x0_0x1_phys_0)";
        assert_eq!(
            classify_memory_counter(&format!(r"\GPU Process Memory{base}\Dedicated Usage")),
            MemoryCounter::Dedicated
        );
        assert_eq!(
            classify_memory_counter(&format!(r"\GPU Process Memory{base}\Shared Usage")),
            MemoryCounter::Shared
        );
        assert_eq!(
            classify_memory_counter(&format!(r"\GPU Process Memory{base}\Local Usage")),
            MemoryCounter::Local
        );
        assert_eq!(
            classify_memory_counter(&format!(r"\GPU Process Memory{base}\Non Local Usage")),
            MemoryCounter::NonLocal
        );
        assert_eq!(
            classify_memory_counter(&format!(r"\GPU Process Memory{base}\Total Committed")),
            MemoryCounter::Other
        );
    }

    #[test]
    fn utilization_takes_max_across_engines() {
        let mut map = HashMap::new();
        fold_utilization(&mut map, "pid_10_luid_0x0_0x1_phys_0_eng_0_engtype_3D", 12.0);
        fold_utilization(
            &mut map,
            "pid_10_luid_0x0_0x1_phys_0_eng_1_engtype_Copy",
            44.5,
        );
        fold_utilization(&mut map, "pid_10_luid_0x0_0x1_phys_0_eng_2_engtype_3D", 3.0);
        assert_eq!(map[&10].utilization, 44.5);
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn utilization_ignores_junk() {
        let mut map = HashMap::new();
        fold_utilization(&mut map, "Total", 99.0);
        fold_utilization(&mut map, "pid_10_phys_0", f64::NAN);
        fold_utilization(&mut map, "pid_10_phys_0", -5.0);
        assert!(map.is_empty());
    }

    #[test]
    fn memory_sums_across_adapters() {
        // Dual-GPU: one process holding VRAM on both cards.
        let mut map = HashMap::new();
        fold_memory(
            &mut map,
            "pid_2000_luid_0x0_0xAAA_phys_0",
            MemoryCounter::Dedicated,
            2 * 1024 * 1024 * 1024,
        );
        fold_memory(
            &mut map,
            "pid_2000_luid_0x0_0xBBB_phys_1",
            MemoryCounter::Dedicated,
            1024 * 1024 * 1024,
        );
        fold_memory(
            &mut map,
            "pid_2000_luid_0x0_0xAAA_phys_0",
            MemoryCounter::Shared,
            512 * 1024 * 1024,
        );
        // Local / NonLocal must not be folded into dedicated.
        fold_memory(
            &mut map,
            "pid_2000_luid_0x0_0xAAA_phys_0",
            MemoryCounter::Local,
            99 * 1024 * 1024 * 1024,
        );

        let e = &map[&2000];
        assert_eq!(e.dedicated_vram, 3 * 1024 * 1024 * 1024);
        assert_eq!(e.shared_vram, 512 * 1024 * 1024);
    }

    #[test]
    fn memory_saturates_instead_of_overflowing() {
        let mut map = HashMap::new();
        fold_memory(&mut map, "pid_1_phys_0", MemoryCounter::Dedicated, u64::MAX);
        fold_memory(&mut map, "pid_1_phys_0", MemoryCounter::Dedicated, 1024);
        assert_eq!(map[&1].dedicated_vram, u64::MAX);
    }

    #[test]
    fn parses_powershell_pairs() {
        assert_eq!(
            parse_ps_pair("pid_1234_luid_0x0_0x1_phys_0_eng_0_engtype_3D=12.50"),
            Some(("pid_1234_luid_0x0_0x1_phys_0_eng_0_engtype_3D", 12.5))
        );
        assert_eq!(
            parse_ps_pair(r"\\HOST\gpu process memory(pid_1_phys_0)\dedicated usage=4096"),
            Some((
                r"\\HOST\gpu process memory(pid_1_phys_0)\dedicated usage",
                4096.0
            ))
        );
        assert_eq!(parse_ps_pair("no-equals-sign"), None);
        assert_eq!(parse_ps_pair("pid_1=notanumber"), None);
        assert_eq!(parse_ps_pair(""), None);
    }

    /// Live smoke test: exercises the real PDH API on this machine.
    /// Ignored by default so CI on Linux/macOS stays green.
    #[test]
    #[ignore = "requires Windows with a GPU; run with --ignored"]
    fn live_pdh_returns_data() {
        let mut c = PdhGpuCollector::new();
        c.refresh();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let t0 = std::time::Instant::now();
        c.refresh();
        println!("source: {} | refresh took {:?}", c.source().label(), t0.elapsed());
        if let Some(e) = c.init_error() {
            println!("init error: {e}");
        }
        let mut rows: Vec<_> = c.per_process().values().cloned().collect();
        rows.sort_by_key(|r| std::cmp::Reverse(r.dedicated_vram));
        for r in rows.iter().take(20) {
            println!(
                "pid {:>7}  util {:>7.2}%  dedicated {:>9.1} MiB  shared {:>9.1} MiB",
                r.pid,
                r.utilization,
                r.dedicated_vram as f64 / (1024.0 * 1024.0),
                r.shared_vram as f64 / (1024.0 * 1024.0),
            );
        }
        assert!(!rows.is_empty(), "expected at least one GPU process");
    }
}

# Changelog

## [0.5.0] - 2026-08-17 - Render-GPU selection, wgpu backend, ALIA Labs

### Added

- **User-selectable render GPU.** Pane can now render its own UI on a chosen
  GPU instead of whatever the driver picks. Three ways to choose, highest
  priority first: the `--gpu <name-substring>` CLI flag, the "Render"
  dropdown in the sidebar (persisted as `render_gpu` in config.json), or
  automatic (default). Matching is a case-insensitive substring of the wgpu
  adapter name. The adapter is fixed at startup, so a dropdown change takes
  effect after restart. An unmatched name or failed adapter enumeration
  falls back to automatic selection instead of refusing to start.

### Changed

- **Rendering backend switched from glow (OpenGL) to wgpu.** Required for
  adapter selection; the adapter is picked via egui-wgpu's
  `native_adapter_selector` hook.
- **Rebranded to ALIA Labs.** Repository moved to
  github.com/ALIA-Engineering/Pane. Sidebar credit, snapshot footer, crate
  metadata and the AppUserModelID (now `ALIA.Pane`) updated. Existing
  taskbar pins may need re-pinning because of the AppUserModelID change.

## [0.4.1] - 2026-08-04 - Taskbar icon, PDH docs, NVML perf

### Fixed

- **Taskbar icon missing when the app runs.** Pane now registers a stable
  AppUserModelID (`TxsharDev.Pane`) at startup via
  `SetCurrentProcessExplicitAppUserModelID`, so Windows can associate the
  window with the app for taskbar grouping and icon display.
- **`pdh.rs` doc comment claimed the PowerShell fallback costs ~100 ms per
  refresh**; each `Get-Counter` invocation actually costs ~2.8 s. The comment
  now matches the measured figure.
- **NVML process-name lookup constructed a fresh `sysinfo::System` per PID per
  refresh** (~120 constructions per second at 500 ms refresh across 30
  processes). One shared `System`, refreshed once per tick, is used instead.

## [0.4.0] - 2026-07-24 - Version reset, real PDH, tests

**Version renumbered from 4.0.1 down to 0.4.0.** The 1.0.0 -> 4.0.0 jump happened
across three commits over two days and did not mean anything: there was no stable
public API, no release cadence, and no compatibility promise behind those numbers.
Pane is pre-1.0 software. `0.4.0` is deliberately chosen to keep the ordinal
"fourth milestone" sense of the old numbering while being honest about maturity.
The `v4.0.0` git tag is left alone; it stays as a historical marker.

The entries below this release are preserved verbatim as they were written. Some
of them overstate what shipped (see 3.2.0's "Per-process GPU metrics via PDH",
which was actually PowerShell). They are kept for the record rather than rewritten.

### What actually exists today

- **Monitoring:** GPU (NVIDIA, via NVML), CPU, RAM, disk, network, processes,
  dashboard, VRAM headroom calculator, performance snapshot export.
- **Per-process GPU %, dedicated VRAM and shared VRAM:** Windows only, via PDH.
- **Hardware control:** exactly one - the NVML power limit. Nothing else.
- **Two binaries:** `pane` (egui GUI) and `pane-tui` (ratatui TUI, optional feature).

### Added

- **Real PDH implementation.** `src/metrics/gpu/pdh.rs` now calls the Windows
  PDH API directly through `windows-rs`: `PdhOpenQueryW`,
  `PdhAddEnglishCounterW`, `PdhCollectQueryData`, `PdhGetFormattedCounterArrayW`
  against `\GPU Engine(*)\Utilization Percentage` and
  `\GPU Process Memory(*)\Dedicated Usage` / `Shared Usage`. One query is opened
  at startup and re-sampled per tick. Measured at ~1.8 ms per refresh on a
  dual-GPU box with ~780 counter instances, replacing two PowerShell
  `Get-Counter` invocations that cost ~2.8 s each.
- **PDH instance-name parser** for `pid_<PID>_luid_<hi>_<lo>_phys_<N>[_eng_<N>_engtype_<NAME>]`,
  exposed as pure functions so it can be tested without a GPU.
- **Documented aggregation semantics:** utilization is the max across engines
  for a PID; memory is summed across physical adapters.
- **PowerShell `Get-Counter` retained as an explicit fallback**, used only if
  PDH initialisation or collection fails. `PdhGpuCollector::source()` reports
  which path produced the data.
- **58 unit tests** (previously zero), covering: PDH and PowerShell counter
  parsing including malformed input, NVML graphics/compute process merging and
  deduplication, VRAM/LLM-quantization calculator maths, single- vs dual-GPU
  VRAM aggregation and split verdicts, byte formatting, the history ring
  buffer, and process sorting/filtering.
- **Two live hardware tests** marked `#[ignore]` (`live_pdh_returns_data`,
  `live_nvml_reports_devices`) for manual verification on a real machine.
- **CI now runs `cargo test`** on Windows, Linux and macOS x64, and clippy over
  the `pane-tui` binary as well as `pane`.

### Fixed

- **PowerShell fallback silently returned no memory data.** It fed a full
  counter *path* to a parser that expects a bare instance name, so every
  `GPU Process Memory` sample was dropped. Added `instance_from_path` and a
  regression test.
- **Reduced compiler warnings from 24 to 0** on both binaries
  (`cargo clippy --all-targets` is clean). The 24 were all
  `float_literal_f32_fallback` future-incompatibility warnings in the egui
  layer; the rest were unused `Result`s, a collapsible match arm, and dead code
  that is genuinely GUI-only and is now marked as such.
- TUI process kill no longer discards its error; failures surface in the status bar.

### Changed

- Fan-speed and clock-offset controls are now labelled **"Not implemented"**
  instead of "Requires NVAPI (coming soon)". They were never implemented and
  there is no NVAPI code in the repo. They remain disabled and inert.
- `merge_processes` in the NVML backend replaces inline dedup logic; a PID
  appearing in both the graphics and compute lists is reported once, memory
  summed, and sorted by VRAM descending with a stable PID tiebreak.
- README rewritten so every claim matches the code: the "hardware controls"
  plural is gone, the PDH claim is now true, the PowerShell overhead figure is
  corrected, and the Limitations section is substantially extended.

### Removed

- **`amd` cargo feature.** It was empty - no AMD code existed behind it.
- **`src/tray.rs`.** It contained two comment lines and no implementation. The
  `mod tray;` declaration went with it.
- **`Win32_Graphics_Direct3D`** windows-rs feature - never used.
- **`mod ui` from `src/main.rs`.** The GUI binary was compiling the entire
  ratatui tree behind `#[allow(dead_code)]`. `src/ui/` is still live: it is the
  TUI, used by the `pane-tui` binary only.

### Not done

- **NVAPI fan and clock control.** `NvAPI_GPU_SetCoolerLevels` and
  `NvAPI_GPU_SetPstates20` are undocumented, require privileged access, and
  cannot be verified safely on a live workstation. Documentation was corrected
  instead of shipping something unverified.
- **AMD / Intel device-level metrics.** No ADL/ADLX/Level Zero code was added.

## [4.0.1] - 2026-05-26

### Fixed
- **Windows executable icon** - embedded .ico resource via winresource so the icon shows on taskbar pins, file explorer, and downloaded .exe (was missing, only the runtime window icon worked)

## [4.0.0] - 2026-05-26 - First Public Release

### Added
- **Custom app icon** - logo.png embedded as window/taskbar icon
- **Airstrike Bold font** - custom branding font for PANE logo in sidebar and loading screen
- "SYSTEM MONITOR" subtitle under sidebar logo

### Changed
- **Color palette overhaul** - sky blue accent (#38BDF8), richer emerald/amber/rose/violet, deeper blacks, new card_bg layer for depth
- **Font sizing** - body 14px, headings 20px, monospace 13px, bigger stat card values (26px monospace)
- **Sidebar redesign** - centered logo, full-width theme toggle button, full-width nav labels (removed badge abbreviations), cleaner spacing
- **Loading screen** - 80px Airstrike font, centered branding
- Section headers enlarged (15px, thicker accent bar)
- Button padding increased for better touch targets

### Fixed
- All clippy warnings resolved (removed unused control_card closure, send_gpu_command method, icon method)
- Collapsed nested if statements in PDH collector and GPU control
- Zero warnings on release build

## [3.7.0] - 2026-05-25

### Added
- **GPU Control admin UX** - yellow warning banner when not running as admin, explains how to elevate
- Power limit slider disabled when not elevated (shows "Needs admin" badge)
- Fan/clock controls show "Requires NVAPI (coming soon)" instead of fake sliders
- Apply button disabled and shows hover tooltip when not admin
- NVML badge on power limit when elevated and functional

### Changed
- **README completely rewritten** - reflects GUI app, accurate feature list, platform support matrix, PDH accuracy notes, honest cross-platform scope
- GPU Control Apply button renamed to "Apply Power Limit" for clarity
- Unused control card helper removed

## [3.6.0] - 2026-05-25

### Added
- **Window size persistence** - window dimensions auto-saved to config on resize, restored on startup

### Fixed
- **No more console window flashing** - PDH collector and taskkill/net commands now use CREATE_NO_WINDOW flag to suppress child process console windows
- **No more console window** - `windows_subsystem = "windows"` applied unconditionally, Pane launches as a pure GUI app
- Window size saved with 10px threshold to avoid config spam on minor resizes

### Removed
- System tray support (disabled due to Win32 message loop conflicts with eframe/winit - will revisit in future version)

## [3.4.0] - 2026-05-25

### Added
- **VRAM Headroom Calculator** - shows what LLM models fit in your available VRAM across GPUs, with quant sizes (Q4/Q5/Q8/FP16), KV cache estimates, max context predictions, and split-GPU indicators
- **Performance Snapshot Exporter** - one-click button generates clean text report of all system metrics (GPU, CPU, RAM, disk, processes), copy to clipboard or save to desktop. Ready for Reddit/GitHub/Discord
- 9 popular models in VRAM calculator (Llama 8B-405B, Qwen 14B-80B, Mixtral, DeepSeek V3)

## [3.3.0] - 2026-05-25

### Added
- **Config file persistence** - saves theme, refresh rate, selected GPU, window size, sidebar width, default panel to `%APPDATA%/pane/config.json` (Windows) or `~/.config/pane/config.json` (Linux/Mac)
- Settings auto-saved on theme change, loaded on startup
- Configurable refresh rate (default 500ms)

### Dependencies
- Added `serde`, `serde_json`, `dirs` for config persistence

## [3.2.0] - 2026-05-25

### Added
- **Real GPU power limit control** - Apply button sends SetPowerManagementLimit to GPU via NVML with success/error feedback
- **GPU command channel** - UI thread sends control commands to background metric thread safely
- **Per-process GPU metrics via PDH** - Windows Performance Counters fill in GPU% and VRAM columns in process table (vendor-agnostic, no admin)
- **Cross-platform CI** - GitHub Actions workflow builds for Windows x64, Linux x64, macOS x64 + ARM64 with auto-release on tags

### Changed
- GPU Control Apply button now functional (power limit only - fan/clocks noted as requiring NVAPI)
- Background thread processes GPU commands before each metric collection cycle

## [3.1.0] - 2026-05-25

### Added
- **GPU process table** - shows all processes using the selected GPU directly on the GPU panel (PID, name, type GFX/CMP, VRAM usage), sorted by VRAM descending
- **Process web search** - `?` button per process row opens Google search ("what is [name] Windows process") in default browser
- **Graceful close** - "Close" button sends WM_CLOSE/SIGTERM before resorting to force kill
- **Kill error feedback** - red banner with actual error message when kill fails (e.g. "Access denied - run Pane as administrator")
- **Success feedback** - green banner confirming "PID closed" or "PID killed" after successful action
- **Admin detection** - warning shown in kill confirmation when not running elevated
- **Status message system** - dismissible banners for action feedback across both GPU and Processes panels

### Fixed
- Kill confirmation no longer disappears on metric refresh (confirm_kill state preserved across updates)
- GPU process names resolved via sysinfo instead of showing raw PIDs
- NVML UsedGpuMemory enum properly handled (Used vs Unavailable)

## [3.0.0] - 2026-05-25

### Added
- **Native GUI** - egui/eframe GPU-accelerated window replaces TUI as default (TUI still available as `pane-tui`)
- **Dark / Light / System theme** - toggle in sidebar, full palette swap (text, backgrounds, charts, borders all adapt)
- **Real-time charts** - filled area graphs with grid lines, Y-axis labels, and max value indicators
- **Click-to-copy** - click any metric value to copy to clipboard, hover for tooltip with full precision
- **Copy confirmation** - tooltip shows "Copied!" on click
- **Sidebar navigation** - clean badge labels (GP, CP, ME, etc.), GPU selector, theme toggle, footer with author link
- **Loading screen** - spinner with branding while first metrics are collected
- **Background metric thread** - collection runs off the UI thread, no jank or frame drops
- **Process sort pills** - rounded pill buttons with accent highlight, ascending/descending toggle per column
- **Process kill button** - per-row `x` button with hover tooltip and red confirmation banner

### Changed
- Process filter redesigned with placeholder text and fixed-width input
- Chart rendering uses filled polygons with semi-transparent area under the line
- All panels use dynamic palette (`theme::p()`) instead of hardcoded dark constants
- Sort indicators changed from Unicode arrows to `^` / `v` ASCII for font compatibility
- GPU Control sliders are native egui sliders with suffix labels
- Sidebar icons replaced with monospace text badges for cross-platform font compatibility
- Binary size: ~5 MB (GUI) vs ~1 MB (TUI)

### Fixed
- Light mode text/icon contrast - all text properly dark on light backgrounds
- All clippy warnings resolved (zero warnings on release build)
- Chart grid lines and Y-axis labels respect active theme colors
- Process table kill button no longer uses broken Unicode glyph

## [2.0.0] - 2026-05-25

### Added
- **Dashboard overview** - all metrics at a glance: GPU cards, CPU, RAM, disk, network, top processes in one view
- **GPU Control panel** - fan speed, power limit, core/memory clock offset with interactive sliders
- **Braille sparklines** - 8x vertical resolution graphs across all panels
- **Process kill** - select process, press `k`, confirm with `y`
- **Process selection** - arrow key navigation with highlighted row
- **Dual GPU support** - both GPUs detected and displayed (RTX 5090 + RTX 4090)
- **GPU history tracking** - VRAM, temperature, and power draw histories
- **Panel jump keys** - `h` dashboard, `g` gpu, `c` cpu, `m` memory, `d` disk, `n` network, `p` processes, `x` gpu control
- **Kill confirmation** - red highlight + y/n prompt before killing a process

### Changed
- Default panel is now Dashboard (was GPU)
- UI overhauled: consistent dark borders, color gradients (green/yellow/red), Unicode sort arrows
- History buffer increased from 120 to 200 samples
- Status bar shows context-sensitive keybindings per panel

### Fixed
- All clippy warnings resolved
- Proper iterator usage for slider rendering
- `div_ceil` used instead of manual ceiling division

## [1.0.0] - 2026-05-25

### Added
- Project initialized
- Core architecture designed: ratatui + crossterm + sysinfo + nvml-wrapper + windows-rs
- Cross-platform foundation (Windows, Linux, macOS)
- Deep GPU metrics pipeline design (PDH, NVML, NVAPI, ADLX)
- README with full project vision, architecture, and roadmap
- MIT License

---

*Pane follows [Semantic Versioning](https://semver.org/).*

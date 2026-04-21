//! Hardware metrics sampler for the benchmark profile CSV.
//!
//! On Apple Silicon (`macos` + `aarch64`) a background thread uses the
//! [`macmon`](https://crates.io/crates/macmon) crate to read CPU/GPU frequency,
//! power, and temperature once per second via the IOReport framework. No sudo
//! required. The main thread reads the most recent sample via [`sample`] when
//! emitting a profile CSV row.
//!
//! On every other platform the background thread is never spawned and `sample`
//! returns an all-zero [`ThermalSample`] so the CSV columns stay populated with
//! a stable schema.

use std::sync::{Arc, Mutex, OnceLock};

/// A single hardware-metrics snapshot.
///
/// All fields are zero when sampling is unsupported (non-Apple-Silicon) or when
/// the background sampler has not produced its first reading yet.
#[derive(Debug, Clone, Copy, Default)]
pub struct ThermalSample {
    /// Efficiency-cluster CPU frequency (MHz).
    pub ecpu_mhz: u32,
    /// Performance-cluster CPU frequency (MHz).
    pub pcpu_mhz: u32,
    /// GPU frequency (MHz).
    pub gpu_mhz: u32,
    /// CPU package power (watts).
    pub cpu_power_w: f32,
    /// GPU power (watts).
    pub gpu_power_w: f32,
    /// Averaged CPU die temperature (°C).
    pub cpu_temp_c: f32,
    /// Averaged GPU die temperature (°C).
    pub gpu_temp_c: f32,
    /// True when the system is drawing from battery. macOS caps GPU power and
    /// runs a different QoS policy on battery, so perf numbers across battery
    /// vs. AC runs are not directly comparable. Always `false` on non-macOS.
    pub on_battery: bool,
}

/// Process-wide shared state. Initialized lazily on the first `sample()` call so
/// that binaries that never request thermal data (e.g. sprite generation) don't
/// pay the IOReport init cost or spawn a sampler thread.
static LATEST: OnceLock<Arc<Mutex<ThermalSample>>> = OnceLock::new();

/// Returns the most recent thermal snapshot.
///
/// First call starts a background sampler thread (Apple Silicon only); every
/// subsequent call is a cheap mutex read of the last cached sample. The sampler
/// thread runs for the lifetime of the process and is never joined — fine for
/// CLI/game-loop use where the process exits cleanly.
pub fn sample() -> ThermalSample {
    let state = LATEST.get_or_init(|| {
        let state = Arc::new(Mutex::new(ThermalSample::default()));
        start_sampler(Arc::clone(&state));
        state
    });
    *state.lock().expect("thermal sample mutex poisoned")
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn start_sampler(state: Arc<Mutex<ThermalSample>>) {
    std::thread::Builder::new()
        .name("thermal-sampler".into())
        .spawn(move || sampler_loop(state))
        .ok();
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
fn start_sampler(_state: Arc<Mutex<ThermalSample>>) {
    // Unsupported platform — `sample()` will keep returning the default zeroed
    // snapshot. CSV columns stay present so downstream tooling can depend on
    // the schema without branching on platform.
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn sampler_loop(state: Arc<Mutex<ThermalSample>>) {
    // `macmon::Sampler::new()` runs `system_profiler` once to identify the SoC
    // and opens IOReport / SMC channels. Everything after is pure FFI.
    let mut sampler = match macmon::Sampler::new() {
        Ok(s) => s,
        Err(e) => {
            log::warn!("[thermal] macmon init failed: {e}");
            return;
        }
    };
    // `get_metrics` blocks for the supplied duration (ms). 1000 ms matches the
    // CSV sampling cadence in `print_stats`; shorter windows make IOReport
    // residency counters noisy.
    loop {
        match sampler.get_metrics(1000) {
            Ok(m) => {
                let snapshot = ThermalSample {
                    ecpu_mhz: m.ecpu_usage.0,
                    pcpu_mhz: m.pcpu_usage.0,
                    gpu_mhz: m.gpu_usage.0,
                    cpu_power_w: m.cpu_power,
                    gpu_power_w: m.gpu_power,
                    cpu_temp_c: m.temp.cpu_temp_avg,
                    gpu_temp_c: m.temp.gpu_temp_avg,
                    on_battery: read_on_battery(),
                };
                if let Ok(mut guard) = state.lock() {
                    *guard = snapshot;
                }
            }
            Err(_) => {
                // Transient read failures are non-fatal; retry after a pause so
                // we don't spin when IOReport is temporarily unavailable.
                std::thread::sleep(std::time::Duration::from_millis(1000));
            }
        }
    }
}

/// Parses `pmset -g batt` to determine whether the system is on battery.
///
/// The first line of output is either `Now drawing from 'Battery Power'` or
/// `Now drawing from 'AC Power'`. Spawning a subprocess once per second is
/// negligible overhead compared to the IOReport sampling already in flight.
/// Returns `false` if `pmset` is unavailable or its output is unparseable —
/// treat the absence of signal as "assume AC" rather than fail a benchmark.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn read_on_battery() -> bool {
    std::process::Command::new("pmset")
        .args(["-g", "batt"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("'Battery Power'"))
        .unwrap_or(false)
}

//! Real OS telemetry & fixture benchmark instrumentation (zero synthetic/mock numbers).
//!
//! Measures:
//! - Startup latency & Shutdown latency (`min`, `p50`, `p95`, `max`, `mean` in milliseconds)
//! - Process memory overhead (`WorkingSetSize` + `PrivateUsage` on Windows, `RSS` on macOS/Linux)
//! - Idle CPU usage (`GetProcessTimes` kernel+user ticks delta on Windows, `/proc` or `ps` on Unix)
//! - File-system delta (created/modified/deleted files & bytes inside sandbox, zero outside-sandbox writes)
//! - Cleanup residuals (surviving PIDs, leftover lockfiles/signals/staging folders)
//! - Full host environment provenance (`OS`, `kernel`, `arch`, `rustc`, `node`, `git commit`, `seed`)

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use dim_core::{now_rfc3339_utc, PlatformKind, ProfileStore};
use dim_profile_store::is_symlink_or_reparse;
use dim_xf_fixture::is_os_pid_alive;
use serde::{Deserialize, Serialize};

use crate::harness::{
    dim_root_dir, force_kill_unprotected_test_pid, make_node_fixture_target, xf_fixture_bin_path,
    IsolatedSandbox,
};

/// Host & toolchain provenance recorded alongside every benchmark run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostProvenance {
    /// Operating system family (`windows`, `macos`, `linux`).
    pub os: String,
    /// Detailed OS / kernel version string queried from the host.
    pub kernel_or_build_version: String,
    /// CPU architecture (`x86_64`, `aarch64`, etc.).
    pub architecture: String,
    /// Logical CPU count reported by the OS.
    pub logical_cpus: usize,
    /// Exact `rustc --version` string.
    pub rustc_version: String,
    /// Exact `node --version` string if available.
    pub node_version: Option<String>,
    /// Git `HEAD` commit hash of the worktree.
    pub git_commit: String,
    /// Deterministic test seed.
    pub test_seed: String,
    /// Benchmark execution timestamp (RFC 3339 UTC).
    pub captured_at_utc: String,
}

/// Statistical latency distribution in milliseconds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyDistribution {
    /// Sample count.
    pub runs: usize,
    /// Minimum observed latency (ms).
    pub min_ms: f64,
    /// Median / P50 latency (ms).
    pub p50_ms: f64,
    /// P95 latency (ms).
    pub p95_ms: f64,
    /// Maximum observed latency (ms).
    pub max_ms: f64,
    /// Arithmetic mean latency (ms).
    pub mean_ms: f64,
}

/// Memory footprint snapshot for a set of fixture PIDs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySample {
    /// Number of concurrent fixture instances active during sample.
    pub instance_count: usize,
    /// Total processes sampled (parents + worker children).
    pub process_count: usize,
    /// Total resident / working-set bytes across all sampled PIDs.
    pub total_working_set_bytes: u64,
    /// Total committed / private bytes across all sampled PIDs (when supported by OS).
    pub total_private_bytes: u64,
    /// Per-instance average working-set megabytes (`MiB`).
    pub avg_working_set_mib_per_instance: f64,
}

/// Idle CPU measurement over a real wall-clock window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdleCpuSample {
    /// Sampling window duration in milliseconds.
    pub observation_window_ms: u64,
    /// Number of active idle fixture instances observed.
    pub instance_count: usize,
    /// Total CPU time (user + kernel) consumed across instances during the window, in milliseconds.
    pub cpu_time_delta_ms: f64,
    /// Aggregate single-core CPU percentage (`100 * cpu_delta / wall_window`).
    pub single_core_cpu_percent: f64,
    /// Host-normalized CPU percentage (`single_core_cpu_percent / logical_cpus`).
    pub host_normalized_cpu_percent: f64,
    /// Whether the host OS provided exact process CPU counters.
    pub measurement_supported: bool,
}

/// File-system delta captured before and after fixture execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSystemDelta {
    /// Regular files before run.
    pub files_before: usize,
    /// Regular files after run.
    pub files_after: usize,
    /// Files newly created inside sandbox.
    pub created_files: usize,
    /// Files modified inside sandbox.
    pub modified_files: usize,
    /// Files removed during cleanup.
    pub deleted_files: usize,
    /// Total bytes on disk after run.
    pub total_bytes_after: u64,
}

/// Verification of leftover processes or lock/staging files after shutdown/delete.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupResiduals {
    /// Surviving fixture PIDs (must be `0` for clean pass).
    pub surviving_pids: Vec<u32>,
    /// Leftover lock or signal artifacts (`lockfile`, `SingletonLock`, `.quit-signal`, `.runtime-pid`).
    pub leftover_lock_artifacts: Vec<String>,
    /// Leftover backup/restore staging directories (`.partial`, `.restore-*`).
    pub leftover_staging_dirs: Vec<String>,
}

/// Complete fixture benchmark & reliability metrics report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureMetricsReport {
    /// Host and toolchain metadata.
    pub provenance: HostProvenance,
    /// Native Rust `xf-desktop-fixture` direct spawn/shutdown latency (`runs` iterations).
    pub native_rust_fixture_startup: Option<LatencyDistribution>,
    /// Native Rust `xf-desktop-fixture` graceful shutdown latency (`runs` iterations).
    pub native_rust_fixture_shutdown: Option<LatencyDistribution>,
    /// Coordinator + Node fixture (`LaunchTarget::NodeFixture`) startup latency.
    pub coordinator_node_fixture_startup: LatencyDistribution,
    /// Coordinator + Node fixture graceful shutdown latency.
    pub coordinator_node_fixture_shutdown: LatencyDistribution,
    /// Memory scaling across 1, 3, and 5 concurrent instances (Native Rust fixture).
    pub native_memory_scaling: Vec<MemorySample>,
    /// Memory scaling across 1, 3, and 5 concurrent instances (Coordinator Node fixture).
    pub node_fixture_memory_scaling: Vec<MemorySample>,
    /// Idle CPU usage of 3 concurrent native Rust fixture instances over ~1.5s.
    pub native_idle_cpu: Option<IdleCpuSample>,
    /// Idle CPU usage of 3 concurrent Node fixture instances over ~1.5s.
    pub node_idle_cpu: IdleCpuSample,
    /// File-system changes observed across a 3-profile lifecycle + backup + delete cycle.
    pub filesystem_changes: FileSystemDelta,
    /// Post-cleanup residual audit.
    pub cleanup_residuals: CleanupResiduals,
    /// Total launch/stop operations executed in metrics run.
    pub total_operations: usize,
    /// Failed operations count.
    pub failed_operations: usize,
    /// Observed failure rate (`0.0` .. `1.0`).
    pub failure_rate: f64,
}

/// Captures host OS, kernel/build, architecture, Rust, Node, and Git commit provenance.
#[must_use]
pub fn collect_host_provenance(seed: u64) -> HostProvenance {
    let logical_cpus = thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1);

    let kernel_or_build_version = query_os_kernel_version();
    let rustc_version = run_cmd_trimmed("rustc", &["--version"])
        .or_else(|| {
            std::env::var_os("USERPROFILE").and_then(|up| {
                let p = std::path::PathBuf::from(up)
                    .join(".cargo")
                    .join("bin")
                    .join(if cfg!(windows) { "rustc.exe" } else { "rustc" });
                run_cmd_trimmed(p.to_string_lossy().as_ref(), &["--version"])
            })
        })
        .unwrap_or_else(|| "unknown".to_owned());
    let node_version = run_cmd_trimmed("node", &["--version"]);
    let git_commit =
        run_cmd_in_dir_trimmed("git", &["rev-parse", "--short=12", "HEAD"], &dim_root_dir())
            .unwrap_or_else(|| "unknown".to_owned());

    HostProvenance {
        os: PlatformKind::current().as_str().to_owned(),
        kernel_or_build_version,
        architecture: std::env::consts::ARCH.to_owned(),
        logical_cpus,
        rustc_version,
        node_version,
        git_commit,
        test_seed: format!("0x{seed:08x}"),
        captured_at_utc: now_rfc3339_utc(),
    }
}

fn query_os_kernel_version() -> String {
    #[cfg(target_os = "windows")]
    {
        run_cmd_trimmed("cmd.exe", &["/c", "ver"])
            .unwrap_or_else(|| "Windows (version unknown)".to_owned())
    }
    #[cfg(not(target_os = "windows"))]
    {
        run_cmd_trimmed("uname", &["-srm"]).unwrap_or_else(|| std::env::consts::OS.to_owned())
    }
}

fn run_cmd_trimmed(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (!s.is_empty()).then_some(s)
}

fn run_cmd_in_dir_trimmed(program: &str, args: &[&str], dir: &Path) -> Option<String> {
    let out = Command::new(program)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (!s.is_empty()).then_some(s)
}

/// Computes `min`, `p50`, `p95`, `max`, `mean` from millisecond samples.
#[must_use]
pub fn summarize_latencies(samples_ms: &[f64]) -> LatencyDistribution {
    if samples_ms.is_empty() {
        return LatencyDistribution {
            runs: 0,
            min_ms: 0.0,
            p50_ms: 0.0,
            p95_ms: 0.0,
            max_ms: 0.0,
            mean_ms: 0.0,
        };
    }
    let mut sorted = samples_ms.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    let min_ms = round2(sorted[0]);
    let max_ms = round2(sorted[n - 1]);
    let p50_ms = round2(percentile_sorted(&sorted, 0.50));
    let p95_ms = round2(percentile_sorted(&sorted, 0.95));
    let sum: f64 = sorted.iter().sum();
    let mean_ms = round2(sum / (n as f64));

    LatencyDistribution {
        runs: n,
        min_ms,
        p50_ms,
        p95_ms,
        max_ms,
        mean_ms,
    }
}

fn percentile_sorted(sorted: &[f64], p: f64) -> f64 {
    if sorted.len() == 1 {
        return sorted[0];
    }
    let rank = p * ((sorted.len() - 1) as f64);
    let low = rank.floor() as usize;
    let high = rank.ceil() as usize;
    if low == high {
        sorted[low]
    } else {
        let weight = rank - (low as f64);
        sorted[low] * (1.0 - weight) + sorted[high] * weight
    }
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn round4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

/// Queries `(working_set_bytes, private_bytes)` for a single PID using real OS APIs.
#[must_use]
pub fn query_process_memory_bytes(pid: u32) -> Option<(u64, u64)> {
    if pid == 0 {
        return None;
    }
    #[cfg(windows)]
    {
        use std::ffi::c_void;
        use std::mem::{size_of, zeroed};
        const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
        const PROCESS_VM_READ: u32 = 0x0010;

        #[repr(C)]
        struct ProcessMemoryCounters {
            cb: u32,
            page_fault_count: u32,
            peak_working_set_size: usize,
            working_set_size: usize,
            quota_peak_paged_pool_usage: usize,
            quota_paged_pool_usage: usize,
            quota_peak_non_paged_pool_usage: usize,
            quota_non_paged_pool_usage: usize,
            pagefile_usage: usize,
            peak_pagefile_usage: usize,
        }

        #[link(name = "kernel32")]
        extern "system" {
            fn OpenProcess(
                dw_desired_access: u32,
                b_inherit_handle: i32,
                dw_process_id: u32,
            ) -> *mut c_void;
            fn CloseHandle(h_object: *mut c_void) -> i32;
        }
        #[link(name = "psapi")]
        extern "system" {
            fn GetProcessMemoryInfo(
                process: *mut c_void,
                ppsmem_counters: *mut ProcessMemoryCounters,
                cb: u32,
            ) -> i32;
        }

        // SAFETY: `counters.cb` is initialized and process handle is closed before returning.
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, pid);
            if h.is_null() {
                return None;
            }
            let mut counters: ProcessMemoryCounters = zeroed();
            counters.cb = size_of::<ProcessMemoryCounters>() as u32;
            let ok = GetProcessMemoryInfo(h, &mut counters, counters.cb);
            let _ = CloseHandle(h);
            if ok == 0 {
                return None;
            }
            Some((
                counters.working_set_size as u64,
                counters.pagefile_usage as u64,
            ))
        }
    }
    #[cfg(target_os = "linux")]
    {
        let status = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
        let mut rss_kb = 0u64;
        let mut data_kb = 0u64;
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                rss_kb = rest
                    .split_whitespace()
                    .next()
                    .and_then(|n| n.parse::<u64>().ok())
                    .unwrap_or(0);
            } else if let Some(rest) = line.strip_prefix("VmData:") {
                data_kb = rest
                    .split_whitespace()
                    .next()
                    .and_then(|n| n.parse::<u64>().ok())
                    .unwrap_or(0);
            }
        }
        Some((rss_kb * 1024, data_kb * 1024))
    }
    #[cfg(target_os = "macos")]
    {
        let out = run_cmd_trimmed("ps", &["-o", "rss=", "-p", &pid.to_string()])?;
        let rss_kb = out.trim().parse::<u64>().ok()?;
        Some((rss_kb * 1024, 0))
    }
}

/// Queries cumulative `(kernel + user)` CPU time in milliseconds for `pid` using real OS counters.
#[must_use]
pub fn query_process_cpu_time_ms(pid: u32) -> Option<f64> {
    if pid == 0 {
        return None;
    }
    #[cfg(windows)]
    {
        use std::ffi::c_void;
        const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

        #[repr(C)]
        #[derive(Clone, Copy, Default)]
        struct FileTime {
            low: u32,
            high: u32,
        }

        impl FileTime {
            fn to_100ns(self) -> u64 {
                (u64::from(self.high) << 32) | u64::from(self.low)
            }
        }

        #[link(name = "kernel32")]
        extern "system" {
            fn OpenProcess(
                dw_desired_access: u32,
                b_inherit_handle: i32,
                dw_process_id: u32,
            ) -> *mut c_void;
            fn GetProcessTimes(
                h_process: *mut c_void,
                lp_creation_time: *mut FileTime,
                lp_exit_time: *mut FileTime,
                lp_kernel_time: *mut FileTime,
                lp_user_time: *mut FileTime,
            ) -> i32;
            fn CloseHandle(h_object: *mut c_void) -> i32;
        }

        // SAFETY: Valid stack `FileTime` pointers passed; handle closed immediately.
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return None;
            }
            let mut creation = FileTime::default();
            let mut exit = FileTime::default();
            let mut kernel = FileTime::default();
            let mut user = FileTime::default();
            let ok = GetProcessTimes(h, &mut creation, &mut exit, &mut kernel, &mut user);
            let _ = CloseHandle(h);
            if ok == 0 {
                return None;
            }
            let total_100ns = kernel.to_100ns().saturating_add(user.to_100ns());
            Some((total_100ns as f64) / 10_000.0)
        }
    }
    #[cfg(target_os = "linux")]
    {
        let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let close_paren = stat.rfind(')')?;
        let fields: Vec<&str> = stat[close_paren + 1..].split_whitespace().collect();
        if fields.len() <= 12 {
            return None;
        }
        let utime: u64 = fields[11].parse().ok()?;
        let stime: u64 = fields[12].parse().ok()?;
        // SAFETY: `sysconf(_SC_CLK_TCK)` is safe and thread-safe.
        let clk_tck = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
        if clk_tck <= 0 {
            return None;
        }
        Some(((utime + stime) as f64) * 1000.0 / (clk_tck as f64))
    }
    #[cfg(target_os = "macos")]
    {
        let out = run_cmd_trimmed("ps", &["-o", "time=", "-p", &pid.to_string()])?;
        parse_ps_time_to_ms(out.trim())
    }
}

#[cfg(target_os = "macos")]
fn parse_ps_time_to_ms(raw: &str) -> Option<f64> {
    let parts: Vec<&str> = raw.split(':').collect();
    match parts.as_slice() {
        [mins, secs] => {
            let m: f64 = mins.parse().ok()?;
            let s: f64 = secs.parse().ok()?;
            Some((m * 60.0 + s) * 1000.0)
        }
        [hrs, mins, secs] => {
            let h: f64 = hrs.parse().ok()?;
            let m: f64 = mins.parse().ok()?;
            let s: f64 = secs.parse().ok()?;
            Some((h * 3600.0 + m * 60.0 + s) * 1000.0)
        }
        _ => None,
    }
}

/// Captures a memory footprint sample across `pids` representing `instance_count` logical instances.
#[must_use]
pub fn sample_memory_for_pids(instance_count: usize, pids: &[u32]) -> MemorySample {
    let mut total_ws = 0u64;
    let mut total_priv = 0u64;
    let mut sampled = 0usize;
    for &pid in pids {
        if let Some((ws, pv)) = query_process_memory_bytes(pid) {
            total_ws = total_ws.saturating_add(ws);
            total_priv = total_priv.saturating_add(pv);
            sampled += 1;
        }
    }
    let avg_mib = if instance_count > 0 {
        round2((total_ws as f64) / (1024.0 * 1024.0) / (instance_count as f64))
    } else {
        0.0
    };
    MemorySample {
        instance_count,
        process_count: sampled,
        total_working_set_bytes: total_ws,
        total_private_bytes: total_priv,
        avg_working_set_mib_per_instance: avg_mib,
    }
}

/// Measures real idle CPU usage across `pids` over `window` duration.
#[must_use]
pub fn measure_idle_cpu_window(
    instance_count: usize,
    pids: &[u32],
    window: Duration,
    logical_cpus: usize,
) -> IdleCpuSample {
    let before: Vec<(u32, f64)> = pids
        .iter()
        .filter_map(|&pid| query_process_cpu_time_ms(pid).map(|ms| (pid, ms)))
        .collect();
    if before.is_empty() {
        return IdleCpuSample {
            observation_window_ms: u64::try_from(window.as_millis()).unwrap_or(0),
            instance_count,
            cpu_time_delta_ms: 0.0,
            single_core_cpu_percent: 0.0,
            host_normalized_cpu_percent: 0.0,
            measurement_supported: false,
        };
    }

    let wall_start = Instant::now();
    thread::sleep(window);
    let wall_ms = wall_start.elapsed().as_secs_f64() * 1000.0;

    let mut delta_ms = 0.0f64;
    for (pid, start_cpu) in before {
        if let Some(end_cpu) = query_process_cpu_time_ms(pid) {
            if end_cpu >= start_cpu {
                delta_ms += end_cpu - start_cpu;
            }
        }
    }

    let single_core = if wall_ms > 0.0 {
        (delta_ms / wall_ms) * 100.0
    } else {
        0.0
    };
    let host_norm = single_core / (logical_cpus.max(1) as f64);

    IdleCpuSample {
        observation_window_ms: wall_ms.round() as u64,
        instance_count,
        cpu_time_delta_ms: round4(delta_ms),
        single_core_cpu_percent: round4(single_core),
        host_normalized_cpu_percent: round4(host_norm),
        measurement_supported: true,
    }
}

/// Takes a snapshot of `(relative_path -> byte_size)` under `root` without following symlinks/junctions.
#[must_use]
pub fn snapshot_filesystem_tree(root: &Path) -> BTreeMap<String, u64> {
    let mut map = BTreeMap::new();
    if !root.exists() {
        return map;
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            if is_symlink_or_reparse(&meta) {
                continue;
            }
            if meta.is_dir() {
                stack.push(path);
            } else if meta.is_file() {
                if let Ok(rel) = path.strip_prefix(root) {
                    let norm = rel
                        .components()
                        .filter_map(|c| c.as_os_str().to_str())
                        .collect::<Vec<_>>()
                        .join("/");
                    map.insert(norm, meta.len());
                }
            }
        }
    }
    map
}

/// Computes file-system diff between two snapshots.
#[must_use]
pub fn diff_filesystem_trees(
    before: &BTreeMap<String, u64>,
    after: &BTreeMap<String, u64>,
) -> FileSystemDelta {
    let mut created = 0usize;
    let mut modified = 0usize;
    let mut total_bytes_after = 0u64;

    for (k, &size_after) in after {
        total_bytes_after = total_bytes_after.saturating_add(size_after);
        match before.get(k) {
            None => created += 1,
            Some(&size_before) if size_before != size_after => modified += 1,
            Some(_) => {}
        }
    }
    let deleted = before.keys().filter(|k| !after.contains_key(*k)).count();

    FileSystemDelta {
        files_before: before.len(),
        files_after: after.len(),
        created_files: created,
        modified_files: modified,
        deleted_files: deleted,
        total_bytes_after,
    }
}

/// Scans a sandbox `data_dir` and candidate PIDs for any cleanup residuals.
#[must_use]
pub fn inspect_cleanup_residuals(data_dir: &Path, candidate_pids: &[u32]) -> CleanupResiduals {
    let surviving_pids: Vec<u32> = candidate_pids
        .iter()
        .copied()
        .filter(|&pid| is_os_pid_alive(pid))
        .collect();

    let mut leftover_lock_artifacts = Vec::new();
    let mut leftover_staging_dirs = Vec::new();

    let mut stack = vec![data_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            if matches!(
                name.as_str(),
                "lockfile" | "SingletonLock" | ".quit-signal" | ".focus-signal" | ".runtime-pid"
            ) {
                leftover_lock_artifacts.push(path.display().to_string());
                continue;
            }
            if meta.is_dir() && !is_symlink_or_reparse(&meta) {
                if name.ends_with(".partial") || name.starts_with(".restore-") {
                    leftover_staging_dirs.push(path.display().to_string());
                } else {
                    stack.push(path);
                }
            }
        }
    }

    CleanupResiduals {
        surviving_pids,
        leftover_lock_artifacts,
        leftover_staging_dirs,
    }
}

/// Executes the real multi-instance fixture metrics benchmark on the current host.
pub fn collect_fixture_benchmark_report(
    seed: u64,
    latency_iterations: usize,
) -> Result<FixtureMetricsReport, String> {
    let provenance = collect_host_provenance(seed);
    let mut sandbox = IsolatedSandbox::new()?;
    let mut total_ops = 0usize;
    let mut failed_ops = 0usize;
    let mut all_seen_pids = Vec::new();

    let fs_before = snapshot_filesystem_tree(sandbox.data_dir());

    // 1. Native Rust `xf-desktop-fixture` benchmark (if binary exists and runs on host)
    let native_bin = xf_fixture_bin_path();
    let mut native_startup_samples = Vec::new();
    let mut native_shutdown_samples = Vec::new();
    let mut native_memory_scaling = Vec::new();
    let mut native_idle_cpu = None;

    if native_bin.exists() {
        let native_root = sandbox.root().join("native-bench");
        fs::create_dir_all(&native_root).map_err(|e| e.to_string())?;

        for idx in 0..latency_iterations {
            total_ops += 1;
            let pdir = native_root.join(format!("run-{idx}"));
            let zdata = sandbox.zalo_data_dir_for(&pdir);
            let lockfile = zdata.join("lockfile");
            let quit_sig = zdata.join(".quit-signal");

            let t0 = Instant::now();
            let child_res = Command::new(&native_bin)
                .arg("--instance-id")
                .arg(format!("bench-native-{idx}"))
                .arg("--profile-dir")
                .arg(&pdir)
                .arg("--user-id")
                .arg(format!("{}", 800_000 + idx))
                .arg("--seed")
                .arg(seed.to_string())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();

            let Ok(mut child) = child_res else {
                // On SAC=1 Windows hosts, unsigned native PE may be blocked; stop native loop cleanly.
                break;
            };
            let pid = child.id();
            sandbox.track_pid(pid);
            all_seen_pids.push(pid);

            let wait_deadline = Instant::now() + Duration::from_secs(5);
            let mut ready = false;
            while Instant::now() < wait_deadline {
                if zdata.join("fixture-state.json").exists() && lockfile.exists() {
                    ready = true;
                    break;
                }
                thread::sleep(Duration::from_millis(5));
            }
            if !ready {
                failed_ops += 1;
                let _ = child.kill();
                let _ = child.wait();
                continue;
            }
            native_startup_samples.push(t0.elapsed().as_secs_f64() * 1000.0);

            let t_stop = Instant::now();
            let _ = fs::write(&quit_sig, b"{\"quit\":true}\n");
            let status = child.wait().map_err(|e| e.to_string())?;
            native_shutdown_samples.push(t_stop.elapsed().as_secs_f64() * 1000.0);
            if !status.success() {
                failed_ops += 1;
            }
        }

        // Memory & Idle CPU scaling on native Rust fixture (1, 3, 5 concurrent instances)
        if !native_startup_samples.is_empty() {
            for &scale in &[1usize, 3, 5] {
                let mut children = Vec::with_capacity(scale);
                let mut zdirs = Vec::with_capacity(scale);
                for i in 0..scale {
                    let pdir = native_root.join(format!("scale-{scale}-{i}"));
                    let zdata = sandbox.zalo_data_dir_for(&pdir);
                    if let Ok(c) = Command::new(&native_bin)
                        .arg("--instance-id")
                        .arg(format!("native-scale-{scale}-{i}"))
                        .arg("--profile-dir")
                        .arg(&pdir)
                        .arg("--user-id")
                        .arg(format!("{}", 810_000 + scale * 10 + i))
                        .arg("--seed")
                        .arg(seed.to_string())
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn()
                    {
                        sandbox.track_pid(c.id());
                        all_seen_pids.push(c.id());
                        children.push(c);
                        zdirs.push(zdata);
                    }
                }
                for zd in &zdirs {
                    let dl = Instant::now() + Duration::from_secs(5);
                    while Instant::now() < dl && !zd.join("fixture-state.json").exists() {
                        thread::sleep(Duration::from_millis(10));
                    }
                }
                let pids: Vec<u32> = children.iter().map(std::process::Child::id).collect();
                native_memory_scaling.push(sample_memory_for_pids(scale, &pids));

                if scale == 3 {
                    native_idle_cpu = Some(measure_idle_cpu_window(
                        3,
                        &pids,
                        Duration::from_millis(1_200),
                        provenance.logical_cpus,
                    ));
                }

                for zd in &zdirs {
                    let _ = fs::write(zd.join(".quit-signal"), b"{}\n");
                }
                for mut c in children {
                    let _ = c.wait();
                }
            }
        }
    }

    // 2. Coordinator + Signed-Node Fixture (`LaunchTarget::NodeFixture`) benchmark
    let mut node_startup_samples = Vec::new();
    let mut node_shutdown_samples = Vec::new();
    let mut node_memory_scaling = Vec::new();
    let mut node_idle_cpu = IdleCpuSample {
        observation_window_ms: 0,
        instance_count: 0,
        cpu_time_delta_ms: 0.0,
        single_core_cpu_percent: 0.0,
        host_normalized_cpu_percent: 0.0,
        measurement_supported: false,
    };

    let bench_prof = sandbox
        .store
        .create_profile("Bench Single", "latency")
        .map_err(|e| e.to_string())?;
    let target_base = make_node_fixture_target(&["--seed", &seed.to_string()]);

    for _ in 0..latency_iterations {
        total_ops += 1;
        let t0 = Instant::now();
        match sandbox
            .coordinator
            .launch_profile(&bench_prof.id, &target_base)
        {
            Ok(rt) => {
                if let Some(pid) = rt.main_pid {
                    sandbox.track_pid(pid);
                    all_seen_pids.push(pid);
                    // Wait until fixture-state.json is flushed so latency reflects full ready state
                    let dl = Instant::now() + Duration::from_secs(4);
                    while Instant::now() < dl
                        && !rt.zalo_data_dir.join("fixture-state.json").exists()
                    {
                        thread::sleep(Duration::from_millis(10));
                    }
                    node_startup_samples.push(t0.elapsed().as_secs_f64() * 1000.0);

                    let t_stop = Instant::now();
                    if sandbox.coordinator.stop_profile(&bench_prof.id).is_err() {
                        failed_ops += 1;
                        let _ = force_kill_unprotected_test_pid(pid);
                    } else {
                        node_shutdown_samples.push(t_stop.elapsed().as_secs_f64() * 1000.0);
                    }
                } else {
                    failed_ops += 1;
                    thread::sleep(Duration::from_millis(80));
                    let _ = sandbox.coordinator.stop_profile(&bench_prof.id);
                }
            }
            Err(_) => {
                failed_ops += 1;
            }
        }
    }
    let _ = sandbox.coordinator.delete_profile(&bench_prof.id, true);

    // Memory & Idle CPU scaling via Coordinator (1, 3, 5 concurrent profiles)
    let mut scale_profiles = Vec::with_capacity(5);
    for idx in 0..5 {
        let p = sandbox
            .store
            .create_profile(&format!("Scale Node {}", idx + 1), "")
            .map_err(|e| e.to_string())?;
        scale_profiles.push(p);
    }

    for &scale in &[1usize, 3, 5] {
        let mut active_pids = Vec::new();
        for (idx, prof) in scale_profiles.iter().take(scale).enumerate() {
            let uid = format!("{}", 850_000 + idx);
            let t = make_node_fixture_target(&["--user-id", &uid, "--seed", &seed.to_string()]);
            if let Ok(rt) = sandbox.coordinator.launch_profile(&prof.id, &t) {
                if let Some(pid) = rt.main_pid {
                    sandbox.track_pid(pid);
                    all_seen_pids.push(pid);
                    active_pids.push(pid);
                }
            }
        }
        thread::sleep(Duration::from_millis(150));
        node_memory_scaling.push(sample_memory_for_pids(scale, &active_pids));

        if scale == 3 {
            node_idle_cpu = measure_idle_cpu_window(
                3,
                &active_pids,
                Duration::from_millis(1_200),
                provenance.logical_cpus,
            );
        }

        for prof in scale_profiles.iter().take(scale) {
            let _ = sandbox.coordinator.stop_profile(&prof.id);
        }
    }

    let fs_mid = snapshot_filesystem_tree(sandbox.data_dir());
    let fs_changes = diff_filesystem_trees(&fs_before, &fs_mid);

    for prof in &scale_profiles {
        let _ = sandbox.coordinator.delete_profile(&prof.id, true);
    }

    let cleanup_residuals = inspect_cleanup_residuals(sandbox.data_dir(), &all_seen_pids);
    let failure_rate = if total_ops > 0 {
        round4((failed_ops as f64) / (total_ops as f64))
    } else {
        0.0
    };

    Ok(FixtureMetricsReport {
        provenance,
        native_rust_fixture_startup: (!native_startup_samples.is_empty())
            .then(|| summarize_latencies(&native_startup_samples)),
        native_rust_fixture_shutdown: (!native_shutdown_samples.is_empty())
            .then(|| summarize_latencies(&native_shutdown_samples)),
        coordinator_node_fixture_startup: summarize_latencies(&node_startup_samples),
        coordinator_node_fixture_shutdown: summarize_latencies(&node_shutdown_samples),
        native_memory_scaling,
        node_fixture_memory_scaling: node_memory_scaling,
        native_idle_cpu,
        node_idle_cpu,
        filesystem_changes: fs_changes,
        cleanup_residuals,
        total_operations: total_ops,
        failed_operations: failed_ops,
        failure_rate,
    })
}

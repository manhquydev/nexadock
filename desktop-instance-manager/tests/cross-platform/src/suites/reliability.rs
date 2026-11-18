//! Reliability & Soak Test Matrix (`REL-01` .. `REL-08`).
//!
//! Covers:
//! - 10 full coordinator restart cycles
//! - 100 fixture launch/stop soak cycles (measuring latency stability & zero residual leaks)
//! - Multi-mode crash & controlled-error injection (`immediate`, `after-lock`, `mid-write`, `corrupt-state`, `disk-full`, `permission-denied`)
//! - Slow disk simulation (`--slow-io-ms` + delayed disk space probe)
//! - Concurrent 5-profile heartbeat soak under simultaneous status/rename polling
//! - Corrupted metadata resilience (`profiles.json` empty/truncated/bad-schema, `.zalo-profile-id` mismatch, corrupt `config.json`)
//! - Process tree cleanup (`--spawn-children=2` worker hierarchy on graceful & forced stop)
//! - Stale locks & PID-reuse spoof protection (`lockfile` with dead PID, garbage bytes, or live non-Zalo PID)

use std::fs;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use dim_core::{BackupManager, InstanceLauncher, ProfileStore, RecoveryManager};
use dim_process_manager::StaleLockRecoveryManager;
use dim_profile_store::FileBackupManager;
use dim_xf_fixture::{is_os_pid_alive, FixturePersistentState};
use serde_json::json;

use crate::harness::{
    force_kill_unprotected_test_pid, make_node_fixture_target, run_case_if, xf_fixture_bin_path,
    xf_fixture_mjs_path, CaseSelectionFilter, CaseSuccess, IsolatedSandbox, TestCaseOutcome,
};
use crate::metrics::{inspect_cleanup_residuals, summarize_latencies};

/// Executes all Reliability & Soak scenarios (`REL-01` .. `REL-08`).
pub fn run_reliability_suite(soak_cycles: usize, seed: u64) -> Vec<TestCaseOutcome> {
    run_reliability_suite_filtered(soak_cycles, seed, None)
}

/// Executes Reliability & Soak scenarios (`REL-01` .. `REL-08`) matching `filter`.
pub fn run_reliability_suite_filtered(
    soak_cycles: usize,
    seed: u64,
    filter: Option<&CaseSelectionFilter>,
) -> Vec<TestCaseOutcome> {
    let cycles = soak_cycles.max(10);
    [
        run_case_if(
            filter,
            "REL-01",
            "reliability",
            "10 consecutive full coordinator launch/stop restart cycles",
            "all",
            || rel_01_ten_coordinator_restart_cycles(seed),
        ),
        run_case_if(
            filter,
            "REL-02",
            "reliability",
            &format!("{cycles} fixture launch/stop soak cycles with zero leak verification"),
            "all",
            || rel_02_hundred_fixture_cycles(cycles, seed),
        ),
        run_case_if(
            filter,
            "REL-03",
            "reliability",
            "Crash injection & controlled error matrix (`immediate`, `after-lock`, `corrupt-state`, `disk-full`, `eacces`)",
            "all",
            rel_03_crash_and_controlled_error_matrix,
        ),
        run_case_if(
            filter,
            "REL-04",
            "reliability",
            "Slow disk I/O simulation (`--slow-io-ms`) with offline backup SHA-256 verification",
            "all",
            || rel_04_slow_disk_simulation(seed),
        ),
        run_case_if(
            filter,
            "REL-05",
            "reliability",
            "5 concurrent heartbeating profiles under parallel status & live rename stress",
            "all",
            || rel_05_concurrent_profiles_stress(seed),
        ),
        run_case_if(
            filter,
            "REL-06",
            "reliability",
            "Corrupted metadata resilience (`profiles.json`, `.zalo-profile-id`, `ZaloData/config.json`)",
            "all",
            rel_06_corrupted_metadata_resilience,
        ),
        run_case_if(
            filter,
            "REL-07",
            "reliability",
            "Process tree cleanup with `--spawn-children=2` worker hierarchy",
            "all",
            rel_07_process_tree_cleanup,
        ),
        run_case_if(
            filter,
            "REL-08",
            "reliability",
            "Stale locks, garbage lockfiles, and live non-Zalo PID-reuse spoof protection",
            "all",
            rel_08_stale_locks_and_pid_reuse,
        ),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn rel_01_ten_coordinator_restart_cycles(seed: u64) -> Result<CaseSuccess, String> {
    let mut sandbox = IsolatedSandbox::new()?;
    let prof = sandbox
        .store
        .create_profile("Ten Cycle Coordinator", "")
        .map_err(|e| e.to_string())?;
    let pdir = sandbox
        .store
        .profile_dir_for(&prof)
        .map_err(|e| e.to_string())?;
    let zdata = sandbox.zalo_data_dir_for(&pdir);
    let target = make_node_fixture_target(&["--user-id=610001", "--seed", &seed.to_string()]);

    let mut cycle_pids = Vec::with_capacity(10);
    for cycle in 1..=10 {
        let rt = sandbox
            .coordinator
            .launch_profile(&prof.id, &target)
            .map_err(|e| format!("cycle {cycle} launch failed: {e}"))?;
        let dl = Instant::now() + Duration::from_secs(4);
        while Instant::now() < dl {
            let booted_this_cycle = fs::read_to_string(zdata.join("fixture-state.json"))
                .ok()
                .and_then(|raw| serde_json::from_str::<FixturePersistentState>(&raw).ok())
                .is_some_and(|st| st.boot_sequence >= cycle);
            if booted_this_cycle {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let live = sandbox
            .coordinator
            .inspect_profile(&prof.id)
            .map_err(|e| e.to_string())?;
        let pid = live
            .main_pid
            .or(rt.main_pid)
            .ok_or_else(|| format!("cycle {cycle}: missing main_pid"))?;
        sandbox.track_pid(pid);
        cycle_pids.push(pid);

        sandbox
            .coordinator
            .stop_profile(&prof.id)
            .map_err(|e| format!("cycle {cycle} stop failed: {e}"))?;
    }

    let raw = fs::read_to_string(zdata.join("fixture-state.json")).map_err(|e| e.to_string())?;
    let fstate: FixturePersistentState = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    if fstate.boot_sequence != 10 || fstate.clean_shutdown_count != 10 || !fstate.clean_shutdown {
        return Err(format!(
            "expected 10/10 clean cycles, got boot={}, clean={}",
            fstate.boot_sequence, fstate.clean_shutdown_count
        ));
    }

    let residuals = inspect_cleanup_residuals(sandbox.data_dir(), &cycle_pids);
    if !residuals.surviving_pids.is_empty() || !residuals.leftover_lock_artifacts.is_empty() {
        return Err(format!("residuals detected after 10 cycles: {residuals:?}"));
    }

    Ok(CaseSuccess::pass(json!({
        "cycles_completed": fstate.boot_sequence,
        "clean_shutdown_count": fstate.clean_shutdown_count,
        "surviving_pids": 0,
    })))
}

fn rel_02_hundred_fixture_cycles(cycles: usize, seed: u64) -> Result<CaseSuccess, String> {
    let mut sandbox = IsolatedSandbox::new()?;
    let soak_dir = sandbox.root().join("soak-100-profile");
    let zdata = sandbox.zalo_data_dir_for(&soak_dir);
    fs::create_dir_all(&zdata).map_err(|e| e.to_string())?;

    let native_bin = xf_fixture_bin_path();
    let mut use_native = native_bin.exists();
    let mut startup_ms = Vec::with_capacity(cycles);
    let mut shutdown_ms = Vec::with_capacity(cycles);
    let mut pids = Vec::with_capacity(cycles);

    for cycle in 1..=cycles {
        let t0 = Instant::now();
        let mut child_opt = None;

        if use_native {
            match Command::new(&native_bin)
                .arg("--instance-id=soak-100")
                .arg("--profile-dir")
                .arg(&soak_dir)
                .arg("--user-id=620001")
                .arg("--write-files=1")
                .arg("--write-bytes=256")
                .arg("--seed")
                .arg((seed + cycle as u64).to_string())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(c) => child_opt = Some(c),
                Err(_) if cycle == 1 => {
                    // Fallback cleanly to vendor-signed Node on SAC=1 hosts
                    use_native = false;
                }
                Err(e) => return Err(format!("soak cycle {cycle} spawn failed: {e}")),
            }
        }

        let mut child = match child_opt {
            Some(c) => c,
            None => Command::new("node")
                .arg(xf_fixture_mjs_path())
                .arg("--instance-id=soak-100")
                .arg("--profile-dir")
                .arg(&soak_dir)
                .arg("--user-id=620001")
                .arg("--write-files=1")
                .arg("--write-bytes=256")
                .arg("--seed")
                .arg((seed + cycle as u64).to_string())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| format!("node fallback soak spawn failed on cycle {cycle}: {e}"))?,
        };

        let pid = child.id();
        sandbox.track_pid(pid);
        pids.push(pid);

        // Wait until `lockfile` exists AND `fixture-state.json` records `boot_sequence == cycle`
        let dl = Instant::now() + Duration::from_secs(5);
        let mut ready = false;
        while Instant::now() < dl {
            if zdata.join("lockfile").exists() {
                if let Ok(raw) = fs::read_to_string(zdata.join("fixture-state.json")) {
                    if let Ok(st) = serde_json::from_str::<FixturePersistentState>(&raw) {
                        if st.boot_sequence == cycle as u64 {
                            ready = true;
                            break;
                        }
                    }
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        if !ready {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "soak cycle {cycle} timed out waiting for ready state"
            ));
        }
        startup_ms.push(t0.elapsed().as_secs_f64() * 1000.0);

        let t_stop = Instant::now();
        fs::write(zdata.join(".quit-signal"), b"{\"quit\":true}\n").map_err(|e| e.to_string())?;
        let status = child.wait().map_err(|e| e.to_string())?;
        shutdown_ms.push(t_stop.elapsed().as_secs_f64() * 1000.0);
        if !status.success() {
            return Err(format!(
                "soak cycle {cycle} exited with non-zero status {status}"
            ));
        }
    }

    let final_raw =
        fs::read_to_string(zdata.join("fixture-state.json")).map_err(|e| e.to_string())?;
    let final_state: FixturePersistentState =
        serde_json::from_str(&final_raw).map_err(|e| e.to_string())?;
    if final_state.boot_sequence != cycles as u64
        || final_state.clean_shutdown_count != cycles as u64
        || zdata.join("lockfile").exists()
    {
        return Err(format!(
            "soak state mismatch: boot={}, clean={}",
            final_state.boot_sequence, final_state.clean_shutdown_count
        ));
    }

    let alive_after: Vec<u32> = pids.into_iter().filter(|&p| is_os_pid_alive(p)).collect();
    if !alive_after.is_empty() {
        return Err(format!("surviving PIDs after soak: {alive_after:?}"));
    }

    let first_10_mean = summarize_latencies(&startup_ms[..10.min(startup_ms.len())]).mean_ms;
    let last_10_mean =
        summarize_latencies(&startup_ms[startup_ms.len().saturating_sub(10)..]).mean_ms;

    Ok(CaseSuccess::pass(json!({
        "runtime_mode": if use_native { "native-rust-fixture" } else { "signed-node-fixture" },
        "cycles_completed": cycles,
        "failure_rate": 0.0,
        "startup_latency": summarize_latencies(&startup_ms),
        "shutdown_latency": summarize_latencies(&shutdown_ms),
        "first_10_startup_mean_ms": first_10_mean,
        "last_10_startup_mean_ms": last_10_mean,
        "surviving_pids": 0,
    })))
}

fn rel_03_crash_and_controlled_error_matrix() -> Result<CaseSuccess, String> {
    let sandbox = IsolatedSandbox::new()?;
    let mut results = Vec::new();

    let modes = [
        ("error-disk-full", &["--error-mode=disk-full"][..], 28i32),
        (
            "error-eacces",
            &["--error-mode=permission-denied"][..],
            13i32,
        ),
        (
            "error-corrupt-cfg",
            &["--error-mode=corrupt-config"][..],
            65i32,
        ),
        ("crash-immediate", &["--crash-mode=immediate"][..], 101i32),
        (
            "crash-after-lock",
            &["--crash-mode=after-lock", "--crash-delay-ms=25"][..],
            137i32,
        ),
        (
            "crash-corrupt-state",
            &["--crash-mode=corrupt-state", "--crash-delay-ms=25"][..],
            139i32,
        ),
    ];

    for (label, flags, expected_code) in modes {
        let pdir = sandbox.root().join(format!("fault-{label}"));
        let zdata = sandbox.zalo_data_dir_for(&pdir);
        let status = Command::new("node")
            .arg(xf_fixture_mjs_path())
            .arg("--profile-dir")
            .arg(&pdir)
            .args(flags)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| format!("{label} spawn failed: {e}"))?;

        let code = status.code().unwrap_or(-1);
        if code != expected_code {
            return Err(format!(
                "{label}: expected exit code {expected_code}, got {code}"
            ));
        }
        results.push(json!({
            "scenario": label,
            "exit_code": code,
            "stale_lock_present": zdata.join("lockfile").exists(),
            "last_error_recorded": zdata.join("last-error.json").exists(),
        }));
    }

    Ok(CaseSuccess::pass(json!({
        "fault_modes_verified": results,
    })))
}

fn rel_04_slow_disk_simulation(seed: u64) -> Result<CaseSuccess, String> {
    let mut sandbox = IsolatedSandbox::new()?;
    let prof = sandbox
        .store
        .create_profile("Slow Disk Profile", "")
        .map_err(|e| e.to_string())?;
    let pdir = sandbox
        .store
        .profile_dir_for(&prof)
        .map_err(|e| e.to_string())?;
    let zdata = sandbox.zalo_data_dir_for(&pdir);

    let target = make_node_fixture_target(&[
        "--user-id=640001",
        "--write-files=6",
        "--write-bytes=4096",
        "--slow-io-ms=25",
        "--seed",
        &seed.to_string(),
    ]);

    let rt = sandbox
        .coordinator
        .launch_profile(&prof.id, &target)
        .map_err(|e| e.to_string())?;
    if let Some(pid) = rt.main_pid {
        sandbox.track_pid(pid);
    }

    let dl = Instant::now() + Duration::from_secs(6);
    while Instant::now() < dl && !zdata.join("fixture-state.json").exists() {
        thread::sleep(Duration::from_millis(25));
    }

    sandbox
        .coordinator
        .stop_profile(&prof.id)
        .map_err(|e| e.to_string())?;

    // Backup through a simulated slow DiskSpaceProbe
    let slow_bmgr =
        FileBackupManager::new(Arc::clone(&sandbox.store)).with_disk_space_probe(Arc::new(|_| {
            thread::sleep(Duration::from_millis(35));
            Ok(dim_core::MIN_FREE_DISK_BYTES * 4)
        }));
    let bk = slow_bmgr
        .backup_profile(&prof, &pdir, None, false)
        .map_err(|e| e.to_string())?;
    let verified = slow_bmgr
        .verify_backup(&bk.backup_dir)
        .map_err(|e| e.to_string())?;

    Ok(CaseSuccess::pass(json!({
        "backed_up_files": verified.file_count,
        "backed_up_bytes": verified.total_bytes,
    })))
}

fn rel_05_concurrent_profiles_stress(seed: u64) -> Result<CaseSuccess, String> {
    let mut sandbox = IsolatedSandbox::new()?;
    let mut ids = Vec::with_capacity(5);

    for idx in 0..5 {
        let rec = sandbox
            .store
            .create_profile(&format!("Stress Profile {}", idx + 1), "")
            .map_err(|e| e.to_string())?;
        let uid = format!("{}", 650_001 + idx);
        let target = make_node_fixture_target(&[
            "--user-id",
            &uid,
            "--heartbeat-ms=50",
            "--seed",
            &seed.to_string(),
        ]);
        let rt = sandbox
            .coordinator
            .launch_profile(&rec.id, &target)
            .map_err(|e| e.to_string())?;
        if let Some(pid) = rt.main_pid {
            sandbox.track_pid(pid);
        }
        ids.push(rec.id);
    }

    // Exercise simultaneous metadata renames & status inspections while all 5 fixtures heartbeat on disk
    for round in 1..=5 {
        for (idx, id) in ids.iter().enumerate() {
            let st = sandbox
                .coordinator
                .inspect_profile(id)
                .map_err(|e| e.to_string())?;
            if let Some(pid) = st.main_pid {
                sandbox.track_pid(pid);
            }
            sandbox
                .store
                .rename_profile(id, &format!("Stress Profile {} R{round}", idx + 1))
                .map_err(|e| e.to_string())?;
        }
        thread::sleep(Duration::from_millis(55));
    }

    let mut total_heartbeats = 0u64;
    for id in &ids {
        let rec = sandbox.store.get_profile(id).map_err(|e| e.to_string())?;
        let pdir = sandbox
            .store
            .profile_dir_for(&rec)
            .map_err(|e| e.to_string())?;
        let zdata = sandbox.zalo_data_dir_for(&pdir);
        sandbox
            .coordinator
            .stop_profile(id)
            .map_err(|e| e.to_string())?;
        if let Ok(raw) = fs::read_to_string(zdata.join("fixture-state.json")) {
            if let Ok(st) = serde_json::from_str::<FixturePersistentState>(&raw) {
                total_heartbeats = total_heartbeats.saturating_add(st.heartbeat_ticks);
            }
        }
    }

    Ok(CaseSuccess::pass(json!({
        "concurrent_profiles": 5,
        "live_renames_completed": 25,
        "aggregate_heartbeat_ticks": total_heartbeats,
    })))
}

fn rel_06_corrupted_metadata_resilience() -> Result<CaseSuccess, String> {
    let sandbox = IsolatedSandbox::new()?;
    let prof = sandbox
        .store
        .create_profile("Marker Integrity Probe", "")
        .map_err(|e| e.to_string())?;
    let pdir = sandbox
        .store
        .profile_dir_for(&prof)
        .map_err(|e| e.to_string())?;
    sandbox
        .adapter
        .ensure_profile_layout(&prof, &pdir)
        .map_err(|e| e.to_string())?;
    let zdata = sandbox.zalo_data_dir_for(&pdir);

    // 1. Corrupt `ZaloData/config.json` must NOT panic `detect_zalo_user_id` or `inspect_profile`
    fs::write(zdata.join("config.json"), b"{corrupt_config_json").map_err(|e| e.to_string())?;
    let uid = dim_core::detect_zalo_user_id(&zdata);
    if uid.is_some() {
        return Err(
            "expected None from corrupt config.json without Database/_production".to_owned(),
        );
    }

    // 2. Tamper `.zalo-profile-id` -> `launch_profile` must fail closed with `PROFILE_MARKER_MISMATCH`!
    fs::write(zdata.join(".zalo-profile-id"), b"ffffffff").map_err(|e| e.to_string())?;
    let target = make_node_fixture_target(&["--user-id=660001"]);
    let mismatch_err = sandbox
        .coordinator
        .launch_profile(&prof.id, &target)
        .expect_err("tampered .zalo-profile-id must block launch");
    if mismatch_err.code() != "PROFILE_MARKER_MISMATCH" {
        return Err(format!(
            "expected PROFILE_MARKER_MISMATCH, got {}",
            mismatch_err.code()
        ));
    }

    // 3. Corrupt `profiles.json` variants -> fail-closed with `JSON_ERROR` or `SCHEMA_ERROR`
    let meta = sandbox.store.metadata_path().to_path_buf();

    fs::write(&meta, b"").map_err(|e| e.to_string())?;
    let empty_err = sandbox
        .store
        .load()
        .expect_err("empty profiles.json must fail");
    if empty_err.code() != "JSON_ERROR" {
        return Err(format!(
            "expected JSON_ERROR on empty file, got {}",
            empty_err.code()
        ));
    }

    fs::write(&meta, b"{\"schema_version\": 1, \"profiles\": [{\"broken")
        .map_err(|e| e.to_string())?;
    let trunc_err = sandbox.store.load().expect_err("truncated JSON must fail");
    if trunc_err.code() != "JSON_ERROR" {
        return Err(format!(
            "expected JSON_ERROR on truncated JSON, got {}",
            trunc_err.code()
        ));
    }

    fs::write(&meta, b"{\"schema_version\": 99, \"profiles\": []}").map_err(|e| e.to_string())?;
    let schema99_err = sandbox
        .store
        .load()
        .expect_err("schema_version 99 must fail");
    if schema99_err.code() != "SCHEMA_ERROR" {
        return Err(format!(
            "expected SCHEMA_ERROR, got {}",
            schema99_err.code()
        ));
    }

    fs::write(&meta, b"{\"schema_version\": 0, \"profiles\": []}").map_err(|e| e.to_string())?;
    let schema0_err = sandbox
        .store
        .load()
        .expect_err("schema_version 0 must fail");
    if schema0_err.code() != "SCHEMA_ERROR" {
        return Err(format!(
            "expected SCHEMA_ERROR on v0, got {}",
            schema0_err.code()
        ));
    }

    Ok(CaseSuccess::pass(json!({
        "marker_mismatch_code": mismatch_err.code(),
        "empty_metadata_code": empty_err.code(),
        "truncated_metadata_code": trunc_err.code(),
        "unsupported_schema_code": schema99_err.code(),
        "zero_schema_code": schema0_err.code(),
    })))
}

fn rel_07_process_tree_cleanup() -> Result<CaseSuccess, String> {
    let mut sandbox = IsolatedSandbox::new()?;
    let prof = sandbox
        .store
        .create_profile("Process Tree Profile", "")
        .map_err(|e| e.to_string())?;
    let pdir = sandbox
        .store
        .profile_dir_for(&prof)
        .map_err(|e| e.to_string())?;
    let zdata = sandbox.zalo_data_dir_for(&pdir);

    // Spawn fixture with 2 child worker processes AND `--hang-on-quit` + `--orphan-children`
    // so ONLY the OS adapter's process-tree cleanup can kill the 2 worker children!
    let target = make_node_fixture_target(&[
        "--user-id=670001",
        "--spawn-children=2",
        "--orphan-children",
        "--hang-on-quit",
    ]);
    let rt = sandbox
        .coordinator
        .launch_profile(&prof.id, &target)
        .map_err(|e| e.to_string())?;
    if let Some(pid) = rt.main_pid {
        sandbox.track_pid(pid);
    }

    let dl = Instant::now() + Duration::from_secs(4);
    while Instant::now() < dl && !zdata.join("children.json").exists() {
        thread::sleep(Duration::from_millis(20));
    }

    let children_raw =
        fs::read_to_string(zdata.join("children.json")).map_err(|e| e.to_string())?;
    let children_val: serde_json::Value =
        serde_json::from_str(&children_raw).map_err(|e| e.to_string())?;
    let child_pids: Vec<u32> = children_val
        .get("child_pids")
        .and_then(serde_json::Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_u64().and_then(|n| u32::try_from(n).ok()))
                .collect()
        })
        .unwrap_or_default();

    for &cpid in &child_pids {
        sandbox.track_pid(cpid);
    }
    if child_pids.len() != 2 || !child_pids.iter().all(|&p| is_os_pid_alive(p)) {
        return Err(format!(
            "expected 2 live worker children, got {child_pids:?}"
        ));
    }

    let inspected = sandbox
        .coordinator
        .inspect_profile(&prof.id)
        .map_err(|e| e.to_string())?;

    let stop_count = sandbox
        .coordinator
        .stop_profile(&prof.id)
        .map_err(|e| e.to_string())?;

    thread::sleep(Duration::from_millis(120));
    let surviving_children: Vec<u32> = child_pids
        .iter()
        .copied()
        .filter(|&p| is_os_pid_alive(p))
        .collect();

    for &cpid in &surviving_children {
        let _ = force_kill_unprotected_test_pid(cpid);
    }

    if !surviving_children.is_empty() {
        return Ok(CaseSuccess::defect(
            "DEFECT-06",
            &[
                "crates/platform-macos/src/adapter.rs:210,386-404",
                "crates/platform-linux/src/adapter.rs:206,342-360",
            ],
            "Launch a fixture with `--spawn-children=2 --orphan-children --hang-on-quit`, then call `stop_profile`: POSIX adapters only track `main_pid` (`process_ids` omits child tree) and only signal `main_pid`, leaving child worker processes orphaned.",
            json!({
                "inspected_process_count": inspected.process_count,
                "spawned_child_pids": child_pids,
                "stop_returned_count": stop_count,
                "surviving_orphan_children": surviving_children,
            }),
        ));
    }

    Ok(CaseSuccess::pass(json!({
        "inspected_process_count": inspected.process_count,
        "spawned_child_pids": child_pids,
        "terminated_process_count": stop_count,
        "surviving_children": 0,
    })))
}

fn rel_08_stale_locks_and_pid_reuse() -> Result<CaseSuccess, String> {
    let sandbox = IsolatedSandbox::new()?;
    let prof = sandbox
        .store
        .create_profile("Stale Lock Spoof Probe", "")
        .map_err(|e| e.to_string())?;
    let pdir = sandbox
        .store
        .profile_dir_for(&prof)
        .map_err(|e| e.to_string())?;
    sandbox
        .adapter
        .ensure_profile_layout(&prof, &pdir)
        .map_err(|e| e.to_string())?;
    let zdata = sandbox.zalo_data_dir_for(&pdir);

    // 1. Corrupt non-JSON garbage in `lockfile` + leftover signal files
    fs::write(zdata.join("lockfile"), [0xFF, 0xFE, 0x00, 0x13]).map_err(|e| e.to_string())?;
    fs::write(zdata.join(".quit-signal"), b"old").map_err(|e| e.to_string())?;
    fs::write(zdata.join(".focus-signal"), b"old").map_err(|e| e.to_string())?;
    fs::write(pdir.join(".runtime-pid"), b"99999999").map_err(|e| e.to_string())?;

    let st_garbage = sandbox
        .coordinator
        .inspect_profile(&prof.id)
        .map_err(|e| e.to_string())?;
    if st_garbage.running || !st_garbage.stale_lock_detected {
        return Err(format!(
            "expected running=false & stale_lock_detected=true on garbage lockfile, got {st_garbage:?}"
        ));
    }

    let rec_mgr = StaleLockRecoveryManager;
    let rec_report = rec_mgr
        .recover_stale_state(&prof, &pdir, &st_garbage)
        .map_err(|e| e.to_string())?;
    if !rec_report.recovered || rec_report.removed_files.len() < 4 {
        return Err(format!(
            "expected >= 4 stale artifacts cleaned, got {rec_report:?}"
        ));
    }

    // 2. PID-reuse spoof check: write the CURRENT test runner's live PID (`std::process::id()`, which is NOT `zalo.exe`/`node.exe`)
    // into `lockfile` without holding a RestartManager lock!
    let self_pid = std::process::id();
    fs::write(
        zdata.join("lockfile"),
        serde_json::json!({ "pid": self_pid }).to_string(),
    )
    .map_err(|e| e.to_string())?;

    let st_recycled = sandbox
        .coordinator
        .inspect_profile(&prof.id)
        .map_err(|e| e.to_string())?;
    let _ = fs::remove_file(zdata.join("lockfile"));

    #[cfg(target_os = "windows")]
    if st_recycled.running {
        return Err(
            "Windows adapter falsely treated recycled non-Zalo test runner PID as a running Zalo instance!"
                .to_owned(),
        );
    }

    Ok(CaseSuccess::pass(json!({
        "garbage_lock_detected_as_stale": true,
        "recovered_artifact_count": rec_report.removed_files.len(),
        "recycled_self_pid_running_reported": st_recycled.running,
    })))
}

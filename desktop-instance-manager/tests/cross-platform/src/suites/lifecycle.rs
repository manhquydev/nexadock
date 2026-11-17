//! Process Lifecycle Test Matrix (`LC-01` .. `LC-09`).
//!
//! Covers:
//! - Launching 1, 2, 3, and 5 concurrent isolated instances
//! - Selective single-instance stop without disturbing sibling instances
//! - Stopping all managed instances cleanly
//! - Protecting unmanaged external processes and primary Zalo PIDs
//! - Crash recovery after `mid-write` / `after-lock` crashes
//! - Duplicate launch while running (focus/restore routing)
//! - Slow-startup pre-lock visibility & stop race (`DEFECT-03`)
//! - Concurrent launch/stop race across threads (`DEFECT-04`)
//! - Hung process (`--hang-on-quit`) & custom-exe allowlist cleanup (`DEFECT-06`)

use std::collections::HashSet;
use std::fs;
use std::process::{Command, Stdio};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

use dim_core::{DimError, InstanceLauncher, ProfileStore};
use dim_process_manager::PrimaryProtectionGuard;
use dim_xf_fixture::{is_os_pid_alive, FixturePersistentState};
use serde_json::json;

use crate::harness::{
    force_kill_unprotected_test_pid, make_node_fixture_target, run_case_if, xf_fixture_bin_path,
    xf_fixture_mjs_path, BystanderSnapshot, CaseSelectionFilter, CaseSuccess, IsolatedSandbox,
    TestCaseOutcome,
};

/// Executes all Process Lifecycle scenarios (`LC-01` .. `LC-09`).
pub fn run_lifecycle_suite(seed: u64) -> Vec<TestCaseOutcome> {
    run_lifecycle_suite_filtered(seed, None)
}

/// Executes Process Lifecycle scenarios (`LC-01` .. `LC-09`) matching `filter`.
pub fn run_lifecycle_suite_filtered(
    seed: u64,
    filter: Option<&CaseSelectionFilter>,
) -> Vec<TestCaseOutcome> {
    [
        run_case_if(
            filter,
            "LC-01",
            "lifecycle",
            "Launch 1, 2, 3, and 5 concurrent isolated fixture instances",
            "all",
            || lc_01_scale_1_2_3_5_instances(seed),
        ),
        run_case_if(
            filter,
            "LC-02",
            "lifecycle",
            "Selective stop of 1 instance while 4 sibling instances stay active",
            "all",
            || lc_02_selective_single_stop(seed),
        ),
        run_case_if(
            filter,
            "LC-03",
            "lifecycle",
            "Stop all managed instances and verify clean shutdown persistence",
            "all",
            || lc_03_stop_all_managed(seed),
        ),
        run_case_if(
            filter,
            "LC-04",
            "lifecycle",
            "Protect unmanaged external processes and primary Zalo PIDs from termination",
            "all",
            lc_04_protect_unmanaged_and_primary,
        ),
        run_case_if(
            filter,
            "LC-05",
            "lifecycle",
            "Relaunch and recover state after mid-write simulated crash",
            "all",
            lc_05_relaunch_after_crash,
        ),
        run_case_if(
            filter,
            "LC-06",
            "lifecycle",
            "Duplicate launch of an already-running profile routes to focus without spawning second instance",
            "all",
            lc_06_duplicate_launch_idempotency,
        ),
        run_case_if(
            filter,
            "LC-07",
            "lifecycle",
            "Slow pre-lock startup (`--slow-startup-pre-lock-ms`) visibility and stop safety",
            "all",
            lc_07_slow_startup_stop_visibility,
        ),
        run_case_if(
            filter,
            "LC-08",
            "lifecycle",
            "Concurrent multi-threaded launch/stop race & multi-store `profiles.json.tmp` write safety",
            "all",
            lc_08_concurrent_launch_stop_race,
        ),
        run_case_if(
            filter,
            "LC-09",
            "lifecycle",
            "Simulated process hang (`--hang-on-quit`) forced cleanup across NodeFixture & CustomExecutable",
            "all",
            lc_09_hung_process_forced_cleanup,
        ),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn lc_01_scale_1_2_3_5_instances(seed: u64) -> Result<CaseSuccess, String> {
    let mut sandbox = IsolatedSandbox::new()?;
    let mut profiles = Vec::with_capacity(5);
    for idx in 1..=5 {
        let rec = sandbox
            .store
            .create_profile(&format!("Scale Instance {idx}"), "lc-01")
            .map_err(|e| e.to_string())?;
        profiles.push(rec);
    }

    let mut scale_checkpoints = Vec::new();

    for &target_count in &[1usize, 2, 3, 5] {
        for (idx, prof) in profiles.iter().take(target_count).enumerate() {
            let cur = sandbox
                .coordinator
                .inspect_profile(&prof.id)
                .map_err(|e| e.to_string())?;
            if !cur.running {
                let uid = format!("{}", 910_001 + idx);
                let target =
                    make_node_fixture_target(&["--user-id", &uid, "--seed", &seed.to_string()]);
                let launched = sandbox
                    .coordinator
                    .launch_profile(&prof.id, &target)
                    .map_err(|e| format!("failed launching profile {}: {e}", idx + 1))?;
                if let Some(pid) = launched.main_pid {
                    sandbox.track_pid(pid);
                }
            }
        }

        // Wait briefly for any newly started Node fixture on POSIX to flush lockfile
        thread::sleep(Duration::from_millis(180));

        let mut running_pids = HashSet::new();
        let mut running_uids = HashSet::new();
        let mut running_dirs = HashSet::new();

        for prof in profiles.iter().take(target_count) {
            let st = sandbox
                .coordinator
                .inspect_profile(&prof.id)
                .map_err(|e| e.to_string())?;
            if !st.running {
                return Err(format!(
                    "expected profile '{}' running at scale={target_count}",
                    prof.name
                ));
            }
            let pid = st.main_pid.ok_or("missing main_pid on running profile")?;
            sandbox.track_pid(pid);
            if !is_os_pid_alive(pid) {
                return Err(format!("PID {pid} reported running but not alive in OS"));
            }
            if !running_pids.insert(pid) {
                return Err(format!("duplicate PID {pid} across profiles"));
            }
            running_dirs.insert(st.zalo_data_dir.display().to_string());
            if let Some(uid) = st.detected_zalo_user_id {
                running_uids.insert(uid);
            }
        }

        if running_pids.len() != target_count || running_dirs.len() != target_count {
            return Err(format!(
                "isolation mismatch at scale={target_count}: pids={}, dirs={}",
                running_pids.len(),
                running_dirs.len()
            ));
        }

        scale_checkpoints.push(json!({
            "scale": target_count,
            "distinct_pids": running_pids.len(),
            "distinct_dirs": running_dirs.len(),
            "distinct_user_ids": running_uids.len(),
        }));
    }

    let iso_target = make_node_fixture_target(&[]);
    let iso = sandbox
        .coordinator
        .verify_isolation(&*sandbox.adapter, None, Some(&iso_target))
        .map_err(|e| e.to_string())?;

    for prof in &profiles {
        let _ = sandbox.coordinator.stop_profile(&prof.id);
    }

    Ok(CaseSuccess::pass(json!({
        "checkpoints": scale_checkpoints,
        "verify_isolation_checks": iso.checks,
    })))
}

fn lc_02_selective_single_stop(seed: u64) -> Result<CaseSuccess, String> {
    let mut sandbox = IsolatedSandbox::new()?;
    let mut profiles = Vec::with_capacity(5);
    let mut initial_pids = Vec::with_capacity(5);

    for idx in 0..5 {
        let rec = sandbox
            .store
            .create_profile(&format!("Selective {}", idx + 1), "")
            .map_err(|e| e.to_string())?;
        let uid = format!("{}", 920_001 + idx);
        let target = make_node_fixture_target(&["--user-id", &uid, "--seed", &seed.to_string()]);
        let _st = sandbox
            .coordinator
            .launch_profile(&rec.id, &target)
            .map_err(|e| e.to_string())?;
        thread::sleep(Duration::from_millis(80));
        let refreshed = sandbox
            .coordinator
            .inspect_profile(&rec.id)
            .map_err(|e| e.to_string())?;
        let pid = refreshed.main_pid.ok_or("expected main_pid after launch")?;
        sandbox.track_pid(pid);
        initial_pids.push(pid);
        profiles.push(rec);
    }

    // Stop ONLY index 2 (3rd instance)
    let victim = &profiles[2];
    let victim_pid = initial_pids[2];
    sandbox
        .coordinator
        .stop_profile(&victim.id)
        .map_err(|e| e.to_string())?;

    let victim_after = sandbox
        .coordinator
        .inspect_profile(&victim.id)
        .map_err(|e| e.to_string())?;
    if victim_after.running || is_os_pid_alive(victim_pid) {
        return Err(format!(
            "stopped profile '{}' (PID {victim_pid}) remained alive",
            victim.name
        ));
    }

    let mut survivors_intact = 0usize;
    for (idx, prof) in profiles.iter().enumerate() {
        if idx == 2 {
            continue;
        }
        let st = sandbox
            .coordinator
            .inspect_profile(&prof.id)
            .map_err(|e| e.to_string())?;
        if !st.running
            || st.main_pid != Some(initial_pids[idx])
            || !is_os_pid_alive(initial_pids[idx])
        {
            return Err(format!(
                "sibling profile '{}' was disrupted by selective stop",
                prof.name
            ));
        }
        survivors_intact += 1;
    }

    for (idx, prof) in profiles.iter().enumerate() {
        if idx != 2 {
            let _ = sandbox.coordinator.stop_profile(&prof.id);
        }
    }

    Ok(CaseSuccess::pass(json!({
        "stopped_profile_id": victim.id,
        "stopped_pid": victim_pid,
        "surviving_siblings_verified": survivors_intact,
    })))
}

fn lc_03_stop_all_managed(seed: u64) -> Result<CaseSuccess, String> {
    let mut sandbox = IsolatedSandbox::new()?;
    let mut records = Vec::new();
    let mut pids = Vec::new();

    for idx in 0..4 {
        let rec = sandbox
            .store
            .create_profile(&format!("Batch Stop {}", idx + 1), "")
            .map_err(|e| e.to_string())?;
        let uid = format!("{}", 930_001 + idx);
        let target = make_node_fixture_target(&[
            "--user-id",
            &uid,
            "--seed",
            &seed.to_string(),
            "--set-kv",
            "session=active",
        ]);
        let _ = sandbox
            .coordinator
            .launch_profile(&rec.id, &target)
            .map_err(|e| e.to_string())?;
        thread::sleep(Duration::from_millis(80));
        let st = sandbox
            .coordinator
            .inspect_profile(&rec.id)
            .map_err(|e| e.to_string())?;
        let pid = st.main_pid.ok_or("missing main_pid")?;
        sandbox.track_pid(pid);
        pids.push(pid);
        records.push((rec, st.zalo_data_dir));
    }

    for (rec, _) in &records {
        sandbox
            .coordinator
            .stop_profile(&rec.id)
            .map_err(|e| e.to_string())?;
    }

    let mut clean_count = 0usize;
    for (rec, zdata) in &records {
        let st = sandbox
            .coordinator
            .inspect_profile(&rec.id)
            .map_err(|e| e.to_string())?;
        if st.running || st.lockfile_path.exists() {
            return Err(format!(
                "profile '{}' still running or lockfile remained",
                rec.name
            ));
        }
        let persisted_raw = fs::read_to_string(zdata.join("fixture-state.json"))
            .map_err(|e| format!("missing fixture-state.json: {e}"))?;
        let persisted: FixturePersistentState =
            serde_json::from_str(&persisted_raw).map_err(|e| e.to_string())?;
        if persisted.clean_shutdown && persisted.clean_shutdown_count == 1 {
            clean_count += 1;
        }
    }

    let alive_after: Vec<u32> = pids.into_iter().filter(|&p| is_os_pid_alive(p)).collect();
    if !alive_after.is_empty() {
        return Err(format!("PIDs survived stop-all: {alive_after:?}"));
    }

    Ok(CaseSuccess::pass(json!({
        "managed_instances_stopped": records.len(),
        "clean_shutdown_persisted_count": clean_count,
        "surviving_pids": alive_after,
    })))
}

fn lc_04_protect_unmanaged_and_primary() -> Result<CaseSuccess, String> {
    let mut sandbox = IsolatedSandbox::new()?;

    // Spawn an completely unmanaged external process outside DIM_DATA_DIR
    let unmanaged_dir = sandbox.root().join("external-unmanaged-app");
    let unmanaged_zdata = sandbox.zalo_data_dir_for(&unmanaged_dir);
    fs::create_dir_all(&unmanaged_zdata).map_err(|e| e.to_string())?;

    let mut ext_child = Command::new("node")
        .arg(xf_fixture_mjs_path())
        .arg("--instance-id=external-unmanaged")
        .arg("--profile-dir")
        .arg(&unmanaged_dir)
        .arg("--user-id=9990001")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("failed to spawn unmanaged external process: {e}"))?;
    let ext_pid = ext_child.id();
    sandbox.track_pid(ext_pid);

    let dl = Instant::now() + Duration::from_secs(4);
    while Instant::now() < dl && !unmanaged_zdata.join("lockfile").exists() {
        thread::sleep(Duration::from_millis(20));
    }

    // Launch & stop a managed profile; verify external unmanaged PID is completely untouched.
    let managed = sandbox
        .store
        .create_profile("Managed Scoped", "")
        .map_err(|e| e.to_string())?;
    let target = make_node_fixture_target(&["--user-id=940001"]);
    let st = sandbox
        .coordinator
        .launch_profile(&managed.id, &target)
        .map_err(|e| e.to_string())?;
    if let Some(pid) = st.main_pid {
        sandbox.track_pid(pid);
    }
    sandbox
        .coordinator
        .stop_profile(&managed.id)
        .map_err(|e| e.to_string())?;

    if !is_os_pid_alive(ext_pid) {
        return Err(
            "unmanaged external process was killed when stopping managed profile".to_owned(),
        );
    }

    // Verify both PrimaryProtectionGuard AND adapter.stop() reject protected PIDs before touching the OS!
    let mut forged_state = sandbox
        .coordinator
        .inspect_profile(&managed.id)
        .map_err(|e| e.to_string())?;
    forged_state.running = true;
    forged_state.main_pid = Some(ext_pid);
    forged_state.process_ids = vec![ext_pid];

    let stop_err = sandbox
        .adapter
        .stop(&forged_state, &[ext_pid], 500)
        .expect_err("adapter.stop must refuse protected primary PID");
    if !matches!(stop_err, DimError::ProtectedPrimaryProcess(_)) {
        return Err(format!("unexpected error variant: {stop_err:?}"));
    }
    if !is_os_pid_alive(ext_pid) {
        return Err(
            "protected PID was terminated despite ProtectedPrimaryProcess guard!".to_owned(),
        );
    }

    let synth_primary = dim_core::PrimaryInstanceStatus {
        zalo_data_dir: unmanaged_zdata.clone(),
        lockfile_path: unmanaged_zdata.join("lockfile"),
        running: true,
        protected_pids: vec![ext_pid],
        process_count: 1,
        working_set_bytes: 0,
        detected_zalo_user_id: Some("9990001".to_owned()),
        notes: Vec::new(),
    };
    let guard_err = PrimaryProtectionGuard::ensure_safe_to_stop(&forged_state, &synth_primary)
        .expect_err("PrimaryProtectionGuard must block protected PID");
    if guard_err.code() != "PROTECTED_PRIMARY_PROCESS" {
        return Err(format!("unexpected guard code: {}", guard_err.code()));
    }

    // Cleanly shut down our unmanaged test helper
    let _ = fs::write(unmanaged_zdata.join(".quit-signal"), b"{}\n");
    let _ = ext_child.wait();

    Ok(CaseSuccess::pass(json!({
        "unmanaged_pid_survived_managed_stop": true,
        "protected_pid_guard_code": stop_err.code(),
    })))
}

fn lc_05_relaunch_after_crash() -> Result<CaseSuccess, String> {
    let mut sandbox = IsolatedSandbox::new()?;
    let prof = sandbox
        .store
        .create_profile("MidWrite Crash Profile", "")
        .map_err(|e| e.to_string())?;

    let crash_target = make_node_fixture_target(&[
        "--user-id=950001",
        "--crash-mode=mid-write",
        "--crash-delay-ms=90",
        "--set-kv=token_epoch=epoch-1",
    ]);
    let st_crash_launch = sandbox
        .coordinator
        .launch_profile(&prof.id, &crash_target)
        .map_err(|e| e.to_string())?;
    if let Some(pid) = st_crash_launch.main_pid {
        sandbox.track_pid(pid);
    }

    // Wait for simulated mid-write crash to complete
    thread::sleep(Duration::from_millis(450));

    let st_after_crash = sandbox
        .coordinator
        .inspect_profile(&prof.id)
        .map_err(|e| e.to_string())?;
    if st_after_crash.running {
        return Err("crashed fixture unexpectedly remained running".to_owned());
    }
    let torn_exists = st_after_crash
        .zalo_data_dir
        .join("storage")
        .join("torn_write.bin.tmp")
        .exists();
    if !st_after_crash.stale_lock_detected {
        return Err("expected stale_lock_detected=true after mid-write crash".to_owned());
    }

    // Relaunch cleanly and verify stale lock recovery + persisted state continuity
    let normal_target = make_node_fixture_target(&["--user-id=950001", "--set-kv=recovered=yes"]);
    let st_relaunch = sandbox
        .coordinator
        .launch_profile(&prof.id, &normal_target)
        .map_err(|e| e.to_string())?;
    thread::sleep(Duration::from_millis(150));
    let st_live = sandbox
        .coordinator
        .inspect_profile(&prof.id)
        .map_err(|e| e.to_string())?;
    if let Some(pid) = st_live.main_pid.or(st_relaunch.main_pid) {
        sandbox.track_pid(pid);
    }
    if !st_live.running {
        return Err("profile failed to relaunch after crash recovery".to_owned());
    }

    let state_raw = fs::read_to_string(st_live.zalo_data_dir.join("fixture-state.json"))
        .map_err(|e| e.to_string())?;
    let fstate: FixturePersistentState =
        serde_json::from_str(&state_raw).map_err(|e| e.to_string())?;

    sandbox
        .coordinator
        .stop_profile(&prof.id)
        .map_err(|e| e.to_string())?;

    if fstate.boot_sequence != 2 || !fstate.unclean_previous_boot {
        return Err(format!(
            "unexpected state continuity: boot_sequence={}, unclean_previous_boot={}",
            fstate.boot_sequence, fstate.unclean_previous_boot
        ));
    }
    if fstate.persisted_kv.get("token_epoch").map(String::as_str) != Some("epoch-1") {
        return Err("persisted_kv from pre-crash boot was lost".to_owned());
    }

    Ok(CaseSuccess::pass(json!({
        "stale_lock_detected_before_relaunch": st_after_crash.stale_lock_detected,
        "torn_tmp_artifact_observed": torn_exists,
        "boot_sequence_after_recovery": fstate.boot_sequence,
        "unclean_previous_boot_flagged": fstate.unclean_previous_boot,
    })))
}

fn lc_06_duplicate_launch_idempotency() -> Result<CaseSuccess, String> {
    let mut sandbox = IsolatedSandbox::new()?;
    let prof = sandbox
        .store
        .create_profile("Duplicate Launch Guard", "")
        .map_err(|e| e.to_string())?;
    let target = make_node_fixture_target(&["--user-id=960001"]);

    let first = sandbox
        .coordinator
        .launch_profile(&prof.id, &target)
        .map_err(|e| e.to_string())?;
    thread::sleep(Duration::from_millis(150));
    let first_live = sandbox
        .coordinator
        .inspect_profile(&prof.id)
        .map_err(|e| e.to_string())?;
    let pid1 = first_live
        .main_pid
        .or(first.main_pid)
        .ok_or("missing initial PID")?;
    sandbox.track_pid(pid1);

    let second = sandbox
        .coordinator
        .launch_profile(&prof.id, &target)
        .map_err(|e| e.to_string())?;
    let pid2 = second
        .main_pid
        .ok_or("missing PID on second launch_profile")?;
    if pid1 != pid2 {
        return Err(format!(
            "duplicate launch spawned a different PID ({pid1} vs {pid2})"
        ));
    }

    thread::sleep(Duration::from_millis(200));
    let state_raw = fs::read_to_string(first_live.zalo_data_dir.join("fixture-state.json"))
        .map_err(|e| e.to_string())?;
    let fstate: FixturePersistentState =
        serde_json::from_str(&state_raw).map_err(|e| e.to_string())?;

    sandbox
        .coordinator
        .stop_profile(&prof.id)
        .map_err(|e| e.to_string())?;

    if fstate.focus_count == 0 {
        return Ok(CaseSuccess::defect(
            "DEFECT-05",
            &[
                "crates/platform-linux/src/adapter.rs:310-318",
                "crates/platform-macos/src/adapter.rs:331-356",
            ],
            "Launch a headless fixture profile, then call `launch_profile` or `focus_profile` again; observe `focus_count == 0` because `focus_or_restore` does not write `.focus-signal` or invoke second-instance signaling on POSIX.",
            json!({
                "pid_preserved": pid1,
                "focus_count": fstate.focus_count,
            }),
        ));
    }

    Ok(CaseSuccess::pass(json!({
        "pid_preserved": pid1,
        "focus_count": fstate.focus_count,
        "boot_sequence": fstate.boot_sequence,
    })))
}

fn lc_07_slow_startup_stop_visibility() -> Result<CaseSuccess, String> {
    let mut sandbox = IsolatedSandbox::new()?;
    let bystander = BystanderSnapshot::seed(
        &sandbox,
        "LC07 Bystander Sibling",
        "LC07_BYSTANDER_CANARY_V1",
    )?;
    let bystander_target = make_node_fixture_target(&["--user-id=970099"]);
    let bystander_launch = sandbox
        .coordinator
        .launch_profile(&bystander.record.id, &bystander_target)
        .map_err(|e| format!("failed launching LC-07 bystander profile: {e}"))?;
    let dl = Instant::now() + Duration::from_secs(3);
    while Instant::now() < dl && !bystander.zalo_data_dir.join("lockfile").exists() {
        thread::sleep(Duration::from_millis(15));
    }
    let bystander_live = sandbox
        .coordinator
        .inspect_profile(&bystander.record.id)
        .map_err(|e| e.to_string())?;
    let bystander_pid = bystander_live
        .main_pid
        .or(bystander_launch.main_pid)
        .ok_or("LC-07 bystander profile missing running PID")?;
    sandbox.track_pid(bystander_pid);

    let prof = sandbox
        .store
        .create_profile("Slow PreLock Startup", "")
        .map_err(|e| e.to_string())?;
    let pdir = sandbox
        .store
        .profile_dir_for(&prof)
        .map_err(|e| e.to_string())?;

    // 3,100ms pre-lock delay exceeds Windows 2,500ms NodeFixture wait (and 0ms on macOS/Linux/CustomExe)
    let slow_target =
        make_node_fixture_target(&["--user-id=970001", "--slow-startup-pre-lock-ms=3100"]);

    let rt_after_launch = sandbox
        .coordinator
        .launch_profile(&prof.id, &slow_target)
        .map_err(|e| e.to_string())?;

    let hint_pid = fs::read_to_string(pdir.join(".runtime-pid"))
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok());
    if let Some(hpid) = hint_pid {
        sandbox.track_pid(hpid);
    }

    let stop_count = sandbox
        .coordinator
        .stop_profile(&prof.id)
        .map_err(|e| e.to_string())?;

    let orphan_still_alive = hint_pid.is_some_and(is_os_pid_alive);
    if let Some(hpid) = hint_pid {
        if orphan_still_alive {
            let _ = force_kill_unprotected_test_pid(hpid);
        }
    }

    // Verify live sibling bystander process was NOT killed by `stop_profile(&prof.id)`!
    let bystander_after_stop = sandbox
        .coordinator
        .inspect_profile(&bystander.record.id)
        .map_err(|e| e.to_string())?;
    if !bystander_after_stop.running
        || bystander_after_stop.main_pid != Some(bystander_pid)
        || !is_os_pid_alive(bystander_pid)
    {
        return Err(format!(
            "LC-07 collateral damage: bystander sibling PID {bystander_pid} was disrupted during stop_profile"
        ));
    }
    sandbox
        .coordinator
        .stop_profile(&bystander.record.id)
        .map_err(|e| e.to_string())?;
    let bystander_ev = bystander.verify_untouched(&sandbox)?;

    if !rt_after_launch.running || orphan_still_alive {
        return Ok(CaseSuccess::defect(
            "DEFECT-03",
            &[
                "crates/platform-windows/src/adapter.rs:512-522,620-631",
                "crates/platform-macos/src/adapter.rs:192-193,298,317",
                "crates/platform-linux/src/adapter.rs:189,298-299",
            ],
            "Launch a profile with `--slow-startup-pre-lock-ms=3100`, then immediately call `stop_profile`: `launch_profile` returns `running=false, main_pid=null` while `.runtime-pid` process is alive, and `stop_profile` returns `0` leaving the starting process orphaned in the background.",
            json!({
                "launch_returned_running": rt_after_launch.running,
                "launch_returned_main_pid": rt_after_launch.main_pid,
                "runtime_pid_hint_on_disk": hint_pid,
                "stop_profile_terminated_count": stop_count,
                "orphan_process_survived_stop": orphan_still_alive,
                "bystander_running_preserved_during_stop": true,
                "bystander_non_regression": bystander_ev,
            }),
        ));
    }

    Ok(CaseSuccess::pass(json!({
        "launch_returned_running": rt_after_launch.running,
        "stop_profile_terminated_count": stop_count,
        "bystander_running_preserved_during_stop": true,
        "bystander_non_regression": bystander_ev,
    })))
}

fn lc_08_concurrent_launch_stop_race() -> Result<CaseSuccess, String> {
    let mut sandbox = IsolatedSandbox::new()?;
    let bystander = BystanderSnapshot::seed(
        &sandbox,
        "LC08 Bystander Sibling",
        "LC08_BYSTANDER_CANARY_V1",
    )?;
    let prof = sandbox
        .store
        .create_profile("Concurrent Race Target", "")
        .map_err(|e| e.to_string())?;
    let pdir = sandbox
        .store
        .profile_dir_for(&prof)
        .map_err(|e| e.to_string())?;
    let zdata = sandbox.zalo_data_dir_for(&pdir);

    // Part A (DEFECT-04a): 4 threads racing `InstanceCoordinator::launch_profile` on the same profile.
    let store = Arc::clone(&sandbox.store);
    let adapter = Arc::clone(&sandbox.adapter);
    let coord = Arc::new(dim_process_manager::InstanceCoordinator::new(
        store, adapter,
    ));
    let barrier = Arc::new(Barrier::new(4));

    let mut handles = Vec::with_capacity(4);
    for _ in 0..4 {
        let c = Arc::clone(&coord);
        let b = Arc::clone(&barrier);
        let pid_str = prof.id.clone();
        handles.push(thread::spawn(move || {
            let target =
                make_node_fixture_target(&["--user-id=980001", "--slow-startup-pre-lock-ms=220"]);
            b.wait();
            c.launch_profile(&pid_str, &target)
        }));
    }

    let mut returned_pids = Vec::new();
    for h in handles {
        if let Ok(Ok(rt)) = h.join() {
            if let Some(pid) = rt.main_pid {
                sandbox.track_pid(pid);
                returned_pids.push(pid);
            }
        }
    }

    thread::sleep(Duration::from_millis(350));
    let startup_log = fs::read_to_string(zdata.join("startup.log")).unwrap_or_default();
    let rejected_collisions = startup_log
        .lines()
        .filter(|l| l.contains("SECOND_INSTANCE_REJECTED"))
        .count();

    let _ = sandbox.coordinator.stop_profile(&prof.id);
    sandbox.reap_all_sandbox_processes();
    let bystander_ev = bystander.verify_untouched(&sandbox)?;

    // Part B (DEFECT-04b): 4 independent `JsonProfileStore` instances racing on `profiles.json.tmp`
    // (simulating 4 concurrent `dim-cli` / Tauri processes mutating the same `DIM_DATA_DIR`).
    let store_race_dir = sandbox.root().join("concurrent-store-race");
    let seed_store = dim_profile_store::JsonProfileStore::new(store_race_dir.clone())
        .map_err(|e| e.to_string())?;
    seed_store.ensure_initialized().map_err(|e| e.to_string())?;
    let race_bystander = seed_store
        .create_profile("Store Race Bystander", "bystander")
        .map_err(|e| e.to_string())?;
    let race_bystander_dir = seed_store
        .profile_dir_for(&race_bystander)
        .map_err(|e| e.to_string())?;
    let race_canary = race_bystander_dir.join("bystander-canary.dat");
    fs::write(&race_canary, b"LC08_STORE_RACE_BYSTANDER_V1").map_err(|e| e.to_string())?;

    let store_barrier = Arc::new(Barrier::new(4));
    let mut store_handles = Vec::with_capacity(4);
    for t_idx in 0..4usize {
        let dir_clone = store_race_dir.clone();
        let sb = Arc::clone(&store_barrier);
        store_handles.push(thread::spawn(move || -> usize {
            let Ok(independent_store) = dim_profile_store::JsonProfileStore::new(dir_clone) else {
                return 5;
            };
            let mut errs = 0usize;
            for round in 0..5usize {
                sb.wait();
                if independent_store
                    .create_profile(&format!("Concurrent Writer T{t_idx} R{round}"), "")
                    .is_err()
                {
                    errs += 1;
                }
            }
            errs
        }));
    }

    let mut store_write_errors = 0usize;
    for sh in store_handles {
        store_write_errors += sh.join().unwrap_or(5);
    }

    if fs::read_to_string(&race_canary).ok().as_deref() != Some("LC08_STORE_RACE_BYSTANDER_V1") {
        return Err("concurrent store race corrupted bystander directory contents".to_owned());
    }

    let expected_store_profiles = 1 + (4 * 5);
    let (persisted_store_profiles, store_corrupted_after_race, race_bystander_metadata_present) =
        match seed_store.list_profiles() {
            Ok(list) => {
                let has_bystander = list.iter().any(|p| p.id == race_bystander.id);
                (list.len(), false, has_bystander)
            }
            Err(_) => (0, true, false),
        };
    let store_tmp_race_failed = store_write_errors > 0
        || store_corrupted_after_race
        || persisted_store_profiles != expected_store_profiles
        || !race_bystander_metadata_present;

    // If Coordinator lacked per-profile launch serialization (`rejected_collisions >= 2`) OR
    // `JsonProfileStore` lacked cross-instance locking / unique temp staging (`store_tmp_race_failed`):
    if rejected_collisions >= 2 || store_tmp_race_failed {
        return Ok(CaseSuccess::defect(
            "DEFECT-04",
            &[
                "crates/process-manager/src/coordinator.rs:49-104",
                "crates/profile-store/src/store.rs:169-180",
                "scripts/dim-host.ps1:401-415",
            ],
            "Invoke `InstanceCoordinator::launch_profile` concurrently from 4 threads on the same stopped profile (`--slow-startup-pre-lock-ms=220`) and run 4 independent `JsonProfileStore` writers against one `data_dir`: concurrent launches spawn racing OS processes (`DEFECT-04a`), and shared `profiles.json.tmp` / unlocked read-modify-write loses updates or fails `fs::rename` (`DEFECT-04b`).",
            json!({
                "concurrent_threads": 4,
                "second_instance_collisions_logged": rejected_collisions,
                "returned_pids": returned_pids,
                "concurrent_store_write_errors": store_write_errors,
                "concurrent_store_expected_profiles": expected_store_profiles,
                "concurrent_store_persisted_profiles": persisted_store_profiles,
                "concurrent_store_corrupted": store_corrupted_after_race,
                "bystander_non_regression": bystander_ev,
            }),
        ));
    }

    Ok(CaseSuccess::pass(json!({
        "concurrent_threads": 4,
        "second_instance_collisions_logged": rejected_collisions,
        "concurrent_store_write_errors": store_write_errors,
        "concurrent_store_persisted_profiles": persisted_store_profiles,
        "bystander_non_regression": bystander_ev,
    })))
}

fn lc_09_hung_process_forced_cleanup() -> Result<CaseSuccess, String> {
    let mut sandbox = IsolatedSandbox::new()?;

    // Part A: NodeFixture with `--hang-on-quit`
    let node_prof = sandbox
        .store
        .create_profile("Hung Node Profile", "")
        .map_err(|e| e.to_string())?;
    let hang_target = make_node_fixture_target(&["--user-id=990001", "--hang-on-quit"]);
    let st_hang = sandbox
        .coordinator
        .launch_profile(&node_prof.id, &hang_target)
        .map_err(|e| e.to_string())?;
    thread::sleep(Duration::from_millis(120));
    let st_live = sandbox
        .coordinator
        .inspect_profile(&node_prof.id)
        .map_err(|e| e.to_string())?;
    let node_pid = st_live
        .main_pid
        .or(st_hang.main_pid)
        .ok_or("missing hung node PID")?;
    sandbox.track_pid(node_pid);

    let stop_count = sandbox
        .coordinator
        .stop_profile(&node_prof.id)
        .map_err(|e| e.to_string())?;
    let node_survived = is_os_pid_alive(node_pid);
    if node_survived {
        let _ = force_kill_unprotected_test_pid(node_pid);
        return Ok(CaseSuccess::defect(
            "DEFECT-06",
            &["crates/platform-macos/src/adapter.rs:386-404"],
            "Launch a fixture with `--hang-on-quit` and call `stop_profile`: `MacOsPlatformAdapter::stop` never escalates from `SIGTERM` to `SIGKILL` after `timeout_ms` and returns `Ok(0)` while the hung process remains alive.",
            json!({
                "hung_node_pid": node_pid,
                "stop_returned_count": stop_count,
                "hung_process_survived": true,
            }),
        ));
    }

    // Part B (Windows): Native `xf-desktop-fixture.exe` holding real exclusive Win32 lock with `--hang-on-quit`
    #[cfg(target_os = "windows")]
    {
        let native_bin = xf_fixture_bin_path();
        if native_bin.exists() {
            let cust_prof = sandbox
                .store
                .create_profile("Hung CustomExe Profile", "")
                .map_err(|e| e.to_string())?;
            let cust_dir = sandbox
                .store
                .profile_dir_for(&cust_prof)
                .map_err(|e| e.to_string())?;
            sandbox
                .adapter
                .ensure_profile_layout(&cust_prof, &cust_dir)
                .map_err(|e| e.to_string())?;
            let zdata = sandbox.zalo_data_dir_for(&cust_dir);

            if let Ok(mut child) = Command::new(&native_bin)
                .arg("--instance-id=hung-custom-exe")
                .arg("--profile-dir")
                .arg(&cust_dir)
                .arg("--user-id=990002")
                .arg("--lock-mode=exclusive")
                .arg("--hang-on-quit")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                let cpid = child.id();
                sandbox.track_pid(cpid);
                let dl = Instant::now() + Duration::from_secs(3);
                while Instant::now() < dl && !zdata.join("fixture-state.json").exists() {
                    thread::sleep(Duration::from_millis(20));
                }

                let inspected = sandbox
                    .coordinator
                    .inspect_profile(&cust_prof.id)
                    .map_err(|e| e.to_string())?;
                let stop_res = sandbox
                    .coordinator
                    .stop_profile(&cust_prof.id)
                    .map_err(|e| e.to_string())?;
                let custom_survived = is_os_pid_alive(cpid);

                let _ = child.kill();
                let _ = child.wait();

                if inspected.running && custom_survived {
                    return Ok(CaseSuccess::defect(
                        "DEFECT-06",
                        &[
                            "crates/platform-windows/src/adapter.rs:492-498,771-785",
                            "scripts/dim-host.ps1:1032-1036,1117-1119,2295-2298",
                            "crates/platform-macos/src/adapter.rs:386-404",
                        ],
                        "Start `xf-desktop-fixture.exe --lock-mode=exclusive --hang-on-quit` on a profile: `WindowsPlatformAdapter::inspect_profile` detects `running=true, main_pid=<pid>` via `rstrtmgr.dll`, but `stop_profile()` skips `TerminateProcess` because `allowed_exe` hardcodes `zalo*`/`zocr*`/`node.exe`/`dim-test-fixture*`, returning `Ok(0)` while the hung process keeps holding `lockfile`.",
                        json!({
                            "node_hang_escalated_cleanly": !node_survived,
                            "custom_exe_detected_running_via_rstrtmgr": inspected.running,
                            "custom_exe_pid": cpid,
                            "stop_returned_count": stop_res,
                            "custom_exe_survived_stop": custom_survived,
                        }),
                    ));
                }
            }
        }
    }

    Ok(CaseSuccess::pass(json!({
        "hung_node_pid": node_pid,
        "stop_returned_count": stop_count,
        "hung_process_terminated": !node_survived,
    })))
}

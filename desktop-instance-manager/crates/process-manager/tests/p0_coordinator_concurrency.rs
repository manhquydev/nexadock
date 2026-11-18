//! P0 per-profile launch serialization, parallel distinct-profile launch, launch/stop race,
//! create/launch/delete non-resurrection, multi-process `.dim-profile.lock` contention, and
//! runtime lease crash-recovery tests for `dim-process-manager`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::Duration;

use dim_core::{
    build_runtime_lease, remove_runtime_lease, verify_runtime_lease, write_runtime_lease,
    AdvisoryFileLock, DimError, InstanceLauncher, InstanceMonitor, LaunchTarget, LeaseVerification,
    PlatformCapabilities, PlatformCapabilityReport, PlatformKind, PrimaryInstanceStatus,
    ProfileLifecycleState, ProfileRecord, ProfileRuntimeState, ProfileStore, RecoveryManager,
    SupportStatus, MIN_FREE_DISK_BYTES, PROFILE_OP_LOCK_FILE, RUNTIME_LEASE_FILE, RUNTIME_PID_FILE,
};
use dim_process_manager::{InstanceCoordinator, StaleLockRecoveryManager};
use dim_profile_store::{write_profile_id_marker, JsonProfileStore};

#[derive(Clone, Copy)]
struct ActiveProc {
    pid: u32,
    starting: bool,
}

struct ConcurrencyTestAdapter {
    primary_dir: PathBuf,
    procs: Mutex<HashMap<String, ActiveProc>>,
    spawn_calls: AtomicUsize,
    next_pid: AtomicUsize,
    launch_delay_ms: u64,
    parallel_barrier: Option<Arc<Barrier>>,
}

impl ConcurrencyTestAdapter {
    fn new(
        primary_dir: PathBuf,
        launch_delay_ms: u64,
        parallel_barrier: Option<Arc<Barrier>>,
    ) -> Self {
        Self {
            primary_dir,
            procs: Mutex::new(HashMap::new()),
            spawn_calls: AtomicUsize::new(0),
            next_pid: AtomicUsize::new(40_000),
            launch_delay_ms,
            parallel_barrier,
        }
    }

    fn active_count(&self) -> usize {
        self.procs.lock().unwrap().len()
    }
}

impl PlatformCapabilities for ConcurrencyTestAdapter {
    fn platform(&self) -> PlatformKind {
        PlatformKind::Windows
    }

    fn capabilities(&self) -> PlatformCapabilityReport {
        PlatformCapabilityReport {
            platform: PlatformKind::Windows,
            overall_status: SupportStatus::PartiallySupported,
            official_zalo_status: SupportStatus::PartiallySupported,
            fixture_status: SupportStatus::SupportedAndTested,
            appimage_status: None,
            concurrent_calls_status: SupportStatus::Blocked,
            folder_redirection_detected: false,
            folder_redirection_path: None,
            limitations: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    fn free_disk_bytes(&self, _path: &Path) -> Result<u64, DimError> {
        Ok(MIN_FREE_DISK_BYTES + 50_000_000)
    }

    fn unlink_profile_junctions(&self, _profile_dir: &Path) -> Result<usize, DimError> {
        Ok(0)
    }
}

impl InstanceMonitor for ConcurrencyTestAdapter {
    fn inspect_primary(&self) -> Result<PrimaryInstanceStatus, DimError> {
        Ok(PrimaryInstanceStatus {
            zalo_data_dir: self.primary_dir.clone(),
            lockfile_path: self.primary_dir.join("lockfile"),
            running: true,
            protected_pids: vec![99_999],
            process_count: 1,
            working_set_bytes: 0,
            detected_zalo_user_id: None,
            notes: Vec::new(),
        })
    }

    fn inspect_profile(
        &self,
        profile: &ProfileRecord,
        profile_dir: &Path,
    ) -> Result<ProfileRuntimeState, DimError> {
        let zalo_data = profile_dir.join("AppData").join("Roaming").join("ZaloData");
        let lockfile = zalo_data.join("lockfile");
        let active = self.procs.lock().unwrap().get(&profile.id).copied();

        let (lifecycle, main_pid) = match active {
            Some(proc_info) if proc_info.starting => {
                (ProfileLifecycleState::Starting, Some(proc_info.pid))
            }
            Some(proc_info) => (ProfileLifecycleState::Running, Some(proc_info.pid)),
            None => (ProfileLifecycleState::Stopped, None),
        };
        let running = lifecycle.is_active();
        let stale = !running
            && (lockfile.exists()
                || profile_dir.join(RUNTIME_LEASE_FILE).exists()
                || profile_dir.join(RUNTIME_PID_FILE).exists());

        Ok(ProfileRuntimeState {
            profile_id: profile.id.clone(),
            profile_name: profile.name.clone(),
            profile_dir: profile_dir.to_path_buf(),
            zalo_data_dir: zalo_data,
            lockfile_path: lockfile,
            running,
            stale_lock_detected: stale,
            main_pid,
            process_ids: main_pid.into_iter().collect(),
            process_count: usize::from(main_pid.is_some()),
            working_set_bytes: 0,
            window_handle: None,
            window_title: None,
            detected_zalo_user_id: None,
            disk_usage_bytes: 0,
            status_label: lifecycle.default_status_label(stale).to_owned(),
        })
    }
}

impl InstanceLauncher for ConcurrencyTestAdapter {
    fn ensure_profile_layout(
        &self,
        profile: &ProfileRecord,
        profile_dir: &Path,
    ) -> Result<(), DimError> {
        let zalo_data = profile_dir.join("AppData").join("Roaming").join("ZaloData");
        fs::create_dir_all(&zalo_data)?;
        if !zalo_data.join(".zalo-profile-id").exists() {
            write_profile_id_marker(&zalo_data, &profile.id)?;
        }
        Ok(())
    }

    fn launch(
        &self,
        _target: &LaunchTarget,
        profile: &ProfileRecord,
        profile_dir: &Path,
    ) -> Result<ProfileRuntimeState, DimError> {
        self.spawn_calls.fetch_add(1, Ordering::SeqCst);
        let pid = u32::try_from(self.next_pid.fetch_add(1, Ordering::SeqCst)).unwrap();
        self.procs.lock().unwrap().insert(
            profile.id.clone(),
            ActiveProc {
                pid,
                starting: true,
            },
        );

        if let Some(ref b) = self.parallel_barrier {
            b.wait();
        }

        let zalo_data = profile_dir.join("AppData").join("Roaming").join("ZaloData");
        let step_ms = 10u64;
        let mut waited = 0u64;
        while waited < self.launch_delay_ms {
            if zalo_data.join(".quit-signal").exists() {
                break;
            }
            thread::sleep(Duration::from_millis(step_ms));
            waited += step_ms;
        }

        if !zalo_data.join(".quit-signal").exists() {
            if let Some(entry) = self.procs.lock().unwrap().get_mut(&profile.id) {
                entry.starting = false;
            }
        }
        self.inspect_profile(profile, profile_dir)
    }

    fn focus_or_restore(
        &self,
        _target: &LaunchTarget,
        _profile: &ProfileRecord,
        _profile_dir: &Path,
        _state: &ProfileRuntimeState,
    ) -> Result<(), DimError> {
        Ok(())
    }

    fn stop(
        &self,
        state: &ProfileRuntimeState,
        protected_primary_pids: &[u32],
        _timeout_ms: u64,
    ) -> Result<u32, DimError> {
        if let Some(pid) = state.main_pid {
            if protected_primary_pids.contains(&pid) {
                return Err(DimError::ProtectedPrimaryProcess("protected".to_owned()));
            }
        }
        let removed = self.procs.lock().unwrap().remove(&state.profile_id);
        let _ = fs::remove_file(state.zalo_data_dir.join(".quit-signal"));
        Ok(u32::from(removed.is_some()))
    }
}

fn dummy_fixture_target() -> LaunchTarget {
    LaunchTarget::NodeFixture {
        node_path: None,
        script_path: PathBuf::from("xf-fixture.mjs"),
        extra_args: Vec::new(),
    }
}

#[test]
fn same_profile_four_concurrent_launches_coalesce_to_single_session() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonProfileStore::new(tmp.path().join("data")).unwrap());
    let adapter = Arc::new(ConcurrencyTestAdapter::new(
        tmp.path().join("primary"),
        120,
        None,
    ));
    let coord = Arc::new(InstanceCoordinator::new(
        Arc::clone(&store),
        Arc::clone(&adapter),
    ));

    let prof = store.create_profile("Same Profile Race", "").unwrap();
    let barrier = Arc::new(Barrier::new(4));
    let mut handles = Vec::with_capacity(4);

    for _ in 0..4 {
        let c = Arc::clone(&coord);
        let b = Arc::clone(&barrier);
        let id = prof.id.clone();
        handles.push(thread::spawn(move || {
            let target = dummy_fixture_target();
            b.wait();
            c.launch_profile(&id, &target).unwrap()
        }));
    }

    let mut pids = HashSet::new();
    for h in handles {
        let rt = h.join().unwrap();
        assert!(rt.running);
        assert_eq!(rt.lifecycle_state(), ProfileLifecycleState::Running);
        pids.insert(rt.main_pid.unwrap());
    }

    assert_eq!(
        adapter.spawn_calls.load(Ordering::SeqCst),
        1,
        "4 concurrent launch_profile calls on the same profile must spawn exactly 1 session"
    );
    assert_eq!(pids.len(), 1);
    assert_eq!(coord.stop_profile(&prof.id).unwrap(), 1);
    assert_eq!(adapter.active_count(), 0);
}

#[test]
fn distinct_profiles_launch_in_parallel_without_global_serialization() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonProfileStore::new(tmp.path().join("data")).unwrap());
    // A 2-party barrier inside `Adapter::launch` would deadlock if `InstanceCoordinator`
    // held a global lock across launches; completing proves true per-profile parallelism!
    let launch_barrier = Arc::new(Barrier::new(2));
    let adapter = Arc::new(ConcurrencyTestAdapter::new(
        tmp.path().join("primary"),
        40,
        Some(launch_barrier),
    ));
    let coord = Arc::new(InstanceCoordinator::new(
        Arc::clone(&store),
        Arc::clone(&adapter),
    ));

    let p_a = store.create_profile("Parallel Profile A", "").unwrap();
    let p_b = store.create_profile("Parallel Profile B", "").unwrap();

    let start_barrier = Arc::new(Barrier::new(2));
    let c_a = Arc::clone(&coord);
    let b_a = Arc::clone(&start_barrier);
    let id_a = p_a.id.clone();
    let h_a = thread::spawn(move || {
        let target = dummy_fixture_target();
        b_a.wait();
        c_a.launch_profile(&id_a, &target).unwrap()
    });

    let c_b = Arc::clone(&coord);
    let b_b = Arc::clone(&start_barrier);
    let id_b = p_b.id.clone();
    let h_b = thread::spawn(move || {
        let target = dummy_fixture_target();
        b_b.wait();
        c_b.launch_profile(&id_b, &target).unwrap()
    });

    let rt_a = h_a.join().unwrap();
    let rt_b = h_b.join().unwrap();
    assert!(rt_a.running && rt_b.running);
    assert_ne!(rt_a.main_pid, rt_b.main_pid);
    assert_eq!(adapter.spawn_calls.load(Ordering::SeqCst), 2);

    assert_eq!(coord.stop_profile(&p_a.id).unwrap(), 1);
    assert_eq!(coord.stop_profile(&p_b.id).unwrap(), 1);
}

#[test]
fn concurrent_launch_and_stop_race_leaves_zero_orphans() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonProfileStore::new(tmp.path().join("data")).unwrap());
    let adapter = Arc::new(ConcurrencyTestAdapter::new(
        tmp.path().join("primary"),
        150,
        None,
    ));
    let coord = Arc::new(InstanceCoordinator::new(
        Arc::clone(&store),
        Arc::clone(&adapter),
    ));

    let prof = store.create_profile("Launch Stop Race", "").unwrap();

    for round in 0..8usize {
        let barrier = Arc::new(Barrier::new(2));
        let c_launch = Arc::clone(&coord);
        let b_launch = Arc::clone(&barrier);
        let id_launch = prof.id.clone();

        let h_launch = thread::spawn(move || {
            let target = dummy_fixture_target();
            b_launch.wait();
            c_launch.launch_profile(&id_launch, &target)
        });

        let c_stop = Arc::clone(&coord);
        let b_stop = Arc::clone(&barrier);
        let id_stop = prof.id.clone();
        let h_stop = thread::spawn(move || {
            b_stop.wait();
            if round % 2 == 0 {
                thread::sleep(Duration::from_millis(20));
            }
            c_stop.stop_profile(&id_stop).unwrap()
        });

        let launch_res = h_launch.join().unwrap();
        let _stop_cnt = h_stop.join().unwrap();

        // If `stop_profile` ran before `launch_profile` acquired `op_mutex`, clean up via final stop;
        // if `stop_profile` interrupted or followed `launch_profile`, active count must settle at 0.
        if launch_res.is_ok() {
            let _ = coord.stop_profile(&prof.id).unwrap();
        }
        let inspected = coord.inspect_profile(&prof.id).unwrap();
        assert!(!inspected.running, "round {round} left profile running");
        assert_eq!(
            adapter.active_count(),
            0,
            "round {round} left orphan process in adapter"
        );
    }
}

#[test]
fn concurrent_create_launch_and_delete_race_never_resurrects_deleted_profile() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonProfileStore::new(tmp.path().join("data")).unwrap());
    let adapter = Arc::new(ConcurrencyTestAdapter::new(
        tmp.path().join("primary"),
        60,
        None,
    ));
    let coord = Arc::new(InstanceCoordinator::new(
        Arc::clone(&store),
        Arc::clone(&adapter),
    ));

    for idx in 0..10usize {
        let prof = store
            .create_profile(&format!("Delete Launch Race {idx}"), "")
            .unwrap();
        let pdir = store.profile_dir_for(&prof).unwrap();
        let barrier = Arc::new(Barrier::new(2));

        let c_launch = Arc::clone(&coord);
        let b_launch = Arc::clone(&barrier);
        let id_launch = prof.id.clone();
        let h_launch = thread::spawn(move || {
            let target = dummy_fixture_target();
            b_launch.wait();
            c_launch.launch_profile(&id_launch, &target)
        });

        let c_del = Arc::clone(&coord);
        let b_del = Arc::clone(&barrier);
        let id_del = prof.id.clone();
        let h_del = thread::spawn(move || {
            b_del.wait();
            c_del.delete_profile(&id_del, true)
        });

        let launch_res = h_launch.join().unwrap();
        let del_res = h_del.join().unwrap();

        match del_res {
            Ok(()) => {
                // Profile was deleted: launch MUST have failed (or not spawned), metadata MUST be gone,
                // and `<data_dir>/Profiles/<dir>` MUST NOT have been resurrected on disk!
                assert!(
                    launch_res.is_err(),
                    "deleted profile must not succeed in concurrent launch"
                );
                assert_eq!(store.get_profile(&prof.id).unwrap_err().code(), "NOT_FOUND");
                assert!(
                    !pdir.exists(),
                    "deleted profile directory was resurrected on disk: {}",
                    pdir.display()
                );
            }
            Err(err) => {
                // Launch won the race first, so `delete_profile` cleanly refused with `PROFILE_RUNNING`!
                assert_eq!(err.code(), "PROFILE_RUNNING");
                assert!(launch_res.is_ok());
                assert_eq!(coord.stop_profile(&prof.id).unwrap(), 1);
                coord.delete_profile(&prof.id, true).unwrap();
                assert!(!pdir.exists());
            }
        }
    }

    assert_eq!(adapter.active_count(), 0);
    assert!(store.list_profiles().unwrap().is_empty());
}

#[test]
fn multi_process_coordinator_cross_process_lock_and_lease_crash_recovery() {
    if env::var("DIM_COORD_SUBPROC_MODE").as_deref() == Ok("hold_profile_lock") {
        let lock_file = PathBuf::from(env::var("DIM_COORD_LOCK_PATH").unwrap());
        let _guard =
            AdvisoryFileLock::acquire_exclusive(&lock_file, Duration::from_secs(2)).unwrap();
        println!("LOCKED");
        let mut buf = [0u8; 1];
        let _ = std::io::stdin().read(&mut buf);
        return;
    }

    if env::var("DIM_COORD_SUBPROC_MODE").as_deref() == Ok("live_lease_target") {
        let mut buf = [0u8; 1];
        let _ = std::io::stdin().read(&mut buf);
        return;
    }

    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonProfileStore::new(tmp.path().join("data")).unwrap());
    let adapter = Arc::new(ConcurrencyTestAdapter::new(
        tmp.path().join("primary"),
        10,
        None,
    ));
    let coord = InstanceCoordinator::new(Arc::clone(&store), Arc::clone(&adapter));
    let recovery = StaleLockRecoveryManager;

    let prof = store.create_profile("Crash Recovery & Lock", "").unwrap();
    let pdir = store.profile_dir_for(&prof).unwrap();
    adapter.ensure_profile_layout(&prof, &pdir).unwrap();

    // 1. Cross-process `<data_dir>/locks/<id>.lock` AND legacy `.dim-profile.lock` held by a
    // separate OS subprocess both block `delete_profile`!
    let current_exe = env::current_exe().unwrap();
    for lock_path in [
        store.profile_lock_path(&prof.id),
        pdir.join(PROFILE_OP_LOCK_FILE),
    ] {
        let mut lock_holder = Command::new(&current_exe)
            .args([
                "--exact",
                "multi_process_coordinator_cross_process_lock_and_lease_crash_recovery",
                "--nocapture",
            ])
            .env("DIM_COORD_SUBPROC_MODE", "hold_profile_lock")
            .env("DIM_COORD_LOCK_PATH", &lock_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        let stdout = lock_holder.stdout.take().unwrap();
        let mut reader = BufReader::new(stdout);
        let mut saw_locked = false;
        let mut line = String::new();
        while reader.read_line(&mut line).unwrap() > 0 {
            if line.contains("LOCKED") {
                saw_locked = true;
                break;
            }
            line.clear();
        }
        assert!(saw_locked, "subprocess failed to report LOCKED");

        let blocked_del = coord.delete_profile(&prof.id, true).unwrap_err();
        assert_eq!(blocked_del.code(), "PROFILE_RUNNING");

        drop(lock_holder.stdin.take());
        let _ = lock_holder.wait();
    }
    let _ = fs::remove_file(pdir.join(PROFILE_OP_LOCK_FILE));

    // 2. Live `.runtime-lease.json` for a real running OS subprocess:
    // `verify_runtime_lease` reports `VerifiedLive`, and `StaleLockRecoveryManager` refuses cleanup!
    let mut live_child = Command::new(&current_exe)
        .args([
            "--exact",
            "multi_process_coordinator_cross_process_lock_and_lease_crash_recovery",
            "--nocapture",
        ])
        .env("DIM_COORD_SUBPROC_MODE", "live_lease_target")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let live_lease = build_runtime_lease(live_child.id(), &current_exe, &prof.id, None);
    write_runtime_lease(&pdir, &live_lease).unwrap();
    assert!(matches!(
        verify_runtime_lease(&pdir, &prof.id, &[]),
        LeaseVerification::VerifiedLive(ref l) if l.pid == live_child.id()
    ));

    let forged_stopped_state = adapter.inspect_profile(&prof, &pdir).unwrap();
    let live_rec_err = recovery
        .recover_stale_state(&prof, &pdir, &forged_stopped_state)
        .unwrap_err();
    assert_eq!(live_rec_err.code(), "PROFILE_RUNNING");
    assert!(pdir.join(RUNTIME_LEASE_FILE).exists());

    // 3. Kill child process -> lease becomes `StaleDead` -> crash recovery cleans stale lease + `.runtime-pid`
    // and `launch_profile` recovers cleanly!
    drop(live_child.stdin.take());
    let _ = live_child.wait();
    assert!(matches!(
        verify_runtime_lease(&pdir, &prof.id, &[]),
        LeaseVerification::StaleDead(_)
    ));

    let stale_state = adapter.inspect_profile(&prof, &pdir).unwrap();
    assert!(stale_state.stale_lock_detected);

    let launched = coord
        .launch_profile(&prof.id, &dummy_fixture_target())
        .unwrap();
    assert!(launched.running);
    assert!(!pdir.join(RUNTIME_LEASE_FILE).exists());
    assert_eq!(coord.stop_profile(&prof.id).unwrap(), 1);
    let _ = remove_runtime_lease(&pdir);
}

#[test]
fn multi_process_subprocesses_launch_vs_delete_race_never_resurrects_or_corrupts() {
    if let Ok(role) = env::var("DIM_SUBPROC_RACE_ROLE") {
        let data_dir = PathBuf::from(env::var("DIM_SUBPROC_RACE_DATA_DIR").unwrap());
        let primary_dir = PathBuf::from(env::var("DIM_SUBPROC_RACE_PRIMARY_DIR").unwrap());
        let profile_id = env::var("DIM_SUBPROC_RACE_PROFILE_ID").unwrap();
        let store = Arc::new(JsonProfileStore::new(data_dir).unwrap());
        let adapter = Arc::new(ConcurrencyTestAdapter::new(primary_dir, 25, None));
        let coord = InstanceCoordinator::new(Arc::clone(&store), Arc::clone(&adapter));

        match role.as_str() {
            "launcher" => match coord.launch_profile(&profile_id, &dummy_fixture_target()) {
                Ok(_) => println!("RESULT:OK"),
                Err(err) => println!("RESULT:ERR:{}", err.code()),
            },
            "deleter" => match coord.delete_profile(&profile_id, true) {
                Ok(()) => println!("RESULT:OK"),
                Err(err) => println!("RESULT:ERR:{}", err.code()),
            },
            _ => panic!("unknown role {role}"),
        }
        return;
    }

    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("subproc-launch-delete-race");
    let primary_dir = tmp.path().join("primary");
    let store = Arc::new(JsonProfileStore::new(data_dir.clone()).unwrap());
    store.ensure_initialized().unwrap();

    let current_exe = env::current_exe().unwrap();

    for round in 0..12usize {
        let prof = store
            .create_profile(&format!("Subproc Delete Race {round}"), "")
            .unwrap();
        let pdir = store.profile_dir_for(&prof).unwrap();
        // Seed files inside `profile_dir` so mid-delete `ERROR_DIR_NOT_EMPTY` would be observable!
        let zalo_data = pdir.join("AppData").join("Roaming").join("ZaloData");
        fs::create_dir_all(&zalo_data).unwrap();
        write_profile_id_marker(&zalo_data, &prof.id).unwrap();
        fs::write(zalo_data.join("payload.bin"), b"USER_PROFILE_STATE").unwrap();

        let launcher_child = Command::new(&current_exe)
            .args([
                "--exact",
                "multi_process_subprocesses_launch_vs_delete_race_never_resurrects_or_corrupts",
                "--nocapture",
            ])
            .env("DIM_SUBPROC_RACE_ROLE", "launcher")
            .env("DIM_SUBPROC_RACE_DATA_DIR", &data_dir)
            .env("DIM_SUBPROC_RACE_PRIMARY_DIR", &primary_dir)
            .env("DIM_SUBPROC_RACE_PROFILE_ID", &prof.id)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();

        let deleter_child = Command::new(&current_exe)
            .args([
                "--exact",
                "multi_process_subprocesses_launch_vs_delete_race_never_resurrects_or_corrupts",
                "--nocapture",
            ])
            .env("DIM_SUBPROC_RACE_ROLE", "deleter")
            .env("DIM_SUBPROC_RACE_DATA_DIR", &data_dir)
            .env("DIM_SUBPROC_RACE_PRIMARY_DIR", &primary_dir)
            .env("DIM_SUBPROC_RACE_PROFILE_ID", &prof.id)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();

        let l_out = launcher_child.wait_with_output().unwrap();
        let d_out = deleter_child.wait_with_output().unwrap();
        assert!(l_out.status.success());
        assert!(d_out.status.success());

        let l_stdout = String::from_utf8_lossy(&l_out.stdout);
        let d_stdout = String::from_utf8_lossy(&d_out.stdout);

        let d_result = d_stdout
            .lines()
            .find_map(|line| line.strip_prefix("RESULT:"))
            .expect("missing deleter RESULT line");
        let l_result = l_stdout
            .lines()
            .find_map(|line| line.strip_prefix("RESULT:"))
            .expect("missing launcher RESULT line");

        // Deleter MUST NEVER fail with `IO_ERROR` (`ERROR_DIR_NOT_EMPTY` / os error 145)!
        assert!(
            d_result == "OK" || d_result == "ERR:PROFILE_RUNNING",
            "round {round}: unexpected deleter outcome '{d_result}' (launcher='{l_result}')"
        );

        if d_result == "OK" {
            // Deleted profile MUST NOT exist in `profiles.json` AND `profile_dir` MUST NOT be resurrected!
            assert_eq!(store.get_profile(&prof.id).unwrap_err().code(), "NOT_FOUND");
            assert!(
                !pdir.exists(),
                "round {round}: cross-process launcher resurrected deleted profile_dir '{}' (launcher='{l_result}')",
                pdir.display()
            );
        } else {
            // Launcher held `locks/<id>.lock` first, so deleter cleanly refused with `PROFILE_RUNNING`
            // and all profile files remained intact!
            assert_eq!(l_result, "OK");
            assert!(pdir.exists());
            store.delete_profile(&prof.id, true).unwrap();
            assert!(!pdir.exists());
        }
    }

    assert!(store.list_profiles().unwrap().is_empty());
}

#[test]
fn simultaneous_distinct_profile_launches_block_duplicate_zalo_user_id_toctou() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonProfileStore::new(tmp.path().join("data")).unwrap());
    // 150ms launch delay ensures both threads would overlap pre-lock if not guarded by `in_flight_claims`!
    let adapter = Arc::new(ConcurrencyTestAdapter::new(
        tmp.path().join("primary"),
        150,
        None,
    ));
    let coord = Arc::new(InstanceCoordinator::new(
        Arc::clone(&store),
        Arc::clone(&adapter),
    ));

    let p_a = store.create_profile("Dup UID A", "").unwrap();
    let p_b = store.create_profile("Dup UID B", "").unwrap();
    for p in [&p_a, &p_b] {
        let zd = store
            .profile_dir_for(p)
            .unwrap()
            .join("AppData")
            .join("Roaming")
            .join("ZaloData");
        fs::create_dir_all(zd.join("Database").join("_production").join("849777001")).unwrap();
        write_profile_id_marker(&zd, &p.id).unwrap();
    }

    let barrier = Arc::new(Barrier::new(2));
    let c_a = Arc::clone(&coord);
    let b_a = Arc::clone(&barrier);
    let id_a = p_a.id.clone();
    let h_a = thread::spawn(move || {
        b_a.wait();
        c_a.launch_profile(&id_a, &dummy_fixture_target())
    });

    let c_b = Arc::clone(&coord);
    let b_b = Arc::clone(&barrier);
    let id_b = p_b.id.clone();
    let h_b = thread::spawn(move || {
        b_b.wait();
        c_b.launch_profile(&id_b, &dummy_fixture_target())
    });

    let res_a = h_a.join().unwrap();
    let res_b = h_b.join().unwrap();

    let (ok_count, dup_err_count) = match (&res_a, &res_b) {
        (Ok(_), Err(e)) => (1, usize::from(e.code() == "DUPLICATE_ZALO_USER_ID")),
        (Err(e), Ok(_)) => (1, usize::from(e.code() == "DUPLICATE_ZALO_USER_ID")),
        _ => (0, 0),
    };
    assert_eq!(
        (ok_count, dup_err_count),
        (1, 1),
        "expected exactly 1 launch success and 1 DUPLICATE_ZALO_USER_ID rejection, got A={res_a:?}, B={res_b:?}"
    );
    assert_eq!(adapter.spawn_calls.load(Ordering::SeqCst), 1);

    let _ = coord.stop_all_secondary_profiles().unwrap();
    assert_eq!(adapter.active_count(), 0);
}

//! KHOANG Stage 2A `AccountActivationService` & `InstanceCoordinator` regression suite:
//! - Strictly read-only & non-spawning `activate_desktop_account("primary", ...)`
//!   (`PRIMARY_NOT_RUNNING` / `PRIMARY_HIDDEN_IN_TRAY`, companion snap hard-rejected).
//! - 3 secondary fixture profiles across 20 sequential activations: 100% exact target accuracy,
//!   0 wrong-window hits.
//! - Safe handling of minimized, hidden-in-tray, stopped, and `RecoveryRequired` states.
//! - `inspect_all_profiles_fast()` skips disk walks while `inspect_all_profiles()` computes sizes.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use dim_core::{
    AccountActivationResult, AccountKind, ActionableRemediation, ActivationOutcome, DimError,
    InstanceLauncher, InstanceMonitor, LaunchTarget, PlatformCapabilities,
    PlatformCapabilityReport, PlatformKind, PrimaryInstanceStatus, PrimaryWindowState,
    ProfileLifecycleState, ProfileRecord, ProfileRuntimeState, ProfileStore, SupportStatus,
    WindowActivationState, WindowController, PROFILES_SCHEMA_VERSION,
};
use dim_process_manager::InstanceCoordinator;
use dim_profile_store::JsonProfileStore;

#[derive(Debug, Clone)]
struct SimWindow {
    hwnd: u64,
    pid: u32,
    minimized: bool,
    hidden_in_tray: bool,
    title: String,
}

struct ActivationTestAdapter {
    primary: Mutex<PrimaryInstanceStatus>,
    primary_focus_calls: AtomicUsize,
    spawn_calls: AtomicUsize,
    launch_delay_ms: AtomicUsize,
    force_unconfirmed_fg: Mutex<Option<WindowActivationState>>,
    windows: Mutex<HashMap<String, SimWindow>>,
    recovery_required: Mutex<HashMap<String, bool>>,
    next_id: Mutex<(u32, u64)>,
}

impl ActivationTestAdapter {
    fn new(primary_dir: PathBuf) -> Self {
        Self {
            primary: Mutex::new(PrimaryInstanceStatus {
                zalo_data_dir: primary_dir.clone(),
                lockfile_path: primary_dir.join("lockfile"),
                running: false,
                protected_pids: Vec::new(),
                process_count: 0,
                working_set_bytes: 0,
                detected_zalo_user_id: None,
                window_handle: None,
                window_title: None,
                window_state: PrimaryWindowState::None,
                notes: Vec::new(),
            }),
            primary_focus_calls: AtomicUsize::new(0),
            spawn_calls: AtomicUsize::new(0),
            launch_delay_ms: AtomicUsize::new(0),
            force_unconfirmed_fg: Mutex::new(None),
            windows: Mutex::new(HashMap::new()),
            recovery_required: Mutex::new(HashMap::new()),
            next_id: Mutex::new((6000, 0x10_0000)),
        }
    }

    fn set_primary_hidden_in_tray(&self, pid: u32) {
        let mut p = self.primary.lock().unwrap();
        p.running = true;
        p.protected_pids = vec![pid];
        p.process_count = 1;
        p.window_handle = None;
        p.window_title = None;
        p.window_state = PrimaryWindowState::HiddenInTray;
    }

    fn set_profile_minimized(&self, profile_id: &str, minimized: bool) {
        if let Some(w) = self.windows.lock().unwrap().get_mut(profile_id) {
            w.minimized = minimized;
        }
    }

    fn set_profile_hidden_in_tray(&self, profile_id: &str, hidden: bool) {
        if let Some(w) = self.windows.lock().unwrap().get_mut(profile_id) {
            w.hidden_in_tray = hidden;
        }
    }

    fn set_profile_recovery_required(&self, profile_id: &str, required: bool) {
        self.recovery_required
            .lock()
            .unwrap()
            .insert(profile_id.to_owned(), required);
    }
}

impl PlatformCapabilities for ActivationTestAdapter {
    fn platform(&self) -> PlatformKind {
        PlatformKind::Windows
    }

    fn capabilities(&self) -> PlatformCapabilityReport {
        PlatformCapabilityReport {
            platform: PlatformKind::Windows,
            overall_status: SupportStatus::SupportedAndTested,
            official_zalo_status: SupportStatus::SupportedAndTested,
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
        Ok(100 * 1024 * 1024 * 1024)
    }

    fn unlink_profile_junctions(&self, _profile_dir: &Path) -> Result<usize, DimError> {
        Ok(0)
    }
}

impl InstanceMonitor for ActivationTestAdapter {
    fn inspect_primary(&self) -> Result<PrimaryInstanceStatus, DimError> {
        Ok(self.primary.lock().unwrap().clone())
    }

    fn inspect_profile(
        &self,
        profile: &ProfileRecord,
        profile_dir: &Path,
    ) -> Result<ProfileRuntimeState, DimError> {
        self.inspect_profile_with_options(profile, profile_dir, true)
    }

    fn inspect_profile_with_options(
        &self,
        profile: &ProfileRecord,
        profile_dir: &Path,
        include_size: bool,
    ) -> Result<ProfileRuntimeState, DimError> {
        let zalo_data = profile_dir.join("AppData").join("Roaming").join("ZaloData");
        let lockfile = zalo_data.join("lockfile");
        let is_recovery = self
            .recovery_required
            .lock()
            .unwrap()
            .get(&profile.id)
            .copied()
            .unwrap_or(false);
        let sim = self.windows.lock().unwrap().get(&profile.id).cloned();

        let disk_usage_bytes = if include_size {
            fs::read_dir(&zalo_data)
                .map(|rd| {
                    rd.flatten()
                        .filter_map(|e| e.metadata().ok())
                        .filter(|m| m.is_file())
                        .map(|m| m.len())
                        .sum()
                })
                .unwrap_or(0)
        } else {
            0
        };

        let mut rt = ProfileRuntimeState {
            profile_id: profile.id.clone(),
            profile_name: profile.name.clone(),
            profile_dir: profile_dir.to_path_buf(),
            zalo_data_dir: zalo_data,
            lockfile_path: lockfile,
            running: false,
            stale_lock_detected: false,
            main_pid: sim.as_ref().map(|w| w.pid),
            process_ids: sim.as_ref().map(|w| vec![w.pid]).unwrap_or_default(),
            process_count: usize::from(sim.is_some()),
            working_set_bytes: if sim.is_some() { 1024 * 1024 } else { 0 },
            window_handle: sim.as_ref().and_then(|w| {
                if w.hidden_in_tray {
                    None
                } else {
                    Some(w.hwnd)
                }
            }),
            window_title: sim.as_ref().and_then(|w| {
                if w.hidden_in_tray {
                    None
                } else {
                    Some(w.title.clone())
                }
            }),
            detected_zalo_user_id: None,
            disk_usage_bytes,
            status_label: "Stopped".to_owned(),
        };

        if is_recovery {
            rt.set_lifecycle_state(ProfileLifecycleState::RecoveryRequired);
        } else if sim.is_some() {
            rt.set_lifecycle_state(ProfileLifecycleState::Running);
        } else {
            rt.set_lifecycle_state(ProfileLifecycleState::Stopped);
        }

        Ok(rt)
    }
}

impl InstanceLauncher for ActivationTestAdapter {
    fn ensure_profile_layout(
        &self,
        _profile: &ProfileRecord,
        profile_dir: &Path,
    ) -> Result<(), DimError> {
        fs::create_dir_all(profile_dir.join("AppData").join("Roaming").join("ZaloData"))?;
        Ok(())
    }

    fn launch(
        &self,
        _target: &LaunchTarget,
        profile: &ProfileRecord,
        profile_dir: &Path,
    ) -> Result<ProfileRuntimeState, DimError> {
        let delay = self.launch_delay_ms.load(Ordering::SeqCst);
        if delay > 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay as u64));
        }
        self.spawn_calls.fetch_add(1, Ordering::SeqCst);
        let (pid, hwnd) = {
            let mut ids = self.next_id.lock().unwrap();
            ids.0 += 1;
            ids.1 += 0x100;
            (ids.0, ids.1)
        };
        let sim = SimWindow {
            hwnd,
            pid,
            minimized: false,
            hidden_in_tray: false,
            title: format!("Zalo - {}", profile.name),
        };
        self.windows.lock().unwrap().insert(profile.id.clone(), sim);
        self.inspect_profile_with_options(profile, profile_dir, false)
    }

    fn focus_or_restore(
        &self,
        _target: &LaunchTarget,
        profile: &ProfileRecord,
        _profile_dir: &Path,
        _state: &ProfileRuntimeState,
    ) -> Result<(), DimError> {
        if let Some(w) = self.windows.lock().unwrap().get_mut(&profile.id) {
            w.hidden_in_tray = false;
            w.minimized = false;
        }
        Ok(())
    }

    fn stop(
        &self,
        state: &ProfileRuntimeState,
        _protected_primary_pids: &[u32],
        _timeout_ms: u64,
    ) -> Result<u32, DimError> {
        self.windows.lock().unwrap().remove(&state.profile_id);
        Ok(1)
    }
}

impl WindowController for ActivationTestAdapter {
    fn focus_primary_window(
        &self,
        primary: &PrimaryInstanceStatus,
    ) -> Result<AccountActivationResult, DimError> {
        self.primary_focus_calls.fetch_add(1, Ordering::SeqCst);
        let code = if primary.running {
            "PRIMARY_HIDDEN_IN_TRAY"
        } else {
            "PRIMARY_NOT_RUNNING"
        };
        Ok(AccountActivationResult {
            account_id: "primary".to_owned(),
            account_kind: AccountKind::DesktopPrimary,
            outcome: ActivationOutcome::ManualActionRequired,
            method: Some("read_only_inspect".to_owned()),
            os_foreground_confirmed: Some(false),
            window_handle: primary.window_handle,
            window_title: primary.window_title.clone(),
            message: "Primary is strictly read-only.".to_owned(),
            remediation: Some(ActionableRemediation {
                code: code.to_owned(),
                message: "Manual action required.".to_owned(),
                suggested_action: None,
            }),
        })
    }

    fn is_window_minimized(&self, hwnd: u64) -> bool {
        self.windows
            .lock()
            .unwrap()
            .values()
            .any(|w| w.hwnd == hwnd && w.minimized)
    }

    fn try_focus_verified_window(
        &self,
        hwnd: u64,
        allowed_pids: &[u32],
    ) -> Result<Option<(bool, bool)>, DimError> {
        let mut guard = self.windows.lock().unwrap();
        let Some(w) = guard.values_mut().find(|w| w.hwnd == hwnd) else {
            return Err(DimError::ProcessError("WINDOW_UNVERIFIED".to_owned()));
        };
        if !allowed_pids.contains(&w.pid) {
            return Err(DimError::ProcessError("WINDOW_UNVERIFIED".to_owned()));
        }
        let was_minimized = w.minimized;
        w.minimized = false;
        let fg_confirmed = self
            .force_unconfirmed_fg
            .lock()
            .unwrap()
            .map(|p| p.is_foreground)
            .unwrap_or(true);
        Ok(Some((was_minimized, fg_confirmed)))
    }

    fn probe_window_activation_state(&self, hwnd: u64) -> Option<WindowActivationState> {
        if let Some(forced) = *self.force_unconfirmed_fg.lock().unwrap() {
            return Some(forced);
        }
        let guard = self.windows.lock().unwrap();
        let w = guard.values().find(|w| w.hwnd == hwnd)?;
        Some(WindowActivationState {
            is_foreground: !w.hidden_in_tray && !w.minimized,
            foreign_foreground_present: false,
            is_visible: !w.hidden_in_tray,
            is_minimized: w.minimized,
        })
    }
}

fn dummy_target() -> LaunchTarget {
    LaunchTarget::CustomExecutable {
        path: PathBuf::from("C:\\nonexistent\\fixture.exe"),
        args: Vec::new(),
    }
}

#[test]
fn primary_activation_is_strictly_read_only_non_spawning_and_snap_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("dim-data");
    let primary_dir = tmp.path().join("primary-zalo");
    fs::create_dir_all(&primary_dir).unwrap();

    let store = Arc::new(JsonProfileStore::new(data_dir).unwrap());
    let adapter = Arc::new(ActivationTestAdapter::new(primary_dir));
    let coord = InstanceCoordinator::new(store, Arc::clone(&adapter));
    let target = dummy_target();

    // 1. Not running -> PRIMARY_NOT_RUNNING, zero spawns.
    let res_stopped = coord
        .activate_desktop_account("primary", Some(&target))
        .unwrap();
    assert_eq!(res_stopped.account_kind, AccountKind::DesktopPrimary);
    assert_eq!(res_stopped.outcome, ActivationOutcome::ManualActionRequired);
    assert_eq!(
        res_stopped.remediation.as_ref().map(|r| r.code.as_str()),
        Some("PRIMARY_NOT_RUNNING")
    );
    assert_eq!(adapter.spawn_calls.load(Ordering::SeqCst), 0);

    // 2. Hidden in tray -> PRIMARY_HIDDEN_IN_TRAY, zero spawns.
    adapter.set_primary_hidden_in_tray(4242);
    let res_tray = coord
        .activate_desktop_account("primary", Some(&target))
        .unwrap();
    assert_eq!(res_tray.outcome, ActivationOutcome::ManualActionRequired);
    assert_eq!(
        res_tray.remediation.as_ref().map(|r| r.code.as_str()),
        Some("PRIMARY_HIDDEN_IN_TRAY")
    );
    assert_eq!(adapter.spawn_calls.load(Ordering::SeqCst), 0);
    assert_eq!(adapter.primary_focus_calls.load(Ordering::SeqCst), 2);

    // 3. Companion snap hard-rejected on primary.
    let snap_err = coord.companion_snap_profile("primary", 1, 64).unwrap_err();
    assert_eq!(snap_err.code(), "PROTECTED_PRIMARY_PROCESS");
}

#[test]
fn secondary_three_profiles_twenty_sequential_activations_and_edge_states() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("dim-data");
    let primary_dir = tmp.path().join("primary-zalo");
    fs::create_dir_all(&primary_dir).unwrap();

    let store = Arc::new(JsonProfileStore::new(data_dir).unwrap());
    assert_eq!(
        store.load().unwrap().schema_version,
        PROFILES_SCHEMA_VERSION
    );

    let p1 = store.create_profile("Alpha", "").unwrap();
    let p2 = store.create_profile("Beta", "").unwrap();
    let p3 = store.create_profile("Gamma", "").unwrap();

    // Write payload files to verify inspect_all_profiles_fast skips disk walks.
    for p in [&p1, &p2, &p3] {
        let zd = store
            .profile_dir_for(p)
            .unwrap()
            .join("AppData")
            .join("Roaming")
            .join("ZaloData");
        fs::create_dir_all(&zd).unwrap();
        fs::write(zd.join("data.bin"), vec![1u8; 4096]).unwrap();
    }

    let adapter = Arc::new(ActivationTestAdapter::new(primary_dir));
    let coord = InstanceCoordinator::new(Arc::clone(&store), Arc::clone(&adapter));
    let target = dummy_target();

    // Fast vs full inspection
    let fast_all = coord.inspect_all_profiles_fast().unwrap();
    assert_eq!(fast_all.len(), 3);
    assert!(fast_all.iter().all(|(_, rt)| rt.disk_usage_bytes == 0));

    let full_all = coord.inspect_all_profiles().unwrap();
    assert!(full_all.iter().all(|(_, rt)| rt.disk_usage_bytes >= 4096));

    // Cold launch all 3 profiles via activate_desktop_account
    for p in [&p1, &p2, &p3] {
        let r = coord
            .activate_desktop_account(&p.id, Some(&target))
            .unwrap();
        assert_eq!(r.account_id, p.id);
        assert_eq!(r.outcome, ActivationOutcome::Launched);
    }
    assert_eq!(adapter.spawn_calls.load(Ordering::SeqCst), 3);

    // 20 sequential activations across the 3 profiles (including minimized & tray-hidden transitions)
    let expected_hwnds: HashMap<String, (u64, String)> = adapter
        .windows
        .lock()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), (v.hwnd, v.title.clone())))
        .collect();

    let order = [&p1.id, &p2.id, &p3.id];
    for i in 0..20usize {
        let id = order[i % 3];
        if i == 4 {
            adapter.set_profile_minimized(id, true);
        } else if i == 9 {
            adapter.set_profile_hidden_in_tray(id, true);
        }
        let res = coord.activate_desktop_account(id, Some(&target)).unwrap();
        assert_eq!(res.account_id, *id);
        let (exp_hwnd, exp_title) = &expected_hwnds[id];
        assert_eq!(
            res.window_handle,
            Some(*exp_hwnd),
            "iteration {i}: wrong HWND for {id}"
        );
        assert_eq!(
            res.window_title.as_deref(),
            Some(exp_title.as_str()),
            "iteration {i}: wrong title for {id}"
        );
        if i == 4 {
            assert_eq!(res.outcome, ActivationOutcome::Restored);
            assert_eq!(res.method.as_deref(), Some("direct_restore"));
        } else if i == 9 {
            assert_eq!(res.outcome, ActivationOutcome::Restored);
            assert_eq!(res.method.as_deref(), Some("tray_handoff"));
        } else {
            assert_eq!(res.outcome, ActivationOutcome::Focused);
            assert_eq!(res.method.as_deref(), Some("direct_focus"));
        }
        if i == 9 {
            // Wait past 250ms tray handoff debounce before next cycle.
            std::thread::sleep(std::time::Duration::from_millis(260));
        }
    }

    // RecoveryRequired fails closed and never deletes lockfile or spawns.
    let lock_path = store
        .profile_dir_for(&p2)
        .unwrap()
        .join("AppData")
        .join("Roaming")
        .join("ZaloData")
        .join("lockfile");
    fs::write(&lock_path, b"corrupt-lock").unwrap();
    adapter.set_profile_recovery_required(&p2.id, true);

    let blocked = coord
        .activate_desktop_account(&p2.id, Some(&target))
        .unwrap();
    assert_eq!(blocked.outcome, ActivationOutcome::Blocked);
    assert_eq!(
        blocked.remediation.as_ref().map(|r| r.code.as_str()),
        Some("RECOVERY_REQUIRED")
    );
    assert!(
        lock_path.exists(),
        "RecoveryRequired must NEVER delete lockfile"
    );
    assert_eq!(
        adapter.spawn_calls.load(Ordering::SeqCst),
        3,
        "must never spawn extra processes during 20 activations or RecoveryRequired"
    );
}

#[test]
fn concurrent_activate_desktop_account_calls_coalesce_to_single_launch_without_pending() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("dim-data");
    let primary_dir = tmp.path().join("primary-zalo");
    fs::create_dir_all(&primary_dir).unwrap();

    let store = Arc::new(JsonProfileStore::new(data_dir).unwrap());
    let profile = store.create_profile("Concurrent Secondary", "").unwrap();
    let adapter = Arc::new(ActivationTestAdapter::new(primary_dir));
    adapter.launch_delay_ms.store(120, Ordering::SeqCst);
    let coord = Arc::new(InstanceCoordinator::new(store, Arc::clone(&adapter)));

    let barrier = Arc::new(std::sync::Barrier::new(4));
    let mut handles = Vec::with_capacity(4);
    for _ in 0..4 {
        let c = Arc::clone(&coord);
        let b = Arc::clone(&barrier);
        let pid = profile.id.clone();
        handles.push(std::thread::spawn(move || {
            let target = dummy_target();
            b.wait();
            c.activate_desktop_account(&pid, Some(&target)).unwrap()
        }));
    }

    let mut hwnds = std::collections::HashSet::new();
    for h in handles {
        let res = h.join().unwrap();
        assert_eq!(res.account_id, profile.id);
        assert!(
            matches!(
                res.outcome,
                ActivationOutcome::Launched | ActivationOutcome::Focused
            ),
            "concurrent activation must coalesce and return Launched/Focused, never premature Pending; got {:?}",
            res.outcome
        );
        hwnds.insert(res.window_handle.expect("window_handle must be populated"));
    }

    assert_eq!(
        adapter.spawn_calls.load(Ordering::SeqCst),
        1,
        "4 concurrent activate_desktop_account calls must spawn exactly 1 process"
    );
    assert_eq!(
        hwnds.len(),
        1,
        "all 4 concurrent callers must resolve to the same window handle"
    );
}

#[test]
fn foreground_locked_by_foreign_window_returns_manual_action_required_not_false_focused() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("dim-data");
    let primary_dir = tmp.path().join("primary-zalo");
    fs::create_dir_all(&primary_dir).unwrap();

    let store = Arc::new(JsonProfileStore::new(data_dir).unwrap());
    let profile = store.create_profile("FG Lock Secondary", "").unwrap();
    let adapter = Arc::new(ActivationTestAdapter::new(primary_dir));
    let coord = InstanceCoordinator::new(store, Arc::clone(&adapter));
    let target = dummy_target();

    let launched = coord
        .activate_desktop_account(&profile.id, Some(&target))
        .unwrap();
    assert_eq!(launched.outcome, ActivationOutcome::Launched);

    // 1. OS Foreground Lock blocks activation while another foreign window holds foreground.
    *adapter.force_unconfirmed_fg.lock().unwrap() = Some(WindowActivationState {
        is_foreground: false,
        foreign_foreground_present: true,
        is_visible: true,
        is_minimized: false,
    });
    let locked_res = coord
        .activate_desktop_account(&profile.id, Some(&target))
        .unwrap();
    assert_eq!(locked_res.outcome, ActivationOutcome::ManualActionRequired);
    assert_eq!(locked_res.os_foreground_confirmed, Some(false));
    assert_eq!(
        locked_res.remediation.as_ref().map(|r| r.code.as_str()),
        Some("FOREGROUND_LOCKED")
    );

    // 2. Headless desktop (no foreign foreground window, target window visible & unminimized)
    // remains Focused with os_foreground_confirmed: Some(false).
    std::thread::sleep(std::time::Duration::from_millis(260));
    *adapter.force_unconfirmed_fg.lock().unwrap() = Some(WindowActivationState {
        is_foreground: false,
        foreign_foreground_present: false,
        is_visible: true,
        is_minimized: false,
    });
    let headless_res = coord
        .activate_desktop_account(&profile.id, Some(&target))
        .unwrap();
    assert_eq!(headless_res.outcome, ActivationOutcome::Focused);
    assert_eq!(headless_res.os_foreground_confirmed, Some(false));
}

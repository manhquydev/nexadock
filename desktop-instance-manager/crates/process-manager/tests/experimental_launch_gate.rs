//! Coordinator launch-gate tests for the Experimental community `zalo-linux` repack route.
//!
//! Verifies the `finish_leader_launch` gate semantics required by the route design:
//! - `official_zalo_status == Unsupported` keeps blocking `LaunchTarget::OfficialZalo`.
//! - `official_zalo_status == Experimental` (community repack detected) lets the launch proceed.
//! - The blocked path never reaches the adapter launcher.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use dim_core::{
    DimError, InstanceLauncher, InstanceMonitor, LaunchTarget, PlatformCapabilities,
    PlatformCapabilityReport, PlatformKind, PrimaryInstanceStatus, ProfileRecord,
    ProfileRuntimeState, ProfileStore, SupportStatus, MIN_FREE_DISK_BYTES,
};
use dim_process_manager::InstanceCoordinator;
use dim_profile_store::{unlink_reparse_points_safe, write_profile_id_marker, JsonProfileStore};

/// Minimal adapter whose only configurable behavior is the reported official-launch status.
struct GateTestAdapter {
    official_zalo_status: SupportStatus,
    launch_attempted: AtomicBool,
    primary_dir: PathBuf,
}

impl GateTestAdapter {
    fn new(official_zalo_status: SupportStatus, primary_dir: PathBuf) -> Self {
        Self {
            official_zalo_status,
            launch_attempted: AtomicBool::new(false),
            primary_dir,
        }
    }
}

impl PlatformCapabilities for GateTestAdapter {
    fn platform(&self) -> PlatformKind {
        PlatformKind::Linux
    }

    fn capabilities(&self) -> PlatformCapabilityReport {
        PlatformCapabilityReport {
            platform: PlatformKind::Linux,
            overall_status: SupportStatus::Unsupported,
            official_zalo_status: self.official_zalo_status,
            fixture_status: SupportStatus::Experimental,
            appimage_status: None,
            concurrent_calls_status: SupportStatus::Blocked,
            folder_redirection_detected: false,
            folder_redirection_path: None,
            limitations: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    fn free_disk_bytes(&self, _path: &Path) -> Result<u64, DimError> {
        Ok(MIN_FREE_DISK_BYTES * 4)
    }

    fn unlink_profile_junctions(&self, profile_dir: &Path) -> Result<usize, DimError> {
        unlink_reparse_points_safe(profile_dir)
    }
}

impl InstanceMonitor for GateTestAdapter {
    fn inspect_primary(&self) -> Result<PrimaryInstanceStatus, DimError> {
        Ok(PrimaryInstanceStatus {
            zalo_data_dir: self.primary_dir.clone(),
            lockfile_path: self.primary_dir.join("lockfile"),
            running: false,
            protected_pids: Vec::new(),
            process_count: 0,
            working_set_bytes: 0,
            detected_zalo_user_id: None,
            window_handle: None,
            window_title: None,
            window_state: dim_core::PrimaryWindowState::None,
            notes: Vec::new(),
        })
    }

    fn inspect_profile(
        &self,
        profile: &ProfileRecord,
        profile_dir: &Path,
    ) -> Result<ProfileRuntimeState, DimError> {
        let zalo_data_dir = profile_dir.join(".config").join("ZaloData");
        Ok(ProfileRuntimeState {
            profile_id: profile.id.clone(),
            profile_name: profile.name.clone(),
            profile_dir: profile_dir.to_path_buf(),
            zalo_data_dir: zalo_data_dir.clone(),
            lockfile_path: zalo_data_dir.join("SingletonLock"),
            running: false,
            stale_lock_detected: false,
            main_pid: None,
            process_ids: Vec::new(),
            process_count: 0,
            working_set_bytes: 0,
            window_handle: None,
            window_title: None,
            detected_zalo_user_id: None,
            disk_usage_bytes: 0,
            status_label: "Stopped".to_owned(),
        })
    }
}

impl InstanceLauncher for GateTestAdapter {
    fn ensure_profile_layout(
        &self,
        profile: &ProfileRecord,
        profile_dir: &Path,
    ) -> Result<(), DimError> {
        let zdata = profile_dir.join(".config").join("ZaloData");
        fs::create_dir_all(&zdata)?;
        write_profile_id_marker(&zdata, &profile.id)?;
        Ok(())
    }

    fn launch(
        &self,
        _target: &LaunchTarget,
        profile: &ProfileRecord,
        profile_dir: &Path,
    ) -> Result<ProfileRuntimeState, DimError> {
        self.launch_attempted.store(true, Ordering::SeqCst);
        let zalo_data_dir = profile_dir.join(".config").join("ZaloData");
        Ok(ProfileRuntimeState {
            profile_id: profile.id.clone(),
            profile_name: profile.name.clone(),
            profile_dir: profile_dir.to_path_buf(),
            zalo_data_dir: zalo_data_dir.clone(),
            lockfile_path: zalo_data_dir.join("SingletonLock"),
            running: true,
            stale_lock_detected: false,
            main_pid: Some(42_424),
            process_ids: vec![42_424],
            process_count: 1,
            working_set_bytes: 0,
            window_handle: None,
            window_title: None,
            detected_zalo_user_id: None,
            disk_usage_bytes: 0,
            status_label: "Running".to_owned(),
        })
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
        _state: &ProfileRuntimeState,
        _protected_primary_pids: &[u32],
        _timeout_ms: u64,
    ) -> Result<u32, DimError> {
        Ok(0)
    }
}

fn official_target() -> LaunchTarget {
    LaunchTarget::OfficialZalo {
        custom_install_root: None,
    }
}

fn synthetic_profile(store: &JsonProfileStore, name: &str) -> ProfileRecord {
    store.create_profile(name, "").unwrap()
}

#[test]
fn gate_blocks_official_zalo_when_status_unsupported() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonProfileStore::new(tmp.path().join("data")).unwrap());
    let profile = synthetic_profile(&store, "Blocked Repack");
    let adapter = Arc::new(GateTestAdapter::new(
        SupportStatus::Unsupported,
        tmp.path().join("primary"),
    ));
    let coordinator = InstanceCoordinator::new(Arc::clone(&store), Arc::clone(&adapter));

    let err = coordinator
        .launch_profile(&profile.id, &official_target())
        .expect_err("Unsupported official status must keep blocking OfficialZalo launches");
    assert_eq!(err.code(), "UNSUPPORTED_PLATFORM");
    assert!(
        !adapter.launch_attempted.load(Ordering::SeqCst),
        "blocked gate must never reach the adapter launcher"
    );
}

#[test]
fn gate_allows_official_zalo_when_status_experimental() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonProfileStore::new(tmp.path().join("data")).unwrap());
    let profile = synthetic_profile(&store, "Community Repack");
    let adapter = Arc::new(GateTestAdapter::new(
        SupportStatus::Experimental,
        tmp.path().join("primary"),
    ));
    let coordinator = InstanceCoordinator::new(Arc::clone(&store), Arc::clone(&adapter));

    let state = coordinator
        .launch_profile(&profile.id, &official_target())
        .expect("Experimental community-repack status must allow the OfficialZalo route");
    assert!(state.running);
    assert!(
        adapter.launch_attempted.load(Ordering::SeqCst),
        "allowed gate must hand the launch to the adapter"
    );
}

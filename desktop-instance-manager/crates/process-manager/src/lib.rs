//! Process orchestration, primary Zalo Desktop protection, graceful shutdown, and crash recovery.

pub mod coordinator;
pub mod guard;
pub mod recovery;

pub use coordinator::InstanceCoordinator;
pub use guard::{
    ExternalStorageGuard, GracefulQuitSequence, PrimaryProtectionGuard, RunningProfileStorageRef,
    DEFAULT_GRACEFUL_QUIT_TIMEOUT_MS,
};
pub use recovery::StaleLockRecoveryManager;

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::collections::HashMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use dim_core::{
        DimError, InstanceLauncher, InstanceMonitor, LaunchTarget, PlatformCapabilities,
        PlatformCapabilityReport, PlatformKind, PrimaryInstanceStatus, ProfileRecord,
        ProfileRuntimeState, ProfileStore, RecoveryManager, SupportStatus, MIN_FREE_DISK_BYTES,
    };
    use dim_profile_store::{write_profile_id_marker, JsonProfileStore};

    use super::*;

    struct MockAdapter {
        primary: PrimaryInstanceStatus,
        running_pids: Mutex<HashMap<String, u32>>,
        next_pid: Mutex<u32>,
    }

    impl MockAdapter {
        fn new(primary_dir: PathBuf, primary_pid: u32) -> Self {
            Self {
                primary: PrimaryInstanceStatus {
                    zalo_data_dir: primary_dir.clone(),
                    lockfile_path: primary_dir.join("lockfile"),
                    running: true,
                    protected_pids: vec![primary_pid],
                    process_count: 1,
                    working_set_bytes: 0,
                    detected_zalo_user_id: None,
                    notes: Vec::new(),
                },
                running_pids: Mutex::new(HashMap::new()),
                next_pid: Mutex::new(5000),
            }
        }
    }

    impl PlatformCapabilities for MockAdapter {
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
            Ok(MIN_FREE_DISK_BYTES + 10_000_000)
        }

        fn unlink_profile_junctions(&self, _profile_dir: &Path) -> Result<usize, DimError> {
            Ok(0)
        }
    }

    impl InstanceMonitor for MockAdapter {
        fn inspect_primary(&self) -> Result<PrimaryInstanceStatus, DimError> {
            Ok(self.primary.clone())
        }

        fn inspect_profile(
            &self,
            profile: &ProfileRecord,
            profile_dir: &Path,
        ) -> Result<ProfileRuntimeState, DimError> {
            let zalo_data = profile_dir.join("AppData").join("Roaming").join("ZaloData");
            let lockfile = zalo_data.join("lockfile");
            let pid = self.running_pids.lock().unwrap().get(&profile.id).copied();
            let stale = pid.is_none() && lockfile.exists();
            Ok(ProfileRuntimeState {
                profile_id: profile.id.clone(),
                profile_name: profile.name.clone(),
                profile_dir: profile_dir.to_path_buf(),
                zalo_data_dir: zalo_data,
                lockfile_path: lockfile,
                running: pid.is_some(),
                stale_lock_detected: stale,
                main_pid: pid,
                process_ids: pid.into_iter().collect(),
                process_count: usize::from(pid.is_some()),
                working_set_bytes: 0,
                window_handle: None,
                window_title: None,
                detected_zalo_user_id: None,
                disk_usage_bytes: 0,
                status_label: if pid.is_some() {
                    "Running".to_owned()
                } else {
                    "Stopped".to_owned()
                },
            })
        }
    }

    impl InstanceLauncher for MockAdapter {
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
            let mut next = self.next_pid.lock().unwrap();
            let pid = *next;
            *next += 1;
            self.running_pids
                .lock()
                .unwrap()
                .insert(profile.id.clone(), pid);
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
            self.running_pids.lock().unwrap().remove(&state.profile_id);
            Ok(1)
        }
    }

    #[test]
    fn primary_protection_guard_blocks_primary_pid_and_path() {
        let primary = PrimaryInstanceStatus {
            zalo_data_dir: PathBuf::from("/primary/ZaloData"),
            lockfile_path: PathBuf::from("/primary/ZaloData/lockfile"),
            running: true,
            protected_pids: vec![4242, 4243],
            process_count: 2,
            working_set_bytes: 0,
            detected_zalo_user_id: None,
            notes: Vec::new(),
        };
        let state = ProfileRuntimeState {
            profile_id: "aaaaaaaa".to_owned(),
            profile_name: "Test".to_owned(),
            profile_dir: PathBuf::from("/managed/p1"),
            zalo_data_dir: PathBuf::from("/managed/p1/AppData/Roaming/ZaloData"),
            lockfile_path: PathBuf::from("/managed/p1/AppData/Roaming/ZaloData/lockfile"),
            running: true,
            stale_lock_detected: false,
            main_pid: Some(4242),
            process_ids: vec![4242],
            process_count: 1,
            working_set_bytes: 0,
            window_handle: None,
            window_title: None,
            detected_zalo_user_id: None,
            disk_usage_bytes: 0,
            status_label: "Running".to_owned(),
        };

        let err = PrimaryProtectionGuard::ensure_safe_to_stop(&state, &primary).unwrap_err();
        assert_eq!(err.code(), "PROTECTED_PRIMARY_PROCESS");
    }

    #[test]
    fn coordinator_enforces_stale_recovery_and_duplicate_zalo_user_id() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Arc::new(JsonProfileStore::new(tmp.path().join("data")).unwrap());
        let adapter = Arc::new(MockAdapter::new(tmp.path().join("primary"), 9999));
        let coord = InstanceCoordinator::new(Arc::clone(&store), Arc::clone(&adapter));

        let p1 = store.create_profile("Account A", "").unwrap();
        let p2 = store.create_profile("Account B", "").unwrap();

        let p1_zalo = store
            .profile_dir_for(&p1)
            .unwrap()
            .join("AppData")
            .join("Roaming")
            .join("ZaloData");
        let p2_zalo = store
            .profile_dir_for(&p2)
            .unwrap()
            .join("AppData")
            .join("Roaming")
            .join("ZaloData");
        fs::create_dir_all(
            p1_zalo
                .join("Database")
                .join("_production")
                .join("849999001"),
        )
        .unwrap();
        fs::create_dir_all(
            p2_zalo
                .join("Database")
                .join("_production")
                .join("849999001"),
        )
        .unwrap();

        // Simulate stale lockfile from crash on p1
        fs::write(p1_zalo.join("lockfile"), b"dead").unwrap();
        let state_before = coord.inspect_profile(&p1.id).unwrap();
        assert!(state_before.stale_lock_detected);

        let target = LaunchTarget::NodeFixture {
            node_path: None,
            script_path: PathBuf::from("fixture-app.mjs"),
            extra_args: Vec::new(),
        };

        // Launching p1 recovers stale lockfile and starts p1
        let running_p1 = coord.launch_profile(&p1.id, &target).unwrap();
        assert!(running_p1.running);
        assert!(!p1_zalo.join("lockfile").exists());

        // Launching p2 with same logged-in zalo_user_id must fail
        let dup_err = coord.launch_profile(&p2.id, &target).unwrap_err();
        assert_eq!(dup_err.code(), "DUPLICATE_ZALO_USER_ID");

        // Running recovery on live profile must fail
        let recovery = StaleLockRecoveryManager;
        let rec_err = recovery
            .recover_stale_state(&p1, &store.profile_dir_for(&p1).unwrap(), &running_p1)
            .unwrap_err();
        assert_eq!(rec_err.code(), "PROFILE_RUNNING");

        // Stop p1 cleanly
        let stopped = coord.stop_profile(&p1.id).unwrap();
        assert_eq!(stopped, 1);
    }
}

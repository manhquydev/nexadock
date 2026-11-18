//! Crash recovery for stale lockfiles, orphaned signal files, and dead runtime leases.

use std::fs;
use std::path::Path;

use dim_core::{
    check_profile_lockfile_activity, verify_runtime_lease, DimError, LeaseVerification,
    ProfileLockActivity, ProfileRecord, ProfileRuntimeState, RecoveryManager, RecoveryReport,
    RUNTIME_LEASE_FILE, RUNTIME_PID_FILE,
};

const STALE_DATA_DIR_ARTIFACTS: &[&str] =
    &["lockfile", "SingletonLock", ".quit-signal", ".focus-signal"];

/// Safely cleans up stale lockfiles, signals, and dead runtime leases after an abnormal termination.
#[derive(Debug, Default, Clone, Copy)]
pub struct StaleLockRecoveryManager;

impl RecoveryManager for StaleLockRecoveryManager {
    fn recover_stale_state(
        &self,
        profile: &ProfileRecord,
        profile_dir: &Path,
        state: &ProfileRuntimeState,
    ) -> Result<RecoveryReport, DimError> {
        if state.running || state.lifecycle_state().is_active() {
            return Err(DimError::ProfileRunning(format!(
                "{} ({}) is active ({}); refusing lockfile cleanup",
                profile.name,
                profile.id,
                state.lifecycle_state().as_str()
            )));
        }

        // Defense-in-depth: independently verify no live lease or OS-locked lockfile is held!
        if let LeaseVerification::VerifiedLive(lease) =
            verify_runtime_lease(profile_dir, &profile.id, &[])
        {
            return Err(DimError::ProfileRunning(format!(
                "{} ({}) has a verified live runtime lease (PID {}); refusing lockfile cleanup",
                profile.name, profile.id, lease.pid
            )));
        }
        if check_profile_lockfile_activity(profile_dir) == ProfileLockActivity::Active {
            return Err(DimError::ProfileRunning(format!(
                "{} ({}) has an active or OS-locked lockfile; refusing lockfile cleanup",
                profile.name, profile.id
            )));
        }

        let mut removed = Vec::new();

        for name in STALE_DATA_DIR_ARTIFACTS {
            let candidate = state.zalo_data_dir.join(name);
            if remove_if_present_no_follow(&candidate)? {
                removed.push(candidate.display().to_string());
            }
        }

        for hint_name in [RUNTIME_LEASE_FILE, RUNTIME_PID_FILE] {
            let hint_path = profile_dir.join(hint_name);
            if remove_if_present_no_follow(&hint_path)? {
                removed.push(hint_path.display().to_string());
            }
        }

        let recovered = !removed.is_empty();
        let summary = if recovered {
            format!(
                "Removed {} stale lock/lease/signal artifact(s) for profile '{}'",
                removed.len(),
                profile.name
            )
        } else {
            format!(
                "No stale lock artifacts found for profile '{}'",
                profile.name
            )
        };

        Ok(RecoveryReport {
            recovered,
            removed_files: removed,
            summary,
        })
    }
}

fn remove_if_present_no_follow(path: &Path) -> Result<bool, DimError> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            if meta.is_dir() && !meta.file_type().is_symlink() {
                return Ok(false);
            }
            fs::remove_file(path)?;
            Ok(true)
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(DimError::Io(err)),
    }
}

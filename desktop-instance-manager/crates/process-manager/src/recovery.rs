//! Crash recovery for stale lockfiles and orphaned signal files.

use std::fs;
use std::path::Path;

use dim_core::{DimError, ProfileRecord, ProfileRuntimeState, RecoveryManager, RecoveryReport};

const STALE_DATA_DIR_ARTIFACTS: &[&str] =
    &["lockfile", "SingletonLock", ".quit-signal", ".focus-signal"];

/// Safely cleans up stale lockfiles and runtime hints after an abnormal instance termination.
#[derive(Debug, Default, Clone, Copy)]
pub struct StaleLockRecoveryManager;

impl RecoveryManager for StaleLockRecoveryManager {
    fn recover_stale_state(
        &self,
        profile: &ProfileRecord,
        profile_dir: &Path,
        state: &ProfileRuntimeState,
    ) -> Result<RecoveryReport, DimError> {
        if state.running {
            return Err(DimError::ProfileRunning(format!(
                "{} ({}) is still alive; refusing lockfile cleanup",
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

        let runtime_pid = profile_dir.join(".runtime-pid");
        if remove_if_present_no_follow(&runtime_pid)? {
            removed.push(runtime_pid.display().to_string());
        }

        let recovered = !removed.is_empty();
        let summary = if recovered {
            format!(
                "Removed {} stale lock/signal artifact(s) for profile '{}'",
                removed.len(),
                profile.name
            )
        } else {
            format!("No stale lock artifacts found for profile '{}'", profile.name)
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

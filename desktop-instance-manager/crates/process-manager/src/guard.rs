//! Primary Zalo Desktop protection guard and platform graceful-shutdown policy descriptors.

use std::path::Path;

use dim_core::{DimError, PlatformKind, PrimaryInstanceStatus, ProfileRuntimeState};

/// Default timeout in milliseconds to wait for graceful quit before targeted tree cleanup.
pub const DEFAULT_GRACEFUL_QUIT_TIMEOUT_MS: u64 = 3_500;

/// Platform-specific graceful shutdown sequence metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GracefulQuitSequence {
    /// Windows: send `--z-update-force-quit` or `.quit-signal` + `WM_CLOSE`, then wait before targeted PID-tree cleanup.
    WindowsForceQuitAndWmClose,
    /// macOS: `NSRunningApplication.terminate` followed by safe isolated `SIGTERM`.
    MacOsTerminateAndSigterm,
    /// Linux: send `SIGTERM` to isolated PID tree before `SIGKILL`.
    LinuxSigterm,
}

impl GracefulQuitSequence {
    /// Selects the canonical graceful quit sequence for a target platform.
    #[must_use]
    pub const fn for_platform(platform: PlatformKind) -> Self {
        match platform {
            PlatformKind::Windows => Self::WindowsForceQuitAndWmClose,
            PlatformKind::MacOs => Self::MacOsTerminateAndSigterm,
            PlatformKind::Linux => Self::LinuxSigterm,
        }
    }
}

/// Enforces strict separation between managed profile instances and the user's primary Zalo Desktop.
#[derive(Debug, Default, Clone, Copy)]
pub struct PrimaryProtectionGuard;

impl PrimaryProtectionGuard {
    /// Verifies that `state` neither targets the primary Zalo data directory nor overlaps any primary PID.
    pub fn ensure_safe_to_stop(
        state: &ProfileRuntimeState,
        primary: &PrimaryInstanceStatus,
    ) -> Result<(), DimError> {
        if paths_conflict(&state.zalo_data_dir, &primary.zalo_data_dir) {
            return Err(DimError::ProtectedPrimaryProcess(format!(
                "profile data directory '{}' matches primary Zalo directory",
                primary.zalo_data_dir.display()
            )));
        }

        if let Some(main_pid) = state.main_pid {
            if primary.protected_pids.contains(&main_pid) {
                return Err(DimError::ProtectedPrimaryProcess(format!(
                    "main PID {main_pid} belongs to primary Zalo Desktop"
                )));
            }
        }

        for pid in &state.process_ids {
            if primary.protected_pids.contains(pid) {
                return Err(DimError::ProtectedPrimaryProcess(format!(
                    "child PID {pid} belongs to primary Zalo Desktop"
                )));
            }
        }

        Ok(())
    }

    /// Filters a candidate PID list to guarantee no protected primary PID can ever be touched.
    #[must_use]
    pub fn filter_protected_pids(pids: &[u32], protected_primary_pids: &[u32]) -> Vec<u32> {
        pids.iter()
            .copied()
            .filter(|pid| !protected_primary_pids.contains(pid))
            .collect()
    }
}

fn paths_conflict(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    #[cfg(windows)]
    {
        a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        false
    }
}

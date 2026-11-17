//! Primary Zalo Desktop protection guard, external DB/resource collision guard (`IR-WIN-04`),
//! and platform graceful-shutdown policy descriptors.

use std::path::{Path, PathBuf};

use dim_core::{
    external_paths_outside_profile, paths_overlap_or_equal, read_configured_external_paths,
    resolve_effective_path, DimError, PlatformKind, PrimaryInstanceStatus, ProfileRuntimeState,
};

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

/// Active running profile descriptor supplied to [`ExternalStorageGuard::ensure_no_shared_external_paths`].
#[derive(Debug, Clone)]
pub struct RunningProfileStorageRef {
    /// Human-readable owner label (e.g. `"Account Beta (a1b2c3d4)"`).
    pub owner_label: String,
    /// Isolated `<profile_dir>`.
    pub profile_dir: PathBuf,
    /// Isolated `<zalo_data_dir>`.
    pub zalo_data_dir: PathBuf,
}

/// Guards concurrent `launch_profile` against two active profiles (or a secondary profile and primary
/// Zalo Desktop) sharing the same external `db_path.location` or `resource.location.path` (`IR-WIN-04` / `F-CORE-01`).
#[derive(Debug, Default, Clone, Copy)]
pub struct ExternalStorageGuard;

impl ExternalStorageGuard {
    /// Verifies that `target`'s configured external `db_path.location` and `resource.location.path`
    /// do not overlap with any running profile's storage or any protected primary `ZaloData` storage.
    pub fn ensure_no_shared_external_paths(
        target_profile_dir: &Path,
        target_zalo_data_dir: &Path,
        running_others: &[RunningProfileStorageRef],
        protected_primary_zalo_dirs: &[PathBuf],
    ) -> Result<(), DimError> {
        let target_ext = external_paths_outside_profile(target_profile_dir, target_zalo_data_dir);
        let resolved_target_profile = resolve_effective_path(target_profile_dir, target_profile_dir);

        // 1. Check against all currently running secondary profiles.
        for other in running_others {
            let other_ext =
                external_paths_outside_profile(&other.profile_dir, &other.zalo_data_dir);
            let resolved_other_profile =
                resolve_effective_path(&other.profile_dir, &other.profile_dir);

            for t_path in &target_ext {
                if paths_overlap_or_equal(t_path, &resolved_other_profile) {
                    return Err(DimError::SharedExternalDbPath {
                        path: t_path.display().to_string(),
                        conflicting_owner: other.owner_label.clone(),
                    });
                }
                for o_path in &other_ext {
                    if paths_overlap_or_equal(t_path, o_path) {
                        return Err(DimError::SharedExternalDbPath {
                            path: t_path.display().to_string(),
                            conflicting_owner: other.owner_label.clone(),
                        });
                    }
                }
            }

            //Symmetric check: if an already-running profile configured an external path pointing inside `target_profile_dir`!
            for o_path in &other_ext {
                if paths_overlap_or_equal(o_path, &resolved_target_profile) {
                    return Err(DimError::SharedExternalDbPath {
                        path: o_path.display().to_string(),
                        conflicting_owner: other.owner_label.clone(),
                    });
                }
            }
        }

        // 2. Check against protected primary ZaloData directories & their configured external paths (even when primary is stopped!).
        if !target_ext.is_empty() {
            for prim_zalo_dir in protected_primary_zalo_dirs {
                let resolved_prim = resolve_effective_path(prim_zalo_dir, prim_zalo_dir);
                let prim_cfg = read_configured_external_paths(prim_zalo_dir);
                let mut prim_paths = vec![resolved_prim];
                for raw_opt in [prim_cfg.db_location, prim_cfg.resource_location] {
                    if let Some(raw_p) = raw_opt {
                        prim_paths.push(resolve_effective_path(prim_zalo_dir, &raw_p));
                    }
                }

                for t_path in &target_ext {
                    for p_path in &prim_paths {
                        if paths_overlap_or_equal(t_path, p_path) {
                            return Err(DimError::SharedExternalDbPath {
                                path: t_path.display().to_string(),
                                conflicting_owner: "Primary Zalo Desktop".to_owned(),
                            });
                        }
                    }
                }
            }
        }

        Ok(())
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

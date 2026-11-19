//! Core architectural traits implemented by storage, process orchestration, and platform adapters.

use std::path::Path;

use crate::error::DimError;
use crate::models::{
    AccountActivationResult, AccountKind, ActionableRemediation, ActivationOutcome,
    ApplicationInstallation, BackupManifest, BackupResult, CompanionSnapResult, LaunchTarget,
    PlatformCapabilityReport, PlatformKind, PrimaryInstanceStatus, ProfileRecord,
    ProfileRuntimeState, ProfilesDocument, RecoveryReport, VerifiedDeletionState,
    WindowActivationState, WindowRect,
};

/// Discovers target desktop applications and validates their binary metadata/signatures.
pub trait ApplicationLocator {
    /// Discovers the official application installation or validates a custom installation root.
    fn discover_installation(
        &self,
        custom_root: Option<&Path>,
    ) -> Result<ApplicationInstallation, DimError>;
}

/// Manages `profiles.json` and isolated profile directory records.
pub trait ProfileStore {
    /// Loads the complete `profiles.json` document.
    fn load(&self) -> Result<ProfilesDocument, DimError>;

    /// Lists all registered profiles.
    fn list_profiles(&self) -> Result<Vec<ProfileRecord>, DimError>;

    /// Finds a profile by case-insensitive ID or display name.
    fn get_profile(&self, id_or_name: &str) -> Result<ProfileRecord, DimError>;

    /// Creates a new isolated profile and writes `profiles.json` atomically.
    fn create_profile(&self, name: &str, notes: &str) -> Result<ProfileRecord, DimError>;

    /// Renames a profile's display label without breaking running directory isolation.
    fn rename_profile(&self, id_or_name: &str, new_name: &str) -> Result<ProfileRecord, DimError>;

    /// Deletes a profile only when `confirm` is `true`, `deletion_state` is
    /// [`VerifiedDeletionState::ConfirmedStopped`] (rejecting `Starting`, `Running`, `Stopping`,
    /// and indeterminate `Unknown`/`RecoveryRequired` states), AND independent backend OS lock,
    /// runtime lease, marker, path-containment, and two-phase reparse-unlink checks all pass.
    fn delete_profile_verified(
        &self,
        id_or_name: &str,
        confirm: bool,
        deletion_state: VerifiedDeletionState,
    ) -> Result<(), DimError>;

    /// Deletes a profile after explicit confirmation, independent backend OS lock/lease/marker
    /// verification, and two-phase reparse-point unlinking.
    ///
    /// Implementations MUST NOT trust callers blindly: even when invoked directly without an
    /// `InstanceCoordinator` inspection, the store MUST independently verify `.dim-profile.lock`,
    /// `.runtime-lease.json`, and `lockfile` / `SingletonLock` ownership before touching disk.
    fn delete_profile(&self, id_or_name: &str, confirm: bool) -> Result<(), DimError> {
        self.delete_profile_verified(id_or_name, confirm, VerifiedDeletionState::ConfirmedStopped)
    }

    /// Updates `last_launched_at` and optionally detected `zalo_user_id`.
    fn update_last_launched(
        &self,
        id: &str,
        timestamp_rfc3339: &str,
        zalo_user_id: Option<&str>,
    ) -> Result<ProfileRecord, DimError>;
}

/// Launches, focuses, and gracefully terminates isolated application instances.
pub trait InstanceLauncher {
    /// Ensures OS-specific isolated subdirectories and pre-seeded migration markers exist.
    fn ensure_profile_layout(
        &self,
        profile: &ProfileRecord,
        profile_dir: &Path,
    ) -> Result<(), DimError>;

    /// Launches a profile instance under isolated environment variables without handle inheritance.
    fn launch(
        &self,
        target: &LaunchTarget,
        profile: &ProfileRecord,
        profile_dir: &Path,
    ) -> Result<ProfileRuntimeState, DimError>;

    /// Brings an already-running profile window to the foreground or signals second-instance restore.
    fn focus_or_restore(
        &self,
        target: &LaunchTarget,
        profile: &ProfileRecord,
        profile_dir: &Path,
        state: &ProfileRuntimeState,
    ) -> Result<(), DimError>;

    /// Gracefully stops an isolated profile instance, strictly refusing protected primary PIDs.
    fn stop(
        &self,
        state: &ProfileRuntimeState,
        protected_primary_pids: &[u32],
        timeout_ms: u64,
    ) -> Result<u32, DimError>;
}

/// Inspects live lockfiles and process trees for both primary and isolated instances.
pub trait InstanceMonitor {
    /// Inspects the untouched primary user installation (`%APPDATA%\ZaloData`, etc.) to collect protected PIDs.
    fn inspect_primary(&self) -> Result<PrimaryInstanceStatus, DimError>;

    /// Inspects the runtime status, PID tree, lockfile state, and disk usage of a managed profile.
    fn inspect_profile(
        &self,
        profile: &ProfileRecord,
        profile_dir: &Path,
    ) -> Result<ProfileRuntimeState, DimError>;

    /// Inspects the runtime status of a managed profile with optional disk-usage calculation.
    ///
    /// When `include_size == false`, implementations should skip recursive directory walks and set
    /// `disk_usage_bytes = 0` for fast UI rail/snapshot refreshes.
    fn inspect_profile_with_options(
        &self,
        profile: &ProfileRecord,
        profile_dir: &Path,
        include_size: bool,
    ) -> Result<ProfileRuntimeState, DimError> {
        let _ = include_size;
        self.inspect_profile(profile, profile_dir)
    }
}

/// Optional read-only primary focus and verified-secondary companion window orchestration.
pub trait WindowController {
    /// Strictly read-only, non-spawning focus/restore of an already-visible or minimized primary window.
    fn focus_primary_window(
        &self,
        primary: &PrimaryInstanceStatus,
    ) -> Result<AccountActivationResult, DimError> {
        let code = if primary.running {
            "PRIMARY_HIDDEN_IN_TRAY"
        } else {
            "PRIMARY_NOT_RUNNING"
        };
        Ok(AccountActivationResult {
            account_id: "primary".to_owned(),
            account_kind: AccountKind::DesktopPrimary,
            outcome: ActivationOutcome::ManualActionRequired,
            method: Some("read_only_unsupported".to_owned()),
            os_foreground_confirmed: Some(false),
            window_handle: primary.window_handle,
            window_title: primary.window_title.clone(),
            message: "Read-only primary window focus is not supported on this platform adapter."
                .to_owned(),
            remediation: Some(ActionableRemediation {
                code: code.to_owned(),
                message: "Open or restore primary Zalo manually from the desktop or system tray."
                    .to_owned(),
                suggested_action: Some("open_primary_manually".to_owned()),
            }),
        })
    }

    /// Snaps a verified secondary desktop window beside the host KHOANG window.
    fn companion_snap_secondary(
        &self,
        _profile: &ProfileRecord,
        _profile_dir: &Path,
        _host_hwnd: u64,
        _rail_width_px: i32,
    ) -> Result<CompanionSnapResult, DimError> {
        Err(DimError::UnsupportedPlatform(
            "UNSUPPORTED_OPERATION: companion_snap_secondary is not supported on this platform"
                .to_owned(),
        ))
    }

    /// Restores a previously companion-snapped secondary desktop window to `restore_rect`.
    fn companion_unsnap_secondary(
        &self,
        _profile: &ProfileRecord,
        _profile_dir: &Path,
        _restore_rect: WindowRect,
    ) -> Result<CompanionSnapResult, DimError> {
        Err(DimError::UnsupportedPlatform(
            "UNSUPPORTED_OPERATION: companion_unsnap_secondary is not supported on this platform"
                .to_owned(),
        ))
    }

    /// Returns `true` if `hwnd` is currently minimized.
    fn is_window_minimized(&self, _hwnd: u64) -> bool {
        false
    }

    /// Attempts direct PID-verified focus/restore on an already-visible or minimized window.
    ///
    /// Returns `Ok(Some((was_minimized, os_foreground_confirmed)))` when handled directly,
    /// or `Ok(None)` when the platform adapter delegates to `InstanceLauncher::focus_or_restore`.
    fn try_focus_verified_window(
        &self,
        _hwnd: u64,
        _allowed_pids: &[u32],
    ) -> Result<Option<(bool, bool)>, DimError> {
        Ok(None)
    }

    /// Probes the live foreground, visibility, and minimized state of `hwnd`, if supported by the OS.
    fn probe_window_activation_state(&self, _hwnd: u64) -> Option<WindowActivationState> {
        None
    }
}

/// Reports OS capability status, disk space, Folder Redirection, and safe junction unlinking.
pub trait PlatformCapabilities {
    /// Returns the platform adapter identity.
    fn platform(&self) -> PlatformKind;

    /// Returns honest support status and diagnostic constraints for this platform.
    fn capabilities(&self) -> PlatformCapabilityReport;

    /// Returns free bytes available to the current unprivileged user at `path`.
    fn free_disk_bytes(&self, path: &Path) -> Result<u64, DimError>;

    /// Unlinks any OS directory junctions or symlinks inside `profile_dir` without touching targets.
    fn unlink_profile_junctions(&self, profile_dir: &Path) -> Result<usize, DimError>;

    /// Queries whether the current process is running with elevated OS privileges, or `None` if unknown.
    fn is_elevated(&self) -> Option<bool> {
        None
    }

    /// Queries Windows Smart App Control policy state (`VerifiedAndReputablePolicyState`), if applicable.
    fn smart_app_control_state(&self) -> Option<u32> {
        None
    }

    /// Checks whether `profiles_dir` has restrictive ACLs/permissions preventing unprivileged multi-user writes, or `None` if unsupported.
    fn check_profiles_acl_safe(&self, _profiles_dir: &Path) -> Option<bool> {
        None
    }
}

/// Creates atomic verified offline backups and restores them strictly as new profiles.
pub trait BackupManager {
    /// Creates an atomic `<slug>_<timestamp>.partial` -> `backup.json` backup while profile is stopped.
    fn backup_profile(
        &self,
        profile: &ProfileRecord,
        profile_dir: &Path,
        dest_dir: Option<&Path>,
        is_running: bool,
    ) -> Result<BackupResult, DimError>;

    /// Verifies SHA-256 manifest integrity of an existing backup directory.
    fn verify_backup(&self, backup_dir: &Path) -> Result<BackupManifest, DimError>;

    /// Restores a verified backup strictly into a brand-new profile (never overwriting existing profiles).
    fn restore_backup_as_new_profile(
        &self,
        backup_dir: &Path,
        new_name: Option<&str>,
    ) -> Result<ProfileRecord, DimError>;
}

/// Recovers stale lockfiles and crash remnants after an unclean exit.
pub trait RecoveryManager {
    /// Cleans stale lock/signal artifacts only when the profile has no live owning process.
    fn recover_stale_state(
        &self,
        profile: &ProfileRecord,
        profile_dir: &Path,
        state: &ProfileRuntimeState,
    ) -> Result<RecoveryReport, DimError>;
}

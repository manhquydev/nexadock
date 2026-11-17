//! End-to-end orchestration combining profile store, guards, platform adapters, and recovery.

use std::path::Path;
use std::sync::Arc;

use dim_core::{
    check_duplicate_zalo_user_id, detect_zalo_user_id, ensure_min_free_disk_space,
    now_rfc3339_utc, DimError, InstanceLauncher, InstanceMonitor, LaunchTarget,
    PlatformCapabilities, ProfileRecord, ProfileRuntimeState, ProfileStore, RecoveryManager,
    SupportStatus,
};
use dim_profile_store::{verify_profile_id_marker, JsonProfileStore};

use crate::guard::{PrimaryProtectionGuard, DEFAULT_GRACEFUL_QUIT_TIMEOUT_MS};
use crate::recovery::StaleLockRecoveryManager;

/// Coordinates safe profile creation, launch, focus, graceful quit, recovery, and deletion.
pub struct InstanceCoordinator<A> {
    store: Arc<JsonProfileStore>,
    adapter: Arc<A>,
    recovery: StaleLockRecoveryManager,
}

impl<A> std::fmt::Debug for InstanceCoordinator<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstanceCoordinator")
            .field("data_dir", &self.store.data_dir())
            .finish()
    }
}

impl<A> InstanceCoordinator<A>
where
    A: InstanceLauncher + InstanceMonitor + PlatformCapabilities,
{
    /// Constructs a new coordinator over `store` and a platform `adapter`.
    #[must_use]
    pub fn new(store: Arc<JsonProfileStore>, adapter: Arc<A>) -> Self {
        Self {
            store,
            adapter,
            recovery: StaleLockRecoveryManager,
        }
    }

    /// Launches (or focuses if already running) an isolated profile after enforcing all safety guards.
    pub fn launch_profile(
        &self,
        id_or_name: &str,
        target: &LaunchTarget,
    ) -> Result<ProfileRuntimeState, DimError> {
        let profile = self.store.get_profile(id_or_name)?;
        let profile_dir = self.store.profile_dir_for(&profile)?;
        let caps = self.adapter.capabilities();

        if matches!(target, LaunchTarget::OfficialZalo { .. }) {
            if caps.folder_redirection_detected {
                return Err(DimError::FolderRedirectionBlocked(
                    caps.folder_redirection_path
                        .unwrap_or_else(|| "User Shell Folders\\AppData is redirected".to_owned()),
                ));
            }
            if caps.official_zalo_status == SupportStatus::Unsupported {
                return Err(DimError::UnsupportedPlatform(format!(
                    "official Zalo Desktop is unsupported on {}",
                    caps.platform.as_str()
                )));
            }
        }

        self.adapter.ensure_profile_layout(&profile, &profile_dir)?;

        let runtime_before = self.adapter.inspect_profile(&profile, &profile_dir)?;
        verify_profile_id_marker(&runtime_before.zalo_data_dir, &profile.id, false)?;

        if runtime_before.running {
            self.adapter
                .focus_or_restore(target, &profile, &profile_dir, &runtime_before)?;
            return self.adapter.inspect_profile(&profile, &profile_dir);
        }

        let free_bytes = self.adapter.free_disk_bytes(&profile_dir)?;
        ensure_min_free_disk_space(&profile_dir, free_bytes, 0)?;

        if runtime_before.stale_lock_detected {
            let _ = self
                .recovery
                .recover_stale_state(&profile, &profile_dir, &runtime_before)?;
        }

        self.enforce_unique_zalo_user_id(&profile, &runtime_before.zalo_data_dir)?;

        let launched_state = self.adapter.launch(target, &profile, &profile_dir)?;
        let detected_uid = detect_zalo_user_id(&launched_state.zalo_data_dir);
        let _ = self.store.update_last_launched(
            &profile.id,
            &now_rfc3339_utc(),
            detected_uid.as_deref(),
        )?;

        Ok(launched_state)
    }

    /// Gracefully stops an isolated profile while strictly protecting any primary Zalo Desktop PID.
    pub fn stop_profile(&self, id_or_name: &str) -> Result<u32, DimError> {
        let profile = self.store.get_profile(id_or_name)?;
        let profile_dir = self.store.profile_dir_for(&profile)?;
        let primary = self.adapter.inspect_primary()?;
        let state = self.adapter.inspect_profile(&profile, &profile_dir)?;

        if !state.running {
            if state.stale_lock_detected {
                let _ = self
                    .recovery
                    .recover_stale_state(&profile, &profile_dir, &state)?;
            }
            return Ok(0);
        }

        PrimaryProtectionGuard::ensure_safe_to_stop(&state, &primary)?;
        self.adapter.stop(
            &state,
            &primary.protected_pids,
            DEFAULT_GRACEFUL_QUIT_TIMEOUT_MS,
        )
    }

    /// Deletes a stopped profile after unlinking any junctions/symlinks safely.
    pub fn delete_profile(&self, id_or_name: &str, confirm: bool) -> Result<(), DimError> {
        let profile = self.store.get_profile(id_or_name)?;
        let profile_dir = self.store.profile_dir_for(&profile)?;
        let state = self.adapter.inspect_profile(&profile, &profile_dir)?;

        self.store
            .delete_profile_with_guard(id_or_name, confirm, state.running, &|dir| {
                self.adapter.unlink_profile_junctions(dir)
            })
    }

    /// Inspects runtime state of a single profile.
    pub fn inspect_profile(&self, id_or_name: &str) -> Result<ProfileRuntimeState, DimError> {
        let profile = self.store.get_profile(id_or_name)?;
        let profile_dir = self.store.profile_dir_for(&profile)?;
        self.adapter.inspect_profile(&profile, &profile_dir)
    }

    fn enforce_unique_zalo_user_id(
        &self,
        target: &ProfileRecord,
        target_zalo_data_dir: &Path,
    ) -> Result<(), DimError> {
        let target_uid = detect_zalo_user_id(target_zalo_data_dir);
        let all_profiles = self.store.list_profiles()?;
        let mut running = Vec::new();

        for other in all_profiles {
            if other.id.eq_ignore_ascii_case(&target.id) {
                continue;
            }
            let Ok(other_dir) = self.store.profile_dir_for(&other) else {
                continue;
            };
            let Ok(state) = self.adapter.inspect_profile(&other, &other_dir) else {
                continue;
            };
            if state.running {
                let uid = detect_zalo_user_id(&state.zalo_data_dir).or(state.detected_zalo_user_id);
                running.push((other, uid));
            }
        }

        check_duplicate_zalo_user_id(target, target_uid.as_deref(), &running)
    }
}

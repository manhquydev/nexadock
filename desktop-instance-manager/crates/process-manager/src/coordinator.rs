//! End-to-end orchestration combining profile store, guards, platform adapters, and recovery.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use dim_core::{
    check_duplicate_zalo_user_id, detect_zalo_user_id, ensure_min_free_disk_space,
    ensure_not_primary_zalo_path, now_rfc3339_utc, ApplicationLocator, DimError, InstanceLauncher,
    InstanceMonitor, IsolationCheckItem, IsolationVerificationReport, LaunchTarget,
    PlatformCapabilities, PlatformKind, ProfileRecord, ProfileRuntimeState, ProfileStore,
    RecoveryManager, SupportStatus,
};
use dim_profile_store::{is_symlink_or_reparse, verify_profile_id_marker, JsonProfileStore};

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

    /// Focuses an already-running isolated profile window or signals restore.
    pub fn focus_profile(
        &self,
        id_or_name: &str,
        target: &LaunchTarget,
    ) -> Result<ProfileRuntimeState, DimError> {
        let profile = self.store.get_profile(id_or_name)?;
        let profile_dir = self.store.profile_dir_for(&profile)?;
        let state = self.adapter.inspect_profile(&profile, &profile_dir)?;
        if !state.running {
            return Err(DimError::NotRunning(profile.name));
        }
        self.adapter
            .focus_or_restore(target, &profile, &profile_dir, &state)?;
        Ok(state)
    }

    /// Runs live verifiable isolation checks across application identity, primary protection,
    /// path ownership, process ownership, permissions, duplicate accounts, and folder redirection.
    pub fn verify_isolation(
        &self,
        locator: &dyn ApplicationLocator,
        custom_root: Option<&Path>,
        fixture_target: Option<&LaunchTarget>,
    ) -> Result<IsolationVerificationReport, DimError> {
        let caps = self.adapter.capabilities();
        let profiles = self.store.list_profiles()?;
        let primary_res = self.adapter.inspect_primary();

        let mut checks = Vec::new();

        // 1. Application identity
        let app_check = match fixture_target {
            Some(LaunchTarget::NodeFixture { script_path, .. }) => {
                if script_path.exists() {
                    make_isolation_check(
                        "ApplicationDiscovery",
                        "PASS",
                        format!("Mode=node-fixture, Script={}", script_path.display()),
                    )
                } else {
                    make_isolation_check(
                        "ApplicationDiscovery",
                        "FAIL",
                        format!("Fixture script missing: {}", script_path.display()),
                    )
                }
            }
            Some(LaunchTarget::CustomExecutable { path, .. }) => {
                if path.exists() {
                    make_isolation_check(
                        "ApplicationDiscovery",
                        "PASS",
                        format!("Mode=custom-exe, Path={}", path.display()),
                    )
                } else {
                    make_isolation_check(
                        "ApplicationDiscovery",
                        "FAIL",
                        format!("Custom executable missing: {}", path.display()),
                    )
                }
            }
            _ => match locator.discover_installation(custom_root) {
                Ok(install) if install.support_status == SupportStatus::NotTested => {
                    make_isolation_check(
                        "ApplicationDiscovery",
                        "NOT_TESTED",
                        install
                            .error
                            .unwrap_or_else(|| "Platform installation not verified".to_owned()),
                    )
                }
                Ok(install) if install.is_valid => make_isolation_check(
                    "ApplicationDiscovery",
                    "PASS",
                    format!(
                        "Mode=official-zalo, Version={}, Auth={}",
                        install.version.as_deref().unwrap_or("unknown"),
                        install.signature_status
                    ),
                ),
                Ok(install) => make_isolation_check(
                    "ApplicationDiscovery",
                    "FAIL",
                    install
                        .error
                        .unwrap_or_else(|| format!("Auth={}", install.signature_status)),
                ),
                Err(err) => make_isolation_check("ApplicationDiscovery", "FAIL", err.to_string()),
            },
        };
        checks.push(app_check);

        // 2–5. Primary protection, path ownership, process ownership, and duplicate-account checks
        let primary_opt = primary_res.as_ref().ok();

        let mut primary_overlap = ensure_not_primary_zalo_path(self.store.data_dir()).is_err();
        let mut seen_paths = HashSet::new();
        let mut dup_paths = false;
        let mut symlink_escape = false;
        let mut markers_valid = true;
        let mut seen_pids = HashSet::new();
        let mut pid_overlap = false;
        let mut seen_uids = HashSet::new();
        let mut uid_overlap = false;

        if let Some(primary) = primary_opt {
            if primary.running {
                if let Some(p_uid) = primary
                    .detected_zalo_user_id
                    .as_deref()
                    .filter(|s| !s.is_empty())
                {
                    seen_uids.insert(p_uid.to_owned());
                }
            }
        }

        let primary_pids: HashSet<u32> = primary_opt
            .map(|p| p.protected_pids.iter().copied().collect())
            .unwrap_or_default();

        for rec in &profiles {
            let Ok(pdir) = self.store.profile_dir_for(rec) else {
                dup_paths = true;
                continue;
            };
            if let Ok(meta) = std::fs::symlink_metadata(&pdir) {
                if is_symlink_or_reparse(&meta) {
                    symlink_escape = true;
                }
            }
            if ensure_not_primary_zalo_path(&pdir).is_err() {
                primary_overlap = true;
            }
            let Ok(rt) = self.adapter.inspect_profile(rec, &pdir) else {
                markers_valid = false;
                continue;
            };
            if let Ok(zmeta) = std::fs::symlink_metadata(&rt.zalo_data_dir) {
                if is_symlink_or_reparse(&zmeta) {
                    symlink_escape = true;
                }
            }
            if ensure_not_primary_zalo_path(&rt.zalo_data_dir).is_err() {
                primary_overlap = true;
            }
            if let Some(primary) = primary_opt {
                if paths_equal_case_aware(&rt.zalo_data_dir, &primary.zalo_data_dir)
                    || rt.zalo_data_dir.starts_with(&primary.zalo_data_dir)
                {
                    primary_overlap = true;
                }
            }
            let norm_z = normalize_path_key(&rt.zalo_data_dir);
            if !seen_paths.insert(norm_z) {
                dup_paths = true;
            }
            let require_marker = rt.zalo_data_dir.exists();
            if verify_profile_id_marker(&rt.zalo_data_dir, &rec.id, require_marker).is_err() {
                markers_valid = false;
            }
            if rt.running {
                if let Some(pid) = rt.main_pid {
                    if primary_pids.contains(&pid) || !seen_pids.insert(pid) {
                        pid_overlap = true;
                    }
                }
                for pid in &rt.process_ids {
                    if primary_pids.contains(pid) {
                        pid_overlap = true;
                    }
                }
                let uid = detect_zalo_user_id(&rt.zalo_data_dir).or(rt.detected_zalo_user_id);
                if let Some(uid_val) = uid.filter(|s| !s.is_empty()) {
                    if !seen_uids.insert(uid_val) {
                        uid_overlap = true;
                    }
                }
            }
        }

        match &primary_res {
            Ok(primary) => {
                checks.push(make_isolation_check(
                    "PrimaryZaloDataSeparation",
                    if primary_overlap { "FAIL" } else { "PASS" },
                    if primary_overlap {
                        "Managed storage or profile overlaps primary ZaloData".to_owned()
                    } else {
                        format!(
                            "All {} profile(s) isolated outside primary ZaloData (protected_pids={})",
                            profiles.len(),
                            primary.protected_pids.len()
                        )
                    },
                ));
            }
            Err(DimError::UnsupportedPlatform(msg)) => {
                checks.push(make_isolation_check(
                    "PrimaryZaloDataSeparation",
                    "NOT_TESTED",
                    msg.clone(),
                ));
            }
            Err(err) => {
                checks.push(make_isolation_check(
                    "PrimaryZaloDataSeparation",
                    "BLOCKED",
                    err.to_string(),
                ));
            }
        }

        let path_ownership_ok = !dup_paths && !symlink_escape && markers_valid;
        checks.push(make_isolation_check(
            "DistinctProfilePathsAndMarkers",
            if path_ownership_ok { "PASS" } else { "FAIL" },
            format!(
                "Checked {} profile(s), markers_valid={}, symlink_escape={}",
                profiles.len(),
                markers_valid,
                symlink_escape
            ),
        ));

        match &primary_res {
            Ok(primary) => {
                checks.push(make_isolation_check(
                    "DistinctProcessOwnership",
                    if pid_overlap { "FAIL" } else { "PASS" },
                    format!(
                        "Active isolated PIDs={}, Primary PIDs={}",
                        seen_pids.len(),
                        primary.protected_pids.len()
                    ),
                ));
            }
            Err(DimError::UnsupportedPlatform(msg)) => {
                checks.push(make_isolation_check(
                    "DistinctProcessOwnership",
                    "NOT_TESTED",
                    msg.clone(),
                ));
            }
            Err(err) => {
                checks.push(make_isolation_check(
                    "DistinctProcessOwnership",
                    "BLOCKED",
                    err.to_string(),
                ));
            }
        }

        checks.push(make_isolation_check(
            "DistinctZaloUserIds",
            if uid_overlap { "FAIL" } else { "PASS" },
            if uid_overlap {
                "Duplicate logged-in zalo_user_id detected across active sessions".to_owned()
            } else {
                "No duplicate logged-in zalo_user_id among active instances".to_owned()
            },
        ));

        // 6. File permissions / ACL check
        let acl_check = match self
            .adapter
            .check_profiles_acl_safe(self.store.profiles_dir())
        {
            Some(true) => make_isolation_check(
                "RestrictiveNtfsAcls",
                "PASS",
                "Profiles directory restricts unprivileged multi-user write access".to_owned(),
            ),
            Some(false) => make_isolation_check(
                "RestrictiveNtfsAcls",
                "FAIL",
                "Profiles directory permits broad Everyone or BUILTIN\\Users write access"
                    .to_owned(),
            ),
            None => make_isolation_check(
                "RestrictiveNtfsAcls",
                "NOT_TESTED",
                "OS permission verification unavailable on current adapter/state".to_owned(),
            ),
        };
        checks.push(acl_check);

        // 7. Folder redirection check
        let folder_check = if caps.platform == PlatformKind::Windows {
            if caps.folder_redirection_detected {
                make_isolation_check(
                    "FolderRedirectionCheck",
                    "BLOCKED",
                    caps.folder_redirection_path
                        .unwrap_or_else(|| "User Shell Folders\\AppData is redirected".to_owned()),
                )
            } else {
                make_isolation_check(
                    "FolderRedirectionCheck",
                    "PASS",
                    "AppData resolves locally under USERPROFILE".to_owned(),
                )
            }
        } else {
            make_isolation_check(
                "FolderRedirectionCheck",
                "PASS",
                "Windows Folder Redirection not applicable on POSIX".to_owned(),
            )
        };
        checks.push(folder_check);

        let all_passed = checks.iter().all(|c| c.status == "PASS");
        Ok(IsolationVerificationReport {
            ok: all_passed,
            all_passed,
            timestamp_utc: now_rfc3339_utc(),
            checks,
        })
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

        check_duplicate_zalo_user_id(target, target_uid.as_deref(), &running)?;

        if let Some(t_uid) = target_uid
            .as_deref()
            .or(target.zalo_user_id.as_deref())
            .filter(|s| !s.is_empty())
        {
            if let Ok(primary) = self.adapter.inspect_primary() {
                if primary.running {
                    let p_uid = primary
                        .detected_zalo_user_id
                        .or_else(|| detect_zalo_user_id(&primary.zalo_data_dir));
                    if p_uid.as_deref() == Some(t_uid) {
                        return Err(DimError::DuplicateZaloUserId {
                            user_id: t_uid.to_owned(),
                            running_profile: "Primary Zalo Desktop".to_owned(),
                        });
                    }
                }
            }
        }

        Ok(())
    }
}

fn make_isolation_check(name: &str, status: &str, detail: String) -> IsolationCheckItem {
    IsolationCheckItem {
        check_name: name.to_owned(),
        status: status.to_owned(),
        passed: status == "PASS",
        detail,
    }
}

fn normalize_path_key(path: &Path) -> String {
    let raw = path.to_string_lossy().replace('/', "\\");
    let trimmed = raw.trim_end_matches(['\\', '/']);
    if cfg!(windows) {
        trimmed.to_ascii_lowercase()
    } else {
        trimmed.to_owned()
    }
}

fn paths_equal_case_aware(a: &Path, b: &Path) -> bool {
    normalize_path_key(a) == normalize_path_key(b)
}

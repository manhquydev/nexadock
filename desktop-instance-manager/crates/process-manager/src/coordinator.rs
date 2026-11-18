//! End-to-end orchestration combining profile store, per-profile concurrency coordination,
//! cross-process `.dim-profile.lock` synchronization, primary protection guards, platform adapters,
//! and stale-state crash recovery.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use dim_core::{
    check_duplicate_zalo_user_id, detect_zalo_user_id, ensure_min_free_disk_space,
    ensure_not_primary_zalo_path, is_existing_lock_contended, now_rfc3339_utc,
    read_live_launch_admission_claims, write_launch_admission_claim, AdvisoryFileLock,
    ApplicationLocator, DimError, InstanceLauncher, InstanceMonitor, IsolationCheckItem,
    IsolationVerificationReport, LaunchAdmissionClaim, LaunchTarget, PlatformCapabilities,
    PlatformKind, ProfileLifecycleState, ProfileRecord, ProfileRuntimeState, ProfileStore,
    RecoveryManager, SupportStatus, VerifiedDeletionState, DEFAULT_ADVISORY_LOCK_TIMEOUT,
    PROFILE_OP_LOCK_FILE,
};
use dim_profile_store::{is_symlink_or_reparse, verify_profile_id_marker, JsonProfileStore};

use crate::guard::{
    ExternalStorageGuard, PrimaryProtectionGuard, RunningProfileStorageRef,
    DEFAULT_GRACEFUL_QUIT_TIMEOUT_MS,
};
use crate::recovery::StaleLockRecoveryManager;

#[derive(Debug, Clone)]
enum CoalescedLaunchError {
    NotFound(String),
    ProfileRunning(String),
    MarkerMismatch {
        expected: String,
        actual: String,
    },
    DuplicateUid {
        user_id: String,
        running_profile: String,
    },
    SharedExternalDb {
        path: String,
        owner: String,
    },
    InsufficientDisk {
        path: String,
        available: u64,
        required: u64,
    },
    FolderRedirection(String),
    InstallNotFound(String),
    UnverifiedBinary(String),
    UnsupportedPlatform(String),
    LaunchFailed(String),
    ProcessError(String),
    Other(String),
}

impl CoalescedLaunchError {
    fn from_dim_error(err: &DimError) -> Self {
        match err {
            DimError::ProfileNotFound(s) => Self::NotFound(s.clone()),
            DimError::ProfileRunning(s) => Self::ProfileRunning(s.clone()),
            DimError::ProfileIdentityMismatch { expected, actual } => Self::MarkerMismatch {
                expected: expected.clone(),
                actual: actual.clone(),
            },
            DimError::DuplicateZaloUserId {
                user_id,
                running_profile,
            } => Self::DuplicateUid {
                user_id: user_id.clone(),
                running_profile: running_profile.clone(),
            },
            DimError::SharedExternalDbPath {
                path,
                conflicting_owner,
            } => Self::SharedExternalDb {
                path: path.clone(),
                owner: conflicting_owner.clone(),
            },
            DimError::InsufficientDiskSpace {
                path,
                available_bytes,
                required_bytes,
            } => Self::InsufficientDisk {
                path: path.clone(),
                available: *available_bytes,
                required: *required_bytes,
            },
            DimError::FolderRedirectionBlocked(s) => Self::FolderRedirection(s.clone()),
            DimError::InstallNotFound(s) => Self::InstallNotFound(s.clone()),
            DimError::UnverifiedBinary(s) => Self::UnverifiedBinary(s.clone()),
            DimError::UnsupportedPlatform(s) => Self::UnsupportedPlatform(s.clone()),
            DimError::LaunchFailed(s) => Self::LaunchFailed(s.clone()),
            DimError::ProcessError(s) => Self::ProcessError(s.clone()),
            other => Self::Other(other.to_string()),
        }
    }

    fn into_dim_error(self) -> DimError {
        match self {
            Self::NotFound(s) => DimError::ProfileNotFound(s),
            Self::ProfileRunning(s) => DimError::ProfileRunning(s),
            Self::MarkerMismatch { expected, actual } => {
                DimError::ProfileIdentityMismatch { expected, actual }
            }
            Self::DuplicateUid {
                user_id,
                running_profile,
            } => DimError::DuplicateZaloUserId {
                user_id,
                running_profile,
            },
            Self::SharedExternalDb { path, owner } => DimError::SharedExternalDbPath {
                path,
                conflicting_owner: owner,
            },
            Self::InsufficientDisk {
                path,
                available,
                required,
            } => DimError::InsufficientDiskSpace {
                path,
                available_bytes: available,
                required_bytes: required,
            },
            Self::FolderRedirection(s) => DimError::FolderRedirectionBlocked(s),
            Self::InstallNotFound(s) => DimError::InstallNotFound(s),
            Self::UnverifiedBinary(s) => DimError::UnverifiedBinary(s),
            Self::UnsupportedPlatform(s) => DimError::UnsupportedPlatform(s),
            Self::LaunchFailed(s) => DimError::LaunchFailed(s),
            Self::ProcessError(s) => DimError::ProcessError(s),
            Self::Other(s) => DimError::ProcessError(s),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InFlightPhase {
    Idle,
    Launching,
    Stopping,
    Deleting,
}

#[derive(Debug)]
struct ProfileSyncState {
    phase: InFlightPhase,
    stop_requested: bool,
    launch_epoch: u64,
    last_launch_result: Option<(u64, Result<ProfileRuntimeState, CoalescedLaunchError>)>,
}

#[derive(Debug)]
struct ProfileSyncEntry {
    state: Mutex<ProfileSyncState>,
    cv: Condvar,
    op_mutex: Mutex<()>,
}

impl ProfileSyncEntry {
    fn new() -> Self {
        Self {
            state: Mutex::new(ProfileSyncState {
                phase: InFlightPhase::Idle,
                stop_requested: false,
                launch_epoch: 0,
                last_launch_result: None,
            }),
            cv: Condvar::new(),
            op_mutex: Mutex::new(()),
        }
    }

    fn is_stop_requested(&self) -> bool {
        self.state.lock().map(|s| s.stop_requested).unwrap_or(false)
    }

    fn current_phase(&self) -> InFlightPhase {
        self.state
            .lock()
            .map(|s| s.phase)
            .unwrap_or(InFlightPhase::Idle)
    }
}

#[derive(Debug)]
struct AdmissionClaimGuard {
    key: String,
    claims_map: Arc<Mutex<HashMap<String, LaunchAdmissionClaim>>>,
    claim_file_path: PathBuf,
}

impl Drop for AdmissionClaimGuard {
    fn drop(&mut self) {
        if let Ok(mut map) = self.claims_map.lock() {
            map.remove(&self.key);
        }
        let _ = fs::remove_file(&self.claim_file_path);
    }
}

/// Coordinates safe profile creation, launch, focus, graceful quit, recovery, and deletion.
pub struct InstanceCoordinator<A> {
    store: Arc<JsonProfileStore>,
    adapter: Arc<A>,
    recovery: StaleLockRecoveryManager,
    profile_sync: Mutex<HashMap<String, Arc<ProfileSyncEntry>>>,
    in_flight_claims: Arc<Mutex<HashMap<String, LaunchAdmissionClaim>>>,
    protected_zalo_data_dirs: Vec<PathBuf>,
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
            profile_sync: Mutex::new(HashMap::new()),
            in_flight_claims: Arc::new(Mutex::new(HashMap::new())),
            protected_zalo_data_dirs: Vec::new(),
        }
    }

    /// Registers an explicit protected `ZaloData` directory (e.g. synthetic primary path in tests)
    /// whose configured external DB/resource paths must never be shared with secondary profiles.
    #[must_use]
    pub fn with_protected_zalo_data_dir(mut self, zalo_data_dir: PathBuf) -> Self {
        self.protected_zalo_data_dirs.push(zalo_data_dir);
        self
    }

    fn sync_entry_for(
        &self,
        canonical_profile_id: &str,
    ) -> Result<Arc<ProfileSyncEntry>, DimError> {
        let key = canonical_profile_id.to_ascii_lowercase();
        let mut map = self
            .profile_sync
            .lock()
            .map_err(|_| DimError::Io(std::io::Error::other("coordinator sync map poisoned")))?;
        Ok(Arc::clone(
            map.entry(key)
                .or_insert_with(|| Arc::new(ProfileSyncEntry::new())),
        ))
    }

    /// Launches (or focuses if already running) an isolated profile after enforcing all safety
    /// and per-profile concurrency guards.
    pub fn launch_profile(
        &self,
        id_or_name: &str,
        target: &LaunchTarget,
    ) -> Result<ProfileRuntimeState, DimError> {
        let initial_profile = self.store.get_profile(id_or_name)?;
        let profile_id = initial_profile.id.clone();
        let entry = self.sync_entry_for(&profile_id)?;

        let leader_epoch = {
            let mut st = entry
                .state
                .lock()
                .map_err(|_| DimError::Io(std::io::Error::other("profile sync mutex poisoned")))?;

            if st.phase == InFlightPhase::Launching {
                let wait_epoch = st.launch_epoch;
                while st.phase == InFlightPhase::Launching && st.launch_epoch == wait_epoch {
                    st = entry.cv.wait(st).map_err(|_| {
                        DimError::Io(std::io::Error::other("profile condvar wait poisoned"))
                    })?;
                }
                if let Some((done_epoch, ref outcome)) = st.last_launch_result {
                    if done_epoch == wait_epoch {
                        return outcome
                            .clone()
                            .map_err(CoalescedLaunchError::into_dim_error);
                    }
                }
            }

            while st.phase == InFlightPhase::Stopping || st.phase == InFlightPhase::Deleting {
                st = entry.cv.wait(st).map_err(|_| {
                    DimError::Io(std::io::Error::other("profile condvar wait poisoned"))
                })?;
            }

            st.launch_epoch = st.launch_epoch.wrapping_add(1);
            st.phase = InFlightPhase::Launching;
            st.stop_requested = false;
            st.launch_epoch
        };

        let result = self.execute_serialized_launch(&profile_id, target, &entry);

        if let Ok(mut st) = entry.state.lock() {
            let stored = match &result {
                Ok(rt) => Ok(rt.clone()),
                Err(e) => Err(CoalescedLaunchError::from_dim_error(e)),
            };
            st.last_launch_result = Some((leader_epoch, stored));
            if st.phase == InFlightPhase::Launching {
                st.phase = InFlightPhase::Idle;
            }
            entry.cv.notify_all();
        }

        result
    }

    fn execute_serialized_launch(
        &self,
        profile_id: &str,
        target: &LaunchTarget,
        entry: &ProfileSyncEntry,
    ) -> Result<ProfileRuntimeState, DimError> {
        let _op_guard = entry
            .op_mutex
            .lock()
            .map_err(|_| DimError::Io(std::io::Error::other("profile op mutex poisoned")))?;

        // Acquire cross-process per-profile lock in `<data_dir>/locks/<id>.lock` (outside `Profiles/`)
        // BEFORE re-verifying `get_profile` or creating `profile_dir`, preventing deleted-profile
        // directory resurrection and `ERROR_DIR_NOT_EMPTY` mid-delete races across OS processes.
        let profile_op_lock_path = self.store.profile_lock_path(profile_id);
        let _cross_proc_guard = AdvisoryFileLock::acquire_exclusive(
            &profile_op_lock_path,
            DEFAULT_ADVISORY_LOCK_TIMEOUT,
        )?;

        let profile = self.store.get_profile(profile_id)?;
        let profile_dir = self.store.profile_dir_for(&profile)?;
        if !profile_dir.exists() {
            fs::create_dir_all(&profile_dir)?;
        }
        if is_existing_lock_contended(&profile_dir.join(PROFILE_OP_LOCK_FILE))? {
            return Err(DimError::ProfileRunning(format!(
                "profile '{}' ({}) is locked by another process (.dim-profile.lock)",
                profile.name, profile.id
            )));
        }
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

        if entry.is_stop_requested() {
            return Err(DimError::LaunchFailed(
                "launch cancelled by concurrent stop_profile request".to_owned(),
            ));
        }

        self.adapter.ensure_profile_layout(&profile, &profile_dir)?;

        let runtime_before = self.adapter.inspect_profile(&profile, &profile_dir)?;
        verify_profile_id_marker(&runtime_before.zalo_data_dir, &profile.id, false)?;

        match runtime_before.lifecycle_state() {
            ProfileLifecycleState::Running => {
                self.adapter
                    .focus_or_restore(target, &profile, &profile_dir, &runtime_before)?;
                return self.adapter.inspect_profile(&profile, &profile_dir);
            }
            ProfileLifecycleState::Starting => {
                let wait_until = Instant::now() + Duration::from_millis(1_500);
                while Instant::now() < wait_until && !entry.is_stop_requested() {
                    let cur = self.adapter.inspect_profile(&profile, &profile_dir)?;
                    if cur.lifecycle_state() != ProfileLifecycleState::Starting {
                        return Ok(cur);
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
                return self.adapter.inspect_profile(&profile, &profile_dir);
            }
            ProfileLifecycleState::Stopping => {
                return Err(DimError::ProfileRunning(format!(
                    "profile '{}' is currently stopping",
                    profile.name
                )));
            }
            ProfileLifecycleState::RecoveryRequired => {
                let _ =
                    self.recovery
                        .recover_stale_state(&profile, &profile_dir, &runtime_before)?;
            }
            ProfileLifecycleState::Stopped => {
                if runtime_before.stale_lock_detected {
                    let _ = self.recovery.recover_stale_state(
                        &profile,
                        &profile_dir,
                        &runtime_before,
                    )?;
                }
            }
        }

        let free_bytes = self.adapter.free_disk_bytes(&profile_dir)?;
        ensure_min_free_disk_space(&profile_dir, free_bytes, 0)?;

        // Fast pre-spawn admission check (`<5ms`) serialized across threads & processes via
        // `in_flight_claims` + `<data_dir>/locks/_launch_admission.lock`, released BEFORE
        // `self.adapter.launch(...)` so distinct-profile readiness polling runs in parallel!
        let _admission_claim_guard = self.acquire_launch_admission_claim(
            &profile,
            &profile_dir,
            &runtime_before.zalo_data_dir,
        )?;

        if entry.is_stop_requested() {
            return Err(DimError::LaunchFailed(
                "launch cancelled by concurrent stop_profile request".to_owned(),
            ));
        }

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
        let entry = self.sync_entry_for(&profile.id)?;

        if let Ok(mut st) = entry.state.lock() {
            st.stop_requested = true;
            if st.phase == InFlightPhase::Launching {
                if let Ok(rt_peek) = self.adapter.inspect_profile(&profile, &profile_dir) {
                    if rt_peek.zalo_data_dir.exists() {
                        let _ = fs::write(
                            rt_peek.zalo_data_dir.join(".quit-signal"),
                            format!("{{\"requested_at\":\"{}\"}}\n", now_rfc3339_utc()),
                        );
                    }
                }
            }
            entry.cv.notify_all();
        }

        let _op_guard = entry
            .op_mutex
            .lock()
            .map_err(|_| DimError::Io(std::io::Error::other("profile op mutex poisoned")))?;

        if let Ok(mut st) = entry.state.lock() {
            st.phase = InFlightPhase::Stopping;
            entry.cv.notify_all();
        }

        let res = (|| -> Result<u32, DimError> {
            let cross_lock_guard = AdvisoryFileLock::acquire_exclusive(
                &self.store.profile_lock_path(&profile.id),
                DEFAULT_ADVISORY_LOCK_TIMEOUT,
            )?;

            let fresh_profile = match self.store.get_profile(&profile.id) {
                Ok(rec) => rec,
                Err(DimError::ProfileNotFound(_)) => {
                    drop(cross_lock_guard);
                    return Ok(0);
                }
                Err(err) => return Err(err),
            };
            let fresh_dir = self.store.profile_dir_for(&fresh_profile)?;
            if !fresh_dir.exists() {
                drop(cross_lock_guard);
                return Ok(0);
            }

            let primary = self.adapter.inspect_primary()?;
            let mut state = self.adapter.inspect_profile(&fresh_profile, &fresh_dir)?;

            if !state.running && !state.lifecycle_state().is_active() {
                if state.stale_lock_detected {
                    let _ =
                        self.recovery
                            .recover_stale_state(&fresh_profile, &fresh_dir, &state)?;
                }
                drop(cross_lock_guard);
                return Ok(0);
            }

            PrimaryProtectionGuard::ensure_safe_to_stop(&state, &primary)?;

            let was_starting = state.lifecycle_state() == ProfileLifecycleState::Starting;
            if !was_starting {
                state.set_lifecycle_state(ProfileLifecycleState::Stopping);
            }

            let stopped = self.adapter.stop(
                &state,
                &primary.protected_pids,
                DEFAULT_GRACEFUL_QUIT_TIMEOUT_MS,
            )?;
            drop(cross_lock_guard);
            Ok(stopped)
        })();

        if let Ok(mut st) = entry.state.lock() {
            st.phase = InFlightPhase::Idle;
            st.stop_requested = false;
            entry.cv.notify_all();
        }

        res
    }

    /// Gracefully stops all currently running secondary profiles while leaving primary Zalo Desktop
    /// completely untouched (`IR-WIN-04` / `F-CORE-02` safe upgrade preparation).
    pub fn stop_all_secondary_profiles(&self) -> Result<Vec<(String, u32)>, DimError> {
        let profiles = self.store.list_profiles()?;
        let mut outcomes = Vec::new();
        for rec in profiles {
            let count = self.stop_profile(&rec.id)?;
            if count > 0 {
                outcomes.push((rec.id, count));
            }
        }
        Ok(outcomes)
    }

    /// Deletes a stopped profile after unlinking any junctions/symlinks safely.
    pub fn delete_profile(&self, id_or_name: &str, confirm: bool) -> Result<(), DimError> {
        if !confirm {
            return Err(DimError::ConfirmationRequired(
                "deleting a profile requires explicit confirmation (--confirm)".to_owned(),
            ));
        }

        let profile = self.store.get_profile(id_or_name)?;
        let entry = self.sync_entry_for(&profile.id)?;

        {
            let st = entry
                .state
                .lock()
                .map_err(|_| DimError::Io(std::io::Error::other("profile sync mutex poisoned")))?;
            if st.phase == InFlightPhase::Launching || st.phase == InFlightPhase::Stopping {
                return Err(DimError::ProfileRunning(format!(
                    "profile '{}' ({}) has an active lifecycle transition in flight",
                    profile.name, profile.id
                )));
            }
        }

        let _op_guard = entry
            .op_mutex
            .lock()
            .map_err(|_| DimError::Io(std::io::Error::other("profile op mutex poisoned")))?;

        if let Ok(mut st) = entry.state.lock() {
            st.phase = InFlightPhase::Deleting;
            entry.cv.notify_all();
        }

        let res = (|| -> Result<(), DimError> {
            let fresh_profile = self.store.get_profile(&profile.id)?;
            let profile_dir = self.store.profile_dir_for(&fresh_profile)?;
            let state = self.adapter.inspect_profile(&fresh_profile, &profile_dir)?;
            let deletion_state = VerifiedDeletionState::from_runtime_state(&state);

            self.store.delete_profile_with_guard(
                &fresh_profile.id,
                confirm,
                deletion_state,
                &|dir| self.adapter.unlink_profile_junctions(dir),
            )
        })();

        if let Ok(mut st) = entry.state.lock() {
            st.phase = InFlightPhase::Idle;
            entry.cv.notify_all();
        }

        res
    }

    /// Inspects runtime state of an already-loaded [`ProfileRecord`], overlaying in-flight
    /// coordinator `Stopping` transitions so Tauri IPC and CLI `status` surface accurate lifecycle states.
    pub fn inspect_profile_record(
        &self,
        profile: &ProfileRecord,
        profile_dir: &Path,
    ) -> Result<ProfileRuntimeState, DimError> {
        let mut rt = self.adapter.inspect_profile(profile, profile_dir)?;
        if let Ok(entry) = self.sync_entry_for(&profile.id) {
            if entry.current_phase() == InFlightPhase::Stopping && rt.main_pid.is_some() {
                rt.set_lifecycle_state(ProfileLifecycleState::Stopping);
            }
        }
        Ok(rt)
    }

    /// Inspects runtime state of a single profile, reflecting in-flight `Stopping` transitions.
    pub fn inspect_profile(&self, id_or_name: &str) -> Result<ProfileRuntimeState, DimError> {
        let profile = self.store.get_profile(id_or_name)?;
        let profile_dir = self.store.profile_dir_for(&profile)?;
        self.inspect_profile_record(&profile, &profile_dir)
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

    fn acquire_launch_admission_claim(
        &self,
        target: &ProfileRecord,
        target_profile_dir: &Path,
        target_zalo_data_dir: &Path,
    ) -> Result<AdmissionClaimGuard, DimError> {
        let mut in_proc = self
            .in_flight_claims
            .lock()
            .map_err(|_| DimError::Io(std::io::Error::other("launch admission mutex poisoned")))?;
        let _cross_adm = AdvisoryFileLock::acquire_exclusive(
            &self.store.launch_admission_lock_path(),
            DEFAULT_ADVISORY_LOCK_TIMEOUT,
        )?;

        let target_key = target.id.to_ascii_lowercase();
        let mut merged_claims: HashMap<String, LaunchAdmissionClaim> = HashMap::new();
        for claim in read_live_launch_admission_claims(self.store.data_dir()) {
            let k = claim.profile_id.to_ascii_lowercase();
            if k != target_key {
                merged_claims.insert(k, claim);
            }
        }
        for (k, claim) in in_proc.iter() {
            if k != &target_key {
                merged_claims.insert(k.clone(), claim.clone());
            }
        }
        let active_claims: Vec<LaunchAdmissionClaim> = merged_claims.into_values().collect();

        self.enforce_no_shared_external_storage(
            target,
            target_profile_dir,
            target_zalo_data_dir,
            &active_claims,
        )?;
        let target_uid =
            self.enforce_unique_zalo_user_id(target, target_zalo_data_dir, &active_claims)?;

        let claim_file_path = write_launch_admission_claim(
            self.store.data_dir(),
            &target_key,
            &target.name,
            target_profile_dir,
            target_zalo_data_dir,
            target_uid.clone(),
        )?;

        in_proc.insert(
            target_key.clone(),
            LaunchAdmissionClaim {
                claimer_pid: std::process::id(),
                claimer_create_time: 0,
                profile_id: target_key.clone(),
                profile_name: target.name.clone(),
                profile_dir: target_profile_dir.to_path_buf(),
                zalo_data_dir: target_zalo_data_dir.to_path_buf(),
                zalo_user_id: target_uid,
            },
        );

        Ok(AdmissionClaimGuard {
            key: target_key,
            claims_map: Arc::clone(&self.in_flight_claims),
            claim_file_path,
        })
    }

    fn enforce_unique_zalo_user_id(
        &self,
        target: &ProfileRecord,
        target_zalo_data_dir: &Path,
        active_claims: &[LaunchAdmissionClaim],
    ) -> Result<Option<String>, DimError> {
        let target_uid = detect_zalo_user_id(target_zalo_data_dir);
        let effective_target_uid = target_uid
            .clone()
            .or_else(|| target.zalo_user_id.clone())
            .filter(|s| !s.is_empty());
        let all_profiles = self.store.list_profiles()?;
        let mut running = Vec::new();
        let mut seen_ids = HashSet::new();

        for other in &all_profiles {
            if other.id.eq_ignore_ascii_case(&target.id) {
                continue;
            }
            let Ok(other_dir) = self.store.profile_dir_for(other) else {
                continue;
            };
            let Ok(state) = self.adapter.inspect_profile(other, &other_dir) else {
                continue;
            };
            if state.running {
                let uid = detect_zalo_user_id(&state.zalo_data_dir).or(state.detected_zalo_user_id);
                seen_ids.insert(other.id.to_ascii_lowercase());
                running.push((other.clone(), uid));
            }
        }

        for claim in active_claims {
            let k = claim.profile_id.to_ascii_lowercase();
            if seen_ids.contains(&k) || k.eq_ignore_ascii_case(&target.id) {
                continue;
            }
            let synth_rec = all_profiles
                .iter()
                .find(|p| p.id.eq_ignore_ascii_case(&claim.profile_id))
                .cloned()
                .unwrap_or_else(|| ProfileRecord {
                    id: claim.profile_id.clone(),
                    name: claim.profile_name.clone(),
                    directory_name: String::new(),
                    created_at: String::new(),
                    last_launched_at: None,
                    zalo_user_id: claim.zalo_user_id.clone(),
                    notes: String::new(),
                });
            let uid =
                detect_zalo_user_id(&claim.zalo_data_dir).or_else(|| claim.zalo_user_id.clone());
            seen_ids.insert(k);
            running.push((synth_rec, uid));
        }

        check_duplicate_zalo_user_id(target, target_uid.as_deref(), &running)?;

        if let Some(ref t_uid) = effective_target_uid {
            if let Ok(primary) = self.adapter.inspect_primary() {
                if primary.running {
                    let p_uid = primary
                        .detected_zalo_user_id
                        .or_else(|| detect_zalo_user_id(&primary.zalo_data_dir));
                    if p_uid.as_deref() == Some(t_uid.as_str()) {
                        return Err(DimError::DuplicateZaloUserId {
                            user_id: t_uid.clone(),
                            running_profile: "Primary Zalo Desktop".to_owned(),
                        });
                    }
                }
            }
        }

        Ok(effective_target_uid)
    }

    fn enforce_no_shared_external_storage(
        &self,
        target: &ProfileRecord,
        target_profile_dir: &Path,
        target_zalo_data_dir: &Path,
        active_claims: &[LaunchAdmissionClaim],
    ) -> Result<(), DimError> {
        let all_profiles = self.store.list_profiles()?;
        let mut running_others = Vec::new();
        let mut seen_ids = HashSet::new();

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
                seen_ids.insert(other.id.to_ascii_lowercase());
                running_others.push(RunningProfileStorageRef {
                    owner_label: format!("{} ({})", other.name, other.id),
                    profile_dir: other_dir,
                    zalo_data_dir: state.zalo_data_dir,
                });
            }
        }

        for claim in active_claims {
            let k = claim.profile_id.to_ascii_lowercase();
            if seen_ids.contains(&k) || k.eq_ignore_ascii_case(&target.id) {
                continue;
            }
            seen_ids.insert(k);
            running_others.push(RunningProfileStorageRef {
                owner_label: format!("{} ({})", claim.profile_name, claim.profile_id),
                profile_dir: claim.profile_dir.clone(),
                zalo_data_dir: claim.zalo_data_dir.clone(),
            });
        }

        let mut protected_dirs = self.protected_zalo_data_dirs.clone();
        if let Some(env_prim) = std::env::var_os("DIM_PROTECTED_PRIMARY_ZALO_DATA") {
            if !env_prim.is_empty() {
                protected_dirs.push(PathBuf::from(env_prim));
            }
        } else if let Ok(primary) = self.adapter.inspect_primary() {
            if !primary.zalo_data_dir.as_os_str().is_empty() {
                protected_dirs.push(primary.zalo_data_dir);
            }
        }

        ExternalStorageGuard::ensure_no_shared_external_paths(
            target_profile_dir,
            target_zalo_data_dir,
            &running_others,
            &protected_dirs,
        )
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

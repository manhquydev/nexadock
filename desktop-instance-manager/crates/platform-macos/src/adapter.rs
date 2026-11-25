//! `MacOsPlatformAdapter` implementing `ApplicationLocator`, `PlatformCapabilities`,
//! `InstanceMonitor`, `InstanceLauncher`, and `RecoveryManager` for Apple macOS (`arm64` & `x86_64`).
//!
//! Enforces 5-tier cryptographic bundle verification (`P0-1`), 12-surface profile data isolation
//! (`P0-2`), multi-evidence `lstart`-bound PID ownership (`P0-3`), strict 8-rule graceful shutdown
//! (`P1-2`), and zero-TCC macOS 14+ cooperative window activation (`P1-3`).
//!
//! Live official VNG `Zalo.app` execution is honestly reported as [`SupportStatus::NotTested`]
//! (`LIVE_ZALO_NOT_TESTED`) until validated on physical Apple hardware.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dim_core::{
    detect_zalo_user_id, now_rfc3339_utc, AccountActivationResult, AccountKind,
    ActionableRemediation, ActivationOutcome, ApplicationInstallation, ApplicationLocator,
    DimError, InstanceLauncher, InstanceMonitor, LaunchTarget, PlatformCapabilities,
    PlatformCapabilityReport, PlatformKind, PrimaryInstanceStatus, PrimaryWindowState,
    ProfileRecord, ProfileRuntimeState, RecoveryManager, RecoveryReport, SupportStatus,
    WindowController,
};
use dim_profile_store::{
    is_symlink_or_reparse, unlink_reparse_points_safe, verify_profile_id_marker,
};

use crate::bundle::{
    discover_zalo_bundle, validate_macos_path, AppIdentityVerificationTier, DEFAULT_SYSTEM_ZALO_APP,
};
use crate::codesign::{
    enforce_signature_tier_or_err, verify_macos_bundle_identity, verify_macos_code_signature,
    CodeSignTrustStatus, PinnedPublisherPolicy,
};
use crate::layout::{
    build_isolated_child_env, build_open_launch_args, derive_fixture_user_id,
    ensure_macos_profile_layout_with_ops, ensure_macos_user_symlinks_with_options,
    ensure_not_macos_primary_path, inspect_call_socket_owners, inspect_call_sockets_at,
    macos_zalo_data_dir, normalize_macos_path_key, verify_external_db_path_isolated,
    verify_no_forbidden_isolation_symlinks_with_home, verify_opt_in_symlink_target,
    MacOsLayoutOptions, DARWIN_SUN_PATH_MAX, MACOS_12_ISOLATED_SURFACES, MACOS_CALL_SOCKETS,
    MACOS_PROFILE_META_FILE, MACOS_SAFE_TMPDIR_MAX_BYTES,
};
#[cfg(target_os = "macos")]
use crate::ops::NativeMacOsSystemOps;
#[cfg(not(target_os = "macos"))]
use crate::ops::UnsupportedHostSystemOps;
use crate::ops::{HostFixtureMacOsOps, MacOsSystemOps, SpawnDetachedRequest};
pub use crate::process::read_singleton_or_json_pid;
use crate::process::{
    compute_profile_disk_usage_bytes, inspect_primary_macos, read_runtime_launch_record,
    read_runtime_pid, recover_macos_stale_artifacts_with_ops, resolve_owned_profile_tree,
    stop_isolated_profile_tree, symlink_or_file_present, write_launch_target_hint,
    write_runtime_launch_record, ProfileOperationGuard, RuntimeLaunchRecord,
};
use crate::window::{
    activate_macos_app_by_pid, activate_profile_window, inspect_profile_window,
    signal_fixture_focus_only,
};

/// Honest machine-readable live verification marker when no physical Mac GUI was attached.
pub const LIVE_ZALO_STATUS_MARKER: &str = "LIVE_ZALO_NOT_TESTED";

/// Configuration options for [`MacOsPlatformAdapter`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacOsAdapterConfig {
    /// Enforce 5-tier `codesign --verify --deep --strict` + Gatekeeper + pinned `TeamIdentifier` on macOS (`true` in production).
    pub require_signature_trust: bool,
    /// Explicit cryptographic publisher policy (`CFBundleIdentifier` + Apple `TeamIdentifier`) for `OfficialZalo`.
    ///
    /// Defaults to `None` so publisher identity is NEVER guessed from app filenames or loose `"VNG"` substrings.
    pub pinned_publisher_policy: Option<PinnedPublisherPolicy>,
    /// Append `--user-data-dir=<zalo_data_dir>` and `--disk-cache-dir=<cache_dir>` to launch arguments.
    pub include_user_data_dir_arg: bool,
    /// Directory & opt-in symlink layout options (`Downloads`..`Music`, `Library/Keychains`; both `false` by default).
    pub layout_options: MacOsLayoutOptions,
    /// Maximum milliseconds to poll for `SingletonLock` / `lockfile` registration immediately after spawn.
    pub launch_ready_poll_ms: u64,
}

impl Default for MacOsAdapterConfig {
    fn default() -> Self {
        Self {
            require_signature_trust: true,
            pinned_publisher_policy: None,
            include_user_data_dir_arg: true,
            layout_options: MacOsLayoutOptions::default(),
            launch_ready_poll_ms: 1_500,
        }
    }
}

/// macOS platform adapter isolating profiles via `open -n -a ... --env CFFIXED_USER_HOME=<profile_dir>`
/// with hardened `/tmp/dz-<uid>/<id>` short `TMPDIR`, `lstart`-verified PID tree ownership, and primary Zalo protection.
#[derive(Debug, Clone)]
pub struct MacOsPlatformAdapter {
    ops: Arc<dyn MacOsSystemOps>,
    op_locks: Arc<Mutex<HashSet<String>>>,
    config: MacOsAdapterConfig,
}

impl Default for MacOsPlatformAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl MacOsPlatformAdapter {
    /// Creates a new macOS platform adapter backed by the host OS backend.
    #[must_use]
    pub fn new() -> Self {
        #[cfg(target_os = "macos")]
        let ops: Arc<dyn MacOsSystemOps> = Arc::new(NativeMacOsSystemOps);
        #[cfg(not(target_os = "macos"))]
        let ops: Arc<dyn MacOsSystemOps> = Arc::new(UnsupportedHostSystemOps);

        Self {
            ops,
            op_locks: Arc::new(Mutex::new(HashSet::new())),
            config: MacOsAdapterConfig::default(),
        }
    }

    /// Creates a macOS platform adapter over a custom [`MacOsSystemOps`] implementation.
    #[must_use]
    pub fn with_system_ops(ops: Arc<dyn MacOsSystemOps>) -> Self {
        Self {
            ops,
            op_locks: Arc::new(Mutex::new(HashSet::new())),
            config: MacOsAdapterConfig::default(),
        }
    }

    /// Creates a cross-platform live fixture adapter rooted at `simulated_home` and `short_tmp_root`.
    #[must_use]
    pub fn for_fixture_host(simulated_home: PathBuf, short_tmp_root: PathBuf) -> Self {
        let ops = Arc::new(HostFixtureMacOsOps::new(simulated_home, short_tmp_root));
        Self::with_system_ops(ops).with_signature_enforcement(false)
    }

    /// Configures whether unsigned, ad-hoc, unpinned, or tampered binaries are rejected with `DimError::UnverifiedBinary`.
    #[must_use]
    pub fn with_signature_enforcement(mut self, enforce: bool) -> Self {
        self.config.require_signature_trust = enforce;
        self
    }

    /// Pins a verified Apple `CFBundleIdentifier` and `TeamIdentifier` required for Tier 5 `OfficialZalo` execution.
    #[must_use]
    pub fn with_pinned_publisher(
        mut self,
        bundle_identifier: impl Into<String>,
        team_identifier: impl Into<String>,
    ) -> Self {
        self.config.pinned_publisher_policy = Some(PinnedPublisherPolicy::new(
            bundle_identifier,
            team_identifier,
        ));
        self
    }

    /// Sets or clears the explicit [`PinnedPublisherPolicy`].
    #[must_use]
    pub fn with_pinned_publisher_policy(mut self, policy: Option<PinnedPublisherPolicy>) -> Self {
        self.config.pinned_publisher_policy = policy;
        self
    }

    /// Configures opt-in user-folder and `Library/Keychains` symlink creation.
    #[must_use]
    pub fn with_layout_options(mut self, options: MacOsLayoutOptions) -> Self {
        self.config.layout_options = options;
        self
    }

    /// Configures whether `--user-data-dir=<zalo_data_dir>` and `--disk-cache-dir=<cache_dir>` are appended to launch arguments.
    #[must_use]
    pub fn with_user_data_dir_arg(mut self, enabled: bool) -> Self {
        self.config.include_user_data_dir_arg = enabled;
        self
    }

    /// Configures post-spawn readiness polling duration in milliseconds.
    #[must_use]
    pub fn with_launch_poll_ms(mut self, poll_ms: u64) -> Self {
        self.config.launch_ready_poll_ms = poll_ms;
        self
    }

    /// Borrows the active [`MacOsAdapterConfig`].
    #[must_use]
    pub const fn config(&self) -> &MacOsAdapterConfig {
        &self.config
    }

    /// Borrows the underlying [`MacOsSystemOps`] backend.
    #[must_use]
    pub fn system_ops(&self) -> &Arc<dyn MacOsSystemOps> {
        &self.ops
    }
}

impl ApplicationLocator for MacOsPlatformAdapter {
    fn discover_installation(
        &self,
        custom_root: Option<&Path>,
    ) -> Result<ApplicationInstallation, DimError> {
        let fallback_root = custom_root
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_SYSTEM_ZALO_APP));

        let Some(bundle) = discover_zalo_bundle(&*self.ops, custom_root)? else {
            return Ok(ApplicationInstallation {
                is_valid: false,
                platform: PlatformKind::MacOs,
                install_root: fallback_root.clone(),
                launcher_path: None,
                executable_path: None,
                app_asar_path: None,
                version: None,
                signature_status: "NotFound".to_owned(),
                signer_subject: None,
                support_status: SupportStatus::NotTested,
                notes: vec![format!(
                    "{LIVE_ZALO_STATUS_MARKER}: macOS official Zalo is unverified on physical Apple hardware in this repository."
                )],
                error: Some(format!(
                    "macOS Zalo.app not found at {}",
                    fallback_root.display()
                )),
            });
        };

        let mut notes = bundle.notes.clone();
        notes.push(
            "Isolated launch uses `/usr/bin/open -n -a <bundle> --env CFFIXED_USER_HOME=<profile_dir>`."
                .to_owned(),
        );
        notes.push(
            "Concurrent voice/video calls conflict on global /tmp/socketzalorecv2021.".to_owned(),
        );

        if let Some(structural_err) = bundle.validation_error {
            let exec_exists =
                fs::symlink_metadata(&bundle.executable_path).is_ok_and(|m| m.is_file());
            return Ok(ApplicationInstallation {
                is_valid: false,
                platform: PlatformKind::MacOs,
                install_root: bundle.bundle_path,
                launcher_path: Some(PathBuf::from("/usr/bin/open")),
                executable_path: exec_exists.then_some(bundle.executable_path),
                app_asar_path: bundle.app_asar_path,
                version: bundle.plist.short_version,
                signature_status: "InvalidBundle".to_owned(),
                signer_subject: None,
                support_status: SupportStatus::NotTested,
                notes,
                error: Some(structural_err),
            });
        }

        let sig_report = verify_macos_bundle_identity(
            &*self.ops,
            &bundle,
            self.config.pinned_publisher_policy.as_ref(),
        );
        if let Some(tier) = sig_report.highest_tier {
            notes.push(format!("identity_tier={}", tier.as_str()));
        }
        if sig_report.quarantine_present {
            notes.push(
                "Quarantine xattr (com.apple.quarantine) detected; macOS Gatekeeper assessment preserved."
                    .to_owned(),
            );
        }

        // ApplicationLocator separates structural discovery (`is_valid`) from launch-time Tier-5
        // enforcement (`launch` -> `DimError::UnverifiedBinary`), rejecting tampered, Gatekeeper-rejected,
        // or wrong-publisher bundles immediately while honestly reporting exact 5-tier `signature_status`.
        let hard_trust_violation = self.config.require_signature_trust
            && matches!(
                sig_report.status,
                CodeSignTrustStatus::TamperedSignature
                    | CodeSignTrustStatus::GatekeeperRejected
                    | CodeSignTrustStatus::WrongPublisher
            );
        let is_valid = !hard_trust_violation;
        let error = hard_trust_violation.then(|| {
            sig_report
                .detail
                .clone()
                .unwrap_or_else(|| sig_report.summary_label.clone())
        });

        Ok(ApplicationInstallation {
            is_valid,
            platform: PlatformKind::MacOs,
            install_root: bundle.bundle_path,
            launcher_path: Some(PathBuf::from("/usr/bin/open")),
            executable_path: Some(bundle.executable_path),
            app_asar_path: bundle.app_asar_path,
            version: bundle.plist.short_version,
            signature_status: sig_report.summary_label,
            signer_subject: sig_report.signer_subject,
            support_status: SupportStatus::NotTested,
            notes,
            error,
        })
    }
}

impl PlatformCapabilities for MacOsPlatformAdapter {
    fn platform(&self) -> PlatformKind {
        PlatformKind::MacOs
    }

    fn capabilities(&self) -> PlatformCapabilityReport {
        let socket_paths: Vec<&Path> = MACOS_CALL_SOCKETS.iter().map(Path::new).collect();
        let mut diagnostics = vec![
            format!("host_arch={}", self.ops.host_arch()),
            format!("live_zalo_status={LIVE_ZALO_STATUS_MARKER}"),
            format!(
                "socket_path_budget: DARWIN_SUN_PATH_MAX={DARWIN_SUN_PATH_MAX}, safe_tmpdir_max_bytes={MACOS_SAFE_TMPDIR_MAX_BYTES}"
            ),
            "Singleton isolation uses `open -n` + `CFFIXED_USER_HOME` + `HOME` + hardened `/tmp/dz-<uid>/<id>` `TMPDIR`."
                .to_owned(),
            "Process ownership requires `.zalo-profile-id` + `.runtime-launch.json` + `ps -o lstart=` start token + argv lineage."
                .to_owned(),
            "Dock unread badges (`app.dock.setBadge`) are independent per `open -n` instance (`F-MAC-04`)."
                .to_owned(),
        ];
        diagnostics.extend(inspect_call_sockets_at(&socket_paths));
        diagnostics.extend(inspect_call_socket_owners(&*self.ops, &socket_paths));

        let fixture_status = if self.ops.supports_runtime_ops() {
            SupportStatus::SupportedAndTested
        } else {
            SupportStatus::Experimental
        };

        PlatformCapabilityReport {
            platform: PlatformKind::MacOs,
            overall_status: SupportStatus::NotTested,
            official_zalo_status: SupportStatus::NotTested,
            fixture_status,
            appimage_status: None,
            concurrent_calls_status: SupportStatus::Blocked,
            folder_redirection_detected: false,
            folder_redirection_path: None,
            limitations: vec![
                format!(
                    "{LIVE_ZALO_STATUS_MARKER}: macOS official Zalo.app CFFIXED_USER_HOME isolation has not been executed on physical macOS hardware in this workspace."
                ),
                "ZaloHelper.app/Contents/MacOS/ZaloCall binds hardcoded global /tmp/socketzalorecv2021 & /tmp/socketzalosend2021 (last-writer-wins fs.unlink conflict on concurrent calls; stop & relaunch target profile last to rebind call audio)."
                    .to_owned(),
                "macOS UNUserNotificationCenter (`usernoted`) and `zalo://` URL handlers bind to shared CFBundleIdentifier (com.vng.zalo), whereas Dock unread badges (`app.dock.setBadge`) remain per-instance."
                    .to_owned(),
                "macOS 14+ (Sonoma/Sequoia) NSRunningApplication.activate() is cooperative and fails closed if frontmost focus is declined."
                    .to_owned(),
            ],
            diagnostics,
        }
    }

    fn free_disk_bytes(&self, path: &Path) -> Result<u64, DimError> {
        self.ops.free_disk_bytes(path)
    }

    fn unlink_profile_junctions(&self, profile_dir: &Path) -> Result<usize, DimError> {
        unlink_reparse_points_safe(profile_dir)
    }

    fn is_elevated(&self) -> Option<bool> {
        self.ops.is_elevated()
    }

    fn check_profiles_acl_safe(&self, profiles_dir: &Path) -> Option<bool> {
        self.ops.check_dir_permissions_restrictive(profiles_dir)
    }
}

impl InstanceMonitor for MacOsPlatformAdapter {
    fn inspect_primary(&self) -> Result<PrimaryInstanceStatus, DimError> {
        inspect_primary_macos(&*self.ops)
    }

    fn inspect_profile(
        &self,
        profile: &ProfileRecord,
        profile_dir: &Path,
    ) -> Result<ProfileRuntimeState, DimError> {
        self.inspect_profile_with_options(profile, profile_dir, true)
    }

    fn inspect_profile_with_options(
        &self,
        profile: &ProfileRecord,
        profile_dir: &Path,
        include_size: bool,
    ) -> Result<ProfileRuntimeState, DimError> {
        validate_macos_path(profile_dir)?;
        let real_home = self.ops.user_home_dir();
        ensure_not_macos_primary_path(profile_dir, real_home.as_deref())?;

        if !self.ops.supports_runtime_ops() {
            return Err(DimError::UnsupportedPlatform(
                "macOS runtime inspection requires macOS".to_owned(),
            ));
        }

        let zalo_data = macos_zalo_data_dir(profile_dir);
        let primary_pids: HashSet<u32> = inspect_primary_macos(&*self.ops)
            .map(|p| p.protected_pids.into_iter().collect())
            .unwrap_or_default();

        // Read `<profile_dir>/.runtime-pid` alongside `.runtime-launch.json` so newly spawned
        // processes are owned immediately before `SingletonLock` symlink creation (`DEFECT-03`).
        let runtime_pid = read_runtime_pid(profile_dir);
        let tree = resolve_owned_profile_tree(
            &*self.ops,
            &profile.id,
            profile_dir,
            runtime_pid,
            &primary_pids,
        )?;
        let running = tree.main_pid.is_some();
        let win = inspect_profile_window(&*self.ops, &zalo_data, &tree.process_ids);
        let detected_uid = detect_zalo_user_id(&zalo_data);
        let disk_usage_bytes = if include_size {
            compute_profile_disk_usage_bytes(profile_dir)
        } else {
            0
        };

        let has_lock_artifact = fs::symlink_metadata(zalo_data.join("SingletonLock")).is_ok()
            || fs::symlink_metadata(zalo_data.join("lockfile")).is_ok();
        let status_label = if running {
            if has_lock_artifact {
                "Running".to_owned()
            } else {
                "Starting".to_owned()
            }
        } else if tree.stale_lock_detected {
            "Stopped (Stale Lock)".to_owned()
        } else {
            "Stopped".to_owned()
        };

        Ok(ProfileRuntimeState {
            profile_id: profile.id.clone(),
            profile_name: profile.name.clone(),
            profile_dir: profile_dir.to_path_buf(),
            zalo_data_dir: zalo_data,
            lockfile_path: tree.lockfile_path,
            running,
            stale_lock_detected: tree.stale_lock_detected,
            main_pid: tree.main_pid,
            process_count: tree.process_ids.len(),
            process_ids: tree.process_ids,
            working_set_bytes: tree.working_set_bytes,
            window_handle: win.as_ref().map(|w| w.window_id),
            window_title: win.and_then(|w| w.title),
            detected_zalo_user_id: detected_uid,
            disk_usage_bytes,
            status_label,
        })
    }
}

impl InstanceLauncher for MacOsPlatformAdapter {
    fn ensure_profile_layout(
        &self,
        profile: &ProfileRecord,
        profile_dir: &Path,
    ) -> Result<(), DimError> {
        let _ = ensure_macos_profile_layout_with_ops(
            &*self.ops,
            profile,
            profile_dir,
            self.config.layout_options,
        )?;
        if let Some(real_home) = self.ops.user_home_dir() {
            ensure_macos_user_symlinks_with_options(
                profile_dir,
                &real_home,
                self.config.layout_options,
            )?;
        } else {
            verify_no_forbidden_isolation_symlinks_with_home(
                profile_dir,
                self.config.layout_options,
                None,
            )?;
        }
        Ok(())
    }

    fn launch(
        &self,
        target: &LaunchTarget,
        profile: &ProfileRecord,
        profile_dir: &Path,
    ) -> Result<ProfileRuntimeState, DimError> {
        validate_macos_path(profile_dir)?;
        let real_home = self.ops.user_home_dir();
        ensure_not_macos_primary_path(profile_dir, real_home.as_deref())?;

        if !self.ops.supports_runtime_ops() {
            return Err(DimError::UnsupportedPlatform(
                "macOS launch requires macOS".to_owned(),
            ));
        }

        let resolved_layout = ensure_macos_profile_layout_with_ops(
            &*self.ops,
            profile,
            profile_dir,
            self.config.layout_options,
        )?;
        if let Some(ref home) = real_home {
            ensure_macos_user_symlinks_with_options(profile_dir, home, self.config.layout_options)?;
        } else {
            verify_no_forbidden_isolation_symlinks_with_home(
                profile_dir,
                self.config.layout_options,
                None,
            )?;
        }

        let _op_guard = ProfileOperationGuard::acquire(
            &*self.ops,
            &self.op_locks,
            &profile.id,
            Some(profile_dir),
        )?;

        verify_profile_id_marker(&resolved_layout.zalo_data_dir, &profile.id, true)?;
        verify_external_db_path_isolated(
            &resolved_layout.zalo_data_dir,
            profile_dir,
            real_home.as_deref(),
        )?;

        // Idempotency: if already running, signal or cooperatively focus existing session.
        let before = self.inspect_profile(profile, profile_dir)?;
        if before.running {
            if let Some(pid) = before.main_pid {
                if matches!(target, LaunchTarget::NodeFixture { .. }) {
                    let _ = signal_fixture_focus_only(&before.zalo_data_dir, pid);
                } else {
                    let _ = activate_profile_window(&*self.ops, &before.zalo_data_dir, pid);
                }
            }
            return self.inspect_profile(profile, profile_dir);
        }

        // Automatically recover stale crash artifacts (`SingletonLock`, `SingletonSocket`, `lockfile`, short `TMPDIR`) before launch.
        if before.stale_lock_detected {
            let _ =
                recover_macos_stale_artifacts_with_ops(&*self.ops, profile, profile_dir, false)?;
        }

        let launched_exec_label: String;
        match target {
            LaunchTarget::OfficialZalo {
                custom_install_root,
            } => {
                let fallback_root = custom_install_root
                    .as_deref()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from(DEFAULT_SYSTEM_ZALO_APP));
                let Some(bundle) =
                    discover_zalo_bundle(&*self.ops, custom_install_root.as_deref())?
                else {
                    return Err(DimError::InstallNotFound(format!(
                        "macOS Zalo.app not found at {}",
                        fallback_root.display()
                    )));
                };
                if let Some(structural_err) = bundle.validation_error {
                    return Err(DimError::InstallNotFound(structural_err));
                }
                if (self.config.require_signature_trust || self.ops.is_native_macos())
                    && bundle.structural_tier < Some(AppIdentityVerificationTier::ValidMachO)
                {
                    return Err(DimError::UnverifiedBinary(format!(
                        "refusing to launch OfficialZalo without a valid Mach-O executable at {}",
                        bundle.executable_path.display()
                    )));
                }
                let sig_report = verify_macos_bundle_identity(
                    &*self.ops,
                    &bundle,
                    self.config.pinned_publisher_policy.as_ref(),
                );
                enforce_signature_tier_or_err(
                    &sig_report,
                    self.config.require_signature_trust,
                    true,
                )?;

                launched_exec_label = bundle.bundle_path.to_string_lossy().into_owned();
                write_launch_target_hint(profile_dir, &profile.id, &bundle.bundle_path);
                let open_args = build_open_launch_args(
                    &bundle.bundle_path,
                    &profile.id,
                    profile_dir,
                    &resolved_layout.effective_tmpdir,
                    self.config.include_user_data_dir_arg,
                    &[],
                );
                let out = self
                    .ops
                    .run_command(Path::new("/usr/bin/open"), &open_args, &[])?;
                if !out.success() {
                    return Err(DimError::ProcessError(format!(
                        "`/usr/bin/open -n` exited with status {:?}: {}",
                        out.status_code,
                        out.stderr_lossy().trim()
                    )));
                }
            }
            LaunchTarget::CustomExecutable { path, args } => {
                validate_macos_path(path)?;
                if !path.is_file() {
                    return Err(DimError::InstallNotFound(format!(
                        "custom executable not found: {}",
                        path.display()
                    )));
                }
                if self.config.require_signature_trust && self.ops.is_native_macos() {
                    let sig = verify_macos_code_signature(&*self.ops, path, None);
                    enforce_signature_tier_or_err(&sig, true, false)?;
                }

                launched_exec_label = path.to_string_lossy().into_owned();
                write_launch_target_hint(profile_dir, &profile.id, path);
                let cache_dir = profile_dir
                    .join("Library")
                    .join("Caches")
                    .join("com.vng.zalo");
                let mut full_args = args.clone();
                if self.config.include_user_data_dir_arg {
                    if !full_args.iter().any(|a| a.starts_with("--user-data-dir")) {
                        full_args.push(format!(
                            "--user-data-dir={}",
                            resolved_layout.zalo_data_dir.display()
                        ));
                    }
                    if !full_args.iter().any(|a| a.starts_with("--disk-cache-dir")) {
                        full_args.push(format!("--disk-cache-dir={}", cache_dir.display()));
                    }
                }
                let envs = build_isolated_child_env(
                    &profile.id,
                    profile_dir,
                    &resolved_layout.effective_tmpdir,
                );
                let pid = self.ops.spawn_detached(&SpawnDetachedRequest {
                    program: path.clone(),
                    args: full_args,
                    envs,
                    current_dir: path.parent().map(Path::to_path_buf),
                })?;
                let start_token = self.ops.process_start_token(pid);
                write_runtime_launch_record(
                    profile_dir,
                    &RuntimeLaunchRecord {
                        schema_version: 1,
                        pid,
                        process_start_token: start_token,
                        profile_id: profile.id.clone(),
                        profile_dir: profile_dir.to_path_buf(),
                        zalo_data_dir: resolved_layout.zalo_data_dir.clone(),
                        executable_path: launched_exec_label.clone(),
                        launched_at_utc: now_rfc3339_utc(),
                    },
                )?;
            }
            LaunchTarget::NodeFixture {
                node_path,
                script_path,
                extra_args,
            } => {
                validate_macos_path(script_path)?;
                if !script_path.is_file() {
                    return Err(DimError::InstallNotFound(format!(
                        "fixture script not found: {}",
                        script_path.display()
                    )));
                }
                let node_bin = node_path
                    .as_deref()
                    .unwrap_or_else(|| Path::new("node"))
                    .to_path_buf();

                launched_exec_label = script_path.to_string_lossy().into_owned();
                write_launch_target_hint(profile_dir, &profile.id, script_path);
                let cache_dir = profile_dir
                    .join("Library")
                    .join("Caches")
                    .join("com.vng.zalo");
                let mut full_args = vec![script_path.to_string_lossy().into_owned()];
                if self.config.include_user_data_dir_arg {
                    if !extra_args.iter().any(|a| a.starts_with("--user-data-dir")) {
                        full_args.push(format!(
                            "--user-data-dir={}",
                            resolved_layout.zalo_data_dir.display()
                        ));
                    }
                    if !extra_args.iter().any(|a| a.starts_with("--disk-cache-dir")) {
                        full_args.push(format!("--disk-cache-dir={}", cache_dir.display()));
                    }
                }
                // Deterministic per-profile `--user-id=84xxxxxxx` when caller passed none (`DEFECT-02` / `XP-MAC-03`).
                if !extra_args
                    .iter()
                    .any(|a| a == "--user-id" || a.starts_with("--user-id="))
                {
                    full_args.push(format!("--user-id={}", derive_fixture_user_id(&profile.id)));
                }
                full_args.extend(extra_args.iter().cloned());

                let envs = build_isolated_child_env(
                    &profile.id,
                    profile_dir,
                    &resolved_layout.effective_tmpdir,
                );
                let pid = self.ops.spawn_detached(&SpawnDetachedRequest {
                    program: node_bin,
                    args: full_args,
                    envs,
                    current_dir: script_path.parent().map(Path::to_path_buf),
                })?;
                let start_token = self.ops.process_start_token(pid);
                write_runtime_launch_record(
                    profile_dir,
                    &RuntimeLaunchRecord {
                        schema_version: 1,
                        pid,
                        process_start_token: start_token,
                        profile_id: profile.id.clone(),
                        profile_dir: profile_dir.to_path_buf(),
                        zalo_data_dir: resolved_layout.zalo_data_dir.clone(),
                        executable_path: launched_exec_label.clone(),
                        launched_at_utc: now_rfc3339_utc(),
                    },
                )?;
            }
        }

        // Poll briefly so `SingletonLock` / `lockfile` registration is reflected immediately when fast,
        // while slow-starting instances (`DEFECT-03` / `LC-07`) still return `running = true` via `.runtime-launch.json`.
        let is_node_fixture = matches!(target, LaunchTarget::NodeFixture { .. });
        let is_official_open = matches!(target, LaunchTarget::OfficialZalo { .. });
        let deadline = Instant::now() + Duration::from_millis(self.config.launch_ready_poll_ms);
        loop {
            let state = self.inspect_profile(profile, profile_dir)?;
            if state.running {
                if is_official_open && read_runtime_launch_record(profile_dir).is_none() {
                    if let Some(pid) = state.main_pid {
                        let start_token = self.ops.process_start_token(pid);
                        let _ = write_runtime_launch_record(
                            profile_dir,
                            &RuntimeLaunchRecord {
                                schema_version: 1,
                                pid,
                                process_start_token: start_token,
                                profile_id: profile.id.clone(),
                                profile_dir: profile_dir.to_path_buf(),
                                zalo_data_dir: resolved_layout.zalo_data_dir.clone(),
                                executable_path: launched_exec_label.clone(),
                                launched_at_utc: now_rfc3339_utc(),
                            },
                        );
                    }
                }
                let lock_ready = if is_node_fixture {
                    symlink_or_file_present(&state.zalo_data_dir.join("SingletonLock"))
                } else {
                    symlink_or_file_present(&state.lockfile_path)
                };
                if lock_ready || Instant::now() >= deadline {
                    return Ok(state);
                }
            } else if Instant::now() >= deadline {
                return Ok(state);
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    fn focus_or_restore(
        &self,
        target: &LaunchTarget,
        profile: &ProfileRecord,
        profile_dir: &Path,
        state: &ProfileRuntimeState,
    ) -> Result<(), DimError> {
        if !self.ops.supports_runtime_ops() {
            return Err(DimError::UnsupportedPlatform(
                "macOS focus_or_restore requires macOS".to_owned(),
            ));
        }

        let _op_guard = ProfileOperationGuard::acquire(
            &*self.ops,
            &self.op_locks,
            &profile.id,
            Some(profile_dir),
        )?;

        if !state.running {
            return Err(DimError::NotRunning(profile.name.clone()));
        }
        let Some(pid) = state.main_pid else {
            return Err(DimError::NotRunning(profile.name.clone()));
        };

        // Re-verify live multi-evidence ownership BEFORE touching `.focus-signal` or invoking `osascript` (`P1-3`).
        let live = self.inspect_profile(profile, profile_dir)?;
        if !live.running {
            if self.ops.is_pid_alive(pid) {
                return Err(DimError::ProcessError(format!(
                    "refusing focus_or_restore for profile '{}': candidate PID {pid} is alive in OS table but failed profile ownership verification",
                    profile.id
                )));
            }
            return Err(DimError::NotRunning(profile.name.clone()));
        }
        let Some(verified_pid) = live.main_pid else {
            return Err(DimError::NotRunning(profile.name.clone()));
        };
        if verified_pid != pid {
            return Err(DimError::ProcessError(format!(
                "refusing focus_or_restore for profile '{}': state PID {pid} no longer matches live verified PID {verified_pid}",
                profile.id
            )));
        }

        match target {
            LaunchTarget::NodeFixture { .. } => {
                signal_fixture_focus_only(&live.zalo_data_dir, verified_pid)
            }
            LaunchTarget::OfficialZalo { .. } | LaunchTarget::CustomExecutable { .. } => {
                let _ = activate_profile_window(&*self.ops, &live.zalo_data_dir, verified_pid)?;
                Ok(())
            }
        }
    }

    fn stop(
        &self,
        state: &ProfileRuntimeState,
        protected_primary_pids: &[u32],
        timeout_ms: u64,
    ) -> Result<u32, DimError> {
        if let Some(pid) = state.main_pid {
            if protected_primary_pids.contains(&pid) {
                return Err(DimError::ProtectedPrimaryProcess(format!(
                    "refused to stop protected macOS primary PID {pid}"
                )));
            }
        }
        for &pid in &state.process_ids {
            if protected_primary_pids.contains(&pid) {
                return Err(DimError::ProtectedPrimaryProcess(format!(
                    "refused to stop protected macOS primary child PID {pid}"
                )));
            }
        }

        if state.main_pid.is_none() && !state.running {
            return Ok(0);
        }

        if !self.ops.supports_runtime_ops() {
            return Err(DimError::UnsupportedPlatform(
                "macOS stop requires macOS".to_owned(),
            ));
        }

        let _op_guard = ProfileOperationGuard::acquire(
            &*self.ops,
            &self.op_locks,
            &state.profile_id,
            Some(&state.profile_dir),
        )?;

        stop_isolated_profile_tree(
            &*self.ops,
            &state.profile_id,
            &state.profile_dir,
            &state.zalo_data_dir,
            state.main_pid,
            &state.process_ids,
            protected_primary_pids,
            timeout_ms,
        )
    }
}

impl RecoveryManager for MacOsPlatformAdapter {
    fn recover_stale_state(
        &self,
        profile: &ProfileRecord,
        profile_dir: &Path,
        state: &ProfileRuntimeState,
    ) -> Result<RecoveryReport, DimError> {
        recover_macos_stale_artifacts_with_ops(&*self.ops, profile, profile_dir, state.running)
    }
}

/// Audits runtime file isolation across all 12 macOS profile storage surfaces (`MACOS_12_ISOLATED_SURFACES`)
/// for two or more profiles, verifying actual runtime-written payloads, distinct `effective_tmpdir` and
/// `chromium_cache` roots, `0o700` restrictive permissions, zero cross-profile symlink sharing, and
/// untouched primary `~/Library/Application Support/ZaloData` (`P0-2`).
pub fn verify_12_surface_runtime_isolation(
    ops: &dyn MacOsSystemOps,
    profiles: &[(String, PathBuf)],
    real_home: Option<&Path>,
) -> Result<serde_json::Value, DimError> {
    if profiles.len() < 2 {
        return Err(DimError::ProcessError(
            "verify_12_surface_runtime_isolation requires at least 2 profiles".to_owned(),
        ));
    }

    let mut seen_tmpdirs: HashSet<String> = HashSet::new();
    let mut seen_caches: HashSet<String> = HashSet::new();
    let mut seen_surface_paths: HashSet<String> = HashSet::new();
    let mut per_profile_reports: Vec<serde_json::Value> = Vec::new();

    for (profile_id, pdir) in profiles {
        ensure_not_macos_primary_path(pdir, real_home)?;
        if ops.check_dir_permissions_restrictive(pdir) != Some(true) {
            return Err(DimError::ProcessError(format!(
                "profile directory '{}' lacks restrictive 0o700 permissions",
                pdir.display()
            )));
        }

        let meta_path = pdir.join(MACOS_PROFILE_META_FILE);
        let meta_raw = fs::read_to_string(&meta_path)?;
        let meta_json: serde_json::Value = serde_json::from_str(&meta_raw)?;
        let symlink_keychain = meta_json
            .get("symlink_keychain")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let eff_tmp = meta_json
            .get("effective_tmpdir")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let eff_tmp_key = normalize_macos_path_key(Path::new(&eff_tmp));
        if eff_tmp_key.is_empty() || !seen_tmpdirs.insert(eff_tmp_key) {
            return Err(DimError::ProcessError(format!(
                "duplicate or empty effective_tmpdir '{eff_tmp}' for profile '{profile_id}'"
            )));
        }

        let cache_dir = pdir.join("Library").join("Caches").join("com.vng.zalo");
        let cache_key = normalize_macos_path_key(&cache_dir);
        if !seen_caches.insert(cache_key) {
            return Err(DimError::ProcessError(format!(
                "duplicate chromium cache path '{}' for profile '{profile_id}'",
                cache_dir.display()
            )));
        }

        let mut surface_counts: BTreeMap<String, usize> = BTreeMap::new();
        for &(surface_name, rel_path) in MACOS_12_ISOLATED_SURFACES {
            let surface_dir = if surface_name == "temporary_files" && !eff_tmp.is_empty() {
                PathBuf::from(&eff_tmp)
            } else {
                pdir.join(rel_path)
            };
            let meta = fs::symlink_metadata(&surface_dir).map_err(|e| {
                DimError::ProcessError(format!(
                    "missing isolated surface '{surface_name}' at '{}': {e}",
                    surface_dir.display()
                ))
            })?;
            if is_symlink_or_reparse(&meta) {
                if surface_name == "keychain_dir" && symlink_keychain {
                    verify_opt_in_symlink_target(&surface_dir, rel_path, real_home)?;
                    surface_counts.insert(surface_name.to_owned(), 0);
                    continue;
                }
                return Err(DimError::PathTraversal(format!(
                    "unintended symlink detected on isolated surface '{surface_name}' at '{}'",
                    surface_dir.display()
                )));
            }
            let surf_key = format!("{surface_name}:{}", normalize_macos_path_key(&surface_dir));
            if !seen_surface_paths.insert(surf_key) {
                return Err(DimError::ProcessError(format!(
                    "cross-profile surface collision on '{surface_name}' at '{}'",
                    surface_dir.display()
                )));
            }
            // Keychain surface (`Library/Keychains`) stays an isolated 0o700 directory without writing fake Keychain DBs;
            // the other 11 surfaces must contain real runtime-written files proving non-vacuous isolation!
            let files_found = count_regular_files_shallow_or_nested(&surface_dir, 3);
            if surface_name != "keychain_dir" && files_found == 0 {
                return Err(DimError::ProcessError(format!(
                    "vacuous isolation rejected: surface '{surface_name}' at '{}' has 0 runtime files",
                    surface_dir.display()
                )));
            }
            surface_counts.insert(surface_name.to_owned(), files_found);
        }

        per_profile_reports.push(serde_json::json!({
            "profile_id": profile_id,
            "profile_dir": pdir.display().to_string(),
            "effective_tmpdir": eff_tmp,
            "restrictive_0700": true,
            "surface_file_counts": surface_counts,
        }));
    }

    let primary_untouched = if let Some(home) = real_home {
        let primary_zalo = macos_zalo_data_dir(home);
        !profiles.iter().any(|(pid, _)| {
            let marker = primary_zalo.join(".zalo-profile-id");
            fs::read_to_string(marker)
                .map(|v| v.trim() == pid)
                .unwrap_or(false)
        })
    } else {
        true
    };

    Ok(serde_json::json!({
        "verified_surfaces_count": MACOS_12_ISOLATED_SURFACES.len(),
        "profiles_verified": per_profile_reports,
        "distinct_tmpdirs": seen_tmpdirs.len(),
        "distinct_caches": seen_caches.len(),
        "primary_zalo_data_untouched": primary_untouched,
    }))
}

fn count_regular_files_shallow_or_nested(root: &Path, max_depth: usize) -> usize {
    if max_depth == 0 {
        return 0;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return 0;
    };
    let mut count = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if is_symlink_or_reparse(&meta) || meta.is_file() {
            count += 1;
        } else if meta.is_dir() && max_depth > 1 {
            count += count_regular_files_shallow_or_nested(&path, max_depth - 1);
        }
    }
    count
}

impl WindowController for MacOsPlatformAdapter {
    fn focus_primary_window(
        &self,
        _primary: &PrimaryInstanceStatus,
    ) -> Result<AccountActivationResult, DimError> {
        let live = inspect_primary_macos(&*self.ops)?;
        if !live.running || live.protected_pids.is_empty() {
            return Ok(AccountActivationResult {
                account_id: "primary".to_owned(),
                account_kind: AccountKind::DesktopPrimary,
                outcome: ActivationOutcome::ManualActionRequired,
                method: Some("read_only_inspect".to_owned()),
                os_foreground_confirmed: Some(false),
                window_handle: None,
                window_title: None,
                message: "Primary Zalo is not currently running.".to_owned(),
                remediation: Some(ActionableRemediation {
                    code: "PRIMARY_NOT_RUNNING".to_owned(),
                    message: "Primary Zalo is not running. Launch Zalo.app manually when needed."
                        .to_owned(),
                    suggested_action: Some("launch_primary_manually".to_owned()),
                }),
            });
        }

        if live.window_state == PrimaryWindowState::Visible {
            if let Some(&pid) = live.protected_pids.first() {
                if let Ok(outcome) = activate_macos_app_by_pid(&*self.ops, pid) {
                    return Ok(AccountActivationResult {
                        account_id: "primary".to_owned(),
                        account_kind: AccountKind::DesktopPrimary,
                        outcome: ActivationOutcome::Focused,
                        method: Some("cooperative_activate".to_owned()),
                        os_foreground_confirmed: Some(outcome.is_active),
                        window_handle: live.window_handle,
                        window_title: live.window_title.clone(),
                        message: "Focused primary Zalo window (read-only).".to_owned(),
                        remediation: None,
                    });
                }
            }
        }

        Ok(AccountActivationResult {
            account_id: "primary".to_owned(),
            account_kind: AccountKind::DesktopPrimary,
            outcome: ActivationOutcome::ManualActionRequired,
            method: Some("read_only_inspect".to_owned()),
            os_foreground_confirmed: Some(false),
            window_handle: live.window_handle,
            window_title: live.window_title,
            message: "Primary Zalo window could not be brought to front automatically.".to_owned(),
            remediation: Some(ActionableRemediation {
                code: "PRIMARY_HIDDEN_IN_TRAY".to_owned(),
                message: "Click the Zalo icon in the macOS Dock or menu bar to restore it."
                    .to_owned(),
                suggested_action: Some("click_primary_dock_icon".to_owned()),
            }),
        })
    }
}

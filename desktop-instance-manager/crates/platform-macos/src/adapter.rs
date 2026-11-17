//! `MacOsPlatformAdapter` implementing `ApplicationLocator`, `PlatformCapabilities`,
//! `InstanceMonitor`, `InstanceLauncher`, and `RecoveryManager` for Apple macOS (`arm64` & `x86_64`).
//!
//! Live official VNG `Zalo.app` execution is honestly reported as [`SupportStatus::NotTested`]
//! (`LIVE_ZALO_NOT_TESTED`) until validated on physical Apple hardware.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dim_core::{
    detect_zalo_user_id, ApplicationInstallation, ApplicationLocator, DimError, InstanceLauncher,
    InstanceMonitor, LaunchTarget, PlatformCapabilities, PlatformCapabilityReport, PlatformKind,
    PrimaryInstanceStatus, ProfileRecord, ProfileRuntimeState, RecoveryManager, RecoveryReport,
    SupportStatus,
};
use dim_profile_store::{unlink_reparse_points_safe, verify_profile_id_marker};

use crate::bundle::{discover_zalo_bundle, validate_macos_path, DEFAULT_SYSTEM_ZALO_APP};
use crate::codesign::{enforce_signature_or_err, verify_macos_code_signature};
use crate::layout::{
    build_isolated_child_env, build_open_launch_args, ensure_macos_profile_layout_with_ops,
    ensure_macos_user_symlinks_with_options, ensure_not_macos_primary_path,
    inspect_call_sockets_at, macos_zalo_data_dir, MacOsLayoutOptions, MACOS_CALL_SOCKETS,
    MACOS_SAFE_TMPDIR_MAX_BYTES, MACOS_SUN_PATH_MAX,
};
#[cfg(target_os = "macos")]
use crate::ops::NativeMacOsSystemOps;
#[cfg(not(target_os = "macos"))]
use crate::ops::UnsupportedHostSystemOps;
use crate::ops::{HostFixtureMacOsOps, MacOsSystemOps, SpawnDetachedRequest};
pub use crate::process::read_singleton_or_json_pid;
use crate::process::{
    compute_profile_disk_usage_bytes, inspect_primary_macos, recover_macos_stale_artifacts,
    resolve_owned_profile_tree, stop_isolated_profile_tree, write_launch_target_hint,
    ProfileOperationGuard,
};
use crate::window::{activate_profile_window, inspect_profile_window};

/// Honest machine-readable live verification marker when no physical Mac GUI was attached.
pub const LIVE_ZALO_STATUS_MARKER: &str = "LIVE_ZALO_NOT_TESTED";

/// Configuration options for [`MacOsPlatformAdapter`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacOsAdapterConfig {
    /// Enforce `codesign --verify --deep --strict` + Developer ID check on macOS (`true` in production).
    pub require_signature_trust: bool,
    /// Expected publisher substring in `codesign -dv` `Authority=` chain for `OfficialZalo`.
    pub expected_official_publisher: Option<String>,
    /// Append `--user-data-dir=<zalo_data_dir>` to launch arguments for process-tree ownership tagging.
    pub include_user_data_dir_arg: bool,
    /// Directory & symlink layout options (`Downloads`..`Music`, `Library/Keychains`).
    pub layout_options: MacOsLayoutOptions,
    /// Maximum milliseconds to poll for `SingletonLock` / `lockfile` acquisition immediately after spawn.
    pub launch_ready_poll_ms: u64,
}

impl Default for MacOsAdapterConfig {
    fn default() -> Self {
        Self {
            require_signature_trust: true,
            expected_official_publisher: Some("VNG".to_owned()),
            include_user_data_dir_arg: true,
            layout_options: MacOsLayoutOptions::default(),
            launch_ready_poll_ms: 800,
        }
    }
}

/// macOS platform adapter isolating profiles via `open -n -a ... --env CFFIXED_USER_HOME=<profile_dir>`
/// with `SUN_LEN`-safe `TMPDIR`, `ps`-verified PID tree ownership, and primary Zalo protection.
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

    /// Configures whether unsigned, ad-hoc, or tampered binaries are rejected with `DimError::UnverifiedBinary`.
    #[must_use]
    pub fn with_signature_enforcement(mut self, enforce: bool) -> Self {
        self.config.require_signature_trust = enforce;
        self
    }

    /// Configures the expected `Authority=` publisher substring for `OfficialZalo` bundles.
    #[must_use]
    pub fn with_expected_publisher(mut self, publisher: Option<String>) -> Self {
        self.config.expected_official_publisher = publisher;
        self
    }

    /// Configures isolated user-folder and `Library/Keychains` symlink creation.
    #[must_use]
    pub fn with_layout_options(mut self, options: MacOsLayoutOptions) -> Self {
        self.config.layout_options = options;
        self
    }

    /// Configures whether `--user-data-dir=<zalo_data_dir>` is appended to launch arguments.
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
                notes: vec![
                    format!(
                        "{LIVE_ZALO_STATUS_MARKER}: macOS official Zalo is unverified on physical Apple hardware in this repository."
                    ),
                ],
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
            return Ok(ApplicationInstallation {
                is_valid: false,
                platform: PlatformKind::MacOs,
                install_root: bundle.bundle_path,
                launcher_path: Some(PathBuf::from("/usr/bin/open")),
                executable_path: bundle
                    .executable_path
                    .is_file()
                    .then_some(bundle.executable_path),
                app_asar_path: bundle.app_asar_path,
                version: bundle.plist.short_version,
                signature_status: "InvalidBundle".to_owned(),
                signer_subject: None,
                support_status: SupportStatus::NotTested,
                notes,
                error: Some(structural_err),
            });
        }

        let sig_report = verify_macos_code_signature(
            &*self.ops,
            &bundle.bundle_path,
            self.config.expected_official_publisher.as_deref(),
        );
        if sig_report.quarantine_present {
            notes.push(
                "Quarantine xattr (com.apple.quarantine) detected; macOS Gatekeeper assessment applies."
                    .to_owned(),
            );
        }

        let sig_check = enforce_signature_or_err(&sig_report, self.config.require_signature_trust);
        let is_valid = sig_check.is_ok();
        let error = sig_check.err().map(|e| e.to_string());

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
                "socket_path_budget: SUN_LEN={MACOS_SUN_PATH_MAX}, safe_tmpdir_max_bytes={MACOS_SAFE_TMPDIR_MAX_BYTES}"
            ),
            "Singleton isolation uses `open -n` + `CFFIXED_USER_HOME` + `HOME` + `SUN_LEN`-safe `TMPDIR`."
                .to_owned(),
            "Chromium lock parsed from `SingletonLock` symlink (`<host>-<pid>`) and cross-checked via `ps` UID/args."
                .to_owned(),
        ];
        diagnostics.extend(inspect_call_sockets_at(&socket_paths));

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
                "ZaloHelper.app/Contents/MacOS/ZaloCall binds hardcoded global /tmp/socketzalorecv2021 (fs.unlink conflict on concurrent calls)."
                    .to_owned(),
                "macOS UNUserNotificationCenter and zalo:// URL handlers bind to shared CFBundleIdentifier (com.vng.zalo)."
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

        let tree = resolve_owned_profile_tree(&*self.ops, profile_dir, &primary_pids)?;
        let running = tree.main_pid.is_some();
        let win = inspect_profile_window(&*self.ops, &zalo_data, &tree.process_ids);
        let detected_uid = detect_zalo_user_id(&zalo_data);
        let disk_usage_bytes = compute_profile_disk_usage_bytes(profile_dir);

        let status_label = if running {
            "Running".to_owned()
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
        }

        let _op_guard = ProfileOperationGuard::acquire(
            &*self.ops,
            &self.op_locks,
            &profile.id,
            Some(profile_dir),
        )?;

        verify_profile_id_marker(&resolved_layout.zalo_data_dir, &profile.id, true)?;

        // Idempotency: if already running, focus existing session without spawning a duplicate.
        let before = self.inspect_profile(profile, profile_dir)?;
        if before.running {
            if let Some(pid) = before.main_pid {
                let _ = activate_profile_window(&*self.ops, &before.zalo_data_dir, pid);
            }
            return self.inspect_profile(profile, profile_dir);
        }

        // Automatically recover stale crash artifacts (`SingletonLock`, `SingletonSocket`, `lockfile`) before launch.
        if before.stale_lock_detected {
            let _ = recover_macos_stale_artifacts(profile, profile_dir, false)?;
        }

        match target {
            LaunchTarget::OfficialZalo {
                custom_install_root,
            } => {
                let install = self.discover_installation(custom_install_root.as_deref())?;
                if !install.is_valid {
                    let msg = install
                        .error
                        .unwrap_or_else(|| "Zalo.app installation invalid".to_owned());
                    if install.signature_status.starts_with("Unsigned")
                        || install.signature_status.starts_with("Unverified")
                    {
                        return Err(DimError::UnverifiedBinary(msg));
                    }
                    return Err(DimError::InstallNotFound(msg));
                }

                write_launch_target_hint(profile_dir, &profile.id, &install.install_root);
                let open_args = build_open_launch_args(
                    &install.install_root,
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
                    enforce_signature_or_err(&sig, true)?;
                }

                write_launch_target_hint(profile_dir, &profile.id, path);
                let mut full_args = args.clone();
                if self.config.include_user_data_dir_arg
                    && !full_args.iter().any(|a| a.starts_with("--user-data-dir"))
                {
                    full_args.push(format!(
                        "--user-data-dir={}",
                        resolved_layout.zalo_data_dir.display()
                    ));
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
                let _ = std::fs::write(profile_dir.join(".runtime-pid"), pid.to_string());
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

                write_launch_target_hint(profile_dir, &profile.id, script_path);
                let mut full_args = vec![script_path.to_string_lossy().into_owned()];
                if self.config.include_user_data_dir_arg
                    && !extra_args.iter().any(|a| a.starts_with("--user-data-dir"))
                {
                    full_args.push(format!(
                        "--user-data-dir={}",
                        resolved_layout.zalo_data_dir.display()
                    ));
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
                let _ = std::fs::write(profile_dir.join(".runtime-pid"), pid.to_string());
            }
        }

        // Poll briefly so `SingletonLock` / `lockfile` registration is reflected immediately on return.
        let is_node_fixture = matches!(target, LaunchTarget::NodeFixture { .. });
        let deadline = Instant::now() + Duration::from_millis(self.config.launch_ready_poll_ms);
        loop {
            let state = self.inspect_profile(profile, profile_dir)?;
            if state.running {
                let lock_ready = if is_node_fixture {
                    std::fs::symlink_metadata(state.zalo_data_dir.join("SingletonLock")).is_ok()
                } else {
                    state.lockfile_path.exists()
                        || std::fs::symlink_metadata(&state.lockfile_path).is_ok()
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
        _target: &LaunchTarget,
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

        activate_profile_window(&*self.ops, &state.zalo_data_dir, pid)
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
        recover_macos_stale_artifacts(profile, profile_dir, state.running)
    }
}

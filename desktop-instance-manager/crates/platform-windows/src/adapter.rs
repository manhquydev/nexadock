//! `WindowsPlatformAdapter` implementing all core traits for Windows 10/11.

use std::fs;
use std::path::{Path, PathBuf};

#[cfg(windows)]
use dim_core::{
    build_runtime_lease, remove_runtime_lease, verify_runtime_lease, write_runtime_lease,
    LeaseVerification, ProfileLifecycleState, RUNTIME_LEASE_FILE, RUNTIME_PID_FILE,
};
use dim_core::{
    detect_zalo_user_id, AccountActivationResult, AccountKind, ActionableRemediation,
    ActivationOutcome, ApplicationInstallation, ApplicationLocator, CompanionSnapResult, DimError,
    InstanceLauncher, InstanceMonitor, LaunchTarget, PlatformCapabilities,
    PlatformCapabilityReport, PlatformKind, PrimaryInstanceStatus, PrimaryWindowState,
    ProfileRecord, ProfileRuntimeState, SupportStatus, WindowController, WindowRect,
};
use dim_profile_store::unlink_reparse_points_safe;

#[cfg(not(windows))]
use crate::layout::has_pe_authenticode_table;
#[cfg(windows)]
use crate::layout::{
    build_isolated_env_map, build_windows_command_line, encode_windows_env_block_utf16,
};
use crate::layout::{
    compare_versions, ensure_windows_profile_layout, is_appdata_redirected, parse_version_parts,
    windows_zalo_data_dir,
};

/// Windows 10/11 platform adapter using per-process `CreateProcessW` environment redirection
/// and Restart Manager (`rstrtmgr.dll`) lockfile inspection.
#[derive(Debug, Clone)]
pub struct WindowsPlatformAdapter {
    allow_user_data_dir_fallback: bool,
    require_authenticode_trust: bool,
}

impl Default for WindowsPlatformAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsPlatformAdapter {
    /// Creates a new Windows platform adapter with strict Authenticode trust verification enabled.
    #[must_use]
    pub fn new() -> Self {
        Self {
            allow_user_data_dir_fallback: true,
            require_authenticode_trust: true,
        }
    }

    /// Configures whether `--user-data-dir` should also be appended for custom executables.
    #[must_use]
    pub fn with_user_data_dir_fallback(mut self, enabled: bool) -> Self {
        self.allow_user_data_dir_fallback = enabled;
        self
    }

    /// Configures whether `discover_installation` and launch reject unsigned or untrusted executables.
    ///
    /// Production instances must keep the default (`true`); `false` is reserved for synthetic
    /// directory-layout unit tests using placeholder files.
    #[cfg(test)]
    #[must_use]
    pub fn with_signature_enforcement(mut self, enforce: bool) -> Self {
        self.require_authenticode_trust = enforce;
        self
    }

    #[cfg(windows)]
    fn resolve_target_command(
        &self,
        target: &LaunchTarget,
        zalo_data_dir: &Path,
        extra_override: &[String],
    ) -> Result<(PathBuf, Vec<String>, PathBuf), DimError> {
        match target {
            LaunchTarget::OfficialZalo {
                custom_install_root,
            } => {
                let install = self.discover_installation(custom_install_root.as_deref())?;
                if !install.is_valid {
                    let msg = install
                        .error
                        .unwrap_or_else(|| "Zalo installation invalid".to_owned());
                    if install.signature_status != "NotFound"
                        && install.signature_status != "MissingVersionedBinary"
                    {
                        return Err(DimError::UnverifiedBinary(msg));
                    }
                    return Err(DimError::InstallNotFound(msg));
                }
                let exe = install.executable_path.ok_or_else(|| {
                    DimError::InstallNotFound("missing Zalo-<ver>\\Zalo.exe".to_owned())
                })?;
                let work_dir = exe
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| install.install_root.clone());
                let mut args = Vec::new();
                args.extend(extra_override.iter().cloned());
                Ok((exe, args, work_dir))
            }
            LaunchTarget::CustomExecutable { path, args } => {
                let clean_path = strip_windows_verbatim_prefix(path);
                if !clean_path.exists() {
                    return Err(DimError::InstallNotFound(format!(
                        "custom executable not found: {}",
                        clean_path.display()
                    )));
                }
                if self.require_authenticode_trust {
                    verify_windows_binary_or_err(&clean_path, None)?;
                }
                let work_dir = clean_path
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| zalo_data_dir.to_path_buf());
                let mut full_args = args.clone();
                if self.allow_user_data_dir_fallback
                    && !full_args.iter().any(|a| a.starts_with("--user-data-dir"))
                {
                    full_args.push(format!("--user-data-dir={}", zalo_data_dir.display()));
                }
                full_args.extend(extra_override.iter().cloned());
                Ok((clean_path, full_args, work_dir))
            }
            LaunchTarget::NodeFixture {
                node_path,
                script_path,
                extra_args,
            } => {
                let clean_script = strip_windows_verbatim_prefix(script_path);
                if !clean_script.exists() {
                    return Err(DimError::InstallNotFound(format!(
                        "fixture script not found: {}",
                        clean_script.display()
                    )));
                }
                let node_exe = resolve_node_executable(node_path.as_deref())?;
                if self.require_authenticode_trust {
                    verify_windows_binary_or_err(&node_exe, None)?;
                }
                let work_dir = clean_script
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| zalo_data_dir.to_path_buf());
                let mut full_args = vec![clean_script.to_string_lossy().into_owned()];
                full_args.extend(extra_args.iter().cloned());
                full_args.extend(extra_override.iter().cloned());
                Ok((node_exe, full_args, work_dir))
            }
        }
    }
}

impl ApplicationLocator for WindowsPlatformAdapter {
    fn discover_installation(
        &self,
        custom_root: Option<&Path>,
    ) -> Result<ApplicationInstallation, DimError> {
        let root = match custom_root {
            Some(p) => p.to_path_buf(),
            None => {
                let local = std::env::var_os("LOCALAPPDATA")
                    .map(PathBuf::from)
                    .ok_or_else(|| {
                        DimError::InstallNotFound("LOCALAPPDATA is not set".to_owned())
                    })?;
                local.join("Programs").join("Zalo")
            }
        };

        if !root.exists() {
            return Ok(ApplicationInstallation {
                is_valid: false,
                platform: PlatformKind::Windows,
                install_root: root.clone(),
                launcher_path: None,
                executable_path: None,
                app_asar_path: None,
                version: None,
                signature_status: "NotFound".to_owned(),
                signer_subject: None,
                support_status: SupportStatus::PartiallySupported,
                notes: Vec::new(),
                error: Some(format!("Zalo directory not found at {}", root.display())),
            });
        }

        let stub_path = root.join("Zalo.exe");
        let launcher_path = stub_path.exists().then_some(stub_path);

        let mut best: Option<(Vec<u32>, String, PathBuf, PathBuf)> = None;
        if let Ok(entries) = fs::read_dir(&root) {
            for entry in entries.flatten() {
                let dir_path = entry.path();
                if !dir_path.is_dir() {
                    continue;
                }
                let Some(folder) = dir_path.file_name().and_then(|s| s.to_str()) else {
                    continue;
                };
                if !folder.starts_with("Zalo-") {
                    continue;
                }
                let ver_str = folder.trim_start_matches("Zalo-").to_owned();
                let exe = dir_path.join("Zalo.exe");
                let asar = dir_path.join("resources").join("app.asar");
                if !exe.exists() || !asar.exists() {
                    continue;
                }
                let parsed = parse_version_parts(&ver_str);
                let is_newer = best
                    .as_ref()
                    .map(|(b_ver, _, _, _)| {
                        compare_versions(&parsed, b_ver) == std::cmp::Ordering::Greater
                    })
                    .unwrap_or(true);
                if is_newer {
                    best = Some((parsed, ver_str, exe, asar));
                }
            }
        }

        let Some((_, version, exe_path, asar_path)) = best else {
            return Ok(ApplicationInstallation {
                is_valid: false,
                platform: PlatformKind::Windows,
                install_root: root,
                launcher_path,
                executable_path: None,
                app_asar_path: None,
                version: None,
                signature_status: "MissingVersionedBinary".to_owned(),
                signer_subject: None,
                support_status: SupportStatus::PartiallySupported,
                notes: Vec::new(),
                error: Some(
                    "No valid Zalo-<version> directory with Zalo.exe and resources\\app.asar found"
                        .to_owned(),
                ),
            });
        };

        #[cfg(windows)]
        let (is_valid, signature_status, signer_subject, error) = {
            use crate::win32_ffi::{verify_authenticode_signature, AuthenticodeTrustOutcome};
            match verify_authenticode_signature(&exe_path, Some("VNG")) {
                AuthenticodeTrustOutcome::Trusted {
                    subject,
                    status_label,
                } => (true, status_label, subject, None),
                AuthenticodeTrustOutcome::Missing => (
                    !self.require_authenticode_trust,
                    "Missing".to_owned(),
                    None,
                    self.require_authenticode_trust
                        .then(|| "Official Zalo executable missing".to_owned()),
                ),
                AuthenticodeTrustOutcome::Unsigned => (
                    !self.require_authenticode_trust,
                    "Unsigned".to_owned(),
                    None,
                    self.require_authenticode_trust.then(|| {
                        "Official Zalo Authenticode validation failed: Unsigned".to_owned()
                    }),
                ),
                AuthenticodeTrustOutcome::UntrustedOrTampered {
                    subject,
                    status_label,
                    ..
                } => (
                    !self.require_authenticode_trust,
                    status_label.clone(),
                    subject,
                    self.require_authenticode_trust.then(|| {
                        format!("Official Zalo Authenticode validation failed: {status_label}")
                    }),
                ),
                AuthenticodeTrustOutcome::WrongPublisher {
                    subject,
                    status_label,
                } => (
                    !self.require_authenticode_trust,
                    status_label.clone(),
                    Some(subject),
                    self.require_authenticode_trust.then(|| {
                        format!("Official Zalo publisher verification failed: {status_label}")
                    }),
                ),
                AuthenticodeTrustOutcome::VerificationError { detail } => (
                    !self.require_authenticode_trust,
                    format!("Unverified ({detail})"),
                    None,
                    self.require_authenticode_trust
                        .then(|| format!("Authenticode verification failed: {detail}")),
                ),
            }
        };

        #[cfg(not(windows))]
        let (is_valid, signature_status, signer_subject, error) = {
            let has_sig = has_pe_authenticode_table(&exe_path);
            let status = if has_sig {
                "EmbeddedAuthenticodePresent (WinVerifyTrustRequiresWindows)".to_owned()
            } else {
                "Unsigned".to_owned()
            };
            let valid = !self.require_authenticode_trust;
            let err = if valid {
                None
            } else {
                Some("WinVerifyTrust signature verification requires Windows".to_owned())
            };
            (valid, status, None, err)
        };

        Ok(ApplicationInstallation {
            is_valid,
            platform: PlatformKind::Windows,
            install_root: root,
            launcher_path,
            executable_path: Some(exe_path),
            app_asar_path: Some(asar_path),
            version: Some(version),
            signature_status,
            signer_subject,
            support_status: SupportStatus::PartiallySupported,
            notes: vec![
                "Bypassing stub Zalo.exe avoids short-lived ZaloStubExecutable mutex; WinVerifyTrust + VNG subject verified prior to launch."
                    .to_owned(),
            ],
            error,
        })
    }
}

impl PlatformCapabilities for WindowsPlatformAdapter {
    fn platform(&self) -> PlatformKind {
        PlatformKind::Windows
    }

    fn capabilities(&self) -> PlatformCapabilityReport {
        #[cfg(windows)]
        let raw_appdata = crate::win32_ffi::read_raw_user_shell_appdata();
        #[cfg(not(windows))]
        let raw_appdata: Option<String> = None;

        let redirected = raw_appdata
            .as_deref()
            .map(is_appdata_redirected)
            .unwrap_or(false);

        let official_status = if redirected {
            SupportStatus::Blocked
        } else {
            SupportStatus::PartiallySupported
        };

        #[cfg(windows)]
        let (call_recv_present, call_send_present) =
            crate::win32_ffi::passive_zcall_pipes_present();
        #[cfg(not(windows))]
        let (call_recv_present, call_send_present) = (false, false);

        PlatformCapabilityReport {
            platform: PlatformKind::Windows,
            overall_status: official_status,
            official_zalo_status: official_status,
            fixture_status: SupportStatus::SupportedAndTested,
            appimage_status: None,
            concurrent_calls_status: SupportStatus::Blocked,
            folder_redirection_detected: redirected,
            folder_redirection_path: raw_appdata,
            limitations: vec![
                "Voice/video call helper channels use global named pipes \\\\.\\pipe\\PipeZCallRecv and \\\\.\\pipe\\PipeZCallSend (Confirmed by source code; cross-account live call handoff is gated on human-verified TC-H1B)."
                    .to_owned(),
                "Enable Zalo's in-app floating notification banner (notiWin / znotification.html) so notification clicks route inside each account window; Windows Action Center Toast shares AppUserModelId(\"com.vng.zalo\") via Zalo.lnk."
                    .to_owned(),
                "Smart App Control hosts block unsigned locally compiled PEs unless code-signed or hosted via signed PowerShell/Node."
                    .to_owned(),
            ],
            diagnostics: vec![
                "Per-process USERPROFILE/APPDATA/LOCALAPPDATA/TEMP isolation via CreateProcessW".to_owned(),
                "Lockfile ownership inspection via Restart Manager (rstrtmgr.dll)".to_owned(),
                "6-condition EnumWindows main-window filter excluding 352x82 notiWin popups + second-instance Foreground Lock handoff".to_owned(),
                format!(
                    "Passive PipeZCall directory check (zero client stream connection): PipeZCallRecv={call_recv_present}, PipeZCallSend={call_send_present}"
                ),
            ],
        }
    }

    fn free_disk_bytes(&self, path: &Path) -> Result<u64, DimError> {
        #[cfg(windows)]
        {
            crate::win32_ffi::query_free_disk_bytes(path)
        }
        #[cfg(not(windows))]
        {
            let _ = path;
            Ok(dim_core::MIN_FREE_DISK_BYTES.saturating_mul(2))
        }
    }

    fn unlink_profile_junctions(&self, profile_dir: &Path) -> Result<usize, DimError> {
        unlink_reparse_points_safe(profile_dir)
    }

    fn is_elevated(&self) -> Option<bool> {
        #[cfg(windows)]
        {
            crate::win32_ffi::is_current_process_elevated()
        }
        #[cfg(not(windows))]
        {
            None
        }
    }

    fn smart_app_control_state(&self) -> Option<u32> {
        #[cfg(windows)]
        {
            crate::win32_ffi::read_smart_app_control_state()
        }
        #[cfg(not(windows))]
        {
            None
        }
    }

    fn check_profiles_acl_safe(&self, profiles_dir: &Path) -> Option<bool> {
        #[cfg(windows)]
        {
            crate::win32_ffi::check_directory_acl_safe(profiles_dir)
        }
        #[cfg(not(windows))]
        {
            let _ = profiles_dir;
            None
        }
    }
}

impl InstanceMonitor for WindowsPlatformAdapter {
    fn inspect_primary(&self) -> Result<PrimaryInstanceStatus, DimError> {
        #[cfg(windows)]
        {
            let roaming = std::env::var_os("APPDATA")
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var_os("USERPROFILE")
                        .map(|p| PathBuf::from(p).join("AppData").join("Roaming"))
                })
                .unwrap_or_else(|| PathBuf::from("AppData").join("Roaming"));
            let zalo_data = roaming.join("ZaloData");
            let lockfile = zalo_data.join("lockfile");

            let self_pid = std::process::id();
            let locking_pids: Vec<u32> = crate::win32_ffi::get_locking_pids(&lockfile)
                .into_iter()
                .filter(|&pid| pid > 0 && pid != self_pid && crate::win32_ffi::is_pid_alive(pid))
                .collect();
            let mut protected = Vec::new();
            let snap = crate::win32_ffi::ProcessSnapshot::capture();
            for root in &locking_pids {
                for pid in snap.collect_tree(*root) {
                    if !protected.contains(&pid) {
                        protected.push(pid);
                    }
                }
            }

            let working_set_bytes = crate::win32_ffi::sum_working_set_bytes(&protected);
            let detected_zalo_user_id = detect_zalo_user_id(&zalo_data);
            let running =
                !protected.is_empty() || crate::win32_ffi::is_file_exclusively_locked(&lockfile);
            let (window_handle, window_title, window_state) = if !protected.is_empty() {
                if let Some((hwnd, title)) = crate::win32_ffi::find_main_window_for_pids(&protected)
                {
                    let state = if crate::win32_ffi::is_window_minimized(hwnd) {
                        PrimaryWindowState::Minimized
                    } else {
                        PrimaryWindowState::Visible
                    };
                    (Some(hwnd), Some(title), state)
                } else {
                    (None, None, PrimaryWindowState::HiddenInTray)
                }
            } else if running {
                (None, None, PrimaryWindowState::HiddenInTray)
            } else {
                (None, None, PrimaryWindowState::None)
            };

            Ok(PrimaryInstanceStatus {
                zalo_data_dir: zalo_data,
                lockfile_path: lockfile,
                running,
                protected_pids: protected.clone(),
                process_count: protected.len(),
                working_set_bytes,
                detected_zalo_user_id,
                window_handle,
                window_title,
                window_state,
                notes: vec![
                    "Primary Zalo PIDs are permanently protected against stop/kill.".to_owned(),
                ],
            })
        }
        #[cfg(not(windows))]
        {
            Err(DimError::UnsupportedPlatform(
                "Windows primary inspection requires Windows".to_owned(),
            ))
        }
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
        let zalo_data = windows_zalo_data_dir(profile_dir);
        let lockfile = zalo_data.join("lockfile");
        let detected_uid = detect_zalo_user_id(&zalo_data);

        #[cfg(windows)]
        {
            let marker_mismatch = zalo_data.exists()
                && dim_profile_store::verify_profile_id_marker(&zalo_data, &profile.id, false)
                    .is_err();

            let locking_pids: Vec<u32> = crate::win32_ffi::get_locking_pids(&lockfile)
                .into_iter()
                .filter(|&pid| {
                    pid > 0 && pid != std::process::id() && crate::win32_ffi::is_pid_alive(pid)
                })
                .collect();
            let exclusively_locked =
                lockfile.exists() && crate::win32_ffi::is_file_exclusively_locked(&lockfile);
            let lease_check = verify_runtime_lease(profile_dir, &profile.id, &[]);

            let mut cached_snap: Option<crate::win32_ffi::ProcessSnapshot> = None;
            let is_expected_exe = |snap: &crate::win32_ffi::ProcessSnapshot, pid: u32| -> bool {
                snap.exe_name(pid).is_some_and(|exe| {
                    let lower = exe.to_ascii_lowercase();
                    lower == "zalo.exe"
                        || lower == "node.exe"
                        || lower.starts_with("dim-test-fixture")
                        || lower.starts_with("xf-desktop-fixture")
                })
            };

            let mut main_pid: Option<u32> = None;
            let mut lifecycle_state = ProfileLifecycleState::Stopped;
            let mut indeterminate = false;

            if let Some(&rm_pid) = locking_pids.first() {
                main_pid = Some(rm_pid);
                lifecycle_state = ProfileLifecycleState::Running;
            } else if exclusively_locked {
                if let LeaseVerification::VerifiedLive(ref lease) = lease_check {
                    main_pid = Some(lease.pid);
                    lifecycle_state = ProfileLifecycleState::Running;
                } else {
                    lifecycle_state = ProfileLifecycleState::RecoveryRequired;
                    indeterminate = true;
                }
            } else {
                let lockfile_non_empty = fs::metadata(&lockfile)
                    .map(|m| m.is_file() && m.len() > 0)
                    .unwrap_or(false);
                if lockfile_non_empty {
                    if let Some(json_pid) = read_lockfile_json_pid(&lockfile) {
                        if json_pid > 0
                            && json_pid != std::process::id()
                            && crate::win32_ffi::is_pid_alive(json_pid)
                        {
                            let verified_by_lease = matches!(
                                &lease_check,
                                LeaseVerification::VerifiedLive(l) if l.pid == json_pid
                            );
                            let verified_unmanaged =
                                if matches!(lease_check, LeaseVerification::Absent) {
                                    let snap = cached_snap.get_or_insert_with(
                                        crate::win32_ffi::ProcessSnapshot::capture,
                                    );
                                    is_expected_exe(snap, json_pid)
                                } else {
                                    false
                                };
                            if verified_by_lease || verified_unmanaged {
                                main_pid = Some(json_pid);
                                lifecycle_state = ProfileLifecycleState::Running;
                            }
                        }
                    }
                }

                if main_pid.is_none() {
                    match &lease_check {
                        LeaseVerification::VerifiedLive(lease) => {
                            // Spawned process is verified alive (matching creation time + exe path)
                            // while `lockfile` is not yet created or still 0-byte transient -> `Starting`!
                            main_pid = Some(lease.pid);
                            lifecycle_state = ProfileLifecycleState::Starting;
                        }
                        LeaseVerification::InvalidOrIndeterminate(_) => {
                            lifecycle_state = ProfileLifecycleState::RecoveryRequired;
                            indeterminate = true;
                        }
                        LeaseVerification::Absent | LeaseVerification::StaleDead(_) => {
                            if marker_mismatch {
                                lifecycle_state = ProfileLifecycleState::RecoveryRequired;
                                indeterminate = true;
                            } else {
                                lifecycle_state = ProfileLifecycleState::Stopped;
                            }
                        }
                    }
                }
            }

            let (process_ids, working_set, window_handle, window_title) = if let Some(pid) =
                main_pid
            {
                let snap = cached_snap.unwrap_or_else(crate::win32_ffi::ProcessSnapshot::capture);
                let tree = snap.collect_tree(pid);
                let ws = crate::win32_ffi::sum_working_set_bytes(&tree);
                let win = crate::win32_ffi::find_main_window_for_profile(pid, &tree);
                (tree, ws, win.as_ref().map(|(h, _)| *h), win.map(|(_, t)| t))
            } else {
                (Vec::new(), 0, None, None)
            };

            let running = lifecycle_state.is_active();
            let has_stale_artifacts = lockfile.exists()
                || matches!(lease_check, LeaseVerification::StaleDead(_))
                || profile_dir.join(RUNTIME_LEASE_FILE).exists()
                || profile_dir.join(RUNTIME_PID_FILE).exists();
            let stale_lock_detected = !running && !indeterminate && has_stale_artifacts;
            let status_label = lifecycle_state
                .default_status_label(stale_lock_detected)
                .to_owned();

            Ok(ProfileRuntimeState {
                profile_id: profile.id.clone(),
                profile_name: profile.name.clone(),
                profile_dir: profile_dir.to_path_buf(),
                zalo_data_dir: zalo_data,
                lockfile_path: lockfile,
                running,
                stale_lock_detected,
                main_pid,
                process_count: process_ids.len(),
                process_ids,
                working_set_bytes: working_set,
                window_handle,
                window_title,
                detected_zalo_user_id: detected_uid,
                disk_usage_bytes: if include_size {
                    dir_size_no_follow(profile_dir)
                } else {
                    0
                },
                status_label,
            })
        }
        #[cfg(not(windows))]
        {
            let _ = (profile, detected_uid, lockfile, include_size);
            Err(DimError::UnsupportedPlatform(
                "Windows runtime inspection requires Windows".to_owned(),
            ))
        }
    }
}

impl InstanceLauncher for WindowsPlatformAdapter {
    fn ensure_profile_layout(
        &self,
        profile: &ProfileRecord,
        profile_dir: &Path,
    ) -> Result<(), DimError> {
        ensure_windows_profile_layout(profile, profile_dir)
    }

    fn launch(
        &self,
        target: &LaunchTarget,
        profile: &ProfileRecord,
        profile_dir: &Path,
    ) -> Result<ProfileRuntimeState, DimError> {
        ensure_windows_profile_layout(profile, profile_dir)?;

        #[cfg(windows)]
        {
            let zalo_data = windows_zalo_data_dir(profile_dir);
            let mut extra_override = Vec::new();
            if matches!(target, LaunchTarget::NodeFixture { extra_args, .. } if !extra_args.iter().any(|a| a == "--user-id" || a.starts_with("--user-id=")))
            {
                let resolved_uid = detect_zalo_user_id(&zalo_data)
                    .or_else(|| profile.zalo_user_id.clone())
                    .unwrap_or_else(|| derive_deterministic_fixture_user_id(&profile.id));
                extra_override.push("--user-id".to_owned());
                extra_override.push(resolved_uid);
            }

            let (exe, args, work_dir) =
                self.resolve_target_command(target, &zalo_data, &extra_override)?;
            let base_env: Vec<(String, String)> = std::env::vars().collect();
            let env_map = build_isolated_env_map(&base_env, profile, profile_dir);
            let env_block = encode_windows_env_block_utf16(&env_map);
            let cmd_line = build_windows_command_line(&exe, &args);

            let _ = fs::remove_file(zalo_data.join(".quit-signal"));
            let spawned = crate::win32_ffi::spawn_process_isolated_with_identity(
                &exe, &cmd_line, &work_dir, &env_block,
            )?;
            let lease = build_runtime_lease(
                spawned.pid,
                &exe,
                &profile.id,
                Some((spawned.create_time_100ns, spawned.executable_path)),
            );
            write_runtime_lease(profile_dir, &lease)?;

            let lockfile = zalo_data.join("lockfile");
            let quit_signal = zalo_data.join(".quit-signal");
            let poll_budget_ms = match target {
                LaunchTarget::NodeFixture { .. } | LaunchTarget::CustomExecutable { .. } => 10_000,
                LaunchTarget::OfficialZalo { .. } => 1_500,
            };
            let deadline =
                std::time::Instant::now() + std::time::Duration::from_millis(poll_budget_ms);
            while std::time::Instant::now() < deadline {
                if quit_signal.exists() || is_windows_lockfile_ready(&lockfile) {
                    break;
                }
                if !crate::win32_ffi::is_pid_alive(spawned.pid) {
                    if is_windows_lockfile_ready(&lockfile) {
                        break;
                    }
                    let _ = remove_runtime_lease(profile_dir);
                    if lockfile.exists()
                        && fs::metadata(&lockfile)
                            .map(|m| m.len() == 0)
                            .unwrap_or(false)
                        && !crate::win32_ffi::is_file_exclusively_locked(&lockfile)
                    {
                        let _ = fs::remove_file(&lockfile);
                    }
                    return Err(DimError::LaunchFailed(format!(
                        "spawned process (PID {}) exited before acquiring profile lockfile",
                        spawned.pid
                    )));
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }

            self.inspect_profile(profile, profile_dir)
        }
        #[cfg(not(windows))]
        {
            let _ = target;
            Err(DimError::UnsupportedPlatform(
                "Windows CreateProcessW launch requires Windows".to_owned(),
            ))
        }
    }

    fn focus_or_restore(
        &self,
        target: &LaunchTarget,
        profile: &ProfileRecord,
        profile_dir: &Path,
        state: &ProfileRuntimeState,
    ) -> Result<(), DimError> {
        #[cfg(windows)]
        {
            if let Some(hwnd) = state.window_handle {
                let mut allowed_pids = state.process_ids.clone();
                if let Some(mpid) = state.main_pid {
                    if !allowed_pids.contains(&mpid) {
                        allowed_pids.push(mpid);
                    }
                }
                if let Ok((_, true)) = crate::win32_ffi::focus_window_verified(hwnd, &allowed_pids)
                {
                    return Ok(());
                }
                // Win32 Foreground Lock rejected direct `SetForegroundWindow`, or HWND PID changed;
                // fall through to isolated `second-instance` process handoff (`IR-WIN-01`).
            }

            let base_env: Vec<(String, String)> = std::env::vars().collect();
            let env_map = build_isolated_env_map(&base_env, profile, profile_dir);
            let env_block = encode_windows_env_block_utf16(&env_map);
            let (exe, args, work_dir) =
                self.resolve_target_command(target, &state.zalo_data_dir, &[])?;
            let cmd_line = build_windows_command_line(&exe, &args);
            let _ =
                crate::win32_ffi::spawn_process_isolated(&exe, &cmd_line, &work_dir, &env_block)?;
            Ok(())
        }
        #[cfg(not(windows))]
        {
            let _ = (target, profile, profile_dir, state);
            Err(DimError::UnsupportedPlatform(
                "Windows focus_or_restore requires Windows".to_owned(),
            ))
        }
    }

    fn stop(
        &self,
        state: &ProfileRuntimeState,
        protected_primary_pids: &[u32],
        timeout_ms: u64,
    ) -> Result<u32, DimError> {
        let Some(main_pid) = state.main_pid else {
            return Ok(0);
        };
        if protected_primary_pids.contains(&main_pid)
            || state
                .process_ids
                .iter()
                .any(|p| protected_primary_pids.contains(p))
        {
            return Err(DimError::ProtectedPrimaryProcess(format!(
                "refused to stop protected PID overlap on profile '{}'",
                state.profile_name
            )));
        }

        #[cfg(windows)]
        {
            let lease_verified_pid = match verify_runtime_lease(
                &state.profile_dir,
                &state.profile_id,
                protected_primary_pids,
            ) {
                LeaseVerification::VerifiedLive(lease) => Some(lease.pid),
                LeaseVerification::InvalidOrIndeterminate(msg)
                    if msg.contains("protected primary") =>
                {
                    return Err(DimError::ProtectedPrimaryProcess(msg));
                }
                _ => None,
            };
            let rm_locking_pids = crate::win32_ffi::get_locking_pids(&state.lockfile_path);

            let pre_snap = crate::win32_ffi::ProcessSnapshot::capture();
            let mut pre_verified_owned_tree = Vec::new();
            if let Some(lpid) = lease_verified_pid {
                for pid in pre_snap.collect_tree(lpid) {
                    if !pre_verified_owned_tree.contains(&pid) {
                        pre_verified_owned_tree.push(pid);
                    }
                }
            }
            for &rpid in &rm_locking_pids {
                if rpid > 0 {
                    for pid in pre_snap.collect_tree(rpid) {
                        if !pre_verified_owned_tree.contains(&pid) {
                            pre_verified_owned_tree.push(pid);
                        }
                    }
                }
            }
            let is_zalo_exe = pre_snap
                .exe_name(main_pid)
                .map(|n| n.eq_ignore_ascii_case("zalo.exe"))
                .unwrap_or(false);

            // Step 1: Signal graceful shutdown (.quit-signal for fixtures, --z-update-force-quit & WM_CLOSE for Zalo).
            if state.zalo_data_dir.exists() {
                let _ = fs::write(
                    state.zalo_data_dir.join(".quit-signal"),
                    format!("{{\"requested_at\":\"{}\"}}\n", dim_core::now_rfc3339_utc()),
                );
            }
            if is_zalo_exe && state.lifecycle_state() == ProfileLifecycleState::Running {
                if let Ok(install) = self.discover_installation(None) {
                    if install.is_valid {
                        if let Some(exe) = install.executable_path {
                            let base_env: Vec<(String, String)> = std::env::vars().collect();
                            let synth_profile = ProfileRecord {
                                id: state.profile_id.clone(),
                                name: state.profile_name.clone(),
                                directory_name: String::new(),
                                created_at: String::new(),
                                last_launched_at: None,
                                zalo_user_id: None,
                                notes: String::new(),
                            };
                            let env_map = build_isolated_env_map(
                                &base_env,
                                &synth_profile,
                                &state.profile_dir,
                            );
                            let env_block = encode_windows_env_block_utf16(&env_map);
                            let cmd_line = build_windows_command_line(
                                &exe,
                                &["--z-update-force-quit".to_owned()],
                            );
                            let work_dir = exe.parent().unwrap_or(state.profile_dir.as_path());
                            let _ = crate::win32_ffi::spawn_process_isolated(
                                &exe, &cmd_line, work_dir, &env_block,
                            );
                        }
                    }
                }
            }
            if let Some(hwnd) = state.window_handle {
                crate::win32_ffi::post_wm_close(hwnd);
            }

            // During pre-lock `Starting` with no window handle, the spawned process has not yet entered
            // its lockfile/IPC loop, so cap graceful wait to 150ms before terminating the owned tree cleanly.
            let graceful_wait_ms = if state.lifecycle_state() == ProfileLifecycleState::Starting
                && state.window_handle.is_none()
            {
                timeout_ms.min(150)
            } else {
                timeout_ms
            };

            if crate::win32_ffi::wait_for_exit(main_pid, graceful_wait_ms) {
                let all_children_exited = state
                    .process_ids
                    .iter()
                    .all(|&p| p == main_pid || !crate::win32_ffi::is_pid_alive(p));
                if all_children_exited {
                    wait_for_lock_release(&state.lockfile_path);
                    let _ = remove_runtime_lease(&state.profile_dir);
                    let _ = fs::remove_file(state.zalo_data_dir.join(".quit-signal"));
                    return Ok(1);
                }
            }

            // Step 2: Targeted cleanup of ONLY this profile's surviving non-primary PID tree.
            let snap = crate::win32_ffi::ProcessSnapshot::capture();
            let mut verified_roots = Vec::new();
            if let Some(lpid) = lease_verified_pid {
                verified_roots.push(lpid);
            }
            for &rpid in &rm_locking_pids {
                if rpid > 0 && !verified_roots.contains(&rpid) {
                    verified_roots.push(rpid);
                }
            }

            let mut verified_owned_tree = pre_verified_owned_tree;
            for &root in &verified_roots {
                for pid in snap.collect_tree(root) {
                    if !verified_owned_tree.contains(&pid) {
                        verified_owned_tree.push(pid);
                    }
                }
            }

            let mut candidates = snap.collect_tree(main_pid);
            for &pid in &verified_owned_tree {
                if !candidates.contains(&pid) {
                    candidates.push(pid);
                }
            }
            for &pid in &state.process_ids {
                if !candidates.contains(&pid) {
                    candidates.push(pid);
                }
            }

            let safe_pids = dim_process_manager::PrimaryProtectionGuard::filter_protected_pids(
                &candidates,
                protected_primary_pids,
            );

            let mut stopped_count = 0u32;
            for pid in safe_pids {
                if pid == 0 || pid == std::process::id() || !crate::win32_ffi::is_pid_alive(pid) {
                    continue;
                }
                let is_verified_owned = verified_owned_tree.contains(&pid);
                let allowed_exe = is_verified_owned
                    || snap
                        .exe_name(pid)
                        .map(|n| {
                            let lower = n.to_ascii_lowercase();
                            lower.starts_with("zalo")
                                || lower.starts_with("zocr")
                                || lower == "node.exe"
                                || lower.starts_with("dim-test-fixture")
                                || lower.starts_with("xf-desktop-fixture")
                        })
                        .unwrap_or(false);
                if allowed_exe && crate::win32_ffi::terminate_pid(pid) {
                    let _ = crate::win32_ffi::wait_for_exit(pid, 1_500);
                    stopped_count += 1;
                } else if allowed_exe && !crate::win32_ffi::is_pid_alive(pid) {
                    stopped_count += 1;
                }
            }

            wait_for_lock_release(&state.lockfile_path);
            let _ = remove_runtime_lease(&state.profile_dir);
            let _ = fs::remove_file(state.zalo_data_dir.join(".quit-signal"));
            if !crate::win32_ffi::is_pid_alive(main_pid)
                && !crate::win32_ffi::is_file_exclusively_locked(&state.lockfile_path)
            {
                let _ = fs::remove_file(&state.lockfile_path);
                let _ = fs::remove_file(state.zalo_data_dir.join("children.json"));
            }
            Ok(stopped_count)
        }
        #[cfg(not(windows))]
        {
            let _ = timeout_ms;
            Err(DimError::UnsupportedPlatform(
                "Windows stop requires Windows".to_owned(),
            ))
        }
    }
}

#[cfg(windows)]
fn verify_windows_binary_or_err(
    path: &Path,
    required_publisher_substr: Option<&str>,
) -> Result<(), DimError> {
    use crate::win32_ffi::{verify_authenticode_signature, AuthenticodeTrustOutcome};
    match verify_authenticode_signature(path, required_publisher_substr) {
        AuthenticodeTrustOutcome::Trusted { .. } => Ok(()),
        AuthenticodeTrustOutcome::Missing => Err(DimError::InstallNotFound(format!(
            "executable not found: {}",
            path.display()
        ))),
        AuthenticodeTrustOutcome::Unsigned => Err(DimError::UnverifiedBinary(format!(
            "executable is unsigned: {}",
            path.display()
        ))),
        AuthenticodeTrustOutcome::UntrustedOrTampered { status_label, .. } => {
            Err(DimError::UnverifiedBinary(format!(
                "Authenticode verification failed for {}: {status_label}",
                path.display()
            )))
        }
        AuthenticodeTrustOutcome::WrongPublisher { status_label, .. } => {
            Err(DimError::UnverifiedBinary(format!(
                "Publisher verification failed for {}: {status_label}",
                path.display()
            )))
        }
        AuthenticodeTrustOutcome::VerificationError { detail } => {
            Err(DimError::UnverifiedBinary(format!(
                "Authenticode verification error for {}: {detail}",
                path.display()
            )))
        }
    }
}

#[cfg(windows)]
fn derive_deterministic_fixture_user_id(profile_id: &str) -> String {
    let mut hash: u32 = 2_166_136_261;
    for b in profile_id.as_bytes() {
        hash ^= u32::from(*b);
        hash = hash.wrapping_mul(16_777_619);
    }
    let num = 1_000_000 + (hash % 8_999_999);
    num.to_string()
}

#[cfg(windows)]
fn wait_for_lock_release(lockfile: &Path) {
    for _ in 0..20 {
        if !crate::win32_ffi::is_file_exclusively_locked(lockfile) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[cfg(windows)]
fn strip_windows_verbatim_prefix(path: &Path) -> PathBuf {
    let raw = path.to_string_lossy();
    if let Some(rest) = raw.strip_prefix(r"\\?\") {
        if !rest.starts_with(r"UNC\") && !rest.starts_with(r"unc\") {
            return PathBuf::from(rest);
        }
    }
    path.to_path_buf()
}

#[cfg(windows)]
fn resolve_node_executable(custom: Option<&Path>) -> Result<PathBuf, DimError> {
    if let Some(p) = custom {
        return Ok(p.to_path_buf());
    }
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join("node.exe");
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }
    Err(DimError::InstallNotFound(
        "node.exe could not be found in PATH".to_owned(),
    ))
}

#[cfg(windows)]
fn is_windows_lockfile_ready(lockfile: &Path) -> bool {
    if !lockfile.exists() {
        return false;
    }
    if crate::win32_ffi::is_file_exclusively_locked(lockfile) {
        return true;
    }
    if crate::win32_ffi::get_locking_pids(lockfile)
        .into_iter()
        .any(|p| p > 0 && p != std::process::id() && crate::win32_ffi::is_pid_alive(p))
    {
        return true;
    }
    let non_empty = fs::metadata(lockfile)
        .map(|m| m.is_file() && m.len() > 0)
        .unwrap_or(false);
    non_empty && read_lockfile_json_pid(lockfile).is_some()
}

#[cfg(windows)]
fn read_lockfile_json_pid(lockfile: &Path) -> Option<u32> {
    let raw = fs::read_to_string(lockfile).ok()?;
    let val: serde_json::Value = serde_json::from_str(&raw).ok()?;
    val.get("pid")
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
}

#[cfg(windows)]
fn dir_size_no_follow(root: &Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            if dim_profile_store::is_symlink_or_reparse(&meta) {
                continue;
            }
            if meta.is_dir() {
                stack.push(path);
            } else if meta.is_file() {
                total = total.saturating_add(meta.len());
            }
        }
    }
    total
}

impl WindowController for WindowsPlatformAdapter {
    fn focus_primary_window(
        &self,
        _primary: &PrimaryInstanceStatus,
    ) -> Result<AccountActivationResult, DimError> {
        #[cfg(windows)]
        {
            // Strictly read-only, non-spawning: always re-inspect live protected primary state.
            let live = self.inspect_primary()?;
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
                        message: "Primary Zalo is not running. Launch your primary Zalo Desktop manually when needed.".to_owned(),
                        suggested_action: Some("launch_primary_manually".to_owned()),
                    }),
                });
            }

            if let Some(hwnd) = live.window_handle {
                if matches!(
                    live.window_state,
                    PrimaryWindowState::Visible | PrimaryWindowState::Minimized
                ) {
                    if let Ok((was_minimized, fg_confirmed)) =
                        crate::win32_ffi::focus_window_verified(hwnd, &live.protected_pids)
                    {
                        let outcome = if was_minimized {
                            ActivationOutcome::Restored
                        } else {
                            ActivationOutcome::Focused
                        };
                        let method = if was_minimized {
                            "direct_restore"
                        } else {
                            "direct_focus"
                        };
                        return Ok(AccountActivationResult {
                            account_id: "primary".to_owned(),
                            account_kind: AccountKind::DesktopPrimary,
                            outcome,
                            method: Some(method.to_owned()),
                            os_foreground_confirmed: Some(fg_confirmed),
                            window_handle: Some(hwnd),
                            window_title: live.window_title.clone(),
                            message: if was_minimized {
                                "Restored and focused primary Zalo window (read-only).".to_owned()
                            } else {
                                "Focused primary Zalo window (read-only).".to_owned()
                            },
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
                window_handle: None,
                window_title: None,
                message: "Primary Zalo is running in the system tray.".to_owned(),
                remediation: Some(ActionableRemediation {
                    code: "PRIMARY_HIDDEN_IN_TRAY".to_owned(),
                    message: "Primary Zalo is hidden in the Windows system tray. Click the Zalo tray icon to restore it without spawning a second process.".to_owned(),
                    suggested_action: Some("click_primary_tray_icon".to_owned()),
                }),
            })
        }
        #[cfg(not(windows))]
        {
            Err(DimError::UnsupportedPlatform(
                "Windows primary focus requires Windows".to_owned(),
            ))
        }
    }

    fn companion_snap_secondary(
        &self,
        profile: &ProfileRecord,
        profile_dir: &Path,
        host_hwnd: u64,
        rail_width_px: i32,
    ) -> Result<CompanionSnapResult, DimError> {
        #[cfg(windows)]
        {
            let live_primary = self.inspect_primary()?;
            let state = self.inspect_profile_with_options(profile, profile_dir, false)?;

            if state
                .main_pid
                .is_some_and(|p| live_primary.protected_pids.contains(&p))
                || state
                    .process_ids
                    .iter()
                    .any(|p| live_primary.protected_pids.contains(p))
            {
                return Err(DimError::ProtectedPrimaryProcess(
                    "PRIMARY_PROTECTED: companion snap is strictly forbidden on primary Zalo"
                        .to_owned(),
                ));
            }

            if !state.running {
                return Err(DimError::NotRunning(profile.name.clone()));
            }

            let hwnd = state.window_handle.ok_or_else(|| {
                DimError::ProcessError(format!(
                    "WINDOW_UNVERIFIED: profile '{}' has no verified visible or minimized main window to snap",
                    profile.name
                ))
            })?;

            let mut allowed_pids = state.process_ids.clone();
            if let Some(mpid) = state.main_pid {
                if !allowed_pids.contains(&mpid) {
                    allowed_pids.push(mpid);
                }
            }
            let win_pid = crate::win32_ffi::get_window_pid(hwnd).ok_or_else(|| {
                DimError::ProcessError("WINDOW_UNVERIFIED: secondary HWND has no PID".to_owned())
            })?;
            if live_primary.protected_pids.contains(&win_pid) {
                return Err(DimError::ProtectedPrimaryProcess(
                    "PRIMARY_PROTECTED: target HWND belongs to protected primary PID".to_owned(),
                ));
            }
            if !allowed_pids.contains(&win_pid) {
                return Err(DimError::ProcessError(format!(
                    "WINDOW_UNVERIFIED: HWND {hwnd} PID {win_pid} does not belong to profile '{}'",
                    profile.name
                )));
            }

            let (previous_rect, applied_rect) =
                crate::win32_ffi::companion_snap_window(hwnd, host_hwnd, rail_width_px)?;
            Ok(CompanionSnapResult {
                profile_id: profile.id.clone(),
                snapped: true,
                previous_rect: Some(previous_rect),
                applied_rect: Some(applied_rect),
                message: format!("Snapped profile '{}' beside KHOANG.", profile.name),
            })
        }
        #[cfg(not(windows))]
        {
            let _ = (profile, profile_dir, host_hwnd, rail_width_px);
            Err(DimError::UnsupportedPlatform(
                "companion_snap_secondary requires Windows".to_owned(),
            ))
        }
    }

    fn companion_unsnap_secondary(
        &self,
        profile: &ProfileRecord,
        profile_dir: &Path,
        restore_rect: WindowRect,
    ) -> Result<CompanionSnapResult, DimError> {
        #[cfg(windows)]
        {
            let live_primary = self.inspect_primary()?;
            let state = self.inspect_profile_with_options(profile, profile_dir, false)?;

            if state
                .main_pid
                .is_some_and(|p| live_primary.protected_pids.contains(&p))
                || state
                    .process_ids
                    .iter()
                    .any(|p| live_primary.protected_pids.contains(p))
            {
                return Err(DimError::ProtectedPrimaryProcess(
                    "PRIMARY_PROTECTED: companion unsnap is strictly forbidden on primary Zalo"
                        .to_owned(),
                ));
            }

            let hwnd = state.window_handle.ok_or_else(|| {
                DimError::ProcessError(format!(
                    "WINDOW_UNVERIFIED: profile '{}' has no verified main window to unsnap",
                    profile.name
                ))
            })?;

            let mut allowed_pids = state.process_ids.clone();
            if let Some(mpid) = state.main_pid {
                if !allowed_pids.contains(&mpid) {
                    allowed_pids.push(mpid);
                }
            }
            let win_pid = crate::win32_ffi::get_window_pid(hwnd).ok_or_else(|| {
                DimError::ProcessError("WINDOW_UNVERIFIED: secondary HWND has no PID".to_owned())
            })?;
            if live_primary.protected_pids.contains(&win_pid) {
                return Err(DimError::ProtectedPrimaryProcess(
                    "PRIMARY_PROTECTED: target HWND belongs to protected primary PID".to_owned(),
                ));
            }
            if !allowed_pids.contains(&win_pid) {
                return Err(DimError::ProcessError(format!(
                    "WINDOW_UNVERIFIED: HWND {hwnd} PID {win_pid} does not belong to profile '{}'",
                    profile.name
                )));
            }

            crate::win32_ffi::companion_unsnap_window(hwnd, restore_rect)?;
            Ok(CompanionSnapResult {
                profile_id: profile.id.clone(),
                snapped: false,
                previous_rect: Some(restore_rect),
                applied_rect: None,
                message: format!("Restored profile '{}' window position.", profile.name),
            })
        }
        #[cfg(not(windows))]
        {
            let _ = (profile, profile_dir, restore_rect);
            Err(DimError::UnsupportedPlatform(
                "companion_unsnap_secondary requires Windows".to_owned(),
            ))
        }
    }

    fn is_window_minimized(&self, hwnd: u64) -> bool {
        #[cfg(windows)]
        {
            crate::win32_ffi::is_window_minimized(hwnd)
        }
        #[cfg(not(windows))]
        {
            let _ = hwnd;
            false
        }
    }

    fn try_focus_verified_window(
        &self,
        hwnd: u64,
        allowed_pids: &[u32],
    ) -> Result<Option<(bool, bool)>, DimError> {
        #[cfg(windows)]
        {
            crate::win32_ffi::focus_window_verified(hwnd, allowed_pids).map(Some)
        }
        #[cfg(not(windows))]
        {
            let _ = (hwnd, allowed_pids);
            Ok(None)
        }
    }
}

//! `MacOsPlatformAdapter` implementing all core traits for Apple macOS (marked `NotTested`).

use std::fs;
use std::path::{Path, PathBuf};

use dim_core::{
    detect_zalo_user_id, ApplicationInstallation, ApplicationLocator, DimError, InstanceLauncher,
    InstanceMonitor, LaunchTarget, PlatformCapabilities, PlatformCapabilityReport, PlatformKind,
    PrimaryInstanceStatus, ProfileRecord, ProfileRuntimeState, SupportStatus,
};
use dim_profile_store::unlink_reparse_points_safe;

use crate::layout::{
    ensure_macos_profile_layout, extract_plist_short_version, inspect_call_sockets_at,
    macos_zalo_data_dir, parse_singleton_lock_target, MACOS_CALL_SOCKETS,
};

#[cfg(target_os = "macos")]
use crate::layout::{build_open_args, ensure_macos_user_symlinks};

/// macOS platform adapter using `open -n -a ... --env CFFIXED_USER_HOME=<profile_home>`.
#[derive(Debug, Default, Clone)]
pub struct MacOsPlatformAdapter;

impl MacOsPlatformAdapter {
    /// Creates a new macOS platform adapter.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl ApplicationLocator for MacOsPlatformAdapter {
    fn discover_installation(
        &self,
        custom_root: Option<&Path>,
    ) -> Result<ApplicationInstallation, DimError> {
        let app_bundle = custom_root
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("/Applications/Zalo.app"));

        let exec = app_bundle.join("Contents").join("MacOS").join("Zalo");
        let asar = app_bundle
            .join("Contents")
            .join("Resources")
            .join("app.asar");
        let plist = app_bundle.join("Contents").join("Info.plist");

        if !app_bundle.exists() || !exec.exists() {
            return Ok(ApplicationInstallation {
                is_valid: false,
                platform: PlatformKind::MacOs,
                install_root: app_bundle.clone(),
                launcher_path: None,
                executable_path: None,
                app_asar_path: None,
                version: None,
                signature_status: "NotFound".to_owned(),
                signer_subject: None,
                support_status: SupportStatus::NotTested,
                notes: vec![
                    "macOS adapter is unverified on physical Apple hardware in this repository."
                        .to_owned(),
                ],
                error: Some(format!(
                    "macOS Zalo.app not found at {}",
                    app_bundle.display()
                )),
            });
        }

        let version = fs::read_to_string(&plist)
            .ok()
            .and_then(|xml| extract_plist_short_version(&xml));

        Ok(ApplicationInstallation {
            is_valid: true,
            platform: PlatformKind::MacOs,
            install_root: app_bundle.clone(),
            launcher_path: Some(PathBuf::from("/usr/bin/open")),
            executable_path: Some(exec),
            app_asar_path: asar.exists().then_some(asar),
            version,
            signature_status: "NotVerifiedOnHost (use codesign -dv on macOS)".to_owned(),
            signer_subject: None,
            support_status: SupportStatus::NotTested,
            notes: vec![
                "Isolated launch uses `open -n -a ... --env CFFIXED_USER_HOME=<profile_home>`."
                    .to_owned(),
                "Concurrent calls conflict on global /tmp/socketzalorecv2021.".to_owned(),
            ],
            error: None,
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
            "Singleton isolation relies on `open -n` + `CFFIXED_USER_HOME`.".to_owned(),
            "Chromium lock parsed from `SingletonLock` symlink (`<host>-<pid>`).".to_owned(),
        ];
        diagnostics.extend(inspect_call_sockets_at(&socket_paths));

        PlatformCapabilityReport {
            platform: PlatformKind::MacOs,
            overall_status: SupportStatus::NotTested,
            official_zalo_status: SupportStatus::NotTested,
            fixture_status: SupportStatus::Experimental,
            appimage_status: None,
            concurrent_calls_status: SupportStatus::Blocked,
            folder_redirection_detected: false,
            folder_redirection_path: None,
            limitations: vec![
                "macOS CFFIXED_USER_HOME isolation is NOT_TESTED on physical macOS hardware."
                    .to_owned(),
                "ZaloHelper.app binds fixed Unix domain socket /tmp/socketzalorecv2021 (fs.unlink conflict on concurrent calls)."
                    .to_owned(),
            ],
            diagnostics,
        }
    }

    fn free_disk_bytes(&self, path: &Path) -> Result<u64, DimError> {
        #[cfg(target_os = "macos")]
        {
            unix_free_disk_bytes(path)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = path;
            Ok(dim_core::MIN_FREE_DISK_BYTES.saturating_mul(2))
        }
    }

    fn unlink_profile_junctions(&self, profile_dir: &Path) -> Result<usize, DimError> {
        unlink_reparse_points_safe(profile_dir)
    }
}

impl InstanceMonitor for MacOsPlatformAdapter {
    fn inspect_primary(&self) -> Result<PrimaryInstanceStatus, DimError> {
        #[cfg(target_os = "macos")]
        {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/tmp"));
            let zalo_data = home
                .join("Library")
                .join("Application Support")
                .join("ZaloData");
            let lock_path = zalo_data.join("SingletonLock");
            let pid = read_singleton_or_json_pid(&zalo_data);
            let alive = pid.filter(|&p| is_unix_pid_alive(p));
            let detected_uid = detect_zalo_user_id(&zalo_data);
            Ok(PrimaryInstanceStatus {
                zalo_data_dir: zalo_data,
                lockfile_path: lock_path,
                running: alive.is_some(),
                protected_pids: alive.into_iter().collect(),
                process_count: usize::from(alive.is_some()),
                working_set_bytes: 0,
                detected_zalo_user_id: detected_uid,
                notes: vec!["Primary macOS Zalo PID is protected against termination.".to_owned()],
            })
        }
        #[cfg(not(target_os = "macos"))]
        {
            Err(DimError::UnsupportedPlatform(
                "macOS primary inspection requires macOS".to_owned(),
            ))
        }
    }

    fn inspect_profile(
        &self,
        profile: &ProfileRecord,
        profile_dir: &Path,
    ) -> Result<ProfileRuntimeState, DimError> {
        let zalo_data = macos_zalo_data_dir(profile_dir);
        let singleton_lock = zalo_data.join("SingletonLock");
        let fixture_lock = zalo_data.join("lockfile");
        let detected_uid = detect_zalo_user_id(&zalo_data);

        #[cfg(target_os = "macos")]
        {
            let candidate_pid = read_singleton_or_json_pid(&zalo_data);
            let alive_pid = candidate_pid.filter(|&p| is_unix_pid_alive(p));
            let stale = alive_pid.is_none() && (singleton_lock.exists() || fixture_lock.exists());
            let lockfile_path = if singleton_lock.exists() || !fixture_lock.exists() {
                singleton_lock
            } else {
                fixture_lock
            };

            Ok(ProfileRuntimeState {
                profile_id: profile.id.clone(),
                profile_name: profile.name.clone(),
                profile_dir: profile_dir.to_path_buf(),
                zalo_data_dir: zalo_data,
                lockfile_path,
                running: alive_pid.is_some(),
                stale_lock_detected: stale,
                main_pid: alive_pid,
                process_ids: alive_pid.into_iter().collect(),
                process_count: usize::from(alive_pid.is_some()),
                working_set_bytes: 0,
                window_handle: None,
                window_title: None,
                detected_zalo_user_id: detected_uid,
                disk_usage_bytes: 0,
                status_label: if alive_pid.is_some() {
                    "Running".to_owned()
                } else if stale {
                    "Stopped (Stale Lock)".to_owned()
                } else {
                    "Stopped".to_owned()
                },
            })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (profile, singleton_lock, fixture_lock, detected_uid);
            Err(DimError::UnsupportedPlatform(
                "macOS runtime inspection requires macOS".to_owned(),
            ))
        }
    }
}

impl InstanceLauncher for MacOsPlatformAdapter {
    fn ensure_profile_layout(
        &self,
        profile: &ProfileRecord,
        profile_dir: &Path,
    ) -> Result<(), DimError> {
        ensure_macos_profile_layout(profile, profile_dir)?;
        #[cfg(target_os = "macos")]
        if let Some(real_home) = std::env::var_os("HOME").map(PathBuf::from) {
            ensure_macos_user_symlinks(profile_dir, &real_home)?;
        }
        Ok(())
    }

    fn launch(
        &self,
        target: &LaunchTarget,
        profile: &ProfileRecord,
        profile_dir: &Path,
    ) -> Result<ProfileRuntimeState, DimError> {
        self.ensure_profile_layout(profile, profile_dir)?;

        #[cfg(target_os = "macos")]
        {
            use std::process::{Command, Stdio};

            match target {
                LaunchTarget::OfficialZalo {
                    custom_install_root,
                } => {
                    let install = self.discover_installation(custom_install_root.as_deref())?;
                    if !install.is_valid {
                        return Err(DimError::InstallNotFound(
                            install
                                .error
                                .unwrap_or_else(|| "Zalo.app missing".to_owned()),
                        ));
                    }
                    let args = build_open_args(&install.install_root, profile_dir, &[]);
                    let status = Command::new("/usr/bin/open")
                        .args(&args)
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status()?;
                    if !status.success() {
                        return Err(DimError::ProcessError(format!(
                            "`open -n` exited with status {status}"
                        )));
                    }
                }
                LaunchTarget::CustomExecutable { path, args } => {
                    let child = Command::new(path)
                        .args(args)
                        .env("CFFIXED_USER_HOME", profile_dir)
                        .env("HOME", profile_dir)
                        .env("TMPDIR", profile_dir.join("tmp"))
                        .env("DIM_PROFILE_ID", &profile.id)
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn()?;
                    let _ = fs::write(profile_dir.join(".runtime-pid"), child.id().to_string());
                }
                LaunchTarget::NodeFixture {
                    node_path,
                    script_path,
                    extra_args,
                } => {
                    let node = node_path.as_deref().unwrap_or_else(|| Path::new("node"));
                    let child = Command::new(node)
                        .arg(script_path)
                        .args(extra_args)
                        .env("CFFIXED_USER_HOME", profile_dir)
                        .env("HOME", profile_dir)
                        .env("TMPDIR", profile_dir.join("tmp"))
                        .env("DIM_PROFILE_ID", &profile.id)
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn()?;
                    let _ = fs::write(profile_dir.join(".runtime-pid"), child.id().to_string());
                }
            }
            self.inspect_profile(profile, profile_dir)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = target;
            Err(DimError::UnsupportedPlatform(
                "macOS launch requires macOS".to_owned(),
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
        let _ = (target, profile, profile_dir, state);
        #[cfg(target_os = "macos")]
        {
            if let Some(pid) = state.main_pid {
                let _ = std::process::Command::new("/usr/bin/osascript")
                    .arg("-e")
                    .arg("on run argv\ntell application \"System Events\" to set frontmost of first process whose unix id is (item 1 of argv as integer) to true\nend run")
                    .arg(pid.to_string())
                    .status();
            }
            Ok(())
        }
        #[cfg(not(target_os = "macos"))]
        {
            Err(DimError::UnsupportedPlatform(
                "macOS focus_or_restore requires macOS".to_owned(),
            ))
        }
    }

    fn stop(
        &self,
        state: &ProfileRuntimeState,
        protected_primary_pids: &[u32],
        timeout_ms: u64,
    ) -> Result<u32, DimError> {
        let Some(pid) = state.main_pid else {
            return Ok(0);
        };
        if protected_primary_pids.contains(&pid) {
            return Err(DimError::ProtectedPrimaryProcess(format!(
                "refused to stop protected macOS primary PID {pid}"
            )));
        }

        #[cfg(target_os = "macos")]
        {
            let _ = fs::write(
                state.zalo_data_dir.join(".quit-signal"),
                format!("{{\"requested_at\":\"{}\"}}\n", dim_core::now_rfc3339_utc()),
            );
            // Attempt NSRunningApplication.terminate via safe numeric-arg osascript first.
            let _ = std::process::Command::new("/usr/bin/osascript")
                .arg("-e")
                .arg("use framework \"AppKit\"\non run argv\nset pidVal to (item 1 of argv) as integer\nset appRef to current application's NSRunningApplication's runningApplicationWithProcessIdentifier:pidVal\nif appRef is not missing value then appRef's terminate()\nend run")
                .arg(pid.to_string())
                .status();

            // Follow with safe isolated SIGTERM if still alive.
            if is_unix_pid_alive(pid) {
                if let Ok(raw_pid) = libc::pid_t::try_from(pid) {
                    // SAFETY: `raw_pid` was verified positive, non-primary, and fits in `pid_t`.
                    unsafe {
                        let _ = libc::kill(raw_pid, libc::SIGTERM);
                    }
                }
            }

            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
            while std::time::Instant::now() < deadline {
                if !is_unix_pid_alive(pid) {
                    return Ok(1);
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Ok(0)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = timeout_ms;
            Err(DimError::UnsupportedPlatform(
                "macOS stop requires macOS".to_owned(),
            ))
        }
    }
}

/// Reads PID from Chromium `SingletonLock` symlink or fixture JSON `lockfile`.
#[must_use]
pub fn read_singleton_or_json_pid(zalo_data_dir: &Path) -> Option<u32> {
    let lock_link = zalo_data_dir.join("SingletonLock");
    if let Ok(target) = fs::read_link(&lock_link) {
        if let Some((_, pid)) = parse_singleton_lock_target(&target.to_string_lossy()) {
            return Some(pid);
        }
    }
    let lock_json = zalo_data_dir.join("lockfile");
    if let Ok(raw) = fs::read_to_string(lock_json) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(pid) = val
                .get("pid")
                .and_then(serde_json::Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
            {
                return Some(pid);
            }
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn is_unix_pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let Ok(raw) = libc::pid_t::try_from(pid) else {
        return false;
    };
    // SAFETY: Signal 0 only probes process existence without sending a signal.
    let rc = unsafe { libc::kill(raw, 0) };
    if rc == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(target_os = "macos")]
fn unix_free_disk_bytes(path: &Path) -> Result<u64, DimError> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let probe = if path.exists() {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    let c_path = CString::new(probe.as_os_str().as_bytes())
        .map_err(|_| DimError::PathTraversal("path contains NUL byte".to_owned()))?;
    // SAFETY: `statvfs` initializes `stat` on return 0.
    unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c_path.as_ptr(), &mut stat) != 0 {
            return Err(DimError::Io(std::io::Error::last_os_error()));
        }
        Ok((stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64))
    }
}

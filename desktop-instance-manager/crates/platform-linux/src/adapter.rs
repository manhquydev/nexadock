//! `LinuxPlatformAdapter` implementing core traits for Linux.
//!
//! Official VNG Zalo Desktop is honestly reported as [`SupportStatus::Unsupported`] because VNG
//! publishes no official Linux desktop binary. User-supplied local AppImages and the test desktop
//! fixture run in [`SupportStatus::Experimental`] mode via XDG/HOME and optional `bwrap`.

use std::fs;
use std::path::{Path, PathBuf};

use dim_core::{
    detect_zalo_user_id, ApplicationInstallation, ApplicationLocator, DimError, InstanceLauncher,
    InstanceMonitor, LaunchTarget, PlatformCapabilities, PlatformCapabilityReport, PlatformKind,
    PrimaryInstanceStatus, ProfileRecord, ProfileRuntimeState, SupportStatus,
};
use dim_profile_store::unlink_reparse_points_safe;

use crate::layout::{
    ensure_linux_profile_layout, linux_zalo_data_dir, looks_like_appimage, APPARMOR_NO_SANDBOX_NOTE,
};

#[cfg(target_os = "linux")]
use crate::layout::{build_bwrap_args, build_linux_isolated_env};

/// Linux platform adapter supporting test fixtures and user-supplied local AppImages.
#[derive(Debug, Default, Clone)]
pub struct LinuxPlatformAdapter {
    prefer_bwrap: bool,
}

impl LinuxPlatformAdapter {
    /// Creates a new Linux platform adapter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            prefer_bwrap: false,
        }
    }

    /// Enables Bubblewrap (`bwrap`) sandboxing when `bwrap` is installed in `PATH`.
    #[must_use]
    pub fn with_bwrap(mut self, enabled: bool) -> Self {
        self.prefer_bwrap = enabled;
        self
    }
}

impl ApplicationLocator for LinuxPlatformAdapter {
    fn discover_installation(
        &self,
        custom_root: Option<&Path>,
    ) -> Result<ApplicationInstallation, DimError> {
        if let Some(custom) = custom_root {
            if custom.exists() && looks_like_appimage(custom) {
                return Ok(ApplicationInstallation {
                    is_valid: true,
                    platform: PlatformKind::Linux,
                    install_root: custom.to_path_buf(),
                    launcher_path: None,
                    executable_path: Some(custom.to_path_buf()),
                    app_asar_path: None,
                    version: None,
                    signature_status: "UserSuppliedAppImage (unverified)".to_owned(),
                    signer_subject: None,
                    support_status: SupportStatus::Experimental,
                    notes: vec![
                        "User-supplied local AppImage is Experimental only; we never bundle or redistribute proprietary app.asar."
                            .to_owned(),
                        APPARMOR_NO_SANDBOX_NOTE.to_owned(),
                    ],
                    error: None,
                });
            }
        }

        Ok(ApplicationInstallation {
            is_valid: false,
            platform: PlatformKind::Linux,
            install_root: custom_root
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("/opt/Zalo")),
            launcher_path: None,
            executable_path: None,
            app_asar_path: None,
            version: None,
            signature_status: "NotApplicable".to_owned(),
            signer_subject: None,
            support_status: SupportStatus::Unsupported,
            notes: vec![
                "VNG does not ship an official native Linux Zalo Desktop release.".to_owned(),
                APPARMOR_NO_SANDBOX_NOTE.to_owned(),
            ],
            error: Some(
                "Official VNG Zalo Desktop is unavailable on Linux (SupportStatus::Unsupported)."
                    .to_owned(),
            ),
        })
    }
}

impl PlatformCapabilities for LinuxPlatformAdapter {
    fn platform(&self) -> PlatformKind {
        PlatformKind::Linux
    }

    fn capabilities(&self) -> PlatformCapabilityReport {
        PlatformCapabilityReport {
            platform: PlatformKind::Linux,
            overall_status: SupportStatus::Unsupported,
            official_zalo_status: SupportStatus::Unsupported,
            fixture_status: SupportStatus::Experimental,
            appimage_status: Some(SupportStatus::Experimental),
            concurrent_calls_status: SupportStatus::Unsupported,
            folder_redirection_detected: false,
            folder_redirection_path: None,
            limitations: vec![
                "Official VNG Zalo Desktop is UNSUPPORTED on Linux (no official VNG build exists)."
                    .to_owned(),
                "User-supplied local AppImage and Node.js test fixture run in EXPERIMENTAL XDG/HOME/bwrap isolation."
                    .to_owned(),
                APPARMOR_NO_SANDBOX_NOTE.to_owned(),
            ],
            diagnostics: vec![
                format!("prefer_bwrap={}", self.prefer_bwrap),
                "XDG_CONFIG_HOME / XDG_DATA_HOME / XDG_CACHE_HOME / HOME isolation".to_owned(),
            ],
        }
    }

    fn free_disk_bytes(&self, path: &Path) -> Result<u64, DimError> {
        #[cfg(target_os = "linux")]
        {
            unix_free_disk_bytes(path)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = path;
            Ok(dim_core::MIN_FREE_DISK_BYTES.saturating_mul(2))
        }
    }

    fn unlink_profile_junctions(&self, profile_dir: &Path) -> Result<usize, DimError> {
        unlink_reparse_points_safe(profile_dir)
    }
}

impl InstanceMonitor for LinuxPlatformAdapter {
    fn inspect_primary(&self) -> Result<PrimaryInstanceStatus, DimError> {
        #[cfg(target_os = "linux")]
        {
            let config_base = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
                .unwrap_or_else(|| PathBuf::from("/tmp"));
            let zalo_data = config_base.join("ZaloData");
            let lock_path = zalo_data.join("SingletonLock");
            let pid = read_linux_pid(&zalo_data).filter(|&p| is_unix_pid_alive(p));
            let detected_uid = detect_zalo_user_id(&zalo_data);
            Ok(PrimaryInstanceStatus {
                zalo_data_dir: zalo_data,
                lockfile_path: lock_path,
                running: pid.is_some(),
                protected_pids: pid.into_iter().collect(),
                process_count: usize::from(pid.is_some()),
                working_set_bytes: 0,
                detected_zalo_user_id: detected_uid,
                notes: vec!["Primary Linux config dir PIDs are protected.".to_owned()],
            })
        }
        #[cfg(not(target_os = "linux"))]
        {
            Err(DimError::UnsupportedPlatform(
                "Linux primary inspection requires Linux".to_owned(),
            ))
        }
    }

    fn inspect_profile(
        &self,
        profile: &ProfileRecord,
        profile_dir: &Path,
    ) -> Result<ProfileRuntimeState, DimError> {
        let zalo_data = linux_zalo_data_dir(profile_dir);
        let singleton_lock = zalo_data.join("SingletonLock");
        let fixture_lock = zalo_data.join("lockfile");
        let detected_uid = detect_zalo_user_id(&zalo_data);

        #[cfg(target_os = "linux")]
        {
            let pid = read_linux_pid(&zalo_data).filter(|&p| is_unix_pid_alive(p));
            let stale = pid.is_none() && (singleton_lock.exists() || fixture_lock.exists());
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
                running: pid.is_some(),
                stale_lock_detected: stale,
                main_pid: pid,
                process_ids: pid.into_iter().collect(),
                process_count: usize::from(pid.is_some()),
                working_set_bytes: 0,
                window_handle: None,
                window_title: None,
                detected_zalo_user_id: detected_uid,
                disk_usage_bytes: 0,
                status_label: if pid.is_some() {
                    "Running".to_owned()
                } else if stale {
                    "Stopped (Stale Lock)".to_owned()
                } else {
                    "Stopped".to_owned()
                },
            })
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (profile, singleton_lock, fixture_lock, detected_uid);
            Err(DimError::UnsupportedPlatform(
                "Linux runtime inspection requires Linux".to_owned(),
            ))
        }
    }
}

impl InstanceLauncher for LinuxPlatformAdapter {
    fn ensure_profile_layout(
        &self,
        profile: &ProfileRecord,
        profile_dir: &Path,
    ) -> Result<(), DimError> {
        ensure_linux_profile_layout(profile, profile_dir)
    }

    fn launch(
        &self,
        target: &LaunchTarget,
        profile: &ProfileRecord,
        profile_dir: &Path,
    ) -> Result<ProfileRuntimeState, DimError> {
        if matches!(target, LaunchTarget::OfficialZalo { .. }) {
            return Err(DimError::UnsupportedPlatform(
                "Official VNG Zalo Desktop does not exist for Linux (SupportStatus::Unsupported)"
                    .to_owned(),
            ));
        }

        ensure_linux_profile_layout(profile, profile_dir)?;

        #[cfg(target_os = "linux")]
        {
            use std::process::{Command, Stdio};

            let (exe, args) = match target {
                LaunchTarget::CustomExecutable { path, args } => (path.clone(), args.clone()),
                LaunchTarget::NodeFixture {
                    node_path,
                    script_path,
                    extra_args,
                } => {
                    let node = node_path
                        .as_deref()
                        .unwrap_or_else(|| Path::new("node"))
                        .to_path_buf();
                    let mut full = vec![script_path.to_string_lossy().into_owned()];
                    full.extend(extra_args.iter().cloned());
                    (node, full)
                }
                LaunchTarget::OfficialZalo { .. } => unreachable!(),
            };

            let env_map = build_linux_isolated_env(profile, profile_dir);
            let child = if self.prefer_bwrap && find_in_path("bwrap").is_some() {
                let bwrap_args = build_bwrap_args(profile_dir, &exe, &args);
                Command::new("bwrap")
                    .args(&bwrap_args)
                    .envs(&env_map)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()?
            } else {
                Command::new(&exe)
                    .args(&args)
                    .envs(&env_map)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()?
            };

            let _ = fs::write(profile_dir.join(".runtime-pid"), child.id().to_string());
            self.inspect_profile(profile, profile_dir)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = target;
            Err(DimError::UnsupportedPlatform(
                "Linux launch requires Linux".to_owned(),
            ))
        }
    }

    fn focus_or_restore(
        &self,
        _target: &LaunchTarget,
        _profile: &ProfileRecord,
        _profile_dir: &Path,
        _state: &ProfileRuntimeState,
    ) -> Result<(), DimError> {
        Ok(())
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
                "refused to stop protected Linux primary PID {pid}"
            )));
        }

        #[cfg(target_os = "linux")]
        {
            let _ = fs::write(
                state.zalo_data_dir.join(".quit-signal"),
                format!("{{\"requested_at\":\"{}\"}}\n", dim_core::now_rfc3339_utc()),
            );

            if let Ok(raw_pid) = libc::pid_t::try_from(pid) {
                // SAFETY: `raw_pid` is positive and verified non-primary.
                unsafe {
                    let _ = libc::kill(raw_pid, libc::SIGTERM);
                }
                let deadline =
                    std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
                while std::time::Instant::now() < deadline {
                    if !is_unix_pid_alive(pid) {
                        return Ok(1);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                // SAFETY: Timeout elapsed and process is strictly isolated non-primary.
                unsafe {
                    let _ = libc::kill(raw_pid, libc::SIGKILL);
                }
                return Ok(1);
            }
            Ok(0)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = timeout_ms;
            Err(DimError::UnsupportedPlatform(
                "Linux stop requires Linux".to_owned(),
            ))
        }
    }
}

/// Reads PID from Linux `SingletonLock` symlink (`<host>-<pid>`) or fixture `lockfile` JSON.
#[must_use]
pub fn read_linux_pid(zalo_data_dir: &Path) -> Option<u32> {
    if let Ok(target) = fs::read_link(zalo_data_dir.join("SingletonLock")) {
        let target_str = target.to_string_lossy();
        if let Some((_, pid_str)) = target_str.trim().rsplit_once('-') {
            if let Ok(pid) = pid_str.parse::<u32>() {
                if pid > 0 {
                    return Some(pid);
                }
            }
        }
    }
    if let Ok(raw) = fs::read_to_string(zalo_data_dir.join("lockfile")) {
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

#[cfg(target_os = "linux")]
fn find_in_path(bin: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let cand = dir.join(bin);
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn is_unix_pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let Ok(raw) = libc::pid_t::try_from(pid) else {
        return false;
    };
    // SAFETY: Signal 0 checks PID liveness without affecting the target process.
    let rc = unsafe { libc::kill(raw, 0) };
    if rc == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(target_os = "linux")]
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
    // SAFETY: `statvfs` initializes `stat` on success.
    unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c_path.as_ptr(), &mut stat) != 0 {
            return Err(DimError::Io(std::io::Error::last_os_error()));
        }
        Ok((stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64))
    }
}

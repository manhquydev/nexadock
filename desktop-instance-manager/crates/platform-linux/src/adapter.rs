//! `LinuxPlatformAdapter` implementing `dim-core` traits for Ubuntu/Linux desktop environments.
//!
//! Architectural boundaries:
//! - Official VNG Zalo Desktop is honestly reported as [`SupportStatus::Unsupported`]
//!   (`OFFICIAL_ZALO_UNSUPPORTED`) because VNG publishes no Linux client.
//! - Compatible local desktop applications, user-supplied `.AppImage` bundles, and multi-instance
//!   test fixtures run under isolated `HOME` + XDG Base Directories (`0o700`) and optional
//!   `bubblewrap` (`bwrap`) namespaces in [`SupportStatus::Experimental`] mode.
//! - Security controls (`AppArmor`, `chrome-sandbox`, non-root execution) are strictly preserved;
//!   `--no-sandbox`, `killall`, `pkill`, and root execution are rejected with `SECURITY_BLOCKED`.

use std::fs;
use std::path::{Path, PathBuf};

use dim_core::{
    detect_zalo_user_id, ensure_not_primary_zalo_path, now_rfc3339_utc, validate_directory_input,
    ApplicationInstallation, ApplicationLocator, DimError, InstanceLauncher, InstanceMonitor,
    LaunchTarget, PlatformCapabilities, PlatformCapabilityReport, PlatformKind,
    PrimaryInstanceStatus, ProfileRecord, ProfileRuntimeState, SupportStatus,
};
use dim_profile_store::{is_symlink_or_reparse, unlink_reparse_points_safe};

use crate::discovery::{
    discover_compatible_apps_from_env, inspect_user_binary, BinaryFormatKind,
    BinaryInspectionReport, DiscoveredLinuxApp,
};
#[cfg(unix)]
use crate::isolation::current_effective_uid;
use crate::isolation::{
    enforce_launch_security_policy, probe_ubuntu_runtime_at, UbuntuRuntimeProbe,
};
use crate::layout::{
    ensure_linux_profile_layout, linux_zalo_data_dir, looks_like_appimage,
    measure_profile_disk_usage_bytes, verify_directory_ownership_and_mode,
    APPARMOR_NO_SANDBOX_NOTE,
};
use crate::procfs::{collect_primary_process_tree, collect_profile_process_tree, read_lock_pid};

#[cfg(target_os = "linux")]
use crate::isolation::find_executable_in_path;
#[cfg(target_os = "linux")]
use crate::layout::{build_bwrap_desktop_args, build_sanitized_linux_spawn_env};
#[cfg(target_os = "linux")]
use crate::procfs::parse_wmctrl_windows_for_pids;

/// Virtualized filesystem/procfs overrides for deterministic unit & cross-platform testing.
#[derive(Debug, Clone)]
struct VirtualLinuxEnv {
    fs_root: PathBuf,
    proc_root: PathBuf,
    primary_config_dir: PathBuf,
    effective_uid: u32,
    arch: String,
    bwrap_path: Option<PathBuf>,
}

/// Linux platform adapter managing isolated XDG/HOME profiles, `/proc` process trees,
/// binary provenance checks, and Ubuntu AppArmor security preflights.
#[derive(Debug, Default, Clone)]
pub struct LinuxPlatformAdapter {
    prefer_bwrap: bool,
    pinned_sha256: Option<String>,
    virtual_env: Option<VirtualLinuxEnv>,
}

impl LinuxPlatformAdapter {
    /// Creates a new Linux platform adapter using standard unprivileged `HOME` + XDG isolation.
    #[must_use]
    pub fn new() -> Self {
        Self {
            prefer_bwrap: false,
            pinned_sha256: None,
            virtual_env: None,
        }
    }

    /// Requests Bubblewrap (`bwrap`) mount/IPC sandboxing on launch.
    ///
    /// If `bwrap` is unavailable or blocked by kernel/AppArmor policy, launch fails closed with
    /// `SECURITY_BLOCKED` rather than silently dropping sandbox expectations.
    #[must_use]
    pub fn with_bwrap(mut self, enabled: bool) -> Self {
        self.prefer_bwrap = enabled;
        self
    }

    /// Pins an expected lowercase hex SHA-256 checksum for user-supplied binary verification.
    #[must_use]
    pub fn with_expected_sha256(mut self, sha256_hex: impl Into<String>) -> Self {
        self.pinned_sha256 = Some(sha256_hex.into());
        self
    }

    /// Constructs a deterministic virtualized adapter over synthetic `/`, `/proc`, and `~/.config`
    /// trees for cross-platform integration tests without touching host OS state.
    #[must_use]
    pub fn with_virtual_roots(
        fs_root: PathBuf,
        proc_root: PathBuf,
        primary_config_dir: PathBuf,
        effective_uid: u32,
        arch: impl Into<String>,
        bwrap_path: Option<PathBuf>,
    ) -> Self {
        Self {
            prefer_bwrap: false,
            pinned_sha256: None,
            virtual_env: Some(VirtualLinuxEnv {
                fs_root,
                proc_root,
                primary_config_dir,
                effective_uid,
                arch: arch.into(),
                bwrap_path,
            }),
        }
    }

    /// Probes Ubuntu distribution version, AppArmor `userns` restrictions, Wayland/X11 session,
    /// PipeWire/PulseAudio sockets, FUSE device, and effective UID.
    #[must_use]
    pub fn probe_ubuntu_environment(&self) -> UbuntuRuntimeProbe {
        if let Some(venv) = &self.virtual_env {
            return probe_ubuntu_runtime_at(
                &venv.fs_root,
                &venv.arch,
                venv.effective_uid,
                &|_k| None,
                venv.bwrap_path.clone(),
            );
        }
        crate::isolation::probe_live_ubuntu_runtime()
    }

    /// Scans standard XDG `.desktop` directories and user AppImage folders for compatible local binaries.
    #[must_use]
    pub fn discover_compatible_applications(&self) -> Vec<DiscoveredLinuxApp> {
        discover_compatible_apps_from_env(&|k| std::env::var(k).ok())
    }

    /// Performs format, permission, startup-script, `ldd`, and SHA-256 inspection on a local binary.
    pub fn inspect_local_binary(&self, path: &Path) -> Result<BinaryInspectionReport, DimError> {
        inspect_user_binary(path, self.pinned_sha256.as_deref())
    }

    /// Prepares a restored profile directory on Linux by recreating skipped `tmp/` directories
    /// and enforcing `0o700` POSIX permissions.
    pub fn prepare_restored_profile(
        &self,
        profile: &ProfileRecord,
        profile_dir: &Path,
    ) -> Result<(), DimError> {
        ensure_linux_profile_layout(profile, profile_dir)
    }

    #[cfg(target_os = "linux")]
    fn effective_uid(&self) -> u32 {
        self.virtual_env
            .as_ref()
            .map_or_else(current_effective_uid, |v| v.effective_uid)
    }

    fn active_fs_root(&self) -> &Path {
        self.virtual_env
            .as_ref()
            .map_or_else(|| Path::new("/"), |v| v.fs_root.as_path())
    }
}

impl ApplicationLocator for LinuxPlatformAdapter {
    fn discover_installation(
        &self,
        custom_root: Option<&Path>,
    ) -> Result<ApplicationInstallation, DimError> {
        if let Some(custom) = custom_root {
            validate_directory_input(custom)?;
            ensure_not_primary_zalo_path(custom)?;

            if custom.exists() {
                let report = inspect_user_binary(custom, self.pinned_sha256.as_deref())?;
                if let Some(block) = &report.security_block {
                    return Ok(ApplicationInstallation {
                        is_valid: false,
                        platform: PlatformKind::Linux,
                        install_root: custom.to_path_buf(),
                        launcher_path: None,
                        executable_path: Some(report.path),
                        app_asar_path: None,
                        version: report.inferred_version,
                        signature_status: "SECURITY_BLOCKED".to_owned(),
                        signer_subject: None,
                        support_status: SupportStatus::Blocked,
                        notes: report.notes,
                        error: Some(block.to_audit_message()),
                    });
                }

                let is_recognized = looks_like_appimage(custom)
                    || !matches!(report.format, BinaryFormatKind::Unknown);

                if is_recognized {
                    let sha_short = report
                        .sha256
                        .as_deref()
                        .map(|h| &h[..16.min(h.len())])
                        .unwrap_or("n/a");
                    let provenance_tag = if report.matched_community_release.is_some() {
                        "CommunityReleaseDigestMatch (UNOFFICIAL, NOT VNG SIGNED)"
                    } else {
                        "UserSuppliedLocalBinary (unverified)"
                    };
                    let sig_status = format!(
                        "{provenance_tag} [format={}, sha256_prefix={sha_short}]",
                        report.format.as_str()
                    );

                    let mut notes = vec![
                        "User-supplied local binary/AppImage runs in Experimental isolation; proprietary VNG app.asar is never bundled or redistributed."
                            .to_owned(),
                        APPARMOR_NO_SANDBOX_NOTE.to_owned(),
                    ];
                    if matches!(report.format, BinaryFormatKind::Unknown) {
                        notes.push(
                            "Warning: file has .AppImage extension but lacks ELF64/AppImage header magic."
                                .to_owned(),
                        );
                    }
                    for miss in &report.missing_libraries {
                        notes.push(format!(
                            "Missing shared library '{}': install `{}` (Ubuntu 24.04) or `{}` (Ubuntu 22.04).",
                            miss.soname, miss.ubuntu_2404_package, miss.ubuntu_2204_package
                        ));
                    }
                    notes.extend(report.notes);

                    return Ok(ApplicationInstallation {
                        is_valid: true,
                        platform: PlatformKind::Linux,
                        install_root: custom.to_path_buf(),
                        launcher_path: None,
                        executable_path: Some(report.path),
                        app_asar_path: None,
                        version: report.inferred_version,
                        signature_status: sig_status,
                        signer_subject: None,
                        support_status: SupportStatus::Experimental,
                        notes,
                        error: None,
                    });
                }
            }
        }

        let mut notes = vec![
            "OFFICIAL_ZALO_UNSUPPORTED: VNG does not ship an official native Linux Zalo Desktop release."
                .to_owned(),
            APPARMOR_NO_SANDBOX_NOTE.to_owned(),
        ];
        if custom_root.is_none() {
            let candidates = self.discover_compatible_applications();
            for cand in candidates.iter().take(5) {
                notes.push(format!(
                    "Compatible local community candidate detected (NOT official VNG): '{}' at {} ({})",
                    cand.name,
                    cand.executable_path.display(),
                    cand.format.as_str()
                ));
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
            signature_status: "NotApplicable (OFFICIAL_ZALO_UNSUPPORTED)".to_owned(),
            signer_subject: None,
            support_status: SupportStatus::Unsupported,
            notes,
            error: Some(
                "Official VNG Zalo Desktop is unavailable on Linux (OFFICIAL_ZALO_UNSUPPORTED / SupportStatus::Unsupported)."
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
        let probe = self.probe_ubuntu_environment();
        let mut diagnostics = vec![
            format!("prefer_bwrap={}", self.prefer_bwrap),
            format!(
                "distro={} version={} session={} euid={}",
                probe.distro_id,
                probe.version_id,
                probe.display_session.as_str(),
                probe.effective_uid
            ),
            format!(
                "apparmor_enabled={} restrict_unprivileged_userns={:?} bwrap_profile={}",
                probe.apparmor_enabled,
                probe.apparmor_restrict_unprivileged_userns,
                probe.bwrap_apparmor_profile_present
            ),
            format!(
                "pipewire_socket={} pulse_socket={} dev_fuse={} camera_video0={}",
                probe.pipewire_socket_present,
                probe.pulse_socket_present,
                probe.dev_fuse_present,
                probe.camera_device_present
            ),
            "Isolation mode: separate HOME + XDG_CONFIG_HOME/XDG_DATA_HOME/XDG_CACHE_HOME/XDG_STATE_HOME + user-dirs.dirs + 0o700 (data separation; optional bwrap for filesystem/IPC sandboxing)"
                .to_owned(),
        ];
        if let Some(de) = &probe.desktop_environment {
            diagnostics.push(format!("desktop_environment={de}"));
        }

        PlatformCapabilityReport {
            platform: PlatformKind::Linux,
            overall_status: SupportStatus::Unsupported,
            official_zalo_status: SupportStatus::Unsupported,
            fixture_status: SupportStatus::Experimental,
            appimage_status: Some(SupportStatus::Experimental),
            concurrent_calls_status: SupportStatus::Blocked,
            folder_redirection_detected: false,
            folder_redirection_path: None,
            limitations: vec![
                "OFFICIAL_ZALO_UNSUPPORTED: Official VNG Zalo Desktop does not exist on Linux."
                    .to_owned(),
                "User-supplied local AppImage and Node.js/Rust test fixtures run in EXPERIMENTAL HOME/XDG/bwrap isolation."
                    .to_owned(),
                APPARMOR_NO_SANDBOX_NOTE.to_owned(),
                "Community port `VN-Linux-Family/zalo-for-linux` binds hardcoded TCP `127.0.0.1:29631` and `127.0.0.1:29632` for zcall; concurrent calls across instances in shared host networking are BLOCKED."
                    .to_owned(),
                "Native Wayland (GNOME Mutter on Ubuntu 24.04) restricts arbitrary external window activation; focus uses single-instance restore handoff on Wayland and PID-verified XID activation on X11."
                    .to_owned(),
            ],
            diagnostics,
        }
    }

    fn free_disk_bytes(&self, path: &Path) -> Result<u64, DimError> {
        #[cfg(target_os = "linux")]
        {
            if self.virtual_env.is_none() {
                return unix_free_disk_bytes(path);
            }
        }
        let _ = path;
        Ok(dim_core::MIN_FREE_DISK_BYTES.saturating_mul(4))
    }

    fn unlink_profile_junctions(&self, profile_dir: &Path) -> Result<usize, DimError> {
        unlink_reparse_points_safe(profile_dir)
    }

    fn is_elevated(&self) -> Option<bool> {
        if let Some(venv) = &self.virtual_env {
            return Some(venv.effective_uid == 0);
        }
        #[cfg(unix)]
        {
            Some(current_effective_uid() == 0)
        }
        #[cfg(not(unix))]
        {
            None
        }
    }

    fn check_profiles_acl_safe(&self, profiles_dir: &Path) -> Option<bool> {
        if !profiles_dir.exists() {
            return None;
        }
        let Ok(meta) = fs::symlink_metadata(profiles_dir) else {
            return Some(false);
        };
        if is_symlink_or_reparse(&meta) || !meta.is_dir() {
            return Some(false);
        }
        if let Some(venv) = &self.virtual_env {
            return Some(
                verify_directory_ownership_and_mode(profiles_dir, venv.effective_uid, false)
                    .is_ok(),
            );
        }
        #[cfg(unix)]
        {
            Some(
                verify_directory_ownership_and_mode(profiles_dir, current_effective_uid(), false)
                    .is_ok(),
            )
        }
        #[cfg(not(unix))]
        {
            None
        }
    }
}

impl InstanceMonitor for LinuxPlatformAdapter {
    fn inspect_primary(&self) -> Result<PrimaryInstanceStatus, DimError> {
        if let Some(venv) = &self.virtual_env {
            let zalo_data = venv.primary_config_dir.join("ZaloData");
            let lock_path = zalo_data.join("SingletonLock");
            let tree =
                collect_primary_process_tree(&venv.proc_root, &zalo_data, Some(venv.effective_uid));
            let detected_uid = detect_zalo_user_id(&zalo_data);
            return Ok(PrimaryInstanceStatus {
                zalo_data_dir: zalo_data,
                lockfile_path: lock_path,
                running: tree.main_pid.is_some(),
                protected_pids: tree.process_ids.clone(),
                process_count: tree.process_ids.len(),
                working_set_bytes: tree.working_set_bytes,
                detected_zalo_user_id: detected_uid,
                notes: vec!["Primary Linux XDG config ZaloData PIDs are protected.".to_owned()],
            });
        }

        #[cfg(target_os = "linux")]
        {
            let config_base = std::env::var_os("XDG_CONFIG_HOME")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var_os("HOME")
                        .filter(|v| !v.is_empty())
                        .map(|h| PathBuf::from(h).join(".config"))
                })
                .unwrap_or_else(|| PathBuf::from("/tmp"));
            let zalo_data = config_base.join("ZaloData");
            let lock_path = zalo_data.join("SingletonLock");
            let euid = current_effective_uid();
            let tree = collect_primary_process_tree(Path::new("/proc"), &zalo_data, Some(euid));
            let detected_uid = detect_zalo_user_id(&zalo_data);

            Ok(PrimaryInstanceStatus {
                zalo_data_dir: zalo_data,
                lockfile_path: lock_path,
                running: tree.main_pid.is_some(),
                protected_pids: tree.process_ids.clone(),
                process_count: tree.process_ids.len(),
                working_set_bytes: tree.working_set_bytes,
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

        if let Some(venv) = &self.virtual_env {
            let tree = collect_profile_process_tree(
                &venv.proc_root,
                &profile.id,
                profile_dir,
                &zalo_data,
                Some(venv.effective_uid),
            );
            let singleton_present = fs::symlink_metadata(&singleton_lock).is_ok();
            let fixture_present = fs::symlink_metadata(&fixture_lock).is_ok();
            let stale = tree.main_pid.is_none() && (singleton_present || fixture_present);
            let lockfile_path = if singleton_present || !fixture_present {
                singleton_lock
            } else {
                fixture_lock
            };
            let disk_usage_bytes = measure_profile_disk_usage_bytes(profile_dir);

            return Ok(ProfileRuntimeState {
                profile_id: profile.id.clone(),
                profile_name: profile.name.clone(),
                profile_dir: profile_dir.to_path_buf(),
                zalo_data_dir: zalo_data,
                lockfile_path,
                running: tree.main_pid.is_some(),
                stale_lock_detected: stale,
                main_pid: tree.main_pid,
                process_ids: tree.process_ids.clone(),
                process_count: tree.process_ids.len(),
                working_set_bytes: tree.working_set_bytes,
                window_handle: None,
                window_title: None,
                detected_zalo_user_id: detected_uid,
                disk_usage_bytes,
                status_label: if tree.main_pid.is_some() {
                    "Running".to_owned()
                } else if stale {
                    "Stopped (Stale Lock)".to_owned()
                } else {
                    "Stopped".to_owned()
                },
            });
        }

        #[cfg(target_os = "linux")]
        {
            let euid = current_effective_uid();
            let tree = collect_profile_process_tree(
                Path::new("/proc"),
                &profile.id,
                profile_dir,
                &zalo_data,
                Some(euid),
            );
            // Note: `SingletonLock` is typically a dangling symlink (`<host>-<pid>`), so `exists()`
            // returns `false` on Unix! `symlink_metadata` accurately detects dangling symlinks.
            let singleton_present = fs::symlink_metadata(&singleton_lock).is_ok();
            let fixture_present = fs::symlink_metadata(&fixture_lock).is_ok();
            let stale = tree.main_pid.is_none() && (singleton_present || fixture_present);
            let lockfile_path = if singleton_present || !fixture_present {
                singleton_lock
            } else {
                fixture_lock
            };

            let (window_handle, window_title) = if tree.main_pid.is_some() {
                query_x11_window_for_pids(&tree.process_ids)
            } else {
                (None, None)
            };
            let disk_usage_bytes = measure_profile_disk_usage_bytes(profile_dir);

            Ok(ProfileRuntimeState {
                profile_id: profile.id.clone(),
                profile_name: profile.name.clone(),
                profile_dir: profile_dir.to_path_buf(),
                zalo_data_dir: zalo_data,
                lockfile_path,
                running: tree.main_pid.is_some(),
                stale_lock_detected: stale,
                main_pid: tree.main_pid,
                process_ids: tree.process_ids.clone(),
                process_count: tree.process_ids.len(),
                working_set_bytes: tree.working_set_bytes,
                window_handle,
                window_title,
                detected_zalo_user_id: detected_uid,
                disk_usage_bytes,
                status_label: if tree.main_pid.is_some() {
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
                "OFFICIAL_ZALO_UNSUPPORTED: Official VNG Zalo Desktop does not exist for Linux"
                    .to_owned(),
            ));
        }

        let probe = self.probe_ubuntu_environment();
        if probe.effective_uid == 0 {
            return Err(DimError::ProcessError(
                crate::status::SecurityBlockReason::RunningAsRoot { euid: 0 }.to_audit_message(),
            ));
        }

        ensure_linux_profile_layout(profile, profile_dir)?;

        // Perform portable preflight validation across both virtualized tests and live Linux.
        let (exe, args, is_electron_appimage) = match target {
            LaunchTarget::CustomExecutable { path, args } => {
                validate_directory_input(path)?;
                ensure_not_primary_zalo_path(path)?;
                let report = inspect_user_binary(path, self.pinned_sha256.as_deref())?;
                if let Some(block) = report.security_block {
                    return Err(DimError::ProcessError(block.to_audit_message()));
                }
                if !report.is_executable {
                    return Err(DimError::ProcessError(format!(
                        "binary is not marked executable (+x): {}",
                        report.path.display()
                    )));
                }
                if let Some(arch) = report.arch {
                    if !arch.matches_current_arch() {
                        return Err(DimError::UnsupportedPlatform(format!(
                            "binary CPU architecture '{}' is incompatible with host '{}'",
                            arch.as_str(),
                            std::env::consts::ARCH
                        )));
                    }
                }
                let is_appimage = matches!(
                    report.format,
                    BinaryFormatKind::AppImageType2 | BinaryFormatKind::AppImageUruntimeDwarfs
                );
                (report.path, args.clone(), is_appimage)
            }
            LaunchTarget::NodeFixture {
                node_path,
                script_path,
                extra_args,
            } => {
                validate_directory_input(script_path)?;
                if !script_path.is_file() {
                    return Err(DimError::InstallNotFound(format!(
                        "fixture script not found: {}",
                        script_path.display()
                    )));
                }
                let resolved_node = if let Some(explicit_node) = node_path {
                    validate_directory_input(explicit_node)?;
                    if !explicit_node.is_file() {
                        return Err(DimError::InstallNotFound(format!(
                            "specified node runtime not found: {}",
                            explicit_node.display()
                        )));
                    }
                    explicit_node.clone()
                } else {
                    #[cfg(target_os = "linux")]
                    {
                        find_executable_in_path("node").ok_or_else(|| {
                            DimError::InstallNotFound(
                                "missing dependency: `node` runtime not found in PATH".to_owned(),
                            )
                        })?
                    }
                    #[cfg(not(target_os = "linux"))]
                    {
                        PathBuf::from("node")
                    }
                };
                let mut full = vec![script_path.to_string_lossy().into_owned()];
                full.extend(extra_args.iter().cloned());
                (resolved_node, full, false)
            }
            LaunchTarget::OfficialZalo { .. } => unreachable!(),
        };

        enforce_launch_security_policy(
            &probe,
            self.active_fs_root(),
            &exe,
            &args,
            is_electron_appimage,
            self.prefer_bwrap,
        )
        .map_err(|reason| DimError::ProcessError(reason.to_audit_message()))?;

        #[cfg(target_os = "linux")]
        {
            use std::process::{Command, Stdio};

            verify_directory_ownership_and_mode(profile_dir, self.effective_uid(), true)
                .map_err(|reason| DimError::ProcessError(reason.to_audit_message()))?;

            let env_map =
                build_sanitized_linux_spawn_env(profile, profile_dir, &|k| std::env::var(k).ok());

            let mut child = if self.prefer_bwrap {
                let bwrap_bin = probe.bwrap_path.ok_or_else(|| {
                    DimError::InstallNotFound(
                        "bubblewrap (`bwrap`) requested but not installed in PATH".to_owned(),
                    )
                })?;
                let bwrap_args =
                    build_bwrap_desktop_args(profile_dir, &exe, &args, &|k| std::env::var(k).ok());
                Command::new(bwrap_bin)
                    .args(&bwrap_args)
                    .env_clear()
                    .envs(&env_map)
                    .current_dir(profile_dir)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()?
            } else {
                Command::new(&exe)
                    .args(&args)
                    .env_clear()
                    .envs(&env_map)
                    .current_dir(profile_dir)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()?
            };

            let spawned_pid = child.id();
            let runtime_hint = serde_json::json!({
                "pid": spawned_pid,
                "profile_id": profile.id,
                "started_at": now_rfc3339_utc(),
            });
            let _ = fs::write(
                profile_dir.join(".runtime-pid"),
                format!("{runtime_hint}\n"),
            );

            // Wait up to 1,500 ms for `SingletonLock` or `lockfile` creation (exiting early if ready or exited).
            let zalo_data = linux_zalo_data_dir(profile_dir);
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1_500);
            while std::time::Instant::now() < deadline {
                if fs::symlink_metadata(zalo_data.join("SingletonLock")).is_ok()
                    || fs::symlink_metadata(zalo_data.join("lockfile")).is_ok()
                {
                    break;
                }
                if let Ok(Some(_exit_status)) = child.try_wait() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }

            self.inspect_profile(profile, profile_dir)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (exe, args);
            Err(DimError::UnsupportedPlatform(
                "Linux launch requires Linux".to_owned(),
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
        if !state.running {
            return Err(DimError::NotRunning(profile.name.clone()));
        }

        // Write `.focus-signal` atomically whenever focusing a fixture or hybrid target.
        let is_fixture = matches!(target, LaunchTarget::NodeFixture { .. })
            || state.zalo_data_dir.join("lockfile").is_file();
        if is_fixture {
            let focus_path = state.zalo_data_dir.join(".focus-signal");
            let tmp_path = state.zalo_data_dir.join(".focus-signal.tmp");
            let payload = format!("{{\"focused_at\":\"{}\"}}\n", now_rfc3339_utc());
            fs::write(&tmp_path, payload)?;
            fs::rename(&tmp_path, &focus_path)?;
        }

        #[cfg(target_os = "linux")]
        {
            if self.virtual_env.is_none() && !is_fixture {
                let _ = try_activate_linux_window(target, profile, profile_dir, state);
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (target, profile_dir);
        }

        Ok(())
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

        // Strict primary-process guard across main_pid AND every descendant PID.
        if protected_primary_pids.contains(&main_pid)
            || state
                .process_ids
                .iter()
                .any(|p| protected_primary_pids.contains(p))
        {
            return Err(DimError::ProtectedPrimaryProcess(format!(
                "refused to stop Linux process tree overlapping protected primary PID(s): main_pid={main_pid}"
            )));
        }

        #[cfg(target_os = "linux")]
        {
            let _ = fs::write(
                state.zalo_data_dir.join(".quit-signal"),
                format!("{{\"requested_at\":\"{}\"}}\n", now_rfc3339_utc()),
            );

            let euid = current_effective_uid();
            let initial_tree = collect_profile_process_tree(
                Path::new("/proc"),
                &state.profile_id,
                &state.profile_dir,
                &state.zalo_data_dir,
                Some(euid),
            );

            let targets = if initial_tree.process_ids.is_empty() {
                vec![main_pid]
            } else {
                initial_tree.process_ids
            };
            let initial_count = u32::try_from(targets.len()).unwrap_or(1);

            // Phase 1: Send graceful SIGTERM strictly to the verified root process first so
            // Electron's `before-quit` / `will-quit` flushes SQLite WALs and hangs up calls cleanly.
            send_signal_verified(main_pid, libc::SIGTERM, protected_primary_pids);

            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
            while std::time::Instant::now() < deadline {
                reap_direct_children(&targets);
                let live = collect_profile_process_tree(
                    Path::new("/proc"),
                    &state.profile_id,
                    &state.profile_dir,
                    &state.zalo_data_dir,
                    Some(euid),
                );
                if live.process_ids.is_empty() && !is_unix_pid_active_non_zombie(main_pid) {
                    cleanup_post_stop_hints(&state.profile_dir, &state.zalo_data_dir);
                    return Ok(initial_count);
                }
                std::thread::sleep(std::time::Duration::from_millis(40));
            }

            // Phase 2: Escalate to SIGKILL ONLY on remaining verified profile-owned PIDs (leaves-first),
            // NEVER invoking `killall` or `pkill`.
            let remaining = collect_profile_process_tree(
                Path::new("/proc"),
                &state.profile_id,
                &state.profile_dir,
                &state.zalo_data_dir,
                Some(euid),
            );
            for &pid in remaining.process_ids.iter().rev() {
                send_signal_verified(pid, libc::SIGKILL, protected_primary_pids);
            }
            if remaining.process_ids.is_empty() && is_unix_pid_active_non_zombie(main_pid) {
                send_signal_verified(main_pid, libc::SIGKILL, protected_primary_pids);
            }

            std::thread::sleep(std::time::Duration::from_millis(40));
            reap_direct_children(&targets);
            cleanup_post_stop_hints(&state.profile_dir, &state.zalo_data_dir);
            Ok(initial_count)
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
    read_lock_pid(zalo_data_dir)
}

#[cfg(target_os = "linux")]
fn send_signal_verified(pid: u32, sig: libc::c_int, protected_primary_pids: &[u32]) {
    if pid <= 1 || protected_primary_pids.contains(&pid) {
        return;
    }
    if let Ok(raw_pid) = libc::pid_t::try_from(pid) {
        // SAFETY: `raw_pid > 1` targets a single verified non-primary process ID (never 0 or -1 group broadcast).
        unsafe {
            let _ = libc::kill(raw_pid, sig);
        }
    }
}

#[cfg(target_os = "linux")]
fn reap_direct_children(pids: &[u32]) {
    for &pid in pids {
        if let Ok(raw_pid) = libc::pid_t::try_from(pid) {
            let mut status: libc::c_int = 0;
            // SAFETY: `waitpid` with `WNOHANG` non-blockingly reaps `raw_pid` only if it is our direct child.
            unsafe {
                let _ = libc::waitpid(raw_pid, &mut status, libc::WNOHANG);
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn is_unix_pid_active_non_zombie(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    if let Some(entry) = crate::procfs::read_proc_entry(Path::new("/proc"), pid) {
        return entry.is_active();
    }
    let Ok(raw) = libc::pid_t::try_from(pid) else {
        return false;
    };
    // SAFETY: Signal 0 probes process existence without delivering a signal.
    let rc = unsafe { libc::kill(raw, 0) };
    rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(target_os = "linux")]
fn cleanup_post_stop_hints(profile_dir: &Path, zalo_data_dir: &Path) {
    let _ = remove_file_no_follow(&profile_dir.join(".runtime-pid"));
    let _ = remove_file_no_follow(&zalo_data_dir.join(".quit-signal"));
}

#[cfg(target_os = "linux")]
fn remove_file_no_follow(path: &Path) -> std::io::Result<()> {
    if let Ok(meta) = fs::symlink_metadata(path) {
        if !meta.is_dir() || meta.file_type().is_symlink() {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn query_x11_window_for_pids(pids: &[u32]) -> (Option<u64>, Option<String>) {
    use std::process::Command;
    if pids.is_empty() || std::env::var_os("DISPLAY").is_none() {
        return (None, None);
    }
    let Some(wmctrl) = find_executable_in_path("wmctrl") else {
        return (None, None);
    };
    let Ok(out) = Command::new(wmctrl).arg("-lp").output() else {
        return (None, None);
    };
    if !out.status.success() {
        return (None, None);
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let matches = parse_wmctrl_windows_for_pids(&stdout, pids);
    if let Some(win) = matches.first() {
        (Some(win.window_id), Some(win.title.clone()))
    } else {
        (None, None)
    }
}

#[cfg(target_os = "linux")]
fn try_activate_linux_window(
    target: &LaunchTarget,
    profile: &ProfileRecord,
    profile_dir: &Path,
    state: &ProfileRuntimeState,
) -> Result<(), DimError> {
    use std::process::{Command, Stdio};

    // 1. On X11 / XWayland, prefer exact PID-verified window ID activation via `wmctrl` or `xdotool`.
    if std::env::var_os("DISPLAY").is_some() {
        if let Some(wmctrl) = find_executable_in_path("wmctrl") {
            if let Ok(out) = Command::new(&wmctrl).arg("-lp").output() {
                if out.status.success() {
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    if let Some(win) =
                        parse_wmctrl_windows_for_pids(&stdout, &state.process_ids).first()
                    {
                        let status = Command::new(wmctrl)
                            .args(["-i", "-a", &win.window_id_hex])
                            .stdin(Stdio::null())
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .status()?;
                        if status.success() {
                            return Ok(());
                        }
                    }
                }
            }
        }
        if let Some(xdotool) = find_executable_in_path("xdotool") {
            for &pid in &state.process_ids {
                let status = Command::new(&xdotool)
                    .args([
                        "search",
                        "--onlyvisible",
                        "--pid",
                        &pid.to_string(),
                        "windowactivate",
                    ])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
                if status.is_ok_and(|s| s.success()) {
                    return Ok(());
                }
            }
        }
    }

    // 2. On Wayland (or without wmctrl/xdotool), trigger Electron's single-instance restore handoff
    // inside the exact isolated profile environment so the owning instance restores its own window.
    if let LaunchTarget::CustomExecutable { path, args } = target {
        if path.is_file() {
            let env_map =
                build_sanitized_linux_spawn_env(profile, profile_dir, &|k| std::env::var(k).ok());
            let _ = Command::new(path)
                .args(args)
                .env_clear()
                .envs(&env_map)
                .current_dir(profile_dir)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
        }
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn unix_free_disk_bytes(path: &Path) -> Result<u64, DimError> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let mut probe = path;
    while !probe.exists() {
        if let Some(parent) = probe.parent() {
            if parent.as_os_str().is_empty() {
                probe = Path::new("/");
                break;
            }
            probe = parent;
        } else {
            probe = Path::new("/");
            break;
        }
    }
    let c_path = CString::new(probe.as_os_str().as_bytes())
        .map_err(|_| DimError::PathTraversal("path contains NUL byte".to_owned()))?;
    // SAFETY: `statvfs` initializes `stat` when return code is 0.
    unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c_path.as_ptr(), &mut stat) != 0 {
            return Err(DimError::Io(std::io::Error::last_os_error()));
        }
        Ok((stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64))
    }
}

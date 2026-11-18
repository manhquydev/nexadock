//! `LinuxPlatformAdapter` implementing `dim-core` traits for Ubuntu/Linux desktop environments.
//!
//! Architectural boundaries:
//! - Official VNG Zalo Desktop is honestly reported as [`SupportStatus::Unsupported`]
//!   (`OFFICIAL_ZALO_UNSUPPORTED`) because VNG publishes no Linux client.
//! - Compatible local desktop applications, user-supplied `.AppImage` bundles, and multi-instance
//!   test fixtures run under isolated `HOME` + XDG Base Directories (`0o700`) and optional
//!   `bubblewrap` (`bwrap`) namespaces in [`SupportStatus::Experimental`] mode.
//! - Default [`LauncherLifetimePolicy::PersistentDetachedSession`] detaches spawned instances into
//!   a new POSIX session (`setsid` + `bwrap --new-session`, without `--die-with-parent`) so instances
//!   survive short-lived CLI commands (`dim-cli launch <id>`), while `(pid, starttime_ticks)` +
//!   `DIM_PROFILE_ID` + `DIM_INSTANCE_NONCE` guarantee PID-reuse-safe tracking and clean `stop()`.
//! - Security controls (`AppArmor`, `chrome-sandbox`, non-root execution) are strictly preserved;
//!   `--no-sandbox`, `killall`, `pkill`, and root execution are rejected with `SECURITY_BLOCKED`.

use std::collections::BTreeMap;
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
    enforce_launch_security_policy, probe_ubuntu_runtime_at, DisplaySessionKind, UbuntuRuntimeProbe,
};
use crate::layout::{
    ensure_linux_profile_layout, is_safe_bindable_socket_or_file, linux_zalo_data_dir,
    looks_like_appimage, measure_profile_disk_usage_bytes, plan_bwrap_desktop_sandbox,
    verify_directory_ownership_and_mode, write_atomic_private_file, BwrapSandboxOptions,
    BwrapSandboxPlan, DesktopSocketSurface, LauncherLifetimePolicy, APPARMOR_NO_SANDBOX_NOTE,
};
use crate::procfs::{collect_primary_process_tree, collect_profile_process_tree, read_lock_pid};

#[cfg(target_os = "linux")]
use crate::isolation::find_executable_in_path;
#[cfg(target_os = "linux")]
use crate::layout::{build_sanitized_linux_spawn_env_with_nonce, generate_instance_nonce};
#[cfg(target_os = "linux")]
use crate::procfs::{parse_wmctrl_windows_for_pids, read_proc_entry, read_proc_stat_starttime};

/// Virtualized filesystem/procfs/session overrides for deterministic unit & cross-platform testing.
#[derive(Debug, Clone)]
struct VirtualLinuxEnv {
    fs_root: PathBuf,
    proc_root: PathBuf,
    primary_config_dir: PathBuf,
    effective_uid: u32,
    arch: String,
    bwrap_path: Option<PathBuf>,
    env_vars: BTreeMap<String, String>,
}

/// Linux platform adapter managing isolated XDG/HOME profiles, `/proc` process trees,
/// binary provenance checks, desktop socket sandboxing, and Ubuntu AppArmor security preflights.
#[derive(Debug, Clone)]
pub struct LinuxPlatformAdapter {
    prefer_bwrap: bool,
    lifetime_policy: LauncherLifetimePolicy,
    portal_file_chooser: bool,
    pinned_sha256: Option<String>,
    virtual_env: Option<VirtualLinuxEnv>,
}

impl Default for LinuxPlatformAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxPlatformAdapter {
    /// Creates a new Linux platform adapter using standard unprivileged `HOME` + XDG isolation,
    /// [`LauncherLifetimePolicy::PersistentDetachedSession`], and portal file-chooser preference.
    #[must_use]
    pub fn new() -> Self {
        Self {
            prefer_bwrap: false,
            lifetime_policy: LauncherLifetimePolicy::PersistentDetachedSession,
            portal_file_chooser: true,
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

    /// Configures the spawned child session & `bwrap` lifetime policy.
    ///
    /// Defaults to [`LauncherLifetimePolicy::PersistentDetachedSession`] so instances launched via
    /// `dim-cli launch <id>` survive after the CLI process exits; pass
    /// [`LauncherLifetimePolicy::DieWithParent`] only for transient parent-bound test harnesses.
    #[must_use]
    pub fn with_lifetime_policy(mut self, policy: LauncherLifetimePolicy) -> Self {
        self.lifetime_policy = policy;
        self
    }

    /// Returns the active [`LauncherLifetimePolicy`] configured on this adapter.
    #[must_use]
    pub const fn lifetime_policy(&self) -> LauncherLifetimePolicy {
        self.lifetime_policy
    }

    /// Controls whether `GTK_USE_PORTAL=1` is injected by default (when unset in host env) so
    /// GTK/Electron file choosers route via `xdg-desktop-portal` over session D-Bus while keeping
    /// direct downloads confined to `<profile_dir>/Downloads` and `<profile_dir>/Documents`.
    #[must_use]
    pub fn with_portal_file_chooser(mut self, enabled: bool) -> Self {
        self.portal_file_chooser = enabled;
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
            lifetime_policy: LauncherLifetimePolicy::PersistentDetachedSession,
            portal_file_chooser: true,
            pinned_sha256: None,
            virtual_env: Some(VirtualLinuxEnv {
                fs_root,
                proc_root,
                primary_config_dir,
                effective_uid,
                arch: arch.into(),
                bwrap_path,
                env_vars: BTreeMap::new(),
            }),
        }
    }

    /// Inserts a synthetic session environment variable on a virtualized adapter (no-op if live).
    #[must_use]
    pub fn with_virtual_env_var(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        if let Some(ref mut venv) = self.virtual_env {
            venv.env_vars.insert(key.into(), value.into());
        }
        self
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
                &|k| venv.env_vars.get(k).cloned(),
                venv.bwrap_path.clone(),
            );
        }
        crate::isolation::probe_live_ubuntu_runtime()
    }

    /// Computes the least-privilege `bubblewrap` (`bwrap`) sandbox plan and security-tradeoff
    /// diagnostics for a profile and executable under the current (or virtualized) session environment.
    #[must_use]
    pub fn plan_sandbox(
        &self,
        profile: &ProfileRecord,
        profile_dir: &Path,
        executable: &Path,
        app_args: &[String],
        instance_nonce: Option<&str>,
    ) -> BwrapSandboxPlan {
        let opts = BwrapSandboxOptions {
            lifetime_policy: self.lifetime_policy,
            profile_id: Some(&profile.id),
            instance_nonce,
            prefer_portal_file_chooser: self.portal_file_chooser,
            extra_ro_files: &[],
        };
        if let Some(venv) = &self.virtual_env {
            return plan_bwrap_desktop_sandbox(
                profile_dir,
                executable,
                app_args,
                &opts,
                &|k| venv.env_vars.get(k).cloned(),
                &|_| true,
            );
        }
        let endpoint_probe: &dyn Fn(&Path) -> bool = if cfg!(target_os = "linux") {
            &is_safe_bindable_socket_or_file
        } else {
            &|_| true
        };
        plan_bwrap_desktop_sandbox(
            profile_dir,
            executable,
            app_args,
            &opts,
            &|k| std::env::var(k).ok(),
            endpoint_probe,
        )
    }

    /// Scans standard XDG `.desktop` directories and user AppImage folders for compatible local binaries.
    #[must_use]
    pub fn discover_compatible_applications(&self) -> Vec<DiscoveredLinuxApp> {
        discover_compatible_apps_from_env(&|k| std::env::var(k).ok())
    }

    /// Performs format, permission, startup-script, ELF dynamic dependency, and SHA-256 inspection on a local binary.
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
            format!(
                "prefer_bwrap={} lifetime_policy={} portal_file_chooser={}",
                self.prefer_bwrap,
                self.lifetime_policy.as_str(),
                self.portal_file_chooser
            ),
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
            "Isolation mode: separate HOME + XDG_CONFIG_HOME/XDG_DATA_HOME/XDG_CACHE_HOME/XDG_STATE_HOME + user-dirs.dirs + 0o700 (data separation; optional bwrap for filesystem/IPC sandboxing; never binds full $HOME, full /dev, full $XDG_RUNTIME_DIR, or system D-Bus)"
                .to_owned(),
        ];
        if let Some(de) = &probe.desktop_environment {
            diagnostics.push(format!("desktop_environment={de}"));
        }

        // Surface explicit sandbox socket security-tradeoff notes for active host surfaces.
        if probe.display_session == DisplaySessionKind::X11 {
            diagnostics.push(
                DesktopSocketSurface::X11Server
                    .security_tradeoff_note()
                    .to_owned(),
            );
        }
        diagnostics.push(
            DesktopSocketSurface::SessionDbus
                .security_tradeoff_note()
                .to_owned(),
        );
        if probe.pipewire_socket_present {
            diagnostics.push(
                DesktopSocketSurface::PipeWireDaemon
                    .security_tradeoff_note()
                    .to_owned(),
            );
        }
        if probe.pulse_socket_present {
            diagnostics.push(
                DesktopSocketSurface::PulseAudioServer
                    .security_tradeoff_note()
                    .to_owned(),
            );
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
                "Sharing X11 (`/tmp/.X11-unix/X<n>`), unfiltered session D-Bus (`DBUS_SESSION_BUS_ADDRESS`), or PipeWire/PulseAudio sockets for desktop compatibility reduces `bwrap` sandbox isolation compared to pure Wayland + `xdg-desktop-portal` + `xdg-dbus-proxy`."
                    .to_owned(),
                "Community port `VN-Linux-Family/zalo-for-linux` binds hardcoded TCP `127.0.0.1:29631` and `127.0.0.1:29632` for zcall; concurrent calls across instances in shared host networking are BLOCKED."
                    .to_owned(),
                "Native Wayland (GNOME Mutter on Ubuntu 24.04) restricts external window activation without an xdg-activation-v1 surface token; GUI window focus returns UNSUPPORTED_OPERATION on Wayland and uses strict PID-verified XID activation on X11 (`.focus-signal` for test fixtures)."
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
        let runtime_pid_file = profile_dir.join(".runtime-pid");
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
            let runtime_hint_present = fs::symlink_metadata(&runtime_pid_file).is_ok();
            let stale = tree.main_pid.is_none()
                && (singleton_present || fixture_present || runtime_hint_present);
            let lockfile_path = if singleton_present || (!fixture_present && !runtime_hint_present)
            {
                singleton_lock
            } else if fixture_present {
                fixture_lock
            } else {
                runtime_pid_file
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
                    if singleton_present || fixture_present {
                        "Running".to_owned()
                    } else {
                        "Starting".to_owned()
                    }
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
            let runtime_hint_present = fs::symlink_metadata(&runtime_pid_file).is_ok();
            let stale = tree.main_pid.is_none()
                && (singleton_present || fixture_present || runtime_hint_present);
            let lockfile_path = if singleton_present || (!fixture_present && !runtime_hint_present)
            {
                singleton_lock
            } else if fixture_present {
                fixture_lock
            } else {
                runtime_pid_file
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
                    if singleton_present || fixture_present {
                        "Running".to_owned()
                    } else {
                        "Starting".to_owned()
                    }
                } else if stale {
                    "Stopped (Stale Lock)".to_owned()
                } else {
                    "Stopped".to_owned()
                },
            })
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (
                profile,
                singleton_lock,
                fixture_lock,
                runtime_pid_file,
                detected_uid,
            );
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
        let (exe, args, is_electron_appimage, extra_ro_files) = match target {
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
                report.validate_executable_format()?;
                if let Some(arch) = report.arch {
                    if !arch.matches_current_arch() {
                        return Err(DimError::UnsupportedPlatform(format!(
                            "binary CPU architecture '{}' is incompatible with host '{}'",
                            arch.as_str(),
                            std::env::consts::ARCH
                        )));
                    }
                }
                let is_appimage = looks_like_appimage(&report.path)
                    || matches!(
                        report.format,
                        BinaryFormatKind::AppImageType2
                            | BinaryFormatKind::AppImageUruntimeDwarfs
                            | BinaryFormatKind::ExtractedAppDir
                    );
                (report.path, args.clone(), is_appimage, Vec::new())
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
                if !extra_args
                    .iter()
                    .any(|a| a == "--user-id" || a.starts_with("--user-id="))
                {
                    let zalo_data = linux_zalo_data_dir(profile_dir);
                    let resolved_uid = detect_zalo_user_id(&zalo_data)
                        .or_else(|| profile.zalo_user_id.clone())
                        .unwrap_or_else(|| derive_deterministic_fixture_user_id(&profile.id));
                    full.push("--user-id".to_owned());
                    full.push(resolved_uid);
                }
                full.extend(extra_args.iter().cloned());
                (resolved_node, full, false, vec![script_path.clone()])
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

            let instance_nonce = generate_instance_nonce();
            let env_map = build_sanitized_linux_spawn_env_with_nonce(
                profile,
                profile_dir,
                Some(&instance_nonce),
                self.portal_file_chooser,
                &|k| std::env::var(k).ok(),
            );

            let mut child = if self.prefer_bwrap {
                let bwrap_bin = probe.bwrap_path.ok_or_else(|| {
                    DimError::InstallNotFound(
                        "bubblewrap (`bwrap`) requested but not installed in PATH".to_owned(),
                    )
                })?;
                let sandbox_opts = BwrapSandboxOptions {
                    lifetime_policy: self.lifetime_policy,
                    profile_id: Some(&profile.id),
                    instance_nonce: Some(&instance_nonce),
                    prefer_portal_file_chooser: self.portal_file_chooser,
                    extra_ro_files: &extra_ro_files,
                };
                let sandbox_plan = plan_bwrap_desktop_sandbox(
                    profile_dir,
                    &exe,
                    &args,
                    &sandbox_opts,
                    &|k| std::env::var(k).ok(),
                    &is_safe_bindable_socket_or_file,
                );
                let mut cmd = Command::new(bwrap_bin);
                cmd.args(&sandbox_plan.args)
                    .env_clear()
                    .envs(&env_map)
                    .current_dir(profile_dir)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                configure_child_lifetime(&mut cmd, self.lifetime_policy);
                cmd.spawn()?
            } else {
                let mut cmd = Command::new(&exe);
                cmd.args(&args)
                    .env_clear()
                    .envs(&env_map)
                    .current_dir(profile_dir)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                configure_child_lifetime(&mut cmd, self.lifetime_policy);
                cmd.spawn()?
            };

            let spawned_pid = child.id();
            // Capture `/proc/<pid>/stat` field 22 (`starttime_ticks`) BEFORE calling `try_wait()`
            // so the kernel cannot recycle `spawned_pid` prior to recording its start time.
            let starttime_ticks = read_proc_stat_starttime(Path::new("/proc"), spawned_pid);
            let runtime_hint = serde_json::json!({
                "pid": spawned_pid,
                "profile_id": profile.id,
                "instance_nonce": instance_nonce,
                "starttime_ticks": starttime_ticks,
                "lifetime_policy": self.lifetime_policy.as_str(),
                "uses_bwrap": self.prefer_bwrap,
                "started_at": now_rfc3339_utc(),
            });
            let _ = write_atomic_private_file(
                &profile_dir.join(".runtime-pid"),
                format!("{runtime_hint}\n").as_bytes(),
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
            let _ = (exe, args, extra_ro_files);
            Err(DimError::UnsupportedPlatform(
                "Linux launch requires Linux".to_owned(),
            ))
        }
    }

    fn focus_or_restore(
        &self,
        target: &LaunchTarget,
        profile: &ProfileRecord,
        _profile_dir: &Path,
        state: &ProfileRuntimeState,
    ) -> Result<(), DimError> {
        if !state.running {
            return Err(DimError::NotRunning(profile.name.clone()));
        }

        // 1. Fixture targets (or any running instance holding a fixture `lockfile`) support
        //    deterministic `.focus-signal` IPC regardless of Wayland/X11/Headless session.
        let is_fixture = matches!(target, LaunchTarget::NodeFixture { .. })
            || state.zalo_data_dir.join("lockfile").is_file();
        if is_fixture {
            let payload = format!("{{\"focused_at\":\"{}\"}}\n", now_rfc3339_utc());
            write_atomic_private_file(
                &state.zalo_data_dir.join(".focus-signal"),
                payload.as_bytes(),
            )?;
            return Ok(());
        }

        // 2. For GUI desktop targets, never return a fake `Ok(())` when PID-verified window focus
        //    cannot be safely performed!
        let probe = self.probe_ubuntu_environment();
        match probe.display_session {
            DisplaySessionKind::Wayland => Err(DimError::UnsupportedPlatform(
                "UNSUPPORTED_OPERATION: safe PID-verified window focus is not supported on Wayland compositors without an xdg-activation-v1 surface token"
                    .to_owned(),
            )),
            DisplaySessionKind::Headless => Err(DimError::UnsupportedPlatform(
                "UNSUPPORTED_OPERATION: safe PID-verified window focus is unavailable in a headless session without DISPLAY or WAYLAND_DISPLAY"
                    .to_owned(),
            )),
            DisplaySessionKind::X11 => {
                #[cfg(target_os = "linux")]
                {
                    if self.virtual_env.is_none() {
                        return activate_verified_x11_window(&state.process_ids);
                    }
                }
                Err(DimError::UnsupportedPlatform(
                    "UNSUPPORTED_OPERATION: X11 PID-verified window focus requires a live Linux X11 display server"
                        .to_owned(),
                ))
            }
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
            let _ = write_atomic_private_file(
                &state.zalo_data_dir.join(".quit-signal"),
                format!("{{\"requested_at\":\"{}\"}}\n", now_rfc3339_utc()).as_bytes(),
            );

            let euid = current_effective_uid();
            let initial_tree = collect_profile_process_tree(
                Path::new("/proc"),
                &state.profile_id,
                &state.profile_dir,
                &state.zalo_data_dir,
                Some(euid),
            );

            if initial_tree
                .process_ids
                .iter()
                .any(|p| protected_primary_pids.contains(p))
            {
                return Err(DimError::ProtectedPrimaryProcess(format!(
                    "refused to stop Linux process tree overlapping protected primary PID(s): profile_id={}",
                    state.profile_id
                )));
            }

            // If no active `/proc` entry matches `(profile_id, starttime_ticks, instance_nonce)`,
            // the profile tree already exited (or `state.main_pid` was recycled by an unrelated
            // process); clean up hints and NEVER signal unverified `state.main_pid`!
            if initial_tree.process_ids.is_empty() {
                reap_direct_children(&[main_pid]);
                cleanup_post_stop_hints(&state.profile_dir, &state.zalo_data_dir);
                return Ok(0);
            }

            let initial_targets = initial_tree.process_ids.clone();
            let initial_count = u32::try_from(initial_targets.len()).unwrap_or(1);

            // Phase 1: Send graceful SIGTERM strictly to the verified application main process
            // (`initial_tree.main_pid`) so Electron's `before-quit` / `will-quit` flushes SQLite
            // WALs cleanly (allowing any outer `bwrap` wrapper to exit naturally when its child exits).
            if let Some(graceful_pid) = initial_tree.main_pid {
                let exp_ticks = initial_tree.process_starttimes.get(&graceful_pid).copied();
                send_signal_verified(
                    graceful_pid,
                    exp_ticks,
                    libc::SIGTERM,
                    protected_primary_pids,
                );
            }

            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
            while std::time::Instant::now() < deadline {
                reap_direct_children(&initial_targets);
                let live = collect_profile_process_tree(
                    Path::new("/proc"),
                    &state.profile_id,
                    &state.profile_dir,
                    &state.zalo_data_dir,
                    Some(euid),
                );
                if live.process_ids.is_empty() {
                    cleanup_post_stop_hints(&state.profile_dir, &state.zalo_data_dir);
                    return Ok(initial_count);
                }
                std::thread::sleep(std::time::Duration::from_millis(40));
            }

            // Phase 2: Check `/proc/<pid>/fd` for open SQLite DB/WAL descriptors (`F-UBU-03`)
            // before escalating to SIGKILL. If a PID still holds an active SQLite descriptor under
            // `<zalo_data_dir>/Database/_production/`, grant one additional SIGTERM flush window and
            // refuse blind mid-WAL SIGKILL if the descriptor remains open.
            let mut remaining = collect_profile_process_tree(
                Path::new("/proc"),
                &state.profile_id,
                &state.profile_dir,
                &state.zalo_data_dir,
                Some(euid),
            );
            let has_open_db = remaining.process_ids.iter().any(|&pid| {
                pid_holds_open_sqlite_db_fd(Path::new("/proc"), pid, &state.zalo_data_dir)
            });
            if has_open_db {
                if let Some(graceful_pid) = remaining.main_pid {
                    let exp_ticks = remaining.process_starttimes.get(&graceful_pid).copied();
                    send_signal_verified(
                        graceful_pid,
                        exp_ticks,
                        libc::SIGTERM,
                        protected_primary_pids,
                    );
                }
                let wal_deadline =
                    std::time::Instant::now() + std::time::Duration::from_millis(500);
                while std::time::Instant::now() < wal_deadline {
                    reap_direct_children(&initial_targets);
                    remaining = collect_profile_process_tree(
                        Path::new("/proc"),
                        &state.profile_id,
                        &state.profile_dir,
                        &state.zalo_data_dir,
                        Some(euid),
                    );
                    let still_open = remaining.process_ids.iter().any(|&pid| {
                        pid_holds_open_sqlite_db_fd(Path::new("/proc"), pid, &state.zalo_data_dir)
                    });
                    if !still_open {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(40));
                }
                if remaining.process_ids.iter().any(|&pid| {
                    pid_holds_open_sqlite_db_fd(Path::new("/proc"), pid, &state.zalo_data_dir)
                }) {
                    return Err(DimError::ProcessError(format!(
                        "refused SIGKILL on profile '{}' ({}): process still holds open SQLite WAL/DB descriptors under '{}'",
                        state.profile_name,
                        state.profile_id,
                        state.zalo_data_dir.display()
                    )));
                }
            }

            // Escalate to SIGKILL ONLY on remaining verified profile-owned PIDs in reverse
            // topological order (leaves-first: helpers/renderers -> zygote -> app main ->
            // outer `bwrap` wrapper), verifying `(pid, starttime_ticks)` right before each `kill(2)`.
            for &pid in remaining.process_ids.iter().rev() {
                let exp_ticks = remaining.process_starttimes.get(&pid).copied();
                send_signal_verified(pid, exp_ticks, libc::SIGKILL, protected_primary_pids);
            }

            std::thread::sleep(std::time::Duration::from_millis(40));
            reap_direct_children(&initial_targets);
            reap_direct_children(&remaining.process_ids);
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

/// Derives a deterministic 7-digit fixture `zalo_user_id` (`1000000..9999998`) via FNV-1a over
/// `profile_id`, matching Windows and macOS `NodeFixture` launch parity (`DEFECT-02`).
#[must_use]
pub fn derive_deterministic_fixture_user_id(profile_id: &str) -> String {
    let mut hash: u32 = 2_166_136_261;
    for b in profile_id.as_bytes() {
        hash ^= u32::from(*b);
        hash = hash.wrapping_mul(16_777_619);
    }
    let uid = 1_000_000 + (hash % 8_999_999);
    uid.to_string()
}

/// Checks `/proc/<pid>/fd` symlink targets (or plain descriptor target files in synthetic `proc_root`
/// tests) to determine whether `pid` holds an open SQLite DB/WAL/journal descriptor inside
/// `<zalo_data_dir>/Database/_production` (`F-UBU-03`).
#[must_use]
pub fn pid_holds_open_sqlite_db_fd(proc_root: &Path, pid: u32, zalo_data_dir: &Path) -> bool {
    let fd_dir = proc_root.join(pid.to_string()).join("fd");
    let Ok(entries) = fs::read_dir(&fd_dir) else {
        return false;
    };
    let db_prod_dir = zalo_data_dir.join("Database").join("_production");
    let db_prod_norm = db_prod_dir.to_string_lossy().replace('\\', "/");

    for entry in entries.flatten() {
        let fd_path = entry.path();
        let target_path = fs::read_link(&fd_path).ok().or_else(|| {
            fs::read_to_string(&fd_path)
                .ok()
                .map(|s| PathBuf::from(s.trim()))
        });
        let Some(target) = target_path else {
            continue;
        };
        let target_norm = target.to_string_lossy().replace('\\', "/");
        if !target_norm.starts_with(&db_prod_norm) {
            continue;
        }
        let lower_name = target
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if lower_name.ends_with(".db")
            || lower_name.ends_with("-wal")
            || lower_name.ends_with("-shm")
            || lower_name.ends_with("-journal")
            || lower_name.contains(".sqlite")
        {
            return true;
        }
    }
    false
}

/// Reads PID from Linux `SingletonLock` symlink (`<host>-<pid>`) or fixture `lockfile` JSON.
#[must_use]
pub fn read_linux_pid(zalo_data_dir: &Path) -> Option<u32> {
    read_lock_pid(zalo_data_dir)
}

#[cfg(target_os = "linux")]
fn detach_into_new_session() -> std::io::Result<()> {
    // SAFETY: In a freshly forked child (`pid != pgid`), `setsid()` and `setpgid(0, 0)` are
    // POSIX async-signal-safe syscalls creating a new session and process group without a
    // controlling terminal so `dim-cli` exit or terminal `SIGHUP` never kills the desktop instance.
    unsafe {
        if libc::setsid() == -1 {
            let _ = libc::setpgid(0, 0);
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn configure_child_lifetime(cmd: &mut std::process::Command, policy: LauncherLifetimePolicy) {
    use std::os::unix::process::CommandExt;
    if policy.detaches_from_launcher() {
        // SAFETY: `detach_into_new_session` invokes only POSIX async-signal-safe syscalls
        // (`setsid`/`setpgid`) in the post-fork child with zero heap allocation or lock usage.
        unsafe {
            cmd.pre_exec(detach_into_new_session);
        }
    }
}

#[cfg(target_os = "linux")]
fn send_signal_verified(
    pid: u32,
    expected_starttime_ticks: Option<u64>,
    sig: libc::c_int,
    protected_primary_pids: &[u32],
) {
    if pid <= 1 || protected_primary_pids.contains(&pid) {
        return;
    }
    let Some(entry) = read_proc_entry(Path::new("/proc"), pid) else {
        return;
    };
    if !entry.is_active() {
        return;
    }
    if let Some(exp_ticks) = expected_starttime_ticks {
        if entry.starttime_ticks != exp_ticks {
            return;
        }
    }
    if let Ok(raw_pid) = libc::pid_t::try_from(pid) {
        // SAFETY: `raw_pid > 1` targets a single `(pid, starttime_ticks)`-verified non-primary
        // process ID (never 0 or -1 group broadcast).
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
fn activate_verified_x11_window(allowed_pids: &[u32]) -> Result<(), DimError> {
    use std::process::{Command, Stdio};

    if allowed_pids.is_empty() {
        return Err(DimError::ProcessError(
            "X11 PID-verified window focus failed: profile has no verified active PIDs".to_owned(),
        ));
    }

    let wmctrl_bin = find_executable_in_path("wmctrl");
    let xdotool_bin = find_executable_in_path("xdotool");

    if wmctrl_bin.is_none() && xdotool_bin.is_none() {
        return Err(DimError::ProcessError(
            "X11 PID-verified window focus failed: neither `wmctrl` nor `xdotool` is installed in PATH"
                .to_owned(),
        ));
    }

    if let Some(wmctrl) = wmctrl_bin {
        if let Ok(out) = Command::new(&wmctrl).arg("-lp").output() {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                for win in parse_wmctrl_windows_for_pids(&stdout, allowed_pids) {
                    let status = Command::new(&wmctrl)
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

    if let Some(xdotool) = xdotool_bin {
        for &pid in allowed_pids {
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

    Err(DimError::ProcessError(format!(
        "X11 PID-verified window focus failed: no visible X11 window matched verified profile PIDs {allowed_pids:?}"
    )))
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

//! Ubuntu 24.04 / 22.04 isolation model comparison, AppArmor `userns` probing, `/proc/mounts`
//! inspection, structured Electron/AppImage preflight diagnostics, non-root enforcement, and
//! `SECURITY_BLOCKED` policy guards.
//!
//! Explicitly distinguishes **profile data separation** (`HOME`/`XDG_*` redirection, where a
//! compromised process still runs with the user's full ambient Linux UID authority) from a **true
//! OS sandbox** (`bubblewrap` mount/IPC namespaces, Landlock LSM, seccomp-bpf, and Chromium's
//! internal `chrome-sandbox` user/PID/net namespaces).

use std::fs::{self, File};
use std::path::{Path, PathBuf};

use crate::discovery::{
    inspect_user_binary, resolve_elf64_missing_libraries_at, BinaryFormatKind,
    BinaryInspectionReport, ElfCpuArch,
};
use crate::layout::{looks_like_appimage, verify_directory_ownership_and_mode};
pub use crate::status::{PreflightCategory, PreflightDiagnosticKind, PreflightSeverity};
use crate::status::{SecurityBlockReason, UbuntuTargetTier, VerificationLabel};

/// Mandatory advisory warning accompanying any Ubuntu 24.04 AppArmor `userns` guidance.
///
/// Scopes profile evaluation strictly to isolated test environments, confirms host
/// `/etc/apparmor.d` is never modified automatically, and warns that an AppArmor `userns`
/// profile does NOT guarantee third-party AppImage compatibility.
pub const APPARMOR_PROFILE_NON_GUARANTEE_WARNING: &str =
    "ISOLATED TEST ENVIRONMENT ADVISORY ONLY (host /etc/apparmor.d is NEVER modified automatically, AppArmor is NEVER disabled system-wide, and --no-sandbox is NEVER auto-appended): an AppArmor `userns` profile on the outer AppImage path is NOT guaranteed to make every third-party AppImage work — FUSE-mounted inner binaries (/tmp/.mount_*/...), nested bubblewrap transitions (/etc/apparmor.d/bwrap-userns-restrict -> bwrap-unprivileged), `noexec` or `nosuid` mount options, missing /dev/fuse or libfuse2t64/libfuse3-3, or Electron 22 Wayland/GPU bugs may still block execution.";

/// Architectural isolation mechanisms evaluated for Linux desktop multi-instance execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxIsolationMechanism {
    /// A. Per-process environment variable overrides (`XDG_CONFIG_HOME`, `TMPDIR`) without separate `$HOME`.
    ProcessEnvironmentOnly,
    /// B. Dedicated `HOME` + full XDG Base Directory + `user-dirs.dirs` + `0o700` per-profile tree.
    SeparateHomeAndXdgProfile,
    /// C. `bubblewrap` (`bwrap`) mount + IPC namespace container hiding real `$HOME` and sibling profiles.
    BubblewrapNamespaceSandbox,
    /// D. Direct unprivileged Linux user namespaces (`CLONE_NEWUSER` / `unshare`).
    UserNamespaceIsolation,
    /// E. Complementary kernel/OS primitives (`Landlock` LSM, `systemd --user` cgroup v2 scopes, targeted AppArmor profiles).
    ComplementaryOsControls,
}

/// Structured capability and security comparison row for an isolation mechanism.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsolationComparisonEntry {
    /// Mechanism identifier.
    pub mechanism: LinuxIsolationMechanism,
    /// Isolates `SingletonLock`, SQLite DBs, cookies, caches, and received files across profiles.
    pub separates_profile_data: bool,
    /// Enforces a kernel-level filesystem/IPC security boundary against a compromised process.
    pub is_true_security_sandbox: bool,
    /// Isolates hardcoded loopback TCP ports (`127.0.0.1:29631`/`29632` in `zalo-for-linux` calls).
    pub isolates_loopback_tcp_ports: bool,
    /// Works unprivileged on stock Ubuntu 24.04 LTS (`apparmor_restrict_unprivileged_userns = 1`) without admin profile changes.
    pub works_stock_ubuntu_2404_unprivileged: bool,
    /// Concise architectural summary and Ubuntu 24.04 caveat.
    pub summary: &'static str,
}

/// Returns the canonical 5-way isolation comparison table for Ubuntu 24.04 LTS.
#[must_use]
pub fn compare_linux_isolation_mechanisms() -> Vec<IsolationComparisonEntry> {
    vec![
        IsolationComparisonEntry {
            mechanism: LinuxIsolationMechanism::ProcessEnvironmentOnly,
            separates_profile_data: false,
            is_true_security_sandbox: false,
            isolates_loopback_tcp_ports: false,
            works_stock_ubuntu_2404_unprivileged: true,
            summary: "Insufficient alone: libraries calling getpwuid() or reading $HOME bypass XDG-only env vars and leak into real ~.",
        },
        IsolationComparisonEntry {
            mechanism: LinuxIsolationMechanism::SeparateHomeAndXdgProfile,
            separates_profile_data: true,
            is_true_security_sandbox: false,
            isolates_loopback_tcp_ports: false,
            works_stock_ubuntu_2404_unprivileged: true,
            summary: "Default data-separation layer (HOME + XDG_* + user-dirs.dirs + 0o700). Prevents cross-profile file collisions, NOT a security sandbox against malicious code.",
        },
        IsolationComparisonEntry {
            mechanism: LinuxIsolationMechanism::BubblewrapNamespaceSandbox,
            separates_profile_data: true,
            is_true_security_sandbox: true,
            isolates_loopback_tcp_ports: false,
            works_stock_ubuntu_2404_unprivileged: false,
            summary: "True filesystem/IPC sandbox hiding real $HOME; Ubuntu 24.04 ships /etc/apparmor.d/bwrap-userns-restrict for bwrap, but child transition to bwrap-unprivileged can still deny inner Electron chrome-sandbox nested userns.",
        },
        IsolationComparisonEntry {
            mechanism: LinuxIsolationMechanism::UserNamespaceIsolation,
            separates_profile_data: false,
            is_true_security_sandbox: true,
            isolates_loopback_tcp_ports: false,
            works_stock_ubuntu_2404_unprivileged: false,
            summary: "Blocked for unconfined binaries on Ubuntu 23.10+/24.04 by kernel.apparmor_restrict_unprivileged_userns=1 unless covered by an AppArmor `userns,` profile (not guaranteed for FUSE-mounted AppImages).",
        },
        IsolationComparisonEntry {
            mechanism: LinuxIsolationMechanism::ComplementaryOsControls,
            separates_profile_data: false,
            is_true_security_sandbox: true,
            isolates_loopback_tcp_ports: false,
            works_stock_ubuntu_2404_unprivileged: true,
            summary: "Landlock LSM (Linux 5.13+, no userns needed) + systemd --user cgroup v2 scopes keep least-privilege security without --no-sandbox or disabling AppArmor.",
        },
    ]
}

/// Active Linux graphical display protocol detected from session environment variables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplaySessionKind {
    /// Native Wayland compositor (`XDG_SESSION_TYPE=wayland` or `WAYLAND_DISPLAY` set).
    Wayland,
    /// X11 server (`XDG_SESSION_TYPE=x11` or only `DISPLAY` set).
    X11,
    /// Headless CI runner or SSH session with neither `WAYLAND_DISPLAY` nor `DISPLAY`.
    Headless,
}

impl DisplaySessionKind {
    /// Lowercase session identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Wayland => "wayland",
            Self::X11 => "x11",
            Self::Headless => "headless",
        }
    }
}

/// Parsed entry from Linux `/proc/mounts` (`fstab(5)` 6-column format).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcMountEntry {
    /// Mounted block device or pseudo-filesystem source (`fs_spec`).
    pub device: String,
    /// Unescaped mount point path (`fs_file`, e.g. `"/tmp"` or `"/home/user/My Apps"`).
    pub mount_point: PathBuf,
    /// Filesystem type (`fs_vfstype`, e.g. `"ext4"`, `"tmpfs"`).
    pub fs_type: String,
    /// Comma-separated mount options (`fs_mntops`, e.g. `["rw", "nosuid", "nodev", "noexec"]`).
    pub options: Vec<String>,
}

impl ProcMountEntry {
    /// Returns `true` if `options` contains the exact mount flag `opt` (e.g., `"noexec"` or `"nosuid"`).
    #[must_use]
    pub fn has_option(&self, opt: &str) -> bool {
        self.options.iter().any(|o| o == opt)
    }
}

/// Parses Linux `/proc/mounts` text into structured [`ProcMountEntry`] records,
/// decoding standard octal path escapes (`\040` space, `\011` tab, `\012` newline, `\134` backslash).
#[must_use]
pub fn parse_proc_mounts(content: &str) -> Vec<ProcMountEntry> {
    let mut entries = Vec::new();
    for raw_line in content.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut cols = trimmed.split_whitespace();
        let (Some(dev), Some(mnt_raw), Some(fstype), Some(opts_raw)) =
            (cols.next(), cols.next(), cols.next(), cols.next())
        else {
            continue;
        };
        let mount_point = PathBuf::from(unescape_proc_mount_token(mnt_raw));
        let options = opts_raw
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .collect();
        entries.push(ProcMountEntry {
            device: unescape_proc_mount_token(dev),
            mount_point,
            fs_type: fstype.to_owned(),
            options,
        });
    }
    entries
}

/// Finds the most specific (longest component prefix) [`ProcMountEntry`] covering `target_path`.
///
/// Supports both live Linux paths (`fs_root == "/"`) and virtualized test roots where `target_path`
/// resides underneath `fs_root`.
#[must_use]
pub fn find_mount_for_path<'a>(
    mounts: &'a [ProcMountEntry],
    target_path: &Path,
    fs_root: &Path,
) -> Option<&'a ProcMountEntry> {
    let normalized = normalize_virtual_linux_path(target_path, fs_root);
    let norm_path = Path::new(&normalized);

    mounts
        .iter()
        .filter(|entry| norm_path.starts_with(&entry.mount_point))
        .max_by_key(|entry| entry.mount_point.components().count())
}

/// Single structured finding inside an [`AppImagePreflightReport`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflightDiagnostic {
    /// Specific diagnostic classification.
    pub kind: PreflightDiagnosticKind,
    /// Whether this finding blocks execution or is a non-fatal warning.
    pub severity: PreflightSeverity,
    /// Concise human-readable explanation of what was detected.
    pub summary: String,
    /// Safe, least-privilege remediation guidance (never `--no-sandbox` or root execution).
    pub remediation: String,
}

impl PreflightDiagnostic {
    /// Returns the high-level [`PreflightCategory`] for this diagnostic.
    #[must_use]
    pub const fn category(&self) -> PreflightCategory {
        self.kind.category()
    }
}

/// Comprehensive preflight diagnostic report for launching an Electron binary, AppImage,
/// or extracted AppDir on Ubuntu 24.04 / 22.04 LTS.
///
/// Cleanly separates:
/// 1. **User namespace / AppArmor restrictions** ([`PreflightCategory::UserNamespaceAndAppArmor`]),
/// 2. **Filesystem permission / `/proc/mounts` issues** ([`PreflightCategory::FilesystemAndMount`]),
/// 3. **Electron / AppImage runtime environment issues** ([`PreflightCategory::RuntimeEnvironment`]),
/// 4. **Hard security policy guards** ([`PreflightCategory::SecurityPolicyGuard`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppImagePreflightReport {
    /// Inspected binary or AppDir path.
    pub binary_path: PathBuf,
    /// `true` only when zero [`PreflightSeverity::Blocking`] diagnostics were found.
    pub ready_to_launch: bool,
    /// Resulting verification/block classification (`SecurityBlocked`, `Blocked`, or `NotTested`).
    pub verification_label: VerificationLabel,
    /// Ordered structured diagnostics (blocking findings and warnings).
    pub diagnostics: Vec<PreflightDiagnostic>,
    /// Explicit security and compatibility advisories (including AppArmor profile non-guarantee caveats).
    pub advisories: Vec<String>,
}

impl AppImagePreflightReport {
    /// Returns all blocking diagnostics preventing safe execution.
    #[must_use]
    pub fn blocking_diagnostics(&self) -> Vec<&PreflightDiagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == PreflightSeverity::Blocking)
            .collect()
    }

    /// Returns `true` if any diagnostic (blocking or warning) belongs to `category`.
    #[must_use]
    pub fn has_category(&self, category: PreflightCategory) -> bool {
        self.diagnostics.iter().any(|d| d.category() == category)
    }

    /// Returns `true` if any [`PreflightSeverity::Blocking`] diagnostic belongs to `category`.
    #[must_use]
    pub fn has_blocking_category(&self, category: PreflightCategory) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == PreflightSeverity::Blocking && d.category() == category)
    }
}

/// Snapshot of Ubuntu/Linux OS security, display, audio, and AppArmor capabilities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UbuntuRuntimeProbe {
    /// Parsed `ID` from `/etc/os-release` (e.g., `"ubuntu"`).
    pub distro_id: String,
    /// Parsed `VERSION_ID` from `/etc/os-release` (e.g., `"24.04"`).
    pub version_id: String,
    /// Classified Ubuntu target matrix tier.
    pub target_tier: UbuntuTargetTier,
    /// Effective POSIX UID of the current process (`0` == `root`).
    pub effective_uid: u32,
    /// True if AppArmor kernel module reports enabled (`/sys/module/apparmor/parameters/enabled == Y`).
    pub apparmor_enabled: bool,
    /// Value of `/proc/sys/kernel/apparmor_restrict_unprivileged_userns` (`Some(1)` on Ubuntu 24.04).
    pub apparmor_restrict_unprivileged_userns: Option<u8>,
    /// Value of `/proc/sys/kernel/unprivileged_userns_clone` (`Some(0)` when kernel-disabled).
    pub unprivileged_userns_clone: Option<u8>,
    /// True if `/etc/apparmor.d/` contains a `bwrap` profile (`bwrap` or `bwrap-userns-restrict`).
    pub bwrap_apparmor_profile_present: bool,
    /// Resolved path to `bwrap` in `PATH`, if installed.
    pub bwrap_path: Option<PathBuf>,
    /// Active graphical session protocol (`Wayland`, `X11`, or `Headless`).
    pub display_session: DisplaySessionKind,
    /// Desktop environment (`XDG_CURRENT_DESKTOP`, e.g. `"ubuntu:GNOME"`).
    pub desktop_environment: Option<String>,
    /// True if PipeWire socket (`$XDG_RUNTIME_DIR/pipewire-0`) exists.
    pub pipewire_socket_present: bool,
    /// True if PulseAudio / `pipewire-pulse` socket (`$XDG_RUNTIME_DIR/pulse/native`) exists.
    pub pulse_socket_present: bool,
    /// True if `/dev/fuse` exists for AppImage FUSE mounting.
    pub dev_fuse_present: bool,
    /// True if at least one V4L2 camera node (`/dev/video0`) exists.
    pub camera_device_present: bool,
}

impl UbuntuRuntimeProbe {
    /// Returns `true` when unprivileged user namespaces are restricted for unconfined binaries.
    #[must_use]
    pub const fn unconfined_userns_restricted(&self) -> bool {
        if matches!(self.unprivileged_userns_clone, Some(0)) {
            return true;
        }
        self.apparmor_enabled && matches!(self.apparmor_restrict_unprivileged_userns, Some(1))
    }

    /// Checks whether `bubblewrap` (`bwrap`) can initialize outer namespaces on this host.
    pub fn evaluate_bwrap_readiness(&self) -> Result<(), SecurityBlockReason> {
        if self.effective_uid == 0 {
            return Err(SecurityBlockReason::RunningAsRoot { euid: 0 });
        }
        if self.bwrap_path.is_none() {
            return Err(SecurityBlockReason::BubblewrapBlocked {
                detail: "bwrap executable not installed in PATH (`sudo apt install bubblewrap`)"
                    .to_owned(),
            });
        }
        if matches!(self.unprivileged_userns_clone, Some(0)) {
            return Err(SecurityBlockReason::BubblewrapBlocked {
                detail: "kernel.unprivileged_userns_clone=0 disables unprivileged user namespaces"
                    .to_owned(),
            });
        }
        if self.apparmor_enabled
            && matches!(self.apparmor_restrict_unprivileged_userns, Some(1))
            && !self.bwrap_apparmor_profile_present
        {
            return Err(SecurityBlockReason::BubblewrapBlocked {
                detail: "apparmor_restrict_unprivileged_userns=1 and no /etc/apparmor.d/bwrap* profile found".to_owned(),
            });
        }
        Ok(())
    }
}

/// Probes the live Linux filesystem (`/`) and current process environment/UID.
#[must_use]
pub fn probe_live_ubuntu_runtime() -> UbuntuRuntimeProbe {
    let euid = current_effective_uid();
    let arch = std::env::consts::ARCH;
    probe_ubuntu_runtime_at(
        Path::new("/"),
        arch,
        euid,
        &|k| std::env::var(k).ok(),
        find_executable_in_path("bwrap"),
    )
}

/// Deterministic root-parameterized Ubuntu/AppArmor/session probe for unit and integration testing.
#[must_use]
pub fn probe_ubuntu_runtime_at(
    fs_root: &Path,
    arch: &str,
    effective_uid: u32,
    env_lookup: &dyn Fn(&str) -> Option<String>,
    bwrap_path: Option<PathBuf>,
) -> UbuntuRuntimeProbe {
    let os_release_path = join_under_root(fs_root, "etc/os-release");
    let (distro_id, version_id) = fs::read_to_string(os_release_path)
        .ok()
        .map(|s| parse_os_release(&s))
        .unwrap_or_else(|| ("unknown".to_owned(), "unknown".to_owned()));

    let target_tier = UbuntuTargetTier::classify(&distro_id, &version_id, arch);

    let apparmor_enabled = fs::read_to_string(join_under_root(
        fs_root,
        "sys/module/apparmor/parameters/enabled",
    ))
    .ok()
    .is_some_and(|v| v.trim().eq_ignore_ascii_case("Y") || v.trim() == "1");

    let apparmor_restrict_unprivileged_userns = read_sysctl_u8(&join_under_root(
        fs_root,
        "proc/sys/kernel/apparmor_restrict_unprivileged_userns",
    ));
    let unprivileged_userns_clone = read_sysctl_u8(&join_under_root(
        fs_root,
        "proc/sys/kernel/unprivileged_userns_clone",
    ));

    let apparmor_dir = join_under_root(fs_root, "etc/apparmor.d");
    let bwrap_apparmor_profile_present = apparmor_dir.join("bwrap").is_file()
        || apparmor_dir.join("bwrap-userns-restrict").is_file()
        || apparmor_dir.join("usr.bin.bwrap").is_file();

    let session_type = env_lookup("XDG_SESSION_TYPE")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let has_wayland = env_lookup("WAYLAND_DISPLAY").is_some_and(|s| !s.trim().is_empty());
    let has_x11 = env_lookup("DISPLAY").is_some_and(|s| !s.trim().is_empty());
    let display_session = if session_type == "wayland" || has_wayland {
        DisplaySessionKind::Wayland
    } else if session_type == "x11" || has_x11 {
        DisplaySessionKind::X11
    } else {
        DisplaySessionKind::Headless
    };

    let desktop_environment = env_lookup("XDG_CURRENT_DESKTOP")
        .or_else(|| env_lookup("DESKTOP_SESSION"))
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());

    let runtime_dir = env_lookup("XDG_RUNTIME_DIR")
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| join_under_root(fs_root, &format!("run/user/{effective_uid}")));

    let pipewire_socket_present = fs::symlink_metadata(runtime_dir.join("pipewire-0")).is_ok();
    let pulse_socket_present =
        fs::symlink_metadata(runtime_dir.join("pulse").join("native")).is_ok();
    let dev_fuse_present = fs::symlink_metadata(join_under_root(fs_root, "dev/fuse")).is_ok();
    let camera_device_present =
        fs::symlink_metadata(join_under_root(fs_root, "dev/video0")).is_ok();

    UbuntuRuntimeProbe {
        distro_id,
        version_id,
        target_tier,
        effective_uid,
        apparmor_enabled,
        apparmor_restrict_unprivileged_userns,
        unprivileged_userns_clone,
        bwrap_apparmor_profile_present,
        bwrap_path,
        display_session,
        desktop_environment,
        pipewire_socket_present,
        pulse_socket_present,
        dev_fuse_present,
        camera_device_present,
    }
}

/// Checks whether `/etc/apparmor.d/` contains an active (uncommented) local `userns` profile
/// covering `binary_path`.
///
/// Note: Even when this returns `true`, [`APPARMOR_PROFILE_NON_GUARANTEE_WARNING`] applies—an
/// outer-path profile does not guarantee FUSE-mounted inner binaries or nested `bwrap` children
/// will succeed.
#[must_use]
pub fn has_targeted_apparmor_userns_profile(fs_root: &Path, binary_path: &Path) -> bool {
    let apparmor_dir = join_under_root(fs_root, "etc/apparmor.d");
    let Ok(entries) = fs::read_dir(apparmor_dir) else {
        return false;
    };
    let needle = binary_path.to_string_lossy();
    let escaped_needle = escape_apparmor_path(&needle);
    let virt_paths = if fs_root != Path::new("/") && binary_path.starts_with(fs_root) {
        let virt = normalize_virtual_linux_path(binary_path, fs_root);
        let escaped_virt = escape_apparmor_path(&virt);
        Some((virt, escaped_virt))
    } else {
        None
    };

    for entry in entries.flatten() {
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let Ok(raw) = fs::read_to_string(&p) else {
            continue;
        };
        let active_body = strip_apparmor_comments(&raw);
        if !active_body.contains("userns") {
            continue;
        }
        let matches_direct = contains_exact_profile_path(&active_body, needle.as_ref())
            || contains_exact_profile_path(&active_body, &escaped_needle);
        let matches_virt = virt_paths.as_ref().is_some_and(|(v, ev)| {
            contains_exact_profile_path(&active_body, v)
                || contains_exact_profile_path(&active_body, ev)
        });
        if matches_direct || matches_virt {
            return true;
        }
    }
    false
}

/// Runs live Ubuntu/Linux preflight classification for an AppImage or Electron binary against `/`.
#[must_use]
pub fn preflight_live_appimage(
    binary_path: &Path,
    profile_dir: Option<&Path>,
    args: &[String],
    require_bwrap: bool,
) -> AppImagePreflightReport {
    let probe = probe_live_ubuntu_runtime();
    evaluate_appimage_preflight(
        &probe,
        Path::new("/"),
        binary_path,
        profile_dir,
        args,
        require_bwrap,
    )
}

/// Evaluates structured preflight diagnostics for an Electron binary, AppImage, or AppDir,
/// cleanly distinguishing:
/// - **(a) User namespace / AppArmor restrictions** (`apparmor_restrict_unprivileged_userns=1`, `unprivileged_userns_clone=0`, `bwrap`),
/// - **(b) Filesystem permission & mount issues** (`+x`, `noexec`/`nosuid` from `/proc/mounts`, unreadable file, `0o777` or wrong-UID profile dir),
/// - **(c) Electron / AppImage runtime environment issues** (`/dev/fuse`, headless display, static ELF64 `DT_NEEDED` missing libraries, header/arch).
///
/// Strictly preserves security invariants: never disables AppArmor, never suggests `--no-sandbox`,
/// never modifies `/etc/apparmor.d`, and never executes any binary or `ldd`.
#[must_use]
pub fn evaluate_appimage_preflight(
    probe: &UbuntuRuntimeProbe,
    fs_root: &Path,
    binary_path: &Path,
    profile_dir: Option<&Path>,
    args: &[String],
    require_bwrap: bool,
) -> AppImagePreflightReport {
    let mut diagnostics = Vec::new();
    let mut advisories = Vec::new();
    let display_bin = binary_path.display().to_string();

    // 1. Hard security policy guards: root & forbidden --no-sandbox flags.
    if probe.effective_uid == 0 {
        diagnostics.push(PreflightDiagnostic {
            kind: PreflightDiagnosticKind::RunningAsRoot { euid: 0 },
            severity: PreflightSeverity::Blocking,
            summary:
                "Refusing to launch desktop profile manager or target application as root (euid=0)."
                    .to_owned(),
            remediation: "Run as a standard unprivileged desktop user account (UID >= 1000)."
                .to_owned(),
        });
    }

    for arg in args {
        let trimmed = arg.trim();
        if is_forbidden_sandbox_flag(trimmed) {
            diagnostics.push(PreflightDiagnostic {
                kind: PreflightDiagnosticKind::ForbiddenNoSandboxFlag {
                    flag: trimmed.to_owned(),
                },
                severity: PreflightSeverity::Blocking,
                summary: format!(
                    "CLI argument '{trimmed}' disables Chromium process sandboxing and is forbidden by policy."
                ),
                remediation: "Remove '--no-sandbox' / sandbox-disabling flags; never weaken Chromium or AppArmor security."
                    .to_owned(),
            });
        }
    }

    // 2. Profile directory ownership & 0o700 permission hygiene (Category b: FilesystemAndMount).
    if let Some(pdir) = profile_dir {
        if let Err(SecurityBlockReason::InsecurePermissions { path, detail }) =
            verify_directory_ownership_and_mode(pdir, probe.effective_uid, true)
        {
            diagnostics.push(PreflightDiagnostic {
                kind: PreflightDiagnosticKind::InsecureFilesystemPermissions { path, detail },
                severity: PreflightSeverity::Blocking,
                summary: format!(
                    "Profile directory '{}' has unsafe ownership, symlink indirection, or permissive mode bits (e.g. 0o777).",
                    pdir.display()
                ),
                remediation: "Ensure profile directory is owned by the active UID and restricted to mode 0700 (`chmod 0700`)."
                    .to_owned(),
            });
        }
    }

    // 3. Binary existence & readability check (Category b: FilesystemAndMount).
    let target_meta = match fs::metadata(binary_path) {
        Ok(m) => m,
        Err(err) => {
            diagnostics.push(PreflightDiagnostic {
                kind: PreflightDiagnosticKind::UnreadableOrMissingBinary {
                    path: display_bin,
                    detail: err.to_string(),
                },
                severity: PreflightSeverity::Blocking,
                summary: format!("Target binary is missing or unreadable: {err}."),
                remediation: "Verify the path exists and has owner read+execute permissions (`0755` or `0700`)."
                    .to_owned(),
            });
            return finalize_preflight_report(binary_path, diagnostics, advisories);
        }
    };

    if target_meta.is_file() {
        if let Err(err) = File::open(binary_path) {
            diagnostics.push(PreflightDiagnostic {
                kind: PreflightDiagnosticKind::UnreadableOrMissingBinary {
                    path: display_bin,
                    detail: err.to_string(),
                },
                severity: PreflightSeverity::Blocking,
                summary: format!("Target binary exists in directory listing but cannot be opened for reading: {err}."),
                remediation: "Grant owner read+execute permissions (`chmod u+rx <file>`) on the binary."
                    .to_owned(),
            });
            return finalize_preflight_report(binary_path, diagnostics, advisories);
        }
    }

    // 4. Static binary inspection (zero execution, zero ldd).
    let report = match inspect_user_binary(binary_path, None) {
        Ok(r) => r,
        Err(err) => {
            diagnostics.push(PreflightDiagnostic {
                kind: PreflightDiagnosticKind::UnreadableOrMissingBinary {
                    path: display_bin,
                    detail: err.to_string(),
                },
                severity: PreflightSeverity::Blocking,
                summary: format!("Static binary inspection failed: {err}."),
                remediation: "Supply a readable local ELF64 binary, AppImage, or extracted AppDir outside protected primary Zalo paths."
                    .to_owned(),
            });
            return finalize_preflight_report(binary_path, diagnostics, advisories);
        }
    };

    match &report.security_block {
        Some(SecurityBlockReason::SetuidBinaryRejected { mode }) => {
            diagnostics.push(PreflightDiagnostic {
                kind: PreflightDiagnosticKind::SetuidOrSetgidBinary { mode: *mode },
                severity: PreflightSeverity::Blocking,
                summary: format!(
                    "Target binary carries POSIX setuid/setgid bit (mode={mode:#o})."
                ),
                remediation: "Refuse elevated setuid/setgid third-party binaries; clear setuid/setgid bits or replace with an unprivileged build."
                    .to_owned(),
            });
        }
        Some(SecurityBlockReason::UnsafeStartupScript { findings }) => {
            diagnostics.push(PreflightDiagnostic {
                kind: PreflightDiagnosticKind::UnsafeStartupScript {
                    findings: findings.clone(),
                },
                severity: PreflightSeverity::Blocking,
                summary: format!(
                    "Startup wrapper script contains unsafe commands: {}.",
                    findings.join("; ")
                ),
                remediation: "Remove privilege escalation, '--no-sandbox', 'killall'/'pkill', and hardcoded /home/<user> paths from wrapper scripts."
                    .to_owned(),
            });
        }
        Some(SecurityBlockReason::InsecurePermissions { path, detail }) => {
            diagnostics.push(PreflightDiagnostic {
                kind: PreflightDiagnosticKind::InsecureFilesystemPermissions {
                    path: path.clone(),
                    detail: detail.clone(),
                },
                severity: PreflightSeverity::Blocking,
                summary: format!("Target binary has insecure filesystem permissions ({detail})."),
                remediation: "Remove group/world write permissions (`chmod go-w <file>`)."
                    .to_owned(),
            });
        }
        _ => {}
    }

    if !report.is_executable {
        diagnostics.push(PreflightDiagnostic {
            kind: PreflightDiagnosticKind::MissingExecutableBit {
                path: report.path.display().to_string(),
            },
            severity: PreflightSeverity::Blocking,
            summary: format!(
                "Binary '{}' lacks POSIX execute permission (+x).",
                report.path.display()
            ),
            remediation: "Verify the file's SHA-256 integrity against a trusted reference digest, then run `chmod u+x <file>`."
                .to_owned(),
        });
    }

    if !report.has_valid_executable_header {
        diagnostics.push(PreflightDiagnostic {
            kind: PreflightDiagnosticKind::InvalidExecutableHeader {
                binary_path: report.path.display().to_string(),
                detail: format!(
                    "detected format '{}' lacks a structurally valid 64-byte ELF64 or '#!' executable header",
                    report.format.as_str()
                ),
            },
            severity: PreflightSeverity::Blocking,
            summary: format!(
                "File '{}' is truncated, corrupted, or not a valid Linux ELF64 / AppImage executable.",
                report.path.display()
            ),
            remediation: "Re-download or rebuild a valid x86_64/aarch64 ELF64 AppImage and verify its SHA-256 integrity."
                .to_owned(),
        });
    }

    if let Some(bin_arch) = report.arch {
        if let Some((expected_arch, host_label)) = expected_arch_for_probe(probe) {
            if bin_arch != expected_arch {
                diagnostics.push(PreflightDiagnostic {
                    kind: PreflightDiagnosticKind::CpuArchitectureMismatch {
                        binary_arch: bin_arch.as_str().to_owned(),
                        host_arch: host_label.to_owned(),
                    },
                    severity: PreflightSeverity::Blocking,
                    summary: format!(
                        "Binary targets '{}' but host runtime is '{host_label}'.",
                        bin_arch.as_str()
                    ),
                    remediation: format!(
                        "Use an AppImage or ELF64 build compiled for '{host_label}'."
                    ),
                });
            }
        }
    }

    // 5. Parse `/proc/mounts` for `noexec` and `nosuid` mount options (Category b: FilesystemAndMount).
    let mounts_path = join_under_root(fs_root, "proc/mounts");
    if let Ok(mounts_raw) = fs::read_to_string(&mounts_path) {
        let mounts = parse_proc_mounts(&mounts_raw);
        if let Some(mnt) = find_mount_for_path(&mounts, binary_path, fs_root) {
            let mnt_str = mnt.mount_point.display().to_string();
            if mnt.has_option("noexec") {
                diagnostics.push(PreflightDiagnostic {
                    kind: PreflightDiagnosticKind::MountOptionNoexec {
                        path: display_bin.clone(),
                        mount_point: mnt_str.clone(),
                    },
                    severity: PreflightSeverity::Blocking,
                    summary: format!(
                        "Binary resides on mount '{mnt_str}' which has the 'noexec' mount option in /proc/mounts; Linux execve(2) will fail with EACCES."
                    ),
                    remediation: "Move the executable to an user-owned directory on an exec-enabled filesystem (do not bypass 'noexec' via dynamic-linker invocation)."
                        .to_owned(),
                });
            }
            if mnt.has_option("nosuid") {
                diagnostics.push(PreflightDiagnostic {
                    kind: PreflightDiagnosticKind::MountOptionNosuid {
                        path: display_bin.clone(),
                        mount_point: mnt_str,
                    },
                    severity: PreflightSeverity::Warning,
                    summary: "Binary resides on a 'nosuid' mount; setuid-root chrome-sandbox helpers cannot elevate here (Chromium will rely on unprivileged user namespaces)."
                        .to_owned(),
                    remediation: "Ensure unprivileged user namespaces are available or keep the application inside an environment where userns sandboxing works."
                        .to_owned(),
                });
            }
        }

        // AppImages also mount or extract runtime executables under $TMPDIR (`profile_dir/tmp`) or `/tmp`.
        let is_appimage_container = matches!(
            report.format,
            BinaryFormatKind::AppImageType2 | BinaryFormatKind::AppImageUruntimeDwarfs
        ) || looks_like_appimage(binary_path);
        if is_appimage_container {
            let scratch_probe = profile_dir
                .map(|p| p.join("tmp"))
                .unwrap_or_else(|| join_under_root(fs_root, "tmp"));
            if let Some(tmp_mnt) = find_mount_for_path(&mounts, &scratch_probe, fs_root) {
                let tmp_mnt_str = tmp_mnt.mount_point.display().to_string();
                let already_flagged = diagnostics.iter().any(|d| {
                    matches!(
                        &d.kind,
                        PreflightDiagnosticKind::MountOptionNoexec { mount_point, .. }
                            if mount_point == &tmp_mnt_str
                    )
                });
                if tmp_mnt.has_option("noexec") && !already_flagged {
                    diagnostics.push(PreflightDiagnostic {
                        kind: PreflightDiagnosticKind::MountOptionNoexec {
                            path: scratch_probe.display().to_string(),
                            mount_point: tmp_mnt_str.clone(),
                        },
                        severity: PreflightSeverity::Blocking,
                        summary: format!(
                            "AppImage temporary mount/extract location '{}' resides on 'noexec' mount '{tmp_mnt_str}'.",
                            scratch_probe.display()
                        ),
                        remediation: "Place profile directories (or extract the AppDir) onto an exec-enabled filesystem."
                            .to_owned(),
                    });
                }
            }
        }
    }

    // 6. User namespace & Ubuntu 24.04 AppArmor policy checks (Category a: UserNamespaceAndAppArmor).
    let needs_chromium_userns = is_electron_or_appimage_target(binary_path, &report);

    if matches!(probe.unprivileged_userns_clone, Some(0))
        && (needs_chromium_userns || require_bwrap)
    {
        diagnostics.push(PreflightDiagnostic {
            kind: PreflightDiagnosticKind::KernelUnprivilegedUsernsDisabled,
            severity: PreflightSeverity::Blocking,
            summary: "Kernel sysctl 'kernel.unprivileged_userns_clone = 0' disables unprivileged user namespaces for both Chromium sandboxing and bubblewrap."
                .to_owned(),
            remediation: "Do NOT pass '--no-sandbox'; unprivileged namespace sandboxing requires a host kernel with unprivileged user namespaces enabled."
                .to_owned(),
        });
    }

    if require_bwrap {
        if let Err(SecurityBlockReason::BubblewrapBlocked { detail }) =
            probe.evaluate_bwrap_readiness()
        {
            if !matches!(probe.unprivileged_userns_clone, Some(0)) {
                diagnostics.push(PreflightDiagnostic {
                    kind: PreflightDiagnosticKind::BubblewrapUnavailableOrRestricted { detail },
                    severity: PreflightSeverity::Blocking,
                    summary: "Bubblewrap ('bwrap') containerization was requested but cannot initialize outer namespaces on this host."
                        .to_owned(),
                    remediation: "Install 'bubblewrap' (`sudo apt install bubblewrap`) and verify `/etc/apparmor.d/bwrap-userns-restrict` is present on Ubuntu 24.04."
                        .to_owned(),
                });
            }
        }
    }

    if needs_chromium_userns
        && probe.apparmor_enabled
        && matches!(probe.apparmor_restrict_unprivileged_userns, Some(1))
    {
        advisories.push(APPARMOR_PROFILE_NON_GUARANTEE_WARNING.to_owned());

        if !has_targeted_apparmor_userns_profile(fs_root, binary_path) {
            diagnostics.push(PreflightDiagnostic {
                kind: PreflightDiagnosticKind::AppArmorUnprivilegedUsernsRestricted {
                    binary_path: display_bin.clone(),
                },
                severity: PreflightSeverity::Blocking,
                summary: format!(
                    "Ubuntu 24.04 AppArmor ('kernel.apparmor_restrict_unprivileged_userns=1') blocks unprivileged user namespaces for unconfined target '{display_bin}'."
                ),
                remediation: format!(
                    "Never disable system-wide AppArmor or pass '--no-sandbox'. In isolated test environments only, an administrator may evaluate a targeted AppArmor 4.0 `userns,` profile (note: {APPARMOR_PROFILE_NON_GUARANTEE_WARNING})"
                ),
            });
        } else {
            let caveat_detail = if require_bwrap && probe.bwrap_apparmor_profile_present {
                "Targeted AppArmor `userns,` profile detected, but Ubuntu 24.04 `/etc/apparmor.d/bwrap-userns-restrict` transitions child processes inside `bwrap` to `bwrap-unprivileged` and FUSE AppImages re-exec from `/tmp/.mount_*/...`, so inner Chromium userns creation is NOT guaranteed to succeed."
            } else {
                "Targeted AppArmor `userns,` profile detected for outer path, but FUSE-mounted AppImages execute inner binaries from `/tmp/.mount_*/...` and Electron 22 Wayland/sandbox transitions may still fail; AppArmor `userns` profiles are NOT guaranteed to make every third-party AppImage work."
            };
            diagnostics.push(PreflightDiagnostic {
                kind: PreflightDiagnosticKind::NestedUsernsTransitionCaveat {
                    detail: caveat_detail.to_owned(),
                },
                severity: PreflightSeverity::Warning,
                summary: caveat_detail.to_owned(),
                remediation: "Treat third-party AppImage execution as Experimental; if FUSE mount indirection blocks inner Chromium userns in an isolated test VM, evaluate an extracted AppDir whose inner Electron binary path is explicitly known."
                    .to_owned(),
            });
        }
    } else if looks_like_appimage(binary_path) {
        advisories.push(APPARMOR_PROFILE_NON_GUARANTEE_WARNING.to_owned());
    }

    // 7. Electron / AppImage runtime environment checks (Category c: RuntimeEnvironment).
    if matches!(report.format, BinaryFormatKind::AppImageType2) && !probe.dev_fuse_present {
        diagnostics.push(PreflightDiagnostic {
            kind: PreflightDiagnosticKind::MissingDevFuse {
                binary_path: display_bin.clone(),
            },
            severity: PreflightSeverity::Blocking,
            summary: format!(
                "Type-2 SquashFS AppImage '{display_bin}' requires `/dev/fuse` to self-mount, but `/dev/fuse` is missing."
            ),
            remediation: "Ensure `/dev/fuse` and `libfuse2t64` (Ubuntu 24.04) are available on the desktop host, or use a pre-extracted AppDir."
                .to_owned(),
        });
    } else if matches!(report.format, BinaryFormatKind::AppImageUruntimeDwarfs)
        && !probe.dev_fuse_present
    {
        diagnostics.push(PreflightDiagnostic {
            kind: PreflightDiagnosticKind::MissingDevFuse {
                binary_path: display_bin.clone(),
            },
            severity: PreflightSeverity::Warning,
            summary: format!(
                "uruntime/DwarFS AppImage '{display_bin}' has no `/dev/fuse` device available; FUSE3 mounting will fail unless extraction mode is used."
            ),
            remediation: "Enable `/dev/fuse` with `fuse3` (`libfuse3-3`) or pre-extract the AppImage into an isolated AppDir."
                .to_owned(),
        });
    }

    if probe.display_session == DisplaySessionKind::Headless {
        diagnostics.push(PreflightDiagnostic {
            kind: PreflightDiagnosticKind::HeadlessDisplayMissing {
                binary_path: display_bin,
            },
            severity: PreflightSeverity::Blocking,
            summary: "Session is headless (both `WAYLAND_DISPLAY` and `DISPLAY` are unset); Electron/GTK graphical windows cannot launch."
                .to_owned(),
            remediation: "Launch from an active Ubuntu desktop Wayland or X11 session (or virtual Xvfb/Wayland compositor in CI fixture tests)."
                .to_owned(),
        });
    }

    // Resolve static ELF64 DT_NEEDED missing shared libraries against `fs_root` when system lib dirs exist.
    let mut missing_libs = report.missing_libraries.clone();
    if let Some(elf_meta) = &report.elf_dynamic {
        let has_linux_lib_tree = join_under_root(fs_root, "lib").is_dir()
            || join_under_root(fs_root, "usr/lib").is_dir()
            || join_under_root(fs_root, "lib64").is_dir();
        if has_linux_lib_tree {
            let elf_origin_bin = if report.format == BinaryFormatKind::ExtractedAppDir
                && binary_path.join("zalo").is_file()
            {
                binary_path.join("zalo")
            } else {
                report.path.clone()
            };
            missing_libs = resolve_elf64_missing_libraries_at(elf_meta, &elf_origin_bin, fs_root);
        }
    }

    if !missing_libs.is_empty() {
        let names = missing_libs
            .iter()
            .map(|m| m.soname.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let pkgs_2404 = missing_libs
            .iter()
            .map(|m| m.ubuntu_2404_package)
            .collect::<Vec<_>>()
            .join(", ");
        diagnostics.push(PreflightDiagnostic {
            kind: PreflightDiagnosticKind::MissingSharedLibraries {
                libraries: missing_libs,
            },
            severity: PreflightSeverity::Blocking,
            summary: format!(
                "Static ELF64 DT_NEEDED inspection found missing shared libraries: {names}."
            ),
            remediation: format!(
                "Install required Ubuntu 24.04 shared library packages via apt: {pkgs_2404}."
            ),
        });
    }

    finalize_preflight_report(binary_path, diagnostics, advisories)
}

/// Validates CLI flags and host security invariants before launching any target process.
///
/// Enforces:
/// 1. Never run as `root` (`effective_uid == 0`).
/// 2. Never pass `--no-sandbox` or `--disable-setuid-sandbox`.
/// 3. If `is_electron_appimage` is true and Ubuntu 24.04 AppArmor blocks unconfined `userns` without a
///    matching `/etc/apparmor.d/` profile, return `SECURITY_BLOCKED` rather than weakening security.
pub fn enforce_launch_security_policy(
    probe: &UbuntuRuntimeProbe,
    fs_root: &Path,
    binary_path: &Path,
    args: &[String],
    is_electron_appimage: bool,
    require_bwrap: bool,
) -> Result<(), SecurityBlockReason> {
    if probe.effective_uid == 0 {
        return Err(SecurityBlockReason::RunningAsRoot { euid: 0 });
    }

    for arg in args {
        let trimmed = arg.trim();
        if is_forbidden_sandbox_flag(trimmed) {
            return Err(SecurityBlockReason::NoSandboxForbidden {
                flag: trimmed.to_owned(),
            });
        }
    }

    if require_bwrap {
        probe.evaluate_bwrap_readiness()?;
    }

    if is_electron_appimage
        && probe.unconfined_userns_restricted()
        && !has_targeted_apparmor_userns_profile(fs_root, binary_path)
    {
        return Err(SecurityBlockReason::AppArmorUsernsRestricted {
            binary_path: binary_path.display().to_string(),
        });
    }

    Ok(())
}

/// Generates an AppArmor 4.0 `userns` profile snippet scoped strictly to **isolated test environments**
/// (documentation/advisory output only; `dim-platform-linux` NEVER writes to `/etc/apparmor.d`,
/// NEVER disables system-wide AppArmor, and NEVER invokes `sudo`).
///
/// Explicitly warns that an AppArmor `userns` profile is **not guaranteed** to make every third-party
/// AppImage work (FUSE `/tmp/.mount_*` inner binaries, nested `bwrap-userns-restrict` transitions,
/// `noexec` mounts, missing FUSE2/3, or Electron 22 Wayland bugs may still block execution).
#[must_use]
pub fn render_recommended_apparmor_profile(appimage_path: &Path) -> String {
    let escaped_path = escape_apparmor_path(&appimage_path.to_string_lossy());
    format!(
        concat!(
            "# ISOLATED TEST ENVIRONMENT ADVISORY SNIPPET ONLY.\n",
            "# dim-platform-linux NEVER writes to /etc/apparmor.d automatically, NEVER runs sudo,\n",
            "# NEVER disables system-wide AppArmor, and NEVER auto-appends --no-sandbox.\n",
            "# WARNING: An AppArmor `userns` profile is NOT guaranteed to make every third-party\n",
            "# AppImage work (FUSE-mounted /tmp/.mount_* inner binaries, nested bwrap transitions\n",
            "# under /etc/apparmor.d/bwrap-userns-restrict, `noexec` mounts, missing FUSE2/3, or\n",
            "# Electron 22 Wayland bugs may still block execution).\n",
            "abi <abi/4.0>,\n",
            "include <tunables/global>\n\n",
            "profile dim_zalo_appimage \"{}\" flags=(unconfined) {{\n",
            "  userns,\n",
            "}}\n"
        ),
        escaped_path
    )
}

/// Queries the current process effective UID (`geteuid` on Unix, `1000` non-root placeholder on non-Unix).
#[must_use]
pub fn current_effective_uid() -> u32 {
    #[cfg(unix)]
    {
        // SAFETY: `geteuid` is thread-safe, takes no pointers, and cannot fail.
        unsafe { libc::geteuid() }
    }
    #[cfg(not(unix))]
    {
        1000
    }
}

/// Locates an executable file in the current `PATH`.
#[must_use]
pub fn find_executable_in_path(bin_name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let cand = dir.join(bin_name);
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn finalize_preflight_report(
    binary_path: &Path,
    diagnostics: Vec<PreflightDiagnostic>,
    advisories: Vec<String>,
) -> AppImagePreflightReport {
    let has_security_block = diagnostics.iter().any(|d| {
        d.severity == PreflightSeverity::Blocking
            && (d.category() == PreflightCategory::SecurityPolicyGuard
                || matches!(
                    d.kind,
                    PreflightDiagnosticKind::KernelUnprivilegedUsernsDisabled
                        | PreflightDiagnosticKind::AppArmorUnprivilegedUsernsRestricted { .. }
                        | PreflightDiagnosticKind::BubblewrapUnavailableOrRestricted { .. }
                        | PreflightDiagnosticKind::InsecureFilesystemPermissions { .. }
                ))
    });
    let has_any_block = diagnostics
        .iter()
        .any(|d| d.severity == PreflightSeverity::Blocking);

    let verification_label = if has_security_block {
        VerificationLabel::SecurityBlocked
    } else if has_any_block {
        VerificationLabel::Blocked
    } else {
        VerificationLabel::NotTested
    };

    AppImagePreflightReport {
        binary_path: binary_path.to_path_buf(),
        ready_to_launch: !has_any_block,
        verification_label,
        diagnostics,
        advisories,
    }
}

fn is_forbidden_sandbox_flag(arg: &str) -> bool {
    arg == "--no-sandbox"
        || arg.starts_with("--no-sandbox=")
        || arg == "--disable-setuid-sandbox"
        || arg == "--disable-namespace-sandbox"
}

fn is_electron_or_appimage_target(binary_path: &Path, report: &BinaryInspectionReport) -> bool {
    if matches!(
        report.format,
        BinaryFormatKind::AppImageType2
            | BinaryFormatKind::AppImageUruntimeDwarfs
            | BinaryFormatKind::ExtractedAppDir
    ) || looks_like_appimage(binary_path)
    {
        return true;
    }

    if let Some(elf) = &report.elf_dynamic {
        return elf.needed_libraries.iter().any(|lib| {
            matches!(
                lib.as_str(),
                "libgtk-3.so.0" | "libnss3.so" | "libatk-bridge-2.0.so.0" | "libffmpeg.so"
            )
        });
    }

    false
}

fn expected_arch_for_probe(probe: &UbuntuRuntimeProbe) -> Option<(ElfCpuArch, &'static str)> {
    match probe.target_tier {
        UbuntuTargetTier::Ubuntu2404X86_64 | UbuntuTargetTier::Ubuntu2204X86_64 => {
            Some((ElfCpuArch::X86_64, "x86_64"))
        }
        UbuntuTargetTier::UbuntuLtsArm64 => Some((ElfCpuArch::Aarch64, "aarch64")),
        UbuntuTargetTier::OtherLinux => {
            if cfg!(target_arch = "x86_64") {
                Some((ElfCpuArch::X86_64, "x86_64"))
            } else if cfg!(target_arch = "aarch64") {
                Some((ElfCpuArch::Aarch64, "aarch64"))
            } else {
                None
            }
        }
    }
}

fn strip_apparmor_comments(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for line in raw.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            continue;
        }
        let code_part = match line.find('#') {
            Some(idx) => &line[..idx],
            None => line,
        };
        out.push_str(code_part);
        out.push('\n');
    }
    out
}

fn escape_apparmor_path(raw: &str) -> String {
    let mut escaped = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            other => escaped.push(other),
        }
    }
    escaped
}

fn contains_exact_profile_path(active_body: &str, candidate: &str) -> bool {
    if candidate.is_empty() || candidate == "/" {
        return false;
    }
    for (start, _) in active_body.match_indices(candidate) {
        let end = start + candidate.len();
        let before_ok = start == 0
            || active_body[..start]
                .chars()
                .next_back()
                .is_some_and(|c| c == '"' || c.is_whitespace());
        let after_ok = end == active_body.len()
            || active_body[end..]
                .chars()
                .next()
                .is_some_and(|c| matches!(c, '"' | '{' | ',') || c.is_whitespace());
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

fn unescape_proc_mount_token(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\'
            && i + 3 < bytes.len()
            && (b'0'..=b'3').contains(&bytes[i + 1])
            && (b'0'..=b'7').contains(&bytes[i + 2])
            && (b'0'..=b'7').contains(&bytes[i + 3])
        {
            let val =
                ((bytes[i + 1] - b'0') << 6) | ((bytes[i + 2] - b'0') << 3) | (bytes[i + 3] - b'0');
            out.push(val);
            i += 4;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn normalize_virtual_linux_path(target_path: &Path, fs_root: &Path) -> String {
    if let Ok(rel) = target_path.strip_prefix(fs_root) {
        let mut s = String::from("/");
        let mut first = true;
        for comp in rel.components() {
            if let std::path::Component::Normal(os) = comp {
                if !first {
                    s.push('/');
                }
                s.push_str(&os.to_string_lossy());
                first = false;
            }
        }
        return s;
    }
    target_path.to_string_lossy().replace('\\', "/")
}

fn parse_os_release(raw: &str) -> (String, String) {
    let mut id = String::from("linux");
    let mut version_id = String::from("unknown");
    for line in raw.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("ID=") {
            id = rest.trim_matches(['"', '\'']).to_owned();
        } else if let Some(rest) = trimmed.strip_prefix("VERSION_ID=") {
            version_id = rest.trim_matches(['"', '\'']).to_owned();
        }
    }
    (id, version_id)
}

fn read_sysctl_u8(path: &Path) -> Option<u8> {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse::<u8>().ok())
}

fn join_under_root(root: &Path, rel: &str) -> PathBuf {
    let mut out = root.to_path_buf();
    for seg in rel.split('/').filter(|s| !s.is_empty()) {
        out.push(seg);
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::collections::HashMap;

    use super::*;

    fn make_valid_type2_appimage_with_needed(needed: &[&str]) -> Vec<u8> {
        let mut dynstr = vec![0u8];
        let mut offs = Vec::new();
        for s in needed {
            offs.push(dynstr.len() as u64);
            dynstr.extend_from_slice(s.as_bytes());
            dynstr.push(0);
        }
        let base_off: u64 = 0x200;
        let base_vaddr: u64 = 0x0040_0200;
        let mut payload = dynstr.clone();
        while payload.len() % 8 != 0 {
            payload.push(0);
        }
        let dyn_rel = payload.len() as u64;
        let mut push_dyn = |t: u64, v: u64| {
            payload.extend_from_slice(&t.to_le_bytes());
            payload.extend_from_slice(&v.to_le_bytes());
        };
        push_dyn(5, base_vaddr); // DT_STRTAB
        push_dyn(10, dynstr.len() as u64); // DT_STRSZ
        for off in offs {
            push_dyn(1, off); // DT_NEEDED
        }
        push_dyn(0, 0); // DT_NULL
        let dyn_sz = payload.len() as u64 - dyn_rel;
        let filesz = payload.len() as u64;

        let make_ph = |ty: u32, off: u64, va: u64, sz: u64| {
            let mut p = [0u8; 56];
            p[0..4].copy_from_slice(&ty.to_le_bytes());
            p[4..8].copy_from_slice(&5u32.to_le_bytes());
            p[8..16].copy_from_slice(&off.to_le_bytes());
            p[16..24].copy_from_slice(&va.to_le_bytes());
            p[24..32].copy_from_slice(&va.to_le_bytes());
            p[32..40].copy_from_slice(&sz.to_le_bytes());
            p[40..48].copy_from_slice(&sz.to_le_bytes());
            p[48..56].copy_from_slice(&0x1000u64.to_le_bytes());
            p
        };

        let mut out = vec![0u8; base_off as usize];
        out[0..4].copy_from_slice(b"\x7fELF");
        out[4] = 2;
        out[5] = 1;
        out[6] = 1;
        out[8..11].copy_from_slice(b"AI\x02");
        out[16..18].copy_from_slice(&2u16.to_le_bytes());
        out[18..20].copy_from_slice(&0x3Eu16.to_le_bytes());
        out[32..40].copy_from_slice(&64u64.to_le_bytes());
        out[52..54].copy_from_slice(&64u16.to_le_bytes());
        out[54..56].copy_from_slice(&56u16.to_le_bytes());
        out[56..58].copy_from_slice(&2u16.to_le_bytes());
        out[64..120].copy_from_slice(&make_ph(1, base_off, base_vaddr, filesz)); // PT_LOAD
        out[120..176].copy_from_slice(&make_ph(
            2,
            base_off + dyn_rel,
            base_vaddr + dyn_rel,
            dyn_sz,
        )); // PT_DYNAMIC
        out.extend_from_slice(&payload);
        out
    }

    fn setup_ubuntu2404_fs(root: &Path) {
        fs::create_dir_all(root.join("etc/apparmor.d")).unwrap();
        fs::create_dir_all(root.join("sys/module/apparmor/parameters")).unwrap();
        fs::create_dir_all(root.join("proc/sys/kernel")).unwrap();
        fs::create_dir_all(root.join("usr/lib/x86_64-linux-gnu")).unwrap();
        fs::create_dir_all(root.join("tmp")).unwrap();
        fs::create_dir_all(root.join("opt/apps")).unwrap();

        fs::write(
            root.join("etc/os-release"),
            "ID=ubuntu\nVERSION_ID=\"24.04\"\n",
        )
        .unwrap();
        fs::write(root.join("sys/module/apparmor/parameters/enabled"), "Y\n").unwrap();
        fs::write(
            root.join("proc/sys/kernel/apparmor_restrict_unprivileged_userns"),
            "1\n",
        )
        .unwrap();
        fs::write(
            root.join("proc/sys/kernel/unprivileged_userns_clone"),
            "1\n",
        )
        .unwrap();
        fs::write(
            root.join("etc/apparmor.d/bwrap-userns-restrict"),
            "profile bwrap /usr/bin/bwrap flags=(unconfined) { userns, }\n",
        )
        .unwrap();
        fs::write(
            root.join("proc/mounts"),
            concat!(
                "/dev/nvme0n1p2 / ext4 rw,relatime 0 0\n",
                "tmpfs /tmp/noexec\\040zone tmpfs rw,nosuid,nodev,noexec 0 0\n",
            ),
        )
        .unwrap();
        fs::write(root.join("usr/lib/x86_64-linux-gnu/libc.so.6"), b"elf").unwrap();
    }

    #[test]
    fn proc_mounts_unescapes_spaces_and_matches_longest_mount_prefix() {
        let sample = concat!(
            "/dev/sda1 / ext4 rw,relatime 0 0\n",
            "tmpfs /tmp tmpfs rw,nosuid,nodev 0 0\n",
            "tmpfs /tmp/My\\040Downloads tmpfs rw,nosuid,nodev,noexec 0 0\n",
        );
        let mounts = parse_proc_mounts(sample);
        assert_eq!(mounts.len(), 3);
        assert_eq!(mounts[2].mount_point, PathBuf::from("/tmp/My Downloads"));
        assert!(mounts[2].has_option("noexec"));
        assert!(mounts[2].has_option("nosuid"));

        // `/tmp2/app` must match `/`, NEVER `/tmp`!
        let m_root =
            find_mount_for_path(&mounts, Path::new("/tmp2/Zalo.AppImage"), Path::new("/")).unwrap();
        assert_eq!(m_root.mount_point, PathBuf::from("/"));
        assert!(!m_root.has_option("noexec"));

        // `/tmp/My Downloads/Zalo.AppImage` must match `/tmp/My Downloads` (`noexec`)!
        let m_dl = find_mount_for_path(
            &mounts,
            Path::new("/tmp/My Downloads/Zalo.AppImage"),
            Path::new("/"),
        )
        .unwrap();
        assert_eq!(m_dl.mount_point, PathBuf::from("/tmp/My Downloads"));
        assert!(m_dl.has_option("noexec"));
    }

    #[test]
    fn apparmor_profile_ignores_commented_rules_and_warns_non_guarantee() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("fs");
        fs::create_dir_all(root.join("etc/apparmor.d")).unwrap();
        let appimage = Path::new("/opt/My Apps/Zalo-26.11.10.AppImage");

        // Commented-out rule must NOT count as an active userns profile!
        fs::write(
            root.join("etc/apparmor.d/commented-out"),
            "# profile fake \"/opt/My Apps/Zalo-26.11.10.AppImage\" flags=(unconfined) { userns, }\n",
        )
        .unwrap();
        assert!(!has_targeted_apparmor_userns_profile(&root, appimage));

        // Stock Ubuntu 24.04 system profile for `/usr/lib/electron/electron` must NEVER falsely
        // authorize an unconfined binary sharing the same filename (`/home/user/Downloads/electron`),
        // nor suffix/prefix-colliding paths!
        fs::write(
            root.join("etc/apparmor.d/electron"),
            "abi <abi/4.0>,\nprofile electron /usr/lib/electron/electron flags=(unconfined) {\n  userns,\n}\n",
        )
        .unwrap();
        assert!(!has_targeted_apparmor_userns_profile(
            &root,
            Path::new("/home/user/Downloads/electron")
        ));
        assert!(!has_targeted_apparmor_userns_profile(
            &root,
            Path::new("/home/user/Downloads/usr/lib/electron/electron")
        ));
        assert!(!has_targeted_apparmor_userns_profile(
            &root,
            Path::new("/usr/lib/electron/electron-untrusted")
        ));
        assert!(has_targeted_apparmor_userns_profile(
            &root,
            Path::new("/usr/lib/electron/electron")
        ));

        let snippet = render_recommended_apparmor_profile(appimage);
        assert!(snippet.contains("ISOLATED TEST ENVIRONMENT"));
        assert!(snippet.contains("NOT guaranteed to make every third-party"));
        assert!(!snippet.contains("\n  --no-sandbox"));

        fs::write(root.join("etc/apparmor.d/active-test-profile"), &snippet).unwrap();
        assert!(has_targeted_apparmor_userns_profile(&root, appimage));

        // Virtual-root normalized Linux path matching (`root.join("opt/virtual/Zalo.AppImage")` -> `"/opt/virtual/Zalo.AppImage"`):
        fs::write(
            root.join("etc/apparmor.d/virtual-zalo"),
            "profile dim_virt \"/opt/virtual/Zalo.AppImage\" flags=(unconfined) { userns, }\n",
        )
        .unwrap();
        assert!(has_targeted_apparmor_userns_profile(
            &root,
            &root.join("opt/virtual/Zalo.AppImage")
        ));
    }

    #[test]
    fn preflight_cleanly_distinguishes_userns_filesystem_and_runtime_env_failures() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("ubuntu2404");
        setup_ubuntu2404_fs(&root);

        let noexec_dir = root.join("tmp/noexec zone");
        fs::create_dir_all(&noexec_dir).unwrap();
        let app_in_noexec = noexec_dir.join("Zalo-26.11.10.AppImage");
        let elf_bytes = make_valid_type2_appimage_with_needed(&["libc.so.6", "libopus.so.0"]);
        fs::write(&app_in_noexec, &elf_bytes).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&app_in_noexec, fs::Permissions::from_mode(0o755)).unwrap();
        }

        // Headless probe (`WAYLAND_DISPLAY` & `DISPLAY` unset), `/dev/fuse` missing, `apparmor_restrict_unprivileged_userns=1`
        let headless_probe = probe_ubuntu_runtime_at(&root, "x86_64", 1000, &|_| None, None);
        let rep =
            evaluate_appimage_preflight(&headless_probe, &root, &app_in_noexec, None, &[], false);

        assert!(!rep.ready_to_launch);
        assert_eq!(rep.verification_label, VerificationLabel::SecurityBlocked);
        // (a) User namespace / AppArmor restriction detected:
        assert!(rep.has_blocking_category(PreflightCategory::UserNamespaceAndAppArmor));
        // (b) Filesystem `noexec` mount detected:
        assert!(rep.has_blocking_category(PreflightCategory::FilesystemAndMount));
        assert!(rep
            .diagnostics
            .iter()
            .any(|d| d.kind.code() == "MOUNT_OPTION_NOEXEC"));
        // (c) Runtime environment issues (`/dev/fuse`, headless display, missing `libopus.so.0`) ALL detected:
        assert!(rep.has_blocking_category(PreflightCategory::RuntimeEnvironment));
        assert!(rep
            .diagnostics
            .iter()
            .any(|d| d.kind.code() == "MISSING_DEV_FUSE"));
        assert!(rep
            .diagnostics
            .iter()
            .any(|d| d.kind.code() == "HEADLESS_DISPLAY_MISSING"));
        assert!(rep.diagnostics.iter().any(|d| matches!(
            &d.kind,
            PreflightDiagnosticKind::MissingSharedLibraries { libraries }
                if libraries.len() == 1 && libraries[0].soname == "libopus.so.0"
        )));
        assert!(rep
            .advisories
            .iter()
            .any(|a| a.contains("NOT guaranteed to make every third-party AppImage work")));

        // Now resolve all 3 categories in an isolated virtual desktop environment and verify readiness + non-guarantee caveat!
        let exec_app = root.join("opt/apps/Zalo-26.11.10.AppImage");
        fs::write(&exec_app, &elf_bytes).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&exec_app, fs::Permissions::from_mode(0o755)).unwrap();
        }
        fs::create_dir_all(root.join("dev")).unwrap();
        fs::write(root.join("dev/fuse"), b"").unwrap();
        fs::write(root.join("usr/lib/x86_64-linux-gnu/libopus.so.0"), b"elf").unwrap();
        fs::write(
            root.join("etc/apparmor.d/test-zalo"),
            render_recommended_apparmor_profile(&exec_app),
        )
        .unwrap();

        let mut desktop_env = HashMap::new();
        desktop_env.insert("XDG_SESSION_TYPE", "wayland");
        desktop_env.insert("WAYLAND_DISPLAY", "wayland-0");
        let fake_bwrap = root.join("opt/apps/bwrap");
        fs::write(&fake_bwrap, b"#!/bin/sh\n").unwrap();

        let wayland_probe = probe_ubuntu_runtime_at(
            &root,
            "x86_64",
            1000,
            &|k| desktop_env.get(k).map(|v| (*v).to_owned()),
            Some(fake_bwrap),
        );
        let ready_rep =
            evaluate_appimage_preflight(&wayland_probe, &root, &exec_app, None, &[], true);
        assert!(ready_rep.ready_to_launch);
        assert_eq!(ready_rep.verification_label, VerificationLabel::NotTested);
        // Must still emit Warning `NestedUsernsTransitionCaveat` rather than promising AppImages are guaranteed to work!
        assert!(ready_rep.diagnostics.iter().any(|d| {
            d.severity == PreflightSeverity::Warning
                && d.kind.code() == "NESTED_USERNS_TRANSITION_CAVEAT"
        }));
    }
}

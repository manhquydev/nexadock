//! Ubuntu 24.04 / 22.04 isolation model comparison, AppArmor `userns` probing,
//! non-root enforcement, and `SECURITY_BLOCKED` policy guards.
//!
//! Explicitly distinguishes **profile data separation** (`HOME`/`XDG_*` redirection, where a
//! compromised process still runs with the user's full ambient Linux UID authority) from a **true
//! OS sandbox** (`bubblewrap` mount/IPC namespaces, Landlock LSM, seccomp-bpf, and Chromium's
//! internal `chrome-sandbox` user/PID/net namespaces).

use std::fs;
use std::path::{Path, PathBuf};

use crate::status::{SecurityBlockReason, UbuntuTargetTier};

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
            summary: "True filesystem/IPC sandbox hiding real $HOME; Ubuntu 24.04 ships /etc/apparmor.d/bwrap-userns-restrict for bwrap, but inner Electron chrome-sandbox nested userns can still be denied without a profile.",
        },
        IsolationComparisonEntry {
            mechanism: LinuxIsolationMechanism::UserNamespaceIsolation,
            separates_profile_data: false,
            is_true_security_sandbox: true,
            isolates_loopback_tcp_ports: false,
            works_stock_ubuntu_2404_unprivileged: false,
            summary: "Blocked for unconfined binaries on Ubuntu 23.10+/24.04 by kernel.apparmor_restrict_unprivileged_userns=1 unless granted `userns,` in /etc/apparmor.d/.",
        },
        IsolationComparisonEntry {
            mechanism: LinuxIsolationMechanism::ComplementaryOsControls,
            separates_profile_data: false,
            is_true_security_sandbox: true,
            isolates_loopback_tcp_ports: false,
            works_stock_ubuntu_2404_unprivileged: true,
            summary: "Landlock LSM (Linux 5.13+, no userns needed) + systemd --user cgroup v2 scopes + admin-installed targeted AppArmor `userns,` profile keep full security without --no-sandbox.",
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

    /// Checks whether ` bubblewrap ` (`bwrap`) can initialize outer namespaces on this host.
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

/// Checks whether `/etc/apparmor.d/` contains a local `userns` profile covering `binary_path`.
#[must_use]
pub fn has_targeted_apparmor_userns_profile(fs_root: &Path, binary_path: &Path) -> bool {
    let apparmor_dir = join_under_root(fs_root, "etc/apparmor.d");
    let Ok(entries) = fs::read_dir(apparmor_dir) else {
        return false;
    };
    let needle = binary_path.to_string_lossy();
    let file_name = binary_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();

    for entry in entries.flatten() {
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let Ok(raw) = fs::read_to_string(&p) else {
            continue;
        };
        if raw.contains("userns")
            && ((!needle.is_empty() && raw.contains(needle.as_ref()))
                || (!file_name.is_empty() && raw.contains(file_name)))
        {
            return true;
        }
    }
    false
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
        if trimmed == "--no-sandbox"
            || trimmed == "--disable-setuid-sandbox"
            || trimmed == "--disable-namespace-sandbox"
        {
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

/// Generates a least-privilege AppArmor 4.0 profile snippet (documentation/advisory only;
/// `dim-platform-linux` NEVER writes to `/etc/apparmor.d` or invokes `sudo`).
#[must_use]
pub fn render_recommended_apparmor_profile(appimage_path: &Path) -> String {
    let display_path = appimage_path.display();
    format!(
        "abi <abi/4.0>,\ninclude <tunables/global>\n\nprofile dim_zalo_appimage \"{display_path}\" flags=(unconfined) {{\n  userns,\n}}\n"
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

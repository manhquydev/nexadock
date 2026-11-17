//! Verification labels, security block classifications, structured preflight diagnostic kinds,
//! and Ubuntu target metadata.
//!
//! Every Linux adapter report distinguishes adapter logic verification, synthetic fixture testing,
//! community-port live testing, official VNG Zalo non-existence, untested states, and blocked
//! security/runtime states—never collapsing them into a single `"Supported"` label.

use std::fmt;

/// Granular acceptance and verification labels required by the Ubuntu multi-instance contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VerificationLabel {
    /// Native Linux adapter logic (`/proc`, XDG, POSIX `0o700`, `statvfs`, signal handling) verified on Linux.
    LinuxAdapterTested,
    /// Multi-instance desktop test fixture (`fixture-app.mjs` / `dim-test-fixture`) verified at runtime.
    DesktopFixtureTested,
    /// User-supplied community port binary interactively verified across live Zalo workflows.
    CommunityPortTested,
    /// Official VNG Zalo Desktop for Linux does not exist (`SupportStatus::Unsupported`).
    OfficialZaloUnsupported,
    /// Feature or target OS combination has not been executed at runtime yet.
    NotTested,
    /// Blocked by an external OS, hardware, account, or runtime environment boundary.
    Blocked,
    /// Explicitly blocked because running would require weakening OS/Chromium security controls.
    SecurityBlocked,
}

impl VerificationLabel {
    /// Returns the canonical upper-snake-case machine token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LinuxAdapterTested => "LINUX_ADAPTER_TESTED",
            Self::DesktopFixtureTested => "DESKTOP_FIXTURE_TESTED",
            Self::CommunityPortTested => "COMMUNITY_PORT_TESTED",
            Self::OfficialZaloUnsupported => "OFFICIAL_ZALO_UNSUPPORTED",
            Self::NotTested => "NOT_TESTED",
            Self::Blocked => "BLOCKED",
            Self::SecurityBlocked => "SECURITY_BLOCKED",
        }
    }
}

impl fmt::Display for VerificationLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Missing shared library entry paired with Ubuntu 24.04 / 22.04 `apt` remediation hints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingLibraryDiagnostic {
    /// Shared object SONAME reported missing (e.g., `"libopus.so.0"`).
    pub soname: String,
    /// Recommended package on Ubuntu 24.04 LTS (`noble`).
    pub ubuntu_2404_package: &'static str,
    /// Recommended package on Ubuntu 22.04 LTS (`jammy`).
    pub ubuntu_2204_package: &'static str,
}

/// Specific security boundary reason when an operation or launch mode is `SECURITY_BLOCKED`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecurityBlockReason {
    /// Process is running as `root` (`UID == 0` or `EUID == 0`), which violates least-privilege desktop isolation.
    RunningAsRoot {
        /// Observed effective user ID.
        euid: u32,
    },
    /// Caller or wrapper attempted to disable Chromium/Electron sandboxing (`--no-sandbox`).
    NoSandboxForbidden {
        /// Offending CLI flag detected.
        flag: String,
    },
    /// Ubuntu AppArmor (`kernel.apparmor_restrict_unprivileged_userns = 1`) blocks unprivileged userns
    /// for an unconfined Electron AppImage without a targeted `/etc/apparmor.d/` profile.
    AppArmorUsernsRestricted {
        /// Binary path subject to unconfined AppArmor userns denial.
        binary_path: String,
    },
    /// Bubblewrap (`bwrap`) was requested, but `bwrap` is missing or blocked by kernel/AppArmor policy.
    BubblewrapBlocked {
        /// Diagnostic explanation of why `bwrap` cannot run safely.
        detail: String,
    },
    /// User-supplied executable has the POSIX `setuid` or `setgid` bit set.
    SetuidBinaryRejected {
        /// Observed octal permission bits.
        mode: u32,
    },
    /// Startup or wrapper script contains unsafe commands (`sudo`, `pkexec`, `--no-sandbox`, `killall`, etc.).
    UnsafeStartupScript {
        /// Detected hazardous patterns.
        findings: Vec<String>,
    },
    /// Profile or storage directory permits unauthorized group/world write access or symlink escape.
    InsecurePermissions {
        /// Path with unsafe ownership or permissions.
        path: String,
        /// Explanation of the permission violation.
        detail: String,
    },
}

impl SecurityBlockReason {
    /// Formats a machine-prefixed `SECURITY_BLOCKED:` message suitable for `DimError` and audit logs.
    #[must_use]
    pub fn to_audit_message(&self) -> String {
        match self {
            Self::RunningAsRoot { euid } => format!(
                "SECURITY_BLOCKED: refusing to run desktop profile manager or target app as root (euid={euid})"
            ),
            Self::NoSandboxForbidden { flag } => format!(
                "SECURITY_BLOCKED: flag '{flag}' weakens Chromium process sandboxing and is forbidden by policy; never disable AppArmor system-wide (in isolated test environments only, an admin-reviewed AppArmor userns profile may be evaluated, though it is not guaranteed to make every third-party AppImage work)"
            ),
            Self::AppArmorUsernsRestricted { binary_path } => format!(
                "SECURITY_BLOCKED: Ubuntu AppArmor restricts unprivileged user namespaces for unconfined AppImage '{binary_path}'; refusing automatic --no-sandbox fallback (host /etc/apparmor.d is never modified automatically; even in isolated test environments, an AppArmor userns profile is not guaranteed to make every third-party AppImage work if FUSE mounts, nested bwrap transitions, noexec mounts, or Electron Wayland issues intervene)"
            ),
            Self::BubblewrapBlocked { detail } => {
                format!("SECURITY_BLOCKED: bubblewrap sandbox unavailable or restricted ({detail})")
            }
            Self::SetuidBinaryRejected { mode } => format!(
                "SECURITY_BLOCKED: refusing to launch setuid/setgid binary (mode={mode:#o})"
            ),
            Self::UnsafeStartupScript { findings } => format!(
                "SECURITY_BLOCKED: startup script contains unsafe operations ({})",
                findings.join(", ")
            ),
            Self::InsecurePermissions { path, detail } => format!(
                "SECURITY_BLOCKED: insecure filesystem permissions on '{path}' ({detail})"
            ),
        }
    }
}

/// High-level root-cause domain for Ubuntu 24.04 AppImage and Electron launch preflight diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PreflightCategory {
    /// Non-negotiable security policy boundary (`root` execution, `--no-sandbox`, `setuid`, hazardous script).
    SecurityPolicyGuard,
    /// User namespace or Ubuntu 24.04 AppArmor unprivileged `userns` policy restriction
    /// (`kernel.apparmor_restrict_unprivileged_userns=1`, `unprivileged_userns_clone=0`, `bwrap-userns-restrict`).
    UserNamespaceAndAppArmor,
    /// Filesystem permission, ownership, or `/proc/mounts` mount-flag issue (`+x`, `noexec`, `nosuid`, `0o777`).
    FilesystemAndMount,
    /// Electron / AppImage runtime environment prerequisite (`/dev/fuse`, `DISPLAY`/`WAYLAND_DISPLAY`, `DT_NEEDED` libs, ELF header).
    RuntimeEnvironment,
}

impl PreflightCategory {
    /// Machine-readable kebab-case category identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SecurityPolicyGuard => "security-policy-guard",
            Self::UserNamespaceAndAppArmor => "userns-apparmor-restriction",
            Self::FilesystemAndMount => "filesystem-permission-or-mount",
            Self::RuntimeEnvironment => "electron-appimage-runtime-env",
        }
    }
}

impl fmt::Display for PreflightCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Severity of an individual preflight finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PreflightSeverity {
    /// Launch will fail or is refused by security policy until remediated.
    Blocking,
    /// Non-fatal caveat that may degrade isolation or cause secondary runtime failures.
    Warning,
}

impl PreflightSeverity {
    /// Machine-readable severity token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Blocking => "blocking",
            Self::Warning => "warning",
        }
    }
}

/// Structured preflight diagnostic classification distinguishing:
/// - **(a) User namespace / AppArmor restrictions** (`apparmor_restrict_unprivileged_userns=1`, `unprivileged_userns_clone=0`, `bwrap` policy),
/// - **(b) Filesystem permission / mount issues** (`+x`, `noexec` / `nosuid` from `/proc/mounts`, unreadable file, `0o777` or wrong-UID directory),
/// - **(c) Electron / AppImage runtime environment issues** (`/dev/fuse`, headless display, missing ELF64 `DT_NEEDED` libraries, malformed header),
/// - plus hard **security policy guards** (`root`, `--no-sandbox`, `setuid`, unsafe wrapper scripts).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreflightDiagnosticKind {
    /// Process effective UID is `0` (`root`), which violates least-privilege desktop execution.
    RunningAsRoot {
        /// Observed effective UID (`0`).
        euid: u32,
    },
    /// Caller supplied a forbidden Chromium sandbox-disabling flag (`--no-sandbox`, etc.).
    ForbiddenNoSandboxFlag {
        /// Offending CLI argument.
        flag: String,
    },
    /// Executable carries POSIX `setuid` or `setgid` mode bits.
    SetuidOrSetgidBinary {
        /// Observed octal permission bits.
        mode: u32,
    },
    /// Wrapper or `AppRun` script contains hazardous commands (`sudo`, `--no-sandbox`, `killall`, etc.).
    UnsafeStartupScript {
        /// Hazardous script directives found during static audit.
        findings: Vec<String>,
    },
    /// Kernel `kernel.unprivileged_userns_clone=0` disables unprivileged user namespaces kernel-wide.
    KernelUnprivilegedUsernsDisabled,
    /// Ubuntu 24.04 AppArmor (`kernel.apparmor_restrict_unprivileged_userns=1`) blocks unprivileged
    /// `CLONE_NEWUSER` for an unconfined Electron AppImage/binary without a matching `userns,` profile.
    AppArmorUnprivilegedUsernsRestricted {
        /// Binary path subject to unconfined AppArmor `userns` denial.
        binary_path: String,
    },
    /// Bubblewrap (`bwrap`) was requested, but `bwrap` is missing or lacks an AppArmor `userns` profile.
    BubblewrapUnavailableOrRestricted {
        /// Diagnostic detail explaining why `bwrap` cannot run.
        detail: String,
    },
    /// Caveat that even with an AppArmor `userns` profile, nested sandbox transitions
    /// (e.g. `/etc/apparmor.d/bwrap-userns-restrict` `bwrap-unprivileged` child transition or
    /// FUSE-mounted `/tmp/.mount_*` inner binary paths) may still block third-party AppImages.
    NestedUsernsTransitionCaveat {
        /// Explanation of the nested transition / FUSE inner-path limitation.
        detail: String,
    },
    /// Target binary does not exist or cannot be opened for static inspection.
    UnreadableOrMissingBinary {
        /// Inspected path.
        path: String,
        /// Underlying OS I/O error detail.
        detail: String,
    },
    /// Target binary lacks POSIX execute permission (`+x`).
    MissingExecutableBit {
        /// Binary path missing `+x`.
        path: String,
    },
    /// Target binary or AppImage mount directory resides on a `noexec` filesystem in `/proc/mounts`.
    MountOptionNoexec {
        /// Affected binary or temporary mount path.
        path: String,
        /// Covering mount point in `/proc/mounts` carrying `noexec`.
        mount_point: String,
    },
    /// Target binary resides on a `nosuid` filesystem in `/proc/mounts` (prevents setuid sandbox fallback).
    MountOptionNosuid {
        /// Affected binary path.
        path: String,
        /// Covering mount point in `/proc/mounts` carrying `nosuid`.
        mount_point: String,
    },
    /// Binary or profile directory fails POSIX ownership or permission hygiene (`0o777`, wrong UID, symlink).
    InsecureFilesystemPermissions {
        /// Path with unsafe permissions or ownership.
        path: String,
        /// Permission violation detail.
        detail: String,
    },
    /// AppImage requires `/dev/fuse` to mount its embedded SquashFS/DwarFS payload, but `/dev/fuse` is absent.
    MissingDevFuse {
        /// AppImage path inspected.
        binary_path: String,
    },
    /// Session is `Headless` (both `WAYLAND_DISPLAY` and `DISPLAY` are unset/empty), blocking GUI launch.
    HeadlessDisplayMissing {
        /// Binary path inspected.
        binary_path: String,
    },
    /// Static ELF64 `DT_NEEDED` scan detected shared libraries missing from Ubuntu multiarch / `DT_RUNPATH` paths.
    MissingSharedLibraries {
        /// Missing shared library entries with Ubuntu 24.04 (`t64`) and 22.04 `apt` package hints.
        libraries: Vec<MissingLibraryDiagnostic>,
    },
    /// File lacks a valid 64-byte ELF64 or `#!` executable header (e.g., corrupt or truncated `.AppImage`).
    InvalidExecutableHeader {
        /// Inspected binary path.
        binary_path: String,
        /// Header validation failure detail.
        detail: String,
    },
    /// ELF64 `e_machine` architecture does not match the active host CPU architecture.
    CpuArchitectureMismatch {
        /// Architecture parsed from ELF64 header.
        binary_arch: String,
        /// Active host CPU architecture.
        host_arch: String,
    },
}

impl PreflightDiagnosticKind {
    /// Returns the high-level root-cause domain ([`PreflightCategory`]) for this diagnostic.
    #[must_use]
    pub const fn category(&self) -> PreflightCategory {
        match self {
            Self::RunningAsRoot { .. }
            | Self::ForbiddenNoSandboxFlag { .. }
            | Self::SetuidOrSetgidBinary { .. }
            | Self::UnsafeStartupScript { .. } => PreflightCategory::SecurityPolicyGuard,

            Self::KernelUnprivilegedUsernsDisabled
            | Self::AppArmorUnprivilegedUsernsRestricted { .. }
            | Self::BubblewrapUnavailableOrRestricted { .. }
            | Self::NestedUsernsTransitionCaveat { .. } => {
                PreflightCategory::UserNamespaceAndAppArmor
            }

            Self::UnreadableOrMissingBinary { .. }
            | Self::MissingExecutableBit { .. }
            | Self::MountOptionNoexec { .. }
            | Self::MountOptionNosuid { .. }
            | Self::InsecureFilesystemPermissions { .. } => PreflightCategory::FilesystemAndMount,

            Self::MissingDevFuse { .. }
            | Self::HeadlessDisplayMissing { .. }
            | Self::MissingSharedLibraries { .. }
            | Self::InvalidExecutableHeader { .. }
            | Self::CpuArchitectureMismatch { .. } => PreflightCategory::RuntimeEnvironment,
        }
    }

    /// Returns a stable upper-snake-case diagnostic machine code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::RunningAsRoot { .. } => "RUNNING_AS_ROOT",
            Self::ForbiddenNoSandboxFlag { .. } => "FORBIDDEN_NO_SANDBOX_FLAG",
            Self::SetuidOrSetgidBinary { .. } => "SETUID_OR_SETGID_BINARY",
            Self::UnsafeStartupScript { .. } => "UNSAFE_STARTUP_SCRIPT",
            Self::KernelUnprivilegedUsernsDisabled => "KERNEL_UNPRIVILEGED_USERNS_DISABLED",
            Self::AppArmorUnprivilegedUsernsRestricted { .. } => {
                "APPARMOR_UNPRIVILEGED_USERNS_RESTRICTED"
            }
            Self::BubblewrapUnavailableOrRestricted { .. } => {
                "BUBBLEWRAP_UNAVAILABLE_OR_RESTRICTED"
            }
            Self::NestedUsernsTransitionCaveat { .. } => "NESTED_USERNS_TRANSITION_CAVEAT",
            Self::UnreadableOrMissingBinary { .. } => "UNREADABLE_OR_MISSING_BINARY",
            Self::MissingExecutableBit { .. } => "MISSING_EXECUTABLE_BIT",
            Self::MountOptionNoexec { .. } => "MOUNT_OPTION_NOEXEC",
            Self::MountOptionNosuid { .. } => "MOUNT_OPTION_NOSUID",
            Self::InsecureFilesystemPermissions { .. } => "INSECURE_FILESYSTEM_PERMISSIONS",
            Self::MissingDevFuse { .. } => "MISSING_DEV_FUSE",
            Self::HeadlessDisplayMissing { .. } => "HEADLESS_DISPLAY_MISSING",
            Self::MissingSharedLibraries { .. } => "MISSING_SHARED_LIBRARIES",
            Self::InvalidExecutableHeader { .. } => "INVALID_EXECUTABLE_HEADER",
            Self::CpuArchitectureMismatch { .. } => "CPU_ARCHITECTURE_MISMATCH",
        }
    }
}

impl fmt::Display for PreflightDiagnosticKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.category().as_str(), self.code())
    }
}

/// Target Ubuntu distribution tier in the compatibility matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UbuntuTargetTier {
    /// Primary target: Ubuntu 24.04 LTS (`noble`) `x86_64`.
    Ubuntu2404X86_64,
    /// Secondary LTS target: Ubuntu 22.04 LTS (`jammy`) `x86_64`.
    Ubuntu2204X86_64,
    /// Conditional architecture target: Ubuntu 24.04/22.04 LTS `aarch64` (`ARM64`).
    UbuntuLtsArm64,
    /// Non-Ubuntu or unrecognized Linux distribution.
    OtherLinux,
}

impl UbuntuTargetTier {
    /// Classifies an OS release (`id`, `version_id`) and `uname -m` architecture.
    #[must_use]
    pub fn classify(distro_id: &str, version_id: &str, arch: &str) -> Self {
        if !distro_id.trim().eq_ignore_ascii_case("ubuntu") {
            return Self::OtherLinux;
        }
        let ver = version_id.trim();
        let arch_norm = arch.trim().to_ascii_lowercase();
        if arch_norm == "aarch64" || arch_norm == "arm64" {
            if ver.starts_with("24.04") || ver.starts_with("22.04") {
                return Self::UbuntuLtsArm64;
            }
            return Self::OtherLinux;
        }
        if arch_norm == "x86_64" || arch_norm == "amd64" {
            if ver.starts_with("24.04") {
                return Self::Ubuntu2404X86_64;
            }
            if ver.starts_with("22.04") {
                return Self::Ubuntu2204X86_64;
            }
        }
        Self::OtherLinux
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn preflight_diagnostic_kinds_map_to_distinct_domains() {
        let userns = PreflightDiagnosticKind::AppArmorUnprivilegedUsernsRestricted {
            binary_path: "/opt/Zalo.AppImage".to_owned(),
        };
        assert_eq!(
            userns.category(),
            PreflightCategory::UserNamespaceAndAppArmor
        );
        assert_eq!(userns.code(), "APPARMOR_UNPRIVILEGED_USERNS_RESTRICTED");

        let noexec = PreflightDiagnosticKind::MountOptionNoexec {
            path: "/tmp/Zalo.AppImage".to_owned(),
            mount_point: "/tmp".to_owned(),
        };
        assert_eq!(noexec.category(), PreflightCategory::FilesystemAndMount);
        assert_eq!(noexec.code(), "MOUNT_OPTION_NOEXEC");

        let fuse = PreflightDiagnosticKind::MissingDevFuse {
            binary_path: "/opt/Zalo.AppImage".to_owned(),
        };
        assert_eq!(fuse.category(), PreflightCategory::RuntimeEnvironment);
        assert_eq!(fuse.code(), "MISSING_DEV_FUSE");

        let root = PreflightDiagnosticKind::RunningAsRoot { euid: 0 };
        assert_eq!(root.category(), PreflightCategory::SecurityPolicyGuard);
        assert_eq!(root.code(), "RUNNING_AS_ROOT");
    }

    #[test]
    fn security_block_audit_messages_preserve_prefixes_and_caveats() {
        let msg_flag = SecurityBlockReason::NoSandboxForbidden {
            flag: "--no-sandbox".to_owned(),
        }
        .to_audit_message();
        assert!(msg_flag.starts_with("SECURITY_BLOCKED: flag '--no-sandbox'"));
        assert!(msg_flag.contains("not guaranteed to make every third-party AppImage work"));

        let msg_aa = SecurityBlockReason::AppArmorUsernsRestricted {
            binary_path: "/home/test/Zalo.AppImage".to_owned(),
        }
        .to_audit_message();
        assert!(msg_aa.starts_with(
            "SECURITY_BLOCKED: Ubuntu AppArmor restricts unprivileged user namespaces"
        ));
        assert!(msg_aa.contains("isolated test environments"));
        assert!(msg_aa.contains("not guaranteed to make every third-party AppImage work"));
    }
}

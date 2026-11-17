//! Verification labels, security block classifications, and Ubuntu target metadata.
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
    /// Blocked by an external OS, hardware, account, or security boundary.
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
                "SECURITY_BLOCKED: flag '{flag}' weakens Chromium process sandboxing and is forbidden by policy; configure a targeted AppArmor userns profile instead"
            ),
            Self::AppArmorUsernsRestricted { binary_path } => format!(
                "SECURITY_BLOCKED: Ubuntu AppArmor restricts unprivileged user namespaces for unconfined AppImage '{binary_path}'; refusing automatic --no-sandbox fallback"
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

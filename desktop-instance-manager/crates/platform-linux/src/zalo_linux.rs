//! Community `zalo-linux` repack route: detection, audited-allowlist pinning, and provenance.
//!
//! The `zalo-linux` package is an UNOFFICIAL community repack of the Zalo desktop client
//! (launcher `/usr/bin/zalo`, application `/opt/zalo-linux/app`, NOT signed or endorsed by VNG).
//! NexaDock treats it as [`SupportStatus::Experimental`]:
//!
//! - The wrapper script is never executed during discovery; it is hazard-audited statically and
//!   its exact SHA-256 digest is recorded (audited allowlist) at discovery time together with
//!   payload digests (`app/package.json`, `electron/electron`) and the app-directory uid/mode.
//! - Every launch re-verifies the wrapper bytes AND the recorded payload digests/ownership, so
//!   any modification of the launcher or payload between discovery and launch fails closed
//!   (`UNVERIFIED_BINARY` / `PROCESS_ERROR`) instead of executing.
//! - The wrapper's own internal `--no-sandbox` flag (a zalo-linux repack design decision) is
//!   accepted ONLY for this audited route; NexaDock never passes `--no-sandbox` itself, and the
//!   general startup-script rejection and launch policy for user-supplied binaries stay unchanged.
//! - Residual trust boundary: the repack payload installs user-owned (commonly group-writable),
//!   so payload integrity between discovery and launch is enforced by digest + uid/mode
//!   consistency, NOT by VNG signatures; the wrapper stays the only byte-pinned entry point.

use std::fs;
use std::path::{Path, PathBuf};

use dim_core::{DimError, SupportStatus};

use crate::discovery::{inspect_user_binary, sha256_file};
use crate::status::SecurityBlockReason;

/// Absolute path of the `zalo-linux` repack launcher wrapper script.
pub const ZALO_LINUX_LAUNCHER_PATH: &str = "/usr/bin/zalo";

/// Absolute installation root of the `zalo-linux` repack.
pub const ZALO_LINUX_INSTALL_ROOT: &str = "/opt/zalo-linux";

/// Absolute path of the repack's Electron application directory.
pub const ZALO_LINUX_APP_DIR: &str = "/opt/zalo-linux/app";

/// Provenance tag surfaced in `ApplicationInstallation::signature_status`.
pub const ZALO_LINUX_PROVENANCE_TAG: &str = "CommunityRepack (zalo-linux pkg, NOT VNG signed)";

/// Upper bound for wrapper scripts considered for the audited allowlist.
const MAX_LAUNCHER_BYTES: u64 = 512 * 1024;

/// A detected and audited `zalo-linux` repack route under a (real or virtualized) filesystem root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZaloLinuxRoute {
    /// Audited wrapper script path (`<fs_root>/usr/bin/zalo`).
    pub launcher_path: PathBuf,
    /// Installation root (`<fs_root>/opt/zalo-linux`).
    pub install_root: PathBuf,
    /// Electron application directory (`<fs_root>/opt/zalo-linux/app`).
    pub app_dir: PathBuf,
    /// Bundled Electron runtime (`<fs_root>/opt/zalo-linux/electron/electron`) when present.
    pub electron_path: Option<PathBuf>,
    /// SHA-256 of the audited wrapper bytes (the allowlist key).
    pub launcher_sha256: String,
    /// SHA-256 of `<app_dir>/package.json` when present (payload pinning).
    pub package_json_sha256: Option<String>,
    /// SHA-256 of the bundled Electron runtime when present (payload pinning).
    pub electron_sha256: Option<String>,
    /// Owner uid of the application directory (`None` off-Unix).
    pub app_dir_uid: Option<u32>,
    /// Permission bits of the application directory (`None` off-Unix).
    pub app_dir_mode_bits: Option<u32>,
    /// Version declared by the application `package.json`, when parseable.
    pub version: Option<String>,
}

/// The allowlist snapshot recorded at discovery time and re-verified before every launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZaloLinuxAllowlist {
    /// SHA-256 of the audited wrapper bytes.
    pub launcher_sha256: String,
    /// SHA-256 of `<app_dir>/package.json` recorded at discovery.
    pub package_json_sha256: Option<String>,
    /// SHA-256 of the bundled Electron runtime recorded at discovery.
    pub electron_sha256: Option<String>,
    /// Owner uid of the application directory recorded at discovery.
    pub app_dir_uid: Option<u32>,
    /// Permission bits of the application directory recorded at discovery.
    pub app_dir_mode_bits: Option<u32>,
}

impl ZaloLinuxAllowlist {
    /// Captures the allowlist snapshot from a freshly audited route (discovery time).
    #[must_use]
    pub fn from_route(route: &ZaloLinuxRoute) -> Self {
        Self {
            launcher_sha256: route.launcher_sha256.clone(),
            package_json_sha256: route.package_json_sha256.clone(),
            electron_sha256: route.electron_sha256.clone(),
            app_dir_uid: route.app_dir_uid,
            app_dir_mode_bits: route.app_dir_mode_bits,
        }
    }
}

impl ZaloLinuxRoute {
    /// Returns `true` when this freshly probed route still byte-matches the allowlist recorded
    /// at discovery time (wrapper + payload digests + app-directory uid/mode consistency).
    #[must_use]
    pub fn integrity_matches(&self, recorded: &ZaloLinuxAllowlist) -> bool {
        self.launcher_sha256
            .eq_ignore_ascii_case(&recorded.launcher_sha256)
            && self.package_json_sha256 == recorded.package_json_sha256
            && self.electron_sha256 == recorded.electron_sha256
            && self.app_dir_uid == recorded.app_dir_uid
            && self.app_dir_mode_bits == recorded.app_dir_mode_bits
    }

    /// Provenance/signature summary line for `ApplicationInstallation::signature_status`.
    #[must_use]
    pub fn signature_status(&self) -> String {
        let sha_short = &self.launcher_sha256[..16.min(self.launcher_sha256.len())];
        format!(
            "{ZALO_LINUX_PROVENANCE_TAG} [format=shell-script-wrapper, sha256_prefix={sha_short}]"
        )
    }

    /// Honest support classification for this route.
    #[must_use]
    pub const fn support_status() -> SupportStatus {
        SupportStatus::Experimental
    }
}

/// Returns the (virtualizable) launcher path for `fs_root`.
#[must_use]
pub fn zalo_linux_launcher_path(fs_root: &Path) -> PathBuf {
    join_anchored(fs_root, ZALO_LINUX_LAUNCHER_PATH)
}

/// Returns the (virtualizable) installation root for `fs_root`.
#[must_use]
pub fn zalo_linux_install_root(fs_root: &Path) -> PathBuf {
    join_anchored(fs_root, ZALO_LINUX_INSTALL_ROOT)
}

/// Returns the (virtualizable) application directory for `fs_root`.
#[must_use]
pub fn zalo_linux_app_dir(fs_root: &Path) -> PathBuf {
    join_anchored(fs_root, ZALO_LINUX_APP_DIR)
}

/// Probes the community `zalo-linux` repack route under `fs_root` without executing anything.
///
/// * `Ok(None)` — route not installed (launcher file or application directory missing).
/// * `Ok(Some(route))` — route installed, hazard-audited, and (when `expected` is supplied)
///   byte-identical to the wrapper/payload snapshot recorded at discovery time.
/// * `Err(_)` — launcher present but rejected: SHA-256 mismatch against the recorded allowlist,
///   symlinked launcher, missing executable bit, setuid/world-writable mode bits, non-allowlisted
///   script hazards, world-writable application directory, or an oversized wrapper script.
pub fn probe_zalo_linux_route(
    fs_root: &Path,
    expected: Option<&ZaloLinuxAllowlist>,
) -> Result<Option<ZaloLinuxRoute>, DimError> {
    let launcher_path = zalo_linux_launcher_path(fs_root);
    let app_dir = zalo_linux_app_dir(fs_root);
    if !launcher_path.is_file() || !app_dir.is_dir() {
        return Ok(None);
    }

    // A symlinked launcher would swap the audited bytes for an untracked target; the repack
    // package installs a regular file, so any symlink fails closed.
    let launcher_meta = fs::symlink_metadata(&launcher_path)?;
    if launcher_meta.file_type().is_symlink() {
        return Err(DimError::ProcessError(format!(
            "SECURITY_BLOCKED: zalo-linux launcher '{}' is a symlink; the audited allowlist requires the regular package-installed wrapper file.",
            launcher_path.display()
        )));
    }

    // Bound the audited content before hashing: only small wrapper scripts are allowlistable.
    if launcher_meta.len() > MAX_LAUNCHER_BYTES {
        return Err(DimError::ProcessError(format!(
            "SECURITY_BLOCKED: zalo-linux launcher '{}' exceeds the {MAX_LAUNCHER_BYTES}-byte audited-allowlist limit.",
            launcher_path.display()
        )));
    }

    let expected_wrapper = expected.map(|snapshot| snapshot.launcher_sha256.as_str());
    let report = inspect_user_binary(&launcher_path, expected_wrapper)?;
    if let Some(block) = &report.security_block {
        if !is_allowlisted_wrapper_block(block) {
            return Err(DimError::ProcessError(block.to_audit_message()));
        }
    }
    if !report.is_executable {
        return Err(DimError::ProcessError(format!(
            "SECURITY_BLOCKED: zalo-linux launcher '{}' lacks the executable bit (+x).",
            launcher_path.display()
        )));
    }
    let Some(launcher_sha256) = report.sha256.clone() else {
        return Err(DimError::ProcessError(format!(
            "SECURITY_BLOCKED: could not compute SHA-256 for the zalo-linux launcher '{}'.",
            launcher_path.display()
        )));
    };

    // Payload pinning: hash the app manifest and the bundled Electron runtime, and capture the
    // application-directory ownership/mode so launch can reject post-discovery tampering or
    // permission widening. A world-writable payload directory fails closed outright.
    let package_json = app_dir.join("package.json");
    if !package_json.is_file() {
        return Err(DimError::ProcessError(format!(
            "SECURITY_BLOCKED: zalo-linux application manifest '{}' is missing.",
            package_json.display()
        )));
    }
    let package_json_sha256 = Some(sha256_file(&package_json)?);
    let install_root = zalo_linux_install_root(fs_root);
    let electron_candidate = install_root.join("electron").join("electron");
    if !electron_candidate.is_file() {
        return Err(DimError::ProcessError(format!(
            "SECURITY_BLOCKED: bundled zalo-linux Electron runtime '{}' is missing.",
            electron_candidate.display()
        )));
    }
    let electron_path = Some(electron_candidate.clone());
    let electron_sha256 = Some(sha256_file(&electron_candidate)?);

    let app_dir_meta = fs::metadata(&app_dir)?;
    #[cfg(unix)]
    let (app_dir_uid, app_dir_mode_bits) = {
        use std::os::unix::fs::MetadataExt;
        use std::os::unix::fs::PermissionsExt;
        let mode = app_dir_meta.permissions().mode();
        if mode & 0o002 != 0 {
            return Err(DimError::ProcessError(format!(
                "SECURITY_BLOCKED: zalo-linux application directory '{}' is world-writable (mode={:#o}).",
                app_dir.display(),
                mode & 0o7777
            )));
        }
        (Some(app_dir_meta.uid()), Some(mode & 0o7777))
    };
    #[cfg(not(unix))]
    let (app_dir_uid, app_dir_mode_bits) = {
        let _ = app_dir_meta;
        (None, None)
    };

    let version = read_app_version(&app_dir);
    let route = ZaloLinuxRoute {
        launcher_path,
        install_root,
        app_dir,
        electron_path,
        launcher_sha256,
        package_json_sha256,
        electron_sha256,
        app_dir_uid,
        app_dir_mode_bits,
        version,
    };

    // Launch-time re-verification: compare the fresh probe against the discovery snapshot.
    if let Some(snapshot) = expected {
        if !route.integrity_matches(snapshot) {
            return Err(DimError::ProcessError(format!(
                "SECURITY_BLOCKED: zalo-linux payload or launcher changed since discovery (audited-allowlist digest/ownership mismatch for '{}'); re-run discovery to re-audit.",
                route.app_dir.display()
            )));
        }
    }

    Ok(Some(route))
}

/// Accepts `UnsafeStartupScript` findings ONLY when every hazard is the wrapper's own internal
/// `--no-sandbox` / `--disable-setuid-sandbox` flag (documented zalo-linux repack behavior).
/// Privilege escalation, process kills, injections, and hardcoded home paths still fail closed.
fn is_allowlisted_wrapper_block(block: &SecurityBlockReason) -> bool {
    match block {
        SecurityBlockReason::UnsafeStartupScript { findings } => findings.iter().all(|finding| {
            finding.contains("--no-sandbox") || finding.contains("--disable-setuid-sandbox")
        }),
        _ => false,
    }
}

/// Reads the repack application version from `<app_dir>/package.json` defensively: target
/// content is data, never instructions, and malformed or oversized files yield `None`.
fn read_app_version(app_dir: &Path) -> Option<String> {
    const MAX_PACKAGE_JSON_BYTES: usize = 256 * 1024;
    const MAX_VERSION_CHARS: usize = 64;

    let raw = fs::read(app_dir.join("package.json")).ok()?;
    if raw.len() > MAX_PACKAGE_JSON_BYTES {
        return None;
    }
    let value: serde_json::Value = serde_json::from_slice(&raw).ok()?;
    let version = value.get("version")?.as_str()?.trim();
    if version.is_empty() || version.len() > MAX_VERSION_CHARS {
        return None;
    }
    Some(version.to_owned())
}

/// Joins an absolute package path onto `fs_root`, keeping virtualized test roots consistent.
fn join_anchored(fs_root: &Path, absolute: &str) -> PathBuf {
    fs_root.join(absolute.trim_start_matches('/'))
}

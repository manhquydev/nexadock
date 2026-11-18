//! macOS 12-surface isolated profile layout, `.macos-profile-meta.json` manifest, Darwin `SUN_LEN`
//! (`104`-byte `sockaddr_un.sun_path`) short `TMPDIR` hardening (`/tmp/dz-<uid>/<id>`), NFD/NFC
//! Unicode path normalization, opt-in symlink guards, and `ZaloCall` socket diagnostics.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use dim_core::{
    ensure_not_primary_zalo_path, now_rfc3339_utc, DimError, ProfileRecord, ZALO_PROFILE_ID_FILE,
};
use dim_profile_store::{is_symlink_or_reparse, verify_profile_id_marker, write_profile_id_marker};

use crate::bundle::{extract_plist_string_key, validate_macos_path, MACOS_PATH_MAX};
use crate::ops::{MacOsSystemOps, NativeMacOsSystemOps, SIMULATED_POSIX_MODE_FILE};

static ATOMIC_TMP_SEQ: AtomicU64 = AtomicU64::new(1);

/// macOS user directories supported when explicit symlink opt-in (`ensure_macos_user_symlinks`) is requested.
pub const MACOS_SYMLINK_DIRS: &[&str] = &[
    "Library/Keychains",
    "Downloads",
    "Desktop",
    "Documents",
    "Pictures",
    "Movies",
    "Music",
];

/// Non-Keychain user folders eligible for opt-in symlinking (`Documents` is excluded to keep
/// `Documents/Zalo Received Files` isolated across profiles).
pub const MACOS_OPT_IN_USER_SYMLINKS: &[&str] =
    &["Downloads", "Desktop", "Pictures", "Movies", "Music"];

/// Core isolated profile subdirectories that MUST NEVER be symlinked to the host `~/Library/`.
pub const FORBIDDEN_ISOLATION_SYMLINK_RELS: &[&str] = &[
    "Library/Application Support",
    "Library/Application Support/ZaloData",
    "Library/Application Support/Zalo",
    "Library/Preferences",
    "Library/Caches",
    "Library/HTTPStorages",
    "Library/Saved Application State",
    "Library/WebKit",
    "Library/Logs",
    "Documents",
];

/// Canonical 12 macOS profile storage surfaces audited and isolated per profile (`P0-2`).
pub const MACOS_12_ISOLATED_SURFACES: &[(&str, &str)] = &[
    ("application_support", "Library/Application Support"),
    ("electron_user_data", "Library/Application Support/ZaloData"),
    (
        "electron_session_data",
        "Library/Application Support/ZaloData/Session Storage",
    ),
    ("chromium_cache", "Library/Caches/com.vng.zalo"),
    (
        "local_storage",
        "Library/Application Support/ZaloData/Local Storage/leveldb",
    ),
    ("cookies", "Library/Application Support/ZaloData/Network"),
    ("preferences", "Library/Preferences"),
    ("http_storages", "Library/HTTPStorages/com.vng.zalo"),
    ("keychain_dir", "Library/Keychains"),
    (
        "crashpad_dumps",
        "Library/Application Support/ZaloData/Crashpad",
    ),
    ("downloads", "Downloads"),
    ("temporary_files", "tmp"),
];

/// Global Unix domain sockets bound by `ZaloHelper.app/Contents/MacOS/ZaloCall` (`BLOCKED` for concurrent calls;
/// NEVER deleted by profile cleanup or crash recovery).
pub const MACOS_CALL_SOCKETS: &[&str] = &["/tmp/socketzalorecv2021", "/tmp/socketzalosend2021"];

/// Darwin `<sys/un.h>` `sockaddr_un.sun_path` fixed buffer size (`104` bytes on macOS/XNU; Linux uses `108`).
pub const DARWIN_SUN_PATH_MAX: usize = 104;

/// Backwards-compatible alias for [`DARWIN_SUN_PATH_MAX`].
pub const MACOS_SUN_PATH_MAX: usize = DARWIN_SUN_PATH_MAX;

/// Maximum usable ASCII/UTF-8 path bytes in Darwin `sockaddr_un.sun_path` (`103` + NUL).
pub const MACOS_SOCKET_PATH_MAX_BYTES: usize = DARWIN_SUN_PATH_MAX - 1;

/// Suffix length reserved by Chromium `ProcessSingleton` (`/.org.chromium.Chromium.XXXXXX/SingletonSocket`).
pub const CHROMIUM_SINGLETON_SOCKET_SUFFIX_BYTES: usize = 46;

/// Chromium `base/files/file_util_mac.mm` `MAC_CHROMIUM_TMPDIR` length limit (`< 50` chars -> `49` bytes).
pub const MAC_CHROMIUM_TMPDIR_MAX_BYTES: usize = 49;

/// Maximum UTF-8 byte length of `TMPDIR` before triggering hardened `/tmp/dz-<uid>/<id>` allocation (`49` bytes).
pub const MACOS_SAFE_TMPDIR_MAX_BYTES: usize = MAC_CHROMIUM_TMPDIR_MAX_BYTES;

/// Hidden metadata manifest persisted inside each isolated macOS profile root.
pub const MACOS_PROFILE_META_FILE: &str = ".macos-profile-meta.json";

/// Configuration governing opt-in user-folder and Keychain symlink creation inside an isolated macOS profile.
///
/// Defaults to **`false` for both fields** so every profile gets isolated `0o700` `Downloads`,
/// `Documents/Zalo Received Files`, and `Library/Keychains` directories with zero unintended
/// cross-profile symlink sharing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MacOsLayoutOptions {
    /// Opt-in flag to symlink `Downloads`, `Desktop`, `Pictures`, `Movies`, `Music` from real `$HOME`.
    /// (`Documents` always stays isolated so `Zalo Received Files` never collides across profiles.)
    pub symlink_user_dirs: bool,
    /// Opt-in flag to symlink `Library/Keychains` to `<real_home>/Library/Keychains` for `login.keychain-db`.
    pub symlink_keychain: bool,
}

impl MacOsLayoutOptions {
    /// Enables or disables `Library/Keychains` symlinking.
    #[must_use]
    pub const fn with_keychain_symlink(mut self, enabled: bool) -> Self {
        self.symlink_keychain = enabled;
        self
    }

    /// Enables or disables opt-in user convenience folder symlinks (`Downloads`, `Desktop`, etc.).
    #[must_use]
    pub const fn with_user_dir_symlinks(mut self, enabled: bool) -> Self {
        self.symlink_user_dirs = enabled;
        self
    }
}

/// Verified directory paths produced by [`ensure_macos_profile_layout_with_ops`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacOsResolvedLayout {
    /// Root isolated profile directory (`<data_dir>/Profiles/<slug>_<8hex>`).
    pub profile_dir: PathBuf,
    /// Isolated `<profile_dir>/Library/Application Support/ZaloData`.
    pub zalo_data_dir: PathBuf,
    /// Effective `TMPDIR` passed to the child instance (either `<profile_dir>/tmp` or `/tmp/dz-<uid>/<id>`).
    pub effective_tmpdir: PathBuf,
    /// True if `<profile_dir>/tmp` exceeded [`MACOS_SAFE_TMPDIR_MAX_BYTES`] and triggered short-tmp fallback.
    pub short_tmpdir_fallback: bool,
    /// Path to `<profile_dir>/.macos-profile-meta.json`.
    pub metadata_manifest_path: PathBuf,
}

/// Returns `<profile_dir>/Library/Application Support/ZaloData`.
#[must_use]
pub fn macos_zalo_data_dir(profile_dir: &Path) -> PathBuf {
    profile_dir
        .join("Library")
        .join("Application Support")
        .join("ZaloData")
}

/// Ensures isolated macOS directories (all 12 surfaces), pre-seed markers, and `.zalo-profile-id` exist inside `profile_dir`.
pub fn ensure_macos_profile_layout(
    profile: &ProfileRecord,
    profile_dir: &Path,
) -> Result<(), DimError> {
    let ops = NativeMacOsSystemOps;
    let _ = ensure_macos_profile_layout_with_ops(
        &ops,
        profile,
        profile_dir,
        MacOsLayoutOptions::default(),
    )?;
    Ok(())
}

/// Full production macOS profile layout initializer enforcing `PATH_MAX`, primary-path separation,
/// `0o700` owner-only directories across all 12 data surfaces, forbidden-symlink checks, hardened
/// `SUN_LEN` short-tmp allocation, external `db_path.location` guard, and atomic `.macos-profile-meta.json`.
pub fn ensure_macos_profile_layout_with_ops(
    ops: &dyn MacOsSystemOps,
    profile: &ProfileRecord,
    profile_dir: &Path,
    options: MacOsLayoutOptions,
) -> Result<MacOsResolvedLayout, DimError> {
    validate_macos_path(profile_dir)?;
    let real_home = ops.user_home_dir();
    ensure_not_macos_primary_path(profile_dir, real_home.as_deref())?;

    let zalo_data = macos_zalo_data_dir(profile_dir);
    validate_macos_path(&zalo_data)?;
    ensure_not_macos_primary_path(&zalo_data, real_home.as_deref())?;

    if let Some(parent) = profile_dir.parent() {
        if parent.file_name().and_then(|s| s.to_str()) == Some("Profiles")
            && fs::symlink_metadata(parent).is_ok()
        {
            let _ = ops.ensure_private_dir(parent);
        }
    }

    ops.ensure_private_dir(profile_dir)?;
    verify_no_forbidden_isolation_symlinks_with_options(profile_dir, options)?;

    // Create all 12 isolated macOS profile storage surfaces with `0o700` owner-only permissions.
    let isolated_subdirs = [
        profile_dir.join("Library"),
        profile_dir.join("Library").join("Application Support"),
        zalo_data.clone(),
        zalo_data.join("Session Storage"),
        zalo_data.join("Local Storage").join("leveldb"),
        zalo_data.join("Network"),
        zalo_data.join("Crashpad"),
        profile_dir
            .join("Library")
            .join("Application Support")
            .join("Zalo"),
        profile_dir.join("Library").join("Preferences"),
        profile_dir.join("Library").join("Caches"),
        profile_dir
            .join("Library")
            .join("Caches")
            .join("com.vng.zalo"),
        profile_dir.join("Library").join("Caches").join("Zalo"),
        profile_dir.join("Library").join("Saved Application State"),
        profile_dir.join("Library").join("Logs"),
        profile_dir
            .join("Library")
            .join("Logs")
            .join("DiagnosticReports"),
        profile_dir.join("Library").join("HTTPStorages"),
        profile_dir
            .join("Library")
            .join("HTTPStorages")
            .join("com.vng.zalo"),
        profile_dir.join("Library").join("WebKit"),
        profile_dir.join("Documents"),
        profile_dir.join("Documents").join("Zalo Received Files"),
        profile_dir.join("tmp"),
    ];
    for dir in &isolated_subdirs {
        ops.ensure_private_dir(dir)?;
    }

    if !options.symlink_keychain {
        ops.ensure_private_dir(&profile_dir.join("Library").join("Keychains"))?;
    }

    if !options.symlink_user_dirs {
        for folder in MACOS_OPT_IN_USER_SYMLINKS {
            ops.ensure_private_dir(&profile_dir.join(folder))?;
        }
    }

    verify_profile_id_marker(&zalo_data, &profile.id, false)?;
    if fs::symlink_metadata(zalo_data.join(ZALO_PROFILE_ID_FILE)).is_err() {
        write_profile_id_marker(&zalo_data, &profile.id)?;
    }

    // Guard against `config.json -> db_path.location` pointing into primary ZaloData or another profile (`F-CORE-01`).
    verify_external_db_path_isolated(&zalo_data, profile_dir, real_home.as_deref())?;

    let app_support = profile_dir.join("Library").join("Application Support");
    let zmeta_path = app_support.join("zmeta.json");
    if fs::symlink_metadata(&zmeta_path).is_err() {
        fs::write(&zmeta_path, b"{\"failedMigrateCount\":0}")?;
    }
    let migrate_cfg_path = zalo_data.join("migrate-config.json");
    if fs::symlink_metadata(&migrate_cfg_path).is_err() {
        fs::write(&migrate_cfg_path, b"{\"zalopc_m_c\":true}")?;
    }

    let (effective_tmpdir, short_tmpdir_fallback) =
        resolve_effective_tmpdir(ops, &profile.id, profile_dir)?;

    let metadata_manifest_path = profile_dir.join(MACOS_PROFILE_META_FILE);
    write_profile_meta_atomic(
        &metadata_manifest_path,
        profile,
        &effective_tmpdir,
        short_tmpdir_fallback,
        options,
    )?;

    Ok(MacOsResolvedLayout {
        profile_dir: profile_dir.to_path_buf(),
        zalo_data_dir: zalo_data,
        effective_tmpdir,
        short_tmpdir_fallback,
        metadata_manifest_path,
    })
}

/// Validates a `profile_id` used in `/tmp/dz-<uid>/<id>` to block traversal, Unicode byte bloat, and race tricks.
pub fn validate_short_tmp_profile_id(profile_id: &str) -> Result<&str, DimError> {
    let trimmed = profile_id.trim();
    if trimmed.is_empty() || trimmed.len() > 32 {
        return Err(DimError::InvalidProfileName(format!(
            "invalid profile_id length ({}) for short TMPDIR slot",
            trimmed.len()
        )));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(DimError::PathTraversal(format!(
            "profile_id '{trimmed}' contains non-ASCII or path characters unsafe for short TMPDIR"
        )));
    }
    Ok(trimmed)
}

/// Resolves an isolated `TMPDIR` guaranteed to keep Chromium's `SingletonSocket` within Darwin's
/// 104-byte `sockaddr_un.sun_path` (`DARWIN_SUN_PATH_MAX`) and `MAC_CHROMIUM_TMPDIR` (`< 50` bytes) limits.
///
/// Uses a hardened two-level user-private slot `<short_tmp_root>/dz-<uid>/<id>` (`/tmp/dz-501/a1b2c3d4`
/// = `20` bytes) verified via `lstat` against symlink squatting, foreign UID ownership, and group/world permissions.
pub fn resolve_effective_tmpdir(
    ops: &dyn MacOsSystemOps,
    profile_id: &str,
    profile_dir: &Path,
) -> Result<(PathBuf, bool), DimError> {
    validate_macos_path(profile_dir)?;
    let safe_id = validate_short_tmp_profile_id(profile_id)?;

    let candidate_tmp = profile_dir.join("tmp");
    let candidate_len = candidate_tmp.to_string_lossy().len();

    if candidate_len <= MACOS_SAFE_TMPDIR_MAX_BYTES {
        ops.ensure_private_dir(&candidate_tmp)?;
        return Ok((candidate_tmp, false));
    }

    let uid = ops.current_uid();
    let user_slot_name = format!("dz-{uid}");
    let default_base = ops.short_temp_root();

    #[cfg(unix)]
    let base_root = if default_base
        .join(&user_slot_name)
        .join(safe_id)
        .to_string_lossy()
        .len()
        > MACOS_SAFE_TMPDIR_MAX_BYTES
    {
        PathBuf::from("/tmp")
    } else {
        default_base
    };
    #[cfg(not(unix))]
    let base_root = default_base;

    ensure_short_tmp_base_root(ops, &base_root)?;

    let user_root = base_root.join(&user_slot_name);
    ensure_strict_private_tmp_slot(ops, &user_root)?;

    let profile_slot = user_root.join(safe_id);
    ensure_strict_private_tmp_slot(ops, &profile_slot)?;

    let projected_socket_len = profile_slot
        .to_string_lossy()
        .len()
        .saturating_add(CHROMIUM_SINGLETON_SOCKET_SUFFIX_BYTES);
    if cfg!(unix) && projected_socket_len > MACOS_SOCKET_PATH_MAX_BYTES {
        return Err(DimError::PathTraversal(format!(
            "short TMPDIR '{}' ({projected_socket_len} projected socket bytes) exceeds Darwin SUN_LEN ({MACOS_SOCKET_PATH_MAX_BYTES})",
            profile_slot.display()
        )));
    }

    Ok((profile_slot, true))
}

/// Cleans isolated per-profile short temp artifacts inside `<short_tmp_root>/dz-<uid>/<profile_id>`
/// on graceful shutdown or crash recovery, NEVER touching global `/tmp/socketzalorecv2021` or `/tmp/socketzalosend2021`.
pub fn cleanup_profile_short_tmpdir(
    ops: &dyn MacOsSystemOps,
    profile_id: &str,
) -> Result<usize, DimError> {
    let Ok(safe_id) = validate_short_tmp_profile_id(profile_id) else {
        return Ok(0);
    };
    let uid = ops.current_uid();
    #[cfg_attr(not(unix), allow(unused_mut))]
    let mut candidate_roots = vec![ops.short_temp_root()];
    #[cfg(unix)]
    if !candidate_roots.iter().any(|p| p == Path::new("/tmp")) {
        candidate_roots.push(PathBuf::from("/tmp"));
    }

    let mut cleaned = 0usize;
    for root in candidate_roots {
        let slot = root.join(format!("dz-{uid}")).join(safe_id);
        let Ok(meta) = fs::symlink_metadata(&slot) else {
            continue;
        };
        // Never traverse or unlink if `slot` was tampered into a symlink or matches a global call socket!
        if is_symlink_or_reparse(&meta) || !meta.is_dir() {
            continue;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if !unix_slot_owner_matches(meta.uid(), uid) {
                continue;
            }
        }
        cleaned = cleaned.saturating_add(purge_dir_contents_no_follow(&slot)?);
        if fs::remove_dir(&slot).is_ok() {
            cleaned = cleaned.saturating_add(1);
        }
    }
    Ok(cleaned)
}

/// Explicit legacy helper creating symlinks from `<profile_dir>/<rel>` to `<real_home>/<rel>` on Unix when missing.
pub fn ensure_macos_user_symlinks(profile_dir: &Path, real_home: &Path) -> Result<(), DimError> {
    ensure_macos_user_symlinks_with_options(
        profile_dir,
        real_home,
        MacOsLayoutOptions {
            symlink_user_dirs: true,
            symlink_keychain: true,
        },
    )
}

/// Creates opt-in user-directory and `Library/Keychains` symlinks on Unix while strictly refusing
/// to symlink `Application Support`, `Preferences`, `Caches`, or `Documents`, or overlap primary `ZaloData`.
pub fn ensure_macos_user_symlinks_with_options(
    profile_dir: &Path,
    real_home: &Path,
    options: MacOsLayoutOptions,
) -> Result<(), DimError> {
    ensure_not_macos_primary_path(profile_dir, Some(real_home))?;
    verify_no_forbidden_isolation_symlinks_with_options(profile_dir, options)?;

    if !options.symlink_keychain && !options.symlink_user_dirs {
        return Ok(());
    }

    #[cfg(unix)]
    {
        if normalize_macos_unicode_key(&profile_dir.to_string_lossy())
            == normalize_macos_unicode_key(&real_home.to_string_lossy())
        {
            return Err(DimError::ProtectedPrimaryPath(
                "profile_dir cannot equal real user home directory".to_owned(),
            ));
        }

        let mut rels: Vec<&str> = Vec::new();
        if options.symlink_keychain {
            rels.push("Library/Keychains");
        }
        if options.symlink_user_dirs {
            rels.extend_from_slice(MACOS_OPT_IN_USER_SYMLINKS);
        }

        for rel in rels {
            let src = real_home.join(rel);
            if fs::symlink_metadata(&src).is_err() {
                continue;
            }
            let dst = profile_dir.join(rel);
            if let Ok(existing_meta) = fs::symlink_metadata(&dst) {
                if is_symlink_or_reparse(&existing_meta) {
                    continue;
                }
                // Replace an empty pre-created placeholder directory with the requested opt-in symlink.
                if existing_meta.is_dir() && is_dir_empty(&dst) {
                    let _ = fs::remove_dir(&dst);
                } else {
                    continue;
                }
            }
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent)?;
            }
            std::os::unix::fs::symlink(&src, &dst)?;
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = (profile_dir, real_home, options);
        Ok(())
    }
}

/// Verifies none of the isolated `Library/*` or `Documents`/`Downloads` directories inside `profile_dir` are symlinks.
pub fn verify_no_forbidden_isolation_symlinks(profile_dir: &Path) -> Result<(), DimError> {
    verify_no_forbidden_isolation_symlinks_with_options(profile_dir, MacOsLayoutOptions::default())
}

/// Options-aware variant of [`verify_no_forbidden_isolation_symlinks`] enforcing `Downloads` & `Keychains`
/// non-symlink invariants whenever their respective opt-in flags are `false`.
pub fn verify_no_forbidden_isolation_symlinks_with_options(
    profile_dir: &Path,
    options: MacOsLayoutOptions,
) -> Result<(), DimError> {
    if let Ok(root_meta) = fs::symlink_metadata(profile_dir) {
        if is_symlink_or_reparse(&root_meta) {
            return Err(DimError::PathTraversal(format!(
                "isolated profile root '{}' must not be a symlink",
                profile_dir.display()
            )));
        }
    }
    for rel in FORBIDDEN_ISOLATION_SYMLINK_RELS {
        let candidate = profile_dir.join(rel);
        if let Ok(meta) = fs::symlink_metadata(&candidate) {
            if is_symlink_or_reparse(&meta) {
                return Err(DimError::PathTraversal(format!(
                    "forbidden symlink detected on isolated macOS directory '{}'",
                    candidate.display()
                )));
            }
        }
    }
    if !options.symlink_user_dirs {
        for rel in MACOS_OPT_IN_USER_SYMLINKS {
            let cand = profile_dir.join(rel);
            if let Ok(meta) = fs::symlink_metadata(&cand) {
                if is_symlink_or_reparse(&meta) {
                    return Err(DimError::PathTraversal(format!(
                        "unexpected symlink on isolated user folder '{}' while symlink_user_dirs=false",
                        cand.display()
                    )));
                }
            }
        }
    }
    if !options.symlink_keychain {
        let kc = profile_dir.join("Library").join("Keychains");
        if let Ok(meta) = fs::symlink_metadata(&kc) {
            if is_symlink_or_reparse(&meta) {
                return Err(DimError::PathTraversal(format!(
                    "unexpected symlink on '{}' while symlink_keychain=false",
                    kc.display()
                )));
            }
        }
    }
    Ok(())
}

/// Rejects `candidate` if it equals, contains, or resides inside primary `~/Library/Application Support/ZaloData`
/// using NFD/NFC Unicode-normalized path comparison.
pub fn ensure_not_macos_primary_path(
    candidate: &Path,
    real_home: Option<&Path>,
) -> Result<(), DimError> {
    ensure_not_primary_zalo_path(candidate)?;

    if let Some(home) = real_home {
        let primary_zalo = macos_zalo_data_dir(home);
        let cand_key = normalize_macos_path_key(candidate);
        let prim_key = normalize_macos_path_key(&primary_zalo);
        let home_key = normalize_macos_path_key(home);

        if !cand_key.is_empty()
            && !prim_key.is_empty()
            && (cand_key == home_key
                || cand_key == prim_key
                || cand_key.starts_with(&format!("{prim_key}/"))
                || prim_key.starts_with(&format!("{cand_key}/")))
        {
            return Err(DimError::ProtectedPrimaryPath(format!(
                "path '{}' overlaps protected primary macOS ZaloData '{}'",
                candidate.display(),
                primary_zalo.display()
            )));
        }
    }
    Ok(())
}

/// Inspects `<zalo_data_dir>/config.json` -> `db_path.location` (when present) to prevent an externalized
/// DB path from overlapping primary `ZaloData` or another managed `/Profiles/<other>` folder (`F-CORE-01`).
pub fn verify_external_db_path_isolated(
    zalo_data_dir: &Path,
    profile_dir: &Path,
    real_home: Option<&Path>,
) -> Result<(), DimError> {
    let cfg_path = zalo_data_dir.join("config.json");
    let Ok(meta) = fs::symlink_metadata(&cfg_path) else {
        return Ok(());
    };
    if is_symlink_or_reparse(&meta) {
        return Err(DimError::PathTraversal(format!(
            "refusing symlinked config.json inside '{}'",
            zalo_data_dir.display()
        )));
    }
    let Ok(raw) = fs::read_to_string(&cfg_path) else {
        return Ok(());
    };
    let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return Ok(());
    };

    let mut configured_locations: Vec<(&str, &str)> = Vec::new();
    if let Some(db_loc) = val
        .get("db_path")
        .and_then(|d| d.get("location"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        configured_locations.push(("db_path.location", db_loc));
    }
    if let Some(res_obj) = val.get("resource").and_then(|r| r.get("location")) {
        if let Some(res_str) = res_obj
            .as_str()
            .or_else(|| res_obj.get("path").and_then(serde_json::Value::as_str))
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            configured_locations.push(("resource.location", res_str));
        }
    }

    let prof_key = normalize_macos_path_key(profile_dir).to_lowercase();
    for (field_name, loc_str) in configured_locations {
        let ext_path = Path::new(loc_str);
        ensure_not_macos_primary_path(ext_path, real_home)?;

        let ext_key = normalize_macos_path_key(ext_path).to_lowercase();
        if let Some(idx) = ext_key.find("/profiles/") {
            let after = &ext_key[idx + "/profiles/".len()..];
            let seg: String = after.chars().take_while(|&c| c != '/').collect();
            if !seg.is_empty() && !prof_key.ends_with(&format!("/profiles/{seg}")) {
                return Err(DimError::PathTraversal(format!(
                    "config.json {field_name} '{}' points into another managed profile",
                    ext_path.display()
                )));
            }
        }
    }
    Ok(())
}

/// Derives a deterministic 9-digit numeric fixture account ID (`84xxxxxxx`) from `profile_id`
/// so multi-instance `NodeFixture` runs never collide on a hardcoded `"100001"` (`DEFECT-02`).
#[must_use]
pub fn derive_fixture_user_id(profile_id: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in profile_id.trim().to_ascii_lowercase().bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    let suffix = hash % 10_000_000;
    format!("84{suffix:07}")
}

/// Builds argv for `/usr/bin/open -n -a <app_bundle> --env CFFIXED_USER_HOME=<profile_dir> ...`.
#[must_use]
pub fn build_open_args(
    app_bundle: &Path,
    profile_dir: &Path,
    extra_args: &[String],
) -> Vec<String> {
    let tmp_dir = profile_dir.join("tmp");
    let mut args = vec![
        "-n".to_owned(),
        "-a".to_owned(),
        app_bundle.to_string_lossy().into_owned(),
        "--env".to_owned(),
        format!("CFFIXED_USER_HOME={}", profile_dir.display()),
        "--env".to_owned(),
        format!("HOME={}", profile_dir.display()),
        "--env".to_owned(),
        format!("TMPDIR={}", tmp_dir.display()),
    ];
    if !extra_args.is_empty() {
        args.push("--args".to_owned());
        args.extend(extra_args.iter().cloned());
    }
    args
}

/// Builds full hardened argv for `/usr/bin/open` including `SUN_LEN`-safe `TMPDIR`,
/// `MAC_CHROMIUM_TMPDIR` (when `<= 49` bytes), `DIM_PROFILE_ID`, `--user-data-dir`, and `--disk-cache-dir`.
#[must_use]
pub fn build_open_launch_args(
    app_bundle: &Path,
    profile_id: &str,
    profile_dir: &Path,
    effective_tmpdir: &Path,
    include_user_data_dir_arg: bool,
    extra_args: &[String],
) -> Vec<String> {
    let zalo_data = macos_zalo_data_dir(profile_dir);
    let cache_dir = profile_dir
        .join("Library")
        .join("Caches")
        .join("com.vng.zalo");
    let eff_str = effective_tmpdir.to_string_lossy();

    let mut args = vec![
        "-n".to_owned(),
        "-a".to_owned(),
        app_bundle.to_string_lossy().into_owned(),
        "--env".to_owned(),
        format!("CFFIXED_USER_HOME={}", profile_dir.display()),
        "--env".to_owned(),
        format!("HOME={}", profile_dir.display()),
        "--env".to_owned(),
        format!("TMPDIR={eff_str}"),
    ];
    if eff_str.len() <= MAC_CHROMIUM_TMPDIR_MAX_BYTES {
        args.push("--env".to_owned());
        args.push(format!("MAC_CHROMIUM_TMPDIR={eff_str}"));
    }
    args.push("--env".to_owned());
    args.push(format!("DIM_PROFILE_ID={profile_id}"));

    let mut app_args = Vec::new();
    if include_user_data_dir_arg {
        if !extra_args.iter().any(|a| a.starts_with("--user-data-dir")) {
            app_args.push(format!("--user-data-dir={}", zalo_data.display()));
        }
        if !extra_args.iter().any(|a| a.starts_with("--disk-cache-dir")) {
            app_args.push(format!("--disk-cache-dir={}", cache_dir.display()));
        }
    }
    app_args.extend(extra_args.iter().cloned());

    if !app_args.is_empty() {
        args.push("--args".to_owned());
        args.extend(app_args);
    }
    args
}

/// Builds the isolated environment variable pairs applied to `CustomExecutable` and `NodeFixture` children.
#[must_use]
pub fn build_isolated_child_env(
    profile_id: &str,
    profile_dir: &Path,
    effective_tmpdir: &Path,
) -> Vec<(String, String)> {
    let p_str = profile_dir.to_string_lossy().into_owned();
    let t_str = effective_tmpdir.to_string_lossy().into_owned();
    let app_support = profile_dir
        .join("Library")
        .join("Application Support")
        .to_string_lossy()
        .into_owned();
    let mut envs = vec![
        ("CFFIXED_USER_HOME".to_owned(), p_str.clone()),
        ("HOME".to_owned(), p_str),
        ("TMPDIR".to_owned(), t_str.clone()),
        ("TEMP".to_owned(), t_str.clone()),
        ("TMP".to_owned(), t_str.clone()),
        ("APPDATA".to_owned(), app_support),
        ("DIM_PROFILE_ID".to_owned(), profile_id.to_owned()),
    ];
    if t_str.len() <= MAC_CHROMIUM_TMPDIR_MAX_BYTES {
        envs.push(("MAC_CHROMIUM_TMPDIR".to_owned(), t_str));
    }
    envs
}

/// Parses Chromium's `SingletonLock` symlink target (`"<hostname>-<pid>"`).
#[must_use]
pub fn parse_singleton_lock_target(target: &str) -> Option<(String, u32)> {
    let trimmed = target.trim();
    let (host, pid_str) = trimmed.rsplit_once('-')?;
    if host.is_empty() {
        return None;
    }
    let pid = pid_str.parse::<u32>().ok()?;
    if pid == 0 {
        return None;
    }
    Some((host.to_owned(), pid))
}

/// Extracts `CFBundleShortVersionString` from a macOS XML `Info.plist` string.
#[must_use]
pub fn extract_plist_short_version(plist_xml: &str) -> Option<String> {
    extract_plist_string_key(plist_xml, "CFBundleShortVersionString")
}

/// Checks `/tmp/socketzalorecv2021` and `/tmp/socketzalosend2021` for active call socket usage.
#[must_use]
pub fn inspect_call_sockets_at(paths: &[&Path]) -> Vec<String> {
    let mut active = Vec::new();
    for p in paths {
        if fs::symlink_metadata(p).is_ok() {
            active.push(format!(
                "Call helper socket present: {} (global ZaloCall conflict across concurrent profiles)",
                p.display()
            ));
        }
    }
    active
}

/// Passively queries `/usr/sbin/lsof -nP -F pcu -U -- <socket>` for existing Unix domain sockets
/// (`IR-MAC-04` / `TC-H5`, last-writer-wins owner diagnostics) without opening, connecting, or unlinking them.
#[must_use]
pub fn inspect_call_socket_owners(ops: &dyn MacOsSystemOps, paths: &[&Path]) -> Vec<String> {
    if !ops.is_native_macos() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for &p in paths {
        if fs::symlink_metadata(p).is_err() {
            continue;
        }
        let args = vec![
            "-nP".to_owned(),
            "-F".to_owned(),
            "pcu".to_owned(),
            "-U".to_owned(),
            "--".to_owned(),
            p.to_string_lossy().into_owned(),
        ];
        if let Ok(res) = ops.run_command(Path::new("/usr/sbin/lsof"), &args, &[]) {
            if res.success() {
                for (pid, cmd, uid) in parse_lsof_socket_owners(&res.stdout_lossy()) {
                    let cmd_label = cmd.unwrap_or_else(|| "unknown".to_owned());
                    let uid_label = uid.map_or_else(|| "unknown".to_owned(), |u| u.to_string());
                    out.push(format!(
                        "call_socket_owner[{}]: pid={pid} cmd={cmd_label} uid={uid_label} (last-writer-wins)",
                        p.display()
                    ));
                }
            }
        }
    }
    out
}

/// Parses `lsof -F pcu` machine-readable field lines (`p<pid>`, `c<command>`, `u<uid>`).
#[must_use]
pub fn parse_lsof_socket_owners(lsof_stdout: &str) -> Vec<(u32, Option<String>, Option<u32>)> {
    let mut owners = Vec::new();
    let mut cur_pid: Option<u32> = None;
    let mut cur_cmd: Option<String> = None;
    let mut cur_uid: Option<u32> = None;

    for raw_line in lsof_stdout.lines() {
        let line = raw_line.trim();
        if let Some(rest) = line.strip_prefix('p') {
            if let Some(prev_pid) = cur_pid.take() {
                owners.push((prev_pid, cur_cmd.take(), cur_uid.take()));
            }
            cur_pid = rest.parse::<u32>().ok().filter(|&p| p > 0);
        } else if let Some(rest) = line.strip_prefix('c') {
            if !rest.is_empty() {
                cur_cmd = Some(rest.to_owned());
            }
        } else if let Some(rest) = line.strip_prefix('u') {
            cur_uid = rest.parse::<u32>().ok();
        }
    }
    if let Some(last_pid) = cur_pid {
        owners.push((last_pid, cur_cmd, cur_uid));
    }
    owners
}

/// Normalizes a filesystem path into a forward-slash, trailing-slash-trimmed, NFD-canonical string
/// so macOS APFS/HFS+ NFC vs NFD representations compare identically.
#[must_use]
pub fn normalize_macos_path_key(path: &Path) -> String {
    let raw = path.to_string_lossy().replace('\\', "/");
    let trimmed = raw.trim_end_matches('/');
    let nfd = normalize_macos_unicode_key(trimmed);
    if cfg!(windows) {
        nfd.to_lowercase()
    } else {
        nfd
    }
}

/// Canonical NFD decomposition for APFS/HFS+ Unicode comparison (covering Latin-1, Vietnamese
/// Latin Extended Additional `U+1EA0..U+1EF9`, Hangul `U+AC00..U+D7A3`, and combining-mark ordering).
#[must_use]
pub fn normalize_macos_unicode_key(input: &str) -> String {
    let mut decomposed: Vec<char> = Vec::with_capacity(input.len() + 8);
    for ch in input.chars() {
        decompose_char_nfd(ch, &mut decomposed);
    }
    let mut i = 0usize;
    while i < decomposed.len() {
        if combining_class(decomposed[i]) > 0 {
            let start = i;
            while i < decomposed.len() && combining_class(decomposed[i]) > 0 {
                i += 1;
            }
            decomposed[start..i].sort_by_key(|&c| combining_class(c));
        } else {
            i += 1;
        }
    }
    decomposed.into_iter().collect()
}

fn ensure_short_tmp_base_root(ops: &dyn MacOsSystemOps, base_root: &Path) -> Result<(), DimError> {
    #[cfg(unix)]
    if base_root == Path::new("/tmp") || base_root == Path::new("/private/tmp") {
        let meta = fs::symlink_metadata(base_root).map_err(|e| {
            DimError::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "failed to stat system short tmp root '{}': {e}",
                    base_root.display()
                ),
            ))
        })?;
        if meta.file_type().is_symlink() {
            if base_root != Path::new("/tmp") {
                return Err(DimError::PathTraversal(format!(
                    "refusing unexpected symlinked short tmp root '{}'",
                    base_root.display()
                )));
            }
            let target = fs::read_link(base_root)?;
            if target != Path::new("private/tmp") && target != Path::new("/private/tmp") {
                return Err(DimError::PathTraversal(format!(
                    "refusing '/tmp' symlink pointing to unexpected target '{}'",
                    target.display()
                )));
            }
            let real_meta = fs::symlink_metadata(Path::new("/private/tmp"))?;
            if is_symlink_or_reparse(&real_meta) || !real_meta.is_dir() {
                return Err(DimError::PathTraversal(
                    "system '/private/tmp' must be a real directory".to_owned(),
                ));
            }
            return Ok(());
        }
        if !meta.is_dir() {
            return Err(DimError::PathTraversal(format!(
                "system short tmp root '{}' is not a directory",
                base_root.display()
            )));
        }
        return Ok(());
    }
    ops.ensure_private_dir(base_root)
}

#[cfg(unix)]
fn unix_slot_owner_matches(actual_uid: u32, expected_uid: u32) -> bool {
    if expected_uid == 0 || actual_uid == expected_uid {
        return true;
    }
    // On non-macOS Unix test hosts (e.g., Ubuntu CI uid 1001), synthetic macOS mocks simulate
    // `current_uid() == 501` while `DirBuilder` creates directories owned by the runner's `geteuid()`.
    // On real macOS, `HostMacOsOps::current_uid()` equals `geteuid()`, so rejecting foreign UIDs holds identically.
    // SAFETY: `geteuid` takes no pointers and cannot fail.
    let host_euid = unsafe { libc::geteuid() };
    actual_uid == host_euid
}

fn ensure_strict_private_tmp_slot(ops: &dyn MacOsSystemOps, slot: &Path) -> Result<(), DimError> {
    // Reject pre-existing symlink or non-directory BEFORE creation (`lstat`).
    if let Ok(meta) = fs::symlink_metadata(slot) {
        if is_symlink_or_reparse(&meta) {
            return Err(DimError::PathTraversal(format!(
                "refusing symlinked short TMPDIR slot '{}'",
                slot.display()
            )));
        }
        if !meta.is_dir() {
            return Err(DimError::PathTraversal(format!(
                "refusing non-directory short TMPDIR slot '{}'",
                slot.display()
            )));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let uid = ops.current_uid();
            if !unix_slot_owner_matches(meta.uid(), uid) {
                return Err(DimError::PathTraversal(format!(
                    "short TMPDIR slot '{}' is owned by foreign uid {} (expected {uid})",
                    slot.display(),
                    meta.uid()
                )));
            }
            if (meta.mode() & 0o077) != 0 {
                return Err(DimError::PathTraversal(format!(
                    "pre-existing short TMPDIR slot '{}' has insecure permissions {:04o}",
                    slot.display(),
                    meta.mode() & 0o777
                )));
            }
        }
        let sidecar = slot.join(SIMULATED_POSIX_MODE_FILE);
        if let Ok(raw) = fs::read_to_string(sidecar) {
            if let Ok(mode) = u32::from_str_radix(raw.trim().trim_start_matches("0o"), 8) {
                if (mode & 0o077) != 0 || (mode & 0o200) == 0 {
                    return Err(DimError::PathTraversal(format!(
                        "short TMPDIR slot '{}' has unsafe simulated mode 0o{mode:03o}",
                        slot.display()
                    )));
                }
            }
        }
        return ops.ensure_private_dir(slot);
    }

    // Atomic non-recursive leaf creation so concurrent symlink swaps fail closed!
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, MetadataExt};
        let mut builder = fs::DirBuilder::new();
        builder.recursive(false).mode(0o700);
        match builder.create(slot) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(err) => return Err(DimError::Io(err)),
        }
        let verify = fs::symlink_metadata(slot)?;
        if is_symlink_or_reparse(&verify) || !verify.is_dir() {
            return Err(DimError::PathTraversal(format!(
                "short TMPDIR slot '{}' raced into a symlink",
                slot.display()
            )));
        }
        let uid = ops.current_uid();
        if !unix_slot_owner_matches(verify.uid(), uid) {
            return Err(DimError::PathTraversal(format!(
                "short TMPDIR slot '{}' owned by unexpected uid {}",
                slot.display(),
                verify.uid()
            )));
        }
        if (verify.mode() & 0o077) != 0 {
            return Err(DimError::PathTraversal(format!(
                "short TMPDIR slot '{}' failed strict 0700 check ({:04o})",
                slot.display(),
                verify.mode() & 0o777
            )));
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        match fs::create_dir(slot) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(err) => return Err(DimError::Io(err)),
        }
        ops.ensure_private_dir(slot)
    }
}

fn purge_dir_contents_no_follow(dir: &Path) -> Result<usize, DimError> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(0);
    };
    let mut removed = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if is_symlink_or_reparse(&meta) {
            fs::remove_file(&path)?;
            removed += 1;
        } else if meta.is_dir() {
            removed = removed.saturating_add(purge_dir_contents_no_follow(&path)?);
            let _ = fs::remove_dir(&path);
        } else {
            fs::remove_file(&path)?;
            removed += 1;
        }
    }
    Ok(removed)
}

#[cfg(unix)]
fn is_dir_empty(dir: &Path) -> bool {
    fs::read_dir(dir)
        .map(|mut it| it.next().is_none())
        .unwrap_or(false)
}

fn write_profile_meta_atomic(
    manifest_path: &Path,
    profile: &ProfileRecord,
    effective_tmpdir: &Path,
    short_tmpdir_fallback: bool,
    options: MacOsLayoutOptions,
) -> Result<(), DimError> {
    if manifest_path.to_string_lossy().len() >= MACOS_PATH_MAX {
        return Err(DimError::PathTraversal(
            "metadata manifest path exceeds MACOS_PATH_MAX".to_owned(),
        ));
    }
    let payload = serde_json::json!({
        "schema_version": 2,
        "profile_id": profile.id,
        "profile_name": profile.name,
        "directory_name": profile.directory_name,
        "updated_at_utc": now_rfc3339_utc(),
        "effective_tmpdir": effective_tmpdir.to_string_lossy(),
        "short_tmpdir_fallback": short_tmpdir_fallback,
        "symlink_user_dirs": options.symlink_user_dirs,
        "symlink_keychain": options.symlink_keychain,
        "isolated_surface_count": MACOS_12_ISOLATED_SURFACES.len(),
    });
    let text = serde_json::to_string_pretty(&payload)?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = ATOMIC_TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp_name = format!(
        ".macos-profile-meta.{}.{nanos}.{seq}.tmp",
        std::process::id()
    );
    let tmp_path = manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(tmp_name);
    {
        let mut f = File::create(&tmp_path)?;
        f.write_all(text.as_bytes())?;
        f.write_all(b"\n")?;
        f.sync_all()?;
    }
    fs::rename(&tmp_path, manifest_path)?;
    Ok(())
}

fn decompose_char_nfd(ch: char, out: &mut Vec<char>) {
    let code = ch as u32;
    if (0xAC00..=0xD7A3).contains(&code) {
        let s_index = code - 0xAC00;
        let l_index = s_index / (21 * 28);
        let v_index = (s_index % (21 * 28)) / 28;
        let t_index = s_index % 28;
        if let Some(l) = char::from_u32(0x1100 + l_index) {
            out.push(l);
        }
        if let Some(v) = char::from_u32(0x1161 + v_index) {
            out.push(v);
        }
        if t_index > 0 {
            if let Some(t) = char::from_u32(0x11A7 + t_index) {
                out.push(t);
            }
        }
        return;
    }

    if let Some(seq) = latin_vietnamese_nfd(ch) {
        for &c in seq {
            out.push(c);
        }
        return;
    }

    out.push(ch);
}

fn combining_class(ch: char) -> u8 {
    match ch {
        '\u{031B}' => 216,
        '\u{0323}' => 220,
        '\u{0327}' => 202,
        '\u{0300}' | '\u{0301}' | '\u{0302}' | '\u{0303}' | '\u{0304}' | '\u{0306}'
        | '\u{0307}' | '\u{0308}' | '\u{0309}' | '\u{030A}' | '\u{030B}' | '\u{030C}' => 230,
        _ => 0,
    }
}

#[allow(clippy::too_many_lines)]
fn latin_vietnamese_nfd(ch: char) -> Option<&'static [char]> {
    match ch {
        'À' => Some(&['A', '\u{0300}']),
        'Á' => Some(&['A', '\u{0301}']),
        'Â' => Some(&['A', '\u{0302}']),
        'Ã' => Some(&['A', '\u{0303}']),
        'Ä' => Some(&['A', '\u{0308}']),
        'Å' => Some(&['A', '\u{030A}']),
        'Ă' => Some(&['A', '\u{0306}']),
        'Ç' => Some(&['C', '\u{0327}']),
        'È' => Some(&['E', '\u{0300}']),
        'É' => Some(&['E', '\u{0301}']),
        'Ê' => Some(&['E', '\u{0302}']),
        'Ë' => Some(&['E', '\u{0308}']),
        'Ì' => Some(&['I', '\u{0300}']),
        'Í' => Some(&['I', '\u{0301}']),
        'Î' => Some(&['I', '\u{0302}']),
        'Ï' => Some(&['I', '\u{0308}']),
        'Ĩ' => Some(&['I', '\u{0303}']),
        'Ñ' => Some(&['N', '\u{0303}']),
        'Ò' => Some(&['O', '\u{0300}']),
        'Ó' => Some(&['O', '\u{0301}']),
        'Ô' => Some(&['O', '\u{0302}']),
        'Õ' => Some(&['O', '\u{0303}']),
        'Ö' => Some(&['O', '\u{0308}']),
        'Ơ' => Some(&['O', '\u{031B}']),
        'Ù' => Some(&['U', '\u{0300}']),
        'Ú' => Some(&['U', '\u{0301}']),
        'Û' => Some(&['U', '\u{0302}']),
        'Ü' => Some(&['U', '\u{0308}']),
        'Ũ' => Some(&['U', '\u{0303}']),
        'Ư' => Some(&['U', '\u{031B}']),
        'Ý' => Some(&['Y', '\u{0301}']),
        'à' => Some(&['a', '\u{0300}']),
        'á' => Some(&['a', '\u{0301}']),
        'â' => Some(&['a', '\u{0302}']),
        'ã' => Some(&['a', '\u{0303}']),
        'ä' => Some(&['a', '\u{0308}']),
        'å' => Some(&['a', '\u{030A}']),
        'ă' => Some(&['a', '\u{0306}']),
        'ç' => Some(&['c', '\u{0327}']),
        'è' => Some(&['e', '\u{0300}']),
        'é' => Some(&['e', '\u{0301}']),
        'ê' => Some(&['e', '\u{0302}']),
        'ë' => Some(&['e', '\u{0308}']),
        'ì' => Some(&['i', '\u{0300}']),
        'í' => Some(&['i', '\u{0301}']),
        'î' => Some(&['i', '\u{0302}']),
        'ï' => Some(&['i', '\u{0308}']),
        'ĩ' => Some(&['i', '\u{0303}']),
        'ñ' => Some(&['n', '\u{0303}']),
        'ò' => Some(&['o', '\u{0300}']),
        'ó' => Some(&['o', '\u{0301}']),
        'ô' => Some(&['o', '\u{0302}']),
        'õ' => Some(&['o', '\u{0303}']),
        'ö' => Some(&['o', '\u{0308}']),
        'ơ' => Some(&['o', '\u{031B}']),
        'ù' => Some(&['u', '\u{0300}']),
        'ú' => Some(&['u', '\u{0301}']),
        'û' => Some(&['u', '\u{0302}']),
        'ü' => Some(&['u', '\u{0308}']),
        'ũ' => Some(&['u', '\u{0303}']),
        'ư' => Some(&['u', '\u{031B}']),
        'ý' => Some(&['y', '\u{0301}']),
        'ÿ' => Some(&['y', '\u{0308}']),
        'Ạ' => Some(&['A', '\u{0323}']),
        'ạ' => Some(&['a', '\u{0323}']),
        'Ả' => Some(&['A', '\u{0309}']),
        'ả' => Some(&['a', '\u{0309}']),
        'Ấ' => Some(&['A', '\u{0302}', '\u{0301}']),
        'ấ' => Some(&['a', '\u{0302}', '\u{0301}']),
        'Ầ' => Some(&['A', '\u{0302}', '\u{0300}']),
        'ầ' => Some(&['a', '\u{0302}', '\u{0300}']),
        'Ẩ' => Some(&['A', '\u{0302}', '\u{0309}']),
        'ẩ' => Some(&['a', '\u{0302}', '\u{0309}']),
        'Ẫ' => Some(&['A', '\u{0302}', '\u{0303}']),
        'ẫ' => Some(&['a', '\u{0302}', '\u{0303}']),
        'Ậ' => Some(&['A', '\u{0323}', '\u{0302}']),
        'ậ' => Some(&['a', '\u{0323}', '\u{0302}']),
        'Ắ' => Some(&['A', '\u{0306}', '\u{0301}']),
        'ắ' => Some(&['a', '\u{0306}', '\u{0301}']),
        'Ằ' => Some(&['A', '\u{0306}', '\u{0300}']),
        'ằ' => Some(&['a', '\u{0306}', '\u{0300}']),
        'Ẳ' => Some(&['A', '\u{0306}', '\u{0309}']),
        'ẳ' => Some(&['a', '\u{0306}', '\u{0309}']),
        'Ẵ' => Some(&['A', '\u{0306}', '\u{0303}']),
        'ẵ' => Some(&['a', '\u{0306}', '\u{0303}']),
        'Ặ' => Some(&['A', '\u{0323}', '\u{0306}']),
        'ặ' => Some(&['a', '\u{0323}', '\u{0306}']),
        'Ẹ' => Some(&['E', '\u{0323}']),
        'ẹ' => Some(&['e', '\u{0323}']),
        'Ẻ' => Some(&['E', '\u{0309}']),
        'ẻ' => Some(&['e', '\u{0309}']),
        'Ẽ' => Some(&['E', '\u{0303}']),
        'ẽ' => Some(&['e', '\u{0303}']),
        'Ế' => Some(&['E', '\u{0302}', '\u{0301}']),
        'ế' => Some(&['e', '\u{0302}', '\u{0301}']),
        'Ề' => Some(&['E', '\u{0302}', '\u{0300}']),
        'ề' => Some(&['e', '\u{0302}', '\u{0300}']),
        'Ể' => Some(&['E', '\u{0302}', '\u{0309}']),
        'ể' => Some(&['e', '\u{0302}', '\u{0309}']),
        'Ễ' => Some(&['E', '\u{0302}', '\u{0303}']),
        'ễ' => Some(&['e', '\u{0302}', '\u{0303}']),
        'Ệ' => Some(&['E', '\u{0323}', '\u{0302}']),
        'ệ' => Some(&['e', '\u{0323}', '\u{0302}']),
        'Ỉ' => Some(&['I', '\u{0309}']),
        'ỉ' => Some(&['i', '\u{0309}']),
        'Ị' => Some(&['I', '\u{0323}']),
        'ị' => Some(&['i', '\u{0323}']),
        'Ọ' => Some(&['O', '\u{0323}']),
        'ọ' => Some(&['o', '\u{0323}']),
        'Ỏ' => Some(&['O', '\u{0309}']),
        'ỏ' => Some(&['o', '\u{0309}']),
        'Ố' => Some(&['O', '\u{0302}', '\u{0301}']),
        'ố' => Some(&['o', '\u{0302}', '\u{0301}']),
        'Ồ' => Some(&['O', '\u{0302}', '\u{0300}']),
        'ồ' => Some(&['o', '\u{0302}', '\u{0300}']),
        'Ổ' => Some(&['O', '\u{0302}', '\u{0309}']),
        'ổ' => Some(&['o', '\u{0302}', '\u{0309}']),
        'Ỗ' => Some(&['O', '\u{0302}', '\u{0303}']),
        'ỗ' => Some(&['o', '\u{0302}', '\u{0303}']),
        'Ộ' => Some(&['O', '\u{0323}', '\u{0302}']),
        'ộ' => Some(&['o', '\u{0323}', '\u{0302}']),
        'Ớ' => Some(&['O', '\u{031B}', '\u{0301}']),
        'ớ' => Some(&['o', '\u{031B}', '\u{0301}']),
        'Ờ' => Some(&['O', '\u{031B}', '\u{0300}']),
        'ờ' => Some(&['o', '\u{031B}', '\u{0300}']),
        'Ở' => Some(&['O', '\u{031B}', '\u{0309}']),
        'ở' => Some(&['o', '\u{031B}', '\u{0309}']),
        'Ỡ' => Some(&['O', '\u{031B}', '\u{0303}']),
        'ỡ' => Some(&['o', '\u{031B}', '\u{0303}']),
        'Ợ' => Some(&['O', '\u{031B}', '\u{0323}']),
        'ợ' => Some(&['o', '\u{031B}', '\u{0323}']),
        'Ụ' => Some(&['U', '\u{0323}']),
        'ụ' => Some(&['u', '\u{0323}']),
        'Ủ' => Some(&['U', '\u{0309}']),
        'ủ' => Some(&['u', '\u{0309}']),
        'Ứ' => Some(&['U', '\u{031B}', '\u{0301}']),
        'ứ' => Some(&['u', '\u{031B}', '\u{0301}']),
        'Ừ' => Some(&['U', '\u{031B}', '\u{0300}']),
        'ừ' => Some(&['u', '\u{031B}', '\u{0300}']),
        'Ử' => Some(&['U', '\u{031B}', '\u{0309}']),
        'ử' => Some(&['u', '\u{031B}', '\u{0309}']),
        'Ữ' => Some(&['U', '\u{031B}', '\u{0303}']),
        'ữ' => Some(&['u', '\u{031B}', '\u{0303}']),
        'Ự' => Some(&['U', '\u{031B}', '\u{0323}']),
        'ự' => Some(&['u', '\u{031B}', '\u{0323}']),
        'Ỳ' => Some(&['Y', '\u{0300}']),
        'ỳ' => Some(&['y', '\u{0300}']),
        'Ỵ' => Some(&['Y', '\u{0323}']),
        'ỵ' => Some(&['y', '\u{0323}']),
        'Ỷ' => Some(&['Y', '\u{0309}']),
        'ỷ' => Some(&['y', '\u{0309}']),
        'Ỹ' => Some(&['Y', '\u{0303}']),
        'ỹ' => Some(&['y', '\u{0303}']),
        _ => None,
    }
}

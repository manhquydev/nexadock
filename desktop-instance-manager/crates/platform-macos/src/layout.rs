//! macOS isolated profile layout, `.macos-profile-meta.json` manifest, `SUN_LEN` (104-byte)
//! `SingletonSocket` safe `TMPDIR` allocation, NFD/NFC Unicode path normalization, user-folder
//! symlinks, `open -n --env CFFIXED_USER_HOME` builders, and `ZaloCall` socket diagnostics.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use dim_core::{
    ensure_not_primary_zalo_path, now_rfc3339_utc, DimError, ProfileRecord, ZALO_PROFILE_ID_FILE,
};
use dim_profile_store::{is_symlink_or_reparse, verify_profile_id_marker, write_profile_id_marker};

use crate::bundle::{extract_plist_string_key, validate_macos_path, MACOS_PATH_MAX};
use crate::ops::{MacOsSystemOps, NativeMacOsSystemOps};

/// macOS user directories optionally symlinked into each isolated `CFFIXED_USER_HOME`.
pub const MACOS_SYMLINK_DIRS: &[&str] = &[
    "Library/Keychains",
    "Downloads",
    "Desktop",
    "Documents",
    "Pictures",
    "Movies",
    "Music",
];

/// Non-Keychain user convenience folders optionally symlinked from the real home directory.
pub const MACOS_USER_FOLDER_SYMLINKS: &[&str] = &[
    "Downloads",
    "Desktop",
    "Documents",
    "Pictures",
    "Movies",
    "Music",
];

/// Isolated profile subdirectories that MUST NEVER be symlinked to the host `~/Library/`.
pub const FORBIDDEN_ISOLATION_SYMLINK_RELS: &[&str] = &[
    "Library/Application Support",
    "Library/Application Support/ZaloData",
    "Library/Application Support/Zalo",
    "Library/Preferences",
];

/// Global Unix domain sockets bound by `ZaloHelper.app/Contents/MacOS/ZaloCall`.
pub const MACOS_CALL_SOCKETS: &[&str] = &["/tmp/socketzalorecv2021", "/tmp/socketzalosend2021"];

/// Darwin `<sys/un.h>` `sockaddr_un.sun_path` fixed array capacity (`103` path bytes + NUL).
pub const MACOS_SUN_PATH_MAX: usize = 104;

/// Maximum usable UTF-8 byte length for a Unix domain socket path on macOS (`SUN_LEN - 1`).
pub const MACOS_SOCKET_PATH_MAX_BYTES: usize = MACOS_SUN_PATH_MAX - 1;

/// Suffix length reserved by Chromium `ProcessSingleton` (`/.org.chromium.Chromium.XXXXXX/SingletonSocket`).
pub const CHROMIUM_SINGLETON_SOCKET_SUFFIX_BYTES: usize = 46;

/// Maximum UTF-8 byte length of `TMPDIR` before Chromium's `SingletonSocket` risks exceeding `SUN_LEN` (`54` bytes).
pub const MACOS_SAFE_TMPDIR_MAX_BYTES: usize = 54;

/// Hidden metadata manifest persisted inside each isolated macOS profile root.
pub const MACOS_PROFILE_META_FILE: &str = ".macos-profile-meta.json";

/// Configuration governing user-folder and Keychain symlink creation inside an isolated macOS profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MacOsLayoutOptions {
    /// Whether `Downloads`, `Desktop`, `Documents`, `Pictures`, `Movies`, `Music` are symlinked.
    pub symlink_user_dirs: bool,
    /// Whether `Library/Keychains` is symlinked so Electron `safeStorage` reaches `login.keychain-db`.
    pub symlink_keychain: bool,
}

impl Default for MacOsLayoutOptions {
    fn default() -> Self {
        Self {
            symlink_user_dirs: true,
            symlink_keychain: true,
        }
    }
}

/// Verified directory paths produced by [`ensure_macos_profile_layout_with_ops`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacOsResolvedLayout {
    /// Root isolated profile directory (`<data_dir>/Profiles/<slug>_<8hex>`).
    pub profile_dir: PathBuf,
    /// Isolated `<profile_dir>/Library/Application Support/ZaloData`.
    pub zalo_data_dir: PathBuf,
    /// Effective `TMPDIR` passed to the child instance (either `<profile_dir>/tmp` or short `/tmp/dim-zalo-<uid>-<id>`).
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

/// Ensures isolated macOS directories, pre-seed markers, and `.zalo-profile-id` exist inside `profile_dir`.
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
/// `0o700` owner-only directories, forbidden-symlink checks, `SUN_LEN` short-tmp fallback, and
/// `.macos-profile-meta.json` persistence.
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

    // Tighten parent `Profiles/` directory when managed under MultiZaloDesktop.
    if let Some(parent) = profile_dir.parent() {
        if parent.file_name().and_then(|s| s.to_str()) == Some("Profiles") && parent.exists() {
            let _ = ops.ensure_private_dir(parent);
        }
    }

    ops.ensure_private_dir(profile_dir)?;

    // Ensure none of the critical isolated Library subdirectories were replaced by symlinks.
    verify_no_forbidden_isolation_symlinks(profile_dir)?;

    let isolated_subdirs = [
        profile_dir.join("Library"),
        profile_dir.join("Library").join("Application Support"),
        zalo_data.clone(),
        profile_dir
            .join("Library")
            .join("Application Support")
            .join("Zalo"),
        profile_dir.join("Library").join("Preferences"),
        profile_dir.join("Library").join("Caches"),
        profile_dir.join("Library").join("Saved Application State"),
        profile_dir.join("Library").join("Logs"),
        profile_dir.join("Library").join("HTTPStorages"),
        profile_dir.join("Library").join("WebKit"),
        profile_dir.join("tmp"),
    ];
    for dir in &isolated_subdirs {
        ops.ensure_private_dir(dir)?;
    }

    if !options.symlink_user_dirs {
        ops.ensure_private_dir(&profile_dir.join("Documents").join("Zalo Received Files"))?;
    }

    // Verify or write `.zalo-profile-id` marker.
    verify_profile_id_marker(&zalo_data, &profile.id, false)?;
    if !zalo_data.join(ZALO_PROFILE_ID_FILE).exists() {
        write_profile_id_marker(&zalo_data, &profile.id)?;
    }

    // Pre-seed Zalo migration markers so `migration.js` never probes legacy paths.
    let app_support = profile_dir.join("Library").join("Application Support");
    let zmeta_path = app_support.join("zmeta.json");
    if !zmeta_path.exists() {
        fs::write(&zmeta_path, b"{\"failedMigrateCount\":0}")?;
    }
    let migrate_cfg_path = zalo_data.join("migrate-config.json");
    if !migrate_cfg_path.exists() {
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

/// Resolves an isolated `TMPDIR` guaranteed to keep Chromium's `SingletonSocket` within macOS's
/// 104-byte `sockaddr_un.sun_path` (`SUN_LEN`) limit.
pub fn resolve_effective_tmpdir(
    ops: &dyn MacOsSystemOps,
    profile_id: &str,
    profile_dir: &Path,
) -> Result<(PathBuf, bool), DimError> {
    validate_macos_path(profile_dir)?;
    let candidate_tmp = profile_dir.join("tmp");
    let candidate_len = candidate_tmp.to_string_lossy().len();

    if candidate_len <= MACOS_SAFE_TMPDIR_MAX_BYTES {
        ops.ensure_private_dir(&candidate_tmp)?;
        return Ok((candidate_tmp, false));
    }

    let safe_id: String = profile_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(16)
        .collect();
    if safe_id.is_empty() {
        return Err(DimError::InvalidProfileName(
            "cannot derive short TMPDIR for empty profile ID".to_owned(),
        ));
    }

    let uid = ops.current_uid();
    let leaf = format!("dim-zalo-{uid}-{safe_id}");
    let default_short_tmp = ops.short_temp_root().join(&leaf);

    // On Unix/macOS, if a custom `short_temp_root` in tests is itself too long, fall back to `/tmp`.
    #[cfg(unix)]
    let short_tmp = if default_short_tmp.to_string_lossy().len() > MACOS_SAFE_TMPDIR_MAX_BYTES {
        PathBuf::from("/tmp").join(&leaf)
    } else {
        default_short_tmp
    };
    #[cfg(not(unix))]
    let short_tmp = default_short_tmp;

    let projected_socket_len = short_tmp
        .to_string_lossy()
        .len()
        .saturating_add(CHROMIUM_SINGLETON_SOCKET_SUFFIX_BYTES);
    if cfg!(unix) && projected_socket_len > MACOS_SOCKET_PATH_MAX_BYTES {
        return Err(DimError::PathTraversal(format!(
            "short TMPDIR '{}' ({projected_socket_len} projected socket bytes) exceeds macOS SUN_LEN ({MACOS_SOCKET_PATH_MAX_BYTES})",
            short_tmp.display()
        )));
    }

    // Critical `/tmp` anti-squatting check: reject pre-planted symlinks and enforce `0o700` + UID ownership.
    if let Ok(meta) = fs::symlink_metadata(&short_tmp) {
        if is_symlink_or_reparse(&meta) {
            return Err(DimError::PathTraversal(format!(
                "refusing symlinked short TMPDIR '{}'",
                short_tmp.display()
            )));
        }
    }
    ops.ensure_private_dir(&short_tmp)?;
    Ok((short_tmp, true))
}

/// Creates symlinks from `<profile_dir>/<rel>` to `<real_home>/<rel>` on Unix when missing.
pub fn ensure_macos_user_symlinks(profile_dir: &Path, real_home: &Path) -> Result<(), DimError> {
    ensure_macos_user_symlinks_with_options(profile_dir, real_home, MacOsLayoutOptions::default())
}

/// Creates configurable user-directory and `Library/Keychains` symlinks on Unix while strictly
/// refusing to symlink `Application Support` or `Preferences` or overlap primary `ZaloData`.
pub fn ensure_macos_user_symlinks_with_options(
    profile_dir: &Path,
    real_home: &Path,
    options: MacOsLayoutOptions,
) -> Result<(), DimError> {
    ensure_not_macos_primary_path(profile_dir, Some(real_home))?;
    verify_no_forbidden_isolation_symlinks(profile_dir)?;

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
            rels.extend_from_slice(MACOS_USER_FOLDER_SYMLINKS);
        }

        for rel in rels {
            let src = real_home.join(rel);
            if !src.exists() {
                continue;
            }
            let dst = profile_dir.join(rel);
            if fs::symlink_metadata(&dst).is_ok() {
                continue;
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

/// Verifies none of the isolated `Application Support` / `Preferences` directories inside `profile_dir` are symlinks.
pub fn verify_no_forbidden_isolation_symlinks(profile_dir: &Path) -> Result<(), DimError> {
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

/// Builds full hardened argv for `/usr/bin/open` including `SUN_LEN`-safe `TMPDIR`, `DIM_PROFILE_ID`,
/// and `--args --user-data-dir=<zalo_data_dir>` for child helper command-line ownership tagging.
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
    let mut args = vec![
        "-n".to_owned(),
        "-a".to_owned(),
        app_bundle.to_string_lossy().into_owned(),
        "--env".to_owned(),
        format!("CFFIXED_USER_HOME={}", profile_dir.display()),
        "--env".to_owned(),
        format!("HOME={}", profile_dir.display()),
        "--env".to_owned(),
        format!("TMPDIR={}", effective_tmpdir.display()),
        "--env".to_owned(),
        format!("DIM_PROFILE_ID={profile_id}"),
    ];

    let mut app_args = Vec::new();
    if include_user_data_dir_arg && !extra_args.iter().any(|a| a.starts_with("--user-data-dir")) {
        app_args.push(format!("--user-data-dir={}", zalo_data.display()));
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
    vec![
        ("CFFIXED_USER_HOME".to_owned(), p_str.clone()),
        ("HOME".to_owned(), p_str),
        ("TMPDIR".to_owned(), t_str.clone()),
        ("TEMP".to_owned(), t_str.clone()),
        ("TMP".to_owned(), t_str),
        ("APPDATA".to_owned(), app_support),
        ("DIM_PROFILE_ID".to_owned(), profile_id.to_owned()),
    ]
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
    // Canonical sort of adjacent non-starter combining marks by combining class.
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

fn write_profile_meta_atomic(
    manifest_path: &Path,
    profile: &ProfileRecord,
    effective_tmpdir: &Path,
    short_tmpdir_fallback: bool,
    options: MacOsLayoutOptions,
) -> Result<(), DimError> {
    if manifest_path.len_check_exceeded() {
        return Err(DimError::PathTraversal(
            "metadata manifest path exceeds MACOS_PATH_MAX".to_owned(),
        ));
    }
    let payload = serde_json::json!({
        "schema_version": 1,
        "profile_id": profile.id,
        "profile_name": profile.name,
        "directory_name": profile.directory_name,
        "updated_at_utc": now_rfc3339_utc(),
        "effective_tmpdir": effective_tmpdir.to_string_lossy(),
        "short_tmpdir_fallback": short_tmpdir_fallback,
        "symlink_user_dirs": options.symlink_user_dirs,
        "symlink_keychain": options.symlink_keychain,
    });
    let text = serde_json::to_string_pretty(&payload)?;
    let tmp_path = manifest_path.with_extension("json.tmp");
    {
        let mut f = File::create(&tmp_path)?;
        f.write_all(text.as_bytes())?;
        f.write_all(b"\n")?;
        f.sync_all()?;
    }
    fs::rename(&tmp_path, manifest_path)?;
    Ok(())
}

trait PathLenCheck {
    fn len_check_exceeded(&self) -> bool;
}

impl PathLenCheck for Path {
    fn len_check_exceeded(&self) -> bool {
        self.to_string_lossy().len() >= MACOS_PATH_MAX
    }
}

fn decompose_char_nfd(ch: char, out: &mut Vec<char>) {
    // Hangul syllable algorithmic NFD decomposition (U+AC00..=U+D7A3).
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
        '\u{031B}' => 216, // COMBINING HORN (ơ, ư)
        '\u{0323}' => 220, // COMBINING DOT BELOW (ạ, ệ, ự...)
        '\u{0327}' => 202, // COMBINING CEDILLA
        '\u{0300}' | '\u{0301}' | '\u{0302}' | '\u{0303}' | '\u{0304}' | '\u{0306}'
        | '\u{0307}' | '\u{0308}' | '\u{0309}' | '\u{030A}' | '\u{030B}' | '\u{030C}' => 230,
        _ => 0,
    }
}

#[allow(clippy::too_many_lines)]
fn latin_vietnamese_nfd(ch: char) -> Option<&'static [char]> {
    match ch {
        // Uppercase & lowercase Latin-1 / Extended-A vowels & accents
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
        // Vietnamese Latin Extended Additional (U+1EA0..=U+1EF9)
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

//! macOS profile layout, `open -n --env CFFIXED_USER_HOME` command builder,
//! `SingletonLock` parser, `Info.plist` version extractor, and call socket diagnostics.

use std::fs;
use std::path::{Path, PathBuf};

use dim_core::{DimError, ProfileRecord, ZALO_PROFILE_ID_FILE};
use dim_profile_store::write_profile_id_marker;

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

/// Global Unix domain sockets bound by `ZaloHelper.app/Contents/MacOS/ZaloCall`.
pub const MACOS_CALL_SOCKETS: &[&str] = &["/tmp/socketzalorecv2021", "/tmp/socketzalosend2021"];

/// Returns `<profile_dir>/Library/Application Support/ZaloData`.
#[must_use]
pub fn macos_zalo_data_dir(profile_dir: &Path) -> PathBuf {
    profile_dir
        .join("Library")
        .join("Application Support")
        .join("ZaloData")
}

/// Ensures isolated macOS directories and `.zalo-profile-id` exist inside `profile_dir`.
pub fn ensure_macos_profile_layout(
    profile: &ProfileRecord,
    profile_dir: &Path,
) -> Result<(), DimError> {
    let zalo_data = macos_zalo_data_dir(profile_dir);
    let tmp_dir = profile_dir.join("tmp");
    let lib_dir = profile_dir.join("Library");
    fs::create_dir_all(&zalo_data)?;
    fs::create_dir_all(&tmp_dir)?;
    fs::create_dir_all(&lib_dir)?;

    let marker = zalo_data.join(ZALO_PROFILE_ID_FILE);
    if !marker.exists() {
        write_profile_id_marker(&zalo_data, &profile.id)?;
    }

    Ok(())
}

/// Creates symlinks from `<profile_dir>/<rel>` to `<real_home>/<rel>` on Unix when missing.
pub fn ensure_macos_user_symlinks(profile_dir: &Path, real_home: &Path) -> Result<(), DimError> {
    #[cfg(unix)]
    {
        for rel in MACOS_SYMLINK_DIRS {
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
        let _ = (profile_dir, real_home);
        Ok(())
    }
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
    let key_marker = "<key>CFBundleShortVersionString</key>";
    let idx = plist_xml.find(key_marker)?;
    let rest = &plist_xml[idx + key_marker.len()..];
    let start_tag = "<string>";
    let end_tag = "</string>";
    let s_idx = rest.find(start_tag)? + start_tag.len();
    let after_start = &rest[s_idx..];
    let e_idx = after_start.find(end_tag)?;
    let ver = after_start[..e_idx].trim();
    (!ver.is_empty()).then(|| ver.to_owned())
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

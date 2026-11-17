//! Linux XDG/HOME profile layout, optional Bubblewrap (`bwrap`) command builder,
//! Ubuntu AppArmor `--no-sandbox` advisory helpers, and `SingletonLock` parsing.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use dim_core::{DimError, ProfileRecord, ZALO_PROFILE_ID_FILE};
use dim_profile_store::write_profile_id_marker;

/// Advisory explaining Ubuntu 23.10+/24.04 AppArmor unprivileged user namespace restriction.
pub const APPARMOR_NO_SANDBOX_NOTE: &str =
    "Ubuntu 23.10+/24.04 AppArmor restricts unprivileged user namespaces; user-supplied local Electron AppImages may require --no-sandbox unless an AppArmor profile is configured.";

/// Returns `<profile_dir>/.config/ZaloData`.
#[must_use]
pub fn linux_zalo_data_dir(profile_dir: &Path) -> PathBuf {
    profile_dir.join(".config").join("ZaloData")
}

/// Ensures isolated Linux XDG directories and `.zalo-profile-id` exist inside `profile_dir`.
pub fn ensure_linux_profile_layout(
    profile: &ProfileRecord,
    profile_dir: &Path,
) -> Result<(), DimError> {
    let config_zalo = linux_zalo_data_dir(profile_dir);
    let local_share = profile_dir.join(".local").join("share");
    let cache_dir = profile_dir.join(".cache");
    let tmp_dir = profile_dir.join("tmp");

    fs::create_dir_all(&config_zalo)?;
    fs::create_dir_all(&local_share)?;
    fs::create_dir_all(&cache_dir)?;
    fs::create_dir_all(&tmp_dir)?;

    let marker = config_zalo.join(ZALO_PROFILE_ID_FILE);
    if !marker.exists() {
        write_profile_id_marker(&config_zalo, &profile.id)?;
    }

    Ok(())
}

/// Builds isolated Linux XDG environment variable overrides for `profile_dir`.
#[must_use]
pub fn build_linux_isolated_env(
    profile: &ProfileRecord,
    profile_dir: &Path,
) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    env.insert("HOME".to_owned(), profile_dir.to_string_lossy().into_owned());
    env.insert(
        "XDG_CONFIG_HOME".to_owned(),
        profile_dir.join(".config").to_string_lossy().into_owned(),
    );
    env.insert(
        "XDG_DATA_HOME".to_owned(),
        profile_dir
            .join(".local")
            .join("share")
            .to_string_lossy()
            .into_owned(),
    );
    env.insert(
        "XDG_CACHE_HOME".to_owned(),
        profile_dir.join(".cache").to_string_lossy().into_owned(),
    );
    env.insert(
        "TMPDIR".to_owned(),
        profile_dir.join("tmp").to_string_lossy().into_owned(),
    );
    env.insert("DIM_PROFILE_ID".to_owned(), profile.id.clone());
    env.insert("DIM_PROFILE_NAME".to_owned(), profile.name.clone());
    env
}

/// Builds an optional Bubblewrap (`bwrap`) sandbox argument vector binding `profile_dir` as `$HOME`.
#[must_use]
pub fn build_bwrap_args(
    profile_dir: &Path,
    executable: &Path,
    app_args: &[String],
) -> Vec<String> {
    let mut args = vec![
        "--die-with-parent".to_owned(),
        "--ro-bind".to_owned(),
        "/usr".to_owned(),
        "/usr".to_owned(),
        "--ro-bind-try".to_owned(),
        "/lib".to_owned(),
        "/lib".to_owned(),
        "--ro-bind-try".to_owned(),
        "/lib64".to_owned(),
        "/lib64".to_owned(),
        "--ro-bind-try".to_owned(),
        "/bin".to_owned(),
        "/bin".to_owned(),
        "--ro-bind-try".to_owned(),
        "/etc/ssl".to_owned(),
        "/etc/ssl".to_owned(),
        "--ro-bind-try".to_owned(),
        "/etc/resolv.conf".to_owned(),
        "/etc/resolv.conf".to_owned(),
        "--proc".to_owned(),
        "/proc".to_owned(),
        "--dev".to_owned(),
        "/dev".to_owned(),
        "--bind".to_owned(),
        profile_dir.to_string_lossy().into_owned(),
        profile_dir.to_string_lossy().into_owned(),
        "--setenv".to_owned(),
        "HOME".to_owned(),
        profile_dir.to_string_lossy().into_owned(),
        "--setenv".to_owned(),
        "XDG_CONFIG_HOME".to_owned(),
        profile_dir.join(".config").to_string_lossy().into_owned(),
        "--setenv".to_owned(),
        "XDG_DATA_HOME".to_owned(),
        profile_dir
            .join(".local")
            .join("share")
            .to_string_lossy()
            .into_owned(),
        "--setenv".to_owned(),
        "XDG_CACHE_HOME".to_owned(),
        profile_dir.join(".cache").to_string_lossy().into_owned(),
        "--".to_owned(),
        executable.to_string_lossy().into_owned(),
    ];
    args.extend(app_args.iter().cloned());
    args
}

/// Checks whether a user-supplied AppImage invocation requires the AppArmor `--no-sandbox` advisory.
#[must_use]
pub fn looks_like_appimage(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("appimage"))
        .unwrap_or(false)
}

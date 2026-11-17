//! Hardened Linux `HOME` / XDG Base Directory profile layout, `user-dirs.dirs` confinement,
//! `0o700` permission enforcement, sanitized child environment builder, and `bubblewrap` (`bwrap`)
//! argument construction.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use dim_core::{
    ensure_not_primary_zalo_path, validate_directory_input, DimError, ProfileRecord,
    ZALO_PROFILE_ID_FILE,
};
use dim_profile_store::{is_symlink_or_reparse, verify_profile_id_marker, write_profile_id_marker};

use crate::status::SecurityBlockReason;

/// Advisory explaining Ubuntu 23.10+/24.04 AppArmor unprivileged user namespace restriction
/// and why `dim-platform-linux` refuses automatic `--no-sandbox` fallback.
pub const APPARMOR_NO_SANDBOX_NOTE: &str =
    "Ubuntu 23.10+/24.04 AppArmor restricts unprivileged user namespaces; unconfined Electron AppImages fail without a targeted AppArmor `userns` profile, and automatic `--no-sandbox` fallback is SECURITY_BLOCKED.";

/// Safe desktop/session environment variables forwarded across `Command::env_clear()` on Linux.
pub const PASSTHROUGH_SESSION_ENV_KEYS: &[&str] = &[
    "PATH",
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "XAUTHORITY",
    "XDG_RUNTIME_DIR",
    "XDG_SESSION_TYPE",
    "XDG_SESSION_CLASS",
    "XDG_SESSION_DESKTOP",
    "XDG_CURRENT_DESKTOP",
    "DESKTOP_SESSION",
    "DBUS_SESSION_BUS_ADDRESS",
    "PULSE_SERVER",
    "PULSE_COOKIE",
    "PIPEWIRE_REMOTE",
    "GTK_THEME",
    "GTK_IM_MODULE",
    "QT_IM_MODULE",
    "XMODIFIERS",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TZ",
    "TERM",
    "FIXTURE_ZALO_USER_ID",
];

/// Returns `<profile_dir>/.config/ZaloData`.
#[must_use]
pub fn linux_zalo_data_dir(profile_dir: &Path) -> PathBuf {
    profile_dir.join(".config").join("ZaloData")
}

/// Ensures isolated Linux XDG directories, user-dirs confinement files, pre-seed migration
/// markers, `0o700` permissions, and `.zalo-profile-id` exist inside `profile_dir`.
///
/// Rejects path traversal, primary `ZaloData` overlap, symlink escapes, and cross-profile
/// `.zalo-profile-id` overwrites.
pub fn ensure_linux_profile_layout(
    profile: &ProfileRecord,
    profile_dir: &Path,
) -> Result<(), DimError> {
    validate_directory_input(profile_dir)?;
    ensure_not_primary_zalo_path(profile_dir)?;

    reject_if_symlink(profile_dir)?;
    fs::create_dir_all(profile_dir)?;
    set_dir_mode_0700(profile_dir)?;

    // If parent is `Profiles` owned by current user, tighten it to `0o700` as well.
    if let Some(parent) = profile_dir.parent() {
        if parent
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|n| n == "Profiles")
        {
            let _ = set_dir_mode_0700(parent);
        }
    }

    let config_dir = profile_dir.join(".config");
    let config_zalo = linux_zalo_data_dir(profile_dir);
    let config_zalo_shell = config_dir.join("Zalo");
    let local_dir = profile_dir.join(".local");
    let local_share = local_dir.join("share");
    let local_state = local_dir.join("state");
    let cache_dir = profile_dir.join(".cache");
    let tmp_dir = profile_dir.join("tmp");
    let downloads_dir = profile_dir.join("Downloads");
    let documents_dir = profile_dir.join("Documents");
    let received_files_dir = documents_dir.join("Zalo Received Files");

    let managed_dirs = [
        &config_dir,
        &config_zalo,
        &config_zalo_shell,
        &local_dir,
        &local_share,
        &local_state,
        &cache_dir,
        &tmp_dir,
        &downloads_dir,
        &documents_dir,
        &received_files_dir,
    ];

    for dir in managed_dirs {
        reject_if_symlink(dir)?;
        fs::create_dir_all(dir)?;
        set_dir_mode_0700(dir)?;
    }

    // Prevent one profile from overwriting another profile's data directory.
    let marker = config_zalo.join(ZALO_PROFILE_ID_FILE);
    reject_if_symlink(&marker)?;
    if marker.exists() {
        verify_profile_id_marker(&config_zalo, &profile.id, true)?;
    } else {
        write_profile_id_marker(&config_zalo, &profile.id)?;
        set_file_mode_0600(&marker)?;
    }

    // Pre-seed migration suppression markers so VNG `migration.js` never probes host paths.
    let zmeta_path = config_dir.join("zmeta.json");
    write_seed_file_if_absent(&zmeta_path, b"{\"failedMigrateCount\":0}\n")?;

    let migrate_cfg_path = config_zalo.join("migrate-config.json");
    write_seed_file_if_absent(&migrate_cfg_path, b"{\"zalopc_m_c\":true}\n")?;

    // Pre-seed XDG `user-dirs.dirs` so Chromium/Electron `getPath("documents"|"downloads")`
    // resolves strictly inside `$HOME` (`profile_dir`) instead of escaping to host directories.
    let user_dirs_path = config_dir.join("user-dirs.dirs");
    let user_dirs_content = concat!(
        "# Isolated MultiZaloDesktop XDG user directories\n",
        "XDG_DESKTOP_DIR=\"$HOME/Downloads\"\n",
        "XDG_DOWNLOAD_DIR=\"$HOME/Downloads\"\n",
        "XDG_TEMPLATES_DIR=\"$HOME/Documents\"\n",
        "XDG_PUBLICSHARE_DIR=\"$HOME/Downloads\"\n",
        "XDG_DOCUMENTS_DIR=\"$HOME/Documents\"\n",
        "XDG_MUSIC_DIR=\"$HOME/Downloads\"\n",
        "XDG_PICTURES_DIR=\"$HOME/Downloads\"\n",
        "XDG_VIDEOS_DIR=\"$HOME/Downloads\"\n",
    );
    write_seed_file_if_absent(&user_dirs_path, user_dirs_content.as_bytes())?;

    let user_dirs_locale = config_dir.join("user-dirs.locale");
    write_seed_file_if_absent(&user_dirs_locale, b"en_US\n")?;

    Ok(())
}

/// Builds isolated Linux `HOME` and XDG Base Directory environment variable overrides for `profile_dir`.
#[must_use]
pub fn build_linux_isolated_env(
    profile: &ProfileRecord,
    profile_dir: &Path,
) -> BTreeMap<String, String> {
    let home = profile_dir.to_string_lossy().into_owned();
    let config = profile_dir.join(".config").to_string_lossy().into_owned();
    let data = profile_dir
        .join(".local")
        .join("share")
        .to_string_lossy()
        .into_owned();
    let state = profile_dir
        .join(".local")
        .join("state")
        .to_string_lossy()
        .into_owned();
    let cache = profile_dir.join(".cache").to_string_lossy().into_owned();
    let tmp = profile_dir.join("tmp").to_string_lossy().into_owned();
    let downloads = profile_dir.join("Downloads").to_string_lossy().into_owned();
    let documents = profile_dir.join("Documents").to_string_lossy().into_owned();

    let mut env = BTreeMap::new();
    env.insert("HOME".to_owned(), home);
    env.insert("XDG_CONFIG_HOME".to_owned(), config);
    env.insert("XDG_DATA_HOME".to_owned(), data);
    env.insert("XDG_STATE_HOME".to_owned(), state);
    env.insert("XDG_CACHE_HOME".to_owned(), cache);
    env.insert("XDG_DOWNLOAD_DIR".to_owned(), downloads);
    env.insert("XDG_DOCUMENTS_DIR".to_owned(), documents);
    env.insert("TMPDIR".to_owned(), tmp.clone());
    env.insert("TEMP".to_owned(), tmp.clone());
    env.insert("TMP".to_owned(), tmp);
    env.insert("DIM_PROFILE_ID".to_owned(), profile.id.clone());
    env.insert("DIM_PROFILE_NAME".to_owned(), profile.name.clone());
    env
}

/// Builds a complete sanitized child process environment map suitable for `Command::env_clear()`,
/// forwarding only whitelisted session/display/audio variables and stripping dangerous loader/host
/// overrides (`LD_PRELOAD`, `LD_AUDIT`, `NODE_OPTIONS`, `APPDATA`, `USERPROFILE`, `CFFIXED_USER_HOME`).
#[must_use]
pub fn build_sanitized_linux_spawn_env(
    profile: &ProfileRecord,
    profile_dir: &Path,
    host_env_lookup: &dyn Fn(&str) -> Option<String>,
) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    for &key in PASSTHROUGH_SESSION_ENV_KEYS {
        if let Some(val) = host_env_lookup(key) {
            if !val.is_empty() && !val.contains('\0') {
                env.insert(key.to_owned(), val);
            }
        }
    }
    if !env.contains_key("PATH") {
        env.insert("PATH".to_owned(), "/usr/local/bin:/usr/bin:/bin".to_owned());
    }
    for (k, v) in build_linux_isolated_env(profile, profile_dir) {
        env.insert(k, v);
    }
    env
}

/// Builds a Bubblewrap (`bwrap`) sandbox argument vector binding `profile_dir` as `$HOME`
/// while hiding the host user's real home directory and sibling profiles.
#[must_use]
pub fn build_bwrap_args(profile_dir: &Path, executable: &Path, app_args: &[String]) -> Vec<String> {
    build_bwrap_desktop_args(profile_dir, executable, app_args, &|k| {
        std::env::var(k).ok()
    })
}

/// Parameterized Bubblewrap (`bwrap`) argument builder with selective Wayland, X11, PipeWire,
/// PulseAudio, and D-Bus session socket passthrough.
#[must_use]
pub fn build_bwrap_desktop_args(
    profile_dir: &Path,
    executable: &Path,
    app_args: &[String],
    host_env_lookup: &dyn Fn(&str) -> Option<String>,
) -> Vec<String> {
    let pdir_str = profile_dir.to_string_lossy().into_owned();
    let exe_str = executable.to_string_lossy().into_owned();

    let mut args = vec![
        "--die-with-parent".to_owned(),
        "--unshare-ipc".to_owned(),
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
        "/etc/ca-certificates".to_owned(),
        "/etc/ca-certificates".to_owned(),
        "--ro-bind-try".to_owned(),
        "/etc/resolv.conf".to_owned(),
        "/etc/resolv.conf".to_owned(),
        "--ro-bind-try".to_owned(),
        "/etc/fonts".to_owned(),
        "/etc/fonts".to_owned(),
        "--ro-bind-try".to_owned(),
        "/etc/localtime".to_owned(),
        "/etc/localtime".to_owned(),
        "--proc".to_owned(),
        "/proc".to_owned(),
        "--dev".to_owned(),
        "/dev".to_owned(),
        "--dev-bind-try".to_owned(),
        "/dev/dri".to_owned(),
        "/dev/dri".to_owned(),
        "--dev-bind-try".to_owned(),
        "/dev/fuse".to_owned(),
        "/dev/fuse".to_owned(),
        "--tmpfs".to_owned(),
        "/tmp".to_owned(),
        "--ro-bind-try".to_owned(),
        "/tmp/.X11-unix".to_owned(),
        "/tmp/.X11-unix".to_owned(),
    ];

    // Forward selective Wayland, PipeWire, PulseAudio, and session D-Bus sockets under XDG_RUNTIME_DIR.
    if let Some(runtime_dir) = host_env_lookup("XDG_RUNTIME_DIR").filter(|s| !s.trim().is_empty()) {
        let rt = Path::new(runtime_dir.trim());
        for sub in ["pipewire-0", "pulse", "bus"] {
            let sock = rt.join(sub).to_string_lossy().into_owned();
            args.push("--ro-bind-try".to_owned());
            args.push(sock.clone());
            args.push(sock);
        }
        if let Some(wl) = host_env_lookup("WAYLAND_DISPLAY").filter(|s| !s.trim().is_empty()) {
            let wl_sock = rt.join(wl.trim()).to_string_lossy().into_owned();
            args.push("--ro-bind-try".to_owned());
            args.push(wl_sock.clone());
            args.push(wl_sock);
        }
    }

    if let Some(xauth) = host_env_lookup("XAUTHORITY").filter(|s| !s.trim().is_empty()) {
        let xa = xauth.trim().to_owned();
        args.push("--ro-bind-try".to_owned());
        args.push(xa.clone());
        args.push(xa);
    }

    // Bind only this profile's isolated directory read-write.
    args.push("--bind".to_owned());
    args.push(pdir_str.clone());
    args.push(pdir_str.clone());

    // Bind executable read-only if it resides outside `/usr`, `/bin`, and `profile_dir`
    // (e.g., `/home/<user>/Applications/Zalo.AppImage` or `/opt/zalo/zalo`), without exposing real `$HOME`.
    if executable.is_absolute()
        && !executable.starts_with("/usr")
        && !executable.starts_with("/bin")
        && !executable.starts_with(profile_dir)
    {
        args.push("--ro-bind".to_owned());
        args.push(exe_str.clone());
        args.push(exe_str.clone());
    }

    args.extend([
        "--setenv".to_owned(),
        "HOME".to_owned(),
        pdir_str,
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
        "XDG_STATE_HOME".to_owned(),
        profile_dir
            .join(".local")
            .join("state")
            .to_string_lossy()
            .into_owned(),
        "--setenv".to_owned(),
        "XDG_CACHE_HOME".to_owned(),
        profile_dir.join(".cache").to_string_lossy().into_owned(),
        "--setenv".to_owned(),
        "TMPDIR".to_owned(),
        profile_dir.join("tmp").to_string_lossy().into_owned(),
        "--".to_owned(),
        exe_str,
    ]);
    args.extend(app_args.iter().cloned());
    args
}

/// Checks whether a path has an `.AppImage` filename extension (case-insensitive).
#[must_use]
pub fn looks_like_appimage(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("appimage"))
}

/// Computes total disk bytes occupied by regular files inside `profile_dir`, never following symlinks.
#[must_use]
pub fn measure_profile_disk_usage_bytes(profile_dir: &Path) -> u64 {
    if !profile_dir.exists() {
        return 0;
    }
    let mut total = 0u64;
    let mut stack = vec![profile_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(meta) = fs::symlink_metadata(&dir) else {
            continue;
        };
        if is_symlink_or_reparse(&meta) || !meta.is_dir() {
            continue;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            let Ok(m) = fs::symlink_metadata(&p) else {
                continue;
            };
            if is_symlink_or_reparse(&m) {
                continue;
            }
            if m.is_dir() {
                stack.push(p);
            } else if m.is_file() {
                total = total.saturating_add(m.len());
            }
        }
    }
    total
}

/// Verifies on Unix that `dir` is a regular directory owned by `expected_uid` without group/world write bits.
pub fn verify_directory_ownership_and_mode(
    dir: &Path,
    expected_uid: u32,
    require_strict_0700: bool,
) -> Result<(), SecurityBlockReason> {
    let meta = fs::symlink_metadata(dir).map_err(|e| SecurityBlockReason::InsecurePermissions {
        path: dir.display().to_string(),
        detail: format!("cannot stat directory: {e}"),
    })?;

    if is_symlink_or_reparse(&meta) || !meta.is_dir() {
        return Err(SecurityBlockReason::InsecurePermissions {
            path: dir.display().to_string(),
            detail: "directory is a symlink or not a regular directory".to_owned(),
        });
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let actual_uid = meta.uid();
        if actual_uid != expected_uid {
            return Err(SecurityBlockReason::InsecurePermissions {
                path: dir.display().to_string(),
                detail: format!(
                    "directory owner uid={actual_uid} does not match euid={expected_uid}"
                ),
            });
        }
        let mode = meta.permissions().mode() & 0o777;
        let forbidden_mask = if require_strict_0700 { 0o077 } else { 0o022 };
        if (mode & forbidden_mask) != 0 {
            return Err(SecurityBlockReason::InsecurePermissions {
                path: dir.display().to_string(),
                detail: format!(
                    "directory mode {mode:#o} exposes unauthorized access (forbidden mask {forbidden_mask:#o})"
                ),
            });
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (expected_uid, require_strict_0700);
    }

    Ok(())
}

fn reject_if_symlink(path: &Path) -> Result<(), DimError> {
    if let Ok(meta) = fs::symlink_metadata(path) {
        if is_symlink_or_reparse(&meta) {
            return Err(DimError::PathTraversal(format!(
                "symlink or reparse point rejected inside Linux profile tree: {}",
                path.display()
            )));
        }
    }
    Ok(())
}

fn write_seed_file_if_absent(path: &Path, content: &[u8]) -> Result<(), DimError> {
    reject_if_symlink(path)?;
    if !path.exists() {
        fs::write(path, content)?;
        set_file_mode_0600(path)?;
    }
    Ok(())
}

fn set_dir_mode_0700(path: &Path) -> Result<(), DimError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

fn set_file_mode_0600(path: &Path) -> Result<(), DimError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

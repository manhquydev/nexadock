//! Hardened Linux `HOME` / XDG Base Directory profile layout, `user-dirs.dirs` confinement,
//! `0o700` permission enforcement, sanitized child environment builder, process lifetime policy,
//! and least-privilege `bubblewrap` (`bwrap`) sandbox planning.

use std::collections::hash_map::RandomState;
use std::collections::BTreeMap;
use std::fs;
use std::hash::{BuildHasher, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

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

/// Environment variable key tagging every managed process tree with its owning profile ID.
pub const DIM_PROFILE_ID_ENV: &str = "DIM_PROFILE_ID";

/// Environment variable key tagging every launched instance with a unique per-launch 128-bit nonce
/// so recycled PIDs after crashes or OS reboots can never impersonate a live profile instance.
pub const DIM_INSTANCE_NONCE_ENV: &str = "DIM_INSTANCE_NONCE";

/// Safe desktop/session environment variables forwarded across `Command::env_clear()` on Linux.
///
/// Dangerous dynamic-linker and runtime code-injection variables (`LD_PRELOAD`, `LD_AUDIT`,
/// `LD_LIBRARY_PATH`, `NODE_OPTIONS`, `ELECTRON_RUN_AS_NODE`, `APPDATA`, `USERPROFILE`,
/// `CFFIXED_USER_HOME`) are intentionally excluded.
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
    "XDG_SEAT",
    "XDG_VTNR",
    "DESKTOP_SESSION",
    "DBUS_SESSION_BUS_ADDRESS",
    "PULSE_SERVER",
    "PULSE_COOKIE",
    "PIPEWIRE_REMOTE",
    "GTK_USE_PORTAL",
    "GTK_THEME",
    "GTK_IM_MODULE",
    "QT_IM_MODULE",
    "XMODIFIERS",
    "LANG",
    "LANGUAGE",
    "LC_ALL",
    "LC_CTYPE",
    "LC_MESSAGES",
    "TZ",
    "TERM",
    "FIXTURE_ZALO_USER_ID",
];

static NONCE_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Controls how a spawned Linux desktop instance binds its OS session and `bubblewrap` (`bwrap`)
/// lifecycle to the invoking launcher process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LauncherLifetimePolicy {
    /// Default product policy: detaches the spawned child into a new POSIX session (`setsid`)
    /// and passes `--new-session` to `bwrap` **without** `--die-with-parent`.
    ///
    /// This guarantees that instances launched by short-lived CLI commands (`dim-cli launch <id>`)
    /// survive after the CLI process exits, while remaining strictly trackable and stoppable via
    /// `(pid, starttime_ticks)` in `<profile_dir>/.runtime-pid`, `DIM_PROFILE_ID`, and
    /// `DIM_INSTANCE_NONCE`.
    #[default]
    PersistentDetachedSession,
    /// Opt-in transient harness policy: keeps the child attached to the parent lifecycle and
    /// passes `--die-with-parent` (`PR_SET_PDEATHSIG(SIGKILL)`) to `bwrap` so sandboxed processes
    /// automatically self-terminate if the parent test harness exits or crashes.
    DieWithParent,
}

impl LauncherLifetimePolicy {
    /// Canonical machine-readable identifier for audit logs and `.runtime-pid`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PersistentDetachedSession => "persistent-detached-session",
            Self::DieWithParent => "die-with-parent",
        }
    }

    /// Returns `true` when the spawned child should call `setsid()` to detach from the launcher.
    #[must_use]
    pub const fn detaches_from_launcher(self) -> bool {
        matches!(self, Self::PersistentDetachedSession)
    }
}

/// Host desktop integration socket surfaces selectively mounted into a `bwrap` sandbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DesktopSocketSurface {
    /// Native Wayland compositor socket (`$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY`).
    WaylandCompositor,
    /// X11 / XWayland display socket (`/tmp/.X11-unix/X<n>` + optional `XAUTHORITY`).
    X11Server,
    /// Session D-Bus socket (`DBUS_SESSION_BUS_ADDRESS` / `$XDG_RUNTIME_DIR/bus`) used by
    /// `xdg-desktop-portal` file choosers and `org.freedesktop.Notifications`.
    SessionDbus,
    /// PipeWire multimedia daemon socket (`$XDG_RUNTIME_DIR/pipewire-0`).
    PipeWireDaemon,
    /// PulseAudio / `pipewire-pulse` audio socket (`$XDG_RUNTIME_DIR/pulse/native`).
    PulseAudioServer,
}

impl DesktopSocketSurface {
    /// Short lowercase identifier for diagnostics.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WaylandCompositor => "wayland",
            Self::X11Server => "x11",
            Self::SessionDbus => "dbus-session",
            Self::PipeWireDaemon => "pipewire",
            Self::PulseAudioServer => "pulseaudio",
        }
    }

    /// Returns `true` if exposing this host socket inherently weakens sandbox isolation compared
    /// to pure Wayland + `xdg-desktop-portal` + `xdg-dbus-proxy` mediation.
    #[must_use]
    pub const fn reduces_sandbox_isolation(self) -> bool {
        !matches!(self, Self::WaylandCompositor)
    }

    /// Structured security-tradeoff explanation for capability reports and sandbox diagnostics.
    #[must_use]
    pub const fn security_tradeoff_note(self) -> &'static str {
        match self {
            Self::WaylandCompositor => {
                "Wayland socket (`WAYLAND_DISPLAY`) enforces per-client surface isolation without global keylogging or cross-window input injection."
            }
            Self::X11Server => {
                "SANDBOX_TRADEOFF [x11]: Binding `/tmp/.X11-unix/X<n>` exposes the shared X11/XWayland server, which lacks per-client UI isolation (clients on the same X server can observe clipboard/keystrokes or synthesize X11 events)."
            }
            Self::SessionDbus => {
                "SANDBOX_TRADEOFF [dbus-session]: Binding the unfiltered user session D-Bus socket enables `xdg-desktop-portal` file choosers and `org.freedesktop.Notifications`, but exposes same-UID session D-Bus services unless filtered via `xdg-dbus-proxy`."
            }
            Self::PipeWireDaemon => {
                "SANDBOX_TRADEOFF [pipewire]: Binding `pipewire-0` directly enables low-latency audio/WebRTC, but exposes host PipeWire graph nodes owned by the user unless mediated by WirePlumber portal permissions."
            }
            Self::PulseAudioServer => {
                "SANDBOX_TRADEOFF [pulseaudio]: Binding `pulse/native` enables audio playback/capture, but shares same-UID PulseAudio/`pipewire-pulse` streams across instances."
            }
        }
    }
}

/// Configuration inputs for [`plan_bwrap_desktop_sandbox`].
#[derive(Debug, Clone)]
pub struct BwrapSandboxOptions<'a> {
    /// Process lifetime policy (`PersistentDetachedSession` vs `DieWithParent`).
    pub lifetime_policy: LauncherLifetimePolicy,
    /// Owning profile ID injected via `--setenv DIM_PROFILE_ID`.
    pub profile_id: Option<&'a str>,
    /// Unique per-launch nonce injected via `--setenv DIM_INSTANCE_NONCE`.
    pub instance_nonce: Option<&'a str>,
    /// When `true`, sets `GTK_USE_PORTAL=1` inside the sandbox (unless explicitly overridden in
    /// host environment) so file chooser dialogs route via `xdg-desktop-portal` without `$HOME` access.
    pub prefer_portal_file_chooser: bool,
    /// Additional validated read-only files required by the launch target (e.g. a Node fixture script
    /// path outside `/usr` and `profile_dir`).
    pub extra_ro_files: &'a [PathBuf],
}

impl Default for BwrapSandboxOptions<'_> {
    fn default() -> Self {
        Self {
            lifetime_policy: LauncherLifetimePolicy::PersistentDetachedSession,
            profile_id: None,
            instance_nonce: None,
            prefer_portal_file_chooser: true,
            extra_ro_files: &[],
        }
    }
}

/// Complete `bubblewrap` (`bwrap`) sandbox plan pairing minimal CLI arguments with explicit
/// bound socket paths and security-tradeoff diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BwrapSandboxPlan {
    /// Lifetime policy governing `--new-session` vs `--die-with-parent`.
    pub lifetime_policy: LauncherLifetimePolicy,
    /// Final `bwrap` CLI arguments (excluding `argv[0]` `"bwrap"`).
    pub args: Vec<String>,
    /// Exact host socket/auth files mounted into the sandbox.
    pub bound_endpoints: Vec<PathBuf>,
    /// Active desktop integration surfaces exposed inside the sandbox.
    pub exposed_surfaces: Vec<DesktopSocketSurface>,
    /// Explicit security-tradeoff notes describing any sandbox isolation reductions caused by
    /// shared X11, unfiltered session D-Bus, PipeWire, or PulseAudio sockets.
    pub security_tradeoff_notes: Vec<String>,
}

/// Generates a unique 32-character lowercase hexadecimal instance nonce (128 bits of entropy
/// from `/dev/urandom` on Unix, with OS-seeded `RandomState` + timestamp + counter fallback).
#[must_use]
pub fn generate_instance_nonce() -> String {
    let mut random_bytes = [0u8; 16];
    if let Ok(mut file) = fs::File::open("/dev/urandom") {
        if file.read_exact(&mut random_bytes).is_ok() && random_bytes.iter().any(|&b| b != 0) {
            let mut hex = String::with_capacity(32);
            for byte in random_bytes {
                use std::fmt::Write as _;
                let _ = write!(&mut hex, "{byte:02x}");
            }
            return hex;
        }
    }

    let seq = NONCE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();

    let mut h1 = RandomState::new().build_hasher();
    h1.write_u64(seq);
    h1.write_u128(nanos);
    h1.write_u32(pid);
    let hi = h1.finish();

    let mut h2 = RandomState::new().build_hasher();
    h2.write_u64(hi ^ 0x9e37_79b9_7f4a_7c15);
    h2.write_u64(seq.wrapping_mul(0x517c_c1b7_2722_0a95));
    h2.write_u32(pid.rotate_left(13));
    let lo = h2.finish();

    format!("{hi:016x}{lo:016x}")
}

/// Returns `<profile_dir>/.config/ZaloData`.
#[must_use]
pub fn linux_zalo_data_dir(profile_dir: &Path) -> PathBuf {
    profile_dir.join(".config").join("ZaloData")
}

/// Directory segment under `<profile_dir>/.local/share/` reserved for per-instance
/// `zalo-linux` community-repack `userData` directories (exported as `ZALO_DATA_DIR`).
pub const ZALO_LINUX_DATA_SEGMENT: &str = "zalo-linux";

/// Returns `<profile_dir>/.local/share/zalo-linux/<instance_slug>` after creating the
/// `zalo-linux` root and the instance directory with `0700` permissions.
///
/// Rejects path traversal, primary-`ZaloData` overlap, and symlink escapes. Existing instance
/// directories are reused with their mode re-tightened to `0700` (the `zalo-linux` wrapper's
/// own `mkdir -p` never tightens permissions, so NexaDock always pre-creates the directory).
pub fn ensure_zalo_linux_instance_data_dir(
    profile_dir: &Path,
    instance_slug: &str,
) -> Result<PathBuf, DimError> {
    validate_directory_input(profile_dir)?;
    ensure_not_primary_zalo_path(profile_dir)?;
    if instance_slug.is_empty()
        || instance_slug == "."
        || instance_slug == ".."
        || instance_slug.contains(['/', '\\'])
    {
        return Err(DimError::PathTraversal(format!(
            "invalid zalo-linux instance slug: {instance_slug}"
        )));
    }
    let instance_dir = zalo_linux_instance_data_dir_path(profile_dir, instance_slug);
    let segment_root = instance_dir
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            DimError::PathTraversal(format!("invalid instance slug: {instance_slug}"))
        })?;
    reject_if_symlink(&segment_root)?;
    reject_if_symlink(&instance_dir)?;
    fs::create_dir_all(&segment_root)?;
    set_dir_mode_0700(&segment_root)?;
    fs::create_dir_all(&instance_dir)?;
    set_dir_mode_0700(&instance_dir)?;
    Ok(instance_dir)
}

/// Returns `<profile_dir>/.local/share/zalo-linux/<instance_slug>` WITHOUT creating anything
/// (read-only companion for runtime inspection and lock detection).
#[must_use]
pub fn zalo_linux_instance_data_dir_path(profile_dir: &Path, instance_slug: &str) -> PathBuf {
    profile_dir
        .join(".local")
        .join("share")
        .join(ZALO_LINUX_DATA_SEGMENT)
        .join(instance_slug)
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
        "# Isolated NexaDock XDG user directories\n",
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
    env.insert(DIM_PROFILE_ID_ENV.to_owned(), profile.id.clone());
    env.insert("DIM_PROFILE_NAME".to_owned(), profile.name.clone());
    env
}

/// Builds a complete sanitized child process environment map suitable for `Command::env_clear()`,
/// forwarding only whitelisted session/display/audio/portal variables and stripping dangerous
/// loader/host overrides (`LD_PRELOAD`, `LD_AUDIT`, `NODE_OPTIONS`, `APPDATA`, `USERPROFILE`, `CFFIXED_USER_HOME`).
#[must_use]
pub fn build_sanitized_linux_spawn_env(
    profile: &ProfileRecord,
    profile_dir: &Path,
    host_env_lookup: &dyn Fn(&str) -> Option<String>,
) -> BTreeMap<String, String> {
    build_sanitized_linux_spawn_env_with_nonce(profile, profile_dir, None, false, host_env_lookup)
}

/// Builds a sanitized child process environment map with optional per-launch `DIM_INSTANCE_NONCE`
/// and `GTK_USE_PORTAL=1` desktop portal file-chooser preference.
#[must_use]
pub fn build_sanitized_linux_spawn_env_with_nonce(
    profile: &ProfileRecord,
    profile_dir: &Path,
    instance_nonce: Option<&str>,
    prefer_portal_file_chooser: bool,
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
    if prefer_portal_file_chooser && !env.contains_key("GTK_USE_PORTAL") {
        env.insert("GTK_USE_PORTAL".to_owned(), "1".to_owned());
    }
    for (k, v) in build_linux_isolated_env(profile, profile_dir) {
        env.insert(k, v);
    }
    if let Some(nonce) = instance_nonce.filter(|s| !s.is_empty() && !s.contains('\0')) {
        env.insert(DIM_INSTANCE_NONCE_ENV.to_owned(), nonce.to_owned());
    }
    env
}

/// Parses a `DBUS_SESSION_BUS_ADDRESS` string (e.g. `unix:path=/run/user/1000/bus` or
/// `unix:path=/tmp/dbus-session-xyz,guid=...`) into a validated filesystem socket path.
///
/// Explicitly rejects `unix:abstract=...`, `tcp:...`, NUL bytes, path traversal (`..`), broad
/// system directories (`/`, `/dev`, `/home`), and system D-Bus sockets (`/run/dbus/system_bus_socket`
/// and `/var/run/dbus/system_bus_socket`).
#[must_use]
pub fn parse_dbus_session_socket_path(dbus_address: &str) -> Option<PathBuf> {
    if dbus_address.contains('\0') {
        return None;
    }
    for transport in dbus_address.split(';') {
        let trimmed = transport.trim();
        let Some(rest) = trimmed.strip_prefix("unix:") else {
            continue;
        };
        let mut path_val: Option<String> = None;
        let mut is_abstract = false;
        for kv in rest.split(',') {
            let kv = kv.trim();
            if kv.starts_with("abstract=") {
                is_abstract = true;
                break;
            }
            if let Some(raw_path) = kv.strip_prefix("path=") {
                path_val = unescape_dbus_value(raw_path);
            }
        }
        if is_abstract {
            continue;
        }
        if let Some(decoded) = path_val {
            if is_safe_socket_file_path_str(&decoded, None) && !is_system_dbus_path(&decoded) {
                return Some(PathBuf::from(decoded));
            }
        }
    }
    None
}

/// Parses a local X11 `DISPLAY` string (`:0`, `:0.0`, `unix:1`, `unix:1.0`) into its exact
/// Unix domain socket path (`/tmp/.X11-unix/X<n>`).
///
/// Returns `None` for remote TCP displays (`localhost:10.0`, `host:0`), empty inputs, or malformed values.
#[must_use]
pub fn parse_x11_display_socket_path(display: &str) -> Option<PathBuf> {
    let trimmed = display.trim();
    if trimmed.is_empty()
        || trimmed.contains('\0')
        || trimmed.contains('/')
        || trimmed.contains('\\')
    {
        return None;
    }
    let (host_part, disp_part) = trimmed.split_once(':')?;
    if !host_part.is_empty() && host_part != "unix" {
        return None;
    }
    let disp_num_str = disp_part.split('.').next()?;
    if disp_num_str.is_empty() || !disp_num_str.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let disp_num = disp_num_str.parse::<u32>().ok()?;
    if disp_num > 65_535 {
        return None;
    }
    Some(PathBuf::from(format!("/tmp/.X11-unix/X{disp_num}")))
}

/// Resolves a `WAYLAND_DISPLAY` value into a validated Wayland compositor socket path.
///
/// Supports both standard socket filenames relative to `XDG_RUNTIME_DIR` (`wayland-0`) and
/// Wayland 1.15+ absolute socket paths (`/run/user/1000/wayland-1`), rejecting traversal and broad dirs.
#[must_use]
pub fn resolve_wayland_socket_path(
    wayland_display: &str,
    xdg_runtime_dir: Option<&str>,
) -> Option<PathBuf> {
    let wl = wayland_display.trim();
    if wl.is_empty() || wl.contains('\0') {
        return None;
    }
    if wl.starts_with('/') {
        if is_safe_socket_file_path_str(wl, xdg_runtime_dir) && !is_system_dbus_path(wl) {
            return Some(PathBuf::from(wl));
        }
        return None;
    }
    if !is_single_safe_filename(wl) {
        return None;
    }
    let rt = validate_xdg_runtime_dir_str(xdg_runtime_dir?)?;
    Some(PathBuf::from(join_posix_str(rt, wl)))
}

/// Resolves the PipeWire socket path from `PIPEWIRE_REMOTE` (if set) or `<XDG_RUNTIME_DIR>/pipewire-0`.
#[must_use]
pub fn resolve_pipewire_socket_path(
    pipewire_remote: Option<&str>,
    xdg_runtime_dir: Option<&str>,
) -> Option<PathBuf> {
    if let Some(raw) = pipewire_remote.map(str::trim).filter(|s| !s.is_empty()) {
        if raw.contains('\0') {
            return None;
        }
        if raw.starts_with('/') {
            if is_safe_socket_file_path_str(raw, xdg_runtime_dir) && !is_system_dbus_path(raw) {
                return Some(PathBuf::from(raw));
            }
            return None;
        }
        if !is_single_safe_filename(raw) {
            return None;
        }
        let rt = validate_xdg_runtime_dir_str(xdg_runtime_dir?)?;
        return Some(PathBuf::from(join_posix_str(rt, raw)));
    }
    let rt = validate_xdg_runtime_dir_str(xdg_runtime_dir?)?;
    Some(PathBuf::from(join_posix_str(rt, "pipewire-0")))
}

/// Resolves the PulseAudio / `pipewire-pulse` socket path from `PULSE_SERVER` (`unix:/path` or
/// `{machine-id}unix:/path`) or `<XDG_RUNTIME_DIR>/pulse/native`.
#[must_use]
pub fn resolve_pulse_socket_path(
    pulse_server: Option<&str>,
    xdg_runtime_dir: Option<&str>,
) -> Option<PathBuf> {
    if let Some(raw) = pulse_server.map(str::trim).filter(|s| !s.is_empty()) {
        if raw.contains('\0') {
            return None;
        }
        for entry in raw.split_whitespace() {
            let stripped = if entry.starts_with('{') {
                entry.split_once('}').map_or(entry, |(_, rest)| rest)
            } else {
                entry
            };
            if let Some(unix_path) = stripped.strip_prefix("unix:") {
                if is_safe_socket_file_path_str(unix_path, xdg_runtime_dir)
                    && !is_system_dbus_path(unix_path)
                {
                    return Some(PathBuf::from(unix_path));
                }
            }
        }
        return None;
    }
    let rt = validate_xdg_runtime_dir_str(xdg_runtime_dir?)?;
    let pulse_dir = join_posix_str(rt, "pulse");
    Some(PathBuf::from(join_posix_str(&pulse_dir, "native")))
}

/// Verifies on the live host filesystem that `path` exists and is a non-directory endpoint
/// (Unix domain socket, character device node for `/dev/fuse`, or regular auth file) rather than
/// a broad directory or broken link.
#[must_use]
pub fn is_safe_bindable_socket_or_file(path: &Path) -> bool {
    if path == Path::new("/dev/dri") {
        return fs::symlink_metadata(path).is_ok_and(|m| m.is_dir() && !is_symlink_or_reparse(&m));
    }
    let Ok(sym_meta) = fs::symlink_metadata(path) else {
        return false;
    };
    !(is_symlink_or_reparse(&sym_meta) || sym_meta.is_dir())
}

/// Builds a Bubblewrap (`bwrap`) sandbox argument vector binding `profile_dir` as `$HOME`
/// while hiding the host user's real home directory and sibling profiles.
///
/// Uses [`LauncherLifetimePolicy::PersistentDetachedSession`] (`--new-session` without
/// `--die-with-parent`) by default so launched instances survive short-lived CLI invocations.
#[must_use]
pub fn build_bwrap_args(profile_dir: &Path, executable: &Path, app_args: &[String]) -> Vec<String> {
    let opts = BwrapSandboxOptions::default();
    let probe: &dyn Fn(&Path) -> bool = if cfg!(target_os = "linux") {
        &is_safe_bindable_socket_or_file
    } else {
        &|_| true
    };
    plan_bwrap_desktop_sandbox(
        profile_dir,
        executable,
        app_args,
        &opts,
        &|k| std::env::var(k).ok(),
        probe,
    )
    .args
}

/// Parameterized Bubblewrap (`bwrap`) argument builder with selective Wayland, X11, PipeWire,
/// PulseAudio, and D-Bus session socket passthrough under [`LauncherLifetimePolicy::PersistentDetachedSession`].
#[must_use]
pub fn build_bwrap_desktop_args(
    profile_dir: &Path,
    executable: &Path,
    app_args: &[String],
    host_env_lookup: &dyn Fn(&str) -> Option<String>,
) -> Vec<String> {
    let opts = BwrapSandboxOptions::default();
    plan_bwrap_desktop_sandbox(
        profile_dir,
        executable,
        app_args,
        &opts,
        host_env_lookup,
        &|_| true,
    )
    .args
}

/// Parameterized Bubblewrap (`bwrap`) argument builder with explicit [`LauncherLifetimePolicy`],
/// `DIM_PROFILE_ID`, and `DIM_INSTANCE_NONCE` injection.
#[must_use]
pub fn build_bwrap_desktop_args_with_policy(
    profile_dir: &Path,
    executable: &Path,
    app_args: &[String],
    lifetime_policy: LauncherLifetimePolicy,
    profile_id: Option<&str>,
    instance_nonce: Option<&str>,
    host_env_lookup: &dyn Fn(&str) -> Option<String>,
) -> Vec<String> {
    let opts = BwrapSandboxOptions {
        lifetime_policy,
        profile_id,
        instance_nonce,
        prefer_portal_file_chooser: true,
        extra_ro_files: &[],
    };
    plan_bwrap_desktop_sandbox(
        profile_dir,
        executable,
        app_args,
        &opts,
        host_env_lookup,
        &|_| true,
    )
    .args
}

/// Plans a least-privilege `bubblewrap` (`bwrap`) desktop sandbox, binding ONLY specific validated
/// socket endpoints that exist and recording structured security-tradeoff notes whenever X11,
/// unfiltered session D-Bus, PipeWire, or PulseAudio sockets are exposed.
///
/// Strictly enforces:
/// - NEVER binds full `$HOME`, full `/dev`, full `$XDG_RUNTIME_DIR`, or system D-Bus (`/run/dbus/system_bus_socket`).
/// - Mounts an isolated `0700` tmpfs at `$XDG_RUNTIME_DIR` containing only individual validated sockets.
/// - Mounts a private tmpfs `/tmp` with only `/tmp/.X11-unix/X<n>` bound when `DISPLAY` is active.
#[must_use]
pub fn plan_bwrap_desktop_sandbox(
    profile_dir: &Path,
    executable: &Path,
    app_args: &[String],
    options: &BwrapSandboxOptions<'_>,
    host_env_lookup: &dyn Fn(&str) -> Option<String>,
    endpoint_exists: &dyn Fn(&Path) -> bool,
) -> BwrapSandboxPlan {
    let pdir_str = profile_dir.to_string_lossy().into_owned();
    let exe_str = executable.to_string_lossy().into_owned();
    let real_home = host_env_lookup("HOME")
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());

    let lifecycle_flag = match options.lifetime_policy {
        LauncherLifetimePolicy::PersistentDetachedSession => "--new-session",
        LauncherLifetimePolicy::DieWithParent => "--die-with-parent",
    };

    let mut args = vec![
        lifecycle_flag.to_owned(),
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
        "/etc/ld.so.cache".to_owned(),
        "/etc/ld.so.cache".to_owned(),
        "--ro-bind-try".to_owned(),
        "/etc/ld.so.conf".to_owned(),
        "/etc/ld.so.conf".to_owned(),
        "--ro-bind-try".to_owned(),
        "/etc/ld.so.conf.d".to_owned(),
        "/etc/ld.so.conf.d".to_owned(),
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
        "/etc/hosts".to_owned(),
        "/etc/hosts".to_owned(),
        "--ro-bind-try".to_owned(),
        "/etc/nsswitch.conf".to_owned(),
        "/etc/nsswitch.conf".to_owned(),
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
    ];

    if endpoint_exists(Path::new("/dev/dri")) {
        args.extend([
            "--dev-bind-try".to_owned(),
            "/dev/dri".to_owned(),
            "/dev/dri".to_owned(),
        ]);
    }
    if looks_like_appimage(executable) && endpoint_exists(Path::new("/dev/fuse")) {
        args.extend([
            "--dev-bind-try".to_owned(),
            "/dev/fuse".to_owned(),
            "/dev/fuse".to_owned(),
        ]);
    }

    args.extend(["--tmpfs".to_owned(), "/tmp".to_owned()]);

    let mut bound_endpoints = Vec::new();
    let mut exposed_surfaces = Vec::new();

    let raw_runtime_dir = host_env_lookup("XDG_RUNTIME_DIR");
    let valid_runtime_dir = raw_runtime_dir
        .as_deref()
        .and_then(validate_xdg_runtime_dir_str)
        .filter(|rt| real_home.as_deref() != Some(*rt));

    if let Some(rt) = valid_runtime_dir {
        // Create an empty 0700 tmpfs directory at XDG_RUNTIME_DIR inside the mount namespace
        // instead of binding the host XDG_RUNTIME_DIR (hiding keyring, gnupg, ssh-agent, systemd).
        args.extend([
            "--dir".to_owned(),
            rt.to_owned(),
            "--chmod".to_owned(),
            "0700".to_owned(),
            rt.to_owned(),
        ]);
    }

    // 1. Wayland socket (`WAYLAND_DISPLAY`).
    if let Some(wl_raw) = host_env_lookup("WAYLAND_DISPLAY") {
        if let Some(wl_sock) = resolve_wayland_socket_path(&wl_raw, valid_runtime_dir) {
            if !path_equals_opt_str(&wl_sock, real_home.as_deref()) && endpoint_exists(&wl_sock) {
                push_ro_bind_try(&mut args, &wl_sock);
                bound_endpoints.push(wl_sock);
                push_unique_surface(
                    &mut exposed_surfaces,
                    DesktopSocketSurface::WaylandCompositor,
                );
            }
        }
    }

    // 2. X11 socket (`/tmp/.X11-unix/X<n>`) + `XAUTHORITY` (ONLY when `DISPLAY` is active).
    if let Some(display_raw) = host_env_lookup("DISPLAY") {
        if let Some(x11_sock) = parse_x11_display_socket_path(&display_raw) {
            if endpoint_exists(&x11_sock) {
                args.extend(["--dir".to_owned(), "/tmp/.X11-unix".to_owned()]);
                push_ro_bind_try(&mut args, &x11_sock);
                bound_endpoints.push(x11_sock);
                push_unique_surface(&mut exposed_surfaces, DesktopSocketSurface::X11Server);

                if let Some(xauth_raw) = host_env_lookup("XAUTHORITY") {
                    let xa = xauth_raw.trim();
                    if is_safe_socket_file_path_str(xa, valid_runtime_dir)
                        && !is_system_dbus_path(xa)
                        && real_home.as_deref() != Some(xa)
                    {
                        let xa_path = PathBuf::from(xa);
                        if endpoint_exists(&xa_path) {
                            push_ro_bind_try(&mut args, &xa_path);
                            bound_endpoints.push(xa_path);
                        }
                    }
                }
            }
        }
    }

    // 3. Session D-Bus socket (`DBUS_SESSION_BUS_ADDRESS` `unix:path=...`, even outside `$XDG_RUNTIME_DIR/bus`,
    //    falling back to `<XDG_RUNTIME_DIR>/bus`; strictly rejecting system D-Bus!).
    let explicit_dbus_env = host_env_lookup("DBUS_SESSION_BUS_ADDRESS");
    let resolved_dbus_sock = if let Some(ref dbus_raw) = explicit_dbus_env {
        parse_dbus_session_socket_path(dbus_raw)
    } else {
        valid_runtime_dir.map(|rt| PathBuf::from(join_posix_str(rt, "bus")))
    };
    let mut synthesized_dbus_addr: Option<String> = None;
    if let Some(dbus_sock) = resolved_dbus_sock {
        if !path_equals_opt_str(&dbus_sock, real_home.as_deref()) && endpoint_exists(&dbus_sock) {
            push_ro_bind_try(&mut args, &dbus_sock);
            if explicit_dbus_env.is_none() {
                synthesized_dbus_addr = Some(format!("unix:path={}", dbus_sock.to_string_lossy()));
            }
            bound_endpoints.push(dbus_sock);
            push_unique_surface(&mut exposed_surfaces, DesktopSocketSurface::SessionDbus);
        }
    }

    // 4. PipeWire socket (`PIPEWIRE_REMOTE` or `<XDG_RUNTIME_DIR>/pipewire-0`).
    let pw_remote = host_env_lookup("PIPEWIRE_REMOTE");
    if let Some(pw_sock) = resolve_pipewire_socket_path(pw_remote.as_deref(), valid_runtime_dir) {
        if !path_equals_opt_str(&pw_sock, real_home.as_deref()) && endpoint_exists(&pw_sock) {
            push_ro_bind_try(&mut args, &pw_sock);
            bound_endpoints.push(pw_sock);
            push_unique_surface(&mut exposed_surfaces, DesktopSocketSurface::PipeWireDaemon);
        }
    }

    // 5. PulseAudio / `pipewire-pulse` socket (`PULSE_SERVER` or `<XDG_RUNTIME_DIR>/pulse/native`).
    let pulse_srv = host_env_lookup("PULSE_SERVER");
    if let Some(pulse_sock) = resolve_pulse_socket_path(pulse_srv.as_deref(), valid_runtime_dir) {
        if !path_equals_opt_str(&pulse_sock, real_home.as_deref()) && endpoint_exists(&pulse_sock) {
            if let Some(rt) = valid_runtime_dir {
                let pulse_dir = join_posix_str(rt, "pulse");
                if pulse_sock.to_string_lossy().starts_with(&pulse_dir) {
                    args.extend([
                        "--dir".to_owned(),
                        pulse_dir.clone(),
                        "--chmod".to_owned(),
                        "0700".to_owned(),
                        pulse_dir,
                    ]);
                }
            }
            push_ro_bind_try(&mut args, &pulse_sock);
            bound_endpoints.push(pulse_sock);
            push_unique_surface(
                &mut exposed_surfaces,
                DesktopSocketSurface::PulseAudioServer,
            );
        }
    }

    // Bind ONLY this profile's isolated directory read-write (never real `$HOME` or sibling profiles).
    args.extend(["--bind".to_owned(), pdir_str.clone(), pdir_str.clone()]);

    // Bind executable read-only if it resides outside `/usr`, `/bin`, and `profile_dir`
    // (e.g., `/home/<user>/Applications/Zalo.AppImage` or `/opt/zalo/zalo`), without exposing real `$HOME`.
    if should_bind_external_readonly_file(executable, profile_dir, real_home.as_deref()) {
        args.extend(["--ro-bind".to_owned(), exe_str.clone(), exe_str.clone()]);
    }

    for extra_file in options.extra_ro_files {
        if should_bind_external_readonly_file(extra_file, profile_dir, real_home.as_deref()) {
            let extra_str = extra_file.to_string_lossy().into_owned();
            args.extend(["--ro-bind".to_owned(), extra_str.clone(), extra_str]);
        }
    }

    let tmp_str = profile_dir.join("tmp").to_string_lossy().into_owned();
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
        "XDG_DOWNLOAD_DIR".to_owned(),
        profile_dir.join("Downloads").to_string_lossy().into_owned(),
        "--setenv".to_owned(),
        "XDG_DOCUMENTS_DIR".to_owned(),
        profile_dir.join("Documents").to_string_lossy().into_owned(),
        "--setenv".to_owned(),
        "TMPDIR".to_owned(),
        tmp_str.clone(),
        "--setenv".to_owned(),
        "TEMP".to_owned(),
        tmp_str.clone(),
        "--setenv".to_owned(),
        "TMP".to_owned(),
        tmp_str,
    ]);

    if let Some(dbus_addr) = synthesized_dbus_addr {
        args.extend([
            "--setenv".to_owned(),
            "DBUS_SESSION_BUS_ADDRESS".to_owned(),
            dbus_addr,
        ]);
    }

    if options.prefer_portal_file_chooser
        && host_env_lookup("GTK_USE_PORTAL")
            .filter(|s| !s.trim().is_empty())
            .is_none()
    {
        args.extend([
            "--setenv".to_owned(),
            "GTK_USE_PORTAL".to_owned(),
            "1".to_owned(),
        ]);
    }

    if let Some(pid_id) = options
        .profile_id
        .filter(|s| !s.is_empty() && !s.contains('\0'))
    {
        args.extend([
            "--setenv".to_owned(),
            DIM_PROFILE_ID_ENV.to_owned(),
            pid_id.to_owned(),
        ]);
    }
    if let Some(nonce) = options
        .instance_nonce
        .filter(|s| !s.is_empty() && !s.contains('\0'))
    {
        args.extend([
            "--setenv".to_owned(),
            DIM_INSTANCE_NONCE_ENV.to_owned(),
            nonce.to_owned(),
        ]);
    }

    args.push("--".to_owned());
    args.push(exe_str);
    args.extend(app_args.iter().cloned());

    let security_tradeoff_notes = exposed_surfaces
        .iter()
        .copied()
        .filter(|s| s.reduces_sandbox_isolation())
        .map(|s| s.security_tradeoff_note().to_owned())
        .collect();

    BwrapSandboxPlan {
        lifetime_policy: options.lifetime_policy,
        args,
        bound_endpoints,
        exposed_surfaces,
        security_tradeoff_notes,
    }
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

fn push_ro_bind_try(args: &mut Vec<String>, path: &Path) {
    let s = path.to_string_lossy().into_owned();
    args.push("--ro-bind-try".to_owned());
    args.push(s.clone());
    args.push(s);
}

fn push_unique_surface(list: &mut Vec<DesktopSocketSurface>, surface: DesktopSocketSurface) {
    if !list.contains(&surface) {
        list.push(surface);
    }
}

fn path_equals_opt_str(path: &Path, maybe_dir: Option<&str>) -> bool {
    let Some(dir) = maybe_dir else {
        return false;
    };
    let p_norm = path.to_string_lossy();
    p_norm.trim_end_matches('/') == dir.trim_end_matches('/')
}

fn should_bind_external_readonly_file(
    file: &Path,
    profile_dir: &Path,
    real_home: Option<&str>,
) -> bool {
    let file_str = file.to_string_lossy();
    let is_abs = file_str.starts_with('/') || file.is_absolute();
    if !is_abs || file_str.contains('\0') {
        return false;
    }
    if has_dot_or_dotdot_segment(&file_str) {
        return false;
    }
    if is_broad_forbidden_bind_dir(&file_str) || path_equals_opt_str(file, real_home) {
        return false;
    }
    !file_str.starts_with("/usr/")
        && file_str != "/usr"
        && !file_str.starts_with("/bin/")
        && file_str != "/bin"
        && !file.starts_with(profile_dir)
}

fn validate_xdg_runtime_dir_str(raw: &str) -> Option<&str> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() || !trimmed.starts_with('/') || trimmed.contains('\0') {
        return None;
    }
    if has_dot_or_dotdot_segment(trimmed)
        || is_broad_forbidden_bind_dir(trimmed)
        || is_system_dbus_path(trimmed)
    {
        return None;
    }
    Some(trimmed)
}

fn is_safe_socket_file_path_str(raw: &str, xdg_runtime_dir: Option<&str>) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || !trimmed.starts_with('/')
        || trimmed.ends_with('/')
        || trimmed.contains('\0')
    {
        return false;
    }
    if has_dot_or_dotdot_segment(trimmed) || is_broad_forbidden_bind_dir(trimmed) {
        return false;
    }
    if let Some(rt) = xdg_runtime_dir {
        if trimmed == rt.trim_end_matches('/') {
            return false;
        }
    }
    // Must have at least two path components (e.g. `/tmp/sock`, never `/sock` or `/`).
    trimmed.split('/').filter(|s| !s.is_empty()).count() >= 2
}

fn is_broad_forbidden_bind_dir(trimmed_path: &str) -> bool {
    let p = trimmed_path.trim_end_matches('/');
    matches!(
        p,
        "" | "/"
            | "/dev"
            | "/proc"
            | "/sys"
            | "/etc"
            | "/home"
            | "/root"
            | "/run"
            | "/var"
            | "/var/run"
            | "/tmp"
            | "/tmp/.X11-unix"
    ) || p.starts_with("/dev/")
        || p.starts_with("/proc/")
        || p.starts_with("/sys/")
}

fn is_system_dbus_path(raw: &str) -> bool {
    let p = raw.trim().trim_end_matches('/');
    p == "/run/dbus"
        || p.starts_with("/run/dbus/")
        || p == "/var/run/dbus"
        || p.starts_with("/var/run/dbus/")
}

fn is_single_safe_filename(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains('\0')
}

fn has_dot_or_dotdot_segment(path_str: &str) -> bool {
    path_str
        .split(['/', '\\'])
        .any(|seg| seg == "." || seg == "..")
}

fn join_posix_str(base: &str, child: &str) -> String {
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        child.trim_start_matches('/')
    )
}

fn unescape_dbus_value(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return None;
            }
            let hi = decode_hex_nibble(bytes[i + 1])?;
            let lo = decode_hex_nibble(bytes[i + 2])?;
            let val = (hi << 4) | lo;
            if val == 0 {
                return None;
            }
            out.push(val);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

const fn decode_hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

pub(crate) fn reject_if_symlink(path: &Path) -> Result<(), DimError> {
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

pub(crate) fn write_atomic_private_file(path: &Path, content: &[u8]) -> Result<(), DimError> {
    reject_if_symlink(path)?;
    let tmp_name = format!(
        ".{}.tmp.{}",
        path.file_name().and_then(|s| s.to_str()).unwrap_or("state"),
        std::process::id()
    );
    let tmp_path = path.with_file_name(tmp_name);
    reject_if_symlink(&tmp_path)?;
    fs::write(&tmp_path, content)?;
    set_file_mode_0600(&tmp_path)?;
    fs::rename(&tmp_path, path)?;
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

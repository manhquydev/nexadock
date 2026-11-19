//! Domain data models and schemas shared across storage, process management, and adapters.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::error::DimError;

/// Current version of `profiles.json`.
pub const PROFILES_SCHEMA_VERSION: u32 = 1;

/// Current version of `backup.json` manifests.
pub const BACKUP_SCHEMA_VERSION: u32 = 1;

/// Minimum free disk threshold required before creating profiles or backups (5 GiB).
pub const MIN_FREE_DISK_BYTES: u64 = 5 * 1024 * 1024 * 1024;

/// Hidden marker file stored inside each profile's `ZaloData` directory to bind identity.
pub const ZALO_PROFILE_ID_FILE: &str = ".zalo-profile-id";

/// Cross-process advisory lock file protecting `<data_dir>/profiles.json` read-modify-write cycles.
pub const PROFILES_LOCK_FILE: &str = "profiles.lock";

/// Subdirectory inside `<data_dir>` storing per-profile and admission advisory lockfiles
/// (`<data_dir>/locks/<profile_id>.lock`), outside `<data_dir>/Profiles` so lock acquisition
/// never resurrects a deleted profile directory or blocks `RemoveDirectoryW` with `ERROR_DIR_NOT_EMPTY`.
pub const PROFILE_LOCKS_DIR: &str = "locks";

/// Cross-process advisory lock file inside `<data_dir>/locks` protecting pre-spawn admission checks.
pub const LAUNCH_ADMISSION_LOCK_FILE: &str = "_launch_admission.lock";

/// Legacy per-profile advisory lock file name checked non-blockingly if already present on disk.
pub const PROFILE_OP_LOCK_FILE: &str = ".dim-profile.lock";

/// Atomic runtime process ownership lease file inside `<profile_dir>`.
pub const RUNTIME_LEASE_FILE: &str = ".runtime-lease.json";

/// Legacy compatibility PID hint file inside `<profile_dir>`.
pub const RUNTIME_PID_FILE: &str = ".runtime-pid";

/// Environment variable overriding the default application data root.
pub const DIM_DATA_DIR_ENV: &str = "DIM_DATA_DIR";

/// Default application directory name under the OS data root.
pub const APP_DIR_NAME: &str = "MultiZaloDesktop";

/// Supported target desktop platforms.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PlatformKind {
    /// Microsoft Windows 10/11.
    Windows,
    /// Apple macOS.
    MacOs,
    /// Linux desktop environments.
    Linux,
}

impl PlatformKind {
    /// Detects the compile-time host platform.
    #[must_use]
    pub const fn current() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }

    /// Returns lowercase canonical identifier string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::MacOs => "macos",
            Self::Linux => "linux",
        }
    }
}

/// Honest feature and platform support matrix status.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SupportStatus {
    /// Verified and tested on target OS.
    SupportedAndTested,
    /// Core flows work with documented feature limitations (e.g., concurrent calls blocked).
    PartiallySupported,
    /// Supported only for local fixtures or user-supplied local bundles.
    Experimental,
    /// Blocked by an external OS or application constraint.
    Blocked,
    /// Not supported on this platform.
    Unsupported,
    /// Architecture implemented but requires verification on target hardware.
    NotTested,
}

/// Single profile entry in `profiles.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileRecord {
    /// 8-character lowercase hexadecimal identifier.
    pub id: String,
    /// User-facing display name.
    pub name: String,
    /// Sanitized folder name `<slug>_<8hex>` under `<data_dir>/Profiles/`.
    pub directory_name: String,
    /// Creation timestamp formatted as RFC 3339 UTC.
    pub created_at: String,
    /// Timestamp of most recent launch formatted as RFC 3339 UTC, or `null`.
    #[serde(default)]
    pub last_launched_at: Option<String>,
    /// Detected numeric Zalo user ID (`Database/_production/<6+ digits>`), or `null`.
    #[serde(default)]
    pub zalo_user_id: Option<String>,
    /// Optional user notes.
    #[serde(default)]
    pub notes: String,
}

/// Root document persisted in `<data_dir>/profiles.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfilesDocument {
    /// Schema format version (must equal [`PROFILES_SCHEMA_VERSION`]).
    pub schema_version: u32,
    /// Ordered list of managed profiles.
    pub profiles: Vec<ProfileRecord>,
}

impl Default for ProfilesDocument {
    fn default() -> Self {
        Self {
            schema_version: PROFILES_SCHEMA_VERSION,
            profiles: Vec::new(),
        }
    }
}

/// Discovered target desktop application installation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApplicationInstallation {
    /// Whether a valid runnable application was discovered.
    pub is_valid: bool,
    /// Platform this installation applies to.
    pub platform: PlatformKind,
    /// Root installation path (e.g., `%LOCALAPPDATA%\Programs\Zalo` or `/Applications/Zalo.app`).
    pub install_root: PathBuf,
    /// Optional stub launcher executable.
    pub launcher_path: Option<PathBuf>,
    /// Primary Electron/runtime executable or bundle path.
    pub executable_path: Option<PathBuf>,
    /// Located `app.asar` bundle path when applicable.
    pub app_asar_path: Option<PathBuf>,
    /// Parsed semantic version string.
    pub version: Option<String>,
    /// Authenticode / code-signature status summary.
    pub signature_status: String,
    /// Signer subject CN if present.
    pub signer_subject: Option<String>,
    /// Support classification for this installation.
    pub support_status: SupportStatus,
    /// Informative notes or warnings.
    pub notes: Vec<String>,
    /// Diagnostic error if `is_valid` is `false`.
    pub error: Option<String>,
}

/// Target application invocation descriptor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LaunchTarget {
    /// Official VNG Zalo Desktop installation.
    OfficialZalo {
        /// Optional override directory instead of standard OS install path.
        custom_install_root: Option<PathBuf>,
    },
    /// User-supplied local executable or AppImage (e.g., Linux AppImage or compiled fixture).
    CustomExecutable {
        /// Absolute path to executable.
        path: PathBuf,
        /// Additional CLI arguments.
        args: Vec<String>,
    },
    /// Node.js test fixture (`fixture-app.mjs`) executed via signed `node`.
    NodeFixture {
        /// Optional path to `node` binary (defaults to `"node"` in `PATH`).
        node_path: Option<PathBuf>,
        /// Absolute path to `fixture-app.mjs`.
        script_path: PathBuf,
        /// Additional CLI arguments passed to `fixture-app.mjs`.
        extra_args: Vec<String>,
    },
}

/// Observed runtime state of an isolated profile instance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileRuntimeState {
    /// Profile 8-hex identifier.
    pub profile_id: String,
    /// Profile display name.
    pub profile_name: String,
    /// Isolated profile root directory (`<data_dir>/Profiles/<directory_name>`).
    pub profile_dir: PathBuf,
    /// Isolated `ZaloData` path inside the profile directory.
    pub zalo_data_dir: PathBuf,
    /// Path to Chromium single-instance lockfile (`lockfile` or `SingletonLock`).
    pub lockfile_path: PathBuf,
    /// True if an active owning process is verified alive.
    pub running: bool,
    /// True if lockfile/signal remnants exist on disk without an alive process.
    pub stale_lock_detected: bool,
    /// Main process PID if running.
    pub main_pid: Option<u32>,
    /// All descendant process IDs belonging to this profile's process tree.
    pub process_ids: Vec<u32>,
    /// Number of active processes in this profile's process tree.
    pub process_count: usize,
    /// Aggregate working-set memory in bytes across active profile processes.
    pub working_set_bytes: u64,
    /// Native window handle if detected on Windows.
    pub window_handle: Option<u64>,
    /// Visible window title if detected.
    pub window_title: Option<String>,
    /// Detected numeric Zalo user ID (`Database/_production/<6+ digits>`).
    pub detected_zalo_user_id: Option<String>,
    /// Total bytes occupied on disk by the profile directory.
    pub disk_usage_bytes: u64,
    /// Human-readable status summary.
    pub status_label: String,
}

/// Explicit lifecycle state of an isolated profile runtime.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProfileLifecycleState {
    /// No live owning process and no indeterminate ownership lock.
    #[default]
    Stopped,
    /// Process has been spawned and verified via `.runtime-lease.json`, awaiting lockfile readiness.
    Starting,
    /// Process is live and holds/wrote its isolated profile lockfile.
    Running,
    /// Graceful or escalated termination is actively in progress for this profile.
    Stopping,
    /// Indeterminate or corrupted runtime artifacts require recovery before destructive operations.
    RecoveryRequired,
}

impl ProfileLifecycleState {
    /// Machine-readable snake_case identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Stopping => "stopping",
            Self::RecoveryRequired => "recovery_required",
        }
    }

    /// Returns `true` when an active OS process lifecycle (`Starting`, `Running`, or `Stopping`) is present.
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Starting | Self::Running | Self::Stopping)
    }

    /// Human-readable status summary label.
    #[must_use]
    pub const fn default_status_label(self, stale_lock_detected: bool) -> &'static str {
        match self {
            Self::Stopped => {
                if stale_lock_detected {
                    "Stopped (Stale Lock)"
                } else {
                    "Stopped"
                }
            }
            Self::Starting => "Starting",
            Self::Running => "Running",
            Self::Stopping => "Stopping",
            Self::RecoveryRequired => "Recovery Required",
        }
    }
}

/// Runtime ownership classification evaluated prior to destructive operations (`delete_profile`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeOwnershipState {
    /// Verified no live process owns or locks the profile.
    ConfirmedStopped,
    /// Profile process is currently in pre-lock startup (`Starting`).
    Starting,
    /// Profile process is actively running (`Running`).
    Running,
    /// Profile process is currently shutting down (`Stopping`).
    Stopping,
    /// Runtime ownership cannot be proven stopped (`RecoveryRequired` / indeterminate lock or marker).
    Unknown,
}

/// Explicit runtime verification proof evaluated before deleting a profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifiedDeletionState {
    /// Runtime monitor confirmed the profile is stopped and has no indeterminate ownership.
    ConfirmedStopped,
    /// Runtime monitor observed an active or indeterminate state that MUST block deletion.
    NotSafeToDelete(RuntimeOwnershipState),
}

impl VerifiedDeletionState {
    /// Derives deletion safety proof from an inspected [`ProfileRuntimeState`].
    #[must_use]
    pub fn from_runtime_state(state: &ProfileRuntimeState) -> Self {
        match state.lifecycle_state() {
            ProfileLifecycleState::Starting => {
                Self::NotSafeToDelete(RuntimeOwnershipState::Starting)
            }
            ProfileLifecycleState::Running => Self::NotSafeToDelete(RuntimeOwnershipState::Running),
            ProfileLifecycleState::Stopping => {
                Self::NotSafeToDelete(RuntimeOwnershipState::Stopping)
            }
            ProfileLifecycleState::RecoveryRequired => {
                Self::NotSafeToDelete(RuntimeOwnershipState::Unknown)
            }
            ProfileLifecycleState::Stopped => {
                if state.running {
                    Self::NotSafeToDelete(RuntimeOwnershipState::Running)
                } else {
                    Self::ConfirmedStopped
                }
            }
        }
    }
}

impl ProfileRuntimeState {
    /// Derives the explicit [`ProfileLifecycleState`] without breaking existing struct literals.
    #[must_use]
    pub fn lifecycle_state(&self) -> ProfileLifecycleState {
        if self.status_label.eq_ignore_ascii_case("starting") {
            ProfileLifecycleState::Starting
        } else if self.status_label.eq_ignore_ascii_case("stopping") {
            ProfileLifecycleState::Stopping
        } else if self.status_label.eq_ignore_ascii_case("recovery required")
            || self.status_label.eq_ignore_ascii_case("recovery_required")
        {
            ProfileLifecycleState::RecoveryRequired
        } else if self.running {
            ProfileLifecycleState::Running
        } else {
            ProfileLifecycleState::Stopped
        }
    }

    /// Updates `running` and `status_label` atomically so `lifecycle_state()` and `running` never drift.
    pub fn set_lifecycle_state(&mut self, state: ProfileLifecycleState) {
        self.running = state.is_active();
        self.status_label = state
            .default_status_label(self.stale_lock_detected)
            .to_owned();
    }
}

/// Alias for [`ProfileRuntimeState`] exposing explicit [`ProfileLifecycleState`].
pub type RuntimeStatus = ProfileRuntimeState;

/// Observed window visibility state of the untouched primary Zalo Desktop installation.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PrimaryWindowState {
    /// A visible, non-minimized main window was detected.
    Visible,
    /// A minimized (`IsIconic`) main window was detected.
    Minimized,
    /// Primary process is running, but no visible/minimized main window was found (hidden in tray).
    HiddenInTray,
    /// Primary process is not running or no window state is available.
    #[default]
    None,
}

/// Account kind in the unified KHOANG workspace rail.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AccountKind {
    /// Untouched primary Zalo Desktop installation (strictly read-only).
    DesktopPrimary,
    /// Managed isolated desktop Zalo profile (`Profiles/<slug>_<id>/`).
    DesktopIsolated,
    /// Isolated web session (`WebSessions/<id>/`).
    WebIsolated,
}

/// Outcome of a unified account activation request.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActivationOutcome {
    /// Existing visible window was brought to focus.
    Focused,
    /// Minimized or tray-hidden window was restored and focused.
    Restored,
    /// Stopped profile was launched.
    Launched,
    /// Activation is already in flight and coalesced.
    Pending,
    /// User must perform a manual action (e.g. primary Zalo is hidden in tray or not running).
    ManualActionRequired,
    /// Activation was blocked by a safety guard (e.g. recovery required, duplicate UID).
    Blocked,
    /// Activation failed due to an unexpected error.
    Failed,
}

/// Structured remediation guidance when activation is blocked or requires manual action.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActionableRemediation {
    /// Machine-readable remediation code (e.g. `PRIMARY_HIDDEN_IN_TRAY`, `RECOVERY_REQUIRED`).
    pub code: String,
    /// Human-readable explanation of the issue.
    pub message: String,
    /// Optional suggested next step for the user or UI.
    #[serde(default)]
    pub suggested_action: Option<String>,
}

/// Result of activating an account in the unified workspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountActivationResult {
    /// Account identifier (`"primary"` or profile/session ID).
    pub account_id: String,
    /// Kind of account activated.
    pub account_kind: AccountKind,
    /// Outcome classification.
    pub outcome: ActivationOutcome,
    /// Activation mechanism used (e.g. `"direct_focus"`, `"direct_restore"`, `"tray_handoff"`, `"launch"`).
    #[serde(default)]
    pub method: Option<String>,
    /// Whether OS foreground ownership was confirmed after activation.
    #[serde(default)]
    pub os_foreground_confirmed: Option<bool>,
    /// Native window handle if applicable.
    #[serde(default)]
    pub window_handle: Option<u64>,
    /// Window title if applicable.
    #[serde(default)]
    pub window_title: Option<String>,
    /// Human-readable status or error message.
    pub message: String,
    /// Optional structured remediation when blocked or manual action is required.
    #[serde(default)]
    pub remediation: Option<ActionableRemediation>,
}

/// Observed foreground and visibility state of a native window handle after an activation attempt.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct WindowActivationState {
    /// True if the target window currently owns OS foreground focus.
    pub is_foreground: bool,
    /// True if a different, non-null foreign window currently holds OS foreground focus.
    pub foreign_foreground_present: bool,
    /// True if the target window is visible (`IsWindowVisible`).
    pub is_visible: bool,
    /// True if the target window is still minimized (`IsIconic`).
    pub is_minimized: bool,
}

/// Screen-space rectangle in physical pixels.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct WindowRect {
    /// Left edge X coordinate.
    pub left: i32,
    /// Top edge Y coordinate.
    pub top: i32,
    /// Right edge X coordinate.
    pub right: i32,
    /// Bottom edge Y coordinate.
    pub bottom: i32,
}

/// Result of companion side-by-side snap or unsnap on a verified secondary desktop window.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompanionSnapResult {
    /// Target secondary profile ID.
    pub profile_id: String,
    /// True if the secondary window is currently snapped beside the host window.
    pub snapped: bool,
    /// Original window rectangle before snapping (used for unsnap restore).
    #[serde(default)]
    pub previous_rect: Option<WindowRect>,
    /// Applied snapped rectangle if `snapped == true`.
    #[serde(default)]
    pub applied_rect: Option<WindowRect>,
    /// Human-readable status message.
    pub message: String,
}

/// Persisted metadata for an isolated Zalo Web session (`WebSessions/<id>/`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WebSessionRecord {
    /// Unique session identifier.
    pub id: String,
    /// User-facing display name.
    pub display_name: String,
    /// Optional UI accent color token.
    #[serde(default)]
    pub color_token: Option<String>,
    /// Monotonic generation counter incremented on session reset.
    pub profile_generation: u32,
    /// Optional platform-specific data store UUID (e.g. macOS 14+ `WKWebsiteDataStore`).
    #[serde(default)]
    pub data_store_uuid: Option<String>,
    /// Creation timestamp formatted as RFC 3339 UTC.
    pub created_at: String,
    /// Last update timestamp formatted as RFC 3339 UTC.
    pub updated_at: String,
}

/// Inspection status of the user's untouched primary Zalo Desktop installation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrimaryInstanceStatus {
    /// Primary user `ZaloData` directory (e.g., `%APPDATA%\ZaloData`).
    pub zalo_data_dir: PathBuf,
    /// Primary `lockfile` or `SingletonLock` path.
    pub lockfile_path: PathBuf,
    /// True if primary Zalo Desktop is currently running.
    pub running: bool,
    /// Protected PIDs belonging to primary Zalo that must NEVER be terminated.
    pub protected_pids: Vec<u32>,
    /// Process tree count of primary Zalo if running.
    pub process_count: usize,
    /// Aggregate working-set memory in bytes across active primary processes.
    #[serde(default)]
    pub working_set_bytes: u64,
    /// Detected numeric Zalo user ID in primary `ZaloData`, if present.
    #[serde(default)]
    pub detected_zalo_user_id: Option<String>,
    /// Native window handle if detected read-only on the primary instance.
    #[serde(default)]
    pub window_handle: Option<u64>,
    /// Visible or minimized main window title if detected read-only.
    #[serde(default)]
    pub window_title: Option<String>,
    /// Read-only window visibility classification (`visible`, `minimized`, `hidden_in_tray`, `none`).
    #[serde(default)]
    pub window_state: PrimaryWindowState,
    /// Diagnostic observations.
    pub notes: Vec<String>,
}

/// Single verifiable isolation check outcome (`PASS`, `FAIL`, `NOT_TESTED`, or `BLOCKED`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IsolationCheckItem {
    /// Machine-readable check identifier.
    pub check_name: String,
    /// Explicit verification status: `"PASS"`, `"FAIL"`, `"NOT_TESTED"`, or `"BLOCKED"`.
    pub status: String,
    /// Boolean compatibility field (`true` only when `status == "PASS"`).
    pub passed: bool,
    /// Human-readable verification evidence or failure reason.
    pub detail: String,
}

/// Complete runtime isolation verification report returned by `InstanceCoordinator::verify_isolation`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IsolationVerificationReport {
    /// Overall pass indicator (`true` only when every check status is `"PASS"`).
    pub ok: bool,
    /// Explicit alias indicating zero checks failed, skipped, or were blocked.
    pub all_passed: bool,
    /// Report generation timestamp in RFC 3339 UTC.
    pub timestamp_utc: String,
    /// Individual isolation checks.
    pub checks: Vec<IsolationCheckItem>,
}

/// Platform capability and limitation report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlatformCapabilityReport {
    /// Target platform.
    pub platform: PlatformKind,
    /// Overall platform status.
    pub overall_status: SupportStatus,
    /// Status for official VNG Zalo Desktop.
    pub official_zalo_status: SupportStatus,
    /// Status for the local multi-instance test desktop fixture.
    pub fixture_status: SupportStatus,
    /// Status for user-supplied local AppImage (Linux only).
    pub appimage_status: Option<SupportStatus>,
    /// Status for concurrent audio/video calls across multiple instances.
    pub concurrent_calls_status: SupportStatus,
    /// True if Windows Folder Redirection moves `AppData` away from `%USERPROFILE%`.
    pub folder_redirection_detected: bool,
    /// Unexpanded registry value if Folder Redirection was detected.
    pub folder_redirection_path: Option<String>,
    /// Explicit technical limitations on this platform.
    pub limitations: Vec<String>,
    /// Additional diagnostic checks.
    pub diagnostics: Vec<String>,
}

/// Single file entry recorded inside `backup.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupFileEntry {
    /// Forward-slash normalized relative path within the backup payload.
    pub relative_path: String,
    /// Exact byte size of the file.
    pub size_bytes: u64,
    /// Lowercase hex SHA-256 digest.
    pub sha256: String,
}

/// Atomic backup manifest stored as `<backup_dir>/backup.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupManifest {
    /// Schema version (`1`).
    pub schema_version: u32,
    /// Unique backup identifier.
    pub backup_id: String,
    /// Creation timestamp formatted as RFC 3339 UTC.
    pub created_at: String,
    /// Source profile 8-hex ID.
    pub source_profile_id: String,
    /// Source profile display name.
    pub source_profile_name: String,
    /// Source directory name.
    pub source_directory_name: String,
    /// Source detected Zalo user ID, if known.
    pub zalo_user_id: Option<String>,
    /// Total backed-up files.
    pub file_count: u64,
    /// Total backed-up payload bytes.
    pub total_bytes: u64,
    /// Per-file SHA-256 inventory.
    pub files: Vec<BackupFileEntry>,
}

/// Result returned when an atomic offline backup completes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupResult {
    /// Final committed backup directory path.
    pub backup_dir: PathBuf,
    /// Path to `backup.json` manifest.
    pub manifest_path: PathBuf,
    /// Verified manifest written into the backup.
    pub manifest: BackupManifest,
}

/// Summary of stale lock/crash recovery actions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecoveryReport {
    /// Whether any stale artifacts were removed.
    pub recovered: bool,
    /// Relative names or paths of removed stale lock/signal files.
    pub removed_files: Vec<String>,
    /// Human-readable explanation.
    pub summary: String,
}

/// Resolves the active application data directory.
///
/// Precedence:
/// 1. Explicit `override_dir` argument
/// 2. `DIM_DATA_DIR` environment variable (if non-empty)
/// 3. OS-specific standard location (`%LOCALAPPDATA%\MultiZaloDesktop`,
///    `~/Library/Application Support/MultiZaloDesktop`, or
///    `$XDG_DATA_HOME/MultiZaloDesktop`).
pub fn resolve_data_dir(override_dir: Option<&Path>) -> Result<PathBuf, DimError> {
    resolve_data_dir_for_platform(PlatformKind::current(), override_dir, &|key| {
        std::env::var(key).ok()
    })
}

/// Deterministic platform-parameterized variant of [`resolve_data_dir`] for testing.
pub fn resolve_data_dir_for_platform(
    platform: PlatformKind,
    override_dir: Option<&Path>,
    env_lookup: &dyn Fn(&str) -> Option<String>,
) -> Result<PathBuf, DimError> {
    if let Some(explicit) = override_dir {
        if !explicit.as_os_str().is_empty() {
            return Ok(explicit.to_path_buf());
        }
    }

    if let Some(env_override) = env_lookup(DIM_DATA_DIR_ENV) {
        let trimmed = env_override.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }

    match platform {
        PlatformKind::Windows => {
            if let Some(local) = env_lookup("LOCALAPPDATA").filter(|s| !s.trim().is_empty()) {
                return Ok(PathBuf::from(local).join(APP_DIR_NAME));
            }
            if let Some(home) = env_lookup("USERPROFILE").filter(|s| !s.trim().is_empty()) {
                return Ok(PathBuf::from(home)
                    .join("AppData")
                    .join("Local")
                    .join(APP_DIR_NAME));
            }
            Err(DimError::SchemaError(
                "cannot resolve %LOCALAPPDATA% or %USERPROFILE% on Windows".to_owned(),
            ))
        }
        PlatformKind::MacOs => {
            if let Some(home) = env_lookup("HOME").filter(|s| !s.trim().is_empty()) {
                return Ok(PathBuf::from(home)
                    .join("Library")
                    .join("Application Support")
                    .join(APP_DIR_NAME));
            }
            Err(DimError::SchemaError(
                "cannot resolve $HOME on macOS".to_owned(),
            ))
        }
        PlatformKind::Linux => {
            if let Some(xdg) = env_lookup("XDG_DATA_HOME").filter(|s| !s.trim().is_empty()) {
                return Ok(PathBuf::from(xdg).join(APP_DIR_NAME));
            }
            if let Some(home) = env_lookup("HOME").filter(|s| !s.trim().is_empty()) {
                return Ok(PathBuf::from(home)
                    .join(".local")
                    .join("share")
                    .join(APP_DIR_NAME));
            }
            Err(DimError::SchemaError(
                "cannot resolve $XDG_DATA_HOME or $HOME on Linux".to_owned(),
            ))
        }
    }
}

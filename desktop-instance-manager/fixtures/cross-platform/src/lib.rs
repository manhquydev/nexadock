//! Cross-platform synthetic desktop application fixture library (`dim-xf-fixture`).
//!
//! Provides realistic multi-instance desktop behavior without touching real Zalo user data:
//! - Instance ID & isolated profile directory resolution (`APPDATA`, `CFFIXED_USER_HOME`, `XDG_*`)
//! - Live OS-level file locks (`CreateFileW` share-mode locks on Windows, `flock` + `SingletonLock` on Unix)
//! - Child helper process trees (`--spawn-children`) for tree-cleanup verification
//! - Deterministic seeded file writes, WAL append journal, and SHA-256 payload manifests
//! - Persistent state across restarts (`fixture-state.json` + `launch-state.json`)
//! - Fault injection: slow startup (pre-lock & post-lock), process hang, crash modes, controlled errors

use std::collections::BTreeMap;
use std::env;
use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use dim_core::now_rfc3339_utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

static SIGNAL_SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Crash injection behavior for the synthetic desktop fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CrashMode {
    /// Normal execution without crash injection.
    None,
    /// Abort immediately before acquiring lockfiles.
    Immediate,
    /// Acquire lockfile & state, wait `crash_delay_ms`, then exit `137` leaving stale locks behind.
    AfterLock,
    /// Leave a torn `.tmp` write artifact alongside stale lockfiles, then exit `138`.
    MidWrite,
    /// Write malformed JSON into `corrupt-cache.json` and exit `139` leaving stale locks.
    CorruptState,
}

impl CrashMode {
    /// Parses a CLI crash mode token.
    #[must_use]
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "immediate" => Self::Immediate,
            "after-lock" | "after_lock" | " crash" | "true" | "1" => Self::AfterLock,
            "mid-write" | "mid_write" => Self::MidWrite,
            "corrupt-state" | "corrupt_state" => Self::CorruptState,
            _ => Self::None,
        }
    }
}

/// OS file-lock sharing behavior for `ZaloData/lockfile`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LockShareMode {
    /// Hold an exclusive OS handle (`dwShareMode = 0` on Windows, `LOCK_EX` on Unix).
    Exclusive,
    /// Allow concurrent read while denying write/delete (`FILE_SHARE_READ` on Windows, `LOCK_EX` on Unix).
    ReadShared,
    /// Write `lockfile` atomically with `create_new` and close descriptor immediately (legacy compatibility).
    None,
}

impl LockShareMode {
    /// Parses a CLI lock share mode token.
    #[must_use]
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "exclusive" | "0" => Self::Exclusive,
            "none" | "unlocked" => Self::None,
            _ => Self::ReadShared,
        }
    }
}

/// Controlled error simulation modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ControlledErrorMode {
    /// No controlled error.
    None,
    /// Exit immediately with `error_exit_code` and record `last-error.json`.
    ExitCode,
    /// Simulate `ENOSPC` / disk-full failure (`exit 28`).
    DiskFull,
    /// Simulate `EACCES` / permission-denied failure (`exit 13`).
    PermissionDenied,
    /// Simulate unreadable/corrupt profile config (`exit 65`).
    CorruptConfig,
    /// Simulate single-instance lock contention (`exit 2`).
    LockConflict,
}

impl ControlledErrorMode {
    /// Parses a CLI controlled error token.
    #[must_use]
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "exit-code" | "exit_code" | "custom" => Self::ExitCode,
            "disk-full" | "disk_full" | "enospc" => Self::DiskFull,
            "permission-denied" | "permission_denied" | "eacces" => Self::PermissionDenied,
            "corrupt-config" | "corrupt_config" => Self::CorruptConfig,
            "lock-conflict" | "lock_conflict" => Self::LockConflict,
            _ => Self::None,
        }
    }
}

/// Complete command-line configuration for `xf-desktop-fixture`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureCliConfig {
    /// Logical instance identifier (`--instance-id` or env fallback).
    pub instance_id: String,
    /// Explicit profile root override (`--profile-dir`).
    pub profile_dir_override: Option<PathBuf>,
    /// Explicit `ZaloData` user-data directory override (`--user-data-dir`).
    pub user_data_dir_override: Option<PathBuf>,
    /// Simulated logged-in numeric account ID (`Database/_production/<user_id>`).
    pub user_id: String,
    /// Send force-quit signal to running instance (`--z-update-force-quit`).
    pub force_quit: bool,
    /// Crash injection mode.
    pub crash_mode: CrashMode,
    /// Delay in milliseconds before executing `AfterLock`/`MidWrite`/`CorruptState` crash.
    pub crash_delay_ms: u64,
    /// Delay in milliseconds BEFORE acquiring `lockfile` (`--slow-startup-pre-lock-ms`).
    pub slow_startup_pre_lock_ms: u64,
    /// Delay in milliseconds AFTER acquiring `lockfile` before reporting ready (`--slow-startup-post-lock-ms`).
    pub slow_startup_post_lock_ms: u64,
    /// Ignore graceful shutdown signals (`.quit-signal`, `SIGTERM`, `--z-update-force-quit`).
    pub hang_on_quit: bool,
    /// Artificial delay in milliseconds before completing graceful shutdown.
    pub quit_delay_ms: u64,
    /// Optional auto-exit lifetime in milliseconds (`--stay-ms`).
    pub stay_ms: Option<u64>,
    /// Number of deterministic payload files to write inside `ZaloData/storage/`.
    pub write_files: usize,
    /// Byte size of each deterministic payload file.
    pub write_bytes: usize,
    /// Per-file write sleep in milliseconds to simulate slow disk I/O (`--slow-io-ms`).
    pub slow_io_ms: u64,
    /// Deterministic PRNG seed for payload generation.
    pub seed: u64,
    /// OS file-lock mode on `ZaloData/lockfile`.
    pub lock_mode: LockShareMode,
    /// Number of child worker processes to spawn (`--spawn-children`).
    pub spawn_children: usize,
    /// Leave child worker processes alive when crashing or exiting (`--orphan-children`).
    pub orphan_children: bool,
    /// Internal worker-mode parent PID (`--worker-of <pid>`).
    pub worker_of_pid: Option<u32>,
    /// Internal worker index (`--worker-index <idx>`).
    pub worker_index: usize,
    /// Controlled error mode (`--error-mode`).
    pub error_mode: ControlledErrorMode,
    /// Controlled error exit code (`--error-code`).
    pub error_exit_code: u8,
    /// Key-value state pairs to persist into `fixture-state.json` (`--set-kv k=v`).
    pub persist_kv: BTreeMap<String, String>,
    /// Optional heartbeat interval in milliseconds for periodic WAL updates (`--heartbeat-ms`).
    pub heartbeat_ms: u64,
}

impl Default for FixtureCliConfig {
    fn default() -> Self {
        let instance_id = env::var("DIM_INSTANCE_ID")
            .or_else(|_| env::var("DIM_PROFILE_ID"))
            .or_else(|_| env::var("NEXADOCK_PROFILE_ID"))
            .or_else(|_| env::var("MULTIZALO_PROFILE_ID"))
            .unwrap_or_else(|_| format!("inst-{:08x}", std::process::id()));
        let user_id = env::var("FIXTURE_ZALO_USER_ID").unwrap_or_else(|_| "100001".to_owned());
        Self {
            instance_id,
            profile_dir_override: None,
            user_data_dir_override: None,
            user_id,
            force_quit: false,
            crash_mode: CrashMode::None,
            crash_delay_ms: 150,
            slow_startup_pre_lock_ms: 0,
            slow_startup_post_lock_ms: 0,
            hang_on_quit: false,
            quit_delay_ms: 0,
            stay_ms: None,
            write_files: 2,
            write_bytes: 512,
            slow_io_ms: 0,
            seed: 0x2026_1117,
            lock_mode: LockShareMode::ReadShared,
            spawn_children: 0,
            orphan_children: false,
            worker_of_pid: None,
            worker_index: 0,
            error_mode: ControlledErrorMode::None,
            error_exit_code: 42,
            persist_kv: BTreeMap::new(),
            heartbeat_ms: 0,
        }
    }
}

impl FixtureCliConfig {
    /// Parses CLI arguments into [`FixtureCliConfig`].
    pub fn from_args<I: Iterator<Item = String>>(iter: I) -> Self {
        let mut cfg = Self::default();
        let args: Vec<String> = iter.collect();
        let mut i = 0;

        let next_val = |args: &[String], idx: &mut usize, prefix: &str| -> Option<String> {
            if let Some(rest) = args[*idx].strip_prefix(prefix) {
                return Some(rest.to_owned());
            }
            if *idx + 1 < args.len() {
                *idx += 1;
                return Some(args[*idx].clone());
            }
            None
        };

        while i < args.len() {
            let a = args[i].as_str();
            if a == "--z-update-force-quit" {
                cfg.force_quit = true;
            } else if a == "--crash" {
                if cfg.crash_mode == CrashMode::None {
                    cfg.crash_mode = CrashMode::AfterLock;
                }
            } else if a == "--crash-mode" || a.starts_with("--crash-mode=") {
                if let Some(v) = next_val(&args, &mut i, "--crash-mode=") {
                    cfg.crash_mode = CrashMode::parse(&v);
                }
            } else if a == "--crash-delay-ms" || a.starts_with("--crash-delay-ms=") {
                if let Some(v) = next_val(&args, &mut i, "--crash-delay-ms=") {
                    cfg.crash_delay_ms = v.parse::<u64>().unwrap_or(150).max(5);
                }
            } else if a == "--instance-id" || a.starts_with("--instance-id=") {
                if let Some(v) = next_val(&args, &mut i, "--instance-id=") {
                    if !v.trim().is_empty() {
                        cfg.instance_id = v.trim().to_owned();
                    }
                }
            } else if a == "--profile-dir" || a.starts_with("--profile-dir=") {
                if let Some(v) = next_val(&args, &mut i, "--profile-dir=") {
                    cfg.profile_dir_override = Some(PathBuf::from(v));
                }
            } else if a == "--user-data-dir" || a.starts_with("--user-data-dir=") {
                if let Some(v) = next_val(&args, &mut i, "--user-data-dir=") {
                    cfg.user_data_dir_override = Some(PathBuf::from(v));
                }
            } else if a == "--user-id" || a.starts_with("--user-id=") {
                if let Some(v) = next_val(&args, &mut i, "--user-id=") {
                    cfg.user_id = v;
                }
            } else if a == "--slow-startup-pre-lock-ms"
                || a.starts_with("--slow-startup-pre-lock-ms=")
                || a == "--slow-startup-ms"
                || a.starts_with("--slow-startup-ms=")
            {
                let prefix = if a.starts_with("--slow-startup-ms=") {
                    "--slow-startup-ms="
                } else {
                    "--slow-startup-pre-lock-ms="
                };
                if let Some(v) = next_val(&args, &mut i, prefix) {
                    cfg.slow_startup_pre_lock_ms = v.parse::<u64>().unwrap_or(0);
                }
            } else if a == "--slow-startup-post-lock-ms"
                || a.starts_with("--slow-startup-post-lock-ms=")
            {
                if let Some(v) = next_val(&args, &mut i, "--slow-startup-post-lock-ms=") {
                    cfg.slow_startup_post_lock_ms = v.parse::<u64>().unwrap_or(0);
                }
            } else if a == "--hang-on-quit" || a == "--hang" {
                cfg.hang_on_quit = true;
            } else if a == "--quit-delay-ms" || a.starts_with("--quit-delay-ms=") {
                if let Some(v) = next_val(&args, &mut i, "--quit-delay-ms=") {
                    cfg.quit_delay_ms = v.parse::<u64>().unwrap_or(0);
                }
            } else if a == "--stay-ms" || a.starts_with("--stay-ms=") {
                if let Some(v) = next_val(&args, &mut i, "--stay-ms=") {
                    cfg.stay_ms = v.parse::<u64>().ok();
                }
            } else if a == "--write-files" || a.starts_with("--write-files=") {
                if let Some(v) = next_val(&args, &mut i, "--write-files=") {
                    cfg.write_files = v.parse::<usize>().unwrap_or(2);
                }
            } else if a == "--write-bytes" || a.starts_with("--write-bytes=") {
                if let Some(v) = next_val(&args, &mut i, "--write-bytes=") {
                    cfg.write_bytes = v.parse::<usize>().unwrap_or(512);
                }
            } else if a == "--slow-io-ms" || a.starts_with("--slow-io-ms=") {
                if let Some(v) = next_val(&args, &mut i, "--slow-io-ms=") {
                    cfg.slow_io_ms = v.parse::<u64>().unwrap_or(0);
                }
            } else if a == "--seed" || a.starts_with("--seed=") {
                if let Some(v) = next_val(&args, &mut i, "--seed=") {
                    cfg.seed = if let Some(hex) = v.strip_prefix("0x") {
                        u64::from_str_radix(hex, 16).unwrap_or(0x2026_1117)
                    } else {
                        v.parse::<u64>().unwrap_or(0x2026_1117)
                    };
                }
            } else if a == "--lock-mode" || a.starts_with("--lock-mode=") {
                if let Some(v) = next_val(&args, &mut i, "--lock-mode=") {
                    cfg.lock_mode = LockShareMode::parse(&v);
                }
            } else if a == "--spawn-children" || a.starts_with("--spawn-children=") {
                if let Some(v) = next_val(&args, &mut i, "--spawn-children=") {
                    cfg.spawn_children = v.parse::<usize>().unwrap_or(0).min(16);
                }
            } else if a == "--orphan-children" {
                cfg.orphan_children = true;
            } else if a == "--worker-of" || a.starts_with("--worker-of=") {
                if let Some(v) = next_val(&args, &mut i, "--worker-of=") {
                    cfg.worker_of_pid = v.parse::<u32>().ok();
                }
            } else if a == "--worker-index" || a.starts_with("--worker-index=") {
                if let Some(v) = next_val(&args, &mut i, "--worker-index=") {
                    cfg.worker_index = v.parse::<usize>().unwrap_or(0);
                }
            } else if a == "--error-mode" || a.starts_with("--error-mode=") {
                if let Some(v) = next_val(&args, &mut i, "--error-mode=") {
                    cfg.error_mode = ControlledErrorMode::parse(&v);
                }
            } else if a == "--error-code" || a.starts_with("--error-code=") {
                if let Some(v) = next_val(&args, &mut i, "--error-code=") {
                    cfg.error_exit_code = v.parse::<u8>().unwrap_or(42);
                }
            } else if a == "--set-kv" || a.starts_with("--set-kv=") {
                if let Some(v) = next_val(&args, &mut i, "--set-kv=") {
                    if let Some((k, val)) = v.split_once('=') {
                        cfg.persist_kv.insert(k.trim().to_owned(), val.to_owned());
                    }
                }
            } else if a == "--heartbeat-ms" || a.starts_with("--heartbeat-ms=") {
                if let Some(v) = next_val(&args, &mut i, "--heartbeat-ms=") {
                    cfg.heartbeat_ms = v.parse::<u64>().unwrap_or(0);
                }
            }
            i += 1;
        }

        cfg
    }
}

/// JSON structure persisted in `ZaloData/lockfile`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureLockRecord {
    /// Owning process PID.
    pub pid: u32,
    /// Logical instance ID.
    pub instance_id: String,
    /// Profile ID if injected by manager.
    pub profile_id: Option<String>,
    /// Lock acquisition timestamp in RFC 3339 UTC.
    pub started_at: String,
    /// Fixture binary identifier.
    pub app: String,
    /// Active lock mode.
    pub lock_mode: LockShareMode,
}

/// Persistent state stored in `ZaloData/fixture-state.json` across restarts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FixturePersistentState {
    /// Logical instance ID most recently bound to this directory.
    pub instance_id: String,
    /// Resolved profile directory root.
    pub profile_dir: String,
    /// Resolved `ZaloData` directory.
    pub zalo_data_dir: String,
    /// Monotonic boot counter across restarts.
    pub boot_sequence: u64,
    /// Count of graceful shutdowns completed.
    pub clean_shutdown_count: u64,
    /// True if the previous boot did not record a clean shutdown.
    pub unclean_previous_boot: bool,
    /// Count of focus/restore signals received.
    pub focus_count: u64,
    /// Active or most recent owning PID.
    pub last_pid: u32,
    /// Child worker PIDs spawned in the most recent boot.
    pub child_pids: Vec<u32>,
    /// Historical PIDs (bounded to last 32 boots).
    pub pid_history: Vec<u32>,
    /// Startup timestamp of most recent boot (RFC 3339 UTC).
    pub started_at_utc: String,
    /// Ready timestamp after post-lock initialization (RFC 3339 UTC).
    pub ready_at_utc: String,
    /// Measured internal startup latency in milliseconds.
    pub startup_duration_ms: u64,
    /// Most recent focus timestamp (RFC 3339 UTC).
    pub last_focused_at: Option<String>,
    /// Most recent shutdown/crash timestamp (RFC 3339 UTC).
    pub last_stopped_at: Option<String>,
    /// True when the most recent run exited cleanly.
    pub clean_shutdown: bool,
    /// True when the most recent run exited via simulated crash.
    pub crashed: bool,
    /// Crash mode if crashed.
    pub crash_mode: Option<CrashMode>,
    /// Active simulated Zalo numeric user ID.
    pub zalo_user_id: String,
    /// Snapshot of isolated environment variables observed by the fixture.
    pub observed_env: BTreeMap<String, String>,
    /// Arbitrary persisted key-value state across restarts.
    pub persisted_kv: BTreeMap<String, String>,
    /// Number of heartbeat ticks written during run.
    pub heartbeat_ticks: u64,
}

/// Legacy-compatible `launch-state.json` mirror so existing manager helpers can also inspect runs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LegacyLaunchState {
    /// Total launches recorded.
    pub launch_count: u64,
    /// Total focus events recorded.
    pub focus_count: u64,
    /// Last PID.
    pub last_pid: u32,
    /// Last start time.
    pub last_started_at: String,
    /// Last focus time.
    pub last_focused_at: Option<String>,
    /// Last stop time.
    pub last_stopped_at: Option<String>,
    /// Clean shutdown indicator.
    pub clean_shutdown: bool,
    /// Crash indicator.
    pub crashed: bool,
    /// Simulated account ID.
    pub zalo_user_id: String,
}

/// Single deterministic payload chunk written by the fixture.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FixtureChunkEntry {
    /// Relative path under `ZaloData`.
    pub relative_path: String,
    /// Byte length.
    pub size_bytes: u64,
    /// Lowercase SHA-256 digest.
    pub sha256: String,
}

/// Manifest stored at `ZaloData/fixture-files.json` for data-isolation verification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FixturePayloadManifest {
    /// Instance ID that generated this payload.
    pub instance_id: String,
    /// Owning PID.
    pub pid: u32,
    /// Hex-formatted 64-bit seed used (`0x...`) for lossless Rust/JS parity.
    pub seed: String,
    /// Total bytes written across chunks.
    pub total_bytes: u64,
    /// Deterministic chunk entries.
    pub chunks: Vec<FixtureChunkEntry>,
}

/// Ready event emitted as single-line JSON on stdout once initialization finishes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureReadyBanner {
    /// Event type (`"ready"`).
    pub event: String,
    /// Instance ID.
    pub instance_id: String,
    /// Main process ID.
    pub pid: u32,
    /// Spawned child worker PIDs.
    pub child_pids: Vec<u32>,
    /// Resolved profile directory.
    pub profile_dir: String,
    /// Resolved `ZaloData` directory.
    pub zalo_data_dir: String,
    /// Boot sequence count.
    pub boot_sequence: u64,
    /// Startup latency in milliseconds.
    pub startup_duration_ms: u64,
    /// Simulated Zalo user ID.
    pub zalo_user_id: String,
}

/// Controlled error diagnostic stored at `ZaloData/last-error.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlledErrorReport {
    /// Error mode triggered.
    pub error_mode: ControlledErrorMode,
    /// Process exit code returned.
    pub exit_code: u8,
    /// Instance ID.
    pub instance_id: String,
    /// Process ID.
    pub pid: u32,
    /// Timestamp in RFC 3339 UTC.
    pub occurred_at_utc: String,
    /// Diagnostic detail.
    pub message: String,
}

/// Deterministic SplitMix64 generator so payload bytes require zero external RNG crates.
#[derive(Debug, Clone)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// Creates a new generator seeded with `seed`.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Advances state and returns next `u64`.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Fills `buf` deterministically.
    pub fn fill_bytes(&mut self, buf: &mut [u8]) {
        let mut chunks = buf.chunks_exact_mut(8);
        for chunk in &mut chunks {
            let word = self.next_u64().to_le_bytes();
            chunk.copy_from_slice(&word);
        }
        let rem = chunks.into_remainder();
        if !rem.is_empty() {
            let word = self.next_u64().to_le_bytes();
            rem.copy_from_slice(&word[..rem.len()]);
        }
    }
}

/// Live OS file lock guard keeping the underlying descriptor open until dropped or released.
pub struct OsFileLock {
    lockfile_path: PathBuf,
    singleton_symlink_path: Option<PathBuf>,
    handle: Option<File>,
}

impl OsFileLock {
    /// Attempts to acquire the single-instance lock on `lockfile_path`.
    pub fn try_acquire(
        lockfile_path: &Path,
        zalo_data_dir: &Path,
        record: &FixtureLockRecord,
    ) -> io::Result<Option<Self>> {
        if let Some(parent) = lockfile_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json_bytes = format!(
            "{}\n",
            serde_json::to_string_pretty(record).unwrap_or_else(|_| "{}".to_owned())
        );

        match record.lock_mode {
            LockShareMode::None => {
                for attempt in 0..2 {
                    match OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(lockfile_path)
                    {
                        Ok(mut f) => {
                            f.write_all(json_bytes.as_bytes())?;
                            f.sync_all()?;
                            let symlink = create_unix_singleton_symlink(zalo_data_dir, record.pid);
                            return Ok(Some(Self {
                                lockfile_path: lockfile_path.to_path_buf(),
                                singleton_symlink_path: symlink,
                                handle: None,
                            }));
                        }
                        Err(e) if e.kind() == io::ErrorKind::AlreadyExists && attempt == 0 => {
                            if !existing_lock_owner_alive(lockfile_path) {
                                let _ = fs::remove_file(lockfile_path);
                                continue;
                            }
                            return Ok(None);
                        }
                        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => return Ok(None),
                        Err(e) => return Err(e),
                    }
                }
                Ok(None)
            }
            LockShareMode::Exclusive | LockShareMode::ReadShared => {
                acquire_os_held_lock(lockfile_path, zalo_data_dir, record, &json_bytes)
            }
        }
    }

    /// Releases the open OS handle and unlinks lock artifacts cleanly.
    pub fn release_and_unlink(&mut self) {
        // Drop the file handle BEFORE unlinking on Windows so `DeleteFileW` succeeds cleanly.
        self.handle.take();
        let _ = fs::remove_file(&self.lockfile_path);
        if let Some(sym) = self.singleton_symlink_path.take() {
            let _ = fs::remove_file(sym);
        }
    }
}

#[cfg(windows)]
fn acquire_os_held_lock(
    lockfile_path: &Path,
    _zalo_data_dir: &Path,
    record: &FixtureLockRecord,
    json_bytes: &str,
) -> io::Result<Option<OsFileLock>> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const ERROR_SHARING_VIOLATION: i32 = 32;
    const ERROR_LOCK_VIOLATION: i32 = 33;

    let share = match record.lock_mode {
        LockShareMode::Exclusive => 0,
        LockShareMode::ReadShared | LockShareMode::None => FILE_SHARE_READ,
    };

    for attempt in 0..2 {
        let open_res = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .share_mode(share)
            .open(lockfile_path);

        match open_res {
            Ok(mut f) => {
                // If an existing stale unlocked file was truncated, write fresh owner metadata and keep handle open.
                f.write_all(json_bytes.as_bytes())?;
                f.flush()?;
                return Ok(Some(OsFileLock {
                    lockfile_path: lockfile_path.to_path_buf(),
                    singleton_symlink_path: None,
                    handle: Some(f),
                }));
            }
            Err(e)
                if matches!(
                    e.raw_os_error(),
                    Some(ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION)
                ) =>
            {
                return Ok(None);
            }
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied && attempt == 0 => {
                if !existing_lock_owner_alive(lockfile_path) {
                    let _ = fs::remove_file(lockfile_path);
                    continue;
                }
                return Ok(None);
            }
            Err(e) => return Err(e),
        }
    }
    Ok(None)
}

#[cfg(unix)]
fn acquire_os_held_lock(
    lockfile_path: &Path,
    zalo_data_dir: &Path,
    record: &FixtureLockRecord,
    json_bytes: &str,
) -> io::Result<Option<OsFileLock>> {
    use std::os::unix::io::AsRawFd;

    let mut f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lockfile_path)?;

    // SAFETY: `f.as_raw_fd()` is a valid open file descriptor owned by `f`.
    let rc = unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if rc != 0 {
        let err = io::Error::last_os_error();
        if err.kind() == io::ErrorKind::WouldBlock
            || err.raw_os_error() == Some(libc::EWOULDBLOCK)
            || err.raw_os_error() == Some(libc::EAGAIN)
        {
            return Ok(None);
        }
        return Err(err);
    }

    f.set_len(0)?;
    f.write_all(json_bytes.as_bytes())?;
    f.sync_all()?;
    let symlink = create_unix_singleton_symlink(zalo_data_dir, record.pid);
    Ok(Some(OsFileLock {
        lockfile_path: lockfile_path.to_path_buf(),
        singleton_symlink_path: symlink,
        handle: Some(f),
    }))
}

#[cfg(unix)]
fn create_unix_singleton_symlink(zalo_data_dir: &Path, pid: u32) -> Option<PathBuf> {
    let link_path = zalo_data_dir.join("SingletonLock");
    if fs::symlink_metadata(&link_path).is_ok() {
        let _ = fs::remove_file(&link_path);
    }
    let host = env::var("HOSTNAME")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "localhost".to_owned());
    let target = format!("{host}-{pid}");
    std::os::unix::fs::symlink(target, &link_path)
        .ok()
        .map(|()| link_path)
}

#[cfg(not(unix))]
fn create_unix_singleton_symlink(_zalo_data_dir: &Path, _pid: u32) -> Option<PathBuf> {
    None
}

fn existing_lock_owner_alive(lockfile_path: &Path) -> bool {
    let Ok(raw) = fs::read_to_string(lockfile_path) else {
        return false;
    };
    let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return false;
    };
    let Some(pid) = val
        .get("pid")
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
    else {
        return false;
    };
    if pid == 0 || pid == std::process::id() {
        return false;
    }
    is_os_pid_alive(pid)
}

/// Checks whether `pid` is currently alive on the host OS.
#[must_use]
pub fn is_os_pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(windows)]
    {
        const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
        const STILL_ACTIVE: u32 = 259;
        #[link(name = "kernel32")]
        extern "system" {
            fn OpenProcess(
                dw_desired_access: u32,
                b_inherit_handle: i32,
                dw_process_id: u32,
            ) -> *mut std::ffi::c_void;
            fn GetExitCodeProcess(h_process: *mut std::ffi::c_void, lp_exit_code: *mut u32) -> i32;
            fn CloseHandle(h_object: *mut std::ffi::c_void) -> i32;
        }
        // SAFETY: Query-only process handle is closed immediately before returning.
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return false;
            }
            let mut code: u32 = 0;
            let ok = GetExitCodeProcess(h, &mut code);
            let _ = CloseHandle(h);
            ok != 0 && code == STILL_ACTIVE
        }
    }
    #[cfg(unix)]
    {
        #[cfg(target_os = "linux")]
        if let Ok(stat_raw) = fs::read_to_string(format!("/proc/{pid}/stat")) {
            if let Some(rparen) = stat_raw.rfind(')') {
                if let Some(state_ch) = stat_raw
                    .get(rparen + 1..)
                    .and_then(|rest| rest.trim_start().chars().next())
                {
                    if matches!(state_ch, 'Z' | 'X') {
                        return false;
                    }
                }
            }
        }
        let Ok(raw) = libc::pid_t::try_from(pid) else {
            return false;
        };
        // SAFETY: Signal 0 only probes PID existence.
        let rc = unsafe { libc::kill(raw, 0) };
        if rc == 0 {
            return true;
        }
        io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
}

fn install_signal_handlers() {
    #[cfg(unix)]
    {
        extern "C" fn on_sig(_sig: libc::c_int) {
            SIGNAL_SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
        }
        // SAFETY: `on_sig` is async-signal-safe (only performs an atomic store).
        unsafe {
            let _ = libc::signal(libc::SIGTERM, on_sig as *const () as libc::sighandler_t);
            let _ = libc::signal(libc::SIGINT, on_sig as *const () as libc::sighandler_t);
        }
    }
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn SetConsoleCtrlHandler(
                handler: Option<unsafe extern "system" fn(u32) -> i32>,
                add: i32,
            ) -> i32;
        }
        unsafe extern "system" fn ctrl_handler(_ctrl_type: u32) -> i32 {
            SIGNAL_SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
            1
        }
        // SAFETY: Registers a valid `extern "system"` console control handler.
        unsafe {
            let _ = SetConsoleCtrlHandler(Some(ctrl_handler), 1);
        }
    }
}

/// Resolves `(profile_dir, zalo_data_dir)` according to CLI overrides and OS environment isolation.
#[must_use]
pub fn resolve_fixture_directories(cfg: &FixtureCliConfig) -> (PathBuf, PathBuf) {
    if let Some(ud) = &cfg.user_data_dir_override {
        let profile = cfg
            .profile_dir_override
            .clone()
            .or_else(|| {
                ud.parent()
                    .and_then(Path::parent)
                    .and_then(Path::parent)
                    .map(Path::to_path_buf)
            })
            .unwrap_or_else(|| ud.clone());
        return (profile, ud.clone());
    }

    if let Some(pdir) = &cfg.profile_dir_override {
        let zdata = if cfg!(target_os = "windows") {
            pdir.join("AppData").join("Roaming").join("ZaloData")
        } else if cfg!(target_os = "macos") {
            pdir.join("Library")
                .join("Application Support")
                .join("ZaloData")
        } else {
            pdir.join(".config").join("ZaloData")
        };
        return (pdir.clone(), zdata);
    }

    if let Some(appdata) = env::var_os("APPDATA").filter(|v| !v.is_empty()) {
        let roaming = PathBuf::from(appdata);
        let profile = env::var_os("USERPROFILE")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                roaming
                    .parent()
                    .and_then(Path::parent)
                    .map(Path::to_path_buf)
            })
            .unwrap_or_else(|| roaming.clone());
        return (profile, roaming.join("ZaloData"));
    }

    if cfg!(target_os = "macos") {
        let home = env::var_os("CFFIXED_USER_HOME")
            .or_else(|| env::var_os("HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(env::temp_dir);
        let zdata = home
            .join("Library")
            .join("Application Support")
            .join("ZaloData");
        return (home, zdata);
    }

    if let Some(xdg) = env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        let cfg_dir = PathBuf::from(xdg);
        let home = env::var_os("HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| cfg_dir.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| cfg_dir.clone());
        return (home, cfg_dir.join("ZaloData"));
    }

    let home = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir);
    (home.clone(), home.join(".config").join("ZaloData"))
}

fn capture_observed_env() -> BTreeMap<String, String> {
    let keys = [
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "TEMP",
        "TMP",
        "HOMEDRIVE",
        "HOMEPATH",
        "HOME",
        "CFFIXED_USER_HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_CACHE_HOME",
        "TMPDIR",
        "DIM_PROFILE_ID",
        "DIM_PROFILE_NAME",
        "DIM_INSTANCE_ID",
    ];
    let mut map = BTreeMap::new();
    for k in keys {
        if let Ok(val) = env::var(k) {
            map.insert(k.to_owned(), val);
        }
    }
    map
}

/// Executes the synthetic desktop fixture lifecycle and returns its process exit code.
pub fn run_fixture_process(cfg: &FixtureCliConfig) -> io::Result<u8> {
    install_signal_handlers();

    // Child worker mode: stay alive until parent dies (unless orphan mode) or signal arrives.
    if let Some(parent_pid) = cfg.worker_of_pid {
        return run_child_worker(parent_pid, cfg);
    }

    let boot_timer = Instant::now();
    let started_at_utc = now_rfc3339_utc();
    let pid = std::process::id();
    let (profile_dir, zalo_data_dir) = resolve_fixture_directories(cfg);
    fs::create_dir_all(&zalo_data_dir)?;

    let lockfile_path = zalo_data_dir.join("lockfile");
    let quit_signal_path = zalo_data_dir.join(".quit-signal");
    let focus_signal_path = zalo_data_dir.join(".focus-signal");
    let wal_log_path = zalo_data_dir.join("wal.log");
    let startup_log_path = zalo_data_dir.join("startup.log");
    let state_path = zalo_data_dir.join("fixture-state.json");
    let legacy_state_path = zalo_data_dir.join("launch-state.json");
    let files_manifest_path = zalo_data_dir.join("fixture-files.json");

    // 1. Controlled error injection (before acquiring normal session state)
    if cfg.error_mode != ControlledErrorMode::None {
        let code = match cfg.error_mode {
            ControlledErrorMode::None => 0,
            ControlledErrorMode::ExitCode => cfg.error_exit_code,
            ControlledErrorMode::DiskFull => 28,
            ControlledErrorMode::PermissionDenied => 13,
            ControlledErrorMode::CorruptConfig => 65,
            ControlledErrorMode::LockConflict => 2,
        };
        let report = ControlledErrorReport {
            error_mode: cfg.error_mode,
            exit_code: code,
            instance_id: cfg.instance_id.clone(),
            pid,
            occurred_at_utc: now_rfc3339_utc(),
            message: format!("Simulated controlled error {:?}", cfg.error_mode),
        };
        let _ = write_json_atomic(&zalo_data_dir.join("last-error.json"), &report);
        append_line(
            &wal_log_path,
            &format!("CONTROLLED_ERROR mode={:?} code={code}", cfg.error_mode),
        );
        return Ok(code);
    }

    // 2. Immediate pre-lock crash injection
    if cfg.crash_mode == CrashMode::Immediate {
        append_line(&wal_log_path, &format!("CRASH_IMMEDIATE pid={pid}"));
        return Ok(101);
    }

    // 3. Simulated slow startup BEFORE acquiring lockfile
    if cfg.slow_startup_pre_lock_ms > 0 {
        append_line(
            &wal_log_path,
            &format!(
                "SLOW_STARTUP_PRE_LOCK_BEGIN pid={pid} delay_ms={}",
                cfg.slow_startup_pre_lock_ms
            ),
        );
        thread::sleep(Duration::from_millis(cfg.slow_startup_pre_lock_ms));
    }

    // 4. Handle `--z-update-force-quit` helper invocation cleanly without clobbering active lock
    if cfg.force_quit {
        write_json_atomic(
            &quit_signal_path,
            &serde_json::json!({
                "requested_at": now_rfc3339_utc(),
                "requester_pid": pid,
            }),
        )?;
        return Ok(0);
    }

    // 5. Acquire OS single-instance lock
    let lock_record = FixtureLockRecord {
        pid,
        instance_id: cfg.instance_id.clone(),
        profile_id: env::var("DIM_PROFILE_ID").ok(),
        started_at: started_at_utc.clone(),
        app: "xf-desktop-fixture".to_owned(),
        lock_mode: cfg.lock_mode,
    };

    let Some(mut lock_guard) =
        OsFileLock::try_acquire(&lockfile_path, &zalo_data_dir, &lock_record)?
    else {
        append_line(
            &startup_log_path,
            &format!("SECOND_INSTANCE_REJECTED requester_pid={pid}"),
        );
        let _ = write_json_atomic(
            &focus_signal_path,
            &serde_json::json!({
                "focused_at": now_rfc3339_utc(),
                "requester_pid": pid,
            }),
        );
        return Ok(2);
    };

    // Clear stale signals before entering lifecycle
    let _ = fs::remove_file(&quit_signal_path);
    let _ = fs::remove_file(&focus_signal_path);

    // 6. Spawn child worker processes if requested
    let mut children = spawn_worker_children(cfg)?;
    let child_pids: Vec<u32> = children.iter().map(Child::id).collect();
    if !child_pids.is_empty() {
        let _ = write_json_atomic(
            &zalo_data_dir.join("children.json"),
            &serde_json::json!({
                "parent_pid": pid,
                "child_pids": child_pids,
            }),
        );
    }

    // 7. Simulated slow startup AFTER acquiring lockfile
    if cfg.slow_startup_post_lock_ms > 0 {
        thread::sleep(Duration::from_millis(cfg.slow_startup_post_lock_ms));
    }

    // 8. Write simulated Zalo account session, Chromium Local State, and seeded payload chunks
    let prod_dir = zalo_data_dir
        .join("Database")
        .join("_production")
        .join(&cfg.user_id);
    fs::create_dir_all(&prod_dir)?;
    write_json_atomic(
        &prod_dir.join("session.json"),
        &serde_json::json!({
            "zalo_user_id": cfg.user_id,
            "instance_id": cfg.instance_id,
            "pid": pid,
            "updated_at": now_rfc3339_utc(),
        }),
    )?;
    write_json_atomic(
        &zalo_data_dir.join("Local State"),
        &serde_json::json!({
            "os_crypt": { "xf_fixture": true },
            "instance_id": cfg.instance_id,
        }),
    )?;
    write_json_atomic(
        &zalo_data_dir.join("config.json"),
        &serde_json::json!({
            "fixture": true,
            "app_version": "xf-fixture-1.0.0",
            "instance_id": cfg.instance_id,
        }),
    )?;

    let payload_manifest = write_seeded_payload_chunks(&zalo_data_dir, cfg, pid)?;
    write_json_atomic(&files_manifest_path, &payload_manifest)?;

    // 9. Load and advance persistent state
    let prev: Option<FixturePersistentState> = fs::read_to_string(&state_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());
    let unclean_prev = prev.as_ref().is_some_and(|p| !p.clean_shutdown);
    let mut state = prev.unwrap_or_default();
    state.instance_id = cfg.instance_id.clone();
    state.profile_dir = profile_dir.display().to_string();
    state.zalo_data_dir = zalo_data_dir.display().to_string();
    state.boot_sequence = state.boot_sequence.saturating_add(1);
    state.unclean_previous_boot = unclean_prev;
    state.last_pid = pid;
    state.child_pids = child_pids.clone();
    state.pid_history.push(pid);
    if state.pid_history.len() > 32 {
        let excess = state.pid_history.len() - 32;
        state.pid_history.drain(0..excess);
    }
    state.started_at_utc = started_at_utc;
    state.ready_at_utc = now_rfc3339_utc();
    state.startup_duration_ms = u64::try_from(boot_timer.elapsed().as_millis()).unwrap_or(u64::MAX);
    state.clean_shutdown = false;
    state.crashed = false;
    state.crash_mode = None;
    state.zalo_user_id = cfg.user_id.clone();
    state.observed_env = capture_observed_env();
    for (k, v) in &cfg.persist_kv {
        state.persisted_kv.insert(k.clone(), v.clone());
    }
    write_json_atomic(&state_path, &state)?;
    sync_legacy_launch_state(&legacy_state_path, &state)?;

    append_line(
        &startup_log_path,
        &format!(
            "PRIMARY_STARTED pid={pid} boot={} instance_id={} startup_ms={}",
            state.boot_sequence, state.instance_id, state.startup_duration_ms
        ),
    );
    append_line(
        &wal_log_path,
        &format!(
            "BOOT_READY seq={} pid={pid} children={:?}",
            state.boot_sequence, child_pids
        ),
    );

    let banner = FixtureReadyBanner {
        event: "ready".to_owned(),
        instance_id: state.instance_id.clone(),
        pid,
        child_pids,
        profile_dir: state.profile_dir.clone(),
        zalo_data_dir: state.zalo_data_dir.clone(),
        boot_sequence: state.boot_sequence,
        startup_duration_ms: state.startup_duration_ms,
        zalo_user_id: state.zalo_user_id.clone(),
    };
    if let Ok(line) = serde_json::to_string(&banner) {
        let _ = writeln!(io::stdout(), "{line}");
        let _ = io::stdout().flush();
    }

    // 10. Post-lock crash modes (deliberately leave lockfiles / torn files intact!)
    match cfg.crash_mode {
        CrashMode::None | CrashMode::Immediate => {}
        CrashMode::AfterLock => {
            thread::sleep(Duration::from_millis(cfg.crash_delay_ms));
            state.crashed = true;
            state.crash_mode = Some(CrashMode::AfterLock);
            state.last_stopped_at = Some(now_rfc3339_utc());
            let _ = write_json_atomic(&state_path, &state);
            let _ = sync_legacy_launch_state(&legacy_state_path, &state);
            append_line(
                &startup_log_path,
                &format!("CRASH_SIMULATED mode=after-lock pid={pid}"),
            );
            if !cfg.orphan_children {
                terminate_children(&mut children);
            }
            std::process::exit(137);
        }
        CrashMode::MidWrite => {
            thread::sleep(Duration::from_millis(cfg.crash_delay_ms));
            let torn_tmp = zalo_data_dir.join("storage").join("torn_write.bin.tmp");
            if let Some(p) = torn_tmp.parent() {
                let _ = fs::create_dir_all(p);
            }
            if let Ok(mut f) = File::create(&torn_tmp) {
                let _ = f.write_all(b"PARTIAL_UNCOMMITTED_FRAME_");
            }
            state.crashed = true;
            state.crash_mode = Some(CrashMode::MidWrite);
            state.last_stopped_at = Some(now_rfc3339_utc());
            let _ = write_json_atomic(&state_path, &state);
            let _ = sync_legacy_launch_state(&legacy_state_path, &state);
            append_line(&wal_log_path, &format!("CRASH_MID_WRITE pid={pid}"));
            if !cfg.orphan_children {
                terminate_children(&mut children);
            }
            std::process::exit(138);
        }
        CrashMode::CorruptState => {
            thread::sleep(Duration::from_millis(cfg.crash_delay_ms));
            let _ = fs::write(
                zalo_data_dir.join("corrupt-cache.json"),
                b"{\"broken\": [1, 2, ",
            );
            state.crashed = true;
            state.crash_mode = Some(CrashMode::CorruptState);
            state.last_stopped_at = Some(now_rfc3339_utc());
            let _ = write_json_atomic(&state_path, &state);
            let _ = sync_legacy_launch_state(&legacy_state_path, &state);
            append_line(&wal_log_path, &format!("CRASH_CORRUPT_STATE pid={pid}"));
            if !cfg.orphan_children {
                terminate_children(&mut children);
            }
            std::process::exit(139);
        }
    }

    // 11. Event loop: monitor focus signals, quit signals, heartbeats, and stay_ms
    let loop_started = Instant::now();
    let mut last_heartbeat = Instant::now();

    loop {
        if focus_signal_path.exists() {
            let _ = fs::remove_file(&focus_signal_path);
            state.focus_count = state.focus_count.saturating_add(1);
            state.last_focused_at = Some(now_rfc3339_utc());
            let _ = write_json_atomic(&state_path, &state);
            let _ = sync_legacy_launch_state(&legacy_state_path, &state);
            append_line(
                &startup_log_path,
                &format!("FOCUS_RECEIVED count={}", state.focus_count),
            );
        }

        let quit_requested =
            quit_signal_path.exists() || SIGNAL_SHUTDOWN_REQUESTED.load(Ordering::SeqCst);
        if quit_requested {
            if cfg.hang_on_quit {
                // Acknowledge receipt in WAL so tests can prove the hang was triggered, then ignore!
                if quit_signal_path.exists() {
                    let _ = fs::remove_file(&quit_signal_path);
                }
                SIGNAL_SHUTDOWN_REQUESTED.store(false, Ordering::SeqCst);
                append_line(&wal_log_path, &format!("HANG_ON_QUIT_IGNORED pid={pid}"));
            } else {
                let _ = fs::remove_file(&quit_signal_path);
                if cfg.quit_delay_ms > 0 {
                    thread::sleep(Duration::from_millis(cfg.quit_delay_ms));
                }
                break;
            }
        }

        if let Some(stay) = cfg.stay_ms {
            if loop_started.elapsed() >= Duration::from_millis(stay) {
                break;
            }
        }

        if cfg.heartbeat_ms > 0
            && last_heartbeat.elapsed() >= Duration::from_millis(cfg.heartbeat_ms)
        {
            state.heartbeat_ticks = state.heartbeat_ticks.saturating_add(1);
            let _ = write_json_atomic(&state_path, &state);
            append_line(
                &wal_log_path,
                &format!("HEARTBEAT tick={} pid={pid}", state.heartbeat_ticks),
            );
            last_heartbeat = Instant::now();
        }

        thread::sleep(Duration::from_millis(25));
    }

    // 12. Graceful shutdown: terminate owned children, persist clean state, release OS lock
    if !cfg.orphan_children {
        terminate_children(&mut children);
    }

    state.clean_shutdown = true;
    state.clean_shutdown_count = state.clean_shutdown_count.saturating_add(1);
    state.last_stopped_at = Some(now_rfc3339_utc());
    let _ = write_json_atomic(&state_path, &state);
    let _ = sync_legacy_launch_state(&legacy_state_path, &state);
    append_line(
        &startup_log_path,
        &format!("PRIMARY_STOPPED pid={pid} code=0"),
    );
    append_line(
        &wal_log_path,
        &format!("CLEAN_SHUTDOWN seq={} pid={pid}", state.boot_sequence),
    );

    lock_guard.release_and_unlink();
    let _ = fs::remove_file(zalo_data_dir.join("children.json"));
    Ok(0)
}

fn run_child_worker(parent_pid: u32, cfg: &FixtureCliConfig) -> io::Result<u8> {
    let (_, zalo_data_dir) = resolve_fixture_directories(cfg);
    let _ = fs::create_dir_all(&zalo_data_dir);
    let worker_marker = zalo_data_dir.join(format!("worker-{}.pid", cfg.worker_index));
    let _ = fs::write(&worker_marker, std::process::id().to_string());

    let started = Instant::now();
    loop {
        if SIGNAL_SHUTDOWN_REQUESTED.load(Ordering::SeqCst) && !cfg.hang_on_quit {
            break;
        }
        if !cfg.orphan_children && !is_os_pid_alive(parent_pid) {
            break;
        }
        if let Some(stay) = cfg.stay_ms {
            if started.elapsed() >= Duration::from_millis(stay.saturating_add(5_000)) {
                break;
            }
        } else if started.elapsed() >= Duration::from_secs(120) {
            // Hard safety ceiling so orphan test workers can never leak indefinitely on a developer machine.
            break;
        }
        thread::sleep(Duration::from_millis(40));
    }

    let _ = fs::remove_file(&worker_marker);
    Ok(0)
}

fn spawn_worker_children(cfg: &FixtureCliConfig) -> io::Result<Vec<Child>> {
    if cfg.spawn_children == 0 {
        return Ok(Vec::new());
    }
    let self_exe = env::current_exe()?;
    let parent_pid = std::process::id().to_string();
    let (profile_dir, zalo_data_dir) = resolve_fixture_directories(cfg);
    let mut out = Vec::with_capacity(cfg.spawn_children);

    for idx in 0..cfg.spawn_children {
        let mut cmd = Command::new(&self_exe);
        cmd.arg("--worker-of")
            .arg(&parent_pid)
            .arg("--worker-index")
            .arg(idx.to_string())
            .arg("--profile-dir")
            .arg(&profile_dir)
            .arg("--user-data-dir")
            .arg(&zalo_data_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if cfg.orphan_children {
            cmd.arg("--orphan-children");
        }
        if cfg.hang_on_quit {
            cmd.arg("--hang-on-quit");
        }
        if let Some(stay) = cfg.stay_ms {
            cmd.arg("--stay-ms").arg(stay.to_string());
        }
        out.push(cmd.spawn()?);
    }
    Ok(out)
}

fn terminate_children(children: &mut [Child]) {
    for child in children {
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn write_seeded_payload_chunks(
    zalo_data_dir: &Path,
    cfg: &FixtureCliConfig,
    pid: u32,
) -> io::Result<FixturePayloadManifest> {
    let storage_dir = zalo_data_dir.join("storage");
    fs::create_dir_all(&storage_dir)?;

    let mut mix_seed = cfg.seed;
    for b in cfg.instance_id.as_bytes() {
        mix_seed = mix_seed
            .wrapping_mul(1_099_511_628_211)
            .wrapping_add(u64::from(*b));
    }
    let mut rng = SplitMix64::new(mix_seed);
    let mut chunks = Vec::with_capacity(cfg.write_files);
    let mut total_bytes = 0u64;

    for idx in 0..cfg.write_files {
        if cfg.slow_io_ms > 0 {
            thread::sleep(Duration::from_millis(cfg.slow_io_ms));
        }
        let mut buf = vec![0u8; cfg.write_bytes];
        rng.fill_bytes(&mut buf);
        let rel = format!("storage/chunk_{idx:04}.bin");
        let full = zalo_data_dir.join(&rel);
        write_bytes_atomic(&full, &buf)?;
        let digest = sha256_bytes(&buf);
        let len = buf.len() as u64;
        total_bytes = total_bytes.saturating_add(len);
        chunks.push(FixtureChunkEntry {
            relative_path: rel,
            size_bytes: len,
            sha256: digest,
        });
    }

    Ok(FixturePayloadManifest {
        instance_id: cfg.instance_id.clone(),
        pid,
        seed: format!("0x{mix_seed:016x}"),
        total_bytes,
        chunks,
    })
}

fn sync_legacy_launch_state(path: &Path, state: &FixturePersistentState) -> io::Result<()> {
    let legacy = LegacyLaunchState {
        launch_count: state.boot_sequence,
        focus_count: state.focus_count,
        last_pid: state.last_pid,
        last_started_at: state.started_at_utc.clone(),
        last_focused_at: state.last_focused_at.clone(),
        last_stopped_at: state.last_stopped_at.clone(),
        clean_shutdown: state.clean_shutdown,
        crashed: state.crashed,
        zalo_user_id: state.zalo_user_id.clone(),
    };
    write_json_atomic(path, &legacy)
}

fn write_json_atomic<T: Serialize>(path: &Path, val: &T) -> io::Result<()> {
    let json = serde_json::to_string_pretty(val)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    write_bytes_atomic(path, format!("{json}\n").as_bytes())
}

fn write_bytes_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("payload");
    let tmp = path.with_file_name(format!(".{file_name}.tmp.{}", std::process::id()));
    {
        let mut f = File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

fn append_line(path: &Path, line: &str) {
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{} {line}", now_rfc3339_utc());
    }
}

/// Computes lowercase hex SHA-256 of an in-memory buffer.
#[must_use]
pub fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for b in digest {
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

/// Computes lowercase hex SHA-256 of a file on disk.
pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut f = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 32_768];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for b in digest {
        let _ = write!(&mut out, "{b:02x}");
    }
    Ok(out)
}

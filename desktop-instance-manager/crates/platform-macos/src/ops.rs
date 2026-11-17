//! Injectable operating-system seam (`MacOsSystemOps`) separating pure Rust adapter logic from
//! macOS CLI tools (`open`, `codesign`, `spctl`, `plutil`, `mdfind`, `ps`, `osascript`), POSIX
//! syscalls (`libc`), cross-platform live fixture processes, and deterministic unit-test mocks.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use dim_core::{DimError, MIN_FREE_DISK_BYTES};

/// Sidecar filename used by [`HostFixtureMacOsOps`] on non-Unix hosts to simulate POSIX directory modes.
pub const SIMULATED_POSIX_MODE_FILE: &str = ".dim-simulated-posix-mode";

/// Captured status and standard streams from an external command invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    /// Process exit code (`Some(0)` indicates success).
    pub status_code: Option<i32>,
    /// Captured `stdout` bytes.
    pub stdout: Vec<u8>,
    /// Captured `stderr` bytes.
    pub stderr: Vec<u8>,
}

impl CommandOutput {
    /// Returns `true` when `status_code == Some(0)`.
    #[must_use]
    pub const fn success(&self) -> bool {
        matches!(self.status_code, Some(0))
    }

    /// Converts `stdout` to lossy UTF-8 text.
    #[must_use]
    pub fn stdout_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    /// Converts `stderr` to lossy UTF-8 text.
    #[must_use]
    pub fn stderr_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }

    /// Concatenates `stdout` and `stderr` for tools like `codesign -dv` that emit metadata on `stderr`.
    #[must_use]
    pub fn combined_lossy(&self) -> String {
        let out = self.stdout_lossy();
        let err = self.stderr_lossy();
        if out.is_empty() {
            err
        } else if err.is_empty() {
            out
        } else {
            format!("{out}\n{err}")
        }
    }
}

/// Single live process row parsed from `ps` or the cross-platform fixture process registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessEntry {
    /// Process identifier (`PID`).
    pub pid: u32,
    /// Parent process identifier (`PPID`).
    pub ppid: u32,
    /// Effective owner user ID (`UID`).
    pub uid: u32,
    /// Resident set size in KiB (`RSS`).
    pub rss_kib: u64,
    /// Kernel process start timestamp token (`ps -o lstart=` on Darwin/Unix, `FILETIME` on Windows fixture host)
    /// used to defeat PID-reuse misattribution across crashes or reboots.
    pub start_token: Option<String>,
    /// Short executable name or path (`comm` / extracted binary).
    pub executable: String,
    /// Full command line (`args`).
    pub args: String,
}

/// Visible or minimized application window associated with an owning PID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowInfo {
    /// macOS `CGWindowID` (`kCGWindowNumber`) or synthetic fixture window identifier.
    pub window_id: u64,
    /// Owning process PID (`kCGWindowOwnerPID`).
    pub owner_pid: u32,
    /// Window title if accessible under macOS Screen Recording / Accessibility rules or fixture state.
    pub title: Option<String>,
    /// Whether the window is currently visible on screen (layer 0).
    pub is_on_screen: bool,
}

/// Parameters for launching a detached child process without inherited standard I/O handles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnDetachedRequest {
    /// Executable path or binary name.
    pub program: PathBuf,
    /// Discrete command-line arguments (never shell-concatenated).
    pub args: Vec<String>,
    /// Environment variable overrides applied to the child process.
    pub envs: Vec<(String, String)>,
    /// Optional working directory for the child process.
    pub current_dir: Option<PathBuf>,
}

/// Operating-system abstraction enabling native macOS execution, cross-platform fixture runs,
/// and deterministic unit-test simulation.
pub trait MacOsSystemOps: std::fmt::Debug + Send + Sync {
    /// Returns `true` when backed by real macOS system tools (`open`, `codesign`, `osascript`).
    fn is_native_macos(&self) -> bool;

    /// Returns `true` when runtime lifecycle operations (`launch`, `inspect`, `focus`, `stop`) are supported.
    fn supports_runtime_ops(&self) -> bool;

    /// Returns the CPU architecture identifier (`"aarch64"`, `"x86_64"`, etc.).
    fn host_arch(&self) -> &'static str;

    /// Returns the current effective POSIX user ID (`geteuid()`), or a deterministic UID on non-Unix hosts.
    fn current_uid(&self) -> u32;

    /// Returns whether the current process is running as `root` (`euid == 0`), if known.
    fn is_elevated(&self) -> Option<bool>;

    /// Resolves the real login user's home directory (`~/`) outside isolated profile overrides.
    fn user_home_dir(&self) -> Option<PathBuf>;

    /// Base directory for short `SUN_LEN`-safe temporary socket directories (`/tmp` on macOS).
    fn short_temp_root(&self) -> PathBuf;

    /// Returns free bytes available to the unprivileged user at `path`.
    fn free_disk_bytes(&self, path: &Path) -> Result<u64, DimError>;

    /// Checks whether `dir` is owned by the current user and denies group/world write access (`0o022 == 0`).
    fn check_dir_permissions_restrictive(&self, dir: &Path) -> Option<bool>;

    /// Creates `dir` (and parents) as a real non-symlink directory and enforces `0o700` owner-only permissions.
    fn ensure_private_dir(&self, dir: &Path) -> Result<(), DimError>;

    /// Executes a short-lived external utility synchronously with discrete arguments.
    fn run_command(
        &self,
        program: &Path,
        args: &[String],
        envs: &[(String, String)],
    ) -> Result<CommandOutput, DimError>;

    /// Spawns a detached long-running child process and returns its OS PID.
    fn spawn_detached(&self, req: &SpawnDetachedRequest) -> Result<u32, DimError>;

    /// Enumerates live processes on the host (including `start_token` when supported).
    fn list_processes(&self) -> Result<Vec<ProcessEntry>, DimError>;

    /// Queries the kernel process creation timestamp token for `pid`, if live and accessible.
    fn process_start_token(&self, pid: u32) -> Option<String> {
        self.list_processes()
            .ok()?
            .into_iter()
            .find(|p| p.pid == pid)
            .and_then(|p| p.start_token)
    }

    /// Probes whether `pid` currently refers to a live (non-zombie) OS process.
    fn is_pid_alive(&self, pid: u32) -> bool;

    /// Sends graceful `SIGTERM` to a verified isolated non-primary `pid`.
    fn send_sigterm(&self, pid: u32) -> Result<bool, DimError>;

    /// Queries visible windows owned by any PID in `pids`.
    fn query_windows_for_pids(&self, pids: &[u32]) -> Vec<WindowInfo>;
}

/// Production [`MacOsSystemOps`] implementation invoking real Darwin/POSIX APIs and macOS system utilities.
#[derive(Debug, Default, Clone, Copy)]
pub struct NativeMacOsSystemOps;

impl MacOsSystemOps for NativeMacOsSystemOps {
    fn is_native_macos(&self) -> bool {
        cfg!(target_os = "macos")
    }

    fn supports_runtime_ops(&self) -> bool {
        cfg!(target_os = "macos")
    }

    fn host_arch(&self) -> &'static str {
        std::env::consts::ARCH
    }

    fn current_uid(&self) -> u32 {
        #[cfg(unix)]
        {
            // SAFETY: `geteuid` is a thread-safe POSIX syscall with no preconditions.
            unsafe { libc::geteuid() }
        }
        #[cfg(not(unix))]
        {
            501
        }
    }

    fn is_elevated(&self) -> Option<bool> {
        #[cfg(unix)]
        {
            Some(self.current_uid() == 0)
        }
        #[cfg(not(unix))]
        {
            None
        }
    }

    fn user_home_dir(&self) -> Option<PathBuf> {
        #[cfg(unix)]
        if let Some(pw_home) = lookup_passwd_home_dir(self.current_uid()) {
            return Some(pw_home);
        }
        std::env::var_os("HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    }

    fn short_temp_root(&self) -> PathBuf {
        PathBuf::from("/tmp")
    }

    fn free_disk_bytes(&self, path: &Path) -> Result<u64, DimError> {
        #[cfg(unix)]
        {
            unix_free_disk_bytes(path)
        }
        #[cfg(not(unix))]
        {
            let _ = path;
            Ok(MIN_FREE_DISK_BYTES.saturating_mul(2))
        }
    }

    fn check_dir_permissions_restrictive(&self, dir: &Path) -> Option<bool> {
        #[cfg(unix)]
        {
            unix_check_dir_restrictive(dir, self.current_uid())
        }
        #[cfg(not(unix))]
        {
            let _ = dir;
            None
        }
    }

    fn ensure_private_dir(&self, dir: &Path) -> Result<(), DimError> {
        #[cfg(unix)]
        {
            unix_ensure_private_dir(dir, self.current_uid())
        }
        #[cfg(not(unix))]
        {
            reject_symlink_target(dir)?;
            fs::create_dir_all(dir)?;
            Ok(())
        }
    }

    fn run_command(
        &self,
        program: &Path,
        args: &[String],
        envs: &[(String, String)],
    ) -> Result<CommandOutput, DimError> {
        let mut cmd = Command::new(program);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in envs {
            cmd.env(k, v);
        }
        let out = cmd.output()?;
        Ok(CommandOutput {
            status_code: out.status.code(),
            stdout: out.stdout,
            stderr: out.stderr,
        })
    }

    fn spawn_detached(&self, req: &SpawnDetachedRequest) -> Result<u32, DimError> {
        spawn_detached_os_process(req)
    }

    fn list_processes(&self) -> Result<Vec<ProcessEntry>, DimError> {
        let args = vec![
            "-axo".to_owned(),
            "pid=,ppid=,uid=,stat=,rss=,lstart=,args=".to_owned(),
        ];
        let out = self.run_command(Path::new("/bin/ps"), &args, &[])?;
        if !out.success() {
            return Err(DimError::ProcessError(format!(
                "`/bin/ps` failed with status {:?}: {}",
                out.status_code,
                out.stderr_lossy().trim()
            )));
        }
        Ok(crate::process::parse_ps_table(&out.stdout_lossy()))
    }

    fn is_pid_alive(&self, pid: u32) -> bool {
        if pid == 0 {
            return false;
        }
        #[cfg(unix)]
        {
            if let Ok(procs) = self.list_processes() {
                return procs.iter().any(|p| p.pid == pid);
            }
            unix_kill_zero_alive(pid)
        }
        #[cfg(not(unix))]
        {
            false
        }
    }

    fn send_sigterm(&self, pid: u32) -> Result<bool, DimError> {
        #[cfg(unix)]
        {
            unix_send_sigterm(pid)
        }
        #[cfg(not(unix))]
        {
            let _ = pid;
            Err(DimError::UnsupportedPlatform(
                "POSIX SIGTERM requires a Unix/macOS host".to_owned(),
            ))
        }
    }

    fn query_windows_for_pids(&self, pids: &[u32]) -> Vec<WindowInfo> {
        if pids.is_empty() || !cfg!(target_os = "macos") {
            return Vec::new();
        }
        crate::window::query_macos_windows_via_osascript(self, pids)
    }
}

/// Default non-macOS host fallback that keeps pure-Rust discovery/layout helpers available while
/// honestly returning `DimError::UnsupportedPlatform` for native macOS runtime operations.
#[derive(Debug, Default, Clone, Copy)]
pub struct UnsupportedHostSystemOps;

impl MacOsSystemOps for UnsupportedHostSystemOps {
    fn is_native_macos(&self) -> bool {
        false
    }

    fn supports_runtime_ops(&self) -> bool {
        false
    }

    fn host_arch(&self) -> &'static str {
        std::env::consts::ARCH
    }

    fn current_uid(&self) -> u32 {
        #[cfg(unix)]
        {
            // SAFETY: `geteuid` has no memory preconditions.
            unsafe { libc::geteuid() }
        }
        #[cfg(not(unix))]
        {
            501
        }
    }

    fn is_elevated(&self) -> Option<bool> {
        None
    }

    fn user_home_dir(&self) -> Option<PathBuf> {
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    }

    fn short_temp_root(&self) -> PathBuf {
        std::env::temp_dir()
    }

    fn free_disk_bytes(&self, _path: &Path) -> Result<u64, DimError> {
        Ok(MIN_FREE_DISK_BYTES.saturating_mul(2))
    }

    fn check_dir_permissions_restrictive(&self, _dir: &Path) -> Option<bool> {
        None
    }

    fn ensure_private_dir(&self, dir: &Path) -> Result<(), DimError> {
        reject_symlink_target(dir)?;
        fs::create_dir_all(dir)?;
        Ok(())
    }

    fn run_command(
        &self,
        _program: &Path,
        _args: &[String],
        _envs: &[(String, String)],
    ) -> Result<CommandOutput, DimError> {
        Err(DimError::UnsupportedPlatform(
            "macOS command execution requires macOS".to_owned(),
        ))
    }

    fn spawn_detached(&self, _req: &SpawnDetachedRequest) -> Result<u32, DimError> {
        Err(DimError::UnsupportedPlatform(
            "macOS launch requires macOS".to_owned(),
        ))
    }

    fn list_processes(&self) -> Result<Vec<ProcessEntry>, DimError> {
        Err(DimError::UnsupportedPlatform(
            "macOS process enumeration requires macOS".to_owned(),
        ))
    }

    fn is_pid_alive(&self, _pid: u32) -> bool {
        false
    }

    fn send_sigterm(&self, _pid: u32) -> Result<bool, DimError> {
        Err(DimError::UnsupportedPlatform(
            "macOS stop requires macOS".to_owned(),
        ))
    }

    fn query_windows_for_pids(&self, _pids: &[u32]) -> Vec<WindowInfo> {
        Vec::new()
    }
}

#[derive(Debug, Clone)]
struct TrackedFixtureProcess {
    entry: ProcessEntry,
}

/// Cross-platform live fixture [`MacOsSystemOps`] implementation allowing end-to-end multi-session
/// Node.js / custom-executable fixture verification on Windows, Linux, and macOS hosts.
#[derive(Debug)]
pub struct HostFixtureMacOsOps {
    simulated_home: PathBuf,
    short_tmp_root: PathBuf,
    uid: u32,
    elevated: bool,
    free_bytes_override: Mutex<Option<u64>>,
    tracked_processes: Mutex<HashMap<u32, TrackedFixtureProcess>>,
    foreign_processes: Mutex<Vec<ProcessEntry>>,
}

impl HostFixtureMacOsOps {
    /// Creates a live fixture runner rooted at `simulated_home` and `short_tmp_root`.
    #[must_use]
    pub fn new(simulated_home: PathBuf, short_tmp_root: PathBuf) -> Self {
        Self {
            simulated_home,
            short_tmp_root,
            uid: 501,
            elevated: false,
            free_bytes_override: Mutex::new(None),
            tracked_processes: Mutex::new(HashMap::new()),
            foreign_processes: Mutex::new(Vec::new()),
        }
    }

    /// Overrides available free disk bytes for low-disk testing.
    pub fn set_free_bytes_override(&self, bytes: Option<u64>) {
        if let Ok(mut guard) = self.free_bytes_override.lock() {
            *guard = bytes;
        }
    }

    /// Registers a synthetic foreign or primary process entry (e.g. to test recycled PID or primary protection).
    pub fn insert_synthetic_process(&self, entry: ProcessEntry) {
        if let Ok(mut guard) = self.foreign_processes.lock() {
            guard.retain(|p| p.pid != entry.pid);
            guard.push(entry);
        }
    }

    /// Removes a synthetic process entry by PID.
    pub fn remove_synthetic_process(&self, pid: u32) {
        if let Ok(mut guard) = self.foreign_processes.lock() {
            guard.retain(|p| p.pid != pid);
        }
    }

    /// Registers a live externally-spawned helper process (e.g. an orphan helper recorded by `fixture-macos-app.mjs`)
    /// so `HostFixtureMacOsOps` tracks it on non-Unix hosts where `/bin/ps` is absent.
    pub fn register_live_external_pid(&self, pid: u32, ppid: u32, executable: &str, args: &str) {
        if !host_os_pid_alive(pid) {
            return;
        }
        let entry = ProcessEntry {
            pid,
            ppid,
            uid: self.uid,
            rss_kib: 24_576,
            start_token: host_os_process_start_token(pid),
            executable: executable.to_owned(),
            args: args.to_owned(),
        };
        if let Ok(mut guard) = self.tracked_processes.lock() {
            guard.insert(
                pid,
                TrackedFixtureProcess {
                    entry: entry.clone(),
                },
            );
        }
        self.save_shared_entry(&entry);
    }

    fn registry_path(&self) -> PathBuf {
        self.short_tmp_root.join(".dim-fixture-proc-table.json")
    }

    fn load_shared_entries(&self) -> Vec<ProcessEntry> {
        let mut by_pid: HashMap<u32, ProcessEntry> = HashMap::new();
        if let Ok(raw) = fs::read_to_string(self.registry_path()) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(arr) = val.as_array() {
                    for item in arr {
                        let Some(pid) = item
                            .get("pid")
                            .and_then(serde_json::Value::as_u64)
                            .and_then(|n| u32::try_from(n).ok())
                        else {
                            continue;
                        };
                        if !host_os_pid_alive(pid) {
                            continue;
                        }
                        let recorded_token = item
                            .get("start_token")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_owned);
                        let live_token = host_os_process_start_token(pid);
                        // If the OS recycled `pid` for a newer process, drop the stale shared table entry!
                        if let (Some(ref rec), Some(ref live)) = (&recorded_token, &live_token) {
                            if rec != live {
                                continue;
                            }
                        }
                        let ppid = item
                            .get("ppid")
                            .and_then(serde_json::Value::as_u64)
                            .and_then(|n| u32::try_from(n).ok())
                            .unwrap_or(1);
                        let uid = item
                            .get("uid")
                            .and_then(serde_json::Value::as_u64)
                            .and_then(|n| u32::try_from(n).ok())
                            .unwrap_or(self.uid);
                        let rss_kib = item
                            .get("rss_kib")
                            .and_then(serde_json::Value::as_u64)
                            .unwrap_or(32_768);
                        let executable = item
                            .get("executable")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("node")
                            .to_owned();
                        let args = item
                            .get("args")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default()
                            .to_owned();
                        by_pid.insert(
                            pid,
                            ProcessEntry {
                                pid,
                                ppid,
                                uid,
                                rss_kib,
                                start_token: live_token.or(recorded_token),
                                executable,
                                args,
                            },
                        );
                    }
                }
            }
        }
        if let Ok(mut guard) = self.tracked_processes.lock() {
            guard.retain(|&pid, tracked| {
                if !host_os_pid_alive(pid) {
                    return false;
                }
                if let (Some(ref rec), Some(ref live)) = (
                    &tracked.entry.start_token,
                    &host_os_process_start_token(pid),
                ) {
                    if rec != live {
                        return false;
                    }
                }
                true
            });
            for (pid, tracked) in guard.iter() {
                by_pid.insert(*pid, tracked.entry.clone());
            }
        }
        if let Ok(foreign) = self.foreign_processes.lock() {
            for entry in foreign.iter() {
                by_pid.insert(entry.pid, entry.clone());
            }
        }
        let mut out: Vec<ProcessEntry> = by_pid.into_values().collect();
        out.sort_by_key(|e| e.pid);
        out
    }

    fn save_shared_entry(&self, entry: &ProcessEntry) {
        let _ = fs::create_dir_all(&self.short_tmp_root);
        let mut entries = self.load_shared_entries();
        entries.retain(|e| e.pid != entry.pid);
        entries.push(entry.clone());
        let json_items: Vec<serde_json::Value> = entries
            .iter()
            .map(|e| {
                serde_json::json!({
                    "pid": e.pid,
                    "ppid": e.ppid,
                    "uid": e.uid,
                    "rss_kib": e.rss_kib,
                    "start_token": e.start_token,
                    "executable": e.executable,
                    "args": e.args,
                })
            })
            .collect();
        if let Ok(text) = serde_json::to_string_pretty(&json_items) {
            let tmp = self.short_tmp_root.join(format!(
                ".dim-fixture-proc-table.json.tmp.{}",
                std::process::id()
            ));
            if fs::write(&tmp, text).is_ok() {
                let _ = fs::rename(&tmp, self.registry_path());
            }
        }
    }
}

impl MacOsSystemOps for HostFixtureMacOsOps {
    fn is_native_macos(&self) -> bool {
        cfg!(target_os = "macos")
    }

    fn supports_runtime_ops(&self) -> bool {
        true
    }

    fn host_arch(&self) -> &'static str {
        std::env::consts::ARCH
    }

    fn current_uid(&self) -> u32 {
        self.uid
    }

    fn is_elevated(&self) -> Option<bool> {
        Some(self.elevated)
    }

    fn user_home_dir(&self) -> Option<PathBuf> {
        Some(self.simulated_home.clone())
    }

    fn short_temp_root(&self) -> PathBuf {
        self.short_tmp_root.clone()
    }

    fn free_disk_bytes(&self, path: &Path) -> Result<u64, DimError> {
        if let Ok(guard) = self.free_bytes_override.lock() {
            if let Some(bytes) = *guard {
                return Ok(bytes);
            }
        }
        #[cfg(unix)]
        {
            unix_free_disk_bytes(path)
        }
        #[cfg(not(unix))]
        {
            let _ = path;
            Ok(MIN_FREE_DISK_BYTES.saturating_mul(4))
        }
    }

    fn check_dir_permissions_restrictive(&self, dir: &Path) -> Option<bool> {
        let probe = if fs::symlink_metadata(dir).is_ok() {
            dir
        } else {
            dir.parent().unwrap_or(dir)
        };
        if let Ok(meta) = fs::symlink_metadata(probe) {
            if meta.file_type().is_symlink() {
                return Some(false);
            }
        }
        let sidecar = probe.join(SIMULATED_POSIX_MODE_FILE);
        if let Ok(raw) = fs::read_to_string(sidecar) {
            if let Ok(mode) = u32::from_str_radix(raw.trim().trim_start_matches("0o"), 8) {
                return Some((mode & 0o022) == 0);
            }
        }
        #[cfg(unix)]
        {
            unix_check_dir_restrictive(probe, self.uid)
        }
        #[cfg(not(unix))]
        {
            Some(true)
        }
    }

    fn ensure_private_dir(&self, dir: &Path) -> Result<(), DimError> {
        #[cfg(unix)]
        {
            unix_ensure_private_dir(dir, self.uid)
        }
        #[cfg(not(unix))]
        {
            reject_symlink_target(dir)?;
            if let Some(bad_mode) = read_simulated_unwritable_or_unsafe_mode(dir) {
                if (bad_mode & 0o200) == 0 || (bad_mode & 0o077) != 0 {
                    return Err(DimError::Io(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        format!(
                            "simulated POSIX permission denied or insecure mode (0o{bad_mode:03o}) on {}",
                            dir.display()
                        ),
                    )));
                }
            }
            fs::create_dir_all(dir)?;
            Ok(())
        }
    }

    fn run_command(
        &self,
        program: &Path,
        args: &[String],
        envs: &[(String, String)],
    ) -> Result<CommandOutput, DimError> {
        let prog_str = program.to_string_lossy();
        if !cfg!(target_os = "macos") && prog_str.ends_with("osascript") {
            return Ok(CommandOutput {
                status_code: Some(0),
                stdout: b"status=ok activated=true active=true\n".to_vec(),
                stderr: Vec::new(),
            });
        }
        if !cfg!(target_os = "macos") && prog_str == "/usr/bin/open" {
            return simulate_open_command_on_fixture_host(self, args);
        }

        let mut cmd = Command::new(program);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in envs {
            cmd.env(k, v);
        }
        let out = cmd.output()?;
        Ok(CommandOutput {
            status_code: out.status.code(),
            stdout: out.stdout,
            stderr: out.stderr,
        })
    }

    fn spawn_detached(&self, req: &SpawnDetachedRequest) -> Result<u32, DimError> {
        let pid = spawn_detached_os_process(req)?;
        let full_args = std::iter::once(req.program.to_string_lossy().into_owned())
            .chain(req.args.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ");
        let start_token = host_os_process_start_token(pid);
        let entry = ProcessEntry {
            pid,
            ppid: std::process::id(),
            uid: self.uid,
            rss_kib: 40_960,
            start_token,
            executable: req.program.to_string_lossy().into_owned(),
            args: full_args,
        };
        if let Ok(mut guard) = self.tracked_processes.lock() {
            guard.insert(
                pid,
                TrackedFixtureProcess {
                    entry: entry.clone(),
                },
            );
        }
        self.save_shared_entry(&entry);
        Ok(pid)
    }

    fn list_processes(&self) -> Result<Vec<ProcessEntry>, DimError> {
        #[cfg(target_os = "macos")]
        {
            NativeMacOsSystemOps.list_processes()
        }
        #[cfg(not(target_os = "macos"))]
        {
            Ok(self.load_shared_entries())
        }
    }

    fn process_start_token(&self, pid: u32) -> Option<String> {
        if let Ok(foreign) = self.foreign_processes.lock() {
            if let Some(found) = foreign.iter().find(|p| p.pid == pid) {
                return found.start_token.clone();
            }
        }
        host_os_process_start_token(pid)
    }

    fn is_pid_alive(&self, pid: u32) -> bool {
        if pid == 0 {
            return false;
        }
        if let Ok(foreign) = self.foreign_processes.lock() {
            if foreign.iter().any(|p| p.pid == pid) {
                return true;
            }
        }
        host_os_pid_alive(pid)
    }

    fn send_sigterm(&self, pid: u32) -> Result<bool, DimError> {
        let _ = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.short_tmp_root.join(".fixture-sigterm.log"))
            .and_then(|mut f| std::io::Write::write_all(&mut f, format!("{pid}\n").as_bytes()));
        #[cfg(not(unix))]
        {
            if self
                .load_shared_entries()
                .iter()
                .any(|e| e.pid == pid && e.args.contains("--hang-on-quit"))
            {
                // Simulate POSIX catchable SIGTERM ignored (`SIG_IGN`) by a hung process.
                return Ok(host_os_pid_alive(pid));
            }
        }
        terminate_host_os_pid(pid)
    }

    fn query_windows_for_pids(&self, _pids: &[u32]) -> Vec<WindowInfo> {
        Vec::new()
    }
}

/// Recorded command call inside [`MockMacOsSystemOps`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedCommand {
    /// Invoked program path.
    pub program: PathBuf,
    /// Discrete arguments passed to `program`.
    pub args: Vec<String>,
    /// Environment overrides supplied to `program`.
    pub envs: Vec<(String, String)>,
}

/// Command handler callback used by [`MockMacOsSystemOps`].
pub type MockCommandHandler = Arc<
    dyn Fn(&Path, &[String], &[(String, String)]) -> Result<CommandOutput, DimError> + Send + Sync,
>;

/// Custom `SIGTERM` behavior hook used by [`MockMacOsSystemOps`] (e.g. to simulate hung processes).
pub type MockSigtermHandler = Arc<dyn Fn(u32) -> Result<bool, DimError> + Send + Sync>;

/// Deterministic in-memory [`MacOsSystemOps`] test double for verifying every macOS code path
/// (`codesign`, `spctl`, `plutil`, `mdfind`, `open`, `ps`, `osascript`, UID/mode checks) on any host.
pub struct MockMacOsSystemOps {
    /// Whether `is_native_macos()` reports `true`.
    pub native_macos: bool,
    /// Simulated CPU architecture (`"aarch64"` or `"x86_64"`).
    pub arch: &'static str,
    /// Simulated effective UID (`501` by default).
    pub uid: u32,
    /// Simulated root elevation state.
    pub elevated: Option<bool>,
    /// Simulated login user home directory.
    pub home_dir: Option<PathBuf>,
    /// Simulated short socket directory root (`/tmp` or temp folder).
    pub short_tmp_root: PathBuf,
    /// Simulated free disk space in bytes.
    pub free_bytes: u64,
    /// Simulated `check_profiles_acl_safe` result.
    pub acl_safe: Option<bool>,
    /// Simulated process table returned by `list_processes()`.
    pub processes: Mutex<Vec<ProcessEntry>>,
    /// Simulated windows returned by `query_windows_for_pids()`.
    pub windows: Mutex<Vec<WindowInfo>>,
    /// Recorded command invocations in chronological order.
    pub recorded_commands: Mutex<Vec<RecordedCommand>>,
    /// Recorded PIDs sent `SIGTERM`.
    pub sigterm_pids: Mutex<Vec<u32>>,
    /// Next PID allocated by `spawn_detached`.
    pub next_spawn_pid: Mutex<u32>,
    /// Optional custom command responder.
    pub command_handler: Option<MockCommandHandler>,
    /// Optional custom `SIGTERM` responder (e.g. simulating `--hang-on-quit` unkillable process).
    pub sigterm_handler: Option<MockSigtermHandler>,
}

impl std::fmt::Debug for MockMacOsSystemOps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockMacOsSystemOps")
            .field("native_macos", &self.native_macos)
            .field("arch", &self.arch)
            .field("uid", &self.uid)
            .finish()
    }
}

impl MockMacOsSystemOps {
    /// Creates a new deterministic mock configured as an Apple Silicon (`aarch64`) macOS user (`uid = 501`).
    #[must_use]
    pub fn new_macos(home_dir: PathBuf, short_tmp_root: PathBuf) -> Self {
        Self {
            native_macos: true,
            arch: "aarch64",
            uid: 501,
            elevated: Some(false),
            home_dir: Some(home_dir),
            short_tmp_root,
            free_bytes: MIN_FREE_DISK_BYTES.saturating_mul(4),
            acl_safe: Some(true),
            processes: Mutex::new(Vec::new()),
            windows: Mutex::new(Vec::new()),
            recorded_commands: Mutex::new(Vec::new()),
            sigterm_pids: Mutex::new(Vec::new()),
            next_spawn_pid: Mutex::new(41_000),
            command_handler: None,
            sigterm_handler: None,
        }
    }

    /// Attaches a custom command handler closure.
    #[must_use]
    pub fn with_command_handler(mut self, handler: MockCommandHandler) -> Self {
        self.command_handler = Some(handler);
        self
    }

    /// Attaches a custom `SIGTERM` handler closure.
    #[must_use]
    pub fn with_sigterm_handler(mut self, handler: MockSigtermHandler) -> Self {
        self.sigterm_handler = Some(handler);
        self
    }

    /// Replaces the simulated process table.
    pub fn set_processes(&self, entries: Vec<ProcessEntry>) {
        if let Ok(mut guard) = self.processes.lock() {
            *guard = entries;
        }
    }

    /// Replaces the simulated window list.
    pub fn set_windows(&self, entries: Vec<WindowInfo>) {
        if let Ok(mut guard) = self.windows.lock() {
            *guard = entries;
        }
    }

    /// Returns a snapshot of all recorded command invocations.
    #[must_use]
    pub fn commands(&self) -> Vec<RecordedCommand> {
        self.recorded_commands
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    /// Returns a snapshot of all PIDs that received `SIGTERM`.
    #[must_use]
    pub fn terminated_pids(&self) -> Vec<u32> {
        self.sigterm_pids
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }
}

impl MacOsSystemOps for MockMacOsSystemOps {
    fn is_native_macos(&self) -> bool {
        self.native_macos
    }

    fn supports_runtime_ops(&self) -> bool {
        true
    }

    fn host_arch(&self) -> &'static str {
        self.arch
    }

    fn current_uid(&self) -> u32 {
        self.uid
    }

    fn is_elevated(&self) -> Option<bool> {
        self.elevated
    }

    fn user_home_dir(&self) -> Option<PathBuf> {
        self.home_dir.clone()
    }

    fn short_temp_root(&self) -> PathBuf {
        self.short_tmp_root.clone()
    }

    fn free_disk_bytes(&self, _path: &Path) -> Result<u64, DimError> {
        Ok(self.free_bytes)
    }

    fn check_dir_permissions_restrictive(&self, _dir: &Path) -> Option<bool> {
        self.acl_safe
    }

    fn ensure_private_dir(&self, dir: &Path) -> Result<(), DimError> {
        reject_symlink_target(dir)?;
        if let Some(bad_mode) = read_simulated_mode_any_host(dir) {
            if (bad_mode & 0o200) == 0 || (bad_mode & 0o077) != 0 {
                return Err(DimError::Io(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!(
                        "simulated POSIX permission error (0o{bad_mode:03o}) on {}",
                        dir.display()
                    ),
                )));
            }
        }
        fs::create_dir_all(dir)?;
        Ok(())
    }

    fn run_command(
        &self,
        program: &Path,
        args: &[String],
        envs: &[(String, String)],
    ) -> Result<CommandOutput, DimError> {
        if let Ok(mut guard) = self.recorded_commands.lock() {
            guard.push(RecordedCommand {
                program: program.to_path_buf(),
                args: args.to_vec(),
                envs: envs.to_vec(),
            });
        }
        if let Some(handler) = &self.command_handler {
            return handler(program, args, envs);
        }
        if program.to_string_lossy().ends_with("osascript") {
            return Ok(CommandOutput {
                status_code: Some(0),
                stdout: b"status=ok activated=true active=true\n".to_vec(),
                stderr: Vec::new(),
            });
        }
        Ok(CommandOutput {
            status_code: Some(0),
            stdout: Vec::new(),
            stderr: Vec::new(),
        })
    }

    fn spawn_detached(&self, req: &SpawnDetachedRequest) -> Result<u32, DimError> {
        let pid = if let Ok(mut guard) = self.next_spawn_pid.lock() {
            let allocated = *guard;
            *guard = guard.saturating_add(1);
            allocated
        } else {
            49_999
        };
        let full_args = std::iter::once(req.program.to_string_lossy().into_owned())
            .chain(req.args.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ");
        if let Ok(mut procs) = self.processes.lock() {
            procs.push(ProcessEntry {
                pid,
                ppid: 1,
                uid: self.uid,
                rss_kib: 65_536,
                start_token: Some(format!("Mon Nov 17 12:00:{:02} 2026", pid % 60)),
                executable: req.program.to_string_lossy().into_owned(),
                args: full_args,
            });
        }
        Ok(pid)
    }

    fn list_processes(&self) -> Result<Vec<ProcessEntry>, DimError> {
        Ok(self.processes.lock().map(|g| g.clone()).unwrap_or_default())
    }

    fn is_pid_alive(&self, pid: u32) -> bool {
        self.processes
            .lock()
            .map(|g| g.iter().any(|p| p.pid == pid))
            .unwrap_or(false)
    }

    fn send_sigterm(&self, pid: u32) -> Result<bool, DimError> {
        if let Ok(mut sigs) = self.sigterm_pids.lock() {
            sigs.push(pid);
        }
        if let Some(handler) = &self.sigterm_handler {
            return handler(pid);
        }
        if let Ok(mut procs) = self.processes.lock() {
            let before = procs.len();
            procs.retain(|p| p.pid != pid);
            return Ok(procs.len() < before);
        }
        Ok(false)
    }

    fn query_windows_for_pids(&self, pids: &[u32]) -> Vec<WindowInfo> {
        self.windows
            .lock()
            .map(|g| {
                g.iter()
                    .filter(|w| pids.contains(&w.owner_pid))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn reject_symlink_target(dir: &Path) -> Result<(), DimError> {
    if let Ok(meta) = fs::symlink_metadata(dir) {
        if meta.file_type().is_symlink() {
            return Err(DimError::PathTraversal(format!(
                "refusing to use symlinked directory '{}'",
                dir.display()
            )));
        }
        if !meta.is_dir() {
            return Err(DimError::PathTraversal(format!(
                "expected directory but found non-directory '{}'",
                dir.display()
            )));
        }
    }
    Ok(())
}

fn read_simulated_mode_any_host(dir: &Path) -> Option<u32> {
    let mut probe = Some(dir);
    while let Some(curr) = probe {
        let sidecar = curr.join(SIMULATED_POSIX_MODE_FILE);
        if let Ok(raw) = fs::read_to_string(sidecar) {
            if let Ok(mode) = u32::from_str_radix(raw.trim().trim_start_matches("0o"), 8) {
                return Some(mode);
            }
        }
        probe = curr.parent();
    }
    None
}

#[cfg(not(unix))]
fn read_simulated_unwritable_or_unsafe_mode(dir: &Path) -> Option<u32> {
    read_simulated_mode_any_host(dir)
}

fn simulate_open_command_on_fixture_host(
    ops: &HostFixtureMacOsOps,
    args: &[String],
) -> Result<CommandOutput, DimError> {
    let mut bundle: Option<PathBuf> = None;
    let mut envs: Vec<(String, String)> = Vec::new();
    let mut app_args: Vec<String> = Vec::new();
    let mut in_app_args = false;
    let mut idx = 0usize;

    while idx < args.len() {
        let a = &args[idx];
        if in_app_args {
            app_args.push(a.clone());
            idx += 1;
            continue;
        }
        match a.as_str() {
            "-a" if idx + 1 < args.len() => {
                idx += 1;
                bundle = Some(PathBuf::from(&args[idx]));
            }
            "--env" if idx + 1 < args.len() => {
                idx += 1;
                if let Some((k, v)) = args[idx].split_once('=') {
                    envs.push((k.to_owned(), v.to_owned()));
                }
            }
            "--args" => {
                in_app_args = true;
            }
            _ => {}
        }
        idx += 1;
    }

    let Some(bundle_path) = bundle else {
        return Ok(CommandOutput {
            status_code: Some(1),
            stdout: Vec::new(),
            stderr: b"missing -a bundle argument".to_vec(),
        });
    };

    let mjs_entry = bundle_path
        .join("Contents")
        .join("MacOS")
        .join("ZaloFixture.mjs");
    if fs::symlink_metadata(&mjs_entry).is_ok_and(|m| m.is_file()) {
        let mut full_args = vec![mjs_entry.to_string_lossy().into_owned()];
        full_args.extend(app_args);
        let req = SpawnDetachedRequest {
            program: PathBuf::from("node"),
            args: full_args,
            envs,
            current_dir: mjs_entry.parent().map(Path::to_path_buf),
        };
        let _ = ops.spawn_detached(&req)?;
        return Ok(CommandOutput {
            status_code: Some(0),
            stdout: Vec::new(),
            stderr: Vec::new(),
        });
    }

    Ok(CommandOutput {
        status_code: Some(1),
        stdout: Vec::new(),
        stderr: b"simulated open requires macOS or ZaloFixture.mjs".to_vec(),
    })
}

fn spawn_detached_os_process(req: &SpawnDetachedRequest) -> Result<u32, DimError> {
    #[cfg(windows)]
    {
        spawn_detached_windows_no_inherit(req)
    }
    #[cfg(not(windows))]
    {
        let mut cmd = Command::new(&req.program);
        cmd.args(&req.args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        for (k, v) in &req.envs {
            cmd.env(k, v);
        }
        if let Some(cwd) = &req.current_dir {
            cmd.current_dir(cwd);
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }

        let mut child = cmd.spawn()?;
        let pid = child.id();
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(pid)
    }
}

fn host_os_process_start_token(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    #[cfg(windows)]
    {
        windows_process_creation_token(pid)
    }
    #[cfg(unix)]
    {
        unix_ps_lstart_token(pid)
    }
}

#[cfg(unix)]
fn unix_ps_lstart_token(pid: u32) -> Option<String> {
    let out = Command::new("/bin/ps")
        .args(["-p", &pid.to_string(), "-o", "lstart="])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!text.is_empty()).then_some(text)
}

#[cfg(windows)]
fn spawn_detached_windows_no_inherit(req: &SpawnDetachedRequest) -> Result<u32, DimError> {
    use std::os::windows::ffi::OsStrExt;

    const CREATE_UNICODE_ENVIRONMENT: u32 = 0x0000_0400;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    #[repr(C)]
    struct StartupInfoW {
        cb: u32,
        reserved: *mut u16,
        desktop: *mut u16,
        title: *mut u16,
        x: u32,
        y: u32,
        x_size: u32,
        y_size: u32,
        x_count_chars: u32,
        y_count_chars: u32,
        fill_attribute: u32,
        flags: u32,
        show_window: u16,
        cb_reserved2: u16,
        lp_reserved2: *mut u8,
        std_input: *mut core::ffi::c_void,
        std_output: *mut core::ffi::c_void,
        std_error: *mut core::ffi::c_void,
    }

    #[repr(C)]
    struct ProcessInformation {
        process: *mut core::ffi::c_void,
        thread: *mut core::ffi::c_void,
        process_id: u32,
        thread_id: u32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateProcessW(
            application_name: *const u16,
            command_line: *mut u16,
            process_attributes: *const core::ffi::c_void,
            thread_attributes: *const core::ffi::c_void,
            inherit_handles: i32,
            creation_flags: u32,
            environment: *const core::ffi::c_void,
            current_directory: *const u16,
            startup_info: *const StartupInfoW,
            process_information: *mut ProcessInformation,
        ) -> i32;
        fn CloseHandle(object: *mut core::ffi::c_void) -> i32;
    }

    let to_wide = |s: &str| -> Vec<u16> {
        std::ffi::OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    };

    let quote_win_arg = |arg: &str| -> String {
        if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
            return arg.to_owned();
        }
        let mut out = String::from("\"");
        let mut backslashes = 0usize;
        for ch in arg.chars() {
            if ch == '\\' {
                backslashes += 1;
            } else if ch == '"' {
                out.push_str(&"\\".repeat(backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            } else {
                out.push_str(&"\\".repeat(backslashes));
                out.push(ch);
                backslashes = 0;
            }
        }
        out.push_str(&"\\".repeat(backslashes * 2));
        out.push('"');
        out
    };

    let resolved_prog = resolve_windows_program(&req.program);
    let mut cmd_parts = vec![quote_win_arg(&resolved_prog.to_string_lossy())];
    for a in &req.args {
        cmd_parts.push(quote_win_arg(a));
    }
    let mut cmd_w = to_wide(&cmd_parts.join(" "));
    let cwd_w = req
        .current_dir
        .as_ref()
        .map(|d| to_wide(&d.to_string_lossy()));

    let mut env_map: Vec<(String, String)> = std::env::vars().collect();
    for (k, v) in &req.envs {
        if let Some(existing) = env_map
            .iter_mut()
            .find(|(ek, _)| ek.eq_ignore_ascii_case(k))
        {
            existing.1 = v.clone();
        } else {
            env_map.push((k.clone(), v.clone()));
        }
    }
    env_map.sort_by_key(|a| a.0.to_ascii_uppercase());
    let mut env_block: Vec<u16> = Vec::new();
    for (k, v) in env_map {
        let entry = format!("{k}={v}");
        env_block.extend(std::ffi::OsStr::new(&entry).encode_wide());
        env_block.push(0);
    }
    env_block.push(0);

    // SAFETY: All UTF-16 buffers outlive `CreateProcessW`, and `bInheritHandles = 0` prevents pipe leaks.
    unsafe {
        let mut si: StartupInfoW = std::mem::zeroed();
        si.cb = u32::try_from(std::mem::size_of::<StartupInfoW>()).unwrap_or(0);
        let mut pi: ProcessInformation = std::mem::zeroed();
        let ok = CreateProcessW(
            std::ptr::null(),
            cmd_w.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            CREATE_UNICODE_ENVIRONMENT | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW,
            env_block.as_ptr().cast(),
            cwd_w.as_ref().map_or(std::ptr::null(), |v| v.as_ptr()),
            &si,
            &mut pi,
        );
        if ok == 0 {
            return Err(DimError::Io(std::io::Error::last_os_error()));
        }
        let pid = pi.process_id;
        let _ = CloseHandle(pi.thread);
        let _ = CloseHandle(pi.process);
        Ok(pid)
    }
}

#[cfg(windows)]
fn resolve_windows_program(program: &Path) -> PathBuf {
    if program.is_absolute() && program.exists() {
        return program.to_path_buf();
    }
    let name = program.to_string_lossy();
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let direct = dir.join(program);
            if direct.is_file() {
                return direct;
            }
            if !name.to_ascii_lowercase().ends_with(".exe") {
                let with_exe = dir.join(format!("{name}.exe"));
                if with_exe.is_file() {
                    return with_exe;
                }
            }
        }
    }
    program.to_path_buf()
}

fn host_os_pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(unix)]
    {
        unix_kill_zero_alive(pid)
    }
    #[cfg(windows)]
    {
        windows_pid_alive(pid)
    }
}

fn terminate_host_os_pid(pid: u32) -> Result<bool, DimError> {
    if pid == 0 {
        return Ok(false);
    }
    #[cfg(unix)]
    {
        unix_send_sigterm(pid)
    }
    #[cfg(windows)]
    {
        windows_terminate_pid(pid)
    }
}

#[cfg(windows)]
fn windows_process_creation_token(pid: u32) -> Option<String> {
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;

    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low_date_time: u32,
        high_date_time: u32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(
            desired_access: u32,
            inherit_handle: i32,
            process_id: u32,
        ) -> *mut core::ffi::c_void;
        fn GetExitCodeProcess(process: *mut core::ffi::c_void, exit_code: *mut u32) -> i32;
        fn GetProcessTimes(
            process: *mut core::ffi::c_void,
            creation_time: *mut FileTime,
            exit_time: *mut FileTime,
            kernel_time: *mut FileTime,
            user_time: *mut FileTime,
        ) -> i32;
        fn CloseHandle(object: *mut core::ffi::c_void) -> i32;
    }

    // SAFETY: Standard Win32 handle queries with valid stack-allocated `FileTime` structs.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut exit_code: u32 = 0;
        let alive = GetExitCodeProcess(handle, &mut exit_code) != 0 && exit_code == STILL_ACTIVE;
        let mut creation = FileTime::default();
        let mut exit = FileTime::default();
        let mut kernel = FileTime::default();
        let mut user = FileTime::default();
        let times_ok =
            GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) != 0;
        let _ = CloseHandle(handle);
        if !alive || !times_ok {
            return None;
        }
        let ticks = (u64::from(creation.high_date_time) << 32) | u64::from(creation.low_date_time);
        Some(format!("winft:{ticks}"))
    }
}

#[cfg(windows)]
fn windows_pid_alive(pid: u32) -> bool {
    windows_process_creation_token(pid).is_some()
}

#[cfg(windows)]
fn windows_terminate_pid(pid: u32) -> Result<bool, DimError> {
    const PROCESS_TERMINATE: u32 = 0x0001;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(
            desired_access: u32,
            inherit_handle: i32,
            process_id: u32,
        ) -> *mut core::ffi::c_void;
        fn TerminateProcess(process: *mut core::ffi::c_void, exit_code: u32) -> i32;
        fn CloseHandle(object: *mut core::ffi::c_void) -> i32;
    }

    // SAFETY: `pid` has already been verified as an isolated fixture child PID by the caller.
    unsafe {
        let handle = OpenProcess(
            PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            pid,
        );
        if handle.is_null() {
            return Ok(false);
        }
        let terminated = TerminateProcess(handle, 0) != 0;
        let _ = CloseHandle(handle);
        Ok(terminated)
    }
}

#[cfg(unix)]
fn unix_kill_zero_alive(pid: u32) -> bool {
    let Ok(raw) = libc::pid_t::try_from(pid) else {
        return false;
    };
    if raw <= 0 {
        return false;
    }
    // SAFETY: Signal 0 only probes PID existence without delivering a signal.
    let rc = unsafe { libc::kill(raw, 0) };
    if rc == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(unix)]
fn unix_send_sigterm(pid: u32) -> Result<bool, DimError> {
    let Ok(raw) = libc::pid_t::try_from(pid) else {
        return Ok(false);
    };
    if raw <= 1 {
        return Err(DimError::ProcessError(format!(
            "refusing to send SIGTERM to system PID {pid}"
        )));
    }
    // SAFETY: `raw` is > 1 and verified by the caller to belong to an isolated non-primary process.
    let rc = unsafe { libc::kill(raw, libc::SIGTERM) };
    if rc == 0 {
        return Ok(true);
    }
    let err = std::io::Error::last_os_error();
    if err.raw_os_error() == Some(libc::ESRCH) {
        return Ok(false);
    }
    Err(DimError::Io(err))
}

#[cfg(unix)]
fn unix_free_disk_bytes(path: &Path) -> Result<u64, DimError> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let mut probe = path;
    while fs::symlink_metadata(probe).is_err() {
        if let Some(parent) = probe.parent() {
            if parent.as_os_str().is_empty() {
                break;
            }
            probe = parent;
        } else {
            break;
        }
    }
    let c_path = CString::new(probe.as_os_str().as_bytes())
        .map_err(|_| DimError::PathTraversal("path contains NUL byte".to_owned()))?;
    // SAFETY: `statvfs` initializes `stat` when returning 0.
    unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c_path.as_ptr(), &mut stat) != 0 {
            return Err(DimError::Io(std::io::Error::last_os_error()));
        }
        #[allow(clippy::useless_conversion)]
        let bavail = u64::from(stat.f_bavail);
        #[allow(clippy::useless_conversion)]
        let frsize = u64::from(stat.f_frsize);
        Ok(bavail.saturating_mul(frsize))
    }
}

#[cfg(unix)]
fn unix_check_dir_restrictive(dir: &Path, expected_uid: u32) -> Option<bool> {
    use std::os::unix::fs::MetadataExt;

    let probe = if fs::symlink_metadata(dir).is_ok() {
        dir
    } else {
        dir.parent().unwrap_or(dir)
    };
    let meta = fs::symlink_metadata(probe).ok()?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Some(false);
    }
    let owner_ok = meta.uid() == expected_uid || expected_uid == 0;
    let mode_ok = (meta.mode() & 0o022) == 0;
    Some(owner_ok && mode_ok)
}

#[cfg(unix)]
fn unix_ensure_private_dir(dir: &Path, expected_uid: u32) -> Result<(), DimError> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};

    reject_symlink_target(dir)?;
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true).mode(0o700);
    builder.create(dir)?;

    let meta = fs::symlink_metadata(dir)?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(DimError::PathTraversal(format!(
            "directory '{}' resolved to a symlink or non-directory",
            dir.display()
        )));
    }
    if meta.uid() != expected_uid && expected_uid != 0 {
        return Err(DimError::PathTraversal(format!(
            "directory '{}' is owned by uid {} instead of current uid {}",
            dir.display(),
            meta.uid(),
            expected_uid
        )));
    }
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    let verify_meta = fs::symlink_metadata(dir)?;
    if (verify_meta.mode() & 0o077) != 0 {
        return Err(DimError::Io(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "failed to enforce 0700 permissions on '{}' (mode {:04o})",
                dir.display(),
                verify_meta.mode() & 0o777
            ),
        )));
    }
    Ok(())
}

#[cfg(unix)]
fn lookup_passwd_home_dir(uid: u32) -> Option<PathBuf> {
    use std::ffi::CStr;
    use std::os::unix::ffi::OsStrExt;

    let mut buf = vec![0u8; 4096];
    let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
    let mut result: *mut libc::passwd = std::ptr::null_mut();

    // SAFETY: `pwd`, `buf`, and `result` are valid stack/heap allocations outliving `getpwuid_r`.
    let rc = unsafe {
        libc::getpwuid_r(
            uid,
            &mut pwd,
            buf.as_mut_ptr().cast::<libc::c_char>(),
            buf.len(),
            &mut result,
        )
    };
    if rc != 0 || result.is_null() || pwd.pw_dir.is_null() {
        return None;
    }
    // SAFETY: `pwd.pw_dir` points into `buf` and is NUL-terminated when `getpwuid_r` succeeds.
    let dir_bytes = unsafe { CStr::from_ptr(pwd.pw_dir).to_bytes() };
    if dir_bytes.is_empty() {
        return None;
    }
    Some(PathBuf::from(std::ffi::OsStr::from_bytes(dir_bytes)))
}

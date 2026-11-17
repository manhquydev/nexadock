//! Atomic `.runtime-lease.json` persistence and anti-PID-reuse process ownership verification.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::DimError;
use crate::fs_lock::write_bytes_atomic;
use crate::models::{RUNTIME_LEASE_FILE, RUNTIME_PID_FILE};
use crate::time_util::now_rfc3339_utc;

/// Atomic process ownership lease persisted at `<profile_dir>/.runtime-lease.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeLease {
    /// OS process identifier spawned for this isolated profile.
    pub pid: u32,
    /// OS-reported creation timestamp token (`GetProcessTimes` 100ns ticks on Windows,
    /// `/proc/<pid>/stat` starttime ticks on Linux, hashed `ps -o lstart=` on macOS).
    pub process_create_time: u64,
    /// Live OS executable image path observed when spawning `pid`.
    pub executable_path: String,
    /// Bound 8-hex profile identifier.
    pub profile_id: String,
    /// UTC timestamp when the lease was created.
    pub spawned_at_utc: String,
}

/// Live OS identity queried for a candidate PID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessIdentity {
    /// Process ID.
    pub pid: u32,
    /// OS process creation timestamp token.
    pub process_create_time: u64,
    /// OS executable image path.
    pub executable_path: String,
}

/// Outcome of probing the OS kernel for a PID's live identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessIdentityProbe {
    /// Process is alive and its creation time + executable path were verified.
    Alive(ProcessIdentity),
    /// Process does not exist or has already exited.
    Dead,
    /// Process exists (e.g. `ACCESS_DENIED` / `EPERM`), preventing identity verification.
    Indeterminate(String),
}

/// Outcome of verifying `<profile_dir>/.runtime-lease.json` against live OS state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeaseVerification {
    /// No `.runtime-lease.json` exists in `profile_dir`.
    Absent,
    /// Lease matches `expected_profile_id`, is non-primary, and matches the live OS process's
    /// creation timestamp and executable image path.
    VerifiedLive(RuntimeLease),
    /// Lease belongs to `expected_profile_id`, and the original process is dead (or PID was
    /// recycled by the OS to a process with a different creation time or executable path).
    StaleDead(RuntimeLease),
    /// Lease file is corrupted/unreadable, bound to a different `profile_id`, overlaps a
    /// protected primary PID, or points to an inaccessible (`EPERM`/`ACCESS_DENIED`) process.
    InvalidOrIndeterminate(String),
}

/// Outcome of checking whether any `lockfile` or `SingletonLock` inside `profile_dir` is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileLockActivity {
    /// No live or OS-locked lockfile exists.
    Inactive,
    /// An OS-held file lock or verified live non-self lockfile PID is active.
    Active,
    /// Lockfile state could not be determined safely (e.g. permission denied or indeterminate PID).
    Indeterminate,
}

/// Captures `(process_create_time, executable_path)` for a newly spawned `pid` and constructs
/// a [`RuntimeLease`] bound to `profile_id`.
#[must_use]
pub fn build_runtime_lease(
    pid: u32,
    fallback_exe: &Path,
    profile_id: &str,
    known_identity: Option<(u64, String)>,
) -> RuntimeLease {
    let (process_create_time, executable_path) = match known_identity {
        Some((ct, exe)) if ct > 0 && !exe.is_empty() => (ct, exe),
        _ => match query_process_identity(pid) {
            ProcessIdentityProbe::Alive(id) => (id.process_create_time, id.executable_path),
            _ => (0, fallback_exe.display().to_string()),
        },
    };
    RuntimeLease {
        pid,
        process_create_time,
        executable_path,
        profile_id: profile_id.to_owned(),
        spawned_at_utc: now_rfc3339_utc(),
    }
}

/// Atomically writes `<profile_dir>/.runtime-lease.json` and updates `<profile_dir>/.runtime-pid`.
pub fn write_runtime_lease(profile_dir: &Path, lease: &RuntimeLease) -> Result<(), DimError> {
    let lease_path = profile_dir.join(RUNTIME_LEASE_FILE);
    let mut json = serde_json::to_vec_pretty(lease)?;
    json.push(b'\n');
    write_bytes_atomic(&lease_path, &json)?;
    let _ = write_bytes_atomic(
        &profile_dir.join(RUNTIME_PID_FILE),
        format!("{}\n", lease.pid).as_bytes(),
    );
    Ok(())
}

/// Removes `<profile_dir>/.runtime-lease.json` and `<profile_dir>/.runtime-pid` without following symlinks.
pub fn remove_runtime_lease(profile_dir: &Path) -> usize {
    let mut count = 0usize;
    for name in [RUNTIME_LEASE_FILE, RUNTIME_PID_FILE] {
        let p = profile_dir.join(name);
        if let Ok(meta) = fs::symlink_metadata(&p) {
            if (!meta.is_dir() || meta.file_type().is_symlink()) && fs::remove_file(&p).is_ok() {
                count += 1;
            }
        }
    }
    count
}

/// Verifies `<profile_dir>/.runtime-lease.json` against live OS process liveness, creation time,
/// executable image path, and `protected_primary_pids` exclusion.
#[must_use]
pub fn verify_runtime_lease(
    profile_dir: &Path,
    expected_profile_id: &str,
    protected_primary_pids: &[u32],
) -> LeaseVerification {
    let lease_path = profile_dir.join(RUNTIME_LEASE_FILE);
    let meta = match fs::symlink_metadata(&lease_path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return LeaseVerification::Absent,
        Err(e) => {
            return LeaseVerification::InvalidOrIndeterminate(format!(
                "cannot stat {}: {e}",
                lease_path.display()
            ))
        }
    };
    if meta.file_type().is_symlink() || meta.is_dir() {
        return LeaseVerification::InvalidOrIndeterminate(
            ".runtime-lease.json is a symlink or directory".to_owned(),
        );
    }

    let raw = match fs::read_to_string(&lease_path) {
        Ok(s) => s,
        Err(e) => {
            return LeaseVerification::InvalidOrIndeterminate(format!(
                "cannot read .runtime-lease.json: {e}"
            ))
        }
    };
    if raw.trim().is_empty() {
        return LeaseVerification::InvalidOrIndeterminate(
            ".runtime-lease.json is empty (0 bytes)".to_owned(),
        );
    }
    let lease: RuntimeLease = match serde_json::from_str(&raw) {
        Ok(l) => l,
        Err(e) => {
            return LeaseVerification::InvalidOrIndeterminate(format!(
                "corrupted .runtime-lease.json: {e}"
            ))
        }
    };

    if !lease.profile_id.eq_ignore_ascii_case(expected_profile_id) {
        return LeaseVerification::InvalidOrIndeterminate(format!(
            "lease profile_id '{}' does not match '{}'",
            lease.profile_id, expected_profile_id
        ));
    }
    if lease.pid == 0 {
        return LeaseVerification::InvalidOrIndeterminate("lease pid is 0".to_owned());
    }
    if protected_primary_pids.contains(&lease.pid) {
        return LeaseVerification::InvalidOrIndeterminate(format!(
            "lease pid {} overlaps protected primary Zalo process",
            lease.pid
        ));
    }
    if lease.pid == std::process::id() {
        return LeaseVerification::StaleDead(lease);
    }

    match query_process_identity(lease.pid) {
        ProcessIdentityProbe::Dead => LeaseVerification::StaleDead(lease),
        ProcessIdentityProbe::Indeterminate(reason) => {
            LeaseVerification::InvalidOrIndeterminate(reason)
        }
        ProcessIdentityProbe::Alive(live) => {
            // Anti-PID-reuse check 1: creation timestamp must match when non-zero on both sides.
            if lease.process_create_time == 0
                || live.process_create_time == 0
                || lease.process_create_time != live.process_create_time
            {
                return LeaseVerification::StaleDead(lease);
            }
            // Anti-PID-reuse check 2: live executable image path must match recorded image path.
            if !executable_paths_match(&lease.executable_path, &live.executable_path) {
                return LeaseVerification::StaleDead(lease);
            }
            LeaseVerification::VerifiedLive(lease)
        }
    }
}

/// Independently probes `profile_dir` for any OS-locked `lockfile` or live `lockfile`/`SingletonLock`
/// owner across Windows, macOS, and Linux layouts (used as defense-in-depth inside `JsonProfileStore`).
#[must_use]
pub fn check_profile_lockfile_activity(profile_dir: &Path) -> ProfileLockActivity {
    let zalo_dirs = [
        profile_dir.join("AppData").join("Roaming").join("ZaloData"),
        profile_dir
            .join("Library")
            .join("Application Support")
            .join("ZaloData"),
        profile_dir.join(".config").join("ZaloData"),
    ];

    for zdir in zalo_dirs {
        let lockfile = zdir.join("lockfile");
        if lockfile.exists() {
            if is_regular_file_os_locked(&lockfile) {
                return ProfileLockActivity::Active;
            }
            if let Ok(raw) = fs::read_to_string(&lockfile) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
                    if let Some(pid) = val
                        .get("pid")
                        .and_then(serde_json::Value::as_u64)
                        .and_then(|n| u32::try_from(n).ok())
                    {
                        if pid > 0 && pid != std::process::id() {
                            match query_process_identity(pid) {
                                ProcessIdentityProbe::Alive(live) => {
                                    if is_plausible_managed_exe(&live.executable_path) {
                                        return ProfileLockActivity::Active;
                                    }
                                }
                                ProcessIdentityProbe::Indeterminate(_) => {
                                    return ProfileLockActivity::Indeterminate;
                                }
                                ProcessIdentityProbe::Dead => {}
                            }
                        }
                    }
                }
            }
        }

        let singleton = zdir.join("SingletonLock");
        if let Ok(target) = fs::read_link(&singleton) {
            let s = target.to_string_lossy();
            if let Some((_, pid_str)) = s.trim().rsplit_once('-') {
                if let Ok(pid) = pid_str.parse::<u32>() {
                    if pid > 0 && pid != std::process::id() {
                        match query_process_identity(pid) {
                            ProcessIdentityProbe::Alive(_) => return ProfileLockActivity::Active,
                            ProcessIdentityProbe::Indeterminate(_) => {
                                return ProfileLockActivity::Indeterminate;
                            }
                            ProcessIdentityProbe::Dead => {}
                        }
                    }
                }
            }
        }
    }

    // Also inspect legacy/bridge `.runtime-pid` (plain integer or JSON `{"pid": N}`) when no
    // `.runtime-lease.json` already classified the PID as stale/dead.
    if !profile_dir.join(RUNTIME_LEASE_FILE).exists() {
        let rt_pid_path = profile_dir.join(RUNTIME_PID_FILE);
        if let Ok(raw) = fs::read_to_string(&rt_pid_path) {
            let trimmed = raw.trim();
            let parsed_pid = trimmed.parse::<u32>().ok().or_else(|| {
                serde_json::from_str::<serde_json::Value>(trimmed)
                    .ok()?
                    .get("pid")?
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok())
            });
            if let Some(pid) = parsed_pid {
                if pid > 0 && pid != std::process::id() {
                    match query_process_identity(pid) {
                        ProcessIdentityProbe::Alive(live) => {
                            if is_plausible_managed_exe(&live.executable_path) {
                                return ProfileLockActivity::Active;
                            }
                        }
                        ProcessIdentityProbe::Indeterminate(_) => {
                            return ProfileLockActivity::Indeterminate;
                        }
                        ProcessIdentityProbe::Dead => {}
                    }
                }
            }
        }
    }

    ProfileLockActivity::Inactive
}

fn is_plausible_managed_exe(exe_path: &str) -> bool {
    let file_name = Path::new(exe_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(exe_path)
        .to_ascii_lowercase();
    // Exclude test runners (`xf-test-runner*`, `cargo*`) that write synthetic self/spoof PIDs in REL-08,
    // while catching real Zalo, Node fixtures, AppImages, and custom desktop fixtures!
    file_name.starts_with("zalo")
        || file_name.starts_with("zocr")
        || file_name == "node"
        || file_name == "node.exe"
        || file_name.starts_with("dim-test-fixture")
        || file_name.starts_with("xf-desktop-fixture")
        || file_name.ends_with(".appimage")
}

fn normalize_exe_for_cmp(raw: &str) -> String {
    let trimmed = raw.trim();
    let stripped = trimmed
        .strip_prefix(r"\\?\UNC\")
        .map(|rest| format!(r"\\{rest}"))
        .or_else(|| trimmed.strip_prefix(r"\\?\").map(str::to_owned))
        .unwrap_or_else(|| trimmed.to_owned());
    let norm = stripped.replace('/', "\\");
    if cfg!(windows) {
        norm.to_ascii_lowercase()
    } else {
        stripped
    }
}

fn executable_paths_match(expected: &str, actual: &str) -> bool {
    if expected.trim().is_empty() || actual.trim().is_empty() {
        return false;
    }
    let exp_norm = normalize_exe_for_cmp(expected);
    let act_norm = normalize_exe_for_cmp(actual);
    if exp_norm == act_norm {
        return true;
    }
    // On Unix/macOS where `ps -o comm=` or symlinked `/usr/bin/node` -> `/usr/local/bin/node` may resolve canonically:
    if let (Ok(c1), Ok(c2)) = (fs::canonicalize(expected), fs::canonicalize(actual)) {
        if c1 == c2 {
            return true;
        }
    }
    false
}

/// Queries live OS process identity (`pid`, `process_create_time`, `executable_path`) without trusting PID alone.
#[must_use]
pub fn query_process_identity(pid: u32) -> ProcessIdentityProbe {
    if pid == 0 {
        return ProcessIdentityProbe::Dead;
    }
    os_probe::query_process_identity_os(pid)
}

#[cfg(windows)]
mod os_probe {
    use std::ffi::{c_void, OsString};
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::Path;

    use super::{ProcessIdentity, ProcessIdentityProbe};

    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;
    const ERROR_ACCESS_DENIED: u32 = 5;
    const ERROR_SHARING_VIOLATION: u32 = 32;
    const GENERIC_READ_WRITE: u32 = 0xC000_0000;
    const OPEN_EXISTING: u32 = 3;
    const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;
    const INVALID_HANDLE_VALUE: *mut c_void = (-1_isize) as *mut c_void;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(
            dw_desired_access: u32,
            b_inherit_handle: i32,
            dw_process_id: u32,
        ) -> *mut c_void;
        fn CloseHandle(h_object: *mut c_void) -> i32;
        fn GetLastError() -> u32;
        fn GetExitCodeProcess(h_process: *mut c_void, lp_exit_code: *mut u32) -> i32;
        fn GetProcessTimes(
            h_process: *mut c_void,
            lp_creation_time: *mut FileTime,
            lp_exit_time: *mut FileTime,
            lp_kernel_time: *mut FileTime,
            lp_user_time: *mut FileTime,
        ) -> i32;
        fn QueryFullProcessImageNameW(
            h_process: *mut c_void,
            dw_flags: u32,
            lp_exe_name: *mut u16,
            lpdw_size: *mut u32,
        ) -> i32;
        fn CreateFileW(
            lp_file_name: *const u16,
            dw_desired_access: u32,
            dw_share_mode: u32,
            lp_security_attributes: *const c_void,
            dw_creation_disposition: u32,
            dw_flags_and_attributes: u32,
            h_template_file: *mut c_void,
        ) -> *mut c_void;
    }

    struct HandleGuard(*mut c_void);
    impl Drop for HandleGuard {
        fn drop(&mut self) {
            if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
                // SAFETY: Valid Win32 handle owned by HandleGuard.
                unsafe {
                    let _ = CloseHandle(self.0);
                }
            }
        }
    }

    pub(super) fn query_process_identity_os(pid: u32) -> ProcessIdentityProbe {
        // SAFETY: Query-only `OpenProcess` wrapped immediately in `HandleGuard`.
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                let err = GetLastError();
                if err == ERROR_ACCESS_DENIED {
                    return ProcessIdentityProbe::Indeterminate(format!(
                        "OpenProcess({pid}) failed with ERROR_ACCESS_DENIED"
                    ));
                }
                return ProcessIdentityProbe::Dead;
            }
            let _guard = HandleGuard(h);

            let mut exit_code: u32 = 0;
            if GetExitCodeProcess(h, &mut exit_code) == 0 || exit_code != STILL_ACTIVE {
                return ProcessIdentityProbe::Dead;
            }

            let mut creation = FileTime::default();
            let mut exit = FileTime::default();
            let mut kernel = FileTime::default();
            let mut user = FileTime::default();
            if GetProcessTimes(h, &mut creation, &mut exit, &mut kernel, &mut user) == 0 {
                return ProcessIdentityProbe::Indeterminate(format!(
                    "GetProcessTimes({pid}) failed"
                ));
            }
            let create_100ns = (u64::from(creation.high) << 32) | u64::from(creation.low);

            let mut buf = vec![0u16; 2048];
            let mut size = buf.len() as u32;
            if QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut size) == 0 || size == 0 {
                return ProcessIdentityProbe::Indeterminate(format!(
                    "QueryFullProcessImageNameW({pid}) failed"
                ));
            }
            let exe = OsString::from_wide(&buf[..size as usize])
                .to_string_lossy()
                .into_owned();

            ProcessIdentityProbe::Alive(ProcessIdentity {
                pid,
                process_create_time: create_100ns,
                executable_path: exe,
            })
        }
    }

    pub(super) fn is_regular_file_os_locked(path: &Path) -> bool {
        if !path.exists() {
            return false;
        }
        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: `wide` is null-terminated; returned handle is closed immediately via `HandleGuard`.
        unsafe {
            let h = CreateFileW(
                wide.as_ptr(),
                GENERIC_READ_WRITE,
                0,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            );
            if h == INVALID_HANDLE_VALUE {
                return GetLastError() == ERROR_SHARING_VIOLATION;
            }
            let _guard = HandleGuard(h);
            false
        }
    }
}

#[cfg(target_os = "linux")]
mod os_probe {
    use std::fs::{self, OpenOptions};
    use std::io;
    use std::os::raw::c_int;
    use std::os::unix::io::AsRawFd;
    use std::path::Path;

    use super::{ProcessIdentity, ProcessIdentityProbe};

    const LOCK_EX: c_int = 2;
    const LOCK_NB: c_int = 4;
    const LOCK_UN: c_int = 8;

    extern "C" {
        fn kill(pid: c_int, sig: c_int) -> c_int;
        fn flock(fd: c_int, operation: c_int) -> c_int;
    }

    pub(super) fn query_process_identity_os(pid: u32) -> ProcessIdentityProbe {
        let Ok(raw_pid) = c_int::try_from(pid) else {
            return ProcessIdentityProbe::Dead;
        };
        // SAFETY: Signal 0 probes process existence without sending any signal.
        let rc = unsafe { kill(raw_pid, 0) };
        if rc != 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::PermissionDenied {
                return ProcessIdentityProbe::Indeterminate(format!("kill({pid}, 0) returned EPERM"));
            }
            return ProcessIdentityProbe::Dead;
        }

        let stat = match fs::read_to_string(format!("/proc/{pid}/stat")) {
            Ok(s) => s,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return ProcessIdentityProbe::Dead,
            Err(e) => {
                return ProcessIdentityProbe::Indeterminate(format!(
                    "cannot read /proc/{pid}/stat: {e}"
                ))
            }
        };
        let Some(close_paren) = stat.rfind(')') else {
            return ProcessIdentityProbe::Indeterminate(format!("malformed /proc/{pid}/stat"));
        };
        let fields: Vec<&str> = stat[close_paren + 1..].split_whitespace().collect();
        if fields.len() <= 19 {
            return ProcessIdentityProbe::Indeterminate(format!("short /proc/{pid}/stat"));
        }
        if fields[0] == "Z" || fields[0] == "X" {
            return ProcessIdentityProbe::Dead;
        }
        let Ok(starttime) = fields[19].parse::<u64>() else {
            return ProcessIdentityProbe::Indeterminate(format!("unparseable starttime for {pid}"));
        };
        let exe = match fs::read_link(format!("/proc/{pid}/exe")) {
            Ok(p) => p.display().to_string(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => return ProcessIdentityProbe::Dead,
            Err(e) => {
                return ProcessIdentityProbe::Indeterminate(format!(
                    "cannot read /proc/{pid}/exe: {e}"
                ))
            }
        };

        ProcessIdentityProbe::Alive(ProcessIdentity {
            pid,
            process_create_time: starttime,
            executable_path: exe,
        })
    }

    pub(super) fn is_regular_file_os_locked(path: &Path) -> bool {
        let Ok(f) = OpenOptions::new().read(true).open(path) else {
            return false;
        };
        // SAFETY: Valid file descriptor owned by `f`.
        let rc = unsafe { flock(f.as_raw_fd(), LOCK_EX | LOCK_NB) };
        if rc == 0 {
            unsafe {
                let _ = flock(f.as_raw_fd(), LOCK_UN);
            }
            return false;
        }
        io::Error::last_os_error().kind() == io::ErrorKind::WouldBlock
    }
}

#[cfg(target_os = "macos")]
mod os_probe {
    use std::fs::OpenOptions;
    use std::io;
    use std::os::raw::c_int;
    use std::os::unix::io::AsRawFd;
    use std::path::Path;
    use std::process::{Command, Stdio};

    use super::{ProcessIdentity, ProcessIdentityProbe};

    const LOCK_EX: c_int = 2;
    const LOCK_NB: c_int = 4;
    const LOCK_UN: c_int = 8;

    extern "C" {
        fn kill(pid: c_int, sig: c_int) -> c_int;
        fn flock(fd: c_int, operation: c_int) -> c_int;
    }

    pub(super) fn query_process_identity_os(pid: u32) -> ProcessIdentityProbe {
        let Ok(raw_pid) = c_int::try_from(pid) else {
            return ProcessIdentityProbe::Dead;
        };
        // SAFETY: Signal 0 probes process existence without sending any signal.
        let rc = unsafe { kill(raw_pid, 0) };
        if rc != 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::PermissionDenied {
                return ProcessIdentityProbe::Indeterminate(format!("kill({pid}, 0) returned EPERM"));
            }
            return ProcessIdentityProbe::Dead;
        }

        let pid_s = pid.to_string();
        let lstart = Command::new("ps")
            .args(["-o", "lstart=", "-p", &pid_s])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_default();
        let comm = Command::new("ps")
            .args(["-o", "comm=", "-p", &pid_s])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_default();

        if lstart.is_empty() || comm.is_empty() {
            return ProcessIdentityProbe::Dead;
        }
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in lstart.as_bytes() {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        if h == 0 {
            h = 1;
        }

        ProcessIdentityProbe::Alive(ProcessIdentity {
            pid,
            process_create_time: h,
            executable_path: comm,
        })
    }

    pub(super) fn is_regular_file_os_locked(path: &Path) -> bool {
        let Ok(f) = OpenOptions::new().read(true).open(path) else {
            return false;
        };
        // SAFETY: Valid file descriptor owned by `f`.
        let rc = unsafe { flock(f.as_raw_fd(), LOCK_EX | LOCK_NB) };
        if rc == 0 {
            unsafe {
                let _ = flock(f.as_raw_fd(), LOCK_UN);
            }
            return false;
        }
        io::Error::last_os_error().kind() == io::ErrorKind::WouldBlock
    }
}

use os_probe::is_regular_file_os_locked;

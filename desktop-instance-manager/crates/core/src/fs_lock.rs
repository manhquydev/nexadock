//! Cross-process advisory OS file locks (`LockFileEx` on Windows, `flock` on Unix)
//! and atomic same-directory temp-file replacement (`MoveFileExW` / `fs::rename`).

use std::collections::hash_map::RandomState;
use std::fs::{self, File, OpenOptions};
use std::hash::{BuildHasher, Hasher};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::error::DimError;

/// Default bounded timeout when acquiring a cross-process advisory file lock.
pub const DEFAULT_ADVISORY_LOCK_TIMEOUT: Duration = Duration::from_secs(10);

static TMP_SEQ_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Exclusive cross-process advisory OS file lock guard.
///
/// Holds an exclusive OS byte-range lock (`LockFileEx` `LOCKFILE_EXCLUSIVE_LOCK` on Windows,
/// `flock(LOCK_EX)` on Unix) for the lifetime of the guard, releasing it automatically on `Drop`.
#[derive(Debug)]
pub struct AdvisoryFileLock {
    lock_path: PathBuf,
    file: Option<File>,
}

impl AdvisoryFileLock {
    /// Attempts to acquire an exclusive OS advisory lock immediately without blocking.
    ///
    /// Returns `Ok(Some(guard))` if acquired, `Ok(None)` if another handle/process currently
    /// holds the lock, or `Err(DimError::Io)` on underlying filesystem failure.
    pub fn try_acquire_exclusive(lock_path: &Path) -> Result<Option<Self>, DimError> {
        if let Some(parent) = lock_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        if let Ok(meta) = fs::symlink_metadata(lock_path) {
            if meta.file_type().is_symlink() || meta.is_dir() {
                return Err(DimError::Io(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "advisory lock path '{}' cannot be a directory or symlink",
                        lock_path.display()
                    ),
                )));
            }
        }

        let file = open_lock_file(lock_path)?;
        match try_lock_file_exclusive(&file) {
            Ok(true) => Ok(Some(Self {
                lock_path: lock_path.to_path_buf(),
                file: Some(file),
            })),
            Ok(false) => Ok(None),
            Err(err) => Err(DimError::Io(err)),
        }
    }

    /// Acquires an exclusive OS advisory lock, retrying with bounded backoff up to `timeout`.
    pub fn acquire_exclusive(lock_path: &Path, timeout: Duration) -> Result<Self, DimError> {
        let deadline = Instant::now() + timeout;
        let mut sleep_ms = 2u64;

        loop {
            if let Some(guard) = Self::try_acquire_exclusive(lock_path)? {
                return Ok(guard);
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(DimError::Io(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!(
                        "timed out acquiring exclusive advisory lock '{}' after {} ms",
                        lock_path.display(),
                        timeout.as_millis()
                    ),
                )));
            }
            let remaining = deadline.saturating_duration_since(now);
            let wait = Duration::from_millis(sleep_ms).min(remaining);
            thread::sleep(wait);
            sleep_ms = (sleep_ms.saturating_mul(2)).min(20);
        }
    }

    /// Path of the underlying advisory lock file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.lock_path
    }
}

impl Drop for AdvisoryFileLock {
    fn drop(&mut self) {
        if let Some(file) = self.file.take() {
            let _ = unlock_file(&file);
            drop(file);
        }
    }
}

/// Generates a unique temporary file path `<target_name>.<pid>.<16hex>.tmp` in the same directory
/// as `target_path` so concurrent or interrupted writers never collide on a fixed `.tmp` filename.
#[must_use]
pub fn unique_temp_file_path(target_path: &Path) -> PathBuf {
    let parent = target_path.parent().unwrap_or_else(|| Path::new("."));
    let base_name = target_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("atomic");
    let pid = std::process::id();
    let seq = TMP_SEQ_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(seq);
    hasher.write_u128(nanos);
    hasher.write_u32(pid);
    let token = hasher.finish();
    parent.join(format!("{base_name}.{pid}.{token:016x}.tmp"))
}

/// Writes `bytes` to a unique same-directory `.tmp` file, flushes via `File::sync_all()`, closes
/// the handle, and atomically replaces `target_path` (`MoveFileExW` with
/// `MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH` on Windows, `fs::rename` + dir sync on Unix).
///
/// On any failure, removes the unique `.tmp` file and leaves any existing `target_path` untouched.
pub fn write_bytes_atomic(target_path: &Path, bytes: &[u8]) -> Result<(), DimError> {
    if let Some(parent) = target_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let mut tmp_path = unique_temp_file_path(target_path);
    let mut file_opt = None;
    for _ in 0..8 {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)
        {
            Ok(f) => {
                file_opt = Some(f);
                break;
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                tmp_path = unique_temp_file_path(target_path);
            }
            Err(e) => return Err(DimError::Io(e)),
        }
    }

    let mut file = match file_opt {
        Some(f) => f,
        None => {
            return Err(DimError::Io(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "failed to allocate unique temporary file path",
            )))
        }
    };

    let write_and_sync = (|| -> io::Result<()> {
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(())
    })();

    // Always close the temp file handle BEFORE replacing or unlinking on Windows.
    drop(file);

    if let Err(err) = write_and_sync {
        let _ = fs::remove_file(&tmp_path);
        return Err(DimError::Io(err));
    }

    if let Err(err) = atomic_replace_file(&tmp_path, target_path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(DimError::Io(err));
    }

    Ok(())
}

/// Sweeps stale regular temporary files (`<base_file_name>.tmp` or `<base_file_name>.*.tmp`)
/// in `dir`. Callers should invoke this while holding the directory's exclusive advisory lock
/// so active in-flight writes are never disturbed.
pub fn clean_stale_temp_files(dir: &Path, base_file_name: &str) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let legacy_exact = format!("{base_file_name}.tmp");
    let prefix = format!("{base_file_name}.");
    let mut removed = 0usize;

    for entry in entries.flatten() {
        let name_os = entry.file_name();
        let Some(name) = name_os.to_str() else {
            continue;
        };
        let is_matching_tmp =
            name == legacy_exact || (name.starts_with(&prefix) && name.ends_with(".tmp"));
        if !is_matching_tmp {
            continue;
        }
        let path = entry.path();
        if let Ok(meta) = fs::symlink_metadata(&path) {
            if meta.is_file() && !meta.file_type().is_symlink() && fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }
    }
    removed
}

fn open_lock_file(lock_path: &Path) -> io::Result<File> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ: u32 = 0x0000_0001;
        const FILE_SHARE_WRITE: u32 = 0x0000_0002;
        const FILE_SHARE_DELETE: u32 = 0x0000_0004;
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .open(lock_path)
    }
    #[cfg(not(windows))]
    {
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
    }
}

#[cfg(windows)]
mod platform_impl {
    use std::ffi::c_void;
    use std::fs::File;
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::AsRawHandle;
    use std::path::Path;

    const LOCKFILE_FAIL_IMMEDIATELY: u32 = 0x0000_0001;
    const LOCKFILE_EXCLUSIVE_LOCK: u32 = 0x0000_0002;
    const ERROR_SHARING_VIOLATION: i32 = 32;
    const ERROR_LOCK_VIOLATION: i32 = 33;
    const MOVEFILE_REPLACE_EXISTING: u32 = 0x0000_0001;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x0000_0008;

    #[repr(C)]
    struct Overlapped {
        internal: usize,
        internal_high: usize,
        offset: u32,
        offset_high: u32,
        h_event: *mut c_void,
    }

    impl Default for Overlapped {
        fn default() -> Self {
            Self {
                internal: 0,
                internal_high: 0,
                offset: 0,
                offset_high: 0,
                h_event: std::ptr::null_mut(),
            }
        }
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn LockFileEx(
            h_file: *mut c_void,
            dw_flags: u32,
            dw_reserved: u32,
            n_number_of_bytes_to_lock_low: u32,
            n_number_of_bytes_to_lock_high: u32,
            lp_overlapped: *mut Overlapped,
        ) -> i32;
        fn UnlockFileEx(
            h_file: *mut c_void,
            dw_reserved: u32,
            n_number_of_bytes_to_unlock_low: u32,
            n_number_of_bytes_to_unlock_high: u32,
            lp_overlapped: *mut Overlapped,
        ) -> i32;
        fn MoveFileExW(
            lp_existing_file_name: *const u16,
            lp_new_file_name: *const u16,
            dw_flags: u32,
        ) -> i32;
    }

    pub(super) fn try_lock_file_exclusive(file: &File) -> io::Result<bool> {
        let mut overlapped = Overlapped::default();
        // SAFETY: `file.as_raw_handle()` is a valid open file handle and `overlapped` is stack-allocated.
        let ok = unsafe {
            LockFileEx(
                file.as_raw_handle(),
                LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
                0,
                1,
                0,
                &mut overlapped,
            )
        };
        if ok != 0 {
            return Ok(true);
        }
        let err = io::Error::last_os_error();
        if matches!(
            err.raw_os_error(),
            Some(ERROR_LOCK_VIOLATION | ERROR_SHARING_VIOLATION)
        ) {
            return Ok(false);
        }
        Err(err)
    }

    pub(super) fn unlock_file(file: &File) -> io::Result<()> {
        let mut overlapped = Overlapped::default();
        // SAFETY: Unlocks exact `[0, 1)` byte range previously locked on `file`.
        let ok = unsafe { UnlockFileEx(file.as_raw_handle(), 0, 1, 0, &mut overlapped) };
        if ok != 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub(super) fn atomic_replace_file(tmp_path: &Path, target_path: &Path) -> io::Result<()> {
        let tmp_w: Vec<u16> = tmp_path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let target_w: Vec<u16> = target_path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        // Bounded retry on ERROR_SHARING_VIOLATION (32): a concurrent reader may hold
        // `profiles.json` open without FILE_SHARE_DELETE, causing MoveFileExW to fail
        // transiently. Retry with linear backoff (5ms * attempt) up to 10 times (DS-07 fix).
        for attempt in 0..=10u32 {
            // SAFETY: Both paths are valid null-terminated UTF-16 buffers.
            let ok = unsafe {
                MoveFileExW(
                    tmp_w.as_ptr(),
                    target_w.as_ptr(),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            };
            if ok != 0 {
                return Ok(());
            }
            let err = io::Error::last_os_error();
            if matches!(err.raw_os_error(), Some(5 | ERROR_SHARING_VIOLATION)) && attempt < 10 {
                std::thread::sleep(std::time::Duration::from_millis(
                    2 * (u64::from(attempt) + 1),
                ));
                continue;
            }
            return Err(err);
        }
        unreachable!()
    }
}

#[cfg(unix)]
mod platform_impl {
    use std::fs::{self, File};
    use std::io;
    use std::os::raw::c_int;
    use std::os::unix::io::AsRawFd;
    use std::path::Path;

    const LOCK_EX: c_int = 2;
    const LOCK_NB: c_int = 4;
    const LOCK_UN: c_int = 8;

    extern "C" {
        fn flock(fd: c_int, operation: c_int) -> c_int;
    }

    pub(super) fn try_lock_file_exclusive(file: &File) -> io::Result<bool> {
        // SAFETY: `file.as_raw_fd()` is a valid open file descriptor for the duration of the call.
        let rc = unsafe { flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) };
        if rc == 0 {
            return Ok(true);
        }
        let err = io::Error::last_os_error();
        if err.kind() == io::ErrorKind::WouldBlock {
            return Ok(false);
        }
        Err(err)
    }

    pub(super) fn unlock_file(file: &File) -> io::Result<()> {
        // SAFETY: Releases advisory `flock` on valid open file descriptor.
        let rc = unsafe { flock(file.as_raw_fd(), LOCK_UN) };
        if rc == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub(super) fn atomic_replace_file(tmp_path: &Path, target_path: &Path) -> io::Result<()> {
        fs::rename(tmp_path, target_path)?;
        if let Some(parent) = target_path.parent() {
            if let Ok(dir_file) = File::open(parent) {
                let _ = dir_file.sync_all();
            }
        }
        Ok(())
    }
}

use platform_impl::{atomic_replace_file, try_lock_file_exclusive, unlock_file};

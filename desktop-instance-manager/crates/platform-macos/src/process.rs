//! macOS process table parsing (`ps`), UID & command-line ownership verification (defending
//! against recycled/foreign PIDs), primary Zalo PID protection, per-profile concurrency locks,
//! graceful shutdown (`NSRunningApplication.terminate` -> bounded isolated `SIGTERM`), and
//! non-symlink-following crash artifact recovery.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use dim_core::{
    detect_zalo_user_id, now_rfc3339_utc, DimError, PrimaryInstanceStatus, ProfileRecord,
    RecoveryReport,
};
use dim_profile_store::is_symlink_or_reparse;

use crate::layout::{
    ensure_not_macos_primary_path, macos_zalo_data_dir, normalize_macos_path_key,
    normalize_macos_unicode_key, parse_singleton_lock_target,
};
use crate::ops::{MacOsSystemOps, ProcessEntry};
use crate::window::{request_graceful_app_terminate, write_signal_json_atomic};

/// Stale lock and runtime signal filenames inside `ZaloData` cleaned during crash recovery.
pub const MACOS_STALE_ZALODATA_ARTIFACTS: &[&str] = &[
    "SingletonLock",
    "SingletonSocket",
    "SingletonCookie",
    "lockfile",
    ".quit-signal",
    ".focus-signal",
    ".window-state.json",
];

/// Maximum age before a leftover `.op-lock` from a killed CLI process is treated as stale (`30s`).
const OP_LOCK_STALE_AGE: Duration = Duration::from_secs(30);

/// Metadata file inside `<profile_dir>/tmp/` recording the expected binary launched for a profile.
pub const LAUNCH_TARGET_HINT_FILE: &str = ".launch-target.json";

/// Verified live process tree ownership snapshot for a single isolated profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedProfileProcessTree {
    /// Verified main/root PID for the profile (`None` if stopped or lock PID is stale/foreign).
    pub main_pid: Option<u32>,
    /// Full verified PID list (root + descendant helpers).
    pub process_ids: Vec<u32>,
    /// Total resident memory (`RSS * 1024`) in bytes across `process_ids`.
    pub working_set_bytes: u64,
    /// True if lockfile/symlink remnants exist on disk without a verified live owning process.
    pub stale_lock_detected: bool,
    /// Preferred lockfile path (`SingletonLock` or `lockfile`) for reporting.
    pub lockfile_path: PathBuf,
}

/// RAII guard serializing concurrent `launch`, `focus`, and `stop` operations per profile
/// across both threads (`Mutex<HashSet<String>>`) and OS processes (`<profile_dir>/tmp/.op-lock`).
#[derive(Debug)]
pub struct ProfileOperationGuard {
    profile_key: String,
    active_set: Arc<Mutex<HashSet<String>>>,
    lock_file_path: Option<PathBuf>,
}

impl ProfileOperationGuard {
    /// Acquires exclusive operation ownership for `profile_id` (and `<profile_dir>/tmp/.op-lock` if `profile_dir` exists).
    pub fn acquire(
        ops: &dyn MacOsSystemOps,
        active_set: &Arc<Mutex<HashSet<String>>>,
        profile_id: &str,
        profile_dir: Option<&Path>,
    ) -> Result<Self, DimError> {
        let key = profile_id.trim().to_ascii_lowercase();
        {
            let mut set = active_set
                .lock()
                .map_err(|_| DimError::ProcessError("operation lock mutex poisoned".to_owned()))?;
            if !set.insert(key.clone()) {
                return Err(DimError::ProcessError(format!(
                    "duplicate concurrent operation already in progress for profile '{profile_id}'"
                )));
            }
        }

        let mut acquired_file: Option<PathBuf> = None;
        if let Some(pdir) = profile_dir {
            let tmp_dir = pdir.join("tmp");
            if tmp_dir.is_dir() {
                let lock_path = tmp_dir.join(".op-lock");
                match try_create_op_lock_file(ops, &lock_path, profile_id) {
                    Ok(()) => {
                        acquired_file = Some(lock_path);
                    }
                    Err(err) => {
                        if let Ok(mut set) = active_set.lock() {
                            set.remove(&key);
                        }
                        return Err(err);
                    }
                }
            }
        }

        Ok(Self {
            profile_key: key,
            active_set: Arc::clone(active_set),
            lock_file_path: acquired_file,
        })
    }
}

impl Drop for ProfileOperationGuard {
    fn drop(&mut self) {
        if let Some(ref lock_path) = self.lock_file_path {
            let _ = remove_file_no_follow(lock_path);
        }
        if let Ok(mut set) = self.active_set.lock() {
            set.remove(&self.profile_key);
        }
    }
}

/// Parses `/bin/ps -axo pid=,ppid=,uid=,stat=,rss=,comm=,args=` (or 5-column `pid=,ppid=,uid=,rss=,args=`)
/// output into [`ProcessEntry`] rows, automatically discarding `<defunct>` zombie (`Z`) processes.
#[must_use]
pub fn parse_ps_table(ps_stdout: &str) -> Vec<ProcessEntry> {
    let mut entries = Vec::new();
    for raw_line in ps_stdout.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((pid_tok, rest)) = take_token(line) else {
            continue;
        };
        let Some((ppid_tok, rest)) = take_token(rest) else {
            continue;
        };
        let Some((uid_tok, rest)) = take_token(rest) else {
            continue;
        };
        let Some((tok4, rest_after_4)) = take_token(rest) else {
            continue;
        };

        let (Ok(pid), Ok(ppid), Ok(uid)) = (
            pid_tok.parse::<u32>(),
            ppid_tok.parse::<u32>(),
            uid_tok.parse::<u32>(),
        ) else {
            continue;
        };
        if pid == 0 {
            continue;
        }

        // Support both `pid ppid uid stat rss args...` and `pid ppid uid rss args...`.
        let (rss_kib, args_slice) = if let Ok(rss_direct) = tok4.parse::<u64>() {
            (rss_direct, rest_after_4.trim())
        } else {
            // `tok4` is `stat` (e.g., `S`, `Ss`, `R+`, `Z`). Ignore zombie processes!
            if tok4.starts_with('Z') || tok4.starts_with('z') {
                continue;
            }
            let Some((rss_tok, rest_after_rss)) = take_token(rest_after_4) else {
                continue;
            };
            let Ok(parsed_rss) = rss_tok.parse::<u64>() else {
                continue;
            };
            (parsed_rss, rest_after_rss.trim())
        };

        if args_slice.is_empty() || args_slice.contains("<defunct>") {
            continue;
        }

        let executable = extract_executable_from_args(args_slice);
        entries.push(ProcessEntry {
            pid,
            ppid,
            uid,
            rss_kib,
            executable,
            args: args_slice.to_owned(),
        });
    }
    entries
}

/// Reads candidate PID from Chromium `SingletonLock` symlink (`<host>-<pid>`) or fixture JSON `lockfile`.
#[must_use]
pub fn read_singleton_or_json_pid(zalo_data_dir: &Path) -> Option<u32> {
    let lock_link = zalo_data_dir.join("SingletonLock");
    if let Ok(target) = fs::read_link(&lock_link) {
        if let Some((_, pid)) = parse_singleton_lock_target(&target.to_string_lossy()) {
            return Some(pid);
        }
    }
    let lock_json = zalo_data_dir.join("lockfile");
    if let Ok(raw) = fs::read_to_string(lock_json) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(pid) = val
                .get("pid")
                .and_then(serde_json::Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
                .filter(|&p| p > 0)
            {
                return Some(pid);
            }
        }
    }
    None
}

/// Records the executable or bundle launched for a profile so PID ownership checks recognize custom binaries.
pub fn write_launch_target_hint(profile_dir: &Path, profile_id: &str, executable_or_bundle: &Path) {
    let hint_path = profile_dir.join("tmp").join(LAUNCH_TARGET_HINT_FILE);
    let payload = serde_json::json!({
        "profile_id": profile_id,
        "target": executable_or_bundle.to_string_lossy(),
        "recorded_at": now_rfc3339_utc(),
    });
    if let Ok(text) = serde_json::to_string_pretty(&payload) {
        let _ = fs::write(hint_path, text);
    }
}

/// Inspects the untouched primary user Zalo installation (`~/Library/Application Support/ZaloData`
/// plus any running unmanaged `Zalo.app` process trees) to build the protected PID set.
pub fn inspect_primary_macos(ops: &dyn MacOsSystemOps) -> Result<PrimaryInstanceStatus, DimError> {
    if !ops.supports_runtime_ops() {
        return Err(DimError::UnsupportedPlatform(
            "macOS primary inspection requires macOS".to_owned(),
        ));
    }

    let real_home = ops
        .user_home_dir()
        .unwrap_or_else(|| PathBuf::from("/var/empty"));
    let primary_zalo_data = macos_zalo_data_dir(&real_home);
    let singleton_lock = primary_zalo_data.join("SingletonLock");
    let fixture_lock = primary_zalo_data.join("lockfile");
    let lock_path =
        if symlink_or_file_present(&singleton_lock) || !symlink_or_file_present(&fixture_lock) {
            singleton_lock
        } else {
            fixture_lock
        };

    let processes = ops.list_processes()?;
    let current_uid = ops.current_uid();
    let mut protected_set: HashSet<u32> = HashSet::new();

    // 1. Check PID referenced by primary `~/Library/Application Support/ZaloData` lockfile/SingletonLock.
    if let Some(lock_pid) = read_singleton_or_json_pid(&primary_zalo_data) {
        if let Some(entry) = processes.iter().find(|p| p.pid == lock_pid) {
            if uid_matches(entry.uid, current_uid)
                && looks_like_zalo_or_fixture_entry(entry, None)
                && !tree_references_non_primary_profile(lock_pid, &primary_zalo_data, &processes)
            {
                for pid in collect_descendant_pids(lock_pid, &processes) {
                    protected_set.insert(pid);
                }
            }
        }
    }

    // 2. Also protect any running official `Zalo.app` root process tree NOT belonging to managed `Profiles/`.
    let by_pid: HashMap<u32, &ProcessEntry> = processes.iter().map(|p| (p.pid, p)).collect();
    for proc_entry in &processes {
        if !uid_matches(proc_entry.uid, current_uid) {
            continue;
        }
        if !looks_like_official_zalo_bundle_process(proc_entry) {
            continue;
        }
        // Only inspect root Zalo browser processes (whose parent is launchd/1 or not another Zalo helper).
        if let Some(parent) = by_pid.get(&proc_entry.ppid) {
            if looks_like_official_zalo_bundle_process(parent) {
                continue;
            }
        }
        if !tree_references_non_primary_profile(proc_entry.pid, &primary_zalo_data, &processes) {
            for pid in collect_descendant_pids(proc_entry.pid, &processes) {
                protected_set.insert(pid);
            }
        }
    }

    let mut protected_pids: Vec<u32> = protected_set.into_iter().collect();
    protected_pids.sort_unstable();

    let working_set_bytes = sum_working_set_bytes(&protected_pids, &processes);
    let detected_uid = detect_zalo_user_id(&primary_zalo_data);
    let running = !protected_pids.is_empty();

    Ok(PrimaryInstanceStatus {
        zalo_data_dir: primary_zalo_data,
        lockfile_path: lock_path,
        running,
        process_count: protected_pids.len(),
        protected_pids,
        working_set_bytes,
        detected_zalo_user_id: detected_uid,
        notes: vec![
            "Primary macOS ZaloData path and all unmanaged Zalo.app PIDs are strictly protected."
                .to_owned(),
        ],
    })
}

/// Resolves and verifies the live process tree owned by `profile_dir`, rejecting foreign/recycled PIDs.
pub fn resolve_owned_profile_tree(
    ops: &dyn MacOsSystemOps,
    profile_dir: &Path,
    primary_pids: &HashSet<u32>,
) -> Result<VerifiedProfileProcessTree, DimError> {
    let zalo_data = macos_zalo_data_dir(profile_dir);
    let singleton_lock = zalo_data.join("SingletonLock");
    let fixture_lock = zalo_data.join("lockfile");
    let runtime_pid_path = profile_dir.join(".runtime-pid");

    let has_singleton = symlink_or_file_present(&singleton_lock);
    let has_fixture_lock = symlink_or_file_present(&fixture_lock);
    let has_runtime_pid = symlink_or_file_present(&runtime_pid_path);
    let has_socket_or_cookie = symlink_or_file_present(&zalo_data.join("SingletonSocket"))
        || symlink_or_file_present(&zalo_data.join("SingletonCookie"));

    let lockfile_path = if has_singleton || !has_fixture_lock {
        singleton_lock
    } else {
        fixture_lock
    };

    let processes = ops.list_processes()?;
    let current_uid = ops.current_uid();
    let real_home = ops.user_home_dir();
    let target_hint = read_launch_target_hint(profile_dir);

    let mut candidates: Vec<u32> = Vec::new();
    if let Some(lock_pid) = read_singleton_or_json_pid(&zalo_data) {
        candidates.push(lock_pid);
    }
    if let Some(rt_pid) = read_runtime_pid_file(&runtime_pid_path) {
        if !candidates.contains(&rt_pid) {
            candidates.push(rt_pid);
        }
    }

    let mut verified_main: Option<u32> = None;
    let mut verified_tree: Vec<u32> = Vec::new();

    for cand_pid in candidates {
        if verify_candidate_pid_ownership(
            cand_pid,
            profile_dir,
            &zalo_data,
            real_home.as_deref(),
            current_uid,
            primary_pids,
            target_hint.as_deref(),
            &processes,
        ) {
            verified_main = Some(cand_pid);
            verified_tree = collect_descendant_pids(cand_pid, &processes);
            break;
        }
    }

    // Fallback: scan live `ps` table for any process explicitly carrying `--user-data-dir=<zalo_data>`
    // or `<profile_dir>/Library/Application Support/ZaloData`.
    if verified_main.is_none() {
        if let Some(found_root) = find_profile_root_by_cmdline(
            profile_dir,
            &zalo_data,
            current_uid,
            primary_pids,
            &processes,
        ) {
            verified_main = Some(found_root);
            verified_tree = collect_descendant_pids(found_root, &processes);
        }
    }

    let working_set_bytes = sum_working_set_bytes(&verified_tree, &processes);
    let any_lock_artifact =
        has_singleton || has_fixture_lock || has_runtime_pid || has_socket_or_cookie;
    let stale_lock_detected = verified_main.is_none() && any_lock_artifact;

    Ok(VerifiedProfileProcessTree {
        main_pid: verified_main,
        process_ids: verified_tree,
        working_set_bytes,
        stale_lock_detected,
        lockfile_path,
    })
}

/// Collects `root_pid` and all recursive child/descendant PIDs (`ppid -> pid`) in deterministic sorted order.
#[must_use]
pub fn collect_descendant_pids(root_pid: u32, processes: &[ProcessEntry]) -> Vec<u32> {
    let mut children_map: HashMap<u32, Vec<u32>> = HashMap::new();
    let mut alive_set: HashSet<u32> = HashSet::with_capacity(processes.len());

    for p in processes {
        alive_set.insert(p.pid);
        children_map.entry(p.ppid).or_default().push(p.pid);
    }
    if !alive_set.contains(&root_pid) {
        return Vec::new();
    }

    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    visited.insert(root_pid);
    queue.push_back(root_pid);

    while let Some(curr) = queue.pop_front() {
        if let Some(kids) = children_map.get(&curr) {
            for &kid in kids {
                if visited.insert(kid) {
                    queue.push_back(kid);
                }
            }
        }
    }

    let mut out: Vec<u32> = visited.into_iter().collect();
    // Keep `root_pid` first, followed by sorted child PIDs.
    out.retain(|&p| p != root_pid);
    out.sort_unstable();
    let mut ordered = Vec::with_capacity(out.len() + 1);
    ordered.push(root_pid);
    ordered.extend(out);
    ordered
}

/// Computes total regular-file bytes inside `profile_dir` without traversing symlinks or junctions.
#[must_use]
pub fn compute_profile_disk_usage_bytes(profile_dir: &Path) -> u64 {
    let Ok(root_meta) = fs::symlink_metadata(profile_dir) else {
        return 0;
    };
    if is_symlink_or_reparse(&root_meta) || !root_meta.is_dir() {
        return 0;
    }

    let mut total = 0u64;
    let mut stack = vec![profile_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            if is_symlink_or_reparse(&meta) {
                continue;
            }
            if meta.is_dir() {
                stack.push(path);
            } else if meta.is_file() {
                total = total.saturating_add(meta.len());
            }
        }
    }
    total
}

/// Gracefully stops a verified isolated profile process tree while strictly protecting primary PIDs
/// and refusing to signal foreign/recycled PIDs.
pub fn stop_isolated_profile_tree(
    ops: &dyn MacOsSystemOps,
    profile_dir: &Path,
    zalo_data_dir: &Path,
    candidate_main_pid: Option<u32>,
    candidate_tree_pids: &[u32],
    protected_primary_pids: &[u32],
    timeout_ms: u64,
) -> Result<u32, DimError> {
    let real_home = ops.user_home_dir();
    ensure_not_macos_primary_path(profile_dir, real_home.as_deref())?;
    ensure_not_macos_primary_path(zalo_data_dir, real_home.as_deref())?;

    let mut protected_set: HashSet<u32> = protected_primary_pids.iter().copied().collect();
    if let Ok(live_primary) = inspect_primary_macos(ops) {
        if normalize_macos_path_key(zalo_data_dir)
            == normalize_macos_path_key(&live_primary.zalo_data_dir)
        {
            return Err(DimError::ProtectedPrimaryProcess(format!(
                "refused to stop primary macOS ZaloData directory '{}'",
                live_primary.zalo_data_dir.display()
            )));
        }
        protected_set.extend(live_primary.protected_pids);
    }

    if let Some(mpid) = candidate_main_pid {
        if protected_set.contains(&mpid) {
            return Err(DimError::ProtectedPrimaryProcess(format!(
                "refused to stop protected macOS primary PID {mpid}"
            )));
        }
    }
    for &pid in candidate_tree_pids {
        if protected_set.contains(&pid) {
            return Err(DimError::ProtectedPrimaryProcess(format!(
                "refused to stop protected macOS primary child PID {pid}"
            )));
        }
    }

    // Re-verify live process ownership right before signaling so recycled PIDs are never touched!
    let live_tree = resolve_owned_profile_tree(ops, profile_dir, &protected_set)?;
    let Some(main_pid) = live_tree.main_pid else {
        let _ = remove_file_no_follow(&profile_dir.join(".runtime-pid"));
        return Ok(0);
    };
    if protected_set.contains(&main_pid) {
        return Err(DimError::ProtectedPrimaryProcess(format!(
            "refused to stop protected macOS primary PID {main_pid}"
        )));
    }

    let initial_owned_pids = live_tree.process_ids.clone();

    // Step 1: Write `.quit-signal` for cooperative fixtures/helpers.
    if zalo_data_dir.is_dir() {
        let _ = write_signal_json_atomic(
            &zalo_data_dir.join(".quit-signal"),
            &serde_json::json!({
                "requested_at": now_rfc3339_utc(),
                "target_pid": main_pid,
            }),
        );
    }

    // Step 2: Request graceful Cocoa `NSRunningApplication.terminate()` (`⌘Q`).
    let _ = request_graceful_app_terminate(ops, main_pid);

    // Give SQLite WAL (`ftsm_*.db-wal`) extra graceful flush headroom before escalating to SIGTERM.
    let has_open_wal = has_active_sqlite_wal(zalo_data_dir);
    let graceful_budget_ms = if has_open_wal {
        timeout_ms.clamp(250, 2_000)
    } else {
        (timeout_ms / 2).clamp(150, 1_000)
    };
    let overall_deadline = Instant::now() + Duration::from_millis(timeout_ms.max(150));
    let graceful_deadline = Instant::now() + Duration::from_millis(graceful_budget_ms);

    while Instant::now() < graceful_deadline {
        let check = resolve_owned_profile_tree(ops, profile_dir, &protected_set)?;
        if check.main_pid.is_none() {
            let _ = remove_file_no_follow(&profile_dir.join(".runtime-pid"));
            return Ok(u32::try_from(initial_owned_pids.len()).unwrap_or(1));
        }
        std::thread::sleep(Duration::from_millis(40));
    }

    // Step 3: Bounded escalation to isolated `SIGTERM` on verified owned non-primary PIDs only
    // (also resolves the known macOS Zalo login-screen hang after `terminate()`).
    let recheck = resolve_owned_profile_tree(ops, profile_dir, &protected_set)?;
    if recheck.main_pid.is_some() {
        // Signal helper children first, then main PID, strictly excluding any protected PID.
        for &pid in recheck.process_ids.iter().rev() {
            if !protected_set.contains(&pid) {
                let _ = ops.send_sigterm(pid);
            }
        }
    }

    while Instant::now() < overall_deadline {
        let check = resolve_owned_profile_tree(ops, profile_dir, &protected_set)?;
        if check.main_pid.is_none() {
            let _ = remove_file_no_follow(&profile_dir.join(".runtime-pid"));
            return Ok(u32::try_from(initial_owned_pids.len()).unwrap_or(1));
        }
        std::thread::sleep(Duration::from_millis(40));
    }

    let final_check = resolve_owned_profile_tree(ops, profile_dir, &protected_set)?;
    if final_check.main_pid.is_none() {
        let _ = remove_file_no_follow(&profile_dir.join(".runtime-pid"));
        Ok(u32::try_from(initial_owned_pids.len()).unwrap_or(1))
    } else {
        Ok(0)
    }
}

/// Safely removes stale `SingletonLock`, `SingletonSocket`, `SingletonCookie`, `lockfile`,
/// `.runtime-pid`, `.quit-signal`, `.focus-signal`, and `.window-state.json` without following symlinks.
///
/// Refuses cleanup with `DimError::ProfileRunning` if `is_running` is `true`.
pub fn recover_macos_stale_artifacts(
    profile: &ProfileRecord,
    profile_dir: &Path,
    is_running: bool,
) -> Result<RecoveryReport, DimError> {
    if is_running {
        return Err(DimError::ProfileRunning(format!(
            "{} ({}) is currently running; refusing stale lock cleanup",
            profile.name, profile.id
        )));
    }

    let zalo_data = macos_zalo_data_dir(profile_dir);
    let mut removed = Vec::new();

    for artifact in MACOS_STALE_ZALODATA_ARTIFACTS {
        let candidate = zalo_data.join(artifact);
        if remove_file_no_follow(&candidate)? {
            removed.push(candidate.display().to_string());
        }
    }

    let runtime_pid = profile_dir.join(".runtime-pid");
    if remove_file_no_follow(&runtime_pid)? {
        removed.push(runtime_pid.display().to_string());
    }

    let recovered = !removed.is_empty();
    let summary = if recovered {
        format!(
            "Recovered {} stale macOS lock/signal artifact(s) for profile '{}'",
            removed.len(),
            profile.name
        )
    } else {
        format!(
            "No stale macOS lock artifacts present for profile '{}'",
            profile.name
        )
    };

    Ok(RecoveryReport {
        recovered,
        removed_files: removed,
        summary,
    })
}

/// Checks via `symlink_metadata` (`lstat`) whether a file or (possibly dangling) symlink exists.
#[must_use]
pub fn symlink_or_file_present(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

/// Removes a regular file or symlink without ever following symlinks into directories.
pub fn remove_file_no_follow(path: &Path) -> Result<bool, DimError> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            if meta.is_dir() && !is_symlink_or_reparse(&meta) {
                return Ok(false);
            }
            fs::remove_file(path)?;
            Ok(true)
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(DimError::Io(err)),
    }
}

#[allow(clippy::too_many_arguments)]
fn verify_candidate_pid_ownership(
    candidate_pid: u32,
    profile_dir: &Path,
    zalo_data_dir: &Path,
    real_home: Option<&Path>,
    current_uid: u32,
    primary_pids: &HashSet<u32>,
    target_hint: Option<&str>,
    processes: &[ProcessEntry],
) -> bool {
    if candidate_pid <= 1 || primary_pids.contains(&candidate_pid) {
        return false;
    }
    let Some(root_entry) = processes.iter().find(|p| p.pid == candidate_pid) else {
        return false;
    };
    if !uid_matches(root_entry.uid, current_uid) {
        return false;
    }

    let tree_pids = collect_descendant_pids(candidate_pid, processes);
    if tree_pids.iter().any(|p| primary_pids.contains(p)) {
        return false;
    }

    let profile_key = normalize_macos_path_key(profile_dir);
    let zalo_key = normalize_macos_path_key(zalo_data_dir);
    let primary_key = real_home.map(|h| normalize_macos_path_key(&macos_zalo_data_dir(h)));

    let mut explicit_path_match = false;
    for pid in &tree_pids {
        let Some(entry) = processes.iter().find(|p| p.pid == *pid) else {
            continue;
        };
        let args_key = normalize_cmd_for_match(&entry.args);

        // Reject if any process in the tree explicitly points at primary ZaloData.
        if let Some(ref pkey) = primary_key {
            if !pkey.is_empty() && args_key.contains(pkey) && !args_key.contains(&profile_key) {
                return false;
            }
        }

        // Reject if any process in the tree points at a DIFFERENT managed `/Profiles/<other>` folder.
        if references_different_managed_profile(&args_key, &profile_key) {
            return false;
        }

        if (!profile_key.is_empty() && args_key.contains(&profile_key))
            || (!zalo_key.is_empty() && args_key.contains(&zalo_key))
        {
            explicit_path_match = true;
        }
    }

    if explicit_path_match {
        return true;
    }

    // Without `--user-data-dir` on argv yet, require positive executable/bundle identity match
    // (never matching unrelated OS daemons/apps like Safari, Finder, launchd, syslogd).
    looks_like_zalo_or_fixture_entry(root_entry, target_hint)
}

fn find_profile_root_by_cmdline(
    profile_dir: &Path,
    zalo_data_dir: &Path,
    current_uid: u32,
    primary_pids: &HashSet<u32>,
    processes: &[ProcessEntry],
) -> Option<u32> {
    let profile_key = normalize_macos_path_key(profile_dir);
    let zalo_key = normalize_macos_path_key(zalo_data_dir);
    let by_pid: HashMap<u32, &ProcessEntry> = processes.iter().map(|p| (p.pid, p)).collect();

    for entry in processes {
        if !uid_matches(entry.uid, current_uid) || primary_pids.contains(&entry.pid) {
            continue;
        }
        let args_key = normalize_cmd_for_match(&entry.args);
        if (!zalo_key.is_empty() && args_key.contains(&zalo_key))
            || (!profile_key.is_empty() && args_key.contains(&profile_key))
        {
            // Walk up `ppid` if the matched process is a child helper of a Zalo/Electron main process.
            let mut root = entry.pid;
            let mut hops = 0usize;
            while hops < 8 {
                hops += 1;
                let Some(curr) = by_pid.get(&root) else {
                    break;
                };
                if curr.ppid <= 1 || primary_pids.contains(&curr.ppid) {
                    break;
                }
                let Some(parent) = by_pid.get(&curr.ppid) else {
                    break;
                };
                if !uid_matches(parent.uid, current_uid) {
                    break;
                }
                let parent_args = normalize_cmd_for_match(&parent.args);
                if references_different_managed_profile(&parent_args, &profile_key) {
                    break;
                }
                if looks_like_zalo_or_fixture_entry(parent, None)
                    || (!profile_key.is_empty() && parent_args.contains(&profile_key))
                {
                    root = parent.pid;
                } else {
                    break;
                }
            }
            return Some(root);
        }
    }
    None
}

fn references_different_managed_profile(args_key: &str, expected_profile_key: &str) -> bool {
    let marker = "/profiles/";
    let lower_args = args_key.to_lowercase();
    let lower_expected = expected_profile_key.to_lowercase();
    if let Some(idx) = lower_args.find(marker) {
        let after = &lower_args[idx + marker.len()..];
        let seg: String = after
            .chars()
            .take_while(|&c| c != '/' && c != '"' && c != '\'' && !c.is_whitespace())
            .collect();
        if !seg.is_empty() && !lower_expected.ends_with(&format!("/profiles/{seg}")) {
            return true;
        }
    }
    false
}

fn tree_references_non_primary_profile(
    root_pid: u32,
    primary_zalo_data: &Path,
    processes: &[ProcessEntry],
) -> bool {
    let primary_key = normalize_macos_path_key(primary_zalo_data).to_lowercase();
    let tree = collect_descendant_pids(root_pid, processes);
    for pid in tree {
        if let Some(entry) = processes.iter().find(|p| p.pid == pid) {
            let norm = normalize_cmd_for_match(&entry.args).to_lowercase();
            if norm.contains("/multizalodesktop/profiles/") || norm.contains("/profiles/") {
                return true;
            }
            if let Some(idx) = norm.find("--user-data-dir=") {
                let val = norm[idx + "--user-data-dir=".len()..].trim();
                if !val.is_empty() && (primary_key.is_empty() || !val.starts_with(&primary_key)) {
                    return true;
                }
            }
        }
    }
    false
}

fn looks_like_official_zalo_bundle_process(entry: &ProcessEntry) -> bool {
    let norm = entry.args.replace('\\', "/");
    norm.contains("Zalo.app/Contents/MacOS/Zalo")
        || norm.contains("Zalo.app/Contents/Frameworks/")
        || norm.ends_with("/Contents/MacOS/Zalo")
}

fn looks_like_zalo_or_fixture_entry(entry: &ProcessEntry, target_hint: Option<&str>) -> bool {
    if looks_like_official_zalo_bundle_process(entry) {
        return true;
    }
    let norm_args = entry.args.replace('\\', "/").to_ascii_lowercase();
    let norm_exe = entry.executable.replace('\\', "/").to_ascii_lowercase();

    let known_markers = [
        "fixture-macos-app.mjs",
        "fixture-app.mjs",
        "zalofixture",
        "dim-test-fixture",
    ];
    if known_markers
        .iter()
        .any(|m| norm_args.contains(m) || norm_exe.contains(m))
    {
        return true;
    }

    if let Some(hint) = target_hint {
        let norm_hint = hint.replace('\\', "/").to_ascii_lowercase();
        if !norm_hint.is_empty()
            && (norm_args.contains(&norm_hint) || norm_exe.contains(&norm_hint))
        {
            return true;
        }
    }
    false
}

fn normalize_cmd_for_match(cmd: &str) -> String {
    let forward = cmd.replace('\\', "/");
    let nfd = normalize_macos_unicode_key(&forward);
    if cfg!(windows) {
        nfd.to_lowercase()
    } else {
        nfd
    }
}

const fn uid_matches(proc_uid: u32, current_uid: u32) -> bool {
    proc_uid == current_uid || current_uid == 0
}

fn sum_working_set_bytes(pids: &[u32], processes: &[ProcessEntry]) -> u64 {
    let pid_set: HashSet<u32> = pids.iter().copied().collect();
    processes
        .iter()
        .filter(|p| pid_set.contains(&p.pid))
        .fold(0u64, |acc, p| {
            acc.saturating_add(p.rss_kib.saturating_mul(1024))
        })
}

fn read_runtime_pid_file(path: &Path) -> Option<u32> {
    let raw = fs::read_to_string(path).ok()?;
    raw.trim().parse::<u32>().ok().filter(|&p| p > 0)
}

fn read_launch_target_hint(profile_dir: &Path) -> Option<String> {
    let raw = fs::read_to_string(profile_dir.join("tmp").join(LAUNCH_TARGET_HINT_FILE)).ok()?;
    let val = serde_json::from_str::<serde_json::Value>(&raw).ok()?;
    val.get("target")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

fn has_active_sqlite_wal(zalo_data_dir: &Path) -> bool {
    let db_root = zalo_data_dir.join("Database").join("_production");
    let Ok(entries) = fs::read_dir(db_root) else {
        return false;
    };
    for user_entry in entries.flatten() {
        if let Ok(files) = fs::read_dir(user_entry.path()) {
            for f in files.flatten() {
                let name = f.file_name();
                let s = name.to_string_lossy();
                if s.ends_with(".db-wal") {
                    if let Ok(meta) = f.metadata() {
                        if meta.len() > 0 {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

fn try_create_op_lock_file(
    ops: &dyn MacOsSystemOps,
    lock_path: &Path,
    profile_id: &str,
) -> Result<(), DimError> {
    for attempt in 0..2 {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(lock_path)
        {
            Ok(mut f) => {
                let payload = serde_json::json!({
                    "pid": std::process::id(),
                    "profile_id": profile_id,
                    "created_at": now_rfc3339_utc(),
                });
                let _ = f.write_all(payload.to_string().as_bytes());
                return Ok(());
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists && attempt == 0 => {
                if is_op_lock_stale(ops, lock_path) {
                    let _ = remove_file_no_follow(lock_path);
                    continue;
                }
                return Err(DimError::ProcessError(format!(
                    "duplicate concurrent operation locked on disk for profile '{profile_id}'"
                )));
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(DimError::ProcessError(format!(
                    "duplicate concurrent operation locked on disk for profile '{profile_id}'"
                )));
            }
            Err(err) => return Err(DimError::Io(err)),
        }
    }
    Err(DimError::ProcessError(format!(
        "failed to acquire operation lock for profile '{profile_id}'"
    )))
}

fn is_op_lock_stale(ops: &dyn MacOsSystemOps, lock_path: &Path) -> bool {
    if let Ok(meta) = fs::symlink_metadata(lock_path) {
        if is_symlink_or_reparse(&meta) {
            return true;
        }
        if let Ok(modified) = meta.modified() {
            if SystemTime::now()
                .duration_since(modified)
                .unwrap_or(Duration::ZERO)
                >= OP_LOCK_STALE_AGE
            {
                return true;
            }
        }
    }
    if let Ok(raw) = fs::read_to_string(lock_path) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(pid) = val
                .get("pid")
                .and_then(serde_json::Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
            {
                if pid != std::process::id() && !ops.is_pid_alive(pid) {
                    return true;
                }
            }
        }
    }
    false
}

fn take_token(s: &str) -> Option<(&str, &str)> {
    let trimmed = s.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    match trimmed.find(char::is_whitespace) {
        Some(idx) => Some((&trimmed[..idx], &trimmed[idx..])),
        None => Some((trimmed, "")),
    }
}

fn extract_executable_from_args(args: &str) -> String {
    if let Some(app_idx) = args.find(".app/Contents/MacOS/") {
        let prefix_end = app_idx + ".app/Contents/MacOS/".len();
        let after = &args[prefix_end..];
        let exec_part = after.split(" --").next().unwrap_or(after).trim();
        return format!("{}{}", &args[..prefix_end], exec_part);
    }
    args.split_whitespace()
        .next()
        .unwrap_or_default()
        .to_owned()
}

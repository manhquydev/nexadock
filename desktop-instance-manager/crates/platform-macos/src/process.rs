//! Multi-evidence fail-closed macOS process ownership (`SingletonLock` + `.zalo-profile-id` +
//! `.runtime-launch.json` + `ps -o lstart=` start-time verification), primary & unmanaged Zalo
//! protection, orphan helper tracking (`ppid == 1`), strict 8-rule graceful shutdown, and
//! non-symlink-following crash recovery.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use dim_core::{
    detect_zalo_user_id, now_rfc3339_utc, DimError, PrimaryInstanceStatus, ProfileRecord,
    RecoveryReport,
};
use dim_profile_store::{is_symlink_or_reparse, verify_profile_id_marker};

use crate::layout::{
    cleanup_profile_short_tmpdir, ensure_not_macos_primary_path, macos_zalo_data_dir,
    normalize_macos_path_key, normalize_macos_unicode_key, parse_singleton_lock_target,
    MACOS_CALL_SOCKETS,
};
use crate::ops::{MacOsSystemOps, ProcessEntry};
use crate::window::{
    request_graceful_app_terminate, write_signal_json_atomic, MACOS_WINDOW_STATE_FILE,
};

static LAUNCH_REC_SEQ: AtomicU64 = AtomicU64::new(1);

/// Stale lock and runtime signal filenames inside `ZaloData` cleaned during crash recovery.
///
/// Note: Global `ZaloCall` sockets (`/tmp/socketzalorecv2021`, `/tmp/socketzalosend2021`) are
/// strictly excluded and NEVER touched!
pub const MACOS_STALE_ZALODATA_ARTIFACTS: &[&str] = &[
    "SingletonLock",
    "SingletonSocket",
    "SingletonCookie",
    "lockfile",
    ".quit-signal",
    ".focus-signal",
    ".window-state.json",
    ".helper-pids.json",
    "children.json",
];

/// Structured launch provenance file persisted inside `<profile_dir>/.runtime-launch.json`.
pub const RUNTIME_LAUNCH_RECORD_FILE: &str = ".runtime-launch.json";

/// Plaintext PID hint file persisted inside `<profile_dir>/.runtime-pid`.
pub const RUNTIME_PID_FILE: &str = ".runtime-pid";

/// Legacy target hint inside `<profile_dir>/tmp/.launch-target.json`.
pub const LAUNCH_TARGET_HINT_FILE: &str = ".launch-target.json";

/// Maximum age before a leftover `.op-lock` from a killed CLI process is treated as stale (`30s`).
const OP_LOCK_STALE_AGE: Duration = Duration::from_secs(30);

/// Structured launch provenance binding a spawned OS PID and its kernel creation timestamp
/// (`process_start_token`) to a specific managed `profile_id` (`P0-3`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeLaunchRecord {
    /// Schema version (`1`).
    pub schema_version: u32,
    /// Root OS PID recorded at launch.
    pub pid: u32,
    /// Kernel process start token (`ps -o lstart=` or Windows `FILETIME`) defeating PID reuse.
    pub process_start_token: Option<String>,
    /// Managed 8-hex profile identifier.
    pub profile_id: String,
    /// Managed isolated profile root path.
    pub profile_dir: PathBuf,
    /// Managed isolated `ZaloData` path.
    pub zalo_data_dir: PathBuf,
    /// Executable or `.app` bundle path launched.
    pub executable_path: String,
    /// RFC 3339 UTC timestamp when launched.
    pub launched_at_utc: String,
}

/// Verified live process tree ownership snapshot for a single isolated profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedProfileProcessTree {
    /// Verified main/root PID for the profile (`None` if stopped or lock PID is stale/foreign/recycled).
    pub main_pid: Option<u32>,
    /// Full verified PID list (root + descendant helpers + `ppid == 1` profile-tagged orphan helpers).
    pub process_ids: Vec<u32>,
    /// Total resident memory (`RSS * 1024`) in bytes across `process_ids`.
    pub working_set_bytes: u64,
    /// True if lockfile/symlink/runtime remnants exist on disk without a verified live owning process.
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
            if fs::symlink_metadata(&tmp_dir).is_ok_and(|m| m.is_dir()) {
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

/// Parses `/bin/ps -axo pid=,ppid=,uid=,stat=,rss=,lstart=,args=` (as well as legacy 5/6-column `ps` lines
/// without `lstart`) into [`ProcessEntry`] rows, preserving spaces in `.app` paths and discarding `Z` zombies.
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

        let (rss_kib, after_rss) = if let Ok(rss_direct) = tok4.parse::<u64>() {
            (rss_direct, rest_after_4.trim())
        } else {
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

        // Detect optional 5-token `lstart` (`Mon Nov 17 23:15:02 2026`).
        let (start_token, args_slice) = match try_take_lstart(after_rss) {
            Some((lstart_str, rem)) => (Some(lstart_str), rem.trim()),
            None => (None, after_rss),
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
            start_token,
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
    } else if fs::symlink_metadata(&lock_link).is_ok_and(|m| m.is_file()) {
        // Fallback when running `fixture-macos-app.mjs` on a Windows host without symlink privilege.
        if let Ok(raw_target) = fs::read_to_string(&lock_link) {
            if let Some((_, pid)) = parse_singleton_lock_target(&raw_target) {
                return Some(pid);
            }
        }
    }

    let lock_json = zalo_data_dir.join("lockfile");
    if fs::symlink_metadata(&lock_json).is_ok_and(|m| m.is_file()) {
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
    }
    None
}

/// Reads `<profile_dir>/.runtime-pid` (without following symlinks).
#[must_use]
pub fn read_runtime_pid(profile_dir: &Path) -> Option<u32> {
    let path = profile_dir.join(RUNTIME_PID_FILE);
    if !fs::symlink_metadata(&path).is_ok_and(|m| m.is_file()) {
        return None;
    }
    let raw = fs::read_to_string(path).ok()?;
    raw.trim().parse::<u32>().ok().filter(|&p| p > 0)
}

/// Reads `<profile_dir>/.runtime-launch.json` (without following symlinks).
#[must_use]
pub fn read_runtime_launch_record(profile_dir: &Path) -> Option<RuntimeLaunchRecord> {
    let path = profile_dir.join(RUNTIME_LAUNCH_RECORD_FILE);
    if !fs::symlink_metadata(&path).is_ok_and(|m| m.is_file()) {
        return None;
    }
    let raw = fs::read_to_string(path).ok()?;
    let val = serde_json::from_str::<serde_json::Value>(&raw).ok()?;
    if val
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        != Some(1)
    {
        return None;
    }
    let pid = val
        .get("pid")
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .filter(|&p| p > 0)?;
    let process_start_token = val
        .get("process_start_token")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let profile_id = val
        .get("profile_id")
        .and_then(serde_json::Value::as_str)?
        .to_owned();
    let p_dir = PathBuf::from(val.get("profile_dir").and_then(serde_json::Value::as_str)?);
    let z_dir = PathBuf::from(
        val.get("zalo_data_dir")
            .and_then(serde_json::Value::as_str)?,
    );
    let executable_path = val
        .get("executable_path")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let launched_at_utc = val
        .get("launched_at_utc")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();

    Some(RuntimeLaunchRecord {
        schema_version: 1,
        pid,
        process_start_token,
        profile_id,
        profile_dir: p_dir,
        zalo_data_dir: z_dir,
        executable_path,
        launched_at_utc,
    })
}

/// Atomically writes `<profile_dir>/.runtime-launch.json` and `<profile_dir>/.runtime-pid`.
pub fn write_runtime_launch_record(
    profile_dir: &Path,
    record: &RuntimeLaunchRecord,
) -> Result<(), DimError> {
    let rec_path = profile_dir.join(RUNTIME_LAUNCH_RECORD_FILE);
    let pid_path = profile_dir.join(RUNTIME_PID_FILE);
    for p in [&rec_path, &pid_path] {
        if let Ok(meta) = fs::symlink_metadata(p) {
            if is_symlink_or_reparse(&meta) {
                return Err(DimError::PathTraversal(format!(
                    "refusing to write through symlinked runtime file '{}'",
                    p.display()
                )));
            }
        }
    }

    let payload = serde_json::json!({
        "schema_version": record.schema_version,
        "pid": record.pid,
        "process_start_token": record.process_start_token,
        "profile_id": record.profile_id,
        "profile_dir": record.profile_dir.to_string_lossy(),
        "zalo_data_dir": record.zalo_data_dir.to_string_lossy(),
        "executable_path": record.executable_path,
        "launched_at_utc": record.launched_at_utc,
    });
    let text = serde_json::to_string_pretty(&payload)?;
    let seq = LAUNCH_REC_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = profile_dir.join(format!(".runtime-launch.{}.{seq}.tmp", std::process::id()));
    {
        let mut f = File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.write_all(b"\n")?;
        f.sync_all()?;
    }
    fs::rename(&tmp, &rec_path)?;
    fs::write(&pid_path, format!("{}\n", record.pid))?;
    Ok(())
}

/// Removes `.runtime-pid` and `.runtime-launch.json` safely without following symlinks.
pub fn clear_runtime_launch_files(profile_dir: &Path) -> Result<usize, DimError> {
    let mut removed = 0usize;
    for name in [RUNTIME_PID_FILE, RUNTIME_LAUNCH_RECORD_FILE] {
        if remove_file_no_follow(&profile_dir.join(name))? {
            removed += 1;
        }
    }
    Ok(removed)
}

/// Records the executable or bundle launched for a profile inside `<profile_dir>/tmp/.launch-target.json`.
pub fn write_launch_target_hint(profile_dir: &Path, profile_id: &str, executable_or_bundle: &Path) {
    let hint_path = profile_dir.join("tmp").join(LAUNCH_TARGET_HINT_FILE);
    if fs::symlink_metadata(&hint_path).is_ok_and(|m| m.file_type().is_symlink()) {
        return;
    }
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

    let by_pid: HashMap<u32, &ProcessEntry> = processes.iter().map(|p| (p.pid, p)).collect();
    for proc_entry in &processes {
        if !uid_matches(proc_entry.uid, current_uid) {
            continue;
        }
        if !looks_like_official_zalo_bundle_process(proc_entry) {
            continue;
        }
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
    let (window_handle, window_title, window_state) = if running {
        let win = crate::window::inspect_profile_window(ops, &primary_zalo_data, &protected_pids);
        if let Some(w) = win {
            (
                Some(w.window_id),
                w.title,
                dim_core::PrimaryWindowState::Visible,
            )
        } else {
            (None, None, dim_core::PrimaryWindowState::HiddenInTray)
        }
    } else {
        (None, None, dim_core::PrimaryWindowState::None)
    };

    Ok(PrimaryInstanceStatus {
        zalo_data_dir: primary_zalo_data,
        lockfile_path: lock_path,
        running,
        process_count: protected_pids.len(),
        protected_pids,
        working_set_bytes,
        detected_zalo_user_id: detected_uid,
        window_handle,
        window_title,
        window_state,
        notes: vec![
            "Primary macOS ZaloData path and all unmanaged Zalo.app PIDs are strictly protected."
                .to_owned(),
        ],
    })
}

/// Resolves and verifies the live process tree owned by `profile_dir` (`profile_id`), enforcing
/// multi-factor fail-closed checks (`.zalo-profile-id` + `.runtime-launch.json` + `lstart` + argv lineage).
pub fn resolve_owned_profile_tree(
    ops: &dyn MacOsSystemOps,
    profile_id: &str,
    profile_dir: &Path,
    runtime_pid_hint: Option<u32>,
    primary_pids: &HashSet<u32>,
) -> Result<VerifiedProfileProcessTree, DimError> {
    let zalo_data = macos_zalo_data_dir(profile_dir);
    let singleton_lock = zalo_data.join("SingletonLock");
    let fixture_lock = zalo_data.join("lockfile");
    let runtime_pid_path = profile_dir.join(RUNTIME_PID_FILE);
    let runtime_rec_path = profile_dir.join(RUNTIME_LAUNCH_RECORD_FILE);

    let has_singleton = symlink_or_file_present(&singleton_lock);
    let has_fixture_lock = symlink_or_file_present(&fixture_lock);
    let has_runtime_pid = symlink_or_file_present(&runtime_pid_path);
    let has_runtime_rec = symlink_or_file_present(&runtime_rec_path);
    let has_socket_or_cookie = symlink_or_file_present(&zalo_data.join("SingletonSocket"))
        || symlink_or_file_present(&zalo_data.join("SingletonCookie"));

    let lockfile_path = if has_singleton || !has_fixture_lock {
        singleton_lock
    } else {
        fixture_lock
    };

    let mut processes = ops.list_processes()?;
    augment_with_live_fixture_helpers(ops, &zalo_data, &mut processes);

    let current_uid = ops.current_uid();
    let real_home = ops.user_home_dir();
    let launch_rec = read_runtime_launch_record(profile_dir);
    let target_hint = launch_rec
        .as_ref()
        .map(|r| r.executable_path.clone())
        .filter(|s| !s.is_empty())
        .or_else(|| read_launch_target_hint(profile_dir));

    let mut candidates: Vec<u32> = Vec::new();
    if let Some(lock_pid) = read_singleton_or_json_pid(&zalo_data) {
        candidates.push(lock_pid);
    }
    if let Some(rec) = &launch_rec {
        if !candidates.contains(&rec.pid) {
            candidates.push(rec.pid);
        }
    }
    if let Some(rt_pid) = runtime_pid_hint.or_else(|| read_runtime_pid(profile_dir)) {
        if !candidates.contains(&rt_pid) {
            candidates.push(rt_pid);
        }
    }

    let mut verified_main: Option<u32> = None;
    let mut verified_tree: Vec<u32> = Vec::new();

    for &cand_pid in &candidates {
        if verify_candidate_pid_ownership(
            cand_pid,
            profile_id,
            profile_dir,
            &zalo_data,
            real_home.as_deref(),
            current_uid,
            primary_pids,
            launch_rec.as_ref(),
            target_hint.as_deref(),
            &processes,
        ) {
            verified_main = Some(cand_pid);
            verified_tree = collect_profile_owned_pids(
                cand_pid,
                profile_dir,
                &zalo_data,
                current_uid,
                primary_pids,
                &processes,
            );
            break;
        }
    }

    if verified_main.is_none()
        && candidates.is_empty()
        && verify_profile_id_marker(&zalo_data, profile_id, true).is_ok()
    {
        if let Some(found_root) = find_profile_root_by_cmdline(
            profile_id,
            profile_dir,
            &zalo_data,
            current_uid,
            primary_pids,
            launch_rec.as_ref(),
            &processes,
        ) {
            verified_main = Some(found_root);
            verified_tree = collect_profile_owned_pids(
                found_root,
                profile_dir,
                &zalo_data,
                current_uid,
                primary_pids,
                &processes,
            );
        }
    }

    let working_set_bytes = sum_working_set_bytes(&verified_tree, &processes);
    let any_lock_artifact = has_singleton
        || has_fixture_lock
        || has_runtime_pid
        || has_runtime_rec
        || has_socket_or_cookie;
    let stale_lock_detected = verified_main.is_none() && any_lock_artifact;

    Ok(VerifiedProfileProcessTree {
        main_pid: verified_main,
        process_ids: verified_tree,
        working_set_bytes,
        stale_lock_detected,
        lockfile_path,
    })
}

/// Collects `root_pid`, its recursive `ppid` descendants, AND any orphan helpers (`ppid == 1`)
/// owned by `current_uid` whose command line explicitly binds `--user-data-dir=<zalo_data_dir>`.
#[must_use]
pub fn collect_profile_owned_pids(
    root_pid: u32,
    profile_dir: &Path,
    zalo_data_dir: &Path,
    current_uid: u32,
    primary_pids: &HashSet<u32>,
    processes: &[ProcessEntry],
) -> Vec<u32> {
    let mut tree = collect_descendant_pids(root_pid, processes);
    let tagged = find_explicit_profile_tagged_pids(
        profile_dir,
        zalo_data_dir,
        current_uid,
        primary_pids,
        processes,
    );
    for pid in tagged {
        if !tree.contains(&pid) {
            tree.push(pid);
        }
    }
    tree.retain(|p| !primary_pids.contains(p));
    if let Some(pos) = tree.iter().position(|&p| p == root_pid) {
        tree.remove(pos);
        tree.sort_unstable();
        let mut out = Vec::with_capacity(tree.len() + 1);
        out.push(root_pid);
        out.extend(tree);
        out
    } else {
        tree.sort_unstable();
        tree
    }
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

/// Enforces the strict 8-rule graceful shutdown contract (`P1-2`):
/// 1. Multi-evidence ownership verified fail-closed first; primary PIDs strictly protected.
/// 2. `.quit-signal` + `NSRunningApplication.terminate()` sent first.
/// 3. Dedicated graceful window (`2/3 * timeout_ms`) waited with ZERO `SIGTERM` signals.
/// 4. Tracks full child tree including `ppid == 1` orphan helpers.
/// 5. Escalates to `SIGTERM` only after graceful window expires AND `start_token`/ownership of each surviving PID matches.
/// 6. Never sends `SIGTERM` immediately after `terminate()`.
/// 7. Never signals primary Zalo or out-of-scope processes.
/// 8. Verifies ALL owned processes have actually exited before clearing `.runtime-pid`/`.runtime-launch.json`;
///    returns `Err(DimError::ProcessError)` if any owned PID hangs past `timeout_ms` (`DEFECT-06`).
#[allow(clippy::too_many_arguments)]
pub fn stop_isolated_profile_tree(
    ops: &dyn MacOsSystemOps,
    profile_id: &str,
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

    // Rule 1: Re-verify live multi-evidence process ownership right before signaling.
    let live_tree = resolve_owned_profile_tree(
        ops,
        profile_id,
        profile_dir,
        read_runtime_pid(profile_dir),
        &protected_set,
    )?;
    let Some(main_pid) = live_tree.main_pid else {
        // Stale or foreign/recycled PID: never signal!
        return Ok(0);
    };
    if protected_set.contains(&main_pid) {
        return Err(DimError::ProtectedPrimaryProcess(format!(
            "refused to stop protected macOS primary PID {main_pid}"
        )));
    }

    let mut snapshot_procs = ops.list_processes()?;
    augment_with_live_fixture_helpers(ops, zalo_data_dir, &mut snapshot_procs);
    let mut owned_tokens: HashMap<u32, Option<String>> = HashMap::new();
    for &pid in &live_tree.process_ids {
        let tok = snapshot_procs
            .iter()
            .find(|p| p.pid == pid)
            .and_then(|p| p.start_token.clone());
        owned_tokens.insert(pid, tok);
    }
    let initial_count = u32::try_from(live_tree.process_ids.len()).unwrap_or(1);
    let launch_rec = read_runtime_launch_record(profile_dir);
    let is_headless_node_fixture = launch_rec.as_ref().is_some_and(|r| {
        let lower = r.executable_path.to_ascii_lowercase();
        lower.ends_with(".mjs") || lower.ends_with(".js")
    });
    let is_official_zalo = launch_rec.as_ref().is_some_and(|r| {
        let lower = r.executable_path.to_ascii_lowercase();
        lower.ends_with(".app") || lower.contains(".app/contents/macos/")
    });

    // Rule 2: Cooperative `.quit-signal` + Cocoa `NSRunningApplication.terminate()` (for non-headless apps).
    if fs::symlink_metadata(zalo_data_dir).is_ok_and(|m| m.is_dir()) {
        let _ = write_signal_json_atomic(
            &zalo_data_dir.join(".quit-signal"),
            &serde_json::json!({
                "requested_at": now_rfc3339_utc(),
                "target_pid": main_pid,
            }),
        );
    }
    if !is_headless_node_fixture {
        let _ = request_graceful_app_terminate(ops, main_pid);
    }

    // Rule 3 & 6: Dedicated graceful window BEFORE any SIGTERM is ever sent.
    let effective_timeout_ms = timeout_ms.max(150);
    let graceful_budget_ms = (effective_timeout_ms.saturating_mul(2) / 3).clamp(100, 2_200);
    let overall_deadline = Instant::now() + Duration::from_millis(effective_timeout_ms);
    let graceful_deadline =
        (Instant::now() + Duration::from_millis(graceful_budget_ms)).min(overall_deadline);

    while Instant::now() < graceful_deadline {
        let remaining = collect_all_surviving_owned_pids(
            ops,
            profile_id,
            profile_dir,
            zalo_data_dir,
            &owned_tokens,
            &protected_set,
        )?;
        if remaining.is_empty() {
            finalize_clean_stop(ops, profile_id, profile_dir, zalo_data_dir);
            return Ok(initial_count);
        }
        std::thread::sleep(Duration::from_millis(30));
    }

    // Rule 4, 5 & 7: Escalate to isolated `SIGTERM` ONLY on verified surviving owned PIDs (including `ppid == 1` orphans).
    let surviving_for_sigterm = collect_all_surviving_owned_pids(
        ops,
        profile_id,
        profile_dir,
        zalo_data_dir,
        &owned_tokens,
        &protected_set,
    )?;
    if !surviving_for_sigterm.is_empty() {
        // Signal helper/orphan children first, then `main_pid`.
        for &pid in surviving_for_sigterm
            .iter()
            .filter(|&&p| p != main_pid)
            .rev()
        {
            if !protected_set.contains(&pid) {
                let _ = ops.send_sigterm(pid);
            }
        }
        if surviving_for_sigterm.contains(&main_pid) && !protected_set.contains(&main_pid) {
            let _ = ops.send_sigterm(main_pid);
        }
    }

    while Instant::now() < overall_deadline {
        let remaining = collect_all_surviving_owned_pids(
            ops,
            profile_id,
            profile_dir,
            zalo_data_dir,
            &owned_tokens,
            &protected_set,
        )?;
        if remaining.is_empty() {
            finalize_clean_stop(ops, profile_id, profile_dir, zalo_data_dir);
            return Ok(initial_count);
        }
        std::thread::sleep(Duration::from_millis(30));
    }

    // Rule 8 (`IR-MAC-02` + `LC-09` / `REL-07`):
    // - Never send `SIGKILL` to `OfficialZalo` or any profile with active SQLite `-wal` files in
    //   `Database/_production/`; fail closed with `DimError::ProcessError` and keep `.runtime-launch.json`.
    // - For WAL-free non-Zalo fixture/helper trees ignoring `SIGTERM`, re-verify ownership + `start_token`
    //   and escalate to isolated `SIGKILL`, reporting `Ok` ONLY once 0 owned PIDs survive.
    let surviving_after_sigterm = collect_all_surviving_owned_pids(
        ops,
        profile_id,
        profile_dir,
        zalo_data_dir,
        &owned_tokens,
        &protected_set,
    )?;
    if surviving_after_sigterm.is_empty() {
        finalize_clean_stop(ops, profile_id, profile_dir, zalo_data_dir);
        return Ok(initial_count);
    }

    if is_official_zalo || has_active_sqlite_wal(zalo_data_dir) {
        return Err(DimError::ProcessError(format!(
            "profile '{profile_id}' failed to exit within {effective_timeout_ms}ms while SQLite WAL or OfficialZalo session is active; withholding SIGKILL to prevent database corruption (surviving owned PIDs: {surviving_after_sigterm:?})"
        )));
    }

    for &pid in surviving_after_sigterm
        .iter()
        .filter(|&&p| p != main_pid)
        .rev()
    {
        if !protected_set.contains(&pid) {
            let _ = ops.send_sigkill(pid);
        }
    }
    if surviving_after_sigterm.contains(&main_pid) && !protected_set.contains(&main_pid) {
        let _ = ops.send_sigkill(main_pid);
    }

    let sigkill_deadline = Instant::now() + Duration::from_millis(500);
    while Instant::now() < sigkill_deadline {
        let rem = collect_all_surviving_owned_pids(
            ops,
            profile_id,
            profile_dir,
            zalo_data_dir,
            &owned_tokens,
            &protected_set,
        )?;
        if rem.is_empty() {
            finalize_clean_stop_after_forced_kill(ops, profile_id, profile_dir, zalo_data_dir);
            return Ok(initial_count);
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    let final_surviving = collect_all_surviving_owned_pids(
        ops,
        profile_id,
        profile_dir,
        zalo_data_dir,
        &owned_tokens,
        &protected_set,
    )?;
    if final_surviving.is_empty() {
        finalize_clean_stop_after_forced_kill(ops, profile_id, profile_dir, zalo_data_dir);
        Ok(initial_count)
    } else {
        Err(DimError::ProcessError(format!(
            "profile '{profile_id}' failed to exit within {effective_timeout_ms}ms (surviving owned PIDs: {final_surviving:?})"
        )))
    }
}

/// Safely removes stale `SingletonLock`, `SingletonSocket`, `SingletonCookie`, `lockfile`,
/// `.runtime-pid`, `.runtime-launch.json`, `.quit-signal`, `.focus-signal`, `.window-state.json`,
/// and isolated short `/tmp/dz-<uid>/<id>` artifacts without following symlinks (`P0-4` / `P1-1`).
///
/// Never touches global `ZaloCall` sockets (`/tmp/socketzalorecv2021`, `/tmp/socketzalosend2021`),
/// and refuses cleanup with `DimError::ProfileRunning` if `is_running` is `true`.
pub fn recover_macos_stale_artifacts(
    profile: &ProfileRecord,
    profile_dir: &Path,
    is_running: bool,
) -> Result<RecoveryReport, DimError> {
    recover_macos_stale_artifacts_with_ops(
        &crate::ops::NativeMacOsSystemOps,
        profile,
        profile_dir,
        is_running,
    )
}

/// [`MacOsSystemOps`]-parameterized variant of [`recover_macos_stale_artifacts`] that also cleans
/// the profile's isolated short temp slot `<short_tmp_root>/dz-<uid>/<id>`.
pub fn recover_macos_stale_artifacts_with_ops(
    ops: &dyn MacOsSystemOps,
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

    // Extra safety: never touch global ZaloCall sockets even if someone passed a strange path.
    debug_assert!(!MACOS_CALL_SOCKETS
        .iter()
        .any(|s| zalo_data.to_string_lossy().contains(s)));

    for artifact in MACOS_STALE_ZALODATA_ARTIFACTS {
        let candidate = zalo_data.join(artifact);
        if remove_file_no_follow(&candidate)? {
            removed.push(candidate.display().to_string());
        }
    }

    for rt_name in [RUNTIME_PID_FILE, RUNTIME_LAUNCH_RECORD_FILE] {
        let rt_path = profile_dir.join(rt_name);
        if remove_file_no_follow(&rt_path)? {
            removed.push(rt_path.display().to_string());
        }
    }

    let _ = cleanup_profile_short_tmpdir(ops, &profile.id);

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
    profile_id: &str,
    profile_dir: &Path,
    zalo_data_dir: &Path,
    real_home: Option<&Path>,
    current_uid: u32,
    primary_pids: &HashSet<u32>,
    launch_rec: Option<&RuntimeLaunchRecord>,
    target_hint: Option<&str>,
    processes: &[ProcessEntry],
) -> bool {
    if candidate_pid <= 1 || primary_pids.contains(&candidate_pid) {
        return false;
    }

    // Require intact `.zalo-profile-id` matching `profile_id` (fail closed if missing or tampered).
    if verify_profile_id_marker(zalo_data_dir, profile_id, true).is_err() {
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

    // Must look like an expected Zalo / CustomExecutable / NodeFixture target.
    if !looks_like_zalo_or_fixture_entry(root_entry, target_hint) {
        return false;
    }

    // Check `.runtime-launch.json` consistency & `process_start_token` (`lstart`) anti-PID-reuse guard!
    let mut verified_by_launch_record = false;
    if let Some(rec) = launch_rec {
        if rec.pid == candidate_pid {
            if !rec.profile_id.eq_ignore_ascii_case(profile_id) {
                return false;
            }
            match (&rec.process_start_token, &root_entry.start_token) {
                (Some(expected_tok), Some(live_tok)) => {
                    if expected_tok != live_tok {
                        // Deterministic PID-reuse rejection (even if reused by another Zalo/Electron binary!).
                        return false;
                    }
                    verified_by_launch_record = true;
                }
                (None, None) => {
                    verified_by_launch_record = true;
                }
                // If launch recorded a token and live lacks one (or vice versa), do not grant Proof B from record alone.
                _ => {}
            }
        }
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

        if let Some(ref pkey) = primary_key {
            if !pkey.is_empty() && args_key.contains(pkey) && !args_key.contains(&profile_key) {
                return false;
            }
        }
        if references_different_managed_profile(&args_key, &profile_key) {
            return false;
        }
        if (!profile_key.is_empty() && args_key.contains(&profile_key))
            || (!zalo_key.is_empty() && args_key.contains(&zalo_key))
        {
            explicit_path_match = true;
        }
    }

    // Fail closed unless proven by EITHER explicit profile path lineage OR verified `.runtime-launch.json` + start_token!
    explicit_path_match || verified_by_launch_record
}

fn find_profile_root_by_cmdline(
    profile_id: &str,
    profile_dir: &Path,
    zalo_data_dir: &Path,
    current_uid: u32,
    primary_pids: &HashSet<u32>,
    launch_rec: Option<&RuntimeLaunchRecord>,
    processes: &[ProcessEntry],
) -> Option<u32> {
    let profile_key = normalize_macos_path_key(profile_dir);
    let zalo_key = normalize_macos_path_key(zalo_data_dir);
    let by_pid: HashMap<u32, &ProcessEntry> = processes.iter().map(|p| (p.pid, p)).collect();

    for entry in processes {
        if !uid_matches(entry.uid, current_uid) || primary_pids.contains(&entry.pid) {
            continue;
        }
        // Reject if `.runtime-launch.json` proves `entry.pid` recycled an old PID with a different start token.
        if let Some(rec) = launch_rec {
            if rec.pid == entry.pid {
                if !rec.profile_id.eq_ignore_ascii_case(profile_id) {
                    continue;
                }
                if let (Some(exp), Some(act)) = (&rec.process_start_token, &entry.start_token) {
                    if exp != act {
                        continue;
                    }
                }
            }
        }
        let args_key = normalize_cmd_for_match(&entry.args);
        if references_different_managed_profile(&args_key, &profile_key) {
            continue;
        }
        if (!zalo_key.is_empty() && args_key.contains(&zalo_key))
            || (!profile_key.is_empty() && args_key.contains(&profile_key))
        {
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

fn find_explicit_profile_tagged_pids(
    profile_dir: &Path,
    zalo_data_dir: &Path,
    current_uid: u32,
    primary_pids: &HashSet<u32>,
    processes: &[ProcessEntry],
) -> Vec<u32> {
    let profile_key = normalize_macos_path_key(profile_dir);
    let zalo_key = normalize_macos_path_key(zalo_data_dir);
    let mut out = Vec::new();

    for entry in processes {
        if entry.pid <= 1
            || !uid_matches(entry.uid, current_uid)
            || primary_pids.contains(&entry.pid)
        {
            continue;
        }
        let args_key = normalize_cmd_for_match(&entry.args);
        if references_different_managed_profile(&args_key, &profile_key) {
            continue;
        }
        if (!zalo_key.is_empty() && args_key.contains(&zalo_key))
            || (!profile_key.is_empty() && args_key.contains(&profile_key))
        {
            out.push(entry.pid);
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

fn collect_all_surviving_owned_pids(
    ops: &dyn MacOsSystemOps,
    profile_id: &str,
    profile_dir: &Path,
    zalo_data_dir: &Path,
    owned_tokens: &HashMap<u32, Option<String>>,
    protected_set: &HashSet<u32>,
) -> Result<Vec<u32>, DimError> {
    let mut procs = ops.list_processes()?;
    augment_with_live_fixture_helpers(ops, zalo_data_dir, &mut procs);
    let current_uid = ops.current_uid();

    let mut surviving = HashSet::new();

    // 1. Check any initially-owned PID whose `start_token` still matches (hasn't exited or been recycled).
    for (&pid, expected_tok) in owned_tokens {
        if protected_set.contains(&pid) {
            continue;
        }
        if let Some(live) = procs.iter().find(|p| p.pid == pid) {
            if !uid_matches(live.uid, current_uid) {
                continue;
            }
            let same_instance = match (expected_tok, &live.start_token) {
                (Some(exp), Some(act)) => exp == act,
                (None, _) | (_, None) => looks_like_zalo_or_fixture_entry(live, None),
            };
            if same_instance {
                for child_pid in collect_descendant_pids(pid, &procs) {
                    if !protected_set.contains(&child_pid) {
                        surviving.insert(child_pid);
                    }
                }
            }
        }
    }

    // 2. Also include any orphan helper (`ppid == 1`) still carrying `--user-data-dir=<zalo_data_dir>`.
    for orphan_pid in find_explicit_profile_tagged_pids(
        profile_dir,
        zalo_data_dir,
        current_uid,
        protected_set,
        &procs,
    ) {
        if let Some(Some(exp_tok)) = owned_tokens.get(&orphan_pid) {
            if let Some(live) = procs.iter().find(|p| p.pid == orphan_pid) {
                if live.start_token.as_ref() != Some(exp_tok) {
                    continue;
                }
            }
        }
        surviving.insert(orphan_pid);
    }

    // 3. Cross-check standard tree resolution in case a new helper appeared during shutdown.
    if let Ok(tree) = resolve_owned_profile_tree(ops, profile_id, profile_dir, None, protected_set)
    {
        for pid in tree.process_ids {
            if !protected_set.contains(&pid) {
                surviving.insert(pid);
            }
        }
    }

    let mut ordered: Vec<u32> = surviving.into_iter().collect();
    ordered.sort_unstable();
    Ok(ordered)
}

fn finalize_clean_stop(
    ops: &dyn MacOsSystemOps,
    profile_id: &str,
    profile_dir: &Path,
    zalo_data_dir: &Path,
) {
    let _ = clear_runtime_launch_files(profile_dir);
    let _ = remove_file_no_follow(&zalo_data_dir.join(".quit-signal"));
    let _ = remove_file_no_follow(&zalo_data_dir.join(".helper-pids.json"));
    let _ = remove_file_no_follow(&zalo_data_dir.join("children.json"));
    let _ = cleanup_profile_short_tmpdir(ops, profile_id);
}

fn finalize_clean_stop_after_forced_kill(
    ops: &dyn MacOsSystemOps,
    profile_id: &str,
    profile_dir: &Path,
    zalo_data_dir: &Path,
) {
    finalize_clean_stop(ops, profile_id, profile_dir, zalo_data_dir);
    for artifact in MACOS_STALE_ZALODATA_ARTIFACTS {
        let _ = remove_file_no_follow(&zalo_data_dir.join(artifact));
    }
    if let Ok(entries) = fs::read_dir(zalo_data_dir) {
        for entry in entries.flatten() {
            let fname = entry.file_name();
            let s = fname.to_string_lossy();
            if s.starts_with("worker-") && s.ends_with(".pid") {
                let _ = remove_file_no_follow(&entry.path());
            }
        }
    }
}

/// Checks whether `<zalo_data_dir>/Database/_production` contains any active SQLite `-wal` file (`IR-MAC-02`).
#[must_use]
pub fn has_active_sqlite_wal(zalo_data_dir: &Path) -> bool {
    let prod_root = zalo_data_dir.join("Database").join("_production");
    let Ok(meta) = fs::symlink_metadata(&prod_root) else {
        return false;
    };
    if is_symlink_or_reparse(&meta) || !meta.is_dir() {
        return false;
    }
    has_wal_recursive(&prod_root, 2)
}

fn has_wal_recursive(dir: &Path, depth: usize) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if is_symlink_or_reparse(&meta) {
            continue;
        }
        if meta.is_file() {
            let lower = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if lower.ends_with("-wal") {
                return true;
            }
        } else if meta.is_dir() && depth > 0 && has_wal_recursive(&path, depth - 1) {
            return true;
        }
    }
    false
}

/// Discovers live child/orphan helper PIDs recorded by `fixture-macos-app.mjs` or `xf-fixture.mjs` on hosts where `/bin/ps` is absent.
fn augment_with_live_fixture_helpers(
    ops: &dyn MacOsSystemOps,
    zalo_data_dir: &Path,
    processes: &mut Vec<ProcessEntry>,
) {
    let uid = ops.current_uid();
    let mut candidate_helpers: Vec<(u32, u32)> = Vec::new();

    let win_file = zalo_data_dir.join(MACOS_WINDOW_STATE_FILE);
    if fs::symlink_metadata(&win_file).is_ok_and(|m| m.is_file()) {
        if let Ok(raw) = fs::read_to_string(&win_file) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
                let owner = val
                    .get("owner_pid")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|n| u32::try_from(n).ok())
                    .unwrap_or(1);
                if let Some(hpid) = val
                    .get("helper_pid")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|n| u32::try_from(n).ok())
                    .filter(|&p| p > 0)
                {
                    candidate_helpers.push((hpid, owner));
                }
            }
        }
    }

    let helper_file = zalo_data_dir.join(".helper-pids.json");
    if fs::symlink_metadata(&helper_file).is_ok_and(|m| m.is_file()) {
        if let Ok(raw) = fs::read_to_string(&helper_file) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(arr) = val.as_array() {
                    for item in arr {
                        if let Some(hpid) = item
                            .get("pid")
                            .and_then(serde_json::Value::as_u64)
                            .and_then(|n| u32::try_from(n).ok())
                            .filter(|&p| p > 0)
                        {
                            let ppid = item
                                .get("ppid")
                                .and_then(serde_json::Value::as_u64)
                                .and_then(|n| u32::try_from(n).ok())
                                .unwrap_or(1);
                            candidate_helpers.push((hpid, ppid));
                        }
                    }
                }
            }
        }
    }

    let children_file = zalo_data_dir.join("children.json");
    if fs::symlink_metadata(&children_file).is_ok_and(|m| m.is_file()) {
        if let Ok(raw) = fs::read_to_string(&children_file) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
                let parent_pid = val
                    .get("parent_pid")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|n| u32::try_from(n).ok())
                    .unwrap_or(1);
                if let Some(arr) = val.get("child_pids").and_then(serde_json::Value::as_array) {
                    for item in arr {
                        if let Some(cpid) = item
                            .as_u64()
                            .and_then(|n| u32::try_from(n).ok())
                            .filter(|&p| p > 0)
                        {
                            candidate_helpers.push((cpid, parent_pid));
                        }
                    }
                }
            }
        }
    }

    for (hpid, recorded_ppid) in candidate_helpers {
        if processes.iter().any(|p| p.pid == hpid) || !ops.is_pid_alive(hpid) {
            continue;
        }
        let parent_alive = processes.iter().any(|p| p.pid == recorded_ppid);
        let effective_ppid = if parent_alive { recorded_ppid } else { 1 };
        processes.push(ProcessEntry {
            pid: hpid,
            ppid: effective_ppid,
            uid,
            rss_kib: 24_576,
            start_token: ops.process_start_token(hpid),
            executable: "node".to_owned(),
            args: format!(
                "node fixture-macos-app.mjs --type=renderer --user-data-dir={}",
                zalo_data_dir.display()
            ),
        });
    }
}

fn try_take_lstart(input: &str) -> Option<(String, &str)> {
    // `lstart` has 5 whitespace-separated tokens: `<Day> <Mon> <DD> <HH:MM:SS> <YYYY>`
    // e.g. `Mon Nov 17 23:15:02 2026`.
    let (t1, r1) = take_token(input)?;
    let (t2, r2) = take_token(r1)?;
    let (t3, r3) = take_token(r2)?;
    let (t4, r4) = take_token(r3)?;
    let (t5, r5) = take_token(r4)?;

    let is_weekday = t1.len() == 3 && t1.chars().all(|c| c.is_ascii_alphabetic());
    let is_month = t2.len() == 3 && t2.chars().all(|c| c.is_ascii_alphabetic());
    let is_day = t3.parse::<u8>().is_ok_and(|d| (1..=31).contains(&d));
    let is_hms =
        t4.len() == 8 && t4.as_bytes().get(2) == Some(&b':') && t4.as_bytes().get(5) == Some(&b':');
    let is_year = t5.len() == 4 && t5.parse::<u16>().is_ok_and(|y| (1970..=2199).contains(&y));

    if is_weekday && is_month && is_day && is_hms && is_year {
        Some((format!("{t1} {t2} {t3} {t4} {t5}"), r5))
    } else {
        None
    }
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
        "xf-fixture.mjs",
        "zalofixture",
        "dim-test-fixture",
        "xf-fixture",
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

fn read_launch_target_hint(profile_dir: &Path) -> Option<String> {
    let path = profile_dir.join("tmp").join(LAUNCH_TARGET_HINT_FILE);
    if !fs::symlink_metadata(&path).is_ok_and(|m| m.is_file()) {
        return None;
    }
    let raw = fs::read_to_string(path).ok()?;
    let val = serde_json::from_str::<serde_json::Value>(&raw).ok()?;
    val.get("target")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
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

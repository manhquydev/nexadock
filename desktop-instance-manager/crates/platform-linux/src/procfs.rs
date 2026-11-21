//! Stateless Linux `/proc` process-tree ownership, `SingletonLock` resolution,
//! `(pid, starttime_ticks)` + `DIM_INSTANCE_NONCE` recycled-PID spoofing defense,
//! `VmRSS` memory measurement, and PID-scoped X11 window lookup.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::Path;

use dim_profile_store::is_symlink_or_reparse;

use crate::layout::{DIM_INSTANCE_NONCE_ENV, DIM_PROFILE_ID_ENV};

/// Parsed kernel fields from `/proc/<pid>/stat` and `/proc/<pid>/status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcEntry {
    /// Process ID.
    pub pid: u32,
    /// Executable command short name (`comm` inside `(...)`).
    pub comm: String,
    /// Single-character Linux task state (`R`, `S`, `D`, `Z`, `T`, etc.).
    pub state: char,
    /// Parent process ID (`ppid`).
    pub ppid: u32,
    /// Process group ID (`pgrp`).
    pub pgrp: u32,
    /// Kernel task start time in clock ticks since system boot (`/proc/<pid>/stat` field 22).
    pub starttime_ticks: u64,
    /// Resident set size (`VmRSS`) in bytes.
    pub rss_bytes: u64,
    /// Effective/real owner UID from `/proc/<pid>/status` `Uid:` line, if readable.
    pub uid: Option<u32>,
    /// Parsed `KEY=VALUE` environment map from `/proc/<pid>/environ`, if readable.
    pub environ: BTreeMap<String, String>,
    /// Parsed NUL-delimited argv from `/proc/<pid>/cmdline`, if readable.
    pub cmdline: Vec<String>,
}

impl ProcEntry {
    /// Returns `true` when the process is alive and NOT a dead zombie (`state != 'Z'` and `state != 'X'`).
    #[must_use]
    pub const fn is_active(&self) -> bool {
        !matches!(self.state, 'Z' | 'X' | 'x')
    }
}

/// Structured launch ownership metadata persisted in `<profile_dir>/.runtime-pid`
/// to prevent recycled-PID misidentification after crashes or OS reboots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePidHint {
    /// Spawned root process ID (either outer `bwrap` wrapper or direct app main process).
    pub pid: u32,
    /// Expected owning `DIM_PROFILE_ID` recorded at launch, if structured JSON.
    pub profile_id: Option<String>,
    /// Unique per-launch `DIM_INSTANCE_NONCE` recorded at launch, if present.
    pub instance_nonce: Option<String>,
    /// Kernel `/proc/<pid>/stat` field 22 (`starttime` in clock ticks since boot), if recorded.
    pub starttime_ticks: Option<u64>,
}

/// Observed process tree for an isolated Linux profile or primary instance.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileProcessTree {
    /// Verified application main/root PID if running.
    pub main_pid: Option<u32>,
    /// Topologically ordered active PIDs belonging to this profile (outer wrapper/root first,
    /// descendants last so reverse iteration terminates leaves before parents/wrappers).
    pub process_ids: Vec<u32>,
    /// Verified `/proc/<pid>/stat` `starttime_ticks` snapshot per PID in `process_ids`
    /// for TOCTOU-safe signal delivery.
    pub process_starttimes: BTreeMap<u32, u64>,
    /// Sum of `VmRSS` in bytes across all processes in `process_ids`.
    pub working_set_bytes: u64,
}

/// Matched X11 window owned by one of a profile's verified PIDs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedX11Window {
    /// X11 window ID (`XID`) parsed from `wmctrl -lp` (`0x...`).
    pub window_id: u64,
    /// Raw hex token (e.g., `"0x03a00007"`) suitable for `wmctrl -i -a`.
    pub window_id_hex: String,
    /// Owning PID verified against the profile's process tree.
    pub pid: u32,
    /// Window title reported by the window manager.
    pub title: String,
}

/// Parses `/proc/<pid>/stat` safely even when `comm` contains spaces or nested parentheses,
/// returning `(pid, comm, state, ppid, pgrp, starttime_ticks)` where `starttime_ticks` is
/// field 22 (index 19 after `) `).
#[must_use]
pub fn parse_proc_stat(stat_line: &str) -> Option<(u32, String, char, u32, u32, u64)> {
    let open = stat_line.find('(')?;
    let close = stat_line.rfind(") ")?;
    if close <= open {
        return None;
    }
    let pid = stat_line[..open].trim().parse::<u32>().ok()?;
    if pid == 0 {
        return None;
    }
    let comm = stat_line[open + 1..close].to_owned();
    let fields: Vec<&str> = stat_line[close + 2..].split_whitespace().collect();
    if fields.len() <= 19 {
        return None;
    }
    let state = fields[0].chars().next()?;
    let ppid = fields[1].parse::<u32>().ok()?;
    let pgrp = fields[2].parse::<u32>().ok()?;
    // Field 22 in `proc_pid_stat(5)` is `starttime` (%llu); since `fields[0]` is field 3 (`state`),
    // field 22 sits at index `22 - 3 = 19`.
    let starttime_ticks = fields[19].parse::<u64>().ok()?;
    Some((pid, comm, state, ppid, pgrp, starttime_ticks))
}

/// Reads only `/proc/<pid>/stat` field 22 (`starttime_ticks`) for fast post-spawn recording
/// and pre-signal TOCTOU verification.
#[must_use]
pub fn read_proc_stat_starttime(proc_root: &Path, pid: u32) -> Option<u64> {
    if pid == 0 {
        return None;
    }
    let stat_raw = fs::read_to_string(proc_root.join(pid.to_string()).join("stat")).ok()?;
    let (parsed_pid, _, _, _, _, starttime_ticks) = parse_proc_stat(&stat_raw)?;
    (parsed_pid == pid).then_some(starttime_ticks)
}

/// Extracts `VmRSS` in bytes and real `Uid` from `/proc/<pid>/status`.
#[must_use]
pub fn parse_proc_status(status_text: &str) -> (u64, Option<u32>) {
    let mut rss_bytes = 0u64;
    let mut uid = None;
    for line in status_text.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            if let Some(kib) = rest
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u64>().ok())
            {
                rss_bytes = kib.saturating_mul(1024);
            }
        } else if let Some(rest) = line.strip_prefix("Uid:") {
            uid = rest
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u32>().ok());
        }
    }
    (rss_bytes, uid)
}

/// Returns `true` when `/proc/<pid>/environ` bytes were readable but consist entirely of NUL
/// filler. Under `kernel.yama.ptrace_scope=1` a non-ancestor reader receives zero-filled bytes
/// instead of an error, which would otherwise be indistinguishable from "no environment set".
/// Such reads must be treated as *unknown* so callers fall through to `cmdline` matching.
#[must_use]
pub fn environ_bytes_zeroed(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.iter().all(|&b| b == 0)
}

/// Parses a NUL-separated `/proc/<pid>/environ` byte buffer into a key-value map.
/// Zero-filled (yama-masked) buffers are treated as unknown and yield an empty map so
/// `cmdline`-based verification applies; see [`environ_bytes_zeroed`].
#[must_use]
pub fn parse_proc_environ(bytes: &[u8]) -> BTreeMap<String, String> {
    if environ_bytes_zeroed(bytes) {
        return BTreeMap::new();
    }
    let mut map = BTreeMap::new();
    for raw_entry in bytes.split(|&b| b == 0) {
        if raw_entry.is_empty() {
            continue;
        }
        let text = String::from_utf8_lossy(raw_entry);
        if let Some((k, v)) = text.split_once('=') {
            if !k.is_empty() {
                map.insert(k.to_owned(), v.to_owned());
            }
        }
    }
    map
}

/// Parses a NUL-separated `/proc/<pid>/cmdline` byte buffer into argument strings.
#[must_use]
pub fn parse_proc_cmdline(bytes: &[u8]) -> Vec<String> {
    let mut out: Vec<String> = bytes
        .split(|&b| b == 0)
        .filter(|seg| !seg.is_empty())
        .map(|seg| String::from_utf8_lossy(seg).into_owned())
        .collect();
    while out.last().is_some_and(String::is_empty) {
        let _ = out.pop();
    }
    out
}

/// Reads a single process snapshot from `<proc_root>/<pid>`.
#[must_use]
pub fn read_proc_entry(proc_root: &Path, pid: u32) -> Option<ProcEntry> {
    let pid_dir = proc_root.join(pid.to_string());
    let stat_raw = fs::read_to_string(pid_dir.join("stat")).ok()?;
    let (parsed_pid, comm, state, ppid, pgrp, starttime_ticks) = parse_proc_stat(&stat_raw)?;
    let (rss_bytes, uid) = fs::read_to_string(pid_dir.join("status"))
        .ok()
        .map(|s| parse_proc_status(&s))
        .unwrap_or((0, None));
    let environ = fs::read(pid_dir.join("environ"))
        .ok()
        .map(|b| parse_proc_environ(&b))
        .unwrap_or_default();
    let cmdline = fs::read(pid_dir.join("cmdline"))
        .ok()
        .map(|b| parse_proc_cmdline(&b))
        .unwrap_or_default();

    Some(ProcEntry {
        pid: parsed_pid,
        comm,
        state,
        ppid,
        pgrp,
        starttime_ticks,
        rss_bytes,
        uid,
        environ,
        cmdline,
    })
}

/// Enumerates all readable numeric PIDs in `proc_root`.
#[must_use]
pub fn scan_proc_table(proc_root: &Path) -> BTreeMap<u32, ProcEntry> {
    let mut table = BTreeMap::new();
    let Ok(entries) = fs::read_dir(proc_root) else {
        return table;
    };
    for entry in entries.flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        if let Some(proc_entry) = read_proc_entry(proc_root, pid) {
            table.insert(pid, proc_entry);
        }
    }
    table
}

/// Parses a Chromium `SingletonLock` symlink target (`<hostname>-<pid>`, including hyphenated hostnames).
#[must_use]
pub fn parse_singleton_lock_target(raw_target: &str) -> Option<u32> {
    let trimmed = raw_target.trim();
    let (host, pid_str) = trimmed.rsplit_once('-')?;
    if host.is_empty() {
        return None;
    }
    let pid = pid_str.parse::<u32>().ok()?;
    (pid > 0).then_some(pid)
}

/// Reads candidate PID from `SingletonLock` symlink (`<host>-<pid>`) or fixture `lockfile` JSON.
#[must_use]
pub fn read_lock_pid(zalo_data_dir: &Path) -> Option<u32> {
    #[cfg(unix)]
    if let Ok(target) = fs::read_link(zalo_data_dir.join("SingletonLock")) {
        if let Some(pid) = parse_singleton_lock_target(&target.to_string_lossy()) {
            return Some(pid);
        }
    }
    #[cfg(not(unix))]
    if let Ok(raw_link) = fs::read_to_string(zalo_data_dir.join("SingletonLock")) {
        if let Some(pid) = parse_singleton_lock_target(&raw_link) {
            return Some(pid);
        }
    }

    if let Ok(raw) = fs::read_to_string(zalo_data_dir.join("lockfile")) {
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

/// Reads `.runtime-pid` inside `profile_dir` (supports both structured JSON with
/// `starttime_ticks`/`instance_nonce` and legacy plain integer text, rejecting symlinks).
#[must_use]
pub fn read_runtime_pid_hint(profile_dir: &Path) -> Option<RuntimePidHint> {
    let hint_path = profile_dir.join(".runtime-pid");
    let meta = fs::symlink_metadata(&hint_path).ok()?;
    if is_symlink_or_reparse(&meta) || !meta.is_file() {
        return None;
    }
    let raw = fs::read_to_string(&hint_path).ok()?;
    let trimmed = raw.trim();
    if let Ok(pid) = trimmed.parse::<u32>() {
        return (pid > 0).then_some(RuntimePidHint {
            pid,
            profile_id: None,
            instance_nonce: None,
            starttime_ticks: None,
        });
    }
    let val = serde_json::from_str::<serde_json::Value>(trimmed).ok()?;
    let pid = val
        .get("pid")
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .filter(|&p| p > 0)?;
    let profile_id = val
        .get("profile_id")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let instance_nonce = val
        .get("instance_nonce")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let starttime_ticks = val
        .get("starttime_ticks")
        .and_then(serde_json::Value::as_u64);

    Some(RuntimePidHint {
        pid,
        profile_id,
        instance_nonce,
        starttime_ticks,
    })
}

/// Verifies that `entry` genuinely belongs to `profile_id` / `profile_dir` and has NOT been
/// recycled into an unrelated OS process or another profile.
#[must_use]
pub fn entry_matches_profile(
    entry: &ProcEntry,
    profile_id: &str,
    profile_dir: &Path,
    expected_uid: Option<u32>,
) -> bool {
    if !entry.is_active() {
        return false;
    }
    if let (Some(exp_uid), Some(actual_uid)) = (expected_uid, entry.uid) {
        if exp_uid != actual_uid {
            return false;
        }
    }

    // Check authoritative `DIM_PROFILE_ID` in environ, or in `bwrap --setenv DIM_PROFILE_ID <id>`
    // when `/proc/<pid>/environ` is unreadable (`dumpable=0` on `bwrap`).
    if let Some(env_pid_id) = entry_profile_id(entry) {
        return env_pid_id.eq_ignore_ascii_case(profile_id);
    }

    let profile_str = profile_dir.to_string_lossy();
    let expected_xdg = profile_dir.join(".config").to_string_lossy().into_owned();

    if !entry.environ.is_empty() {
        if let Some(xdg) = entry.environ.get("XDG_CONFIG_HOME") {
            return xdg == &expected_xdg;
        }
        if let Some(home) = entry.environ.get("HOME") {
            return home == profile_str.as_ref();
        }
        return false;
    }

    // If `/proc/<pid>/environ` was empty/unreadable (e.g. `bwrap` after capability drop),
    // inspect `cmdline` for `--bind <profile_dir> <profile_dir>` or profile path arguments.
    entry
        .cmdline
        .iter()
        .any(|arg| arg.contains(profile_str.as_ref()))
}

/// Validates a persisted [`RuntimePidHint`] against a live `/proc/<pid>` [`ProcEntry`],
/// enforcing kernel `starttime_ticks`, `DIM_PROFILE_ID`, and `DIM_INSTANCE_NONCE` to defeat
/// PID reuse across crashes and OS reboots.
#[must_use]
pub fn runtime_hint_matches_entry(
    hint: &RuntimePidHint,
    entry: &ProcEntry,
    profile_id: &str,
    profile_dir: &Path,
    expected_uid: Option<u32>,
) -> bool {
    if entry.pid != hint.pid || !entry.is_active() {
        return false;
    }
    if let Some(ref recorded_profile) = hint.profile_id {
        if !recorded_profile.eq_ignore_ascii_case(profile_id) {
            return false;
        }
    }
    if let Some(expected_ticks) = hint.starttime_ticks {
        if entry.starttime_ticks != expected_ticks {
            return false;
        }
    }
    if let Some(ref expected_nonce) = hint.instance_nonce {
        match entry_instance_nonce(entry) {
            Some(actual_nonce) => {
                if actual_nonce != expected_nonce {
                    return false;
                }
            }
            None => {
                // Only permit an unreadable `environ` lacking `--setenv DIM_INSTANCE_NONCE`
                // if `starttime_ticks` was explicitly recorded and already matched above.
                if hint.starttime_ticks.is_none() {
                    return false;
                }
            }
        }
    }
    entry_matches_profile(entry, profile_id, profile_dir, expected_uid)
}

/// Collects the verified active process tree (including any outer `bwrap` wrapper and all inner
/// `zygote`, `renderer`, `zocr-host`, `zcall-native` descendants) and `VmRSS` byte total for a profile.
#[must_use]
pub fn collect_profile_process_tree(
    proc_root: &Path,
    profile_id: &str,
    profile_dir: &Path,
    zalo_data_dir: &Path,
    expected_uid: Option<u32>,
) -> ProfileProcessTree {
    let table = scan_proc_table(proc_root);
    if table.is_empty() {
        return ProfileProcessTree::default();
    }

    // Build parent -> children adjacency list over active processes matching `expected_uid`.
    let mut children_map: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for entry in table.values() {
        if entry.is_active()
            && expected_uid.is_none_or(|exp_u| entry.uid.is_none_or(|act_u| act_u == exp_u))
        {
            children_map.entry(entry.ppid).or_default().push(entry.pid);
        }
    }

    // Filter `.runtime-pid` hint to this profile if present.
    let runtime_hint = read_runtime_pid_hint(profile_dir).filter(|h| {
        h.profile_id
            .as_deref()
            .is_none_or(|hp| hp.eq_ignore_ascii_case(profile_id))
    });

    let verified_runtime_root = runtime_hint.as_ref().and_then(|hint| {
        let entry = table.get(&hint.pid)?;
        runtime_hint_matches_entry(hint, entry, profile_id, profile_dir, expected_uid)
            .then_some(hint.pid)
    });

    let expected_nonce = runtime_hint
        .as_ref()
        .and_then(|h| h.instance_nonce.as_deref());

    let verified_lock_root = read_lock_pid(zalo_data_dir).filter(|&lock_pid| {
        // If `.runtime-pid` recorded this exact PID and rejected it (e.g. `starttime_ticks` mismatch
        // after crash + PID reuse), `SingletonLock` for that recycled PID is also stale!
        if let Some(ref hint) = runtime_hint {
            if hint.pid == lock_pid && verified_runtime_root != Some(lock_pid) {
                // Kernel `starttime_ticks` identity recorded at launch and still matching proves
                // the SAME live process; only its profile tag is unverifiable (e.g. zero-filled
                // `environ` under `yama ptrace_scope=1`), so allow cmdline-subtree verification
                // below instead of rejecting outright.
                let starttime_identity_ok = hint
                    .starttime_ticks
                    .is_some_and(|t| table.get(&lock_pid).is_some_and(|e| e.starttime_ticks == t));
                if !starttime_identity_ok {
                    return false;
                }
            }
        }
        let Some(entry) = table.get(&lock_pid) else {
            return false;
        };
        if !entry_matches_profile(entry, profile_id, profile_dir, expected_uid) {
            // Fallback for yama-masked `environ`: the lock PID's own cmdline may lack profile
            // paths (e.g. `electron --no-sandbox /opt/zalo-linux/app`), so verify ownership via
            // its own or a descendant's cmdline carrying the profile or Zalo userData directory
            // (`--user-data-dir=<zalo_data_dir>` on Electron children).
            return lock_pid_matches_profile_via_cmdline(
                &table,
                lock_pid,
                profile_id,
                profile_dir,
                zalo_data_dir,
                expected_uid,
                expected_nonce,
            );
        }
        if let Some(exp_nonce) = expected_nonce {
            match entry_instance_nonce(entry) {
                Some(actual_nonce) if actual_nonce != exp_nonce => return false,
                None if verified_runtime_root.is_none() => return false,
                _ => {}
            }
        }
        true
    });

    let mut owned_set = BTreeSet::new();
    let mut ordered = Vec::new();
    let mut queue = VecDeque::new();

    // Seed outer wrapper (`bwrap` in `.runtime-pid`) first so topological `ordered.iter().rev()`
    // terminates inner descendants and app main before the outer `bwrap` wrapper.
    if let Some(rt_pid) = verified_runtime_root {
        if owned_set.insert(rt_pid) {
            ordered.push(rt_pid);
            queue.push_back(rt_pid);
        }
    }

    if let Some(lock_pid) = verified_lock_root {
        if owned_set.insert(lock_pid) {
            ordered.push(lock_pid);
            queue.push_back(lock_pid);
        }
    }

    // Capture any active reparented helper processes (`ppid == 1`, e.g. `zocr-host` or `zcall-native`)
    // carrying `DIM_PROFILE_ID=<profile_id>` (and matching `DIM_INSTANCE_NONCE` when tracked).
    for entry in table.values() {
        if !entry.is_active() {
            continue;
        }
        if expected_uid.is_some_and(|exp_u| entry.uid.is_some_and(|act_u| act_u != exp_u)) {
            continue;
        }
        let matches_tag =
            entry_profile_id(entry).is_some_and(|id| id.eq_ignore_ascii_case(profile_id));
        if !matches_tag {
            continue;
        }
        if let Some(exp_nonce) = expected_nonce {
            match entry_instance_nonce(entry) {
                Some(actual_nonce) if actual_nonce == exp_nonce => {}
                // If an outer `bwrap` wrapper has unreadable `environ` and matches `verified_runtime_root`,
                // it was already inserted above; reject untagged/wrong-nonce orphans when nonce is active.
                _ => continue,
            }
        } else if let Some(ref hint) = runtime_hint {
            if entry.pid == hint.pid && verified_runtime_root != Some(hint.pid) {
                continue;
            }
        }

        if owned_set.insert(entry.pid) {
            ordered.push(entry.pid);
            queue.push_back(entry.pid);
        }
    }

    while let Some(cur) = queue.pop_front() {
        if let Some(kids) = children_map.get(&cur) {
            for &kid in kids {
                let Some(kid_entry) = table.get(&kid) else {
                    continue;
                };
                if !kid_entry.is_active() {
                    continue;
                }
                // Guard against crossing into a differently tagged profile or nonce.
                if let Some(other_id) = entry_profile_id(kid_entry) {
                    if !other_id.eq_ignore_ascii_case(profile_id) {
                        continue;
                    }
                }
                if let (Some(exp_nonce), Some(kid_nonce)) =
                    (expected_nonce, entry_instance_nonce(kid_entry))
                {
                    if kid_nonce != exp_nonce {
                        continue;
                    }
                }
                if owned_set.insert(kid) {
                    ordered.push(kid);
                    queue.push_back(kid);
                }
            }
        }
    }

    let main_pid = verified_lock_root
        .or(verified_runtime_root)
        .or_else(|| ordered.first().copied());

    let mut process_starttimes = BTreeMap::new();
    let mut working_set_bytes = 0u64;
    for &pid in &ordered {
        if let Some(entry) = table.get(&pid) {
            process_starttimes.insert(pid, entry.starttime_ticks);
            working_set_bytes = working_set_bytes.saturating_add(entry.rss_bytes);
        }
    }

    ProfileProcessTree {
        main_pid,
        process_ids: ordered,
        process_starttimes,
        working_set_bytes,
    }
}

/// Collects protected PIDs and `VmRSS` for the user's untouched primary Linux `~/.config/ZaloData`.
#[must_use]
pub fn collect_primary_process_tree(
    proc_root: &Path,
    primary_zalo_data_dir: &Path,
    expected_uid: Option<u32>,
) -> ProfileProcessTree {
    let table = scan_proc_table(proc_root);
    let Some(lock_pid) = read_lock_pid(primary_zalo_data_dir) else {
        return ProfileProcessTree::default();
    };
    let Some(root_entry) = table.get(&lock_pid) else {
        return ProfileProcessTree::default();
    };
    if !root_entry.is_active() {
        return ProfileProcessTree::default();
    }
    if let (Some(exp_u), Some(act_u)) = (expected_uid, root_entry.uid) {
        if exp_u != act_u {
            return ProfileProcessTree::default();
        }
    }
    // Primary Zalo never carries DIM_PROFILE_ID.
    if entry_profile_id(root_entry).is_some() {
        return ProfileProcessTree::default();
    }

    let mut children_map: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for entry in table.values() {
        if entry.is_active() {
            children_map.entry(entry.ppid).or_default().push(entry.pid);
        }
    }

    let mut owned_set = BTreeSet::new();
    let mut ordered = Vec::new();
    let mut queue = VecDeque::new();
    owned_set.insert(lock_pid);
    ordered.push(lock_pid);
    queue.push_back(lock_pid);

    while let Some(cur) = queue.pop_front() {
        if let Some(kids) = children_map.get(&cur) {
            for &kid in kids {
                if owned_set.insert(kid) {
                    ordered.push(kid);
                    queue.push_back(kid);
                }
            }
        }
    }

    let mut process_starttimes = BTreeMap::new();
    let mut working_set_bytes = 0u64;
    for &pid in &ordered {
        if let Some(entry) = table.get(&pid) {
            process_starttimes.insert(pid, entry.starttime_ticks);
            working_set_bytes = working_set_bytes.saturating_add(entry.rss_bytes);
        }
    }

    ProfileProcessTree {
        main_pid: Some(lock_pid),
        process_ids: ordered,
        process_starttimes,
        working_set_bytes,
    }
}

/// Parses `wmctrl -lp` output strictly matching only windows whose `_NET_WM_PID` is in `allowed_pids`.
#[must_use]
pub fn parse_wmctrl_windows_for_pids(
    wmctrl_stdout: &str,
    allowed_pids: &[u32],
) -> Vec<OwnedX11Window> {
    if allowed_pids.is_empty() {
        return Vec::new();
    }
    let pid_set: BTreeSet<u32> = allowed_pids.iter().copied().collect();
    let mut windows = Vec::new();

    for line in wmctrl_stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Format: <0xWIN_ID> <DESKTOP_NUM> <PID> <WM_CLIENT_MACHINE> <TITLE...>
        let tokens: Vec<&str> = trimmed.split_whitespace().collect();
        if tokens.len() < 4 {
            continue;
        }
        let wid_hex = tokens[0];
        let Ok(pid) = tokens[2].parse::<u32>() else {
            continue;
        };
        if !pid_set.contains(&pid) {
            continue;
        }
        let raw_hex = wid_hex
            .strip_prefix("0x")
            .or_else(|| wid_hex.strip_prefix("0X"))
            .unwrap_or(wid_hex);
        let Ok(window_id) = u64::from_str_radix(raw_hex, 16) else {
            continue;
        };
        if !wid_hex
            .chars()
            .all(|c| c.is_ascii_hexdigit() || c == 'x' || c == 'X')
        {
            continue;
        }
        let title = if tokens.len() >= 5 {
            tokens[4..].join(" ")
        } else {
            String::new()
        };
        // Skip hidden Electron background helper windows (`Shared Worker`, `SQLite`).
        if title == "Shared Worker" || title == "SQLite" {
            continue;
        }
        windows.push(OwnedX11Window {
            window_id,
            window_id_hex: wid_hex.to_owned(),
            pid,
            title,
        });
    }

    windows
}

fn entry_profile_id(entry: &ProcEntry) -> Option<&str> {
    entry
        .environ
        .get(DIM_PROFILE_ID_ENV)
        .map(String::as_str)
        .or_else(|| cmdline_setenv_value(&entry.cmdline, DIM_PROFILE_ID_ENV))
}

fn entry_instance_nonce(entry: &ProcEntry) -> Option<&str> {
    entry
        .environ
        .get(DIM_INSTANCE_NONCE_ENV)
        .map(String::as_str)
        .or_else(|| cmdline_setenv_value(&entry.cmdline, DIM_INSTANCE_NONCE_ENV))
}

fn cmdline_setenv_value<'a>(cmdline: &'a [String], key: &str) -> Option<&'a str> {
    for window in cmdline.windows(3) {
        if window[0] == "--setenv" && window[1] == key && !window[2].is_empty() {
            return Some(window[2].as_str());
        }
    }
    None
}

/// Verifies that a `SingletonLock` PID belongs to a profile when `/proc/<pid>/environ` is
/// unreadable or zero-filled (e.g. `kernel.yama.ptrace_scope=1` masking a non-ancestor reader)
/// and the PID's own `cmdline` lacks profile paths (`electron --no-sandbox /opt/zalo-linux/app`).
///
/// Ownership is proven when the PID itself, or any active descendant discovered by BFS over
/// `ppid` links, has a `cmdline` argument containing `profile_dir` or `zalo_data_dir`
/// (Electron children carry `--user-data-dir=<zalo_data_dir>`). Descendants tagged with a
/// *different* `DIM_PROFILE_ID` are never trusted. Primary protection stays intact: absence of
/// `DIM_PROFILE_ID` is never a positive signal on its own — concrete path evidence is required.
///
/// When `expected_nonce` is tracked (structured `.runtime-pid`), a *readable conflicting*
/// nonce on the lock PID itself hard-rejects the subtree. Under `yama ptrace_scope=1` the
/// nonce is unreadable (zero-filled environ, no `--setenv` on the zalo-linux route) even for
/// a genuine launch, so absence alone is not a rejection: ownership is then proven by the
/// hint/lock PID agreement, the recorded `starttime_ticks` match, and concrete
/// `--user-data-dir` path evidence within the subtree. Subtrees without any path evidence
/// stay unverifiable and are never signaled (see regression 8).
#[must_use]
fn lock_pid_matches_profile_via_cmdline(
    table: &BTreeMap<u32, ProcEntry>,
    lock_pid: u32,
    profile_id: &str,
    profile_dir: &Path,
    zalo_data_dir: &Path,
    expected_uid: Option<u32>,
    expected_nonce: Option<&str>,
) -> bool {
    let profile_str = profile_dir.to_string_lossy();
    let zalo_str = zalo_data_dir.to_string_lossy();
    let cmdline_has_profile_path = |entry: &ProcEntry| {
        entry
            .cmdline
            .iter()
            .any(|arg| arg.contains(profile_str.as_ref()) || arg.contains(zalo_str.as_ref()))
    };
    let uid_allowed = |entry: &ProcEntry| {
        expected_uid.is_none_or(|exp_u| entry.uid.is_none_or(|act_u| act_u == exp_u))
    };

    let mut children_map: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for entry in table.values() {
        if entry.is_active() {
            children_map.entry(entry.ppid).or_default().push(entry.pid);
        }
    }

    let mut visited = BTreeSet::from([lock_pid]);
    let mut queue = VecDeque::from([lock_pid]);
    while let Some(cur) = queue.pop_front() {
        let Some(entry) = table.get(&cur) else {
            continue;
        };
        if !entry.is_active() || !uid_allowed(entry) {
            continue;
        }
        if let Some(other_id) = entry_profile_id(entry) {
            // Reject a *conflicting* tag; absence of `DIM_PROFILE_ID` is unknown, and
            // ownership is still decided by the cmdline path evidence below.
            if !other_id.eq_ignore_ascii_case(profile_id) {
                if cur == lock_pid {
                    // The lock PID itself openly claims another profile; never adopt it
                    // through descendant evidence.
                    return false;
                }
                continue;
            }
        }
        if let Some(exp_nonce) = expected_nonce {
            match entry_instance_nonce(entry) {
                Some(actual_nonce) if actual_nonce == exp_nonce => {}
                // Zero-filled/unreadable environ under `yama ptrace_scope=1` leaves the
                // nonce unknown even for a genuine launch (the zalo-linux route runs
                // without `bwrap`); ownership is then decided by the cmdline path
                // evidence below.
                None => {}
                _ => {
                    if cur == lock_pid {
                        // Readable conflicting nonce on the lock PID: the PID recorded in
                        // `.runtime-pid` demonstrably belongs to a different instance.
                        return false;
                    }
                    continue;
                }
            }
        }
        if cmdline_has_profile_path(entry) {
            return true;
        }
        if let Some(kids) = children_map.get(&cur) {
            for &kid in kids {
                if visited.insert(kid) {
                    queue.push_back(kid);
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zeroed_environ_is_detected_as_unknown() {
        assert!(environ_bytes_zeroed(&[0u8; 64]));
        assert!(environ_bytes_zeroed(&[0]));
        assert!(
            !environ_bytes_zeroed(&[]),
            "empty read means unreadable, not zero-filled"
        );
        assert!(!environ_bytes_zeroed(b"HOME=/home/u\0"));
        assert!(!environ_bytes_zeroed(b"PATH=/usr/bin\0\0\0"));
    }

    #[test]
    fn zeroed_environ_parses_to_unknown_empty_map() {
        // Under yama ptrace_scope=1 a non-ancestor reader gets zero-filled bytes; parsing must
        // yield an empty (unknown) map so verification falls through to cmdline matching.
        let masked = vec![0u8; 128];
        assert!(parse_proc_environ(&masked).is_empty());

        let mut normal = b"DIM_PROFILE_ID=prof1\0HOME=/home/u\0".to_vec();
        normal.push(0);
        let map = parse_proc_environ(&normal);
        assert_eq!(map.get("DIM_PROFILE_ID").map(String::as_str), Some("prof1"));
    }
}

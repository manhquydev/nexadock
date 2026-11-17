//! Stateless Linux `/proc` process-tree ownership, `SingletonLock` resolution,
//! recycled-PID spoofing defense, `VmRSS` memory measurement, and PID-scoped X11 window lookup.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::Path;

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

/// Observed process tree for an isolated Linux profile or primary instance.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileProcessTree {
    /// Verified main/root PID if running.
    pub main_pid: Option<u32>,
    /// Ordered list of active PIDs belonging to this profile (`main_pid` first, then descendants).
    pub process_ids: Vec<u32>,
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

/// Parses `/proc/<pid>/stat` safely even when `comm` contains spaces or nested parentheses.
#[must_use]
pub fn parse_proc_stat(stat_line: &str) -> Option<(u32, String, char, u32, u32)> {
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
    let mut fields = stat_line[close + 2..].split_whitespace();
    let state = fields.next()?.chars().next()?;
    let ppid = fields.next()?.parse::<u32>().ok()?;
    let pgrp = fields.next()?.parse::<u32>().ok()?;
    Some((pid, comm, state, ppid, pgrp))
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

/// Parses a NUL-separated `/proc/<pid>/environ` byte buffer into a key-value map.
#[must_use]
pub fn parse_proc_environ(bytes: &[u8]) -> BTreeMap<String, String> {
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
    // Trim trailing empty slots if any.
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
    let (parsed_pid, comm, state, ppid, pgrp) = parse_proc_stat(&stat_raw)?;
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

/// Reads `.runtime-pid` inside `profile_dir` (supports both structured JSON and plain integer legacy text).
#[must_use]
pub fn read_runtime_pid_hint(profile_dir: &Path) -> Option<u32> {
    let raw = fs::read_to_string(profile_dir.join(".runtime-pid")).ok()?;
    let trimmed = raw.trim();
    if let Ok(pid) = trimmed.parse::<u32>() {
        return (pid > 0).then_some(pid);
    }
    let val = serde_json::from_str::<serde_json::Value>(trimmed).ok()?;
    val.get("pid")
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .filter(|&p| p > 0)
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

    // If DIM_PROFILE_ID is present in environ, it is authoritative.
    if let Some(env_pid_id) = entry.environ.get("DIM_PROFILE_ID") {
        return env_pid_id.eq_ignore_ascii_case(profile_id);
    }

    let profile_str = profile_dir.to_string_lossy();
    let expected_xdg = profile_dir.join(".config").to_string_lossy().into_owned();

    if let Some(xdg) = entry.environ.get("XDG_CONFIG_HOME") {
        return xdg == &expected_xdg;
    }
    if let Some(home) = entry.environ.get("HOME") {
        return home == profile_str.as_ref();
    }

    // If environ was empty/unreadable (e.g., transient kernel thread or restricted procfs), check cmdline.
    entry
        .cmdline
        .iter()
        .any(|arg| arg.contains(profile_str.as_ref()))
}

/// Collects the verified active process tree and `VmRSS` byte total for a managed profile.
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

    // Build parent -> children adjacency list over active processes.
    let mut children_map: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for entry in table.values() {
        if entry.is_active() {
            children_map.entry(entry.ppid).or_default().push(entry.pid);
        }
    }

    let candidate_root = read_lock_pid(zalo_data_dir)
        .filter(|pid| {
            table
                .get(pid)
                .is_some_and(|e| entry_matches_profile(e, profile_id, profile_dir, expected_uid))
        })
        .or_else(|| {
            read_runtime_pid_hint(profile_dir).filter(|pid| {
                table.get(pid).is_some_and(|e| {
                    entry_matches_profile(e, profile_id, profile_dir, expected_uid)
                })
            })
        });

    let mut owned_set = BTreeSet::new();
    let mut ordered = Vec::new();
    let mut queue = VecDeque::new();

    if let Some(root_pid) = candidate_root {
        owned_set.insert(root_pid);
        ordered.push(root_pid);
        queue.push_back(root_pid);
    }

    // Also capture any active orphaned/reparented helper processes carrying `DIM_PROFILE_ID=<profile_id>`.
    for entry in table.values() {
        if entry.is_active()
            && entry
                .environ
                .get("DIM_PROFILE_ID")
                .is_some_and(|id| id.eq_ignore_ascii_case(profile_id))
            && expected_uid.is_none_or(|u| entry.uid.is_none_or(|eu| eu == u))
            && owned_set.insert(entry.pid)
        {
            ordered.push(entry.pid);
            queue.push_back(entry.pid);
        }
    }

    while let Some(cur) = queue.pop_front() {
        if let Some(kids) = children_map.get(&cur) {
            for &kid in kids {
                if let Some(kid_entry) = table.get(&kid) {
                    if !kid_entry.is_active() {
                        continue;
                    }
                    // Guard against crossing into a differently tagged profile.
                    if let Some(other_id) = kid_entry.environ.get("DIM_PROFILE_ID") {
                        if !other_id.eq_ignore_ascii_case(profile_id) {
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
    }

    let main_pid = candidate_root.or_else(|| ordered.first().copied());
    let working_set_bytes = ordered
        .iter()
        .filter_map(|pid| table.get(pid))
        .fold(0u64, |acc, e| acc.saturating_add(e.rss_bytes));

    ProfileProcessTree {
        main_pid,
        process_ids: ordered,
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
    if root_entry.environ.contains_key("DIM_PROFILE_ID") {
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

    let working_set_bytes = ordered
        .iter()
        .filter_map(|pid| table.get(pid))
        .fold(0u64, |acc, e| acc.saturating_add(e.rss_bytes));

    ProfileProcessTree {
        main_pid: Some(lock_pid),
        process_ids: ordered,
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
        let mut parts = trimmed
            .splitn(5, char::is_whitespace)
            .filter(|s| !s.is_empty());
        // Because columns are space-padded with multiple spaces, iterate token-by-token manually:
        let tokens: Vec<&str> = trimmed.split_whitespace().collect();
        let _ = &mut parts;
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

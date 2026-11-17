//! macOS window state inspection (`CGWindowListCopyWindowInfo` / `.window-state.json`) and
//! TCC-aware foreground activation (`NSRunningApplication` + optional `System Events` raise).

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use dim_core::{now_rfc3339_utc, DimError};

use crate::ops::{MacOsSystemOps, WindowInfo};

/// Fixture/runtime window metadata file written inside `ZaloData` by `fixture-macos-app.mjs`.
pub const MACOS_WINDOW_STATE_FILE: &str = ".window-state.json";

/// Parameterized JXA script querying CoreGraphics `CGWindowListCopyWindowInfo` for specific PIDs.
///
/// Does NOT require macOS TCC Accessibility or Screen Recording permissions to read `kCGWindowNumber`
/// and `kCGWindowOwnerPID`; uses `kCGWindowName` when granted or falls back to `kCGWindowOwnerName`.
pub const JXA_QUERY_WINDOWS_SCRIPT: &str = r#"ObjC.import('CoreGraphics');
ObjC.import('Foundation');
function run(argv) {
  var pidSet = {};
  for (var i = 0; i < argv.length; i++) {
    var p = parseInt(argv[i], 10);
    if (p > 0) pidSet[p] = true;
  }
  var raw = $.CGWindowListCopyWindowInfo($.kCGWindowListOptionAll, 0);
  var list = ObjC.deepUnwrap(raw) || [];
  var out = [];
  for (var j = 0; j < list.length; j++) {
    var w = list[j];
    var ownerPid = Number(w.kCGWindowOwnerPID || 0);
    var layer = Number(w.kCGWindowLayer || 0);
    if (pidSet[ownerPid] && layer === 0) {
      out.push({
        window_id: Number(w.kCGWindowNumber || 0),
        owner_pid: ownerPid,
        title: w.kCGWindowName || w.kCGWindowOwnerName || null,
        is_on_screen: Boolean(w.kCGWindowIsOnscreen)
      });
    }
  }
  return JSON.stringify(out);
}"#;

/// Parameterized AppleScript activating an `NSRunningApplication` by numeric PID (`argv` item 1)
/// without TCC Accessibility requirements, followed by best-effort `System Events` raise.
pub const APPLESCRIPT_ACTIVATE_BY_PID: &str = r#"use framework "AppKit"
on run argv
  set pidVal to (item 1 of argv) as integer
  set appRef to current application's NSRunningApplication's runningApplicationWithProcessIdentifier:pidVal
  if appRef is missing value then
    error "No NSRunningApplication for PID" number -600
  end if
  appRef's activateWithOptions:(current application's NSApplicationActivateIgnoringOtherApps)
  try
    tell application "System Events"
      set frontmost of first process whose unix id is pidVal to true
    end tell
  end try
  return "activated"
end run"#;

/// Parameterized AppleScript sending Cocoa graceful quit (`NSRunningApplication.terminate()`) by PID.
pub const APPLESCRIPT_TERMINATE_BY_PID: &str = r#"use framework "AppKit"
on run argv
  set pidVal to (item 1 of argv) as integer
  set appRef to current application's NSRunningApplication's runningApplicationWithProcessIdentifier:pidVal
  if appRef is not missing value then
    appRef's terminate()
    return "terminated"
  end if
  return "missing"
end run"#;

/// Inspects the primary visible window belonging to a profile's verified `process_ids`.
#[must_use]
pub fn inspect_profile_window(
    ops: &dyn MacOsSystemOps,
    zalo_data_dir: &Path,
    process_ids: &[u32],
) -> Option<WindowInfo> {
    if process_ids.is_empty() {
        return None;
    }

    // 1. Check `.window-state.json` if present and owned by one of the verified live PIDs.
    let win_file = zalo_data_dir.join(MACOS_WINDOW_STATE_FILE);
    if let Ok(raw) = fs::read_to_string(&win_file) {
        if let Some(win) = parse_window_state_json(&raw, process_ids) {
            return Some(win);
        }
    }

    // 2. Query OS / mock window server for the profile's process tree.
    let mut windows = ops.query_windows_for_pids(process_ids);
    windows.sort_by_key(|w| (!w.is_on_screen, w.window_id));
    windows.into_iter().next()
}

/// Queries macOS `CGWindowListCopyWindowInfo` via `/usr/bin/osascript -l JavaScript` using strictly
/// numeric PID arguments (zero shell/script string interpolation).
#[must_use]
pub fn query_macos_windows_via_osascript(
    ops: &dyn MacOsSystemOps,
    pids: &[u32],
) -> Vec<WindowInfo> {
    if pids.is_empty() {
        return Vec::new();
    }
    let mut args = vec![
        "-l".to_owned(),
        "JavaScript".to_owned(),
        "-e".to_owned(),
        JXA_QUERY_WINDOWS_SCRIPT.to_owned(),
    ];
    for pid in pids {
        args.push(pid.to_string());
    }
    let Ok(out) = ops.run_command(Path::new("/usr/bin/osascript"), &args, &[]) else {
        return Vec::new();
    };
    if !out.success() {
        return Vec::new();
    }
    parse_jxa_windows_json(&out.stdout_lossy())
}

/// Parses JSON array emitted by [`JXA_QUERY_WINDOWS_SCRIPT`].
#[must_use]
pub fn parse_jxa_windows_json(json_text: &str) -> Vec<WindowInfo> {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(json_text.trim()) else {
        return Vec::new();
    };
    let Some(arr) = val.as_array() else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let Some(window_id) = item.get("window_id").and_then(serde_json::Value::as_u64) else {
            continue;
        };
        let Some(owner_pid) = item
            .get("owner_pid")
            .and_then(serde_json::Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
        else {
            continue;
        };
        let title = item
            .get("title")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let is_on_screen = item
            .get("is_on_screen")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        out.push(WindowInfo {
            window_id,
            owner_pid,
            title,
            is_on_screen,
        });
    }
    out
}

/// Signals `.focus-signal` and activates the owning `NSRunningApplication` window on macOS.
pub fn activate_profile_window(
    ops: &dyn MacOsSystemOps,
    zalo_data_dir: &Path,
    main_pid: u32,
) -> Result<(), DimError> {
    write_signal_json_atomic(
        &zalo_data_dir.join(".focus-signal"),
        &serde_json::json!({
            "focused_at": now_rfc3339_utc(),
            "target_pid": main_pid,
        }),
    )?;

    if ops.is_native_macos() {
        let args = vec![
            "-e".to_owned(),
            APPLESCRIPT_ACTIVATE_BY_PID.to_owned(),
            main_pid.to_string(),
        ];
        let out = ops.run_command(Path::new("/usr/bin/osascript"), &args, &[])?;
        if !out.success() {
            let err_text = out.combined_lossy();
            // Do not hard-fail if TCC Accessibility (-1719 / -25211) blocked optional System Events raise,
            // but DO surface genuine missing-process failures (-600).
            if err_text.contains("-600") || err_text.contains("No NSRunningApplication") {
                return Err(DimError::NotRunning(format!(
                    "PID {main_pid} has no active NSRunningApplication"
                )));
            }
        }
    }

    Ok(())
}

/// Sends Cocoa `NSRunningApplication.terminate()` (`⌘Q`) via parameterized `osascript`.
pub fn request_graceful_app_terminate(ops: &dyn MacOsSystemOps, main_pid: u32) -> bool {
    if !ops.is_native_macos() {
        return false;
    }
    let args = vec![
        "-e".to_owned(),
        APPLESCRIPT_TERMINATE_BY_PID.to_owned(),
        main_pid.to_string(),
    ];
    ops.run_command(Path::new("/usr/bin/osascript"), &args, &[])
        .is_ok_and(|out| out.success() && out.stdout_lossy().contains("terminated"))
}

/// Writes a JSON signal file atomically without following symlinks.
pub fn write_signal_json_atomic(
    signal_path: &Path,
    payload: &serde_json::Value,
) -> Result<(), DimError> {
    if let Ok(meta) = fs::symlink_metadata(signal_path) {
        if meta.file_type().is_symlink() {
            return Err(DimError::PathTraversal(format!(
                "refusing to write through symlinked signal file '{}'",
                signal_path.display()
            )));
        }
    }
    let tmp = signal_path.with_extension(format!("tmp.{}", std::process::id()));
    let text = serde_json::to_string_pretty(payload)?;
    {
        let mut f = File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.write_all(b"\n")?;
        f.sync_all()?;
    }
    fs::rename(&tmp, signal_path)?;
    Ok(())
}

fn parse_window_state_json(raw: &str, allowed_pids: &[u32]) -> Option<WindowInfo> {
    let val = serde_json::from_str::<serde_json::Value>(raw).ok()?;
    let owner_pid = val
        .get("pid")
        .or_else(|| val.get("owner_pid"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())?;
    if !allowed_pids.contains(&owner_pid) {
        return None;
    }
    let window_id = val
        .get("window_id")
        .or_else(|| val.get("window_handle"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(u64::from(owner_pid));
    let title = val
        .get("title")
        .or_else(|| val.get("window_title"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let is_on_screen = val
        .get("visible")
        .or_else(|| val.get("is_on_screen"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    Some(WindowInfo {
        window_id,
        owner_pid,
        title,
        is_on_screen,
    })
}

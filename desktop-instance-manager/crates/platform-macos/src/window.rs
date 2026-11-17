//! macOS 14+ (Sonoma/Sequoia) cooperative window activation (`NSRunningApplication`) and
//! zero-TCC CoreGraphics window inspection (`CGWindowListCopyWindowInfo` / `.window-state.json`).
//!
//! Deliberately avoids `System Events` AppleEvents so `focus_or_restore` NEVER triggers intrusive
//! macOS Accessibility (`kTCCServiceAccessibility`) or Automation (`kTCCServiceAppleEvents`) dialogs,
//! and fails closed when macOS 14+ cooperative focus declines activation or target PID is missing.

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use dim_core::{now_rfc3339_utc, DimError};

use crate::ops::{MacOsSystemOps, WindowInfo};

static SIGNAL_SEQ: AtomicU64 = AtomicU64::new(1);

/// Fixture/runtime window metadata file written inside `ZaloData` by `fixture-macos-app.mjs`.
pub const MACOS_WINDOW_STATE_FILE: &str = ".window-state.json";

/// Parameterized JXA script querying CoreGraphics `CGWindowListCopyWindowInfo` for specific PIDs.
///
/// Reads `kCGWindowNumber`, `kCGWindowOwnerPID`, `kCGWindowLayer`, and `kCGWindowIsOnscreen`
/// without requiring macOS TCC Accessibility or Screen Recording permissions.
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

/// Zero-TCC AppKit AppleScript activating an `NSRunningApplication` by numeric PID (`argv` item 1),
/// unhiding minimized/hidden state, polling `isActive()`, and returning structured `status=...` text.
pub const APPLESCRIPT_ACTIVATE_BY_PID: &str = r#"use framework "AppKit"
on run argv
  set pidVal to (item 1 of argv) as integer
  set appRef to current application's NSRunningApplication's runningApplicationWithProcessIdentifier:pidVal
  if appRef is missing value then
    return "status=missing activated=false active=false"
  end if
  appRef's unhide()
  set didActivate to (appRef's activateWithOptions:3) as boolean
  set nowActive to (appRef's isActive()) as boolean
  repeat 3 times
    if nowActive then exit repeat
    delay 0.04
    set nowActive to (appRef's isActive()) as boolean
  end repeat
  return "status=ok activated=" & (didActivate as text) & " active=" & (nowActive as text)
end run"#;

/// Parameterized AppKit AppleScript sending Cocoa graceful quit (`NSRunningApplication.terminate()`).
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

/// Structured outcome of a macOS 14+ `NSRunningApplication` activation attempt (`P1-3`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowActivationOutcome {
    /// True if `runningApplicationWithProcessIdentifier:` located the target app.
    pub app_found: bool,
    /// Return value of `activateWithOptions:`.
    pub activated: bool,
    /// Observed `appRef.isActive()` state after bounded polling.
    pub is_active: bool,
    /// True if a layer-0 on-screen window was also confirmed via `CGWindowListCopyWindowInfo` or `.window-state.json`.
    pub window_verified: bool,
}

/// Parses structured stdout emitted by [`APPLESCRIPT_ACTIVATE_BY_PID`].
#[must_use]
pub fn parse_activation_stdout(stdout: &str) -> Option<(bool, bool, bool)> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut status_missing = false;
    let mut status_ok = false;
    let mut activated = false;
    let mut active = false;

    for tok in trimmed.split_whitespace() {
        if tok == "status=missing" {
            status_missing = true;
        } else if tok == "status=ok" {
            status_ok = true;
        } else if let Some(v) = tok.strip_prefix("activated=") {
            activated = v.eq_ignore_ascii_case("true");
        } else if let Some(v) = tok.strip_prefix("active=") {
            active = v.eq_ignore_ascii_case("true");
        }
    }

    if status_missing {
        return Some((false, false, false));
    }
    if status_ok {
        return Some((true, activated, active));
    }
    None
}

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

    let win_file = zalo_data_dir.join(MACOS_WINDOW_STATE_FILE);
    if fs::symlink_metadata(&win_file).is_ok_and(|m| m.is_file()) {
        if let Ok(raw) = fs::read_to_string(&win_file) {
            if let Some(win) = parse_window_state_json(&raw, process_ids) {
                return Some(win);
            }
        }
    }

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

/// Signals `.focus-signal` and activates the owning `NSRunningApplication` on macOS, enforcing
/// macOS 14+ cooperative focus verification (`activated=true` & `active=true`) fail-closed.
pub fn activate_profile_window(
    ops: &dyn MacOsSystemOps,
    zalo_data_dir: &Path,
    main_pid: u32,
) -> Result<WindowActivationOutcome, DimError> {
    if main_pid == 0 {
        return Err(DimError::NotRunning("invalid PID 0".to_owned()));
    }

    write_signal_json_atomic(
        &zalo_data_dir.join(".focus-signal"),
        &serde_json::json!({
            "focused_at": now_rfc3339_utc(),
            "target_pid": main_pid,
        }),
    )?;

    let window_verified = inspect_profile_window(ops, zalo_data_dir, &[main_pid])
        .is_some_and(|w| w.is_on_screen && w.window_id > 0);

    if !ops.is_native_macos() {
        return Ok(WindowActivationOutcome {
            app_found: true,
            activated: true,
            is_active: true,
            window_verified,
        });
    }

    let args = vec![
        "-l".to_owned(),
        "AppleScript".to_owned(),
        "-e".to_owned(),
        APPLESCRIPT_ACTIVATE_BY_PID.to_owned(),
        main_pid.to_string(),
    ];
    let out = ops.run_command(Path::new("/usr/bin/osascript"), &args, &[])?;
    if !out.success() {
        let err_text = out.combined_lossy();
        if err_text.contains("-600") || err_text.contains("No NSRunningApplication") {
            return Err(DimError::NotRunning(format!(
                "PID {main_pid} has no active NSRunningApplication"
            )));
        }
        return Err(DimError::ProcessError(format!(
            "macOS window activation osascript failed for PID {main_pid}: {}",
            err_text.trim()
        )));
    }

    let stdout_text = out.stdout_lossy();
    let Some((app_found, activated, is_active)) = parse_activation_stdout(&stdout_text) else {
        return Err(DimError::ProcessError(format!(
            "unrecognized macOS activation output for PID {main_pid}: '{}'",
            stdout_text.trim()
        )));
    };

    if !app_found {
        return Err(DimError::NotRunning(format!(
            "PID {main_pid} is not registered as a running macOS application"
        )));
    }

    if !activated || !is_active {
        return Err(DimError::ProcessError(format!(
            "macOS 14+ cooperative activation declined focus for PID {main_pid} (activated={activated}, active={is_active}, window_verified={window_verified})"
        )));
    }

    let post_window_verified = window_verified
        || inspect_profile_window(ops, zalo_data_dir, &[main_pid])
            .is_some_and(|w| w.is_on_screen && w.window_id > 0);

    Ok(WindowActivationOutcome {
        app_found: true,
        activated,
        is_active,
        window_verified: post_window_verified,
    })
}

/// Sends Cocoa `NSRunningApplication.terminate()` (`⌘Q`) via parameterized `osascript`.
pub fn request_graceful_app_terminate(ops: &dyn MacOsSystemOps, main_pid: u32) -> bool {
    if !ops.is_native_macos() {
        return false;
    }
    let args = vec![
        "-l".to_owned(),
        "AppleScript".to_owned(),
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
    let seq = SIGNAL_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = signal_path.with_extension(format!("tmp.{}.{seq}", std::process::id()));
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

//! Automated System Tray wake & Win32 Foreground Lock `second-instance` handoff test (`TC-H6` / `IR-WIN-01`).
//!
//! Verifies:
//! 1. Hidden System Tray window (`IsWindowVisible(hwnd_a) == 0`, `find_main_window_for_pids == None`)
//!    wakes via `focus_or_restore` isolated `second-instance` handoff in `< 2000ms` while sibling
//!    `Profile B` remains hidden (`IsWindowVisible(hwnd_b) == 0`).
//! 2. Visible window blocked under Win32 Foreground Lock (when `focus_window(hwnd_a)` cannot steal
//!    foreground directly from a non-foreground caller) falls through to `second-instance` handoff
//!    instead of returning early, restoring `hwnd_a` in `< 2000ms` across 5 consecutive iterations!
//! 3. Strict source-level ban: zero forbidden cross-thread queue attachment or synthetic keyboard injection
//!    APIs in `crates/platform-windows/src/**` and `scripts/dim-host.ps1`.

#![cfg(windows)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::ffi::c_void;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use dim_core::{InstanceLauncher, InstanceMonitor, LaunchTarget, ProfileStore};
use dim_platform_windows::{find_main_window_for_pids, WindowsPlatformAdapter};
use dim_profile_store::JsonProfileStore;

#[link(name = "user32")]
extern "system" {
    fn IsWindowVisible(h_wnd: *mut c_void) -> i32;
    fn GetForegroundWindow() -> *mut c_void;
}

fn workspace_dim_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

struct PsGuiHelper {
    child: Child,
    hwnd: u64,
    pid: u32,
    cmd_file: PathBuf,
    state_file: PathBuf,
}

impl PsGuiHelper {
    fn spawn(profile_dir: &Path, profile_id: &str, title: &str) -> Self {
        let zdata = profile_dir.join("AppData").join("Roaming").join("ZaloData");
        fs::create_dir_all(&zdata).unwrap();
        let cmd_file = zdata.join(".gui-cmd");
        let state_file = zdata.join("gui-state.json");
        let ready_file = zdata.join("gui-ready.txt");
        let ps1_file = profile_dir.join("tray-gui-fixture.ps1");

        // Signed `powershell.exe` Win32 GUI fixture modeling Electron's `DimTestMainWindow` (`900x650`)
        // + `second-instance` handler (`SW_RESTORE`/`SW_SHOW` -> `HWND_TOPMOST` -> `SetForegroundWindow` -> `HWND_NOTOPMOST`).
        let ps_script = r#"
param([string]$ProfileDir, [string]$ProfileId, [string]$WinTitle)
$ErrorActionPreference = 'Stop'
$cs = @'
using System;
using System.Runtime.InteropServices;

public static class GuiFix {
    public delegate IntPtr WndProcDel(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct WNDCLASSEX {
        public uint cbSize; public uint style; public WndProcDel lpfnWndProc;
        public int cbClsExtra; public int cbWndExtra; public IntPtr hInstance;
        public IntPtr hIcon; public IntPtr hCursor; public IntPtr hbrBackground;
        public string lpszMenuName; public string lpszClassName; public IntPtr hIconSm;
    }
    [StructLayout(LayoutKind.Sequential)] public struct POINT { public int x; public int y; }
    [StructLayout(LayoutKind.Sequential)] public struct MSG {
        public IntPtr hwnd; public uint message; public IntPtr wParam; public IntPtr lParam; public uint time; public POINT pt;
    }
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr GetModuleHandle(string name);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern ushort RegisterClassEx(ref WNDCLASSEX wc);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr CreateWindowEx(
        uint exStyle, string cls, string title, uint style, int x, int y, int w, int h,
        IntPtr parent, IntPtr menu, IntPtr inst, IntPtr param);
    [DllImport("user32.dll")] public static extern IntPtr DefWindowProc(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
    [DllImport("user32.dll")] public static extern bool PeekMessage(out MSG msg, IntPtr hWnd, uint min, uint max, uint rem);
    [DllImport("user32.dll")] public static extern bool TranslateMessage(ref MSG msg);
    [DllImport("user32.dll")] public static extern IntPtr DispatchMessage(ref MSG msg);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int cmd);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr after, int x, int y, int cx, int cy, uint flags);
    [DllImport("user32.dll")] public static extern bool DestroyWindow(IntPtr hWnd);

    private static WndProcDel _del = new WndProcDel(DefWindowProc);
    public static IntPtr CreateMain(string title) {
        WNDCLASSEX wc = new WNDCLASSEX();
        wc.cbSize = (uint)Marshal.SizeOf(typeof(WNDCLASSEX));
        wc.lpfnWndProc = _del;
        wc.hInstance = GetModuleHandle(null);
        wc.lpszClassName = "DimTestMainWindow";
        RegisterClassEx(ref wc);
        return CreateWindowEx(0u, "DimTestMainWindow", title, 0x10CF0000u, 60, 60, 900, 650, IntPtr.Zero, IntPtr.Zero, wc.hInstance, IntPtr.Zero);
    }
    public static void Pump() {
        MSG m;
        while (PeekMessage(out m, IntPtr.Zero, 0u, 0u, 1u)) {
            TranslateMessage(ref m);
            DispatchMessage(ref m);
        }
    }
    public static void RestoreFromSecondInstance(IntPtr hWnd) {
        ShowWindow(hWnd, 5);
        SetWindowPos(hWnd, new IntPtr(-1), 0, 0, 0, 0, 0x0003u);
        SetForegroundWindow(hWnd);
        SetWindowPos(hWnd, new IntPtr(-2), 0, 0, 0, 0, 0x0003u);
    }
}
'@
Add-Type -TypeDefinition $cs
$zdata = Join-Path $ProfileDir "AppData\Roaming\ZaloData"
$lockPath = Join-Path $zdata "lockfile"
$focusSig = Join-Path $zdata ".focus-signal"
$quitSig = Join-Path $zdata ".quit-signal"
$cmdFile = Join-Path $zdata ".gui-cmd"
$stateFile = Join-Path $zdata "gui-state.json"
$readyFile = Join-Path $zdata "gui-ready.txt"

$lockFs = [System.IO.File]::Open($lockPath, [System.IO.FileMode]::Create, [System.IO.FileAccess]::ReadWrite, [System.IO.FileShare]::Read)
$lockJson = [System.Text.Encoding]::UTF8.GetBytes("{`"pid`":$PID,`"profile_id`":`"$ProfileId`"}")
$lockFs.Write($lockJson, 0, $lockJson.Length)
$lockFs.Flush()

$hwnd = [GuiFix]::CreateMain($WinTitle)
$handoffCount = 0
[System.IO.File]::WriteAllText($stateFile, "{`"handoff_count`":0,`"hwnd`":$($hwnd.ToInt64())}")
[System.IO.File]::WriteAllText($readyFile, "$($hwnd.ToInt64())")

while ($true) {
    [GuiFix]::Pump()
    if ([System.IO.File]::Exists($quitSig)) { break }
    if ([System.IO.File]::Exists($cmdFile)) {
        $cmd = [System.IO.File]::ReadAllText($cmdFile).Trim()
        Remove-Item $cmdFile -Force -ErrorAction SilentlyContinue
        if ($cmd -eq "hide") { [void][GuiFix]::ShowWindow($hwnd, 0) }
        elseif ($cmd -eq "show") { [void][GuiFix]::ShowWindow($hwnd, 5) }
        elseif ($cmd -eq "front") { [GuiFix]::RestoreFromSecondInstance($hwnd) }
    }
    if ([System.IO.File]::Exists($focusSig)) {
        Remove-Item $focusSig -Force -ErrorAction SilentlyContinue
        $handoffCount++
        [GuiFix]::RestoreFromSecondInstance($hwnd)
        [System.IO.File]::WriteAllText($stateFile, "{`"handoff_count`":$handoffCount,`"hwnd`":$($hwnd.ToInt64())}")
    }
    Start-Sleep -Milliseconds 15
}

[void][GuiFix]::DestroyWindow($hwnd)
$lockFs.Close()
Remove-Item $lockPath -Force -ErrorAction SilentlyContinue
"#;
        fs::write(&ps1_file, ps_script).unwrap();

        let child = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                &ps1_file.display().to_string(),
                "-ProfileDir",
                &profile_dir.display().to_string(),
                "-ProfileId",
                profile_id,
                "-WinTitle",
                title,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn signed powershell GUI fixture");
        let pid = child.id();

        let deadline = Instant::now() + Duration::from_secs(10);
        let mut hwnd = 0u64;
        while Instant::now() < deadline {
            if let Ok(raw) = fs::read_to_string(&ready_file) {
                if let Ok(parsed) = raw.trim().parse::<i64>() {
                    if parsed != 0 {
                        hwnd = parsed as u64;
                        break;
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        assert!(hwnd != 0, "GUI fixture failed to report ready HWND");

        Self {
            child,
            hwnd,
            pid,
            cmd_file,
            state_file,
        }
    }

    fn send_cmd(&self, cmd: &str) {
        fs::write(&self.cmd_file, cmd).unwrap();
    }

    fn handoff_count(&self) -> u64 {
        fs::read_to_string(&self.state_file)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| v["handoff_count"].as_u64())
            .unwrap_or(0)
    }

    fn is_visible(&self) -> bool {
        unsafe { IsWindowVisible(self.hwnd as *mut c_void) != 0 }
    }
}

impl Drop for PsGuiHelper {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn tc_h6_tray_wake_foreground_lock_fallback_and_zero_forbidden_win32_apis() {
    // 1. Source-code audit: verify ZERO forbidden cross-thread input attachment or key injection APIs!
    let root = workspace_dim_root();
    let forbidden_a = format!("{}{}", "AttachThread", "Input");
    let forbidden_b = format!("{}{}", "Send", "Input");
    let forbidden_c = format!("{}{}", "keybd_", "event");
    for rel in [
        "crates/platform-windows/src/win32_ffi.rs",
        "crates/platform-windows/src/adapter.rs",
        "scripts/dim-host.ps1",
    ] {
        let src = fs::read_to_string(root.join(rel)).unwrap();
        assert!(
            !src.contains(&forbidden_a) && !src.contains(&forbidden_b) && !src.contains(&forbidden_c),
            "forbidden Win32 focus hack API found in {rel}"
        );
    }

    // 2. Spawn Profile A, Profile B, and an independent Blocker Foreground Window:
    let tmp = tempfile::tempdir().unwrap();
    let store = JsonProfileStore::new(tmp.path().join("data")).unwrap();
    let adapter = WindowsPlatformAdapter::new();
    let prof_a = store.create_profile("Tray Profile A", "").unwrap();
    let prof_b = store.create_profile("Tray Profile B", "").unwrap();
    let dir_a = store.profile_dir_for(&prof_a).unwrap();
    let dir_b = store.profile_dir_for(&prof_b).unwrap();
    adapter.ensure_profile_layout(&prof_a, &dir_a).unwrap();
    adapter.ensure_profile_layout(&prof_b, &dir_b).unwrap();

    // Minimal Node second-instance handoff script: writes `.focus-signal` into isolated `APPDATA\ZaloData` and exits 2
    // (mirroring Electron's `app.requestSingleInstanceLock()` second-instance signal!).
    let handoff_mjs = tmp.path().join("handoff-second-instance.mjs");
    fs::write(
        &handoff_mjs,
        "import fs from 'node:fs'; import path from 'node:path';\n\
         const z = path.join(process.env.APPDATA, 'ZaloData');\n\
         fs.writeFileSync(path.join(z, '.focus-signal'), new Date().toISOString());\n\
         process.exit(2);\n",
    )
    .unwrap();
    let target = LaunchTarget::NodeFixture {
        node_path: None,
        script_path: handoff_mjs,
        extra_args: Vec::new(),
    };

    let gui_a = PsGuiHelper::spawn(&dir_a, &prof_a.id, "Zalo - Profile A");
    let gui_b = PsGuiHelper::spawn(&dir_b, &prof_b.id, "Zalo - Profile B");
    let blocker_dir = tmp.path().join("blocker");
    let gui_blocker = PsGuiHelper::spawn(&blocker_dir, "ffffffff", "Blocker Foreground Window");

    assert_eq!(find_main_window_for_pids(&[gui_a.pid]).map(|(h, _)| h), Some(gui_a.hwnd));
    assert_eq!(find_main_window_for_pids(&[gui_b.pid]).map(|(h, _)| h), Some(gui_b.hwnd));

    // 3. Five consecutive cycles testing Case 1 (Hidden in System Tray) + Case 2 (Foreground Lock fallback):
    for cycle in 1..=5_u64 {
        // --- Case 1: Hide both Profile A & Profile B to System Tray (`IsWindowVisible == 0`) ---
        gui_a.send_cmd("hide");
        gui_b.send_cmd("hide");
        gui_blocker.send_cmd("front");
        let hide_deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < hide_deadline && (gui_a.is_visible() || gui_b.is_visible()) {
            std::thread::sleep(Duration::from_millis(15));
        }
        assert!(!gui_a.is_visible(), "Profile A must be hidden in Tray");
        assert!(!gui_b.is_visible(), "Profile B must be hidden in Tray");
        assert_eq!(find_main_window_for_pids(&[gui_a.pid]), None);

        let before_count = gui_a.handoff_count();
        let state_hidden = adapter.inspect_profile(&prof_a, &dir_a).unwrap();
        assert_eq!(state_hidden.window_handle, None);

        let t0 = Instant::now();
        adapter
            .focus_or_restore(&target, &prof_a, &dir_a, &state_hidden)
            .unwrap();
        while Instant::now() - t0 < Duration::from_millis(2000) {
            if gui_a.is_visible() && gui_a.handoff_count() > before_count {
                break;
            }
            std::thread::sleep(Duration::from_millis(15));
        }
        let latency_ms = t0.elapsed().as_millis();
        assert!(
            latency_ms < 2000,
            "cycle {cycle} Tray restore exceeded 2000ms ({latency_ms}ms)"
        );
        assert!(gui_a.is_visible(), "cycle {cycle}: Profile A failed to wake from Tray");
        assert!(
            !gui_b.is_visible(),
            "cycle {cycle}: sibling Profile B must remain undisturbed in Tray!"
        );

        // --- Case 2: Visible `hwnd_a` blocked by Foreground Lock ---
        // Simulate Foreground Lock rejection by placing `gui_blocker` on top (or testing non-foreground handle
        // where `GetForegroundWindow() != hwnd_a`) and verifying `focus_or_restore` falls through to handoff
        // whenever `focus_window` cannot place `hwnd_a` at `GetForegroundWindow()`.
        gui_blocker.send_cmd("front");
        std::thread::sleep(Duration::from_millis(40));
        let mut state_locked = adapter.inspect_profile(&prof_a, &dir_a).unwrap();
        // Force a valid non-foreground HWND check if this non-interactive runner allowed direct SetForegroundWindow:
        let fg_now = unsafe { GetForegroundWindow() } as u64;
        if fg_now == gui_a.hwnd || fg_now == 0 {
            // Supply an unfocusable/non-foreground handle (`0xDEAD_BEEF`) to deterministically exercise
            // the `if !focus_window(hwnd)` Foreground Lock fallback path on headless CI/runners too!
            state_locked.window_handle = Some(0x00DEAD00);
        }
        let before_lock_count = gui_a.handoff_count();
        let t1 = Instant::now();
        adapter
            .focus_or_restore(&target, &prof_a, &dir_a, &state_locked)
            .unwrap();
        while Instant::now() - t1 < Duration::from_millis(2000) {
            if gui_a.handoff_count() > before_lock_count {
                break;
            }
            std::thread::sleep(Duration::from_millis(15));
        }
        assert!(
            gui_a.handoff_count() > before_lock_count,
            "cycle {cycle}: Foreground Lock fallback failed to trigger isolated second-instance handoff"
        );
        assert!(t1.elapsed().as_millis() < 2000);
        assert!(!gui_b.is_visible());
    }
}

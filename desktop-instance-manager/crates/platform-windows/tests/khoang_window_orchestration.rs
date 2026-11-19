//! KHOANG Stage 2A Windows orchestration regression suite:
//! 1. Minimized (`IsIconic`) main window classified as `MainChatWindow` (not `NotificationPopup`)
//!    in both Rust (`win32_ffi.rs`) and PowerShell (`dim-host.ps1`), and directly restored.
//! 2. `inspect_profile_with_options(..., false)` skips recursive disk-size walks (`0`) while `true` computes it.
//! 3. `companion_snap_window` + `companion_unsnap_window` preserve `GetParent() == NULL`,
//!    `GWL_STYLE`/`GWL_EXSTYLE`, and restore the exact original `RECT`.

#![cfg(windows)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::upper_case_acronyms,
    non_snake_case
)]

use std::ffi::c_void;
use std::fs;
use std::mem::{size_of, zeroed};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{mpsc, Mutex};
use std::thread;

static WINDOW_TEST_LOCK: Mutex<()> = Mutex::new(());

use dim_core::{InstanceMonitor, ProfileRecord};
use dim_platform_windows::win32_ffi::{companion_snap_window, companion_unsnap_window};
use dim_platform_windows::{
    classify_window_for_pid, find_main_window_for_pids, focus_window_verified, get_window_parent,
    get_window_rect, get_window_styles, is_window_minimized, WindowsPlatformAdapter,
    ZaloWindowRole,
};

const WS_OVERLAPPEDWINDOW: u32 = 0x00CF_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const SW_MINIMIZE: i32 = 6;
const WM_USER_MINIMIZE: u32 = 0x0410;
const WM_USER_SHUTDOWN: u32 = 0x0411;
const PM_REMOVE: u32 = 0x0001;

#[repr(C)]
struct WNDCLASSEXW {
    cb_size: u32,
    style: u32,
    lpfn_wnd_proc: unsafe extern "system" fn(*mut c_void, u32, usize, isize) -> isize,
    cb_cls_extra: i32,
    cb_wnd_extra: i32,
    h_instance: *mut c_void,
    h_icon: *mut c_void,
    h_cursor: *mut c_void,
    hbr_background: *mut c_void,
    lpsz_menu_name: *const u16,
    lpsz_class_name: *const u16,
    h_icon_sm: *mut c_void,
}

#[repr(C)]
struct POINT {
    x: i32,
    y: i32,
}

#[repr(C)]
struct MSG {
    hwnd: *mut c_void,
    message: u32,
    w_param: usize,
    l_param: isize,
    time: u32,
    pt: POINT,
}

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(lp_module_name: *const u16) -> *mut c_void;
}

#[link(name = "user32")]
extern "system" {
    fn RegisterClassExW(lpwcx: *const WNDCLASSEXW) -> u16;
    fn CreateWindowExW(
        dw_ex_style: u32,
        lp_class_name: *const u16,
        lp_window_name: *const u16,
        dw_style: u32,
        x: i32,
        y: i32,
        n_width: i32,
        n_height: i32,
        h_wnd_parent: *mut c_void,
        h_menu: *mut c_void,
        h_instance: *mut c_void,
        lp_param: *mut c_void,
    ) -> *mut c_void;
    fn DefWindowProcW(hwnd: *mut c_void, msg: u32, w_param: usize, l_param: isize) -> isize;
    fn DestroyWindow(hwnd: *mut c_void) -> i32;
    fn ShowWindow(hwnd: *mut c_void, n_cmd_show: i32) -> i32;
    fn PeekMessageW(
        lp_msg: *mut MSG,
        hwnd: *mut c_void,
        w_msg_filter_min: u32,
        w_msg_filter_max: u32,
        w_remove_msg: u32,
    ) -> i32;
    fn TranslateMessage(lp_msg: *const MSG) -> i32;
    fn DispatchMessageW(lp_msg: *const MSG) -> isize;
    fn PostMessageW(hwnd: *mut c_void, msg: u32, w_param: usize, l_param: isize) -> i32;
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe extern "system" fn wnd_proc(
    hwnd: *mut c_void,
    msg: u32,
    w_param: usize,
    l_param: isize,
) -> isize {
    unsafe { DefWindowProcW(hwnd, msg, w_param, l_param) }
}

fn spawn_test_window_pair(with_host: bool) -> (u64, u64, thread::JoinHandle<()>) {
    let (tx, rx) = mpsc::channel::<(u64, u64)>();
    let handle = thread::spawn(move || unsafe {
        let hinst = GetModuleHandleW(std::ptr::null());
        let cls = to_wide("DimTestMainWindow");
        let wc = WNDCLASSEXW {
            cb_size: size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfn_wnd_proc: wnd_proc,
            cb_cls_extra: 0,
            cb_wnd_extra: 0,
            h_instance: hinst,
            h_icon: std::ptr::null_mut(),
            h_cursor: std::ptr::null_mut(),
            hbr_background: std::ptr::null_mut(),
            lpsz_menu_name: std::ptr::null(),
            lpsz_class_name: cls.as_ptr(),
            h_icon_sm: std::ptr::null_mut(),
        };
        let _ = RegisterClassExW(&wc);

        let title_main = to_wide("Zalo - KHOANG Regression Fixture");
        let hwnd_main = CreateWindowExW(
            0,
            cls.as_ptr(),
            title_main.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            100,
            100,
            900,
            650,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinst,
            std::ptr::null_mut(),
        );
        let hwnd_host = if with_host {
            let title_host = to_wide("KHOANG Host Fixture");
            CreateWindowExW(
                0,
                cls.as_ptr(),
                title_host.as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                40,
                60,
                520,
                680,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                hinst,
                std::ptr::null_mut(),
            )
        } else {
            std::ptr::null_mut()
        };

        let _ = tx.send((hwnd_main as u64, hwnd_host as u64));

        let mut msg: MSG = zeroed();
        loop {
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                if msg.message == WM_USER_MINIMIZE {
                    let _ = ShowWindow(hwnd_main, SW_MINIMIZE);
                } else if msg.message == WM_USER_SHUTDOWN {
                    let _ = DestroyWindow(hwnd_main);
                    if !hwnd_host.is_null() {
                        let _ = DestroyWindow(hwnd_host);
                    }
                    return;
                } else {
                    let _ = TranslateMessage(&msg);
                    let _ = DispatchMessageW(&msg);
                }
            }
            thread::sleep(std::time::Duration::from_millis(10));
        }
    });

    let (hwnd_main, hwnd_host) = rx.recv().expect("window pair spawned");
    thread::sleep(std::time::Duration::from_millis(120));
    (hwnd_main, hwnd_host, handle)
}

fn find_ps_script() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .join("..")
        .join("..")
        .join("scripts")
        .join("dim-host.ps1")
}

#[test]
fn minimized_main_window_classified_as_main_and_restored_in_rust_and_ps() {
    let _guard = WINDOW_TEST_LOCK.lock().unwrap();
    let (hwnd_main, _hwnd_host, handle) = spawn_test_window_pair(false);
    let pid = std::process::id();

    unsafe {
        let _ = PostMessageW(hwnd_main as *mut c_void, WM_USER_MINIMIZE, 0, 0);
    }
    let mut minimized = false;
    for _ in 0..30 {
        if is_window_minimized(hwnd_main) {
            minimized = true;
            break;
        }
        thread::sleep(std::time::Duration::from_millis(25));
    }
    assert!(minimized, "fixture main window should become IsIconic");

    // 1. Rust classification must be MainChatWindow, NOT NotificationPopup.
    let classified = classify_window_for_pid(hwnd_main);
    assert_eq!(
        classified.as_ref().map(|(_, role, _)| *role),
        Some(ZaloWindowRole::MainChatWindow),
        "minimized 900x650 window must classify as MainChatWindow, got: {classified:?}"
    );

    let found = find_main_window_for_pids(&[pid]);
    assert_eq!(found.as_ref().map(|(h, _)| *h), Some(hwnd_main));

    // 2. PowerShell dim-host.ps1 FindMainWindowForPids must also find the minimized window.
    let ps_script = find_ps_script();
    let ps_cmd = format!(
        ". '{}' | Out-Null; $pids = New-Object 'System.Collections.Generic.HashSet[int]'; [void]$pids.Add({}); $h = [IntPtr]::Zero; $t = ''; $ok = [DesktopInstanceManager.Host.HostEngine]::FindMainWindowForPids($pids, [ref]$h, [ref]$t); Write-Output \"$ok|$($h.ToInt64())\"",
        ps_script.display(),
        pid
    );
    let ps_out = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &ps_cmd,
        ])
        .output()
        .expect("run powershell dim-host.ps1");
    assert!(
        ps_out.status.success(),
        "powershell failed: {}",
        String::from_utf8_lossy(&ps_out.stderr)
    );
    let ps_raw = String::from_utf8_lossy(&ps_out.stdout);
    let ps_stdout = ps_raw.lines().last().unwrap_or("").trim().to_owned();
    assert_eq!(
        ps_stdout,
        format!("True|{hwnd_main}"),
        "dim-host.ps1 must classify minimized main window via GetWindowPlacement"
    );

    // 3. Direct verified restore must un-minimize the window.
    let (was_minimized, _) =
        focus_window_verified(hwnd_main, &[pid]).expect("focus_window_verified succeeds");
    assert!(
        was_minimized,
        "focus_window_verified must report was_minimized=true"
    );

    let mut restored = false;
    for _ in 0..30 {
        if !is_window_minimized(hwnd_main) {
            restored = true;
            break;
        }
        thread::sleep(std::time::Duration::from_millis(25));
    }
    assert!(restored, "minimized main window must be restored directly");

    unsafe {
        let _ = PostMessageW(hwnd_main as *mut c_void, WM_USER_SHUTDOWN, 0, 0);
    }
    let _ = handle.join();
}

#[test]
fn inspect_profile_with_options_gates_disk_size_walk() {
    let tmp = tempfile::tempdir().unwrap();
    let profile_dir = tmp.path().join("Profiles").join("fast_1111aaaa");
    let zalo_data = profile_dir.join("AppData").join("Roaming").join("ZaloData");
    fs::create_dir_all(&zalo_data).unwrap();
    fs::write(zalo_data.join("payload.bin"), vec![0xABu8; 8192]).unwrap();

    let profile = ProfileRecord {
        id: "1111aaaa".to_owned(),
        name: "Fast".to_owned(),
        directory_name: "fast_1111aaaa".to_owned(),
        created_at: "2026-11-19T00:00:00Z".to_owned(),
        last_launched_at: None,
        zalo_user_id: None,
        notes: String::new(),
    };

    let adapter = WindowsPlatformAdapter::new();
    let fast = adapter
        .inspect_profile_with_options(&profile, &profile_dir, false)
        .unwrap();
    assert_eq!(
        fast.disk_usage_bytes, 0,
        "include_size=false must skip recursive dir_size_no_follow walk"
    );

    let full = adapter
        .inspect_profile_with_options(&profile, &profile_dir, true)
        .unwrap();
    assert!(
        full.disk_usage_bytes >= 8192,
        "include_size=true must compute disk_usage_bytes, got {}",
        full.disk_usage_bytes
    );
}

#[test]
fn companion_snap_and_unsnap_preserve_parent_styles_and_exact_rect() {
    let _guard = WINDOW_TEST_LOCK.lock().unwrap();
    let (hwnd_sec, hwnd_host, handle) = spawn_test_window_pair(true);

    let orig_styles = get_window_styles(hwnd_sec);
    let orig_rect = get_window_rect(hwnd_sec).expect("orig rect");
    assert_eq!(get_window_parent(hwnd_sec), 0, "must start top-level");

    let (prev_rect, applied_rect) =
        companion_snap_window(hwnd_sec, hwnd_host, 64).expect("companion snap succeeds");
    assert_eq!(prev_rect, orig_rect);
    assert_eq!(
        get_window_parent(hwnd_sec),
        0,
        "GetParent() must remain NULL (never SetParent)"
    );
    assert_eq!(
        get_window_styles(hwnd_sec),
        orig_styles,
        "GWL_STYLE / GWL_EXSTYLE must remain unchanged"
    );

    let snapped_rect = get_window_rect(hwnd_sec).expect("snapped rect");
    assert_eq!(snapped_rect, applied_rect);

    companion_unsnap_window(hwnd_sec, prev_rect).expect("companion unsnap succeeds");
    let restored_rect = get_window_rect(hwnd_sec).expect("restored rect");
    assert_eq!(
        restored_rect, orig_rect,
        "unsnap must restore exact original RECT"
    );
    assert_eq!(get_window_parent(hwnd_sec), 0);
    assert_eq!(get_window_styles(hwnd_sec), orig_styles);

    unsafe {
        let _ = PostMessageW(hwnd_sec as *mut c_void, WM_USER_SHUTDOWN, 0, 0);
    }
    let _ = handle.join();
}

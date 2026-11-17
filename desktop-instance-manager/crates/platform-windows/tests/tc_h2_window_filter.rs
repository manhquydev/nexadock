//! Automated Win32 `EnumWindows` 6-condition main-window filter & `notiWin` exclusion suite (`TC-H2` / `IR-WIN-01` + `IR-WIN-02`).
//!
//! Spawns real Win32 HWNDs inside an isolated message-pump thread (zero real Zalo processes touched):
//! 1. `hwnd_main`: `960x680` unowned `Chrome_WidgetWin_1`, title `"Zalo - Account Alpha"`
//! 2. `hwnd_popup`: `352x82` topmost `WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE` (`notiWin` replica)
//! 3. `hwnd_owned_dialog`: `800x600` `Chrome_WidgetWin_1` owned by `hwnd_main` (`GW_OWNER != NULL`)
//! 4. `hwnd_empty_title`: `850x620` unowned `Chrome_WidgetWin_1` with empty title `""`
//!
//! Verifies both Rust (`dim_platform_windows`) and PowerShell (`scripts/dim-host.ps1`) select ONLY
//! `hwnd_main`, never send `WM_CLOSE` to `hwnd_popup`, and return `None`/`0` when `hwnd_main` hides to Tray.

#![cfg(windows)]
#![allow(clippy::unwrap_used, clippy::expect_used, non_snake_case)]

use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::thread;

use dim_platform_windows::win32_ffi::post_wm_close;
use dim_platform_windows::{
    classify_window_for_pid, find_main_window_for_pids, ZaloWindowRole,
};

const WS_OVERLAPPEDWINDOW: u32 = 0x00CF_0000;
const WS_POPUP: u32 = 0x8000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_EX_TOPMOST: u32 = 0x0000_0008;
const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
const WS_EX_NOACTIVATE: u32 = 0x0800_0000;
const SW_HIDE: i32 = 0;
const SWP_NOMOVE: u32 = 0x0002;
const SWP_NOSIZE: u32 = 0x0001;
const SWP_NOACTIVATE: u32 = 0x0010;
const HWND_TOPMOST: *mut c_void = (-1_isize) as *mut c_void;
const WM_CLOSE: u32 = 0x0010;
const WM_USER_HIDE_MAIN: u32 = 0x0401;
const WM_USER_SHUTDOWN: u32 = 0x0402;
const PM_REMOVE: u32 = 0x0001;

static MAIN_WM_CLOSE_COUNT: AtomicU32 = AtomicU32::new(0);
static POPUP_WM_CLOSE_COUNT: AtomicU32 = AtomicU32::new(0);
static OWNED_WM_CLOSE_COUNT: AtomicU32 = AtomicU32::new(0);

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
    fn DestroyWindow(h_wnd: *mut c_void) -> i32;
    fn DefWindowProcW(h_wnd: *mut c_void, msg: u32, w_param: usize, l_param: isize) -> isize;
    fn PeekMessageW(
        lp_msg: *mut MSG,
        h_wnd: *mut c_void,
        w_msg_filter_min: u32,
        w_msg_filter_max: u32,
        w_remove_msg: u32,
    ) -> i32;
    fn TranslateMessage(lp_msg: *const MSG) -> i32;
    fn DispatchMessageW(lp_msg: *const MSG) -> isize;
    fn PostMessageW(h_wnd: *mut c_void, msg: u32, w_param: usize, l_param: isize) -> i32;
    fn SetWindowPos(
        h_wnd: *mut c_void,
        h_wnd_insert_after: *mut c_void,
        x: i32,
        y: i32,
        cx: i32,
        cy: i32,
        u_flags: u32,
    ) -> i32;
    fn ShowWindow(h_wnd: *mut c_void, n_cmd_show: i32) -> i32;
    fn EnumWindows(
        lp_enum_func: unsafe extern "system" fn(*mut c_void, isize) -> i32,
        l_param: isize,
    ) -> i32;
}

static mut G_HWND_MAIN: usize = 0;
static mut G_HWND_POPUP: usize = 0;
static mut G_HWND_OWNED: usize = 0;

unsafe extern "system" fn test_wnd_proc(
    hwnd: *mut c_void,
    msg: u32,
    w_param: usize,
    l_param: isize,
) -> isize {
    unsafe {
        if msg == WM_CLOSE {
            let h = hwnd as usize;
            if h == G_HWND_MAIN {
                MAIN_WM_CLOSE_COUNT.fetch_add(1, Ordering::SeqCst);
            } else if h == G_HWND_POPUP {
                POPUP_WM_CLOSE_COUNT.fetch_add(1, Ordering::SeqCst);
            } else if h == G_HWND_OWNED {
                OWNED_WM_CLOSE_COUNT.fetch_add(1, Ordering::SeqCst);
            }
            return 0;
        }
        DefWindowProcW(hwnd, msg, w_param, l_param)
    }
}

fn wide_nul(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn dim_host_ps1_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .join("scripts")
        .join("dim-host.ps1")
}

fn query_powershell_main_hwnd(pid: u32) -> u64 {
    let ps1 = dim_host_ps1_path();
    let raw_src = std::fs::read_to_string(&ps1).expect("read dim-host.ps1");
    let start_marker = "$csharpSource = @'";
    let end_marker = "\n'@";
    let s_idx = raw_src.find(start_marker).expect("find csharp start") + start_marker.len();
    let e_idx = raw_src[s_idx..].find(end_marker).expect("find csharp end") + s_idx;
    let tmp = tempfile::tempdir().unwrap();
    let cs_file = tmp.path().join("host.cs");
    std::fs::write(&cs_file, &raw_src[s_idx..e_idx]).unwrap();

    let script = format!(
        "$src = [System.IO.File]::ReadAllText('{}'); \
         Add-Type -TypeDefinition $src -ReferencedAssemblies @('System.dll','System.Core.dll','System.Web.Extensions.dll'); \
         $set = New-Object 'System.Collections.Generic.HashSet[int]'; \
         [void]$set.Add({pid}); \
         $h = [IntPtr]::Zero; $t = ''; \
         [void][DesktopInstanceManager.Host.HostEngine]::FindMainWindowForProfile({pid}, $set, [ref]$h, [ref]$t); \
         Write-Output $h.ToInt64()",
        cs_file.display().to_string().replace('\'', "''")
    );
    let out = Command::new("powershell.exe")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &script])
        .output()
        .expect("invoke powershell FindMainWindowForProfile");
    assert!(
        out.status.success(),
        "PowerShell FindMainWindowForProfile failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    stdout
        .lines()
        .last()
        .unwrap_or("0")
        .trim()
        .parse::<i64>()
        .unwrap_or(0) as u64
}

unsafe extern "system" fn collect_z_order_cb(hwnd: *mut c_void, lparam: isize) -> i32 {
    let vec_ref = unsafe { &mut *(lparam as *mut Vec<u64>) };
    vec_ref.push(hwnd as u64);
    1
}

#[test]
fn tc_h2_part_2a_enum_windows_excludes_notiwin_popup_owned_and_empty_title() {
    MAIN_WM_CLOSE_COUNT.store(0, Ordering::SeqCst);
    POPUP_WM_CLOSE_COUNT.store(0, Ordering::SeqCst);
    OWNED_WM_CLOSE_COUNT.store(0, Ordering::SeqCst);

    let (tx, rx) = mpsc::channel::<(u64, u64, u64, u64)>();
    let gui_thread = thread::spawn(move || unsafe {
        let h_inst = GetModuleHandleW(std::ptr::null());
        let cls_main = wide_nul("Chrome_WidgetWin_1");
        let cls_popup = wide_nul("Chrome_WidgetWin_1");

        let wc = WNDCLASSEXW {
            cb_size: size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfn_wnd_proc: test_wnd_proc,
            cb_cls_extra: 0,
            cb_wnd_extra: 0,
            h_instance: h_inst,
            h_icon: std::ptr::null_mut(),
            h_cursor: std::ptr::null_mut(),
            hbr_background: std::ptr::null_mut(),
            lpsz_menu_name: std::ptr::null(),
            lpsz_class_name: cls_main.as_ptr(),
            h_icon_sm: std::ptr::null_mut(),
        };
        let _ = RegisterClassExW(&wc);

        let title_main = wide_nul("Zalo - Account Alpha");
        let title_popup = wide_nul("Zalo Notification");
        let title_owned = wide_nul("Zalo Settings Dialog");
        let title_empty = wide_nul("");

        let hwnd_main = CreateWindowExW(
            0,
            cls_main.as_ptr(),
            title_main.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            40,
            40,
            960,
            680,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            h_inst,
            std::ptr::null_mut(),
        );
        let hwnd_owned = CreateWindowExW(
            0,
            cls_main.as_ptr(),
            title_owned.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            80,
            80,
            800,
            600,
            hwnd_main,
            std::ptr::null_mut(),
            h_inst,
            std::ptr::null_mut(),
        );
        let hwnd_empty = CreateWindowExW(
            0,
            cls_main.as_ptr(),
            title_empty.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            90,
            90,
            850,
            620,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            h_inst,
            std::ptr::null_mut(),
        );
        // Create `notiWin` (`352x82`, `Chrome_WidgetWin_1` too since Electron BrowserWindow uses Chrome_WidgetWin_1!)
        // and place it at `HWND_TOPMOST` so `EnumWindows` visits `hwnd_popup` BEFORE `hwnd_main`!
        let hwnd_popup = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            cls_popup.as_ptr(),
            title_popup.as_ptr(),
            WS_POPUP | WS_VISIBLE,
            100,
            100,
            352,
            82,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            h_inst,
            std::ptr::null_mut(),
        );
        let _ = SetWindowPos(
            hwnd_popup,
            HWND_TOPMOST,
            100,
            100,
            352,
            82,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );

        G_HWND_MAIN = hwnd_main as usize;
        G_HWND_POPUP = hwnd_popup as usize;
        G_HWND_OWNED = hwnd_owned as usize;

        tx.send((
            hwnd_main as u64,
            hwnd_popup as u64,
            hwnd_owned as u64,
            hwnd_empty as u64,
        ))
        .unwrap();

        let mut running = true;
        while running {
            let mut msg: MSG = zeroed();
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                if msg.message == WM_USER_HIDE_MAIN {
                    let _ = ShowWindow(hwnd_main, SW_HIDE);
                } else if msg.message == WM_USER_SHUTDOWN {
                    running = false;
                } else {
                    let _ = TranslateMessage(&msg);
                    let _ = DispatchMessageW(&msg);
                }
            }
            thread::sleep(std::time::Duration::from_millis(10));
        }

        let _ = DestroyWindow(hwnd_popup);
        let _ = DestroyWindow(hwnd_owned);
        let _ = DestroyWindow(hwnd_empty);
        let _ = DestroyWindow(hwnd_main);
    });

    let (hwnd_main, hwnd_popup, hwnd_owned, hwnd_empty) = rx.recv().unwrap();
    assert!(hwnd_main != 0 && hwnd_popup != 0 && hwnd_owned != 0 && hwnd_empty != 0);
    thread::sleep(std::time::Duration::from_millis(50));

    // (a) Confirm `hwnd_popup` (`352x82` topmost) precedes `hwnd_main` in raw `EnumWindows` Z-order:
    let mut z_order = Vec::new();
    unsafe {
        let _ = EnumWindows(collect_z_order_cb, (&mut z_order as *mut Vec<u64>) as isize);
    }
    let idx_popup = z_order.iter().position(|&h| h == hwnd_popup).expect("popup in Z-order");
    let idx_main = z_order.iter().position(|&h| h == hwnd_main).expect("main in Z-order");
    assert!(
        idx_popup < idx_main,
        "expected topmost 352x82 notiWin popup ({idx_popup}) ahead of main window ({idx_main}) in Z-order"
    );

    // Verify `classify_window_for_pid` role classification (`IR-WIN-02`):
    assert_eq!(
        classify_window_for_pid(hwnd_popup).map(|(_, r, _)| r),
        Some(ZaloWindowRole::NotificationPopup)
    );
    assert_eq!(
        classify_window_for_pid(hwnd_owned).map(|(_, r, _)| r),
        Some(ZaloWindowRole::AuxiliaryOrSplash)
    );
    assert_eq!(
        classify_window_for_pid(hwnd_empty).map(|(_, r, _)| r),
        Some(ZaloWindowRole::AuxiliaryOrSplash)
    );
    assert_eq!(
        classify_window_for_pid(hwnd_main).map(|(_, r, _)| r),
        Some(ZaloWindowRole::MainChatWindow)
    );

    // (b) Both Rust & PowerShell `FindMainWindowForProfile` select `hwnd_main` (100% excluding popup/owned/empty):
    let self_pid = std::process::id();
    let rust_selected = find_main_window_for_pids(&[self_pid]).expect("must select main window");
    assert_eq!(rust_selected.0, hwnd_main);
    assert_eq!(rust_selected.1, "Zalo - Account Alpha");

    let ps_selected = query_powershell_main_hwnd(self_pid);
    assert_eq!(ps_selected, hwnd_main, "PowerShell parity mismatch on TC-H2");

    // (c) `post_wm_close` hits ONLY `hwnd_main` (`popup_wm_close_count == 0`):
    post_wm_close(rust_selected.0);
    thread::sleep(std::time::Duration::from_millis(50));
    assert_eq!(MAIN_WM_CLOSE_COUNT.load(Ordering::SeqCst), 1);
    assert_eq!(POPUP_WM_CLOSE_COUNT.load(Ordering::SeqCst), 0);
    assert_eq!(OWNED_WM_CLOSE_COUNT.load(Ordering::SeqCst), 0);

    // (d) Hide `hwnd_main` to System Tray while `hwnd_popup` (`352x82`) stays visible -> MUST return `None` (`0`)!
    unsafe {
        let _ = PostMessageW(hwnd_main as *mut c_void, WM_USER_HIDE_MAIN, 0, 0);
    }
    thread::sleep(std::time::Duration::from_millis(60));
    assert_eq!(find_main_window_for_pids(&[self_pid]), None);
    assert_eq!(query_powershell_main_hwnd(self_pid), 0);

    unsafe {
        let _ = PostMessageW(hwnd_main as *mut c_void, WM_USER_SHUTDOWN, 0, 0);
    }
    gui_thread.join().unwrap();
}

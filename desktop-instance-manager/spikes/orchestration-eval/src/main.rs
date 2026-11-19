#![cfg(windows)]
#![allow(non_camel_case_types, non_snake_case)]

use std::collections::HashSet;
use std::ffi::c_void;
use std::fs;
use std::mem::{size_of, zeroed};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

use dim_core::{
    InstanceLauncher, InstanceMonitor, LaunchTarget, ProfileLifecycleState, ProfileStore,
};
use dim_platform_windows::win32_ffi::{
    classify_window_for_pid, find_main_window_for_profile, focus_window,
    spawn_process_isolated_with_identity, sum_working_set_bytes, ProcessSnapshot, ZaloWindowRole,
};
use dim_platform_windows::{
    build_isolated_env_map, build_windows_command_line, encode_windows_env_block_utf16,
    WindowsPlatformAdapter,
};
use dim_process_manager::InstanceCoordinator;
use dim_profile_store::JsonProfileStore;

const WS_OVERLAPPEDWINDOW: u32 = 0x00CF_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const GWL_STYLE: i32 = -16;
const GWL_EXSTYLE: i32 = -20;
const GW_OWNER: u32 = 4;
const WS_EX_TOOLWINDOW: isize = 0x0000_0080;
const MIN_MAIN_WINDOW_DIM: i32 = 400;

const SW_SHOW: i32 = 5;
const SW_RESTORE: i32 = 9;
const SWP_NOSIZE: u32 = 0x0001;
const SWP_NOMOVE: u32 = 0x0002;
const SWP_NOZORDER: u32 = 0x0004;
const SWP_NOACTIVATE: u32 = 0x0010;
const HWND_TOPMOST: *mut c_void = (-1_isize) as *mut c_void;
const HWND_NOTOPMOST: *mut c_void = (-2_isize) as *mut c_void;
const MONITOR_DEFAULTTONEAREST: u32 = 2;
const SPI_GETFOREGROUNDLOCKTIMEOUT: u32 = 0x2000;
const PM_REMOVE: u32 = 0x0001;
const WM_USER_STOP_RAIL: u32 = 0x0410;
const WM_USER_FRONT_RAIL: u32 = 0x0411;

const GENERIC_READ_WRITE: u32 = 0xC000_0000;
const CREATE_ALWAYS: u32 = 2;
const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;
const INVALID_HANDLE_VALUE: *mut c_void = (-1_isize) as *mut c_void;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RECT {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct POINT {
    x: i32,
    y: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct WINDOWPLACEMENT {
    length: u32,
    flags: u32,
    show_cmd: u32,
    pt_min_position: POINT,
    pt_max_position: POINT,
    rc_normal_position: RECT,
}

#[repr(C)]
struct MONITORINFO {
    cb_size: u32,
    rc_monitor: RECT,
    rc_work: RECT,
    dw_flags: u32,
}

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
struct MSG {
    hwnd: *mut c_void,
    message: u32,
    w_param: usize,
    l_param: isize,
    time: u32,
    pt: POINT,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct FILETIME {
    dw_low_date_time: u32,
    dw_high_date_time: u32,
}

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(lp_module_name: *const u16) -> *mut c_void;
    fn CreateFileW(
        lp_file_name: *const u16,
        dw_desired_access: u32,
        dw_share_mode: u32,
        lp_security_attributes: *const c_void,
        dw_creation_disposition: u32,
        dw_flags_and_attributes: u32,
        h_template_file: *mut c_void,
    ) -> *mut c_void;
    fn CloseHandle(h_object: *mut c_void) -> i32;
    fn OpenProcess(dw_desired_access: u32, b_inherit_handle: i32, dw_process_id: u32) -> *mut c_void;
    fn GetProcessTimes(
        h_process: *mut c_void,
        lp_creation_time: *mut FILETIME,
        lp_exit_time: *mut FILETIME,
        lp_kernel_time: *mut FILETIME,
        lp_user_time: *mut FILETIME,
    ) -> i32;
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
    fn EnumWindows(
        lp_enum_func: unsafe extern "system" fn(*mut c_void, isize) -> i32,
        l_param: isize,
    ) -> i32;
    fn IsWindowVisible(h_wnd: *mut c_void) -> i32;
    fn IsIconic(h_wnd: *mut c_void) -> i32;
    fn IsZoomed(h_wnd: *mut c_void) -> i32;
    fn GetWindowThreadProcessId(h_wnd: *mut c_void, lpdw_process_id: *mut u32) -> u32;
    fn GetClassNameW(h_wnd: *mut c_void, lp_class_name: *mut u16, n_max_count: i32) -> i32;
    fn GetWindow(h_wnd: *mut c_void, u_cmd: u32) -> *mut c_void;
    fn GetParent(h_wnd: *mut c_void) -> *mut c_void;
    fn GetWindowLongPtrW(h_wnd: *mut c_void, n_index: i32) -> isize;
    fn GetWindowRect(h_wnd: *mut c_void, lp_rect: *mut RECT) -> i32;
    fn GetWindowPlacement(h_wnd: *mut c_void, lpwndpl: *mut WINDOWPLACEMENT) -> i32;
    fn GetWindowTextLengthW(h_wnd: *mut c_void) -> i32;
    fn GetWindowTextW(h_wnd: *mut c_void, lp_string: *mut u16, n_max_count: i32) -> i32;
    fn ShowWindow(h_wnd: *mut c_void, n_cmd_show: i32) -> i32;
    fn SetForegroundWindow(h_wnd: *mut c_void) -> i32;
    fn GetForegroundWindow() -> *mut c_void;
    fn SetWindowPos(
        h_wnd: *mut c_void,
        h_wnd_insert_after: *mut c_void,
        x: i32,
        y: i32,
        cx: i32,
        cy: i32,
        u_flags: u32,
    ) -> i32;
    fn MonitorFromWindow(hwnd: *mut c_void, dw_flags: u32) -> *mut c_void;
    fn GetMonitorInfoW(h_monitor: *mut c_void, lpmi: *mut MONITORINFO) -> i32;
    fn GetDpiForWindow(hwnd: *mut c_void) -> u32;
    fn SystemParametersInfoW(ui_action: u32, ui_param: u32, pv_param: *mut c_void, f_win_ini: u32) -> i32;
    fn BringWindowToTop(h_wnd: *mut c_void) -> i32;
    fn AllowSetForegroundWindow(dw_process_id: u32) -> i32;
}

fn wide_nul(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn stats_ms(samples: &[f64]) -> serde_json::Value {
    if samples.is_empty() {
        return serde_json::json!({ "count": 0 });
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    let sum: f64 = sorted.iter().sum();
    let p50 = sorted[n / 2];
    let p95_idx = ((n as f64) * 0.95).ceil() as usize;
    let p95 = sorted[p95_idx.saturating_sub(1).min(n - 1)];
    serde_json::json!({
        "count": n,
        "min_ms": (sorted[0] * 100.0).round() / 100.0,
        "p50_ms": (p50 * 100.0).round() / 100.0,
        "mean_ms": ((sum / n as f64) * 100.0).round() / 100.0,
        "p95_ms": (p95 * 100.0).round() / 100.0,
        "max_ms": (sorted[n - 1] * 100.0).round() / 100.0,
    })
}

/// Proposed remediated classifier handling minimized (`IsIconic`) windows via `GetWindowPlacement`
/// so minimized `Chrome_WidgetWin_1` / `DimTestMainWindow` windows are NOT misclassified as
/// `NotificationPopup` (`160x28` iconic rect) and can be restored directly via `SW_RESTORE` + `SetForegroundWindow`!
fn classify_window_v2(hwnd_val: u64) -> Option<(u32, ZaloWindowRole, String, bool)> {
    let hwnd = hwnd_val as *mut c_void;
    if hwnd.is_null() {
        return None;
    }
    unsafe {
        if IsWindowVisible(hwnd) == 0 {
            return None;
        }
        let mut pid: u32 = 0;
        let _ = GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return None;
        }
        let mut class_buf = [0u16; 256];
        let class_len = GetClassNameW(hwnd, class_buf.as_mut_ptr(), class_buf.len() as i32);
        let class_name = if class_len > 0 {
            String::from_utf16_lossy(&class_buf[..class_len as usize])
        } else {
            String::new()
        };
        let owner = GetWindow(hwnd, GW_OWNER);
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let is_toolwindow = (ex_style & WS_EX_TOOLWINDOW) != 0;
        let is_iconic = IsIconic(hwnd) != 0;

        let (width, height) = if is_iconic {
            let mut wp: WINDOWPLACEMENT = zeroed();
            wp.length = size_of::<WINDOWPLACEMENT>() as u32;
            if GetWindowPlacement(hwnd, &mut wp) != 0 {
                (
                    wp.rc_normal_position.right - wp.rc_normal_position.left,
                    wp.rc_normal_position.bottom - wp.rc_normal_position.top,
                )
            } else {
                (0, 0)
            }
        } else {
            let mut rect: RECT = zeroed();
            if GetWindowRect(hwnd, &mut rect) != 0 {
                (rect.right - rect.left, rect.bottom - rect.top)
            } else {
                (0, 0)
            }
        };

        let mut title = String::new();
        let text_len = GetWindowTextLengthW(hwnd);
        if text_len > 0 && text_len < 4096 {
            let mut text_buf = vec![0u16; (text_len as usize) + 1];
            let copied = GetWindowTextW(hwnd, text_buf.as_mut_ptr(), text_buf.len() as i32);
            if copied > 0 {
                title = String::from_utf16_lossy(&text_buf[..copied as usize]);
            }
        }

        if is_toolwindow || (width > 0 && height > 0 && width <= 420 && height <= 160) {
            return Some((pid, ZaloWindowRole::NotificationPopup, title, is_iconic));
        }

        if (class_name == "Chrome_WidgetWin_1" || class_name == "DimTestMainWindow")
            && owner.is_null()
            && !is_toolwindow
            && !title.trim().is_empty()
            && width >= MIN_MAIN_WINDOW_DIM
            && height >= MIN_MAIN_WINDOW_DIM
        {
            return Some((pid, ZaloWindowRole::MainChatWindow, title, is_iconic));
        }

        Some((pid, ZaloWindowRole::AuxiliaryOrSplash, title, is_iconic))
    }
}

struct SearchV2Ctx {
    main_pid: u32,
    pids: HashSet<u32>,
    found: Option<(u64, String, bool)>,
}

unsafe extern "system" fn enum_v2_cb(hwnd: *mut c_void, lparam: isize) -> i32 {
    let ctx = unsafe { &mut *(lparam as *mut SearchV2Ctx) };
    let Some((pid, role, title, iconic)) = classify_window_v2(hwnd as u64) else {
        return 1;
    };
    if !ctx.pids.contains(&pid) || role != ZaloWindowRole::MainChatWindow {
        return 1;
    }
    if pid == ctx.main_pid {
        ctx.found = Some((hwnd as u64, title, iconic));
        return 0;
    }
    if ctx.found.is_none() {
        ctx.found = Some((hwnd as u64, title, iconic));
    }
    1
}

fn find_main_window_v2(main_pid: u32, tree_pids: &[u32]) -> Option<(u64, String, bool)> {
    let mut pids: HashSet<u32> = tree_pids.iter().copied().collect();
    if main_pid > 0 {
        pids.insert(main_pid);
    }
    let mut ctx = SearchV2Ctx {
        main_pid,
        pids,
        found: None,
    };
    unsafe {
        let _ = EnumWindows(enum_v2_cb, (&mut ctx as *mut SearchV2Ctx) as isize);
    }
    ctx.found
}

fn filetime_to_ms(ft: FILETIME) -> f64 {
    let ticks = (u64::from(ft.dw_high_date_time) << 32) | u64::from(ft.dw_low_date_time);
    ticks as f64 / 10_000.0
}

fn query_process_cpu_ms(pid: u32) -> Option<f64> {
    unsafe {
        let h = OpenProcess(0x1000, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut c = FILETIME::default();
        let mut e = FILETIME::default();
        let mut k = FILETIME::default();
        let mut u = FILETIME::default();
        let ok = GetProcessTimes(h, &mut c, &mut e, &mut k, &mut u);
        let _ = CloseHandle(h);
        if ok != 0 {
            Some(filetime_to_ms(k) + filetime_to_ms(u))
        } else {
            None
        }
    }
}

struct MockRailWindow {
    hwnd: u64,
    handle: Option<thread::JoinHandle<()>>,
}

impl MockRailWindow {
    fn spawn() -> Self {
        let (tx, rx) = mpsc::channel::<u64>();
        let handle = thread::spawn(move || unsafe {
            let h_inst = GetModuleHandleW(std::ptr::null());
            let cls = wide_nul("KhoangMockRailWindow");
            let wc = WNDCLASSEXW {
                cb_size: size_of::<WNDCLASSEXW>() as u32,
                style: 0,
                lpfn_wnd_proc: DefWindowProcW,
                cb_cls_extra: 0,
                cb_wnd_extra: 0,
                h_instance: h_inst,
                h_icon: std::ptr::null_mut(),
                h_cursor: std::ptr::null_mut(),
                hbr_background: std::ptr::null_mut(),
                lpsz_menu_name: std::ptr::null(),
                lpsz_class_name: cls.as_ptr(),
                h_icon_sm: std::ptr::null_mut(),
            };
            let _ = RegisterClassExW(&wc);
            let title = wide_nul("KHOANG Mock Account Rail");
            let hwnd = CreateWindowExW(
                0,
                cls.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                10,
                10,
                240,
                700,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                h_inst,
                std::ptr::null_mut(),
            );
            let _ = SetForegroundWindow(hwnd);
            tx.send(hwnd as u64).unwrap();

            let mut running = true;
            while running {
                let mut msg: MSG = zeroed();
                while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                    if msg.message == WM_USER_STOP_RAIL {
                        running = false;
                    } else if msg.message == WM_USER_FRONT_RAIL {
                        let _ = ShowWindow(hwnd, SW_SHOW);
                        let _ = SetForegroundWindow(hwnd);
                    } else {
                        let _ = TranslateMessage(&msg);
                        let _ = DispatchMessageW(&msg);
                    }
                }
                thread::sleep(Duration::from_millis(5));
            }
            let _ = DestroyWindow(hwnd);
        });
        let hwnd = rx.recv().unwrap();
        thread::sleep(Duration::from_millis(40));
        Self {
            hwnd,
            handle: Some(handle),
        }
    }

    fn bring_to_front(&self) -> bool {
        unsafe {
            let _ = PostMessageW(self.hwnd as *mut c_void, WM_USER_FRONT_RAIL, 0, 0);
        }
        let dl = Instant::now() + Duration::from_millis(200);
        while Instant::now() < dl {
            if unsafe { GetForegroundWindow() as u64 } == self.hwnd {
                return true;
            }
            thread::sleep(Duration::from_millis(5));
        }
        unsafe { GetForegroundWindow() as u64 == self.hwnd }
    }
}

impl Drop for MockRailWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = PostMessageW(self.hwnd as *mut c_void, WM_USER_STOP_RAIL, 0, 0);
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

struct FixtureProc {
    child: Child,
    pid: u32,
    hwnd: u64,
    popup_hwnd: u64,
    owned_hwnd: u64,
    zdata_dir: PathBuf,
    cmd_file: PathBuf,
    state_file: PathBuf,
}

impl FixtureProc {
    fn spawn(
        gui_exe: &Path,
        profile_dir: &Path,
        title: &str,
        user_id: &str,
        x: i32,
        y: i32,
        with_popup: bool,
        with_owned: bool,
        class_override: Option<&str>,
    ) -> (Self, f64) {
        let roaming = profile_dir.join("AppData").join("Roaming");
        let zdata = roaming.join("ZaloData");
        fs::create_dir_all(&zdata).unwrap();
        let ready_file = zdata.join("gui-ready.json");
        let _ = fs::remove_file(&ready_file);

        let t0 = Instant::now();
        let mut cmd = Command::new(gui_exe);
        cmd.args([
            "--title",
            title,
            "--user-id",
            user_id,
            "--x",
            &x.to_string(),
            "--y",
            &y.to_string(),
            "--w",
            "900",
            "--h",
            "650",
            "--stay-ms",
            "120000",
        ]);
        if with_popup {
            cmd.arg("--with-popup");
        }
        if with_owned {
            cmd.arg("--with-owned");
        }
        if let Some(cls) = class_override {
            cmd.args(["--class", cls]);
        }
        cmd.env("USERPROFILE", profile_dir)
            .env("APPDATA", &roaming)
            .env("LOCALAPPDATA", profile_dir.join("AppData").join("Local"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let child = cmd.spawn().expect("spawn gui_fixture");
        let pid = child.id();

        let deadline = Instant::now() + Duration::from_secs(10);
        let mut ready_val: Option<serde_json::Value> = None;
        while Instant::now() < deadline {
            if let Ok(raw) = fs::read_to_string(&ready_file) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                    if v["hwnd"].as_u64().unwrap_or(0) != 0 {
                        ready_val = Some(v);
                        break;
                    }
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
        let v = ready_val.expect("gui_fixture failed to emit gui-ready.json");
        let hwnd = v["hwnd"].as_u64().unwrap();
        let popup_hwnd = v["popup_hwnd"].as_u64().unwrap_or(0);
        let owned_hwnd = v["owned_hwnd"].as_u64().unwrap_or(0);

        (
            Self {
                child,
                pid,
                hwnd,
                popup_hwnd,
                owned_hwnd,
                cmd_file: zdata.join(".gui-cmd"),
                state_file: zdata.join("gui-state.json"),
                zdata_dir: zdata,
            },
            elapsed_ms,
        )
    }

    fn send_cmd(&self, c: &str) {
        fs::write(&self.cmd_file, c).unwrap();
    }

    fn wait_until<F: Fn() -> bool>(&self, pred: F, timeout_ms: u64) -> bool {
        let dl = Instant::now() + Duration::from_millis(timeout_ms);
        while Instant::now() < dl {
            if pred() {
                return true;
            }
            thread::sleep(Duration::from_millis(5));
        }
        pred()
    }

    fn is_visible(&self) -> bool {
        unsafe { IsWindowVisible(self.hwnd as *mut c_void) != 0 }
    }

    fn is_iconic(&self) -> bool {
        unsafe { IsIconic(self.hwnd as *mut c_void) != 0 }
    }

    fn is_zoomed(&self) -> bool {
        unsafe { IsZoomed(self.hwnd as *mut c_void) != 0 }
    }

    fn rect(&self) -> RECT {
        let mut r = RECT::default();
        unsafe {
            let _ = GetWindowRect(self.hwnd as *mut c_void, &mut r);
        }
        r
    }

    fn handoff_count(&self) -> u64 {
        fs::read_to_string(&self.state_file)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| v["handoff_count"].as_u64())
            .unwrap_or(0)
    }
}

impl Drop for FixtureProc {
    fn drop(&mut self) {
        let _ = fs::write(self.zdata_dir.join(".quit-signal"), "quit");
        thread::sleep(Duration::from_millis(20));
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

unsafe extern "system" fn collect_z_cb(hwnd: *mut c_void, lparam: isize) -> i32 {
    let vec_ref = unsafe { &mut *(lparam as *mut Vec<u64>) };
    vec_ref.push(hwnd as u64);
    1
}

fn is_highest_z_order_among(target: u64, peers: &[u64]) -> bool {
    let mut z = Vec::new();
    unsafe {
        let _ = EnumWindows(collect_z_cb, (&mut z as *mut Vec<u64>) as isize);
    }
    let Some(t_idx) = z.iter().position(|&h| h == target) else {
        return false;
    };
    for &p in peers {
        if p == 0 || p == target {
            continue;
        }
        if let Some(p_idx) = z.iter().position(|&h| h == p) {
            if p_idx < t_idx {
                return false;
            }
        }
    }
    true
}

fn read_hwnd_class(hwnd_val: u64) -> String {
    if hwnd_val == 0 {
        return "<null>".to_owned();
    }
    let mut buf = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd_val as *mut c_void, buf.as_mut_ptr(), buf.len() as i32) };
    if len > 0 {
        String::from_utf16_lossy(&buf[..len as usize])
    } else {
        "<empty>".to_owned()
    }
}

/// Raises a verified window handle cleanly using documented Win32 APIs (ZERO forbidden APIs:
/// no AttachThreadInput / SendInput / keybd_event).
/// Returns `(os_foreground_confirmed, used_z_raise_fallback)`.
fn activate_verified_window_v2(hwnd_val: u64, target_pid: u32) -> (bool, bool) {
    let hwnd = hwnd_val as *mut c_void;
    if hwnd.is_null() {
        return (false, false);
    }
    unsafe {
        let _ = AllowSetForegroundWindow(target_pid);
        if IsIconic(hwnd) != 0 {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        } else {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
        let _ = SetForegroundWindow(hwnd);
        if GetForegroundWindow() == hwnd {
            return (true, false);
        }
        // When invoked from a non-foreground caller under Windows Foreground Lock,
        // `SetForegroundWindow` may be blocked; raise Z-order cleanly above peers
        // without changing window styles or stealing keyboard input via forbidden hacks.
        let _ = BringWindowToTop(hwnd);
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        let _ = SetWindowPos(hwnd, HWND_NOTOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        let _ = SetForegroundWindow(hwnd);
        (GetForegroundWindow() == hwnd, true)
    }
}

fn locate_dim_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("desktop-instance-manager root")
        .to_path_buf()
}

fn main() {
    let dim_root = locate_dim_root();
    let gui_exe = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("gui_fixture.exe");
    assert!(
        gui_exe.exists(),
        "gui_fixture.exe must be built alongside orchestration-eval: {}",
        gui_exe.display()
    );

    let xf_fixture = dim_root
        .join("fixtures")
        .join("cross-platform")
        .join("xf-fixture.mjs");
    assert!(xf_fixture.exists(), "missing xf-fixture.mjs");

    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    let synth_primary_dir = tmp.path().join("synth-primary");
    fs::create_dir_all(&synth_primary_dir).unwrap();

    // Ensure all guards point at synthetic primary, NEVER touching live Zalo!
    std::env::set_var(
        "DIM_PROTECTED_PRIMARY_ZALO_DATA",
        synth_primary_dir.join("AppData").join("Roaming").join("ZaloData"),
    );

    let store = Arc::new(JsonProfileStore::new(data_dir.clone()).unwrap());
    let adapter = Arc::new(WindowsPlatformAdapter::new());
    let coord = Arc::new(
        InstanceCoordinator::new(Arc::clone(&store), Arc::clone(&adapter))
            .with_protected_zalo_data_dir(
                synth_primary_dir.join("AppData").join("Roaming").join("ZaloData"),
            ),
    );

    let prof_a = store.create_profile("Secondary Alpha", "").unwrap();
    let prof_b = store.create_profile("Secondary Beta", "").unwrap();
    let prof_c = store.create_profile("Secondary Gamma", "").unwrap();
    let dir_a = store.profile_dir_for(&prof_a).unwrap();
    let dir_b = store.profile_dir_for(&prof_b).unwrap();
    let dir_c = store.profile_dir_for(&prof_c).unwrap();
    adapter.ensure_profile_layout(&prof_a, &dir_a).unwrap();
    adapter.ensure_profile_layout(&prof_b, &dir_b).unwrap();
    adapter.ensure_profile_layout(&prof_c, &dir_c).unwrap();

    let initial_fg_hwnd = unsafe { GetForegroundWindow() as u64 };
    let initial_fg_class = read_hwnd_class(initial_fg_hwnd);
    let mut fg_lock_timeout_ms: u32 = 0;
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_GETFOREGROUNDLOCKTIMEOUT,
            0,
            (&mut fg_lock_timeout_ms as *mut u32).cast(),
            0,
        );
    }

    let rail = MockRailWindow::spawn();
    let dpi_rail = unsafe { GetDpiForWindow(rail.hwnd as *mut c_void) };
    let rail_acquired_fg_initially = rail.bring_to_front();

    // =========================================================================
    // 1. Spawn 3 concurrent secondary GUI fixtures + 1 simulated Primary GUI fixture
    // =========================================================================
    let (fx_a, cold_gui_a_ms) = FixtureProc::spawn(
        &gui_exe,
        &dir_a,
        "Zalo - Secondary Alpha",
        "8810001",
        180,
        60,
        true,
        true,
        None,
    );
    let (fx_b, cold_gui_b_ms) = FixtureProc::spawn(
        &gui_exe,
        &dir_b,
        "Zalo - Secondary Beta",
        "8810002",
        220,
        90,
        true,
        false,
        None,
    );
    let (fx_c, cold_gui_c_ms) = FixtureProc::spawn(
        &gui_exe,
        &dir_c,
        "Zalo - Secondary Gamma",
        "8810003",
        260,
        120,
        true,
        false,
        None,
    );
    let (fx_primary, cold_gui_prim_ms) = FixtureProc::spawn(
        &gui_exe,
        &synth_primary_dir,
        "Zalo - Simulated Primary",
        "8899999",
        300,
        150,
        false,
        false,
        None,
    );

    // =========================================================================
    // 2. AUDIT VERIFICATION 1: Minimized window misclassification bug in current repo!
    // =========================================================================
    fx_a.send_cmd("minimize");
    assert!(fx_a.wait_until(|| fx_a.is_iconic(), 2000));
    thread::sleep(Duration::from_millis(30));

    let min_rect = fx_a.rect();
    let mut min_wp: WINDOWPLACEMENT = unsafe { zeroed() };
    min_wp.length = size_of::<WINDOWPLACEMENT>() as u32;
    unsafe {
        let _ = GetWindowPlacement(fx_a.hwnd as *mut c_void, &mut min_wp);
    }
    let min_visible = fx_a.is_visible();
    let min_iconic = fx_a.is_iconic();
    let current_classify_minimized = classify_window_for_pid(fx_a.hwnd).map(|(_, r, _)| format!("{r:?}"));
    let current_find_minimized = find_main_window_for_profile(fx_a.pid, &[fx_a.pid]);
    let v2_classify_minimized = classify_window_v2(fx_a.hwnd).map(|(_, r, _, ic)| (format!("{r:?}"), ic));
    let v2_find_minimized = find_main_window_v2(fx_a.pid, &[fx_a.pid]);

    let minimized_bug_audit = serde_json::json!({
        "is_window_visible": min_visible,
        "is_iconic": min_iconic,
        "get_window_rect": [min_rect.left, min_rect.top, min_rect.right, min_rect.bottom],
        "get_window_rect_size": [min_rect.right - min_rect.left, min_rect.bottom - min_rect.top],
        "get_window_placement_normal_size": [
            min_wp.rc_normal_position.right - min_wp.rc_normal_position.left,
            min_wp.rc_normal_position.bottom - min_wp.rc_normal_position.top
        ],
        "current_repo_classify_window_for_pid": current_classify_minimized,
        "current_repo_find_main_window_for_profile": current_find_minimized.as_ref().map(|(h, t)| serde_json::json!({"hwnd": h, "title": t})),
        "proposed_v2_classify_window": v2_classify_minimized,
        "proposed_v2_find_main_window": v2_find_minimized.as_ref().map(|(h, t, ic)| serde_json::json!({"hwnd": h, "title": t, "is_iconic": ic})),
    });

    // Restore fx_a before latency tests
    fx_a.send_cmd("restore");
    assert!(fx_a.wait_until(|| !fx_a.is_iconic(), 2000));

    // =========================================================================
    // 3. BENCHMARK: Single-Click Activation Latencies
    // =========================================================================

    // 3A. Running & visible -> direct focus (measuring BOTH repo `focus_window` AND `activate_verified_window_v2`)
    let all_peer_hwnds = [fx_a.hwnd, fx_b.hwnd, fx_c.hwnd, fx_primary.hwnd];
    let mut visible_repo_focus_ms = Vec::new();
    let mut visible_repo_fg_confirmed = 0usize;
    let mut visible_v2_focus_ms = Vec::new();
    let mut visible_v2_z_top = 0usize;
    let mut visible_v2_fg_confirmed = 0usize;
    let targets_vis = [
        ("A", fx_a.pid, fx_a.hwnd),
        ("B", fx_b.pid, fx_b.hwnd),
        ("C", fx_c.pid, fx_c.hwnd),
        ("Primary", fx_primary.pid, fx_primary.hwnd),
    ];
    for i in 0..12 {
        let (_, pid, expected_hwnd) = targets_vis[i % targets_vis.len()];
        let _ = rail.bring_to_front();
        let t0 = Instant::now();
        let found = find_main_window_for_profile(pid, &[pid]);
        let ok = found.map(|(h, _)| h == expected_hwnd && focus_window(h)).unwrap_or(false);
        let dt = t0.elapsed().as_secs_f64() * 1000.0;
        let fg_now = unsafe { GetForegroundWindow() as u64 };
        if ok && fg_now == expected_hwnd {
            visible_repo_fg_confirmed += 1;
        }
        visible_repo_focus_ms.push(dt);
    }
    for i in 0..12 {
        let (_, pid, expected_hwnd) = targets_vis[i % targets_vis.len()];
        let t0 = Instant::now();
        let found = find_main_window_v2(pid, &[pid]);
        let (fg_ok, _) = found
            .map(|(h, _, _)| {
                assert_eq!(h, expected_hwnd);
                activate_verified_window_v2(h, pid)
            })
            .unwrap_or((false, false));
        let dt = t0.elapsed().as_secs_f64() * 1000.0;
        if fg_ok {
            visible_v2_fg_confirmed += 1;
        }
        if is_highest_z_order_among(expected_hwnd, &all_peer_hwnds) {
            visible_v2_z_top += 1;
        }
        visible_v2_focus_ms.push(dt);
    }

    // 3B. Running & minimized -> restore + focus (comparing current fallback vs proposed v2 direct SW_RESTORE)
    let handoff_mjs = tmp.path().join("handoff-second-instance.mjs");
    fs::write(
        &handoff_mjs,
        "import fs from 'node:fs'; import path from 'node:path';\n\
         const z = path.join(process.env.APPDATA, 'ZaloData');\n\
         fs.writeFileSync(path.join(z, '.focus-signal'), new Date().toISOString());\n\
         process.exit(2);\n",
    )
    .unwrap();
    let node_handoff_target = LaunchTarget::NodeFixture {
        node_path: None,
        script_path: handoff_mjs.clone(),
        extra_args: Vec::new(),
    };

    let mut minimized_v2_direct_ms = Vec::new();
    let mut minimized_v2_restored_and_top = 0usize;
    let mut minimized_v2_os_fg = 0usize;
    for _ in 0..8 {
        fx_a.send_cmd("minimize");
        assert!(fx_a.wait_until(|| fx_a.is_iconic(), 2000));

        let t0 = Instant::now();
        let found = find_main_window_v2(fx_a.pid, &[fx_a.pid]);
        let (fg_ok, _) = found
            .map(|(h, _, iconic)| {
                assert_eq!(h, fx_a.hwnd);
                assert!(iconic);
                activate_verified_window_v2(h, fx_a.pid)
            })
            .unwrap_or((false, false));
        let dt = t0.elapsed().as_secs_f64() * 1000.0;
        let restored = !fx_a.is_iconic() && fx_a.is_visible();
        let z_top = is_highest_z_order_among(fx_a.hwnd, &all_peer_hwnds);
        if restored && z_top {
            minimized_v2_restored_and_top += 1;
        }
        if fg_ok && restored {
            minimized_v2_os_fg += 1;
        }
        minimized_v2_direct_ms.push(dt);
    }

    let mut minimized_current_fallback_ms = Vec::new();
    let mut minimized_current_confirmed = 0usize;
    for _ in 0..5 {
        fx_a.send_cmd("minimize");
        assert!(fx_a.wait_until(|| fx_a.is_iconic(), 2000));
        let _ = rail.bring_to_front();
        let before_ho = fx_a.handoff_count();
        let st = adapter.inspect_profile(&prof_a, &dir_a).unwrap();

        let t0 = Instant::now();
        adapter
            .focus_or_restore(&node_handoff_target, &prof_a, &dir_a, &st)
            .unwrap();
        let restored = fx_a.wait_until(|| !fx_a.is_iconic() && fx_a.handoff_count() > before_ho, 5000);
        let dt = t0.elapsed().as_secs_f64() * 1000.0;
        if restored {
            minimized_current_confirmed += 1;
        }
        minimized_current_fallback_ms.push(dt);
    }

    // 3C. Hidden / System Tray (`SW_HIDE`, `IsWindowVisible == 0`) -> restore + focus
    // Keep fx_b hidden to verify zero sibling disturbance, and fx_a keeps its 352x82 notiWin popup visible!
    fx_b.send_cmd("hide");
    assert!(fx_b.wait_until(|| !fx_b.is_visible(), 2000));

    let mut tray_node_handoff_ms = Vec::new();
    let mut tray_node_confirmed = 0usize;
    let mut tray_sibling_undisturbed = true;
    let mut tray_popup_never_focused = true;

    for _ in 0..5 {
        fx_a.send_cmd("hide");
        assert!(fx_a.wait_until(|| !fx_a.is_visible(), 2000));
        assert_eq!(find_main_window_for_profile(fx_a.pid, &[fx_a.pid]), None);
        assert_eq!(find_main_window_v2(fx_a.pid, &[fx_a.pid]), None);

        let before_ho = fx_a.handoff_count();
        let st_hidden = adapter.inspect_profile(&prof_a, &dir_a).unwrap();
        assert_eq!(st_hidden.window_handle, None);

        let t0 = Instant::now();
        adapter
            .focus_or_restore(&node_handoff_target, &prof_a, &dir_a, &st_hidden)
            .unwrap();
        let woke = fx_a.wait_until(|| fx_a.is_visible() && fx_a.handoff_count() > before_ho, 5000);
        let dt = t0.elapsed().as_secs_f64() * 1000.0;
        let fg_now = unsafe { GetForegroundWindow() as u64 };
        if fg_now == fx_a.popup_hwnd {
            tray_popup_never_focused = false;
        }
        if fx_b.is_visible() {
            tray_sibling_undisturbed = false;
        }
        if woke && fx_a.is_visible() {
            tray_node_confirmed += 1;
        }
        tray_node_handoff_ms.push(dt);
    }

    // Also measure native executable second-instance handoff (same mechanism as Electron Zalo.exe!)
    let mut tray_native_handoff_ms = Vec::new();
    let mut tray_native_confirmed = 0usize;
    for _ in 0..5 {
        fx_a.send_cmd("hide");
        assert!(fx_a.wait_until(|| !fx_a.is_visible(), 2000));
        let before_ho = fx_a.handoff_count();
        let base_env: Vec<(String, String)> = std::env::vars().collect();
        let env_map = build_isolated_env_map(&base_env, &prof_a, &dir_a);
        let env_block = encode_windows_env_block_utf16(&env_map);
        let cmd_line = build_windows_command_line(&gui_exe, &["--title".to_owned(), "Handoff".to_owned()]);

        let t0 = Instant::now();
        let _ = spawn_process_isolated_with_identity(&gui_exe, &cmd_line, &dir_a, &env_block).unwrap();
        let woke = fx_a.wait_until(|| fx_a.is_visible() && fx_a.handoff_count() > before_ho, 5000);
        let dt = t0.elapsed().as_secs_f64() * 1000.0;
        if woke && fx_a.is_visible() {
            tray_native_confirmed += 1;
        }
        tray_native_handoff_ms.push(dt);
    }

    // Restore fx_b for rapid switching test
    fx_b.send_cmd("show");
    assert!(fx_b.wait_until(|| fx_b.is_visible(), 2000));

    // =========================================================================
    // 4. BENCHMARK: Rapid Sequential Switching Accuracy (24 switches across 4 accounts + 20 rapid across 3 secondaries)
    // =========================================================================
    // Put C maximized, B minimized initially to test mixed window states!
    fx_c.send_cmd("maximize");
    assert!(fx_c.wait_until(|| fx_c.is_zoomed(), 2000));
    fx_b.send_cmd("minimize");
    assert!(fx_b.wait_until(|| fx_b.is_iconic(), 2000));

    let accounts = [
        ("Secondary Alpha", fx_a.pid, fx_a.hwnd, fx_a.popup_hwnd, fx_a.owned_hwnd),
        ("Secondary Beta", fx_b.pid, fx_b.hwnd, fx_b.popup_hwnd, fx_b.owned_hwnd),
        ("Secondary Gamma", fx_c.pid, fx_c.hwnd, fx_c.popup_hwnd, fx_c.owned_hwnd),
        ("Simulated Primary", fx_primary.pid, fx_primary.hwnd, 0u64, 0u64),
    ];

    let mut switch_24_ms = Vec::new();
    let mut switch_24_exact_hwnd_and_z_top = 0usize;
    let mut switch_24_os_fg = 0usize;
    let mut switch_24_wrong_window = 0usize;
    let mut switch_24_popup_hits = 0usize;
    let mut maximized_preserved = false;

    for i in 0..24 {
        let (label, pid, expected_hwnd, popup_h, owned_h) = accounts[i % accounts.len()];
        let t0 = Instant::now();
        let found = find_main_window_v2(pid, &[pid]);
        let selected_hwnd = found.map(|(h, _, _)| h).unwrap_or(0);
        let (fg_ok, _) = if selected_hwnd != 0 {
            activate_verified_window_v2(selected_hwnd, pid)
        } else {
            (false, false)
        };
        let dt = t0.elapsed().as_secs_f64() * 1000.0;
        let fg_now = unsafe { GetForegroundWindow() as u64 };
        let z_top = selected_hwnd == expected_hwnd
            && is_highest_z_order_among(expected_hwnd, &all_peer_hwnds)
            && unsafe { IsIconic(expected_hwnd as *mut c_void) == 0 && IsWindowVisible(expected_hwnd as *mut c_void) != 0 };
        if z_top {
            switch_24_exact_hwnd_and_z_top += 1;
            if label == "Secondary Gamma" && fx_c.is_zoomed() {
                maximized_preserved = true;
            }
        } else {
            switch_24_wrong_window += 1;
        }
        if fg_ok && fg_now == expected_hwnd {
            switch_24_os_fg += 1;
        }
        if (popup_h != 0 && (selected_hwnd == popup_h || fg_now == popup_h))
            || (owned_h != 0 && (selected_hwnd == owned_h || fg_now == owned_h))
        {
            switch_24_popup_hits += 1;
        }
        switch_24_ms.push(dt);
    }

    // 20 zero-delay back-to-back rapid switches strictly across 3 secondary accounts
    // (confirming simulated primary is NEVER focused or moved during secondary switches)
    let prim_rect_before = fx_primary.rect();
    let mut rapid_20_ms = Vec::new();
    let mut rapid_20_exact_z_top = 0usize;
    let mut rapid_20_os_fg = 0usize;
    let mut rapid_20_primary_touched = false;

    for i in 0..20 {
        let (_, pid, expected_hwnd, _, _) = accounts[i % 3];
        let t0 = Instant::now();
        let found = find_main_window_v2(pid, &[pid]);
        let selected_hwnd = found.map(|(h, _, _)| h).unwrap_or(0);
        let (fg_ok, _) = if selected_hwnd != 0 {
            activate_verified_window_v2(selected_hwnd, pid)
        } else {
            (false, false)
        };
        let dt = t0.elapsed().as_secs_f64() * 1000.0;
        let fg_now = unsafe { GetForegroundWindow() as u64 };
        if selected_hwnd == fx_primary.hwnd || fg_now == fx_primary.hwnd {
            rapid_20_primary_touched = true;
        }
        if selected_hwnd == expected_hwnd
            && is_highest_z_order_among(expected_hwnd, &all_peer_hwnds)
        {
            rapid_20_exact_z_top += 1;
        }
        if fg_ok && fg_now == expected_hwnd {
            rapid_20_os_fg += 1;
        }
        rapid_20_ms.push(dt);
    }
    let prim_rect_after = fx_primary.rect();
    if prim_rect_before != prim_rect_after {
        rapid_20_primary_touched = true;
    }

    // =========================================================================
    // 5. AUDIT VERIFICATION 4: Unverified Window Refusal & Fail-Closed States
    // =========================================================================
    // 5a. Title-spoofing decoy owned by another PID
    let spoof_dir = tmp.path().join("spoof-decoy");
    let (fx_spoof, _) = FixtureProc::spawn(
        &gui_exe,
        &spoof_dir,
        "Zalo - Secondary Alpha",
        "8877777",
        400,
        200,
        false,
        false,
        Some("Chrome_WidgetWin_1"),
    );
    // Hide real fx_a -> must return None (MUST NOT pick fx_spoof even though title matches & class is Chrome_WidgetWin_1!)
    fx_a.send_cmd("hide");
    assert!(fx_a.wait_until(|| !fx_a.is_visible(), 2000));
    let spoof_rejected = find_main_window_for_profile(fx_a.pid, &[fx_a.pid]).is_none()
        && find_main_window_v2(fx_a.pid, &[fx_a.pid]).is_none();
    drop(fx_spoof);
    fx_a.send_cmd("show");
    assert!(fx_a.wait_until(|| fx_a.is_visible(), 2000));

    // 5b. RecoveryRequired state (exclusively locked lockfile with NO runtime lease & non-matching lockfile owner)
    let prof_rec = store.create_profile("Recovery Locked", "").unwrap();
    let dir_rec = store.profile_dir_for(&prof_rec).unwrap();
    adapter.ensure_profile_layout(&prof_rec, &dir_rec).unwrap();
    let rec_lock = dir_rec.join("AppData").join("Roaming").join("ZaloData").join("lockfile");
    let rec_lock_w = wide_nul(&rec_lock.to_string_lossy());
    // Hold exclusive share=0 lock from current process (self_pid is filtered out of Restart Manager locking_pids!)
    let rec_handle = unsafe {
        CreateFileW(
            rec_lock_w.as_ptr(),
            GENERIC_READ_WRITE,
            0,
            std::ptr::null(),
            CREATE_ALWAYS,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        )
    };
    assert!(rec_handle != INVALID_HANDLE_VALUE);
    let rec_state = adapter.inspect_profile(&prof_rec, &dir_rec).unwrap();
    let rec_focus_err = coord
        .focus_profile(&prof_rec.id, &node_handoff_target)
        .err()
        .map(|e| format!("[{}] {}", e.code(), e));
    let rec_launch_err = coord
        .launch_profile(&prof_rec.id, &node_handoff_target)
        .err()
        .map(|e| format!("[{}] {}", e.code(), e));
    let rec_lock_intact = rec_lock.exists();
    unsafe {
        let _ = CloseHandle(rec_handle);
    }

    // =========================================================================
    // 6. EVALUATION 5: Optional Companion Snap Layout (SetWindowPos on verified secondary only)
    // =========================================================================
    fx_c.send_cmd("restore");
    assert!(fx_c.wait_until(|| !fx_c.is_zoomed(), 2000));
    let orig_b_rect = fx_b.rect();
    let orig_b_style = unsafe { GetWindowLongPtrW(fx_b.hwnd as *mut c_void, GWL_STYLE) };
    let orig_b_exstyle = unsafe { GetWindowLongPtrW(fx_b.hwnd as *mut c_void, GWL_EXSTYLE) };
    let orig_b_parent = unsafe { GetParent(fx_b.hwnd as *mut c_void) } as u64;

    let hmon = unsafe { MonitorFromWindow(fx_b.hwnd as *mut c_void, MONITOR_DEFAULTTONEAREST) };
    let mut mi: MONITORINFO = unsafe { zeroed() };
    mi.cb_size = size_of::<MONITORINFO>() as u32;
    let got_mi = unsafe { GetMonitorInfoW(hmon, &mut mi) != 0 };
    let rail_width = 240;
    let t_snap = Instant::now();
    if got_mi {
        let work_h = mi.rc_work.bottom - mi.rc_work.top;
        let work_w = mi.rc_work.right - mi.rc_work.left;
        unsafe {
            let _ = SetWindowPos(
                rail.hwnd as *mut c_void,
                std::ptr::null_mut(),
                mi.rc_work.left,
                mi.rc_work.top,
                rail_width,
                work_h,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            let _ = SetWindowPos(
                fx_b.hwnd as *mut c_void,
                std::ptr::null_mut(),
                mi.rc_work.left + rail_width,
                mi.rc_work.top,
                work_w - rail_width,
                work_h,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
    let snap_latency_ms = t_snap.elapsed().as_secs_f64() * 1000.0;
    thread::sleep(Duration::from_millis(25));
    let mut rail_snapped = RECT::default();
    unsafe {
        let _ = GetWindowRect(rail.hwnd as *mut c_void, &mut rail_snapped);
    }
    let b_snapped = fx_b.rect();
    let b_style_after = unsafe { GetWindowLongPtrW(fx_b.hwnd as *mut c_void, GWL_STYLE) };
    let b_exstyle_after = unsafe { GetWindowLongPtrW(fx_b.hwnd as *mut c_void, GWL_EXSTYLE) };
    let b_parent_after = unsafe { GetParent(fx_b.hwnd as *mut c_void) } as u64;
    // Unsnap back to original rect
    unsafe {
        let _ = SetWindowPos(
            fx_b.hwnd as *mut c_void,
            std::ptr::null_mut(),
            orig_b_rect.left,
            orig_b_rect.top,
            orig_b_rect.right - orig_b_rect.left,
            orig_b_rect.bottom - orig_b_rect.top,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    let b_restored = fx_b.rect();
    let companion_snap_eval = serde_json::json!({
        "monitor_rc_work": [mi.rc_work.left, mi.rc_work.top, mi.rc_work.right, mi.rc_work.bottom],
        "rail_dpi": dpi_rail,
        "snap_latency_ms": (snap_latency_ms * 100.0).round() / 100.0,
        "rail_snapped_rect": [rail_snapped.left, rail_snapped.top, rail_snapped.right, rail_snapped.bottom],
        "secondary_snapped_rect": [b_snapped.left, b_snapped.top, b_snapped.right, b_snapped.bottom],
        "styles_unchanged": orig_b_style == b_style_after && orig_b_exstyle == b_exstyle_after,
        "parent_still_null": orig_b_parent == 0 && b_parent_after == 0,
        "primary_untouched": fx_primary.rect() == prim_rect_before,
        "unsnap_restored_exact": b_restored == orig_b_rect,
    });

    // =========================================================================
    // 7. BENCHMARK: Real InstanceCoordinator + xf-fixture.mjs Cold Launch, Parallel Launch, Duplicate Coalescing & Inspect Overhead
    // =========================================================================
    let xf_data_dir = tmp.path().join("xf-data");
    let xf_store = Arc::new(JsonProfileStore::new(xf_data_dir).unwrap());
    let xf_coord = Arc::new(
        InstanceCoordinator::new(Arc::clone(&xf_store), Arc::clone(&adapter))
            .with_protected_zalo_data_dir(
                synth_primary_dir.join("AppData").join("Roaming").join("ZaloData"),
            ),
    );
    let xf_target = LaunchTarget::NodeFixture {
        node_path: None,
        script_path: xf_fixture.clone(),
        extra_args: vec!["--write-files=2".to_owned(), "--write-bytes=256".to_owned()],
    };

    let xp1 = xf_store.create_profile("XF Cold Single", "").unwrap();
    let t_cold1 = Instant::now();
    let rt_xp1 = xf_coord.launch_profile(&xp1.id, &xf_target).unwrap();
    let xf_cold_single_ms = t_cold1.elapsed().as_secs_f64() * 1000.0;
    assert!(rt_xp1.running);

    // 3 parallel cold launches via InstanceCoordinator
    let xp2 = xf_store.create_profile("XF Par 1", "").unwrap();
    let xp3 = xf_store.create_profile("XF Par 2", "").unwrap();
    let xp4 = xf_store.create_profile("XF Par 3", "").unwrap();
    let par_barrier = Arc::new(Barrier::new(3));
    let t_par = Instant::now();
    let mut par_handles = Vec::new();
    for p in [&xp2, &xp3, &xp4] {
        let c = Arc::clone(&xf_coord);
        let b = Arc::clone(&par_barrier);
        let id = p.id.clone();
        let tgt = xf_target.clone();
        par_handles.push(thread::spawn(move || {
            b.wait();
            let t = Instant::now();
            let rt = c.launch_profile(&id, &tgt).unwrap();
            (rt, t.elapsed().as_secs_f64() * 1000.0)
        }));
    }
    let mut par_times = Vec::new();
    let mut xf_pids = Vec::new();
    for h in par_handles {
        let (rt, ms) = h.join().unwrap();
        assert!(rt.running);
        if let Some(pid) = rt.main_pid {
            xf_pids.push(pid);
        }
        par_times.push(ms);
    }
    let xf_parallel_3_wall_ms = t_par.elapsed().as_secs_f64() * 1000.0;

    // 4x concurrent duplicate clicks on same stopped profile (Starting/Launching race)
    let xp_dup = xf_store.create_profile("XF Dup Race", "").unwrap();
    let dup_barrier = Arc::new(Barrier::new(4));
    let mut dup_handles = Vec::new();
    for _ in 0..4 {
        let c = Arc::clone(&xf_coord);
        let b = Arc::clone(&dup_barrier);
        let id = xp_dup.id.clone();
        let tgt = LaunchTarget::NodeFixture {
            node_path: None,
            script_path: xf_fixture.clone(),
            extra_args: vec!["--slow-startup-pre-lock-ms=200".to_owned()],
        };
        dup_handles.push(thread::spawn(move || {
            b.wait();
            c.launch_profile(&id, &tgt).unwrap()
        }));
    }
    let mut dup_pids = HashSet::new();
    for h in dup_handles {
        let rt = h.join().unwrap();
        if let Some(pid) = rt.main_pid {
            dup_pids.insert(pid);
        }
    }

    // Measure inspect_profile latency on small profile vs 1500-file profile (dir_size_no_follow audit!)
    let xp1_dir = xf_store.profile_dir_for(&xp1).unwrap();
    let mut inspect_small_ms = Vec::new();
    for _ in 0..10 {
        let t = Instant::now();
        let _ = adapter.inspect_profile(&xp1, &xp1_dir).unwrap();
        inspect_small_ms.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let cache_dir = xp1_dir.join("AppData").join("Roaming").join("ZaloData").join("synthetic_cache");
    fs::create_dir_all(&cache_dir).unwrap();
    for n in 0..1500 {
        let sub = cache_dir.join(format!("b{:02}", n % 25));
        if n < 25 {
            fs::create_dir_all(&sub).unwrap();
        }
        let _ = fs::write(sub.join(format!("f_{n:04}.dat")), b"0123456789abcdef");
    }
    let mut inspect_1500_files_ms = Vec::new();
    for _ in 0..10 {
        let t = Instant::now();
        let _ = adapter.inspect_profile(&xp1, &xp1_dir).unwrap();
        inspect_1500_files_ms.push(t.elapsed().as_secs_f64() * 1000.0);
    }

    // Pure EnumWindows + ProcessSnapshot polling cost (without dir_size_no_follow)
    let cpu_before = query_process_cpu_ms(std::process::id()).unwrap_or(0.0);
    let mut window_poll_ms = Vec::new();
    for _ in 0..50 {
        let t = Instant::now();
        let _ = find_main_window_v2(fx_a.pid, &[fx_a.pid]);
        let _ = find_main_window_v2(fx_b.pid, &[fx_b.pid]);
        let _ = find_main_window_v2(fx_c.pid, &[fx_c.pid]);
        let _ = find_main_window_v2(fx_primary.pid, &[fx_primary.pid]);
        window_poll_ms.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let cpu_after = query_process_cpu_ms(std::process::id()).unwrap_or(0.0);

    // Working set footprint of fixtures
    let gui_fixture_ram_mb = sum_working_set_bytes(&[fx_a.pid]) as f64 / (1024.0 * 1024.0);
    let xf_node_ram_mb = if let Some(&p) = xf_pids.first() {
        sum_working_set_bytes(&[p]) as f64 / (1024.0 * 1024.0)
    } else {
        0.0
    };
    let bench_self_ram_mb = sum_working_set_bytes(&[std::process::id()]) as f64 / (1024.0 * 1024.0);

    // Stop all xf-fixture processes cleanly
    let _ = xf_coord.stop_all_secondary_profiles();

    // =========================================================================
    // 8. Measure KHOANG Tauri Shell (`dim-desktop.exe` + WebView2 tree) Footprint in Isolated Temp Sandbox
    // =========================================================================
    let tauri_exe = dim_root
        .join("apps")
        .join("desktop")
        .join("src-tauri")
        .join("target")
        .join("release")
        .join("dim-desktop.exe");
    let tauri_footprint = if tauri_exe.exists() {
        let tauri_tmp_data = tmp.path().join("khoang-shell-sandbox");
        fs::create_dir_all(&tauri_tmp_data).unwrap();
        let mut child = Command::new(&tauri_exe)
            .env("DIM_DATA_DIR", &tauri_tmp_data)
            .env("DIM_NODE_FIXTURE_SCRIPT", &xf_fixture)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn isolated dim-desktop.exe");
        let tauri_pid = child.id();
        // Wait 3.5s for WebView2 + initial status render to settle
        thread::sleep(Duration::from_millis(3500));
        let snap = ProcessSnapshot::capture();
        let tree = snap.collect_tree(tauri_pid);
        let host_ws_mb = sum_working_set_bytes(&[tauri_pid]) as f64 / (1024.0 * 1024.0);
        let total_ws_mb = sum_working_set_bytes(&tree) as f64 / (1024.0 * 1024.0);
        let cpu_ms: f64 = tree.iter().filter_map(|&p| query_process_cpu_ms(p)).sum();
        let proc_names: Vec<String> = tree
            .iter()
            .map(|&p| format!("{}:{:.1}MB", snap.exe_name(p).unwrap_or("?"), sum_working_set_bytes(&[p]) as f64 / (1024.0 * 1024.0)))
            .collect();
        // Cleanly terminate ONLY our spawned dim-desktop.exe tree
        let _ = child.kill();
        let _ = child.wait();
        // Wait briefly and clean up any leftover child webview2 strictly in our collected tree
        thread::sleep(Duration::from_millis(200));
        for &p in &tree {
            if p != tauri_pid && dim_platform_windows::win32_ffi::is_pid_alive(p) {
                let _ = dim_platform_windows::win32_ffi::terminate_pid(p);
            }
        }
        serde_json::json!({
            "measured": true,
            "binary_size_mb": (fs::metadata(&tauri_exe).map(|m| m.len()).unwrap_or(0) as f64 / (1024.0 * 1024.0) * 100.0).round() / 100.0,
            "process_tree_count": tree.len(),
            "rust_host_ram_mb": (host_ws_mb * 100.0).round() / 100.0,
            "total_shell_plus_webview2_ram_mb": (total_ws_mb * 100.0).round() / 100.0,
            "total_cpu_ms_first_3_5s": (cpu_ms * 100.0).round() / 100.0,
            "processes": proc_names,
        })
    } else {
        serde_json::json!({ "measured": false, "reason": "release dim-desktop.exe not found" })
    };

    // Assemble full structured report
    let report = serde_json::json!({
        "environment": {
            "rail_dpi": dpi_rail,
            "initial_foreground_hwnd": initial_fg_hwnd,
            "initial_foreground_class": initial_fg_class,
            "foreground_lock_timeout_ms": fg_lock_timeout_ms,
            "rail_acquired_foreground_initially": rail_acquired_fg_initially,
        },
        "minimized_window_bug_audit": minimized_bug_audit,
        "activation_latency": {
            "running_visible_repo_focus_window": {
                "stats": stats_ms(&visible_repo_focus_ms),
                "os_foreground_confirmed": format!("{visible_repo_fg_confirmed}/12"),
            },
            "running_visible_v2_verified_raise": {
                "stats": stats_ms(&visible_v2_focus_ms),
                "z_order_top_among_accounts": format!("{visible_v2_z_top}/12"),
                "os_foreground_confirmed": format!("{visible_v2_fg_confirmed}/12"),
            },
            "minimized_remediated_v2_direct_restore_focus": {
                "stats": stats_ms(&minimized_v2_direct_ms),
                "restored_and_z_top": format!("{minimized_v2_restored_and_top}/8"),
                "os_foreground_confirmed": format!("{minimized_v2_os_fg}/8"),
            },
            "minimized_current_repo_fallback_handoff": {
                "stats": stats_ms(&minimized_current_fallback_ms),
                "confirmed_restored": format!("{minimized_current_confirmed}/5"),
            },
            "tray_hidden_node_handoff_focus_or_restore": {
                "stats": stats_ms(&tray_node_handoff_ms),
                "confirmed_woke": format!("{tray_node_confirmed}/5"),
                "sibling_hidden_undisturbed": tray_sibling_undisturbed,
                "popup_never_focused": tray_popup_never_focused,
            },
            "tray_hidden_native_exe_second_instance_handoff": {
                "stats": stats_ms(&tray_native_handoff_ms),
                "confirmed_woke": format!("{tray_native_confirmed}/5"),
            },
            "cold_launch_to_visible_focused_gui_fixture_ms": {
                "secondary_a": (cold_gui_a_ms * 100.0).round() / 100.0,
                "secondary_b": (cold_gui_b_ms * 100.0).round() / 100.0,
                "secondary_c": (cold_gui_c_ms * 100.0).round() / 100.0,
                "simulated_primary": (cold_gui_prim_ms * 100.0).round() / 100.0,
            },
            "cold_launch_coordinator_xf_fixture_mjs": {
                "single_ms": (xf_cold_single_ms * 100.0).round() / 100.0,
                "parallel_3_wall_ms": (xf_parallel_3_wall_ms * 100.0).round() / 100.0,
                "parallel_3_per_profile": stats_ms(&par_times),
            },
        },
        "rapid_switching_accuracy": {
            "mixed_4_accounts_24_switches": {
                "exact_target_z_top_and_restored": switch_24_exact_hwnd_and_z_top,
                "os_foreground_confirmed": switch_24_os_fg,
                "total": 24,
                "target_accuracy_pct": ((switch_24_exact_hwnd_and_z_top as f64 / 24.0) * 10000.0).round() / 100.0,
                "wrong_window_activations": switch_24_wrong_window,
                "decoy_popup_or_owned_hits": switch_24_popup_hits,
                "maximized_state_preserved": maximized_preserved,
                "latency": stats_ms(&switch_24_ms),
            },
            "rapid_3_secondaries_20_switches": {
                "exact_target_z_top": rapid_20_exact_z_top,
                "os_foreground_confirmed": rapid_20_os_fg,
                "total": 20,
                "target_accuracy_pct": ((rapid_20_exact_z_top as f64 / 20.0) * 10000.0).round() / 100.0,
                "primary_ever_touched": rapid_20_primary_touched,
                "latency": stats_ms(&rapid_20_ms),
            },
        },
        "fail_closed_security_and_races": {
            "title_spoofing_decoy_rejected": spoof_rejected,
            "recovery_required_detected": rec_state.lifecycle_state() == ProfileLifecycleState::RecoveryRequired,
            "recovery_required_status_label": rec_state.status_label,
            "recovery_focus_refused": rec_focus_err,
            "recovery_launch_refused": rec_launch_err,
            "recovery_lockfile_intact": rec_lock_intact,
            "concurrent_4x_launch_coalesced_unique_pids": dup_pids.len(),
        },
        "companion_snap_layout_eval": companion_snap_eval,
        "resource_and_polling_overhead": {
            "inspect_profile_small_dir": stats_ms(&inspect_small_ms),
            "inspect_profile_1500_files_dir_size_bottleneck": stats_ms(&inspect_1500_files_ms),
            "enum_windows_4_accounts_poll": stats_ms(&window_poll_ms),
            "enum_windows_50x4_total_cpu_ms": ((cpu_after - cpu_before) * 100.0).round() / 100.0,
            "gui_fixture_ram_mb": (gui_fixture_ram_mb * 100.0).round() / 100.0,
            "xf_node_fixture_ram_mb": (xf_node_ram_mb * 100.0).round() / 100.0,
            "orchestrator_rust_process_ram_mb": (bench_self_ram_mb * 100.0).round() / 100.0,
            "khoang_tauri_shell": tauri_footprint,
        }
    });

    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}

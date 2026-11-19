#![cfg(windows)]
#![allow(non_camel_case_types, non_snake_case)]

use std::env;
use std::ffi::c_void;
use std::fs;
use std::mem::{size_of, zeroed};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const WS_OVERLAPPEDWINDOW: u32 = 0x00CF_0000;
const WS_POPUP: u32 = 0x8000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_EX_TOPMOST: u32 = 0x0000_0008;
const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
const WS_EX_NOACTIVATE: u32 = 0x0800_0000;

const SW_HIDE: i32 = 0;
const SW_MAXIMIZE: i32 = 3;
const SW_SHOW: i32 = 5;
const SW_MINIMIZE: i32 = 6;
const SW_RESTORE: i32 = 9;

const SWP_NOSIZE: u32 = 0x0001;
const SWP_NOMOVE: u32 = 0x0002;
const SWP_NOACTIVATE: u32 = 0x0010;
const HWND_TOPMOST: *mut c_void = (-1_isize) as *mut c_void;
const HWND_NOTOPMOST: *mut c_void = (-2_isize) as *mut c_void;

const WM_CLOSE: u32 = 0x0010;
const PM_REMOVE: u32 = 0x0001;

const GENERIC_READ_WRITE: u32 = 0xC000_0000;
const FILE_SHARE_READ: u32 = 0x0000_0001;
const CREATE_ALWAYS: u32 = 2;
const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;
const INVALID_HANDLE_VALUE: *mut c_void = (-1_isize) as *mut c_void;
const ERROR_SHARING_VIOLATION: u32 = 32;

static CLOSE_REQUESTED: AtomicBool = AtomicBool::new(false);

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

#[repr(C)]
struct RECT {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
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
    fn WriteFile(
        h_file: *mut c_void,
        lp_buffer: *const c_void,
        n_number_of_bytes_to_write: u32,
        lp_number_of_bytes_written: *mut u32,
        lp_overlapped: *mut c_void,
    ) -> i32;
    fn FlushFileBuffers(h_file: *mut c_void) -> i32;
    fn CloseHandle(h_object: *mut c_void) -> i32;
    fn GetLastError() -> u32;
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
    fn ShowWindow(h_wnd: *mut c_void, n_cmd_show: i32) -> i32;
    fn IsIconic(h_wnd: *mut c_void) -> i32;
    fn IsWindowVisible(h_wnd: *mut c_void) -> i32;
    fn GetWindowRect(h_wnd: *mut c_void, lp_rect: *mut RECT) -> i32;
    fn SetForegroundWindow(h_wnd: *mut c_void) -> i32;
    fn SetWindowPos(
        h_wnd: *mut c_void,
        h_wnd_insert_after: *mut c_void,
        x: i32,
        y: i32,
        cx: i32,
        cy: i32,
        u_flags: u32,
    ) -> i32;
}

fn wide_nul(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe extern "system" fn wnd_proc(
    hwnd: *mut c_void,
    msg: u32,
    w_param: usize,
    l_param: isize,
) -> isize {
    if msg == WM_CLOSE {
        CLOSE_REQUESTED.store(true, Ordering::SeqCst);
        return 0;
    }
    unsafe { DefWindowProcW(hwnd, msg, w_param, l_param) }
}

struct LockFileGuard(*mut c_void, PathBuf);

impl Drop for LockFileGuard {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
        let _ = fs::remove_file(&self.1);
    }
}

struct Opts {
    title: String,
    class_name: String,
    user_id: String,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    with_popup: bool,
    with_owned: bool,
    stay_ms: u64,
    no_lock: bool,
}

fn parse_opts() -> Opts {
    let mut o = Opts {
        title: "Zalo Fixture".to_owned(),
        class_name: "DimTestMainWindow".to_owned(),
        user_id: "8800001".to_owned(),
        x: 120,
        y: 80,
        w: 900,
        h: 650,
        with_popup: false,
        with_owned: false,
        stay_ms: 90_000,
        no_lock: false,
    };
    let args: Vec<String> = env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        let next = |idx: &mut usize| -> Option<String> {
            if *idx + 1 < args.len() {
                *idx += 1;
                Some(args[*idx].clone())
            } else {
                None
            }
        };
        match a {
            "--title" => {
                if let Some(v) = next(&mut i) {
                    o.title = v;
                }
            }
            "--class" => {
                if let Some(v) = next(&mut i) {
                    o.class_name = v;
                }
            }
            "--user-id" => {
                if let Some(v) = next(&mut i) {
                    o.user_id = v;
                }
            }
            "--x" => {
                if let Some(v) = next(&mut i) {
                    o.x = v.parse().unwrap_or(o.x);
                }
            }
            "--y" => {
                if let Some(v) = next(&mut i) {
                    o.y = v.parse().unwrap_or(o.y);
                }
            }
            "--w" => {
                if let Some(v) = next(&mut i) {
                    o.w = v.parse().unwrap_or(o.w);
                }
            }
            "--h" => {
                if let Some(v) = next(&mut i) {
                    o.h = v.parse().unwrap_or(o.h);
                }
            }
            "--stay-ms" => {
                if let Some(v) = next(&mut i) {
                    o.stay_ms = v.parse().unwrap_or(o.stay_ms);
                }
            }
            "--with-popup" => o.with_popup = true,
            "--with-owned" => o.with_owned = true,
            "--no-lock" => o.no_lock = true,
            _ => {
                if let Some(v) = a.strip_prefix("--user-id=") {
                    o.user_id = v.to_owned();
                } else if let Some(v) = a.strip_prefix("--title=") {
                    o.title = v.to_owned();
                }
            }
        }
        i += 1;
    }
    o
}

fn write_atomic(path: &Path, content: &str) {
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    if fs::write(&tmp, content).is_ok() {
        let _ = fs::rename(&tmp, path);
    }
}

fn restore_to_front(hwnd: *mut c_void) {
    unsafe {
        if IsIconic(hwnd) != 0 {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        } else {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetWindowPos(hwnd, HWND_NOTOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
    }
}

fn main() -> ExitCode {
    let opts = parse_opts();
    let appdata = env::var_os("APPDATA")
        .map(PathBuf::from)
        .expect("APPDATA must be set to isolated sandbox");
    let zdata = appdata.join("ZaloData");
    fs::create_dir_all(&zdata).expect("create ZaloData");

    let lockfile = zdata.join("lockfile");
    let focus_sig = zdata.join(".focus-signal");
    let quit_sig = zdata.join(".quit-signal");
    let cmd_file = zdata.join(".gui-cmd");
    let ready_file = zdata.join("gui-ready.json");
    let state_file = zdata.join("gui-state.json");

    let _lock_guard = if !opts.no_lock {
        let lock_w = wide_nul(&lockfile.to_string_lossy());
        let handle = unsafe {
            CreateFileW(
                lock_w.as_ptr(),
                GENERIC_READ_WRITE,
                FILE_SHARE_READ,
                std::ptr::null(),
                CREATE_ALWAYS,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            let err = unsafe { GetLastError() };
            if err == ERROR_SHARING_VIOLATION {
                // Second instance handoff!
                write_atomic(
                    &focus_sig,
                    &format!("{{\"requester_pid\":{}}}", std::process::id()),
                );
                return ExitCode::from(2);
            }
            eprintln!("CreateFileW lockfile failed: {err}");
            return ExitCode::from(1);
        }
        let payload = format!(
            "{{\"pid\":{},\"app\":\"dim-test-fixture\",\"user_id\":\"{}\"}}\n",
            std::process::id(),
            opts.user_id
        );
        let mut written: u32 = 0;
        unsafe {
            let _ = WriteFile(
                handle,
                payload.as_ptr().cast(),
                payload.len() as u32,
                &mut written,
                std::ptr::null_mut(),
            );
            let _ = FlushFileBuffers(handle);
        }
        Some(LockFileGuard(handle, lockfile.clone()))
    } else {
        None
    };

    let _ = fs::remove_file(&focus_sig);
    let _ = fs::remove_file(&quit_sig);
    let _ = fs::remove_file(&cmd_file);

    let prod_dir = zdata.join("Database").join("_production").join(&opts.user_id);
    let _ = fs::create_dir_all(&prod_dir);
    let _ = fs::write(
        prod_dir.join("session.json"),
        format!("{{\"zalo_user_id\":\"{}\",\"pid\":{}}}\n", opts.user_id, std::process::id()),
    );

    let (hwnd_main, hwnd_popup, hwnd_owned) = unsafe {
        let h_inst = GetModuleHandleW(std::ptr::null());
        let cls_w = wide_nul(&opts.class_name);
        let wc = WNDCLASSEXW {
            cb_size: size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfn_wnd_proc: wnd_proc,
            cb_cls_extra: 0,
            cb_wnd_extra: 0,
            h_instance: h_inst,
            h_icon: std::ptr::null_mut(),
            h_cursor: std::ptr::null_mut(),
            hbr_background: std::ptr::null_mut(),
            lpsz_menu_name: std::ptr::null(),
            lpsz_class_name: cls_w.as_ptr(),
            h_icon_sm: std::ptr::null_mut(),
        };
        let _ = RegisterClassExW(&wc);

        let title_w = wide_nul(&opts.title);
        let hwnd_main = CreateWindowExW(
            0,
            cls_w.as_ptr(),
            title_w.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            opts.x,
            opts.y,
            opts.w,
            opts.h,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            h_inst,
            std::ptr::null_mut(),
        );
        assert!(!hwnd_main.is_null(), "CreateWindowExW main failed");

        let hwnd_popup = if opts.with_popup {
            let pop_title = wide_nul("Zalo Notification");
            let p = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                cls_w.as_ptr(),
                pop_title.as_ptr(),
                WS_POPUP | WS_VISIBLE,
                opts.x + 20,
                opts.y + 20,
                352,
                82,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                h_inst,
                std::ptr::null_mut(),
            );
            let _ = SetWindowPos(
                p,
                HWND_TOPMOST,
                opts.x + 20,
                opts.y + 20,
                352,
                82,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
            p
        } else {
            std::ptr::null_mut()
        };

        let hwnd_owned = if opts.with_owned {
            let owned_title = wide_nul("Zalo Settings Dialog");
            CreateWindowExW(
                0,
                cls_w.as_ptr(),
                owned_title.as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                opts.x + 40,
                opts.y + 40,
                800,
                600,
                hwnd_main,
                std::ptr::null_mut(),
                h_inst,
                std::ptr::null_mut(),
            )
        } else {
            std::ptr::null_mut()
        };

        (hwnd_main, hwnd_popup, hwnd_owned)
    };

    let mut handoff_count: u64 = 0;
    let write_state = |handoffs: u64| {
        let mut r: RECT = unsafe { zeroed() };
        let vis = unsafe { IsWindowVisible(hwnd_main) != 0 };
        let iconic = unsafe { IsIconic(hwnd_main) != 0 };
        unsafe {
            let _ = GetWindowRect(hwnd_main, &mut r);
        }
        let json = serde_json::json!({
            "pid": std::process::id(),
            "hwnd": hwnd_main as u64,
            "popup_hwnd": hwnd_popup as u64,
            "owned_hwnd": hwnd_owned as u64,
            "title": opts.title,
            "handoff_count": handoffs,
            "visible": vis,
            "iconic": iconic,
            "rect": [r.left, r.top, r.right, r.bottom],
        });
        write_atomic(&state_file, &json.to_string());
    };

    write_state(0);
    write_atomic(
        &ready_file,
        &serde_json::json!({
            "pid": std::process::id(),
            "hwnd": hwnd_main as u64,
            "popup_hwnd": hwnd_popup as u64,
            "owned_hwnd": hwnd_owned as u64,
            "title": opts.title,
        })
        .to_string(),
    );

    let start = Instant::now();
    let max_life = Duration::from_millis(opts.stay_ms);

    while start.elapsed() < max_life && !CLOSE_REQUESTED.load(Ordering::SeqCst) {
        unsafe {
            let mut msg: MSG = zeroed();
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                let _ = TranslateMessage(&msg);
                let _ = DispatchMessageW(&msg);
            }
        }

        if quit_sig.exists() {
            let _ = fs::remove_file(&quit_sig);
            break;
        }

        if cmd_file.exists() {
            if let Ok(raw) = fs::read_to_string(&cmd_file) {
                let _ = fs::remove_file(&cmd_file);
                match raw.trim() {
                    "minimize" => unsafe {
                        let _ = ShowWindow(hwnd_main, SW_MINIMIZE);
                    },
                    "maximize" => unsafe {
                        let _ = ShowWindow(hwnd_main, SW_MAXIMIZE);
                    },
                    "restore" => unsafe {
                        let _ = ShowWindow(hwnd_main, SW_RESTORE);
                    },
                    "hide" => unsafe {
                        let _ = ShowWindow(hwnd_main, SW_HIDE);
                    },
                    "show" => unsafe {
                        let _ = ShowWindow(hwnd_main, SW_SHOW);
                    },
                    "front" => restore_to_front(hwnd_main),
                    "quit" => break,
                    _ => {}
                }
                write_state(handoff_count);
            }
        }

        if focus_sig.exists() {
            let _ = fs::remove_file(&focus_sig);
            handoff_count += 1;
            restore_to_front(hwnd_main);
            write_state(handoff_count);
        }

        std::thread::sleep(Duration::from_millis(5));
    }

    unsafe {
        if !hwnd_popup.is_null() {
            let _ = DestroyWindow(hwnd_popup);
        }
        if !hwnd_owned.is_null() {
            let _ = DestroyWindow(hwnd_owned);
        }
        let _ = DestroyWindow(hwnd_main);
    }

    ExitCode::SUCCESS
}

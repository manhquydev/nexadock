//! Minimal, explicitly documented Win32 FFI bindings for Windows 10/11 isolation & monitoring.

#![cfg(windows)]
#![allow(non_camel_case_types, non_snake_case, dead_code)]

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::path::Path;

use dim_core::DimError;

const CREATE_UNICODE_ENVIRONMENT: u32 = 0x0000_0400;
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
const PROCESS_TERMINATE: u32 = 0x0001;
const PROCESS_VM_READ: u32 = 0x0010;
const SYNCHRONIZE: u32 = 0x0010_0000;
const STILL_ACTIVE: u32 = 259;
const ERROR_MORE_DATA: u32 = 234;
const ERROR_SHARING_VIOLATION: u32 = 32;
const WM_CLOSE: u32 = 0x0010;
const SW_RESTORE: i32 = 9;
const SW_SHOW: i32 = 5;
const KEY_READ: u32 = 0x20019;
const HKEY_CURRENT_USER: isize = 0x8000_0001_u32 as i32 as isize;
const GENERIC_READ_WRITE: u32 = 0xC000_0000;
const OPEN_EXISTING: u32 = 3;
const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;
const INVALID_HANDLE_VALUE: *mut c_void = (-1_isize) as *mut c_void;

#[repr(C)]
struct STARTUPINFOW {
    cb: u32,
    lp_reserved: *mut u16,
    lp_desktop: *mut u16,
    lp_title: *mut u16,
    dw_x: u32,
    dw_y: u32,
    dw_x_size: u32,
    dw_y_size: u32,
    dw_x_count_chars: u32,
    dw_y_count_chars: u32,
    dw_fill_attribute: u32,
    dw_flags: u32,
    w_show_window: u16,
    cb_reserved2: u16,
    lp_reserved2: *mut u8,
    h_std_input: *mut c_void,
    h_std_output: *mut c_void,
    h_std_error: *mut c_void,
}

#[repr(C)]
struct PROCESS_INFORMATION {
    h_process: *mut c_void,
    h_thread: *mut c_void,
    dw_process_id: u32,
    dw_thread_id: u32,
}

#[repr(C)]
struct PROCESSENTRY32W {
    dw_size: u32,
    cnt_usage: u32,
    th32_process_id: u32,
    th32_default_heap_id: usize,
    th32_module_id: u32,
    cnt_threads: u32,
    th32_parent_process_id: u32,
    pc_pri_class_base: i32,
    dw_flags: u32,
    sz_exe_file: [u16; 260],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct FILETIME {
    dw_low_date_time: u32,
    dw_high_date_time: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RM_UNIQUE_PROCESS {
    dw_process_id: u32,
    process_start_time: FILETIME,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RM_PROCESS_INFO {
    process: RM_UNIQUE_PROCESS,
    str_app_name: [u16; 256],
    str_service_short_name: [u16; 64],
    application_type: u32,
    app_status: u32,
    ts_session_id: u32,
    b_restartable: i32,
}

#[repr(C)]
struct PROCESS_MEMORY_COUNTERS {
    cb: u32,
    page_fault_count: u32,
    peak_working_set_size: usize,
    working_set_size: usize,
    quota_peak_paged_pool_usage: usize,
    quota_paged_pool_usage: usize,
    quota_peak_non_paged_pool_usage: usize,
    quota_non_paged_pool_usage: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateProcessW(
        lp_application_name: *const u16,
        lp_command_line: *mut u16,
        lp_process_attributes: *const c_void,
        lp_thread_attributes: *const c_void,
        b_inherit_handles: i32,
        dw_creation_flags: u32,
        lp_environment: *const c_void,
        lp_current_directory: *const u16,
        lp_startup_info: *const STARTUPINFOW,
        lp_process_information: *mut PROCESS_INFORMATION,
    ) -> i32;
    fn CloseHandle(h_object: *mut c_void) -> i32;
    fn GetLastError() -> u32;
    fn GetDiskFreeSpaceExW(
        lp_directory_name: *const u16,
        lp_free_bytes_available_to_caller: *mut u64,
        lp_total_number_of_bytes: *mut u64,
        lp_total_number_of_free_bytes: *mut u64,
    ) -> i32;
    fn CreateToolhelp32Snapshot(dw_flags: u32, th32_process_id: u32) -> *mut c_void;
    fn Process32FirstW(h_snapshot: *mut c_void, lppe: *mut PROCESSENTRY32W) -> i32;
    fn Process32NextW(h_snapshot: *mut c_void, lppe: *mut PROCESSENTRY32W) -> i32;
    fn OpenProcess(dw_desired_access: u32, b_inherit_handle: i32, dw_process_id: u32)
        -> *mut c_void;
    fn TerminateProcess(h_process: *mut c_void, u_exit_code: u32) -> i32;
    fn GetExitCodeProcess(h_process: *mut c_void, lp_exit_code: *mut u32) -> i32;
    fn WaitForSingleObject(h_handle: *mut c_void, dw_milliseconds: u32) -> u32;
    fn CreateFileW(
        lp_file_name: *const u16,
        dw_desired_access: u32,
        dw_share_mode: u32,
        lp_security_attributes: *const c_void,
        dw_creation_disposition: u32,
        dw_flags_and_attributes: u32,
        h_template_file: *mut c_void,
    ) -> *mut c_void;
}

#[link(name = "rstrtmgr")]
extern "system" {
    fn RmStartSession(
        p_session_handle: *mut u32,
        dw_session_flags: u32,
        str_session_key: *mut u16,
    ) -> u32;
    fn RmEndSession(dw_session_handle: u32) -> u32;
    fn RmRegisterResources(
        dw_session_handle: u32,
        n_files: u32,
        rgs_file_names: *const *const u16,
        n_applications: u32,
        rg_applications: *const c_void,
        n_services: u32,
        rgs_service_names: *const *const u16,
    ) -> u32;
    fn RmGetList(
        dw_session_handle: u32,
        pn_proc_info_needed: *mut u32,
        pn_proc_info: *mut u32,
        rg_affected_apps: *mut RM_PROCESS_INFO,
        lpdw_reboot_reasons: *mut u32,
    ) -> u32;
}

#[link(name = "advapi32")]
extern "system" {
    fn RegOpenKeyExW(
        h_key: isize,
        lp_sub_key: *const u16,
        ul_options: u32,
        sam_desired: u32,
        phk_result: *mut isize,
    ) -> i32;
    fn RegQueryValueExW(
        h_key: isize,
        lp_value_name: *const u16,
        lp_reserved: *mut u32,
        lp_type: *mut u32,
        lp_data: *mut u8,
        lpcb_data: *mut u32,
    ) -> i32;
    fn RegCloseKey(h_key: isize) -> i32;
}

#[link(name = "user32")]
extern "system" {
    fn EnumWindows(
        lp_enum_func: unsafe extern "system" fn(*mut c_void, isize) -> i32,
        l_param: isize,
    ) -> i32;
    fn IsWindowVisible(h_wnd: *mut c_void) -> i32;
    fn IsIconic(h_wnd: *mut c_void) -> i32;
    fn GetWindowThreadProcessId(h_wnd: *mut c_void, lpdw_process_id: *mut u32) -> u32;
    fn GetWindowTextLengthW(h_wnd: *mut c_void) -> i32;
    fn GetWindowTextW(h_wnd: *mut c_void, lp_string: *mut u16, n_max_count: i32) -> i32;
    fn ShowWindow(h_wnd: *mut c_void, n_cmd_show: i32) -> i32;
    fn SetForegroundWindow(h_wnd: *mut c_void) -> i32;
    fn PostMessageW(h_wnd: *mut c_void, msg: u32, w_param: usize, l_param: isize) -> i32;
}

#[link(name = "psapi")]
extern "system" {
    fn GetProcessMemoryInfo(
        process: *mut c_void,
        ppsmem_counters: *mut PROCESS_MEMORY_COUNTERS,
        cb: u32,
    ) -> i32;
}

struct HandleGuard(*mut c_void);

impl Drop for HandleGuard {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            // SAFETY: HandleGuard exclusively owns the valid Win32 kernel handle.
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

struct RmSessionGuard(u32);

impl Drop for RmSessionGuard {
    fn drop(&mut self) {
        // SAFETY: Session handle was opened by RmStartSession and has not been closed yet.
        unsafe {
            let _ = RmEndSession(self.0);
        }
    }
}

struct RegKeyGuard(isize);

impl Drop for RegKeyGuard {
    fn drop(&mut self) {
        if self.0 != 0 {
            // SAFETY: Key handle was returned by RegOpenKeyExW.
            unsafe {
                let _ = RegCloseKey(self.0);
            }
        }
    }
}

fn to_wide_null(s: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Spawns a detached child process using `CreateProcessW` with an isolated UTF-16 environment
/// block (`CREATE_UNICODE_ENVIRONMENT | CREATE_NEW_PROCESS_GROUP`) and `bInheritHandles = FALSE`.
pub fn spawn_process_isolated(
    exe_path: &Path,
    command_line: &str,
    work_dir: &Path,
    env_utf16_block: &[u16],
) -> Result<u32, DimError> {
    let app_w = to_wide_null(&exe_path.to_string_lossy());
    let mut cmd_w = to_wide_null(command_line);
    let cwd_w = to_wide_null(&work_dir.to_string_lossy());

    // SAFETY: All structs and null-terminated UTF-16 buffers outlive CreateProcessW;
    // bInheritHandles is 0 (FALSE) so parent handles cannot leak into the isolated child.
    unsafe {
        let mut si: STARTUPINFOW = zeroed();
        si.cb = size_of::<STARTUPINFOW>() as u32;
        let mut pi: PROCESS_INFORMATION = zeroed();

        let ok = CreateProcessW(
            app_w.as_ptr(),
            cmd_w.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            CREATE_UNICODE_ENVIRONMENT | CREATE_NEW_PROCESS_GROUP,
            env_utf16_block.as_ptr().cast(),
            cwd_w.as_ptr(),
            &si,
            &mut pi,
        );

        if ok == 0 {
            let err = GetLastError();
            return Err(DimError::ProcessError(format!(
                "CreateProcessW failed with Win32 error {err}"
            )));
        }

        let pid = pi.dw_process_id;
        let _thread_guard = HandleGuard(pi.h_thread);
        let _proc_guard = HandleGuard(pi.h_process);
        Ok(pid)
    }
}

/// Queries `rstrtmgr.dll` (`RmStartSession` -> `RmRegisterResources` -> `RmGetList`) for all PIDs
/// holding locks on `lockfile_path`.
pub fn get_locking_pids(lockfile_path: &Path) -> Vec<u32> {
    if !lockfile_path.exists() {
        return Vec::new();
    }

    let mut session_handle: u32 = 0;
    let mut session_key: [u16; 33] = [0; 33];
    // SAFETY: `session_key` has 33 UTF-16 code units (CCH_RM_SESSION_KEY + 1) and receives NUL-terminated output.
    let rc = unsafe { RmStartSession(&mut session_handle, 0, session_key.as_mut_ptr()) };
    if rc != 0 {
        return Vec::new();
    }
    let _guard = RmSessionGuard(session_handle);

    let file_w = to_wide_null(&lockfile_path.to_string_lossy());
    let file_ptrs = [file_w.as_ptr()];
    // SAFETY: `file_ptrs` points to a valid null-terminated UTF-16 path string for the duration of the call.
    let reg_rc = unsafe {
        RmRegisterResources(
            session_handle,
            1,
            file_ptrs.as_ptr(),
            0,
            std::ptr::null(),
            0,
            std::ptr::null(),
        )
    };
    if reg_rc != 0 {
        return Vec::new();
    }

    let mut needed: u32 = 0;
    let mut count: u32 = 0;
    let mut reboot_reasons: u32 = 0;
    // SAFETY: Initial size probe passes null buffer with count = 0 per Restart Manager API contract.
    let first_rc = unsafe {
        RmGetList(
            session_handle,
            &mut needed,
            &mut count,
            std::ptr::null_mut(),
            &mut reboot_reasons,
        )
    };

    if first_rc != ERROR_MORE_DATA || needed == 0 {
        return Vec::new();
    }

    let cap = (needed as usize).saturating_add(4);
    // SAFETY: `RM_PROCESS_INFO` is `#[repr(C)]` plain old data.
    let mut infos: Vec<RM_PROCESS_INFO> = vec![unsafe { zeroed() }; cap];
    count = cap as u32;
    // SAFETY: `infos` has capacity `count` valid elements.
    let second_rc = unsafe {
        RmGetList(
            session_handle,
            &mut needed,
            &mut count,
            infos.as_mut_ptr(),
            &mut reboot_reasons,
        )
    };
    if second_rc != 0 {
        return Vec::new();
    }

    let mut pids = Vec::new();
    for info in infos.into_iter().take(count as usize) {
        let pid = info.process.dw_process_id;
        if pid != 0 && !pids.contains(&pid) {
            pids.push(pid);
        }
    }
    pids
}

/// Checks whether a file is held with an exclusive share lock (`ERROR_SHARING_VIOLATION`).
pub fn is_file_exclusively_locked(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    let path_w = to_wide_null(&path.to_string_lossy());
    // SAFETY: `path_w` is null-terminated; returned handle is immediately wrapped in HandleGuard.
    unsafe {
        let handle = CreateFileW(
            path_w.as_ptr(),
            GENERIC_READ_WRITE,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        );
        if handle == INVALID_HANDLE_VALUE {
            return GetLastError() == ERROR_SHARING_VIOLATION;
        }
        let _guard = HandleGuard(handle);
        false
    }
}

/// Reads the raw unexpanded `REG_EXPAND_SZ` `AppData` entry from `User Shell Folders`.
pub fn read_raw_user_shell_appdata() -> Option<String> {
    let subkey_w = to_wide_null("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\User Shell Folders");
    let value_w = to_wide_null("AppData");
    let mut hkey: isize = 0;

    // SAFETY: `subkey_w` is null-terminated; `hkey` is guarded on success.
    let open_rc = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            subkey_w.as_ptr(),
            0,
            KEY_READ,
            &mut hkey,
        )
    };
    if open_rc != 0 {
        return None;
    }
    let _key_guard = RegKeyGuard(hkey);

    let mut reg_type: u32 = 0;
    let mut byte_len: u32 = 0;
    // SAFETY: Size probe passes null data buffer.
    let probe_rc = unsafe {
        RegQueryValueExW(
            hkey,
            value_w.as_ptr(),
            std::ptr::null_mut(),
            &mut reg_type,
            std::ptr::null_mut(),
            &mut byte_len,
        )
    };
    if probe_rc != 0 || byte_len == 0 || byte_len > 65_536 {
        return None;
    }

    let mut buf = vec![0u8; byte_len as usize];
    // SAFETY: `buf` matches `byte_len` bytes.
    let read_rc = unsafe {
        RegQueryValueExW(
            hkey,
            value_w.as_ptr(),
            std::ptr::null_mut(),
            &mut reg_type,
            buf.as_mut_ptr(),
            &mut byte_len,
        )
    };
    if read_rc != 0 {
        return None;
    }

    let words: Vec<u16> = buf[..byte_len as usize]
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .take_while(|&w| w != 0)
        .collect();
    Some(String::from_utf16_lossy(&words))
}

/// Queries free bytes available to the caller at `path`.
pub fn query_free_disk_bytes(path: &Path) -> Result<u64, DimError> {
    let probe_path = if path.exists() {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    let path_w = to_wide_null(&probe_path.to_string_lossy());
    let mut avail: u64 = 0;
    // SAFETY: `path_w` is null-terminated and `avail` is valid stack storage.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            path_w.as_ptr(),
            &mut avail,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        // SAFETY: Called immediately after Win32 failure on current thread.
        let err = unsafe { GetLastError() };
        return Err(DimError::Io(std::io::Error::from_raw_os_error(err as i32)));
    }
    Ok(avail)
}

/// Snapshot of process parent relationships and executable names.
pub struct ProcessSnapshot {
    children_by_parent: HashMap<u32, Vec<u32>>,
    exe_names: HashMap<u32, String>,
}

impl ProcessSnapshot {
    /// Captures a point-in-time Toolhelp32 process tree snapshot.
    pub fn capture() -> Self {
        let mut children_by_parent: HashMap<u32, Vec<u32>> = HashMap::new();
        let mut exe_names: HashMap<u32, String> = HashMap::new();

        // SAFETY: Toolhelp snapshot handle is wrapped in HandleGuard.
        let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snap == INVALID_HANDLE_VALUE || snap.is_null() {
            return Self {
                children_by_parent,
                exe_names,
            };
        }
        let _guard = HandleGuard(snap);

        // SAFETY: `PROCESSENTRY32W` is `#[repr(C)]` POD and `dw_size` is initialized before Process32FirstW.
        let mut entry: PROCESSENTRY32W = unsafe { zeroed() };
        entry.dw_size = size_of::<PROCESSENTRY32W>() as u32;

        // SAFETY: `snap` is valid and `entry.dw_size` is properly set.
        if unsafe { Process32FirstW(snap, &mut entry) } == 0 {
            return Self {
                children_by_parent,
                exe_names,
            };
        }

        loop {
            let pid = entry.th32_process_id;
            let ppid = entry.th32_parent_process_id;
            let len = entry
                .sz_exe_file
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.sz_exe_file.len());
            let name = String::from_utf16_lossy(&entry.sz_exe_file[..len]);
            exe_names.insert(pid, name);
            children_by_parent.entry(ppid).or_default().push(pid);

            // SAFETY: Iterates next process entry in snapshot.
            if unsafe { Process32NextW(snap, &mut entry) } == 0 {
                break;
            }
        }

        Self {
            children_by_parent,
            exe_names,
        }
    }

    /// Collects `root_pid` and all descendant PIDs in the process tree.
    pub fn collect_tree(&self, root_pid: u32) -> Vec<u32> {
        let mut visited = HashSet::new();
        let mut result = Vec::new();
        let mut stack = vec![root_pid];

        while let Some(pid) = stack.pop() {
            if !visited.insert(pid) {
                continue;
            }
            result.push(pid);
            if let Some(children) = self.children_by_parent.get(&pid) {
                stack.extend(children.iter().copied());
            }
        }
        result
    }

    /// Returns the executable filename recorded in the snapshot for `pid`.
    pub fn exe_name(&self, pid: u32) -> Option<&str> {
        self.exe_names.get(&pid).map(String::as_str)
    }
}

/// Checks if a PID is currently alive (`STILL_ACTIVE`).
pub fn is_pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    // SAFETY: Returned handle is wrapped in HandleGuard; query-only permission requested.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return false;
        }
        let _guard = HandleGuard(h);
        let mut code: u32 = 0;
        if GetExitCodeProcess(h, &mut code) == 0 {
            return false;
        }
        code == STILL_ACTIVE
    }
}

/// Sums working-set memory bytes across `pids`.
pub fn sum_working_set_bytes(pids: &[u32]) -> u64 {
    let mut total = 0u64;
    for &pid in pids {
        // SAFETY: Query/read handles are guarded and PROCESS_MEMORY_COUNTERS is initialized with cb.
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, pid);
            if h.is_null() {
                continue;
            }
            let _guard = HandleGuard(h);
            let mut counters: PROCESS_MEMORY_COUNTERS = zeroed();
            counters.cb = size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
            if GetProcessMemoryInfo(h, &mut counters, counters.cb) != 0 {
                total = total.saturating_add(counters.working_set_size as u64);
            }
        }
    }
    total
}

struct WindowSearchCtx {
    pids: HashSet<u32>,
    found_hwnd: *mut c_void,
    found_title: String,
}

unsafe extern "system" fn enum_windows_cb(hwnd: *mut c_void, lparam: isize) -> i32 {
    // SAFETY: `lparam` points to stack-pinned `WindowSearchCtx` throughout synchronous EnumWindows.
    let ctx = unsafe { &mut *(lparam as *mut WindowSearchCtx) };
    // SAFETY: `hwnd` is supplied by Windows EnumWindows.
    unsafe {
        if IsWindowVisible(hwnd) == 0 {
            return 1;
        }
        let mut pid: u32 = 0;
        let _ = GetWindowThreadProcessId(hwnd, &mut pid);
        if !ctx.pids.contains(&pid) {
            return 1;
        }
        let len = GetWindowTextLengthW(hwnd);
        if len > 0 && len < 4096 {
            let mut buf = vec![0u16; (len as usize) + 1];
            let copied = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
            if copied > 0 {
                ctx.found_title = String::from_utf16_lossy(&buf[..copied as usize]);
            }
        }
        ctx.found_hwnd = hwnd;
        0
    }
}

/// Finds the first visible top-level window belonging to any PID in `pids`.
pub fn find_main_window_for_pids(pids: &[u32]) -> Option<(u64, String)> {
    let mut ctx = WindowSearchCtx {
        pids: pids.iter().copied().collect(),
        found_hwnd: std::ptr::null_mut(),
        found_title: String::new(),
    };
    let ptr = (&mut ctx as *mut WindowSearchCtx) as isize;
    // SAFETY: `ctx` lives for the entire synchronous duration of `EnumWindows`.
    unsafe {
        let _ = EnumWindows(enum_windows_cb, ptr);
    }
    if ctx.found_hwnd.is_null() {
        None
    } else {
        Some((ctx.found_hwnd as u64, ctx.found_title))
    }
}

/// Restores and activates a visible window handle.
pub fn focus_window(hwnd_val: u64) {
    let hwnd = hwnd_val as *mut c_void;
    if hwnd.is_null() {
        return;
    }
    // SAFETY: Caller supplies a window handle previously returned by `EnumWindows`.
    unsafe {
        if IsIconic(hwnd) != 0 {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        } else {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
        let _ = SetForegroundWindow(hwnd);
    }
}

/// Posts `WM_CLOSE` to a window handle without blocking.
pub fn post_wm_close(hwnd_val: u64) {
    let hwnd = hwnd_val as *mut c_void;
    if hwnd.is_null() {
        return;
    }
    // SAFETY: Caller supplies a window handle previously returned by `EnumWindows`.
    unsafe {
        let _ = PostMessageW(hwnd, WM_CLOSE, 0, 0);
    }
}

/// Waits up to `timeout_ms` for `pid` to exit.
pub fn wait_for_exit(pid: u32, timeout_ms: u64) -> bool {
    // SAFETY: Handle is closed immediately via HandleGuard.
    unsafe {
        let h = OpenProcess(SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return true;
        }
        let _guard = HandleGuard(h);
        let wait_ms = u32::try_from(timeout_ms).unwrap_or(u32::MAX);
        let _ = WaitForSingleObject(h, wait_ms);
    }
    !is_pid_alive(pid)
}

/// Terminates a single non-primary process ID via `TerminateProcess`.
pub fn terminate_pid(pid: u32) -> bool {
    // SAFETY: Handle is closed immediately via HandleGuard; exit code 1 matches forced stop.
    unsafe {
        let h = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if h.is_null() {
            return false;
        }
        let _guard = HandleGuard(h);
        TerminateProcess(h, 1) != 0
    }
}

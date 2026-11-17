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
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
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
const GW_OWNER: u32 = 4;
const GWL_EXSTYLE: i32 = -20;
const WS_EX_TOOLWINDOW: isize = 0x0000_0080;
const MIN_MAIN_WINDOW_DIM: i32 = 400;
const KEY_READ: u32 = 0x20019;
const HKEY_CURRENT_USER: isize = 0x8000_0001_u32 as i32 as isize;
const HKEY_LOCAL_MACHINE: isize = 0x8000_0002_u32 as i32 as isize;
const GENERIC_READ_WRITE: u32 = 0xC000_0000;
const OPEN_EXISTING: u32 = 3;
const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;
const INVALID_HANDLE_VALUE: *mut c_void = (-1_isize) as *mut c_void;

const WTD_UI_NONE: u32 = 2;
const WTD_REVOKE_NONE: u32 = 0;
const WTD_CHOICE_FILE: u32 = 1;
const WTD_STATEACTION_VERIFY: u32 = 1;
const WTD_STATEACTION_CLOSE: u32 = 2;
const WTD_SAFER_FLAG: u32 = 0x0000_0100;

const CERT_QUERY_OBJECT_FILE: u32 = 1;
const CERT_QUERY_CONTENT_FLAG_PKCS7_SIGNED_EMBED: u32 = 1 << 10;
const CERT_QUERY_FORMAT_FLAG_BINARY: u32 = 1 << 1;
const X509_ASN_ENCODING: u32 = 0x0000_0001;
const PKCS_7_ASN_ENCODING: u32 = 0x0001_0000;
const CMSG_SIGNER_INFO_PARAM: u32 = 6;
const CERT_FIND_SUBJECT_CERT: u32 = 11 << 16;
const CERT_NAME_RDN_TYPE: u32 = 2;
const CERT_X500_NAME_STR: u32 = 3;
const CERT_NAME_STR_REVERSE_FLAG: u32 = 0x0200_0000;

const TOKEN_QUERY: u32 = 0x0008;
const TOKEN_ELEVATION_CLASS: u32 = 20;
const SE_FILE_OBJECT: u32 = 1;
const DACL_SECURITY_INFORMATION: u32 = 0x0000_0004;
const ACL_SIZE_INFORMATION_CLASS: u32 = 2;
const ACCESS_ALLOWED_ACE_TYPE: u8 = 0;
const WIN_WORLD_SID: u32 = 1;
const WIN_BUILTIN_USERS_SID: u32 = 27;
const SECURITY_MAX_SID_SIZE: usize = 68;
const WRITE_DISALLOWED_MASK: u32 = 0x0000_0002
    | 0x0000_0004
    | 0x0000_0010
    | 0x0000_0100
    | 0x0001_0000
    | 0x0004_0000
    | 0x0008_0000
    | 0x1000_0000
    | 0x4000_0000;

#[repr(C)]
#[allow(clippy::upper_case_acronyms)]
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
#[allow(clippy::upper_case_acronyms)]
struct RECT {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
#[allow(clippy::upper_case_acronyms)]
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

#[repr(C)]
#[derive(Clone, Copy)]
#[allow(clippy::upper_case_acronyms)]
struct GUID {
    data1: u32,
    data2: u16,
    data3: u16,
    data4: [u8; 8],
}

#[repr(C)]
struct WINTRUST_FILE_INFO {
    cb_struct: u32,
    pcwsz_file_path: *const u16,
    h_file: *mut c_void,
    pg_known_subject: *const GUID,
}

#[repr(C)]
struct WINTRUST_DATA {
    cb_struct: u32,
    p_policy_callback_data: *mut c_void,
    p_sip_client_data: *mut c_void,
    dw_ui_choice: u32,
    fdw_revocation_checks: u32,
    dw_union_choice: u32,
    p_file: *mut WINTRUST_FILE_INFO,
    dw_state_action: u32,
    h_wvt_state_data: *mut c_void,
    pwsz_url_reference: *const u16,
    dw_prov_flags: u32,
    dw_ui_context: u32,
    p_signature_settings: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CRYPTOAPI_BLOB {
    cb_data: u32,
    pb_data: *mut u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CRYPT_ALGORITHM_IDENTIFIER {
    psz_obj_id: *mut i8,
    parameters: CRYPTOAPI_BLOB,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CRYPT_BIT_BLOB {
    cb_data: u32,
    pb_data: *mut u8,
    c_unused_bits: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CERT_PUBLIC_KEY_INFO {
    algorithm: CRYPT_ALGORITHM_IDENTIFIER,
    public_key: CRYPT_BIT_BLOB,
}

#[repr(C)]
struct CERT_INFO {
    dw_version: u32,
    serial_number: CRYPTOAPI_BLOB,
    signature_algorithm: CRYPT_ALGORITHM_IDENTIFIER,
    issuer: CRYPTOAPI_BLOB,
    not_before: FILETIME,
    not_after: FILETIME,
    subject: CRYPTOAPI_BLOB,
    subject_public_key_info: CERT_PUBLIC_KEY_INFO,
    issuer_unique_id: CRYPT_BIT_BLOB,
    subject_unique_id: CRYPT_BIT_BLOB,
    c_extension: u32,
    rg_extension: *mut c_void,
}

#[repr(C)]
struct CMSG_SIGNER_INFO_HEAD {
    dw_version: u32,
    issuer: CRYPTOAPI_BLOB,
    serial_number: CRYPTOAPI_BLOB,
}

#[repr(C)]
struct TOKEN_ELEVATION {
    token_is_elevated: u32,
}

#[repr(C)]
struct ACL_SIZE_INFORMATION {
    ace_count: u32,
    acl_bytes_in_use: u32,
    acl_bytes_free: u32,
}

#[repr(C)]
struct ACE_HEADER {
    ace_type: u8,
    ace_flags: u8,
    ace_size: u16,
}

#[repr(C)]
struct ACCESS_ALLOWED_ACE {
    header: ACE_HEADER,
    mask: u32,
    sid_start: u32,
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
    fn OpenProcess(
        dw_desired_access: u32,
        b_inherit_handle: i32,
        dw_process_id: u32,
    ) -> *mut c_void;
    fn TerminateProcess(h_process: *mut c_void, u_exit_code: u32) -> i32;
    fn GetExitCodeProcess(h_process: *mut c_void, lp_exit_code: *mut u32) -> i32;
    fn GetProcessTimes(
        h_process: *mut c_void,
        lp_creation_time: *mut FILETIME,
        lp_exit_time: *mut FILETIME,
        lp_kernel_time: *mut FILETIME,
        lp_user_time: *mut FILETIME,
    ) -> i32;
    fn QueryFullProcessImageNameW(
        h_process: *mut c_void,
        dw_flags: u32,
        lp_exe_name: *mut u16,
        lpdw_size: *mut u32,
    ) -> i32;
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
    fn GetCurrentProcess() -> *mut c_void;
    fn LocalFree(h_mem: *mut c_void) -> *mut c_void;
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
    fn OpenProcessToken(
        process_handle: *mut c_void,
        desired_access: u32,
        token_handle: *mut *mut c_void,
    ) -> i32;
    fn GetTokenInformation(
        token_handle: *mut c_void,
        token_information_class: u32,
        token_information: *mut c_void,
        token_information_length: u32,
        return_length: *mut u32,
    ) -> i32;
    fn GetNamedSecurityInfoW(
        p_object_name: *const u16,
        object_type: u32,
        security_info: u32,
        ppsid_owner: *mut *mut c_void,
        ppsid_group: *mut *mut c_void,
        pp_dacl: *mut *mut c_void,
        pp_sacl: *mut *mut c_void,
        pp_security_descriptor: *mut *mut c_void,
    ) -> u32;
    fn GetAclInformation(
        p_acl: *mut c_void,
        p_acl_information: *mut c_void,
        n_acl_information_length: u32,
        dw_acl_information_class: u32,
    ) -> i32;
    fn GetAce(p_acl: *mut c_void, dw_ace_index: u32, p_ace: *mut *mut c_void) -> i32;
    fn CreateWellKnownSid(
        well_known_sid_type: u32,
        domain_sid: *mut c_void,
        p_sid: *mut c_void,
        cb_sid: *mut u32,
    ) -> i32;
    fn EqualSid(p_sid1: *const c_void, p_sid2: *const c_void) -> i32;
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
    fn GetClassNameW(h_wnd: *mut c_void, lp_class_name: *mut u16, n_max_count: i32) -> i32;
    fn GetWindow(h_wnd: *mut c_void, u_cmd: u32) -> *mut c_void;
    #[cfg(target_pointer_width = "64")]
    fn GetWindowLongPtrW(h_wnd: *mut c_void, n_index: i32) -> isize;
    #[cfg(not(target_pointer_width = "64"))]
    fn GetWindowLongW(h_wnd: *mut c_void, n_index: i32) -> i32;
    fn GetWindowRect(h_wnd: *mut c_void, lp_rect: *mut RECT) -> i32;
    fn GetWindowTextLengthW(h_wnd: *mut c_void) -> i32;
    fn GetWindowTextW(h_wnd: *mut c_void, lp_string: *mut u16, n_max_count: i32) -> i32;
    fn ShowWindow(h_wnd: *mut c_void, n_cmd_show: i32) -> i32;
    fn SetForegroundWindow(h_wnd: *mut c_void) -> i32;
    fn GetForegroundWindow() -> *mut c_void;
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

#[link(name = "wintrust")]
extern "system" {
    fn WinVerifyTrust(
        hwnd: *mut c_void,
        pg_action_id: *const GUID,
        p_wvt_data: *mut WINTRUST_DATA,
    ) -> i32;
}

#[link(name = "crypt32")]
extern "system" {
    fn CryptQueryObject(
        dw_object_type: u32,
        pv_object: *const c_void,
        dw_expected_content_type_flags: u32,
        dw_expected_format_type_flags: u32,
        dw_flags: u32,
        pdw_msg_and_cert_encoding_type: *mut u32,
        pdw_content_type: *mut u32,
        pdw_format_type: *mut u32,
        ph_cert_store: *mut *mut c_void,
        ph_msg: *mut *mut c_void,
        ppv_context: *mut *const c_void,
    ) -> i32;
    fn CryptMsgGetParam(
        h_crypt_msg: *mut c_void,
        dw_param_type: u32,
        dw_index: u32,
        pv_data: *mut c_void,
        pcb_data: *mut u32,
    ) -> i32;
    fn CryptMsgClose(h_crypt_msg: *mut c_void) -> i32;
    fn CertFindCertificateInStore(
        h_cert_store: *mut c_void,
        dw_cert_encoding_type: u32,
        dw_find_flags: u32,
        dw_find_type: u32,
        pv_find_para: *const c_void,
        p_prev_cert_context: *const c_void,
    ) -> *const c_void;
    fn CertGetNameStringW(
        p_cert_context: *const c_void,
        dw_type: u32,
        dw_flags: u32,
        pv_type_para: *mut c_void,
        psz_name_string: *mut u16,
        cch_name_string: u32,
    ) -> u32;
    fn CertFreeCertificateContext(p_cert_context: *const c_void) -> i32;
    fn CertCloseStore(h_cert_store: *mut c_void, dw_flags: u32) -> i32;
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

/// Verified kernel identity of a newly spawned Windows process.
#[derive(Debug, Clone)]
pub struct SpawnedProcessIdentity {
    /// OS process ID.
    pub pid: u32,
    /// `GetProcessTimes` creation timestamp in 100ns FILETIME ticks.
    pub create_time_100ns: u64,
    /// Full executable image path queried directly from `pi.h_process`.
    pub executable_path: String,
}

/// Spawns a detached child process using `CreateProcessW` with an isolated UTF-16 environment
/// block (`CREATE_UNICODE_ENVIRONMENT | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW`) and
/// `bInheritHandles = FALSE`, capturing `lpCreationTime` and full image path directly from
/// `pi.h_process` before closing handles so PID-reuse checks are race-free.
pub fn spawn_process_isolated_with_identity(
    exe_path: &Path,
    command_line: &str,
    work_dir: &Path,
    env_utf16_block: &[u16],
) -> Result<SpawnedProcessIdentity, DimError> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    let app_w = to_wide_null(&exe_path.to_string_lossy());
    let mut cmd_w = to_wide_null(command_line);
    let cwd_w = to_wide_null(&work_dir.to_string_lossy());

    // SAFETY: All structs and null-terminated UTF-16 buffers outlive CreateProcessW;
    // bInheritHandles is 0 (FALSE) and CREATE_NO_WINDOW prevents console handle sharing.
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
            CREATE_UNICODE_ENVIRONMENT | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW,
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
        let proc_guard = HandleGuard(pi.h_process);

        let mut creation: FILETIME = zeroed();
        let mut exit_t: FILETIME = zeroed();
        let mut kernel_t: FILETIME = zeroed();
        let mut user_t: FILETIME = zeroed();
        let create_time_100ns = if GetProcessTimes(
            proc_guard.0,
            &mut creation,
            &mut exit_t,
            &mut kernel_t,
            &mut user_t,
        ) != 0
        {
            (u64::from(creation.dw_high_date_time) << 32) | u64::from(creation.dw_low_date_time)
        } else {
            0
        };

        let mut img_buf = vec![0u16; 2048];
        let mut img_len = img_buf.len() as u32;
        let executable_path =
            if QueryFullProcessImageNameW(proc_guard.0, 0, img_buf.as_mut_ptr(), &mut img_len) != 0
                && img_len > 0
            {
                OsString::from_wide(&img_buf[..img_len as usize])
                    .to_string_lossy()
                    .into_owned()
            } else {
                exe_path.display().to_string()
            };

        Ok(SpawnedProcessIdentity {
            pid,
            create_time_100ns,
            executable_path,
        })
    }
}

/// Spawns a detached child process using `CreateProcessW` with an isolated UTF-16 environment
/// block (`CREATE_UNICODE_ENVIRONMENT | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW`) and
/// `bInheritHandles = FALSE` so console children do not inherit parent stdio pipes.
pub fn spawn_process_isolated(
    exe_path: &Path,
    command_line: &str,
    work_dir: &Path,
    env_utf16_block: &[u16],
) -> Result<u32, DimError> {
    spawn_process_isolated_with_identity(exe_path, command_line, work_dir, env_utf16_block)
        .map(|s| s.pid)
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
    let subkey_w =
        to_wide_null("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\User Shell Folders");
    let value_w = to_wide_null("AppData");
    let mut hkey: isize = 0;

    // SAFETY: `subkey_w` is null-terminated; `hkey` is guarded on success.
    let open_rc =
        unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, subkey_w.as_ptr(), 0, KEY_READ, &mut hkey) };
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

/// Classification of a visible top-level window owned by a Zalo / fixture process (`IR-WIN-02`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZaloWindowRole {
    /// Primary Electron chat window (`Chrome_WidgetWin_1` or `DimTestMainWindow`, unowned,
    /// non-toolwindow, non-empty title, `>= 400x400`).
    MainChatWindow,
    /// In-app floating notification banner (`notiWin` `352x82` / `WS_EX_TOOLWINDOW`) — NEVER focus or close!
    NotificationPopup,
    /// Owned dialog, splash screen, empty-title overlay, or sub-400px helper surface.
    AuxiliaryOrSplash,
}

#[inline]
unsafe fn get_window_ex_style(hwnd: *mut c_void) -> isize {
    #[cfg(target_pointer_width = "64")]
    {
        unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) }
    }
    #[cfg(not(target_pointer_width = "64"))]
    {
        (unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) }) as isize
    }
}

fn is_supported_main_window_class(class_name: &str) -> bool {
    class_name == "Chrome_WidgetWin_1" || class_name == "DimTestMainWindow"
}

/// Inspects a Win32 window handle and classifies its role if currently visible (`IR-WIN-01`, `IR-WIN-02`).
pub fn classify_window_for_pid(hwnd_val: u64) -> Option<(u32, ZaloWindowRole, String)> {
    let hwnd = hwnd_val as *mut c_void;
    if hwnd.is_null() {
        return None;
    }
    // SAFETY: `hwnd` is queried via read-only Win32 user32 inspection functions with bounded stack buffers.
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
        let ex_style = get_window_ex_style(hwnd);
        let is_toolwindow = (ex_style & WS_EX_TOOLWINDOW) != 0;

        let mut rect: RECT = zeroed();
        let has_rect = GetWindowRect(hwnd, &mut rect) != 0;
        let width = if has_rect { rect.right - rect.left } else { 0 };
        let height = if has_rect { rect.bottom - rect.top } else { 0 };

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
            return Some((pid, ZaloWindowRole::NotificationPopup, title));
        }

        if is_supported_main_window_class(&class_name)
            && owner.is_null()
            && !is_toolwindow
            && !title.trim().is_empty()
            && width >= MIN_MAIN_WINDOW_DIM
            && height >= MIN_MAIN_WINDOW_DIM
        {
            return Some((pid, ZaloWindowRole::MainChatWindow, title));
        }

        Some((pid, ZaloWindowRole::AuxiliaryOrSplash, title))
    }
}

struct WindowSearchCtx {
    main_pid: u32,
    pids: HashSet<u32>,
    found_hwnd: *mut c_void,
    found_title: String,
}

unsafe extern "system" fn enum_windows_cb(hwnd: *mut c_void, lparam: isize) -> i32 {
    // SAFETY: `lparam` points to stack-pinned `WindowSearchCtx` throughout synchronous EnumWindows.
    let ctx = unsafe { &mut *(lparam as *mut WindowSearchCtx) };
    let Some((pid, role, title)) = classify_window_for_pid(hwnd as u64) else {
        return 1;
    };
    if !ctx.pids.contains(&pid) || role != ZaloWindowRole::MainChatWindow {
        return 1;
    }
    if pid == ctx.main_pid {
        ctx.found_hwnd = hwnd;
        ctx.found_title = title;
        return 0;
    }
    if ctx.found_hwnd.is_null() {
        ctx.found_hwnd = hwnd;
        ctx.found_title = title;
    }
    1
}

/// Finds the verified main chat window for an isolated profile (`IR-WIN-01` 6-condition filter),
/// preferring `main_pid` and excluding `notiWin` popups (`352x82`), owned dialogs, and toolwindows.
pub fn find_main_window_for_profile(main_pid: u32, tree_pids: &[u32]) -> Option<(u64, String)> {
    let mut pids: HashSet<u32> = tree_pids.iter().copied().collect();
    if main_pid > 0 {
        pids.insert(main_pid);
    }
    if pids.is_empty() {
        return None;
    }
    let mut ctx = WindowSearchCtx {
        main_pid,
        pids,
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

/// Finds the verified main window belonging to `pids` (preferring `pids[0]` as `main_pid`).
pub fn find_main_window_for_pids(pids: &[u32]) -> Option<(u64, String)> {
    let main_pid = pids.first().copied().unwrap_or(0);
    find_main_window_for_profile(main_pid, pids)
}

/// Restores and activates a visible window handle, returning `true` ONLY when `GetForegroundWindow()`
/// confirms foreground ownership (`IR-WIN-01`). When Win32 Foreground Lock blocks activation,
/// returns `false` so callers fall back to isolated `second-instance` handoff cleanly.
pub fn focus_window(hwnd_val: u64) -> bool {
    let hwnd = hwnd_val as *mut c_void;
    if hwnd.is_null() {
        return false;
    }
    // SAFETY: Caller supplies a verified main-window handle returned by `EnumWindows`.
    unsafe {
        if IsIconic(hwnd) != 0 {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        } else {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
        let _ = SetForegroundWindow(hwnd);
        if GetForegroundWindow() == hwnd {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(35));
        GetForegroundWindow() == hwnd
    }
}

/// Passively checks whether global Zalo call named pipes (`PipeZCallRecv` / `PipeZCallSend`) are
/// currently listed in `\\.\pipe\` via directory enumeration ONLY (zero client stream connection!).
pub fn passive_zcall_pipes_present() -> (bool, bool) {
    let mut recv = false;
    let mut send = false;
    if let Ok(entries) = std::fs::read_dir(r"\\.\pipe\") {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.eq_ignore_ascii_case("PipeZCallRecv") {
                    recv = true;
                } else if name.eq_ignore_ascii_case("PipeZCallSend") {
                    send = true;
                }
            }
        }
    }
    (recv, send)
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

/// Cryptographic Authenticode trust verification outcome for a Windows PE binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthenticodeTrustOutcome {
    /// Binary passed `WinVerifyTrust` chain validation and matched any required publisher substring.
    Trusted {
        /// Extracted signer X.509 RDN subject string.
        subject: Option<String>,
        /// Human-readable status label (`"Signed (VNG)"` or `"Signed"`).
        status_label: String,
    },
    /// File does not exist on disk.
    Missing,
    /// PE file contains no embedded Authenticode signature table.
    Unsigned,
    /// `WinVerifyTrust` rejected the signature, chain, or modified payload hash.
    UntrustedOrTampered {
        /// Raw `WinVerifyTrust` HRESULT/status code.
        win_trust_code: u32,
        /// Extracted signer subject if readable.
        subject: Option<String>,
        /// Human-readable diagnostic label.
        status_label: String,
    },
    /// Signature is cryptographically valid, but signer subject did not match the expected publisher.
    WrongPublisher {
        /// Actual signer X.509 RDN subject string.
        subject: String,
        /// Human-readable diagnostic label.
        status_label: String,
    },
    /// Signature verification could not complete (e.g. I/O or CryptoAPI failure).
    VerificationError {
        /// Diagnostic error summary.
        detail: String,
    },
}

struct CertStoreGuard(*mut c_void);

impl Drop for CertStoreGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: Handle returned by CryptQueryObject.
            unsafe {
                let _ = CertCloseStore(self.0, 0);
            }
        }
    }
}

struct CryptMsgGuard(*mut c_void);

impl Drop for CryptMsgGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: Handle returned by CryptQueryObject.
            unsafe {
                let _ = CryptMsgClose(self.0);
            }
        }
    }
}

struct CertContextGuard(*const c_void);

impl Drop for CertContextGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: Context returned by CertFindCertificateInStore.
            unsafe {
                let _ = CertFreeCertificateContext(self.0);
            }
        }
    }
}

struct LocalMemGuard(*mut c_void);

impl Drop for LocalMemGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: Security descriptor allocated by GetNamedSecurityInfoW.
            unsafe {
                let _ = LocalFree(self.0);
            }
        }
    }
}

/// Verifies a PE file's Authenticode signature using Windows `WinVerifyTrust`
/// (`WINTRUST_ACTION_GENERIC_VERIFY_V2`) and extracts the signer certificate subject via CryptoAPI.
pub fn verify_authenticode_signature(
    path: &Path,
    required_publisher_substr: Option<&str>,
) -> AuthenticodeTrustOutcome {
    if !path.is_file() {
        return AuthenticodeTrustOutcome::Missing;
    }
    if !crate::layout::has_pe_authenticode_table(path) {
        return AuthenticodeTrustOutcome::Unsigned;
    }

    let path_w = to_wide_null(&path.to_string_lossy());
    let signer_subject = extract_authenticode_signer_subject(&path_w);

    let action_id = GUID {
        data1: 0x00AA_C56B,
        data2: 0xCD44,
        data3: 0x11D0,
        data4: [0x8C, 0xC2, 0x00, 0xC0, 0x4F, 0xC2, 0x95, 0xEE],
    };

    let mut file_info = WINTRUST_FILE_INFO {
        cb_struct: size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwsz_file_path: path_w.as_ptr(),
        h_file: std::ptr::null_mut(),
        pg_known_subject: std::ptr::null(),
    };

    // SAFETY: `file_info` and `path_w` live for the entire `WinVerifyTrust` verify/close pair.
    let status = unsafe {
        let mut wvt_data: WINTRUST_DATA = zeroed();
        wvt_data.cb_struct = size_of::<WINTRUST_DATA>() as u32;
        wvt_data.dw_ui_choice = WTD_UI_NONE;
        wvt_data.fdw_revocation_checks = WTD_REVOKE_NONE;
        wvt_data.dw_union_choice = WTD_CHOICE_FILE;
        wvt_data.p_file = &mut file_info;
        wvt_data.dw_state_action = WTD_STATEACTION_VERIFY;
        wvt_data.dw_prov_flags = WTD_SAFER_FLAG;

        let verify_rc = WinVerifyTrust(INVALID_HANDLE_VALUE, &action_id, &mut wvt_data);

        wvt_data.dw_state_action = WTD_STATEACTION_CLOSE;
        let _ = WinVerifyTrust(INVALID_HANDLE_VALUE, &action_id, &mut wvt_data);

        verify_rc
    };

    if status != 0 {
        let code = status as u32;
        return AuthenticodeTrustOutcome::UntrustedOrTampered {
            win_trust_code: code,
            subject: signer_subject,
            status_label: format!("Unverified (WinVerifyTrust=0x{code:08X})"),
        };
    }

    if let Some(expected) = required_publisher_substr {
        let matched = signer_subject
            .as_deref()
            .is_some_and(|s| subject_matches_publisher(s, expected));
        if !matched {
            let actual = signer_subject.unwrap_or_else(|| "<missing-subject>".to_owned());
            return AuthenticodeTrustOutcome::WrongPublisher {
                status_label: format!("Signed (UntrustedPublisher: {actual})"),
                subject: actual,
            };
        }
        return AuthenticodeTrustOutcome::Trusted {
            subject: signer_subject,
            status_label: format!("Signed ({expected})"),
        };
    }

    AuthenticodeTrustOutcome::Trusted {
        subject: signer_subject,
        status_label: "Signed".to_owned(),
    }
}

pub(crate) fn parse_x500_rdn_attributes(subject: &str) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    for ch in subject.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                cur.push(ch);
            }
            ',' | ';' if !in_quotes => {
                if let Some((k, v)) = cur.trim().split_once('=') {
                    let clean_v = v.trim().trim_matches('"').trim();
                    attrs.push((k.trim().to_ascii_uppercase(), clean_v.to_owned()));
                }
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    if let Some((k, v)) = cur.trim().split_once('=') {
        let clean_v = v.trim().trim_matches('"').trim();
        attrs.push((k.trim().to_ascii_uppercase(), clean_v.to_owned()));
    }
    attrs
}

pub(crate) fn subject_matches_publisher(subject: &str, expected: &str) -> bool {
    let expected_trimmed = expected.trim();
    if expected_trimmed.is_empty() {
        return false;
    }
    let attrs = parse_x500_rdn_attributes(subject);
    if expected_trimmed.eq_ignore_ascii_case("VNG")
        || expected_trimmed.eq_ignore_ascii_case("VNG GROUP JSC")
    {
        return attrs
            .iter()
            .any(|(k, v)| (k == "CN" || k == "O") && v.eq_ignore_ascii_case("VNG GROUP JSC"));
    }
    attrs
        .iter()
        .any(|(k, v)| (k == "CN" || k == "O") && v.eq_ignore_ascii_case(expected_trimmed))
        || subject.eq_ignore_ascii_case(expected_trimmed)
}

fn extract_authenticode_signer_subject(path_w: &[u16]) -> Option<String> {
    let mut encoding: u32 = 0;
    let mut content_type: u32 = 0;
    let mut format_type: u32 = 0;
    let mut h_store: *mut c_void = std::ptr::null_mut();
    let mut h_msg: *mut c_void = std::ptr::null_mut();

    // SAFETY: `path_w` is null-terminated UTF-16; returned handles are wrapped in RAII guards.
    let query_ok = unsafe {
        CryptQueryObject(
            CERT_QUERY_OBJECT_FILE,
            path_w.as_ptr().cast(),
            CERT_QUERY_CONTENT_FLAG_PKCS7_SIGNED_EMBED,
            CERT_QUERY_FORMAT_FLAG_BINARY,
            0,
            &mut encoding,
            &mut content_type,
            &mut format_type,
            &mut h_store,
            &mut h_msg,
            std::ptr::null_mut(),
        )
    };
    if query_ok == 0 || h_store.is_null() || h_msg.is_null() {
        return None;
    }
    let _store_guard = CertStoreGuard(h_store);
    let _msg_guard = CryptMsgGuard(h_msg);

    let mut signer_len: u32 = 0;
    // SAFETY: Size probe for CMSG_SIGNER_INFO.
    if unsafe {
        CryptMsgGetParam(
            h_msg,
            CMSG_SIGNER_INFO_PARAM,
            0,
            std::ptr::null_mut(),
            &mut signer_len,
        )
    } == 0
        || (signer_len as usize) < size_of::<CMSG_SIGNER_INFO_HEAD>()
    {
        return None;
    }

    let mut buf = vec![0u8; signer_len as usize];
    // SAFETY: `buf` has `signer_len` bytes allocated.
    if unsafe {
        CryptMsgGetParam(
            h_msg,
            CMSG_SIGNER_INFO_PARAM,
            0,
            buf.as_mut_ptr().cast(),
            &mut signer_len,
        )
    } == 0
    {
        return None;
    }

    // SAFETY: `buf` is at least `size_of::<CMSG_SIGNER_INFO_HEAD>()` bytes and populated by CryptMsgGetParam.
    let head = unsafe { std::ptr::read_unaligned(buf.as_ptr().cast::<CMSG_SIGNER_INFO_HEAD>()) };
    // SAFETY: `CERT_INFO` is POD zeroed with valid Issuer/SerialNumber pointers pointing into `buf` (alive for this function).
    let mut cert_info: CERT_INFO = unsafe { zeroed() };
    cert_info.issuer = head.issuer;
    cert_info.serial_number = head.serial_number;

    // SAFETY: Searches `h_store` for the exact signing certificate matching Issuer + SerialNumber.
    let cert_ctx = unsafe {
        CertFindCertificateInStore(
            h_store,
            X509_ASN_ENCODING | PKCS_7_ASN_ENCODING,
            0,
            CERT_FIND_SUBJECT_CERT,
            (&cert_info as *const CERT_INFO).cast(),
            std::ptr::null(),
        )
    };
    if cert_ctx.is_null() {
        return None;
    }
    let _cert_guard = CertContextGuard(cert_ctx);

    let mut rdn_buf = [0u16; 1024];
    let mut str_type: u32 = CERT_X500_NAME_STR | CERT_NAME_STR_REVERSE_FLAG;
    // SAFETY: `cert_ctx` is a valid PCCERT_CONTEXT, `pv_type_para` points to initialized `str_type` per CryptoAPI contract, and `rdn_buf` holds up to 1024 UTF-16 code units.
    let copied = unsafe {
        CertGetNameStringW(
            cert_ctx,
            CERT_NAME_RDN_TYPE,
            0,
            (&mut str_type as *mut u32).cast(),
            rdn_buf.as_mut_ptr(),
            rdn_buf.len() as u32,
        )
    };
    if copied <= 1 {
        return None;
    }
    let len = (copied as usize).saturating_sub(1).min(rdn_buf.len());
    Some(String::from_utf16_lossy(&rdn_buf[..len]))
}

/// Queries the current Windows process token to determine elevation (`TokenElevation`).
pub fn is_current_process_elevated() -> Option<bool> {
    let mut token: *mut c_void = std::ptr::null_mut();
    // SAFETY: `GetCurrentProcess()` returns a pseudo-handle; `token` is guarded by HandleGuard.
    unsafe {
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 || token.is_null() {
            return None;
        }
        let _guard = HandleGuard(token);
        let mut elevation = TOKEN_ELEVATION {
            token_is_elevated: 0,
        };
        let mut ret_len: u32 = 0;
        let ok = GetTokenInformation(
            token,
            TOKEN_ELEVATION_CLASS,
            (&mut elevation as *mut TOKEN_ELEVATION).cast(),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret_len,
        );
        if ok == 0 {
            return None;
        }
        Some(elevation.token_is_elevated != 0)
    }
}

/// Reads `HKLM\SYSTEM\CurrentControlSet\Control\CI\Policy\VerifiedAndReputablePolicyState` (read-only).
pub fn read_smart_app_control_state() -> Option<u32> {
    let subkey_w = to_wide_null("SYSTEM\\CurrentControlSet\\Control\\CI\\Policy");
    let value_w = to_wide_null("VerifiedAndReputablePolicyState");
    let mut hkey: isize = 0;

    // SAFETY: `subkey_w` is null-terminated; `hkey` is closed via RegKeyGuard.
    let open_rc = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
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
    let mut dword_buf = [0u8; 4];
    let mut byte_len: u32 = 4;
    // SAFETY: Queries a 4-byte REG_DWORD into `dword_buf`.
    let rc = unsafe {
        RegQueryValueExW(
            hkey,
            value_w.as_ptr(),
            std::ptr::null_mut(),
            &mut reg_type,
            dword_buf.as_mut_ptr(),
            &mut byte_len,
        )
    };
    if rc != 0 || byte_len < 4 {
        return None;
    }
    Some(u32::from_le_bytes(dword_buf))
}

/// Checks whether the NTFS DACL on `path` forbids write/modify access by `Everyone` or `BUILTIN\Users`.
pub fn check_directory_acl_safe(path: &Path) -> Option<bool> {
    let probe = if path.exists() { path } else { path.parent()? };
    if !probe.exists() {
        return None;
    }
    let path_w = to_wide_null(&probe.to_string_lossy());
    let mut p_dacl: *mut c_void = std::ptr::null_mut();
    let mut p_sd: *mut c_void = std::ptr::null_mut();

    // SAFETY: `p_sd` is freed via LocalMemGuard; `p_dacl` points inside `p_sd`.
    let rc = unsafe {
        GetNamedSecurityInfoW(
            path_w.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut p_dacl,
            std::ptr::null_mut(),
            &mut p_sd,
        )
    };
    if rc != 0 || p_sd.is_null() {
        return None;
    }
    let _sd_guard = LocalMemGuard(p_sd);

    if p_dacl.is_null() {
        // Null DACL grants Everyone full access.
        return Some(false);
    }

    let mut world_sid = [0u8; SECURITY_MAX_SID_SIZE];
    let mut world_size = SECURITY_MAX_SID_SIZE as u32;
    let mut users_sid = [0u8; SECURITY_MAX_SID_SIZE];
    let mut users_size = SECURITY_MAX_SID_SIZE as u32;

    // SAFETY: Buffers sized to SECURITY_MAX_SID_SIZE per Win32 CreateWellKnownSid contract.
    unsafe {
        if CreateWellKnownSid(
            WIN_WORLD_SID,
            std::ptr::null_mut(),
            world_sid.as_mut_ptr().cast(),
            &mut world_size,
        ) == 0
            || CreateWellKnownSid(
                WIN_BUILTIN_USERS_SID,
                std::ptr::null_mut(),
                users_sid.as_mut_ptr().cast(),
                &mut users_size,
            ) == 0
        {
            return None;
        }
    }

    // SAFETY: `ACL_SIZE_INFORMATION` is POD; `p_dacl` is valid inside `p_sd`.
    let mut acl_info: ACL_SIZE_INFORMATION = unsafe { zeroed() };
    // SAFETY: Queries ACE count for `p_dacl`.
    if unsafe {
        GetAclInformation(
            p_dacl,
            (&mut acl_info as *mut ACL_SIZE_INFORMATION).cast(),
            size_of::<ACL_SIZE_INFORMATION>() as u32,
            ACL_SIZE_INFORMATION_CLASS,
        )
    } == 0
    {
        return None;
    }

    for idx in 0..acl_info.ace_count {
        let mut p_ace: *mut c_void = std::ptr::null_mut();
        // SAFETY: `idx < acl_info.ace_count`, `p_ace` points into `p_dacl`.
        if unsafe { GetAce(p_dacl, idx, &mut p_ace) } == 0 || p_ace.is_null() {
            continue;
        }
        // SAFETY: Every ACE starts with `ACE_HEADER`.
        let hdr = unsafe { &*(p_ace.cast::<ACE_HEADER>()) };
        if hdr.ace_type != ACCESS_ALLOWED_ACE_TYPE
            || (hdr.ace_size as usize) < size_of::<ACCESS_ALLOWED_ACE>()
        {
            continue;
        }
        // SAFETY: Verified `ace_type == ACCESS_ALLOWED_ACE_TYPE` and size >= ACCESS_ALLOWED_ACE.
        let ace = unsafe { &*(p_ace.cast::<ACCESS_ALLOWED_ACE>()) };
        // SAFETY: `sid_start` is at offset 8 inside ACCESS_ALLOWED_ACE, inside the `p_dacl` buffer.
        let ace_sid = unsafe { (p_ace.cast::<u8>()).add(8).cast::<c_void>() };
        // SAFETY: Compares well-known SIDs constructed above against ACE SID.
        let is_broad_principal = unsafe {
            EqualSid(ace_sid, world_sid.as_ptr().cast()) != 0
                || EqualSid(ace_sid, users_sid.as_ptr().cast()) != 0
        };
        if is_broad_principal && (ace.mask & WRITE_DISALLOWED_MASK) != 0 {
            return Some(false);
        }
    }

    Some(true)
}

//! STRICTLY READ-ONLY inspector for live Zalo.exe / ZaloCall.exe / ZaloCap.exe window architecture.
//! SAFETY GUARANTEE: Calls ONLY read-only Win32 enumeration/query APIs. Never sends messages or
//! mutates any window, process, or file.

#[path = "../win32.rs"]
mod win32;

use std::collections::{BTreeMap, HashMap};
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use win32::*;

#[derive(Debug, Clone)]
struct ProcInfo {
    pid: u32,
    ppid: u32,
    exe_name: String,
    full_path: String,
    integrity: String,
}

#[derive(Debug, Clone)]
struct WindowRecord {
    hwnd: usize,
    pid: u32,
    tid: u32,
    class_name: String,
    title: String,
    style: u32,
    ex_style: u32,
    visible: bool,
    iconic: bool,
    zoomed: bool,
    enabled: bool,
    rect: RECT,
    client_rect: RECT,
    parent: usize,
    owner: usize,
    root: usize,
    root_owner: usize,
    dpi_context: String,
    dpi_value: u32,
    dpi_hosting: i32,
    dwm_cloaked: u32,
    dwm_nc_rendering: BOOL,
    dwm_extended_frame: RECT,
    has_ole_drop_target: bool,
    children: Vec<WindowRecord>,
}

fn snapshot_zalo_processes() -> HashMap<u32, ProcInfo> {
    let mut map = HashMap::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE || snap.is_null() {
            return map;
        }
        let mut entry: PROCESSENTRY32W = zeroed();
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snap, &mut entry) != 0 {
            loop {
                let name = from_wide_buf(&entry.szExeFile);
                let lower = name.to_ascii_lowercase();
                if lower.starts_with("zalo") {
                    let pid = entry.th32ProcessID;
                    let mut full_path = String::new();
                    let hproc = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid);
                    if !hproc.is_null() {
                        let mut buf = [0u16; 1024];
                        let mut len = buf.len() as u32;
                        if QueryFullProcessImageNameW(hproc, 0, buf.as_mut_ptr(), &mut len) != 0 {
                            full_path = String::from_utf16_lossy(&buf[..len as usize]);
                        }
                        CloseHandle(hproc);
                    }
                    let integrity = query_process_integrity(pid);
                    map.insert(
                        pid,
                        ProcInfo {
                            pid,
                            ppid: entry.th32ParentProcessID,
                            exe_name: name,
                            full_path,
                            integrity,
                        },
                    );
                }
                if Process32NextW(snap, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
    }
    map
}

fn inspect_hwnd_readonly(hwnd: HWND, recurse_children: bool) -> WindowRecord {
    unsafe {
        let mut pid: u32 = 0;
        let tid = GetWindowThreadProcessId(hwnd, &mut pid);
        let class_name = get_class_name(hwnd);
        let title = get_window_text_readonly(hwnd);
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let visible = IsWindowVisible(hwnd) != 0;
        let iconic = IsIconic(hwnd) != 0;
        let zoomed = IsZoomed(hwnd) != 0;
        let enabled = IsWindowEnabled(hwnd) != 0;
        let mut rect = RECT::default();
        GetWindowRect(hwnd, &mut rect);
        let mut client_rect = RECT::default();
        GetClientRect(hwnd, &mut client_rect);
        let parent = GetParent(hwnd) as usize;
        let owner = GetWindow(hwnd, GW_OWNER) as usize;
        let root = GetAncestor(hwnd, GA_ROOT) as usize;
        let root_owner = GetAncestor(hwnd, GA_ROOTOWNER) as usize;
        let dpi_ctx = GetWindowDpiAwarenessContext(hwnd);
        let dpi_context = describe_dpi_context(dpi_ctx);
        let dpi_value = GetDpiForWindow(hwnd);
        let dpi_hosting = GetWindowDpiHostingBehavior(hwnd);

        let mut dwm_cloaked: u32 = 0;
        let _ = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut dwm_cloaked as *mut u32 as *mut c_void,
            size_of::<u32>() as u32,
        );
        let mut dwm_nc_rendering: BOOL = 0;
        let _ = DwmGetWindowAttribute(
            hwnd,
            DWMWA_NCRENDERING_ENABLED,
            &mut dwm_nc_rendering as *mut BOOL as *mut c_void,
            size_of::<BOOL>() as u32,
        );
        let mut dwm_extended_frame = RECT::default();
        let _ = DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut dwm_extended_frame as *mut RECT as *mut c_void,
            size_of::<RECT>() as u32,
        );

        let ole_prop = to_wide("OleDropTargetInterface");
        let has_ole_drop_target = !GetPropW(hwnd, ole_prop.as_ptr()).is_null();

        let mut children = Vec::new();
        if recurse_children {
            struct ChildCtx {
                direct_parent: HWND,
                found: Vec<HWND>,
            }
            unsafe extern "system" fn enum_child_cb(child: HWND, lparam: LPARAM) -> BOOL {
                let ctx = &mut *(lparam as *mut ChildCtx);
                if GetParent(child) == ctx.direct_parent {
                    ctx.found.push(child);
                }
                TRUE
            }
            let mut ctx = ChildCtx {
                direct_parent: hwnd,
                found: Vec::new(),
            };
            EnumChildWindows(hwnd, enum_child_cb, &mut ctx as *mut ChildCtx as LPARAM);
            for ch in ctx.found {
                children.push(inspect_hwnd_readonly(ch, true));
            }
        }

        WindowRecord {
            hwnd: hwnd as usize,
            pid,
            tid,
            class_name,
            title,
            style,
            ex_style,
            visible,
            iconic,
            zoomed,
            enabled,
            rect,
            client_rect,
            parent,
            owner,
            root,
            root_owner,
            dpi_context,
            dpi_value,
            dpi_hosting,
            dwm_cloaked,
            dwm_nc_rendering,
            dwm_extended_frame,
            has_ole_drop_target,
            children,
        }
    }
}

fn print_window_tree(rec: &WindowRecord, indent: usize, procs: &HashMap<u32, ProcInfo>) {
    let pad = " ".repeat(indent);
    let proc_name = procs
        .get(&rec.pid)
        .map(|p| p.exe_name.as_str())
        .unwrap_or("unknown");
    println!(
        "{pad}HWND=0x{:X} pid={} ({}) tid={} class=\"{}\" title=\"{}\"",
        rec.hwnd, rec.pid, proc_name, rec.tid, rec.class_name, rec.title
    );
    println!(
        "{pad}  vis={} iconic={} zoomed={} enabled={} cloaked={} dwm_nc={} ole_drop={}",
        rec.visible,
        rec.iconic,
        rec.zoomed,
        rec.enabled,
        rec.dwm_cloaked,
        rec.dwm_nc_rendering,
        rec.has_ole_drop_target
    );
    println!(
        "{pad}  rect=({},{},{},{}) [{}x{}] client=[{}x{}] dwm_ext=({},{},{},{}) [{}x{}]",
        rec.rect.left,
        rec.rect.top,
        rec.rect.right,
        rec.rect.bottom,
        rec.rect.width(),
        rec.rect.height(),
        rec.client_rect.width(),
        rec.client_rect.height(),
        rec.dwm_extended_frame.left,
        rec.dwm_extended_frame.top,
        rec.dwm_extended_frame.right,
        rec.dwm_extended_frame.bottom,
        rec.dwm_extended_frame.width(),
        rec.dwm_extended_frame.height(),
    );
    println!("{pad}  style={}", describe_style(rec.style));
    println!("{pad}  exstyle={}", describe_exstyle(rec.ex_style));
    println!(
        "{pad}  dpi={} ({} dpi, hosting={}) parent=0x{:X} owner=0x{:X} root=0x{:X} root_owner=0x{:X}",
        rec.dpi_context,
        rec.dpi_value,
        rec.dpi_hosting,
        rec.parent,
        rec.owner,
        rec.root,
        rec.root_owner
    );
    for ch in &rec.children {
        print_window_tree(ch, indent + 4, procs);
    }
}

fn main() {
    unsafe {
        // Set our inspector itself to PerMonitorV2 so GetWindowRect returns physical coordinates accurately.
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    let procs = snapshot_zalo_processes();
    println!("=== READ-ONLY ZALO ARCHITECTURE INSPECTION ===");
    println!("Inspector DPI awareness: PER_MONITOR_AWARE_V2");
    println!("Inspector integrity: {}", query_process_integrity(unsafe { GetCurrentProcessId() }));
    println!("Total matching Zalo* processes: {}", procs.len());

    let mut by_name: BTreeMap<String, Vec<&ProcInfo>> = BTreeMap::new();
    for p in procs.values() {
        by_name.entry(p.exe_name.clone()).or_default().push(p);
    }
    for (name, list) in &by_name {
        println!("\nProcess group: {} (count={})", name, list.len());
        let mut sorted = list.clone();
        sorted.sort_by_key(|p| p.pid);
        for p in sorted {
            println!(
                "  pid={} ppid={} integrity={} path={}",
                p.pid, p.ppid, p.integrity, p.full_path
            );
        }
    }

    struct EnumCtx<'a> {
        procs: &'a HashMap<u32, ProcInfo>,
        top_windows: Vec<HWND>,
        total_enum_count: usize,
    }
    unsafe extern "system" fn enum_top_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = &mut *(lparam as *mut EnumCtx);
        ctx.total_enum_count += 1;
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if ctx.procs.contains_key(&pid) {
            ctx.top_windows.push(hwnd);
        }
        TRUE
    }

    let mut ctx = EnumCtx {
        procs: &procs,
        top_windows: Vec::new(),
        total_enum_count: 0,
    };
    let enum_ok = unsafe { EnumWindows(enum_top_cb, &mut ctx as *mut EnumCtx as LPARAM) };
    let enum_err = unsafe { GetLastError() };
    println!(
        "\nEnumWindows returned ok={} err={} total_desktop_windows={}",
        enum_ok, enum_err, ctx.total_enum_count
    );

    // Also enumerate HWND_MESSAGE children (message-only windows) for Zalo* PIDs
    let mut msg_windows: Vec<HWND> = Vec::new();
    unsafe {
        #[link(name = "user32")]
        extern "system" {
            fn FindWindowExW(
                hWndParent: HWND,
                hWndChildAfter: HWND,
                lpszClass: *const u16,
                lpszWindow: *const u16,
            ) -> HWND;
        }
        let hwnd_message = (-3isize) as HWND;
        let mut cur: HWND = std::ptr::null_mut();
        loop {
            cur = FindWindowExW(hwnd_message, cur, std::ptr::null(), std::ptr::null());
            if cur.is_null() {
                break;
            }
            let mut pid: u32 = 0;
            GetWindowThreadProcessId(cur, &mut pid);
            if procs.contains_key(&pid) {
                msg_windows.push(cur);
            }
        }
    }

    println!("\n=== TOP-LEVEL WINDOWS OWNED BY ZALO* PROCESSES (count={}) ===", ctx.top_windows.len());
    for hwnd in ctx.top_windows {
        let rec = inspect_hwnd_readonly(hwnd, true);
        println!("------------------------------------------------------------");
        print_window_tree(&rec, 0, &procs);
    }

    println!("\n=== MESSAGE-ONLY (HWND_MESSAGE) WINDOWS OWNED BY ZALO* PROCESSES (count={}) ===", msg_windows.len());
    for hwnd in msg_windows {
        let rec = inspect_hwnd_readonly(hwnd, false);
        println!("------------------------------------------------------------");
        print_window_tree(&rec, 0, &procs);
    }

    // Also report installed keyboard layouts on this machine (read-only)
    unsafe {
        let mut hkls = [0isize; 16];
        let count = GetKeyboardLayoutList(hkls.len() as i32, hkls.as_mut_ptr());
        println!("\n=== KEYBOARD LAYOUTS / IME ENVIRONMENT (count={}) ===", count);
        for i in 0..(count.max(0) as usize) {
            println!("  HKL[{i}] = 0x{:016X} (lang_id=0x{:04X})", hkls[i] as usize, (hkls[i] as usize) & 0xFFFF);
        }
        println!("  System DPI: {}", GetDpiForSystem());
    }
}

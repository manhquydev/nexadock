//! Controlled Win32 native GUI fixture for Option A native hosting empirical tests.

#[path = "../win32.rs"]
mod win32;

use std::cell::RefCell;
use std::env;
use std::ffi::c_void;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::mem::{size_of, zeroed};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use win32::*;

pub const CMD_CONTEXT_MENU: u32 = WM_APP + 1;
pub const CMD_OWNED_POPUP: u32 = WM_APP + 2;
pub const CMD_MODAL_FILE_DIALOG: u32 = WM_APP + 3;
pub const CMD_UI_STALL: u32 = WM_APP + 4;
pub const CMD_SPAWN_COMPANION: u32 = WM_APP + 5;
pub const CMD_IME_PROBE: u32 = WM_APP + 6;
pub const CMD_DUMP_TEXT: u32 = WM_APP + 7;
pub const CMD_FOCUS_EDIT: u32 = WM_APP + 8;
pub const CMD_CLOSE_POPUP: u32 = WM_APP + 9;

struct AppState {
    log_path: Option<PathBuf>,
    companion_ready_path: Option<PathBuf>,
    _main_hwnd: HWND,
    edit_hwnd: HWND,
    owned_popup_hwnd: HWND,
    orig_edit_proc: isize,
    companion_child_pid: Option<u32>,
    start_time: Instant,
}

thread_local! {
    static STATE: RefCell<Option<AppState>> = const { RefCell::new(None) };
}

static MAIN_HWND_GLOBAL: AtomicIsize = AtomicIsize::new(0);

fn log_event(msg: &str) {
    STATE.with(|s| {
        if let Some(st) = s.borrow().as_ref() {
            let ms = st.start_time.elapsed().as_millis();
            let line = format!("[{ms:06}ms] {msg}\n");
            if let Some(path) = &st.log_path {
                if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
                    let _ = f.write_all(line.as_bytes());
                    let _ = f.flush();
                }
            }
        }
    });
}

fn get_edit_text(edit: HWND) -> String {
    unsafe {
        let len = SendMessageW(edit, WM_GETTEXTLENGTH, 0, 0) as usize;
        let mut buf = vec![0u16; len + 2];
        let copied = SendMessageW(edit, WM_GETTEXT, buf.len(), buf.as_mut_ptr() as LPARAM) as usize;
        String::from_utf16_lossy(&buf[..copied.min(buf.len())])
    }
}

unsafe extern "system" fn edit_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let orig = STATE.with(|s| s.borrow().as_ref().map(|st| st.orig_edit_proc).unwrap_or(0));
    match msg {
        WM_SETFOCUS => log_event(&format!("EDIT_WM_SETFOCUS prev=0x{:X}", wparam)),
        WM_KILLFOCUS => log_event(&format!("EDIT_WM_KILLFOCUS next=0x{:X}", wparam)),
        WM_CHAR => log_event(&format!("EDIT_WM_CHAR code=0x{:04X}", wparam)),
        WM_IME_STARTCOMPOSITION => log_event("EDIT_WM_IME_STARTCOMPOSITION"),
        WM_IME_COMPOSITION => log_event(&format!("EDIT_WM_IME_COMPOSITION lparam=0x{:X}", lparam)),
        WM_IME_ENDCOMPOSITION => log_event("EDIT_WM_IME_ENDCOMPOSITION"),
        WM_IME_SETCONTEXT => log_event(&format!("EDIT_WM_IME_SETCONTEXT active={}", wparam)),
        WM_MOUSEWHEEL => {
            let delta = ((wparam >> 16) as u16) as i16;
            let x = (lparam & 0xFFFF) as i16;
            let y = ((lparam >> 16) & 0xFFFF) as i16;
            log_event(&format!("EDIT_WM_MOUSEWHEEL delta={delta} pt=({x},{y})"));
        }
        WM_LBUTTONDOWN => {
            let x = (lparam & 0xFFFF) as i16;
            let y = ((lparam >> 16) & 0xFFFF) as i16;
            log_event(&format!("EDIT_WM_LBUTTONDOWN pt=({x},{y})"));
        }
        WM_RBUTTONUP => {
            let x = (lparam & 0xFFFF) as i16;
            let y = ((lparam >> 16) & 0xFFFF) as i16;
            log_event(&format!("EDIT_WM_RBUTTONUP pt=({x},{y})"));
        }
        _ => {}
    }
    if orig != 0 {
        let proc_fn: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT =
            std::mem::transmute(orig);
        proc_fn(hwnd, msg, wparam, lparam)
    } else {
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

unsafe extern "system" fn popup_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            let brush = CreateSolidBrush(0x00F5E6D3);
            FillRect(hdc, &ps.rcPaint, brush);
            DeleteObject(brush as *mut c_void);
            let txt = to_wide("Owned Popup Window (simulating floating dialog)");
            TextOutW(hdc, 12, 16, txt.as_ptr(), (txt.len() - 1) as i32);
            EndPaint(hwnd, &ps);
            return 0;
        }
        WM_DESTROY => {
            log_event(&format!("OWNED_POPUP_DESTROYED hwnd=0x{:X}", hwnd as usize));
            return 0;
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

unsafe extern "system" fn main_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_SIZE => {
            let w = (lparam & 0xFFFF) as i32;
            let h = ((lparam >> 16) & 0xFFFF) as i32;
            log_event(&format!("MAIN_WM_SIZE w={w} h={h} dpi={}", GetDpiForWindow(hwnd)));
            let edit = STATE.with(|s| s.borrow().as_ref().map(|st| st.edit_hwnd).unwrap_or(std::ptr::null_mut()));
            if !edit.is_null() {
                MoveWindow(edit, 12, 36, (w - 24).max(40), (h - 48).max(40), TRUE);
            }
            return 0;
        }
        WM_ACTIVATE => {
            let state = (wparam & 0xFFFF) as u32;
            log_event(&format!("MAIN_WM_ACTIVATE state={state} other=0x{:X}", lparam as usize));
        }
        WM_NCACTIVATE => {
            log_event(&format!("MAIN_WM_NCACTIVATE active={wparam}"));
        }
        WM_SETFOCUS => {
            log_event(&format!("MAIN_WM_SETFOCUS prev=0x{:X}", wparam));
            let edit = STATE.with(|s| s.borrow().as_ref().map(|st| st.edit_hwnd).unwrap_or(std::ptr::null_mut()));
            if !edit.is_null() {
                SetFocus(edit);
            }
        }
        WM_KILLFOCUS => {
            log_event(&format!("MAIN_WM_KILLFOCUS next=0x{:X}", wparam));
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam >> 16) as u16) as i16;
            log_event(&format!("MAIN_WM_MOUSEWHEEL delta={delta}"));
        }
        WM_DROPFILES => {
            let hdrop = wparam as HDROP;
            let count = DragQueryFileW(hdrop, 0xFFFF_FFFF, std::ptr::null_mut(), 0);
            let mut first = String::new();
            if count > 0 {
                let mut buf = [0u16; 512];
                let len = DragQueryFileW(hdrop, 0, buf.as_mut_ptr(), buf.len() as u32);
                first = String::from_utf16_lossy(&buf[..len as usize]);
            }
            DragFinish(hdrop);
            log_event(&format!("MAIN_WM_DROPFILES count={count} first={first}"));
            return 0;
        }
        WM_DPICHANGED => {
            let new_dpi = (wparam & 0xFFFF) as u32;
            log_event(&format!("MAIN_WM_DPICHANGED dpi={new_dpi}"));
        }
        WM_DPICHANGED_BEFOREPARENT => {
            log_event(&format!("MAIN_WM_DPICHANGED_BEFOREPARENT dpi={}", GetDpiForWindow(hwnd)));
        }
        WM_DPICHANGED_AFTERPARENT => {
            log_event(&format!("MAIN_WM_DPICHANGED_AFTERPARENT dpi={}", GetDpiForWindow(hwnd)));
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            let brush = CreateSolidBrush(0x00EEF3F8);
            FillRect(hdc, &ps.rcPaint, brush);
            DeleteObject(brush as *mut c_void);
            let txt = to_wide("Controlled Win32 Native Fixture (Edit + IME + DragDrop + Popups)");
            SetBkMode(hdc, 1);
            TextOutW(hdc, 12, 10, txt.as_ptr(), (txt.len() - 1) as i32);
            EndPaint(hwnd, &ps);
            return 0;
        }
        WM_CLOSE => {
            log_event("MAIN_WM_CLOSE");
            DestroyWindow(hwnd);
            return 0;
        }
        WM_DESTROY => {
            log_event("MAIN_WM_DESTROY");
            PostQuitMessage(0);
            return 0;
        }
        WM_NCDESTROY => {
            log_event("MAIN_WM_NCDESTROY");
        }
        CMD_CONTEXT_MENU => {
            let mut pt = POINT { x: 40, y: 40 };
            ClientToScreen(hwnd, &mut pt);
            let menu = CreatePopupMenu();
            let i1 = to_wide("Copy Message");
            let i2 = to_wide("Reply");
            AppendMenuW(menu, MF_STRING, 101, i1.as_ptr());
            AppendMenuW(menu, MF_STRING, 102, i2.as_ptr());
            let tid = GetCurrentThreadId();
            let hwnd_val = hwnd as usize;
            // Spawn helper thread to check GUI_INMENUMODE while TrackPopupMenu is live, then cancel cleanly.
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(180));
                let mut gti = GUITHREADINFO::default();
                let ok = GetGUIThreadInfo(tid, &mut gti);
                let in_menu = ok != 0 && ((gti.flags & (GUI_INMENUMODE | GUI_POPUPMENUMODE)) != 0);
                // Cancel popup menu with WM_CANCELMODE (0x001F)
                PostMessageW(hwnd_val as HWND, 0x001F, 0, 0);
                // Also send ESC just in case
                PostMessageW(hwnd_val as HWND, WM_KEYDOWN, 0x1B, 0);
                let _ = in_menu;
            });
            log_event(&format!("CONTEXT_MENU_BEGIN screen_pt=({},{})", pt.x, pt.y));
            let cmd = TrackPopupMenu(
                menu,
                TPM_LEFTALIGN | TPM_TOPALIGN | TPM_RETURNCMD,
                pt.x,
                pt.y,
                0,
                hwnd,
                std::ptr::null(),
            );
            DestroyMenu(menu);
            log_event(&format!("CONTEXT_MENU_END returned_cmd={cmd}"));
            return 0;
        }
        CMD_OWNED_POPUP => {
            let hinst = GetModuleHandleW(std::ptr::null());
            let cls = to_wide("SpikeOwnedPopupClass");
            let title = to_wide("OwnedPopupFixture");
            let mut wr = RECT::default();
            GetWindowRect(hwnd, &mut wr);
            // Deliberately position popup extending outside the embedded container right/bottom edge
            let px = wr.right - 80;
            let py = wr.top + 30;
            let popup = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                cls.as_ptr(),
                title.as_ptr(),
                WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
                px,
                py,
                320,
                160,
                hwnd,
                std::ptr::null_mut(),
                hinst,
                std::ptr::null_mut(),
            );
            let mut pr = RECT::default();
            GetWindowRect(popup, &mut pr);
            let owner = GetWindow(popup, GW_OWNER) as usize;
            let parent = GetParent(popup) as usize;
            let root = GetAncestor(popup, GA_ROOT) as usize;
            log_event(&format!(
                "OWNED_POPUP_CREATED hwnd=0x{:X} rect=({},{},{},{}) parent=0x{:X} owner=0x{:X} root=0x{:X}",
                popup as usize, pr.left, pr.top, pr.right, pr.bottom, parent, owner, root
            ));
            STATE.with(|s| {
                if let Some(st) = s.borrow_mut().as_mut() {
                    st.owned_popup_hwnd = popup;
                }
            });
            return popup as LRESULT;
        }
        CMD_CLOSE_POPUP => {
            let pop = STATE.with(|s| {
                if let Some(st) = s.borrow_mut().as_mut() {
                    let h = st.owned_popup_hwnd;
                    st.owned_popup_hwnd = std::ptr::null_mut();
                    h
                } else {
                    std::ptr::null_mut()
                }
            });
            if !pop.is_null() && IsWindow(pop) != 0 {
                DestroyWindow(pop);
            }
            return 0;
        }
        CMD_MODAL_FILE_DIALOG => {
            let root_before = GetAncestor(hwnd, GA_ROOT);
            let root_enabled_before = IsWindowEnabled(root_before) != 0;
            let tid = GetCurrentThreadId();
            let root_val = root_before as usize;
            let log_path_clone = STATE.with(|s| s.borrow().as_ref().and_then(|st| st.log_path.clone()));
            let start_t = STATE.with(|s| s.borrow().as_ref().map(|st| st.start_time).unwrap_or_else(Instant::now));
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(450));
                let root_enabled_during = IsWindowEnabled(root_val as HWND) != 0;
                // Find modal dialog owned by this thread
                struct FindCtx {
                    found: Vec<HWND>,
                }
                unsafe extern "system" fn enum_t(h: HWND, lp: LPARAM) -> BOOL {
                    let c = &mut *(lp as *mut FindCtx);
                    if get_class_name(h) == "#32770" && IsWindowVisible(h) != 0 {
                        c.found.push(h);
                    }
                    TRUE
                }
                let mut fc = FindCtx { found: Vec::new() };
                EnumThreadWindows(tid, enum_t, &mut fc as *mut FindCtx as LPARAM);
                let dlg_count = fc.found.len();
                let mut dlg_rect = RECT::default();
                if let Some(&dlg) = fc.found.first() {
                    GetWindowRect(dlg, &mut dlg_rect);
                    PostMessageW(dlg, WM_CLOSE, 0, 0);
                }
                if let Some(path) = log_path_clone {
                    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
                        let ms = start_t.elapsed().as_millis();
                        let _ = writeln!(
                            f,
                            "[{ms:06}ms] MODAL_DIALOG_DURING root=0x{:X} root_enabled_during={} dlg_count={} dlg_rect=({},{},{},{})",
                            root_val, root_enabled_during, dlg_count, dlg_rect.left, dlg_rect.top, dlg_rect.right, dlg_rect.bottom
                        );
                    }
                }
            });
            let mut file_buf = [0u16; 512];
            let title = to_wide("Spike Modal File Picker");
            let mut ofn: OPENFILENAMEW = zeroed();
            ofn.lStructSize = size_of::<OPENFILENAMEW>() as u32;
            ofn.hwndOwner = hwnd;
            ofn.lpstrFile = file_buf.as_mut_ptr();
            ofn.nMaxFile = file_buf.len() as u32;
            ofn.lpstrTitle = title.as_ptr();
            ofn.Flags = OFN_EXPLORER | OFN_NOCHANGEDIR;
            log_event(&format!(
                "MODAL_DIALOG_BEGIN root=0x{:X} root_enabled_before={}",
                root_before as usize, root_enabled_before
            ));
            let res = GetOpenFileNameW(&mut ofn);
            let root_enabled_after = IsWindowEnabled(root_before) != 0;
            log_event(&format!(
                "MODAL_DIALOG_END res={} root_enabled_after={}",
                res, root_enabled_after
            ));
            return 0;
        }
        CMD_UI_STALL => {
            let ms = if wparam == 0 { 1000 } else { wparam as u64 };
            log_event(&format!("UI_STALL_BEGIN ms={ms}"));
            thread::sleep(Duration::from_millis(ms));
            log_event("UI_STALL_END");
            return 1;
        }
        CMD_SPAWN_COMPANION => {
            if let Ok(exe) = env::current_exe() {
                let comp_ready = STATE.with(|s| s.borrow().as_ref().and_then(|st| st.companion_ready_path.clone()));
                if let Some(rp) = comp_ready {
                    match Command::new(exe)
                        .arg("--companion")
                        .arg("--title")
                        .arg("SpikeCompanionCallWindow")
                        .arg("--ready-file")
                        .arg(&rp)
                        .spawn()
                    {
                        Ok(child) => {
                            let pid = child.id();
                            STATE.with(|s| {
                                if let Some(st) = s.borrow_mut().as_mut() {
                                    st.companion_child_pid = Some(pid);
                                }
                            });
                            log_event(&format!("COMPANION_SPAWNED pid={pid}"));
                            return pid as LRESULT;
                        }
                        Err(e) => {
                            log_event(&format!("COMPANION_SPAWN_ERROR err={e}"));
                        }
                    }
                }
            }
            return 0;
        }
        CMD_IME_PROBE => {
            let edit = STATE.with(|s| s.borrow().as_ref().map(|st| st.edit_hwnd).unwrap_or(std::ptr::null_mut()));
            if !edit.is_null() {
                let himc = ImmGetContext(edit);
                if himc.is_null() {
                    log_event("IME_PROBE himc=NULL");
                } else {
                    let open_status = ImmGetOpenStatus(himc);
                    let cf_set = COMPOSITIONFORM {
                        dwStyle: CFS_POINT,
                        ptCurrentPos: POINT { x: 24, y: 24 },
                        rcArea: RECT::default(),
                    };
                    let set_ok = ImmSetCompositionWindow(himc, &cf_set);
                    let mut cf_get = COMPOSITIONFORM::default();
                    let get_ok = ImmGetCompositionWindow(himc, &mut cf_get);
                    let comp_str: Vec<u16> = "Tiếng Việt".encode_utf16().collect();
                    let comp_bytes = (comp_str.len() * 2) as u32;
                    let scs_ok = ImmSetCompositionStringW(
                        himc,
                        SCS_SETSTR,
                        comp_str.as_ptr() as *const c_void,
                        comp_bytes,
                        std::ptr::null(),
                        0,
                    );
                    let mut pt_screen = cf_get.ptCurrentPos;
                    ClientToScreen(edit, &mut pt_screen);
                    ImmReleaseContext(edit, himc);
                    log_event(&format!(
                        "IME_PROBE himc=0x{:X} open={} set_win={} get_win={} comp_client=({},{}) comp_screen=({},{}) scs_ok={}",
                        himc as usize,
                        open_status,
                        set_ok,
                        get_ok,
                        cf_get.ptCurrentPos.x,
                        cf_get.ptCurrentPos.y,
                        pt_screen.x,
                        pt_screen.y,
                        scs_ok
                    ));
                }
            }
            return 0;
        }
        CMD_DUMP_TEXT => {
            let edit = STATE.with(|s| s.borrow().as_ref().map(|st| st.edit_hwnd).unwrap_or(std::ptr::null_mut()));
            if !edit.is_null() {
                let txt = get_edit_text(edit);
                log_event(&format!("EDIT_TEXT|{}", txt.replace('\r', "\\r").replace('\n', "\\n")));
            }
            return 0;
        }
        CMD_FOCUS_EDIT => {
            let edit = STATE.with(|s| s.borrow().as_ref().map(|st| st.edit_hwnd).unwrap_or(std::ptr::null_mut()));
            if !edit.is_null() {
                let prev = SetFocus(edit);
                let cur = GetFocus();
                log_event(&format!("CMD_FOCUS_EDIT prev=0x{:X} cur=0x{:X} target=0x{:X}", prev as usize, cur as usize, edit as usize));
            }
            return 0;
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut title = "NativeFixtureWindow".to_string();
    let mut dpi_mode = "pmv2".to_string();
    let mut log_path: Option<PathBuf> = None;
    let mut ready_file: Option<PathBuf> = None;
    let mut companion_ready_path: Option<PathBuf> = None;
    let mut is_companion = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--title" if i + 1 < args.len() => {
                title = args[i + 1].clone();
                i += 1;
            }
            "--dpi" if i + 1 < args.len() => {
                dpi_mode = args[i + 1].clone();
                i += 1;
            }
            "--log" if i + 1 < args.len() => {
                log_path = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--ready-file" if i + 1 < args.len() => {
                ready_file = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--companion-ready" if i + 1 < args.len() => {
                companion_ready_path = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--companion" => {
                is_companion = true;
            }
            _ => {}
        }
        i += 1;
    }

    unsafe {
        let dpi_ctx = match dpi_mode.as_str() {
            "unaware" => DPI_AWARENESS_CONTEXT_UNAWARE,
            "system" => DPI_AWARENESS_CONTEXT_SYSTEM_AWARE,
            _ => DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        };
        SetProcessDpiAwarenessContext(dpi_ctx);

        let hinst = GetModuleHandleW(std::ptr::null());
        let cls_name = to_wide(if is_companion {
            "SpikeCompanionClass"
        } else {
            "SpikeNativeFixtureClass"
        });
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(main_wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbrBackground: (COLOR_WINDOW + 1) as HBRUSH,
            lpszMenuName: std::ptr::null(),
            lpszClassName: cls_name.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        RegisterClassExW(&wc);

        let pop_cls = to_wide("SpikeOwnedPopupClass");
        let wc_pop = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(popup_wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbrBackground: (COLOR_WINDOW + 1) as HBRUSH,
            lpszMenuName: std::ptr::null(),
            lpszClassName: pop_cls.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        RegisterClassExW(&wc_pop);

        let wtitle = to_wide(&title);
        let (x, y, w, h) = if is_companion {
            (180, 180, 420, 260)
        } else {
            (120, 120, 640, 480)
        };
        let hwnd = CreateWindowExW(
            WS_EX_ACCEPTFILES,
            cls_name.as_ptr(),
            wtitle.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            x,
            y,
            w,
            h,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinst,
            std::ptr::null_mut(),
        );
        MAIN_HWND_GLOBAL.store(hwnd as isize, Ordering::SeqCst);
        DragAcceptFiles(hwnd, TRUE);

        let mut edit_hwnd = std::ptr::null_mut();
        let mut orig_edit_proc = 0isize;
        if !is_companion {
            let edit_cls = to_wide("EDIT");
            let init_txt = to_wide("");
            edit_hwnd = CreateWindowExW(
                WS_EX_CLIENTEDGE,
                edit_cls.as_ptr(),
                init_txt.as_ptr(),
                WS_CHILD | WS_VISIBLE | ES_LEFT | ES_MULTILINE | ES_AUTOVSCROLL | ES_AUTOHSCROLL,
                12,
                36,
                600,
                390,
                hwnd,
                1001 as HMENU,
                hinst,
                std::ptr::null_mut(),
            );
            // Subclass EDIT control via GWLP_WNDPROC (-4)
            orig_edit_proc = SetWindowLongPtrW(
                edit_hwnd,
                -4,
                edit_subclass_proc as *const () as usize as isize,
            );
        }

        STATE.with(|s| {
            *s.borrow_mut() = Some(AppState {
                log_path,
                companion_ready_path,
                _main_hwnd: hwnd,
                edit_hwnd,
                owned_popup_hwnd: std::ptr::null_mut(),
                orig_edit_proc,
                companion_child_pid: None,
                start_time: Instant::now(),
            });
        });

        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
        log_event(&format!(
            "FIXTURE_READY pid={} hwnd=0x{:X} edit=0x{:X} dpi={} ctx={}",
            GetCurrentProcessId(),
            hwnd as usize,
            edit_hwnd as usize,
            GetDpiForWindow(hwnd),
            describe_dpi_context(GetWindowDpiAwarenessContext(hwnd))
        ));

        if let Some(rf) = ready_file {
            let _ = fs::write(
                rf,
                format!(
                    "{} 0x{:X} 0x{:X}\n",
                    GetCurrentProcessId(),
                    hwnd as usize,
                    edit_hwnd as usize
                ),
            );
        }

        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        log_event("FIXTURE_MESSAGE_LOOP_EXIT");
    }
}

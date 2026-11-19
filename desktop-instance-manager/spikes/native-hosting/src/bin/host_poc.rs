//! Controlled Win32 Native Window Hosting (Option A) empirical PoC & automated test suite.
//!
//! SAFETY GUARANTEE:
//!   - NEVER touches any Zalo process, HWND, or user Zalo data directory.
//!   - NEVER calls global SendInput / SetCursorPos (all input tests use targeted HWND messages
//!     strictly scoped to our own controlled fixture HWNDs inside this sandboxed desktop).
//!   - Operates EXCLUSIVELY on freshly spawned controlled fixtures:
//!       (A) native-fixture.exe (built in this spike)
//!       (B) msedge.exe --app=http://127.0.0.1:<port>/ --user-data-dir=<temp>
//!   - Cleans up all spawned processes and temporary directories unconditionally on exit.

#[path = "../win32.rs"]
mod win32;

use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::c_void;
use std::fs;
use std::io::{Read, Write};
use std::mem::{size_of, zeroed};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use win32::*;

const SIDEBAR_WIDTH: i32 = 220;

// Native fixture command messages (matching native_fixture.rs)
const CMD_CONTEXT_MENU: u32 = WM_APP + 1;
const CMD_OWNED_POPUP: u32 = WM_APP + 2;
const CMD_MODAL_FILE_DIALOG: u32 = WM_APP + 3;
const CMD_UI_STALL: u32 = WM_APP + 4;
const CMD_SPAWN_COMPANION: u32 = WM_APP + 5;
const CMD_IME_PROBE: u32 = WM_APP + 6;
const CMD_DUMP_TEXT: u32 = WM_APP + 7;
const CMD_FOCUS_EDIT: u32 = WM_APP + 8;
const CMD_CLOSE_POPUP: u32 = WM_APP + 9;

const WM_RBUTTONDOWN: u32 = 0x0204;
const WM_CONTEXTMENU: u32 = 0x007B;

#[derive(Clone, Debug)]
struct WindowSnapshot {
    hwnd: HWND,
    style: isize,
    ex_style: isize,
    placement: WINDOWPLACEMENT,
    rect: RECT,
    client_rect: RECT,
    parent: HWND,
    dpi: u32,
    dpi_ctx: String,
    has_ole_drop: bool,
}

fn snapshot_window(hwnd: HWND) -> WindowSnapshot {
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let mut placement = WINDOWPLACEMENT::default();
        GetWindowPlacement(hwnd, &mut placement);
        let mut rect = RECT::default();
        GetWindowRect(hwnd, &mut rect);
        let mut client_rect = RECT::default();
        GetClientRect(hwnd, &mut client_rect);
        let parent = GetParent(hwnd);
        let dpi = GetDpiForWindow(hwnd);
        let dpi_ctx = describe_dpi_context(GetWindowDpiAwarenessContext(hwnd));
        let ole_prop = to_wide("OleDropTargetInterface");
        let has_ole_drop = !GetPropW(hwnd, ole_prop.as_ptr()).is_null();
        WindowSnapshot {
            hwnd,
            style,
            ex_style,
            placement,
            rect,
            client_rect,
            parent,
            dpi,
            dpi_ctx,
            has_ole_drop,
        }
    }
}

fn list_child_windows(parent: HWND) -> Vec<(HWND, u32, String, RECT, bool, bool)> {
    struct Ctx {
        items: Vec<(HWND, u32, String, RECT, bool, bool)>,
    }
    unsafe extern "system" fn cb(h: HWND, lp: LPARAM) -> BOOL {
        let ctx = &mut *(lp as *mut Ctx);
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, &mut pid);
        let cls = get_class_name(h);
        let mut r = RECT::default();
        GetWindowRect(h, &mut r);
        let vis = IsWindowVisible(h) != 0;
        let ole_prop = to_wide("OleDropTargetInterface");
        let ole = !GetPropW(h, ole_prop.as_ptr()).is_null();
        ctx.items.push((h, pid, cls, r, vis, ole));
        TRUE
    }
    let mut ctx = Ctx { items: Vec::new() };
    unsafe {
        EnumChildWindows(parent, cb, &mut ctx as *mut Ctx as LPARAM);
    }
    ctx.items
}

fn list_top_windows() -> Vec<(HWND, u32, u32, String, String, RECT, bool, HWND)> {
    struct Ctx {
        items: Vec<(HWND, u32, u32, String, String, RECT, bool, HWND)>,
    }
    unsafe extern "system" fn cb(h: HWND, lp: LPARAM) -> BOOL {
        let ctx = &mut *(lp as *mut Ctx);
        let mut pid = 0u32;
        let tid = GetWindowThreadProcessId(h, &mut pid);
        let cls = get_class_name(h);
        let title = get_window_text_readonly(h);
        let mut r = RECT::default();
        GetWindowRect(h, &mut r);
        let vis = IsWindowVisible(h) != 0;
        let owner = GetWindow(h, GW_OWNER);
        ctx.items.push((h, pid, tid, cls, title, r, vis, owner));
        TRUE
    }
    let mut ctx = Ctx { items: Vec::new() };
    unsafe {
        EnumWindows(cb, &mut ctx as *mut Ctx as LPARAM);
    }
    ctx.items
}

fn pump_messages_for(dur: Duration) {
    let start = Instant::now();
    unsafe {
        let mut msg: MSG = zeroed();
        while start.elapsed() < dur {
            let mut did_any = false;
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                did_any = true;
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            if !did_any {
                thread::sleep(Duration::from_millis(8));
            }
        }
    }
}

fn make_lparam_xy(x: i32, y: i32) -> LPARAM {
    (((y as u16) as usize) << 16 | ((x as u16) as usize)) as LPARAM
}

fn post_targeted_click(hwnd: HWND, client_x: i32, client_y: i32, right_button: bool) {
    let lp = make_lparam_xy(client_x, client_y);
    unsafe {
        if right_button {
            PostMessageW(hwnd, WM_RBUTTONDOWN, 0x0002, lp);
            thread::sleep(Duration::from_millis(20));
            PostMessageW(hwnd, WM_RBUTTONUP, 0, lp);
            let mut pt = POINT {
                x: client_x,
                y: client_y,
            };
            ClientToScreen(hwnd, &mut pt);
            PostMessageW(hwnd, WM_CONTEXTMENU, hwnd as usize, make_lparam_xy(pt.x, pt.y));
        } else {
            PostMessageW(hwnd, WM_LBUTTONDOWN, 0x0001, lp);
            thread::sleep(Duration::from_millis(20));
            PostMessageW(hwnd, WM_LBUTTONUP, 0, lp);
        }
    }
}

fn post_targeted_wheel(hwnd: HWND, screen_x: i32, screen_y: i32, delta: i16) {
    let wp = (((delta as u16) as usize) << 16) as WPARAM;
    let lp = make_lparam_xy(screen_x, screen_y);
    unsafe {
        PostMessageW(hwnd, WM_MOUSEWHEEL, wp, lp);
    }
}

fn post_targeted_unicode_text(hwnd: HWND, text: &str) {
    unsafe {
        for unit in text.encode_utf16() {
            PostMessageW(hwnd, WM_CHAR, unit as WPARAM, 1);
        }
    }
}

fn get_edit_text(edit: HWND) -> String {
    unsafe {
        let len = SendMessageW(edit, WM_GETTEXTLENGTH, 0, 0) as usize;
        let mut buf = vec![0u16; len + 2];
        let copied = SendMessageW(edit, WM_GETTEXT, buf.len(), buf.as_mut_ptr() as LPARAM) as usize;
        String::from_utf16_lossy(&buf[..copied.min(buf.len())])
    }
}

unsafe fn clear_window_text(hwnd: HWND) {
    let empty = to_wide("");
    SendMessageW(hwnd, WM_SETTEXT, 0, empty.as_ptr() as LPARAM);
}

fn is_process_alive(pid: u32) -> bool {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, FALSE, pid);
        if h.is_null() {
            return false;
        }
        let mut code = 0u32;
        let ok = GetExitCodeProcess(h, &mut code);
        CloseHandle(h);
        ok != 0 && code == STILL_ACTIVE
    }
}

fn kill_pid_force(pid: u32) {
    unsafe {
        let h = OpenProcess(PROCESS_TERMINATE | SYNCHRONIZE, FALSE, pid);
        if !h.is_null() {
            TerminateProcess(h, 1);
            WaitForSingleObject(h, 3000);
            CloseHandle(h);
        }
    }
}

fn find_descendant_pids(root_pids: &[u32]) -> Vec<u32> {
    let mut children_map: HashMap<u32, Vec<u32>> = HashMap::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE || snap.is_null() {
            return Vec::new();
        }
        let mut entry: PROCESSENTRY32W = zeroed();
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snap, &mut entry) != 0 {
            loop {
                children_map
                    .entry(entry.th32ParentProcessID)
                    .or_default()
                    .push(entry.th32ProcessID);
                if Process32NextW(snap, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
    }
    let mut result = Vec::new();
    let mut visited: HashSet<u32> = HashSet::new();
    let mut stack: Vec<u32> = root_pids.to_vec();
    while let Some(pid) = stack.pop() {
        if visited.insert(pid) {
            result.push(pid);
            if let Some(ch) = children_map.get(&pid) {
                for &c in ch {
                    stack.push(c);
                }
            }
        }
    }
    result
}

// ---------- Host Window Management ----------

struct HostWindow {
    hwnd: HWND,
    sidebar_edit: HWND,
    container: HWND,
}

static mut HOST_CONTAINER_GLOBAL: HWND = std::ptr::null_mut();
static mut HOST_EMBEDDED_GLOBAL: HWND = std::ptr::null_mut();

unsafe extern "system" fn host_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_SIZE => {
            let w = (lparam & 0xFFFF) as i32;
            let h = ((lparam >> 16) & 0xFFFF) as i32;
            let container = HOST_CONTAINER_GLOBAL;
            if !container.is_null() {
                let cw = (w - SIDEBAR_WIDTH).max(50);
                let ch = h.max(50);
                SetWindowPos(
                    container,
                    std::ptr::null_mut(),
                    SIDEBAR_WIDTH,
                    0,
                    cw,
                    ch,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                let emb = HOST_EMBEDDED_GLOBAL;
                if !emb.is_null() && IsWindow(emb) != 0 && GetAncestor(emb, GA_PARENT) == container {
                    SetWindowPos(
                        emb,
                        std::ptr::null_mut(),
                        0,
                        0,
                        cw,
                        ch,
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
            }
            return 0;
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            let brush = CreateSolidBrush(0x0024201C);
            let sb_rect = RECT {
                left: 0,
                top: 0,
                right: SIDEBAR_WIDTH,
                bottom: ps.rcPaint.bottom,
            };
            FillRect(hdc, &sb_rect, brush);
            DeleteObject(brush as *mut c_void);
            SetBkMode(hdc, 1);
            SetTextColor(hdc, 0x00F0F0F0);
            let label = to_wide("KHOANG Host Rail (Mock)");
            TextOutW(hdc, 12, 16, label.as_ptr(), (label.len() - 1) as i32);
            EndPaint(hwnd, &ps);
            return 0;
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            return 0;
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

unsafe extern "system" fn container_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_PAINT {
        let mut ps: PAINTSTRUCT = zeroed();
        let hdc = BeginPaint(hwnd, &mut ps);
        let brush = CreateSolidBrush(0x00D8DDE4);
        FillRect(hdc, &ps.rcPaint, brush);
        DeleteObject(brush as *mut c_void);
        EndPaint(hwnd, &ps);
        return 0;
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn create_host_window(title: &str, width: i32, height: i32) -> HostWindow {
    unsafe {
        let hinst = GetModuleHandleW(std::ptr::null());
        let host_cls = to_wide("SpikeHostWindowClass");
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(host_wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbrBackground: (COLOR_BTNFACE + 1) as HBRUSH,
            lpszMenuName: std::ptr::null(),
            lpszClassName: host_cls.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        RegisterClassExW(&wc);

        let cont_cls = to_wide("SpikeHostContainerClass");
        let wc_cont = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(container_wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbrBackground: (COLOR_WINDOW + 1) as HBRUSH,
            lpszMenuName: std::ptr::null(),
            lpszClassName: cont_cls.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        RegisterClassExW(&wc_cont);

        let wtitle = to_wide(title);
        let hwnd = CreateWindowExW(
            0,
            host_cls.as_ptr(),
            wtitle.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN | WS_VISIBLE,
            40,
            40,
            width,
            height,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinst,
            std::ptr::null_mut(),
        );

        let edit_cls = to_wide("EDIT");
        let empty = to_wide("");
        let sidebar_edit = CreateWindowExW(
            WS_EX_CLIENTEDGE,
            edit_cls.as_ptr(),
            empty.as_ptr(),
            WS_CHILD | WS_VISIBLE | ES_LEFT | ES_AUTOHSCROLL,
            12,
            52,
            SIDEBAR_WIDTH - 24,
            32,
            hwnd,
            2001 as HMENU,
            hinst,
            std::ptr::null_mut(),
        );

        let mut cr = RECT::default();
        GetClientRect(hwnd, &mut cr);
        let container = CreateWindowExW(
            0,
            cont_cls.as_ptr(),
            empty.as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
            SIDEBAR_WIDTH,
            0,
            (cr.width() - SIDEBAR_WIDTH).max(100),
            cr.height().max(100),
            hwnd,
            2002 as HMENU,
            hinst,
            std::ptr::null_mut(),
        );
        HOST_CONTAINER_GLOBAL = container;

        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
        pump_messages_for(Duration::from_millis(120));

        HostWindow {
            hwnd,
            sidebar_edit,
            container,
        }
    }
}

fn embed_window(host: &HostWindow, target: HWND) -> (bool, u32, u128) {
    let t0 = Instant::now();
    unsafe {
        SetLastError(0);
        let prev = SetParent(target, host.container);
        let err = GetLastError();
        if prev.is_null() && err != 0 {
            return (false, err, t0.elapsed().as_millis());
        }
        let orig_style = GetWindowLongPtrW(target, GWL_STYLE) as u32;
        let new_style = (orig_style
            & !(WS_POPUP
                | WS_CAPTION
                | WS_THICKFRAME
                | WS_MINIMIZEBOX
                | WS_MAXIMIZEBOX
                | WS_SYSMENU
                | WS_BORDER
                | WS_DLGFRAME))
            | WS_CHILD
            | WS_VISIBLE;
        SetWindowLongPtrW(target, GWL_STYLE, new_style as isize);

        let orig_ex = GetWindowLongPtrW(target, GWL_EXSTYLE) as u32;
        let new_ex = orig_ex & !(WS_EX_APPWINDOW | WS_EX_WINDOWEDGE | WS_EX_DLGMODALFRAME);
        SetWindowLongPtrW(target, GWL_EXSTYLE, new_ex as isize);

        let mut cr = RECT::default();
        GetClientRect(host.container, &mut cr);
        SetWindowPos(
            target,
            std::ptr::null_mut(),
            0,
            0,
            cr.width(),
            cr.height(),
            SWP_NOZORDER | SWP_FRAMECHANGED | SWP_SHOWWINDOW,
        );
        HOST_EMBEDDED_GLOBAL = target;
        let ok = GetAncestor(target, GA_PARENT) == host.container;
        (ok, err, t0.elapsed().as_millis())
    }
}

fn detach_window(snap: &WindowSnapshot) -> (bool, u32) {
    unsafe {
        HOST_EMBEDDED_GLOBAL = std::ptr::null_mut();
        SetLastError(0);
        SetParent(snap.hwnd, std::ptr::null_mut());
        let err = GetLastError();
        SetWindowLongPtrW(snap.hwnd, GWL_STYLE, snap.style);
        SetWindowLongPtrW(snap.hwnd, GWL_EXSTYLE, snap.ex_style);
        SetWindowPos(
            snap.hwnd,
            std::ptr::null_mut(),
            snap.rect.left,
            snap.rect.top,
            snap.rect.width(),
            snap.rect.height(),
            SWP_NOZORDER | SWP_FRAMECHANGED | SWP_SHOWWINDOW,
        );
        SetWindowPlacement(snap.hwnd, &snap.placement);
        ShowWindow(snap.hwnd, SW_SHOW);
        let ok = GetParent(snap.hwnd).is_null() && IsWindow(snap.hwnd) != 0;
        (ok, err)
    }
}

// ---------- Local HTTP Telemetry Server for Chromium Fixture ----------

struct TelemetryHub {
    events: Arc<Mutex<Vec<String>>>,
    next_cmd: Arc<Mutex<Option<String>>>,
    port: u16,
}

const CHROMIUM_FIXTURE_HTML: &str = r#"<!DOCTYPE html>
<html lang="vi">
<head>
<meta charset="utf-8" />
<title>SpikeChromiumFixture</title>
<style>
  body { margin: 0; padding: 12px; font-family: Segoe UI, sans-serif; background: #f4f6f9; color: #1b2430; }
  #chat-input { width: 92%; height: 42px; font-size: 16px; padding: 6px 10px; border: 2px solid #2b6cb0; border-radius: 4px; }
  #scroll-box { margin-top: 10px; height: 140px; overflow-y: scroll; border: 1px solid #999; background: #fff; padding: 8px; }
  #drop-zone { margin-top: 10px; padding: 12px; border: 2px dashed #4a5568; background: #edf2f7; }
  .row { margin-top: 8px; display: flex; gap: 8px; align-items: center; }
</style>
</head>
<body>
  <div id="status">Chromium/Electron-Architecture Fixture</div>
  <div class="row">
    <input id="chat-input" type="text" placeholder="Type Vietnamese/Unicode text here" />
  </div>
  <div class="row">
    <select id="dropdown"><option>Alpha</option><option>Beta</option><option>Gamma</option></select>
    <input id="file-input" type="file" />
    <button id="popup-btn" type="button">Open Popup</button>
  </div>
  <div id="scroll-box"></div>
  <div id="drop-zone">Drop Zone</div>
<script>
const scrollBox = document.getElementById('scroll-box');
for (let i = 1; i <= 50; i++) {
  const p = document.createElement('div');
  p.textContent = 'Message line #' + i + ' — Xin chào Tiếng Việt';
  scrollBox.appendChild(p);
}

function sendEv(tag, data) {
  const payload = tag + '|' + JSON.stringify(data || {});
  fetch('/event', { method: 'POST', body: payload }).catch(() => {});
}

const inp = document.getElementById('chat-input');
inp.addEventListener('focus', () => sendEv('input_focus', { hasFocus: document.hasFocus() }));
inp.addEventListener('blur', () => sendEv('input_blur', { hasFocus: document.hasFocus() }));
inp.addEventListener('input', () => sendEv('input_val', { val: inp.value, hasFocus: document.hasFocus() }));
inp.addEventListener('keydown', (e) => sendEv('keydown', { key: e.key, code: e.code }));
inp.addEventListener('compositionstart', (e) => sendEv('compositionstart', { data: e.data }));
inp.addEventListener('compositionend', (e) => sendEv('compositionend', { data: e.data }));

window.addEventListener('focus', () => sendEv('win_focus', { hasFocus: document.hasFocus() }));
window.addEventListener('blur', () => sendEv('win_blur', { hasFocus: document.hasFocus() }));
window.addEventListener('resize', () => sendEv('resize', {
  iw: window.innerWidth, ih: window.innerHeight,
  ow: window.outerWidth, oh: window.outerHeight,
  dpr: window.devicePixelRatio
}));
document.addEventListener('click', (e) => sendEv('click', {
  id: e.target ? e.target.id : '', cx: e.clientX, cy: e.clientY, sx: e.screenX, sy: e.screenY, hasFocus: document.hasFocus()
}));
document.addEventListener('contextmenu', (e) => sendEv('contextmenu', {
  id: e.target ? e.target.id : '', cx: e.clientX, cy: e.clientY, sx: e.screenX, sy: e.screenY
}));
scrollBox.addEventListener('wheel', (e) => {
  setTimeout(() => sendEv('wheel', { deltaY: e.deltaY, scrollTop: scrollBox.scrollTop }), 40);
});
const dz = document.getElementById('drop-zone');
dz.addEventListener('dragover', (e) => { e.preventDefault(); sendEv('dragover', {}); });
dz.addEventListener('drop', (e) => { e.preventDefault(); sendEv('drop', { files: e.dataTransfer ? e.dataTransfer.files.length : 0 }); });

document.getElementById('popup-btn').addEventListener('click', () => {
  const w = window.open('/popup', 'spike_popup', 'width=320,height=220,left=120,top=120');
  sendEv('popup_call', { opened: !!w });
});

setInterval(async () => {
  try {
    const r = await fetch('/cmd');
    const cmd = (await r.text()).trim();
    if (!cmd) return;
    if (cmd === 'query_state') {
      const ir = inp.getBoundingClientRect();
      const sr = scrollBox.getBoundingClientRect();
      const fr = document.getElementById('file-input').getBoundingClientRect();
      sendEv('state', {
        iw: window.innerWidth, ih: window.innerHeight,
        ow: window.outerWidth, oh: window.outerHeight,
        dpr: window.devicePixelRatio,
        hasFocus: document.hasFocus(),
        activeId: document.activeElement ? document.activeElement.id : '',
        inputVal: inp.value,
        scrollTop: scrollBox.scrollTop,
        ir: [Math.round(ir.left), Math.round(ir.top), Math.round(ir.width), Math.round(ir.height)],
        sr: [Math.round(sr.left), Math.round(sr.top), Math.round(sr.width), Math.round(sr.height)],
        fr: [Math.round(fr.left), Math.round(fr.top), Math.round(fr.width), Math.round(fr.height)]
      });
    } else if (cmd === 'focus_input') {
      inp.focus();
      sendEv('focused_cmd', { hasFocus: document.hasFocus(), activeId: document.activeElement ? document.activeElement.id : '' });
    } else if (cmd === 'open_popup') {
      const w = window.open('/popup', 'spike_popup', 'width=340,height=220,left=150,top=150');
      sendEv('popup_cmd', { opened: !!w });
    }
  } catch (e) {}
}, 70);

sendEv('ready', {
  iw: window.innerWidth, ih: window.innerHeight,
  ow: window.outerWidth, oh: window.outerHeight,
  dpr: window.devicePixelRatio,
  hasFocus: document.hasFocus()
});
</script>
</body>
</html>
"#;

const CHROMIUM_POPUP_HTML: &str = r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>SpikeChromiumPopup</title></head>
<body><h3>Spike Chromium Popup</h3>
<script>
fetch('/event', { method: 'POST', body: 'popup_loaded|' + JSON.stringify({
  sx: window.screenX, sy: window.screenY, ow: window.outerWidth, oh: window.outerHeight, iw: window.innerWidth, ih: window.innerHeight
})});
</script></body></html>"#;

impl TelemetryHub {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let port = listener.local_addr().unwrap().port();
        let events = Arc::new(Mutex::new(Vec::<String>::new()));
        let next_cmd = Arc::new(Mutex::new(None::<String>));
        let ev_bg = Arc::clone(&events);
        let cmd_bg = Arc::clone(&next_cmd);

        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let ev = Arc::clone(&ev_bg);
                let cm = Arc::clone(&cmd_bg);
                thread::spawn(move || handle_http(stream, &ev, &cm));
            }
        });

        Self {
            events,
            next_cmd,
            port,
        }
    }

    fn send_cmd(&self, cmd: &str) {
        *self.next_cmd.lock().unwrap() = Some(cmd.to_string());
    }

    fn clear_events(&self) {
        self.events.lock().unwrap().clear();
    }

    fn snapshot_events(&self) -> Vec<String> {
        self.events.lock().unwrap().clone()
    }

    fn wait_for_prefix(&self, prefix: &str, timeout: Duration) -> Option<String> {
        let start = Instant::now();
        while start.elapsed() < timeout {
            pump_messages_for(Duration::from_millis(30));
            let list = self.events.lock().unwrap();
            if let Some(found) = list.iter().rev().find(|e| e.starts_with(prefix)) {
                return Some(found.clone());
            }
        }
        None
    }
}

fn handle_http(
    mut stream: TcpStream,
    events: &Arc<Mutex<Vec<String>>>,
    next_cmd: &Arc<Mutex<Option<String>>>,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(800)));
    let mut raw = Vec::<u8>::with_capacity(4096);
    let mut tmp = [0u8; 4096];
    let mut header_end: Option<usize> = None;
    let mut content_length: usize = 0;

    // Loop until full HTTP headers AND full Content-Length body are received!
    loop {
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => {
                raw.extend_from_slice(&tmp[..n]);
                if header_end.is_none() {
                    if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                        header_end = Some(pos + 4);
                        let hdr_str = String::from_utf8_lossy(&raw[..pos]);
                        for line in hdr_str.lines() {
                            if let Some((k, v)) = line.split_once(':') {
                                if k.trim().eq_ignore_ascii_case("content-length") {
                                    content_length = v.trim().parse::<usize>().unwrap_or(0);
                                }
                            }
                        }
                    }
                }
                if let Some(he) = header_end {
                    if raw.len() >= he + content_length {
                        break;
                    }
                }
                if raw.len() > 65536 {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    let he = match header_end {
        Some(h) => h,
        None => return,
    };
    let hdr_str = String::from_utf8_lossy(&raw[..he]);
    let first_line = hdr_str.lines().next().unwrap_or("");
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 {
        return;
    }
    let method = parts[0];
    let path = parts[1];

    let (status, ctype, body) = if method == "GET" && path == "/" {
        ("200 OK", "text/html; charset=utf-8", CHROMIUM_FIXTURE_HTML.to_string())
    } else if method == "GET" && path == "/popup" {
        ("200 OK", "text/html; charset=utf-8", CHROMIUM_POPUP_HTML.to_string())
    } else if method == "GET" && path == "/cmd" {
        let c = next_cmd.lock().unwrap().take().unwrap_or_default();
        ("200 OK", "text/plain; charset=utf-8", c)
    } else if method == "POST" && path == "/event" {
        let body_bytes = &raw[he..(he + content_length).min(raw.len())];
        let payload = String::from_utf8_lossy(body_bytes).trim().to_string();
        if !payload.is_empty() {
            events.lock().unwrap().push(payload);
        }
        ("200 OK", "text/plain", "ok".to_string())
    } else {
        ("404 Not Found", "text/plain", "not found".to_string())
    };

    let resp = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(resp.as_bytes());
    let _ = stream.flush();
}

// ---------- Sub-modes (DPI probe & Host crash target) ----------

fn run_dpi_probe_submode(host_dpi: &str, mixed: bool, target_hwnd: HWND) {
    unsafe {
        let ctx = match host_dpi {
            "unaware" => DPI_AWARENESS_CONTEXT_UNAWARE,
            "system" => DPI_AWARENESS_CONTEXT_SYSTEM_AWARE,
            _ => DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        };
        let set_dpi_ok = SetProcessDpiAwarenessContext(ctx);
        let prev_hosting = if mixed {
            SetThreadDpiHostingBehavior(DPI_HOSTING_BEHAVIOR_MIXED)
        } else {
            SetThreadDpiHostingBehavior(DPI_HOSTING_BEHAVIOR_DEFAULT)
        };
        let host = create_host_window("SpikeDpiProbeHost", 700, 500);
        let host_ctx = describe_dpi_context(GetWindowDpiAwarenessContext(host.hwnd));
        let host_dpi_val = GetDpiForWindow(host.hwnd);
        let host_hosting = GetWindowDpiHostingBehavior(host.hwnd);
        let target_ctx_before = describe_dpi_context(GetWindowDpiAwarenessContext(target_hwnd));
        let target_dpi_before = GetDpiForWindow(target_hwnd);

        let snap = snapshot_window(target_hwnd);
        SetLastError(0);
        let prev_parent = SetParent(target_hwnd, host.container);
        let err = GetLastError();
        // Use GA_PARENT (NOT GetParent, which returns owner for non-WS_CHILD windows!)
        let actual_parent = GetAncestor(target_hwnd, GA_PARENT);
        let embedded = actual_parent == host.container;
        let target_dpi_after = GetDpiForWindow(target_hwnd);
        let target_ctx_after = describe_dpi_context(GetWindowDpiAwarenessContext(target_hwnd));

        println!(
            "DPI_PROBE_RESULT host_mode={} mixed={} set_dpi_ok={} prev_hosting={} host_hosting={} host_ctx={} host_dpi={} target_ctx_before={} target_dpi_before={} setparent_prev=0x{:X} err={} embedded={} target_ctx_after={} target_dpi_after={}",
            host_dpi,
            mixed,
            set_dpi_ok,
            prev_hosting,
            host_hosting,
            host_ctx,
            host_dpi_val,
            target_ctx_before,
            target_dpi_before,
            prev_parent as usize,
            err,
            embedded,
            target_ctx_after,
            target_dpi_after
        );

        // CRITICAL: ALWAYS detach before destroying host, otherwise DestroyWindow(host) kills target_hwnd!
        if embedded || !prev_parent.is_null() {
            let _ = detach_window(&snap);
            pump_messages_for(Duration::from_millis(80));
        }
        DestroyWindow(host.hwnd);
    }
}

fn run_crash_host_submode(target_hwnd: HWND, ready_file: &Path) {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let host = create_host_window("SpikeCrashTestHost", 900, 620);
        let (ok, err, ms) = embed_window(&host, target_hwnd);
        pump_messages_for(Duration::from_millis(200));
        let parent_matches = GetAncestor(target_hwnd, GA_PARENT) == host.container;
        let _ = fs::write(
            ready_file,
            format!(
                "EMBEDDED ok={} err={} ms={} host_hwnd=0x{:X} container=0x{:X} parent_matches={}\n",
                ok, err, ms, host.hwnd as usize, host.container as usize, parent_matches
            ),
        );
        // Stay alive pumping messages until outer harness calls TerminateProcess on us!
        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

// ---------- Cleanup Guard ----------

struct SpikeCleanup {
    pids: Vec<u32>,
    temp_dirs: Vec<PathBuf>,
}

impl SpikeCleanup {
    fn new() -> Self {
        Self {
            pids: Vec::new(),
            temp_dirs: Vec::new(),
        }
    }
    fn track_pid(&mut self, pid: u32) {
        self.pids.push(pid);
    }
    fn track_dir(&mut self, dir: PathBuf) {
        self.temp_dirs.push(dir);
    }
}

impl Drop for SpikeCleanup {
    fn drop(&mut self) {
        let all = find_descendant_pids(&self.pids);
        for pid in all.iter().rev() {
            if is_process_alive(*pid) {
                kill_pid_force(*pid);
            }
        }
        thread::sleep(Duration::from_millis(400));
        for dir in &self.temp_dirs {
            let _ = fs::remove_dir_all(dir);
        }
    }
}

fn parse_hex_hwnd(s: &str) -> HWND {
    let cleaned = s.trim().trim_start_matches("0x").trim_start_matches("0X");
    usize::from_str_radix(cleaned, 16).unwrap_or(0) as HWND
}

fn wait_for_file(path: &Path, timeout: Duration) -> Option<String> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(content) = fs::read_to_string(path) {
            if !content.trim().is_empty() {
                return Some(content);
            }
        }
        pump_messages_for(Duration::from_millis(30));
    }
    None
}

fn spawn_native_fixture(
    exe_dir: &Path,
    temp_dir: &Path,
    tag: &str,
    dpi: &str,
    cleanup: &mut SpikeCleanup,
) -> (u32, HWND, HWND, PathBuf, PathBuf) {
    let fixture_exe = exe_dir.join("native-fixture.exe");
    let log_path = temp_dir.join(format!("native-{tag}.log"));
    let ready_path = temp_dir.join(format!("native-{tag}.ready"));
    let comp_ready_path = temp_dir.join(format!("native-{tag}-companion.ready"));
    let child = Command::new(&fixture_exe)
        .arg("--title")
        .arg(format!("SpikeNativeFixture-{tag}"))
        .arg("--dpi")
        .arg(dpi)
        .arg("--log")
        .arg(&log_path)
        .arg("--ready-file")
        .arg(&ready_path)
        .arg("--companion-ready")
        .arg(&comp_ready_path)
        .spawn()
        .expect("spawn native-fixture.exe");
    let pid = child.id();
    cleanup.track_pid(pid);

    let ready_txt = wait_for_file(&ready_path, Duration::from_secs(6))
        .expect("native fixture ready file timeout");
    let parts: Vec<&str> = ready_txt.split_whitespace().collect();
    let hwnd = parse_hex_hwnd(parts[1]);
    let edit_hwnd = parse_hex_hwnd(parts[2]);
    pump_messages_for(Duration::from_millis(120));
    (pid, hwnd, edit_hwnd, log_path, comp_ready_path)
}

fn find_edge_exe() -> Option<PathBuf> {
    let candidates = [
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    ];
    for c in candidates {
        let p = PathBuf::from(c);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

fn spawn_chromium_fixture(
    hub: &TelemetryHub,
    temp_dir: &Path,
    tag: &str,
    cleanup: &mut SpikeCleanup,
) -> (u32, HWND, PathBuf) {
    let edge = find_edge_exe().expect("msedge.exe not found");
    let user_data = temp_dir.join(format!("edge-profile-{tag}"));
    fs::create_dir_all(&user_data).expect("create edge profile dir");
    cleanup.track_dir(user_data.clone());

    let before_hwnds: HashSet<usize> = list_top_windows().iter().map(|w| w.0 as usize).collect();
    hub.clear_events();

    let url = format!("http://127.0.0.1:{}/", hub.port);
    let child: Child = Command::new(&edge)
        .arg(format!("--app={url}"))
        .arg(format!("--user-data-dir={}", user_data.display()))
        .arg("--new-window")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--window-size=800,560")
        .arg("--window-position=140,100")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn msedge.exe");
    let root_pid = child.id();
    cleanup.track_pid(root_pid);

    let ready_ev = hub.wait_for_prefix("ready|", Duration::from_secs(12));
    println!("  [chromium:{tag}] ready event: {:?}", ready_ev);

    let mut found_hwnd: HWND = std::ptr::null_mut();
    let mut found_pid: u32 = root_pid;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(8) {
        for (h, pid, _tid, cls, title, r, vis, _owner) in list_top_windows() {
            if !before_hwnds.contains(&(h as usize))
                && vis
                && cls == "Chrome_WidgetWin_1"
                && title.contains("SpikeChromiumFixture")
                && r.width() > 200
            {
                found_hwnd = h;
                found_pid = pid;
                break;
            }
        }
        if !found_hwnd.is_null() {
            break;
        }
        pump_messages_for(Duration::from_millis(80));
    }
    assert!(!found_hwnd.is_null(), "failed to locate Edge Chrome_WidgetWin_1");
    cleanup.track_pid(found_pid);
    (found_pid, found_hwnd, user_data)
}

fn run_host_crash_experiment(
    self_exe: &Path,
    temp_dir: &Path,
    label: &str,
    target_pid: u32,
    target_hwnd: HWND,
    log_path_opt: Option<&Path>,
    cleanup: &mut SpikeCleanup,
) {
    println!("\n--- [CRASH RECOVERY TEST: {label}] ---");
    let ready = temp_dir.join(format!("crash-host-{label}.ready"));
    let mut host_child = Command::new(self_exe)
        .arg("--crash-host")
        .arg(format!("0x{:X}", target_hwnd as usize))
        .arg(&ready)
        .spawn()
        .expect("spawn crash-host helper");
    let host_pid = host_child.id();
    cleanup.track_pid(host_pid);

    let status = wait_for_file(&ready, Duration::from_secs(6))
        .unwrap_or_else(|| "TIMEOUT".to_string());
    println!("  helper host pid={host_pid} pre-crash: {}", status.trim());

    let pre_is_window = unsafe { IsWindow(target_hwnd) != 0 };
    let pre_alive = is_process_alive(target_pid);
    let pre_tree: Vec<u32> = find_descendant_pids(&[target_pid])
        .into_iter()
        .filter(|&p| is_process_alive(p))
        .collect();
    println!(
        "  BEFORE TerminateProcess(host={host_pid}): target_hwnd_valid={} target_pid_alive={} alive_tree_procs={}",
        pre_is_window,
        pre_alive,
        pre_tree.len()
    );

    unsafe {
        let hproc = OpenProcess(PROCESS_TERMINATE | SYNCHRONIZE, FALSE, host_pid);
        let term_ok = if !hproc.is_null() {
            let ok = TerminateProcess(hproc, 0xDEAD);
            WaitForSingleObject(hproc, 3000);
            CloseHandle(hproc);
            ok != 0
        } else {
            false
        };
        let _ = host_child.wait();
        println!("  TerminateProcess(host={host_pid}, 0xDEAD) ok={term_ok}");
    }

    for wait_ms in [300u64, 1000, 2000] {
        thread::sleep(Duration::from_millis(wait_ms));
        let win_valid = unsafe { IsWindow(target_hwnd) != 0 };
        let win_vis = if win_valid {
            unsafe { IsWindowVisible(target_hwnd) != 0 }
        } else {
            false
        };
        let proc_alive = is_process_alive(target_pid);
        let remaining_tree: Vec<u32> = find_descendant_pids(&[target_pid])
            .into_iter()
            .filter(|&p| is_process_alive(p))
            .collect();
        let vis_windows_for_tree: usize = list_top_windows()
            .into_iter()
            .filter(|w| remaining_tree.contains(&w.1) && w.6)
            .count();
        println!(
            "  AFTER +{wait_ms}ms: target_hwnd_valid={win_valid} visible={win_vis} target_pid_alive={proc_alive} alive_tree_procs={} visible_top_windows={vis_windows_for_tree}",
            remaining_tree.len()
        );
    }

    if let Some(lp) = log_path_opt {
        if let Ok(log) = fs::read_to_string(lp) {
            let tail: Vec<&str> = log.lines().rev().take(8).collect();
            println!("  target fixture log tail (newest first): {:?}", tail);
        }
    }
}

fn parse_array_4(state_str: &str, key: &str) -> Option<[i32; 4]> {
    let pat = format!("\"{key}\":[");
    let start = state_str.find(&pat)? + pat.len();
    let end = state_str[start..].find(']')? + start;
    let nums: Vec<i32> = state_str[start..end]
        .split(',')
        .filter_map(|s| s.trim().parse::<i32>().ok())
        .collect();
    if nums.len() == 4 {
        Some([nums[0], nums[1], nums[2], nums[3]])
    } else {
        None
    }
}

fn run_full_suite() {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    let self_exe = env::current_exe().expect("current_exe");
    let exe_dir = self_exe.parent().expect("exe parent dir").to_path_buf();
    let run_id = format!(
        "native-hosting-spike-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );
    let temp_dir = env::temp_dir().join(&run_id);
    fs::create_dir_all(&temp_dir).expect("create temp dir");

    let mut cleanup = SpikeCleanup::new();
    cleanup.track_dir(temp_dir.clone());

    println!("====================================================================");
    println!("OPTION A — WIN32 NATIVE WINDOW HOSTING EMPIRICAL VERIFICATION SUITE (v2)");
    println!("Temp workspace: {}", temp_dir.display());
    unsafe {
        println!(
            "Host PID={} SystemDPI={} Integrity={}",
            GetCurrentProcessId(),
            GetDpiForSystem(),
            query_process_integrity(GetCurrentProcessId())
        );
    }
    println!("====================================================================");

    let host = create_host_window("KHOANG Native Hosting PoC (PMv2)", 1080, 700);
    println!(
        "Host created: hwnd=0x{:X} container=0x{:X} sidebar_edit=0x{:X} dpi={} ctx={}",
        host.hwnd as usize,
        host.container as usize,
        host.sidebar_edit as usize,
        unsafe { GetDpiForWindow(host.hwnd) },
        describe_dpi_context(unsafe { GetWindowDpiAwarenessContext(host.hwnd) })
    );

    // =========================================================================
    // PART 1: CONTROLLED WIN32 NATIVE FIXTURE
    // =========================================================================
    println!("\n==================== PART 1: WIN32 NATIVE FIXTURE ====================");
    let (nat_pid, nat_hwnd, nat_edit, nat_log, nat_comp_ready) =
        spawn_native_fixture(&exe_dir, &temp_dir, "main", "pmv2", &mut cleanup);
    let nat_snap = snapshot_window(nat_hwnd);
    let pre_embed_log_lines = fs::read_to_string(&nat_log).map(|s| s.lines().count()).unwrap_or(0);
    println!(
        "Native fixture before embed: pid={nat_pid} hwnd=0x{:X} edit=0x{:X} style={} ex={} rect=({},{},{},{}) dpi={} ctx={}",
        nat_hwnd as usize,
        nat_edit as usize,
        describe_style(nat_snap.style as u32),
        describe_exstyle(nat_snap.ex_style as u32),
        nat_snap.rect.left, nat_snap.rect.top, nat_snap.rect.right, nat_snap.rect.bottom,
        nat_snap.dpi, nat_snap.dpi_ctx
    );

    // 1.1 Embed & Initial Layout
    let (emb_ok, emb_err, emb_ms) = embed_window(&host, nat_hwnd);
    pump_messages_for(Duration::from_millis(180));
    let mut cont_sr = RECT::default();
    let mut nat_sr = RECT::default();
    unsafe {
        GetWindowRect(host.container, &mut cont_sr);
        GetWindowRect(nat_hwnd, &mut nat_sr);
    }
    println!(
        "[1.1 Native Embed] ok={emb_ok} err={emb_err} latency={emb_ms}ms container_rect=({},{},{},{})[{}x{}] embedded_rect=({},{},{},{})[{}x{}] exact_match={}",
        cont_sr.left, cont_sr.top, cont_sr.right, cont_sr.bottom, cont_sr.width(), cont_sr.height(),
        nat_sr.left, nat_sr.top, nat_sr.right, nat_sr.bottom, nat_sr.width(), nat_sr.height(),
        cont_sr == nat_sr
    );

    // 1.2 Rapid Host Resize Stress (20 steps)
    let r_start = Instant::now();
    let mut max_delta = 0i32;
    for step in 0..20 {
        let w = 960 + ((step % 10) * 32);
        let h = 620 + ((step % 10) * 18);
        unsafe {
            SetWindowPos(host.hwnd, std::ptr::null_mut(), 40, 40, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
        }
        pump_messages_for(Duration::from_millis(18));
        let mut cr = RECT::default();
        let mut nr = RECT::default();
        unsafe {
            GetWindowRect(host.container, &mut cr);
            GetWindowRect(nat_hwnd, &mut nr);
        }
        let d = (cr.width() - nr.width()).abs().max((cr.height() - nr.height()).abs());
        max_delta = max_delta.max(d);
    }
    println!(
        "[1.2 Native Resize 20x] total_ms={} max_geom_delta_px={max_delta}",
        r_start.elapsed().as_millis()
    );

    // 1.3 Focus Ping-Pong (Host Rail <-> Embedded Native EDIT) & Vietnamese Unicode Input
    let mut ed_r = RECT::default();
    unsafe { GetWindowRect(nat_edit, &mut ed_r); }
    let ed_cx = (ed_r.left + ed_r.right) / 2;
    let ed_cy = (ed_r.top + ed_r.bottom) / 2;
    let wfp_embedded = unsafe { WindowFromPoint(POINT { x: ed_cx, y: ed_cy }) };

    // Focus host sidebar edit -> type RAIL1-
    unsafe {
        SetFocus(host.sidebar_edit);
    }
    pump_messages_for(Duration::from_millis(60));
    let focus_rail_1 = unsafe { GetFocus() == host.sidebar_edit };
    post_targeted_unicode_text(host.sidebar_edit, "RAIL1-");
    pump_messages_for(Duration::from_millis(60));

    // Focus embedded native EDIT (test direct SetFocus across implicit SetParent shared queue + CMD_FOCUS_EDIT)
    let nat_tid = unsafe { GetWindowThreadProcessId(nat_hwnd, std::ptr::null_mut()) };
    let direct_setfocus_prev = unsafe { SetFocus(nat_edit) };
    let direct_setfocus_err = unsafe { GetLastError() };
    pump_messages_for(Duration::from_millis(80));
    let mut gti_nat_direct = GUITHREADINFO::default();
    unsafe { GetGUIThreadInfo(nat_tid, &mut gti_nat_direct); }

    // Also invoke CMD_FOCUS_EDIT on the child thread
    unsafe { SendMessageW(nat_hwnd, CMD_FOCUS_EDIT, 0, 0); }
    pump_messages_for(Duration::from_millis(60));
    let mut gti_nat_cmd = GUITHREADINFO::default();
    unsafe { GetGUIThreadInfo(nat_tid, &mut gti_nat_cmd); }

    let vn_sample = "Xin chào Tiếng Việt: ă â đ ê ô ơ ư ệ ữ";
    post_targeted_unicode_text(nat_edit, vn_sample);
    pump_messages_for(Duration::from_millis(120));
    unsafe {
        SendMessageW(nat_hwnd, CMD_DUMP_TEXT, 0, 0);
        SendMessageW(nat_hwnd, CMD_IME_PROBE, 0, 0);
    }
    pump_messages_for(Duration::from_millis(80));

    // Return focus to host sidebar edit -> type RAIL2
    unsafe { SetFocus(host.sidebar_edit); }
    pump_messages_for(Duration::from_millis(60));
    let focus_back_rail = unsafe { GetFocus() == host.sidebar_edit };
    post_targeted_unicode_text(host.sidebar_edit, "RAIL2");
    pump_messages_for(Duration::from_millis(60));
    let rail_text = get_edit_text(host.sidebar_edit);

    println!(
        "[1.3 Native Focus & VN Input] wfp_hit=0x{:X}(edit=0x{:X}) focus_rail_1={focus_rail_1} direct_SetFocus_prev=0x{:X}(err={direct_setfocus_err}) gti_focus_after_direct=0x{:X} gti_focus_after_cmd=0x{:X} focus_back_rail={focus_back_rail} rail_text='{}'",
        wfp_embedded as usize,
        nat_edit as usize,
        direct_setfocus_prev as usize,
        gti_nat_direct.hwndFocus as usize,
        gti_nat_cmd.hwndFocus as usize,
        rail_text
    );

    // 1.4 Mouse Wheel & Drag-and-Drop Check
    post_targeted_wheel(nat_edit, ed_cx, ed_cy, -240);
    pump_messages_for(Duration::from_millis(80));
    let ex_embedded = unsafe { GetWindowLongPtrW(nat_hwnd, GWL_EXSTYLE) as u32 };
    let accept_files_preserved = (ex_embedded & WS_EX_ACCEPTFILES) != 0;
    println!("[1.4 Native Mouse/DragDrop] WS_EX_ACCEPTFILES_preserved={accept_files_preserved}");

    // 1.5 Cross-Process Input Queue Stall Coupling Test (AttachThreadInput side-effect!)
    unsafe { PostMessageW(nat_hwnd, CMD_UI_STALL, 1200, 0); }
    thread::sleep(Duration::from_millis(80));
    let stall_t0 = Instant::now();
    unsafe {
        SetWindowPos(host.hwnd, std::ptr::null_mut(), 40, 40, 1020, 680, SWP_NOZORDER | SWP_NOACTIVATE);
    }
    let host_blocked_ms = stall_t0.elapsed().as_millis();
    pump_messages_for(Duration::from_millis(200));
    println!(
        "[1.5 Native Cross-Process Hang Coupling] child_stall=1200ms => host_UI_thread_blocked_ms={host_blocked_ms}ms (Synchronous Win32 freeze!)"
    );

    // 1.6 Popups, Context Menu, Modal File Dialog, Separate Companion Process
    unsafe {
        SendMessageW(nat_hwnd, CMD_CONTEXT_MENU, 0, 0);
        pump_messages_for(Duration::from_millis(100));

        let popup_hwnd = SendMessageW(nat_hwnd, CMD_OWNED_POPUP, 0, 0) as HWND;
        pump_messages_for(Duration::from_millis(120));
        let mut pop_r = RECT::default();
        let mut host_r = RECT::default();
        GetWindowRect(popup_hwnd, &mut pop_r);
        GetWindowRect(host.hwnd, &mut host_r);
        let escapes_host = pop_r.right > host_r.right || pop_r.bottom > host_r.bottom;
        println!(
            "[1.6a Native Owned Popup] hwnd=0x{:X} rect=({},{},{},{}) host_right={} escapes_host_bounds={escapes_host}",
            popup_hwnd as usize, pop_r.left, pop_r.top, pop_r.right, pop_r.bottom, host_r.right
        );
        SendMessageW(nat_hwnd, CMD_CLOSE_POPUP, 0, 0);
        pump_messages_for(Duration::from_millis(100));

        // Modal file dialog: trigger async via PostMessage so host keeps pumping & observes its own IsWindowEnabled!
        PostMessageW(nat_hwnd, CMD_MODAL_FILE_DIALOG, 0, 0);
        pump_messages_for(Duration::from_millis(320));
        let host_enabled_during_modal = IsWindowEnabled(host.hwnd) != 0;
        pump_messages_for(Duration::from_millis(650));
        let host_enabled_after_modal = IsWindowEnabled(host.hwnd) != 0;
        println!(
            "[1.6b Native Modal File Dialog] host_enabled_during_modal={host_enabled_during_modal} host_enabled_after_modal={host_enabled_after_modal}"
        );

        // Separate-process companion window (simulating ZaloCall.exe / ZaloCap.exe)
        let comp_pid = SendMessageW(nat_hwnd, CMD_SPAWN_COMPANION, 0, 0) as u32;
        if comp_pid != 0 {
            cleanup.track_pid(comp_pid);
            if let Some(cr_txt) = wait_for_file(&nat_comp_ready, Duration::from_secs(4)) {
                let cp: Vec<&str> = cr_txt.split_whitespace().collect();
                let comp_hwnd = parse_hex_hwnd(cp[1]);
                let comp_parent = GetAncestor(comp_hwnd, GA_PARENT);
                let comp_vis = IsWindowVisible(comp_hwnd) != 0;
                println!(
                    "[1.6c Separate Companion Process (ZaloCall/ZaloCap analog)] pid={comp_pid} hwnd=0x{:X} ga_parent=0x{:X} visible={comp_vis} contained_in_host={}",
                    comp_hwnd as usize, comp_parent as usize, comp_parent == host.container
                );
                PostMessageW(comp_hwnd, WM_CLOSE, 0, 0);
            }
        }
    }

    // Print key native fixture log events AFTER embedding
    if let Ok(log_txt) = fs::read_to_string(&nat_log) {
        let post_lines: Vec<&str> = log_txt.lines().skip(pre_embed_log_lines).collect();
        let activate_count = post_lines.iter().filter(|l| l.contains("MAIN_WM_ACTIVATE")).count();
        let edit_text_line = post_lines.iter().rev().find(|l| l.contains("EDIT_TEXT|")).copied().unwrap_or("");
        let ime_line = post_lines.iter().rev().find(|l| l.contains("IME_PROBE")).copied().unwrap_or("");
        let wheel_seen = post_lines.iter().any(|l| l.contains("WM_MOUSEWHEEL"));
        let ctx_menu_lines: Vec<&&str> = post_lines.iter().filter(|l| l.contains("CONTEXT_MENU_")).collect();
        let modal_lines: Vec<&&str> = post_lines.iter().filter(|l| l.contains("MODAL_DIALOG_")).collect();
        let vn_exact = edit_text_line.contains(vn_sample);
        println!("[Native Log Summary (post-embed)]");
        println!("  VN exact match={vn_exact} | {edit_text_line}");
        println!("  IME probe: {ime_line}");
        println!("  Wheel received={wheel_seen} | MAIN_WM_ACTIVATE_count_while_embedded={activate_count}");
        for l in &ctx_menu_lines {
            println!("  {l}");
        }
        for l in &modal_lines {
            println!("  {l}");
        }
    }

    // 1.7 Safe Detach & Restoration (Native)
    let (det_ok, det_err) = detach_window(&nat_snap);
    pump_messages_for(Duration::from_millis(150));
    let nat_after = snapshot_window(nat_hwnd);
    let mut ping_res = 0usize;
    let responsive = unsafe {
        SendMessageTimeoutW(nat_hwnd, 0, 0, 0, SMTO_ABORTIFHUNG, 500, &mut ping_res) != 0
    };
    println!(
        "[1.7 Native Safe Detach] ok={det_ok} err={det_err} parent_null={} style_exact={} exstyle_exact={} rect_match={} visible={} responsive={responsive}",
        nat_after.parent.is_null(),
        nat_after.style == nat_snap.style,
        nat_after.ex_style == nat_snap.ex_style,
        nat_after.rect == nat_snap.rect,
        unsafe { IsWindowVisible(nat_hwnd) != 0 }
    );

    // 1.8 DPI Mismatch Matrix (subprocess probes against detached PMv2 native fixture)
    println!("\n--- [1.8a DPI MISMATCH MATRIX: Guest=PMv2 (120dpi)] ---");
    for (mode, mixed) in [
        ("pmv2", false),
        ("system", false),
        ("system", true),
        ("unaware", false),
        ("unaware", true),
    ] {
        let out = Command::new(&self_exe)
            .arg("--dpi-probe")
            .arg(mode)
            .arg(if mixed { "1" } else { "0" })
            .arg(format!("0x{:X}", nat_hwnd as usize))
            .output()
            .expect("run dpi-probe");
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            println!("  {line}");
        }
    }

    // Also test Host=PMv2 embedding an UNAWARE guest (like ZaloCall/ZaloCap or legacy app)
    let (unaw_pid, unaw_hwnd, _unaw_edit, _unaw_log, _) =
        spawn_native_fixture(&exe_dir, &temp_dir, "unaware", "unaware", &mut cleanup);
    println!("--- [1.8b DPI MISMATCH MATRIX: Host=PMv2 vs Guest=UNAWARE (96dpi)] ---");
    for mixed in [false, true] {
        let out = Command::new(&self_exe)
            .arg("--dpi-probe")
            .arg("pmv2")
            .arg(if mixed { "1" } else { "0" })
            .arg(format!("0x{:X}", unaw_hwnd as usize))
            .output()
            .expect("run dpi-probe unaware guest");
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            println!("  {line}");
        }
    }
    kill_pid_force(unaw_pid);

    // 1.9 Host Crash Recovery on Native Fixture (nat_hwnd is alive and cleanly detached!)
    run_host_crash_experiment(
        &self_exe,
        &temp_dir,
        "NativeFixture",
        nat_pid,
        nat_hwnd,
        Some(&nat_log),
        &mut cleanup,
    );
    kill_pid_force(nat_pid);

    // =========================================================================
    // PART 2: CONTROLLED CHROMIUM / ELECTRON-ARCHITECTURE FIXTURE (msedge --app)
    // =========================================================================
    println!("\n==================== PART 2: CHROMIUM / ELECTRON-ARCH FIXTURE ====================");
    let hub = TelemetryHub::start();
    let (chr_pid, chr_hwnd, _chr_profile) =
        spawn_chromium_fixture(&hub, &temp_dir, "main", &mut cleanup);
    let chr_snap = snapshot_window(chr_hwnd);
    let chr_children_before = list_child_windows(chr_hwnd);
    println!(
        "Chromium fixture before embed: pid={chr_pid} hwnd=0x{:X} style={} ex={} rect=({},{},{},{})[{}x{}] client=[{}x{}] dpi={} ctx={} ole_drop={}",
        chr_hwnd as usize,
        describe_style(chr_snap.style as u32),
        describe_exstyle(chr_snap.ex_style as u32),
        chr_snap.rect.left, chr_snap.rect.top, chr_snap.rect.right, chr_snap.rect.bottom,
        chr_snap.rect.width(), chr_snap.rect.height(),
        chr_snap.client_rect.width(), chr_snap.client_rect.height(),
        chr_snap.dpi, chr_snap.dpi_ctx, chr_snap.has_ole_drop
    );
    for (ch, cpid, cls, cr, vis, ole) in &chr_children_before {
        println!(
            "  CHILD before: hwnd=0x{:X} pid={cpid} vis={vis} ole_drop={ole} cls='{}' rect=[{}x{}]",
            *ch as usize, cls, cr.width(), cr.height()
        );
    }

    // 2.1 Embed Chromium into Host Container
    hub.clear_events();
    let (c_emb_ok, c_emb_err, c_emb_ms) = embed_window(&host, chr_hwnd);
    pump_messages_for(Duration::from_millis(350));
    let mut cont_r2 = RECT::default();
    let mut chr_r2 = RECT::default();
    unsafe {
        GetWindowRect(host.container, &mut cont_r2);
        GetWindowRect(chr_hwnd, &mut chr_r2);
    }
    let chr_style_after_embed = unsafe { GetWindowLongPtrW(chr_hwnd, GWL_STYLE) as u32 };
    let chr_ex_after_embed = unsafe { GetWindowLongPtrW(chr_hwnd, GWL_EXSTYLE) as u32 };
    let chr_ole_after = !unsafe {
        GetPropW(chr_hwnd, to_wide("OleDropTargetInterface").as_ptr()).is_null()
    };
    let chr_children_after = list_child_windows(chr_hwnd);
    let render_hwnd = chr_children_after
        .iter()
        .find(|c| c.2 == "Chrome_RenderWidgetHostHWND")
        .map(|c| c.0)
        .unwrap_or(chr_hwnd);

    hub.send_cmd("query_state");
    let state_after_embed = hub.wait_for_prefix("state|", Duration::from_secs(4));
    println!(
        "[2.1 Chromium Embed] ok={c_emb_ok} err={c_emb_err} latency={c_emb_ms}ms container=({},{},{},{})[{}x{}] chr_win=({},{},{},{})[{}x{}] exact_win_match={}",
        cont_r2.left, cont_r2.top, cont_r2.right, cont_r2.bottom, cont_r2.width(), cont_r2.height(),
        chr_r2.left, chr_r2.top, chr_r2.right, chr_r2.bottom, chr_r2.width(), chr_r2.height(),
        cont_r2 == chr_r2
    );
    println!(
        "  style_after={} ex_after={} ole_drop_top_after={chr_ole_after}",
        describe_style(chr_style_after_embed),
        describe_exstyle(chr_ex_after_embed)
    );
    for (ch, cpid, cls, cr, vis, ole) in &chr_children_after {
        println!(
            "  CHILD after: hwnd=0x{:X} pid={cpid} vis={vis} ole_drop={ole} cls='{}' rect=({},{},{},{})[{}x{}]",
            *ch as usize, cls, cr.left, cr.top, cr.right, cr.bottom, cr.width(), cr.height()
        );
    }
    println!("  DOM state after embed: {:?}", state_after_embed);

    // 2.2 Rapid Host Resize Stress (20 steps) on Embedded Chromium
    hub.clear_events();
    let cr_start = Instant::now();
    let mut c_max_win_delta = 0i32;
    let mut c_max_d3d_delta = 0i32;
    for step in 0..20 {
        let w = 960 + ((step % 10) * 32);
        let h = 620 + ((step % 10) * 18);
        unsafe {
            SetWindowPos(host.hwnd, std::ptr::null_mut(), 40, 40, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
        }
        pump_messages_for(Duration::from_millis(25));
        let mut cr = RECT::default();
        let mut wr = RECT::default();
        unsafe {
            GetWindowRect(host.container, &mut cr);
            GetWindowRect(chr_hwnd, &mut wr);
        }
        let wd = (cr.width() - wr.width()).abs().max((cr.height() - wr.height()).abs());
        c_max_win_delta = c_max_win_delta.max(wd);
        for (_, _, cls, d3d_r, _, _) in list_child_windows(chr_hwnd) {
            if cls == "Intermediate D3D Window" {
                let dd = (cr.width() - d3d_r.width()).abs().max((cr.height() - d3d_r.height()).abs());
                c_max_d3d_delta = c_max_d3d_delta.max(dd);
            }
        }
    }
    pump_messages_for(Duration::from_millis(250));
    let resize_events: usize = hub
        .snapshot_events()
        .iter()
        .filter(|e| e.starts_with("resize|"))
        .count();
    let style_after_resize = unsafe { GetWindowLongPtrW(chr_hwnd, GWL_STYLE) as u32 };
    let chromium_reasserted_caption = (style_after_resize & (WS_CAPTION | WS_THICKFRAME)) != 0;
    hub.send_cmd("query_state");
    let state_after_resize = hub.wait_for_prefix("state|", Duration::from_secs(4));
    println!(
        "[2.2 Chromium Resize 20x] total_ms={} max_win_delta_px={c_max_win_delta} max_d3d_delta_px={c_max_d3d_delta} dom_resize_events={resize_events} reasserted_caption={chromium_reasserted_caption}",
        cr_start.elapsed().as_millis()
    );
    println!("  DOM state after resize: {:?}", state_after_resize);

    // 2.3 Focus Activation, Clicking Host Rail <-> Chromium Input, Keyboard & Vietnamese Unicode
    unsafe { clear_window_text(host.sidebar_edit); }
    unsafe { SetFocus(host.sidebar_edit); }
    pump_messages_for(Duration::from_millis(80));
    post_targeted_unicode_text(host.sidebar_edit, "CHR_RAIL1-");
    pump_messages_for(Duration::from_millis(80));

    let mut rend_r = RECT::default();
    unsafe { GetWindowRect(render_hwnd, &mut rend_r); }
    let wfp_rend = unsafe {
        WindowFromPoint(POINT {
            x: rend_r.left + 120,
            y: rend_r.top + 80,
        })
    };
    let wfp_rend_cls = get_class_name(wfp_rend);

    // Click inside #chat-input via targeted WM_LBUTTONDOWN/UP on Chrome_RenderWidgetHostHWND
    let inp_click = state_after_resize
        .as_deref()
        .and_then(|s| parse_array_4(s, "ir"))
        .map(|[l, t, w, h]| (((l + w / 2) as f64 * 1.25) as i32, ((t + h / 2) as f64 * 1.25) as i32))
        .unwrap_or((120, 75));

    hub.clear_events();
    let chr_setfocus_prev = unsafe { SetFocus(render_hwnd) };
    let chr_setfocus_err = unsafe { GetLastError() };
    post_targeted_click(render_hwnd, inp_click.0, inp_click.1, false);
    pump_messages_for(Duration::from_millis(220));
    let click_focus_evs: Vec<String> = hub
        .snapshot_events()
        .into_iter()
        .filter(|e| e.starts_with("click|") || e.starts_with("win_focus|") || e.starts_with("input_focus|"))
        .collect();

    // Also ensure DOM focus and send Vietnamese Unicode WM_CHAR to Chrome_RenderWidgetHostHWND
    hub.send_cmd("focus_input");
    pump_messages_for(Duration::from_millis(160));
    post_targeted_unicode_text(render_hwnd, vn_sample);
    pump_messages_for(Duration::from_millis(280));
    hub.send_cmd("query_state");
    let state_after_vn = hub.wait_for_prefix("state|", Duration::from_secs(4));

    let mut gti_chr = GUITHREADINFO::default();
    let chr_tid = unsafe { GetWindowThreadProcessId(chr_hwnd, std::ptr::null_mut()) };
    unsafe { GetGUIThreadInfo(chr_tid, &mut gti_chr); }

    // Return focus to host sidebar rail & type
    unsafe { SetFocus(host.sidebar_edit); }
    pump_messages_for(Duration::from_millis(100));
    let chr_focus_back_rail = unsafe { GetFocus() == host.sidebar_edit };
    post_targeted_unicode_text(host.sidebar_edit, "CHR_RAIL2");
    pump_messages_for(Duration::from_millis(80));
    let chr_rail_text = get_edit_text(host.sidebar_edit);

    println!(
        "[2.3 Chromium Focus & VN Input] render_hwnd=0x{:X} render_rect=({},{},{},{}) wfp_hit=0x{:X}('{}') SetFocus_prev=0x{:X}(err={chr_setfocus_err}) chr_gti_focus=0x{:X} focus_back_rail={chr_focus_back_rail} rail_text='{}'",
        render_hwnd as usize,
        rend_r.left, rend_r.top, rend_r.right, rend_r.bottom,
        wfp_rend as usize, wfp_rend_cls,
        chr_setfocus_prev as usize,
        gti_chr.hwndFocus as usize,
        chr_rail_text
    );
    println!("  click/focus events: {:?}", click_focus_evs);
    println!("  DOM state after VN Unicode type: {:?}", state_after_vn);

    // 2.4 Chromium Mouse Wheel Scroll (targeted WM_MOUSEWHEEL to Chrome_RenderWidgetHostHWND)
    hub.clear_events();
    let scroll_pt = state_after_vn
        .as_deref()
        .and_then(|s| parse_array_4(s, "sr"))
        .map(|[l, t, w, h]| {
            (
                rend_r.left + ((l + w / 2) as f64 * 1.25) as i32,
                rend_r.top + ((t + h / 2) as f64 * 1.25) as i32,
            )
        })
        .unwrap_or((rend_r.left + 150, rend_r.top + 220));
    post_targeted_wheel(render_hwnd, scroll_pt.0, scroll_pt.1, -360);
    pump_messages_for(Duration::from_millis(260));
    let wheel_ev = hub
        .snapshot_events()
        .into_iter()
        .find(|e| e.starts_with("wheel|"));
    println!("[2.4 Chromium Mouse Wheel] screen_pt=({},{}) event={:?}", scroll_pt.0, scroll_pt.1, wheel_ev);

    // 2.5a Chromium Right-Click Context Menu
    let tops_before_rclick: HashSet<usize> = list_top_windows().iter().map(|w| w.0 as usize).collect();
    hub.clear_events();
    post_targeted_click(render_hwnd, 180, 180, true);
    pump_messages_for(Duration::from_millis(380));
    let new_tops_rclick: Vec<_> = list_top_windows()
        .into_iter()
        .filter(|w| !tops_before_rclick.contains(&(w.0 as usize)) && w.6)
        .collect();
    let ctx_ev = hub
        .snapshot_events()
        .into_iter()
        .find(|e| e.starts_with("contextmenu|"));
    println!(
        "[2.5a Chromium Context Menu] dom_event={:?} new_visible_popup_windows={}",
        ctx_ev,
        new_tops_rclick.len()
    );
    for (h, pid, _, cls, title, r, vis, owner) in &new_tops_rclick {
        println!(
            "  CTX POPUP: hwnd=0x{:X} pid={pid} cls='{}' title='{}' vis={vis} rect=({},{},{},{}) owner=0x{:X}",
            *h as usize, cls, title, r.left, r.top, r.right, r.bottom, *owner as usize
        );
        unsafe { PostMessageW(*h, WM_CLOSE, 0, 0); }
    }
    unsafe { PostMessageW(render_hwnd, WM_KEYDOWN, 0x1B, 0); }
    pump_messages_for(Duration::from_millis(120));

    // 2.5b Chromium window.open() Popup
    let tops_before_popup: HashSet<usize> = list_top_windows().iter().map(|w| w.0 as usize).collect();
    hub.clear_events();
    hub.send_cmd("open_popup");
    let pop_ev = hub.wait_for_prefix("popup_loaded|", Duration::from_secs(3));
    pump_messages_for(Duration::from_millis(250));
    let new_tops_popup: Vec<_> = list_top_windows()
        .into_iter()
        .filter(|w| !tops_before_popup.contains(&(w.0 as usize)) && w.6 && w.3.contains("Chrome"))
        .collect();
    println!(
        "[2.5b Chromium window.open() Popup] dom_event={:?} new_top_windows={}",
        pop_ev,
        new_tops_popup.len()
    );
    for (h, pid, _, cls, title, r, vis, owner) in &new_tops_popup {
        let p = unsafe { GetAncestor(*h, GA_PARENT) };
        println!(
            "  POPUP WIN: hwnd=0x{:X} pid={pid} cls='{}' title='{}' vis={vis} ga_parent=0x{:X} owner=0x{:X} rect=({},{},{},{}) escaped_host={}",
            *h as usize, cls, title, p as usize, *owner as usize, r.left, r.top, r.right, r.bottom, p != host.container
        );
        unsafe { PostMessageW(*h, WM_CLOSE, 0, 0); }
    }
    pump_messages_for(Duration::from_millis(150));

    // 2.5c Chromium <input type="file"> Native Modal File Dialog
    if let Some([fl, ft, _fw, fh]) = state_after_vn.as_deref().and_then(|s| parse_array_4(s, "fr")) {
        let fx = ((fl + 35) as f64 * 1.25) as i32;
        let fy = ((ft + fh / 2) as f64 * 1.25) as i32;
        let tops_before_dlg: HashSet<usize> = list_top_windows().iter().map(|w| w.0 as usize).collect();
        post_targeted_click(render_hwnd, fx, fy, false);
        pump_messages_for(Duration::from_millis(700));
        let host_enabled_during_chr_dlg = unsafe { IsWindowEnabled(host.hwnd) != 0 };
        let new_dlgs: Vec<_> = list_top_windows()
            .into_iter()
            .filter(|w| !tops_before_dlg.contains(&(w.0 as usize)) && w.6 && w.3 == "#32770")
            .collect();
        println!(
            "[2.5c Chromium <input type=file> Dialog] client_click=({fx},{fy}) dlg_opened={} host_enabled_during_dlg={host_enabled_during_chr_dlg}",
            !new_dlgs.is_empty()
        );
        for (dh, dpid, _, dcls, dtitle, dr, _, downer) in &new_dlgs {
            println!(
                "  FILE DLG: hwnd=0x{:X} pid={dpid} cls='{dcls}' title='{dtitle}' rect=({},{},{},{}) owner=0x{:X}",
                *dh as usize, dr.left, dr.top, dr.right, dr.bottom, *downer as usize
            );
            unsafe { PostMessageW(*dh, WM_CLOSE, 0, 0); }
        }
        pump_messages_for(Duration::from_millis(300));
        let host_enabled_after_chr_dlg = unsafe { IsWindowEnabled(host.hwnd) != 0 };
        println!("  host_enabled_after_chr_dlg={host_enabled_after_chr_dlg}");
    } else {
        println!("[2.5c Chromium <input type=file> Dialog] skipped (no fr rect)");
    }

    // 2.6 Safe Detach & Restoration (Chromium)
    let (c_det_ok, c_det_err) = detach_window(&chr_snap);
    pump_messages_for(Duration::from_millis(250));
    let chr_after_detach = snapshot_window(chr_hwnd);
    hub.clear_events();
    hub.send_cmd("query_state");
    let state_after_detach = hub.wait_for_prefix("state|", Duration::from_secs(4));
    println!(
        "[2.6 Chromium Safe Detach] ok={c_det_ok} err={c_det_err} parent_null={} style_exact={} exstyle_exact={} rect_match={} visible={} dom_responsive={}",
        chr_after_detach.parent.is_null(),
        chr_after_detach.style == chr_snap.style,
        chr_after_detach.ex_style == chr_snap.ex_style,
        chr_after_detach.rect == chr_snap.rect,
        unsafe { IsWindowVisible(chr_hwnd) != 0 },
        state_after_detach.is_some()
    );

    // 2.7 DPI Mismatch Probe on Chromium Chrome_WidgetWin_1 (PMv2)
    println!("\n--- [2.7 DPI MISMATCH PROBE ON CHROMIUM Chrome_WidgetWin_1] ---");
    for (mode, mixed) in [
        ("pmv2", false),
        ("system", false),
        ("system", true),
        ("unaware", false),
        ("unaware", true),
    ] {
        let out = Command::new(&self_exe)
            .arg("--dpi-probe")
            .arg(mode)
            .arg(if mixed { "1" } else { "0" })
            .arg(format!("0x{:X}", chr_hwnd as usize))
            .output()
            .expect("run chromium dpi-probe");
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            println!("  {line}");
        }
    }

    // 2.8 HOST CRASH RECOVERY ON CHROMIUM FIXTURE (chr_hwnd is alive & detached!)
    run_host_crash_experiment(
        &self_exe,
        &temp_dir,
        "ChromiumFixture",
        chr_pid,
        chr_hwnd,
        None,
        &mut cleanup,
    );

    unsafe {
        DestroyWindow(host.hwnd);
    }
    println!("\n=== SUITE COMPLETE — RUNNING CLEANUP GUARD ===");
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.first().map(|s| s.as_str()) == Some("--dpi-probe") && args.len() >= 4 {
        let mode = &args[1];
        let mixed = args[2] == "1";
        let hwnd = parse_hex_hwnd(&args[3]);
        run_dpi_probe_submode(mode, mixed, hwnd);
        return;
    }
    if args.first().map(|s| s.as_str()) == Some("--crash-host") && args.len() >= 3 {
        let hwnd = parse_hex_hwnd(&args[1]);
        let ready = PathBuf::from(&args[2]);
        run_crash_host_submode(hwnd, &ready);
        return;
    }
    run_full_suite();
}

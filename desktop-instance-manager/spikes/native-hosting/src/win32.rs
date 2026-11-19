#![allow(non_camel_case_types, non_snake_case, dead_code, clippy::upper_case_acronyms)]

use std::ffi::c_void;
use std::mem::{size_of, zeroed};

pub type HWND = *mut c_void;
pub type HINSTANCE = *mut c_void;
pub type HMODULE = *mut c_void;
pub type HMENU = *mut c_void;
pub type HICON = *mut c_void;
pub type HCURSOR = *mut c_void;
pub type HBRUSH = *mut c_void;
pub type HFONT = *mut c_void;
pub type HDC = *mut c_void;
pub type HANDLE = *mut c_void;
pub type HIMC = *mut c_void;
pub type HDROP = *mut c_void;
pub type HMONITOR = *mut c_void;
pub type DPI_AWARENESS_CONTEXT = isize;
pub type WPARAM = usize;
pub type LPARAM = isize;
pub type LRESULT = isize;
pub type BOOL = i32;
pub type ATOM = u16;
pub type COLORREF = u32;

pub const TRUE: BOOL = 1;
pub const FALSE: BOOL = 0;

// Window styles
pub const WS_OVERLAPPED: u32 = 0x0000_0000;
pub const WS_POPUP: u32 = 0x8000_0000;
pub const WS_CHILD: u32 = 0x4000_0000;
pub const WS_MINIMIZE: u32 = 0x2000_0000;
pub const WS_VISIBLE: u32 = 0x1000_0000;
pub const WS_DISABLED: u32 = 0x0800_0000;
pub const WS_CLIPSIBLINGS: u32 = 0x0400_0000;
pub const WS_CLIPCHILDREN: u32 = 0x0200_0000;
pub const WS_MAXIMIZE: u32 = 0x0100_0000;
pub const WS_CAPTION: u32 = 0x00C0_0000;
pub const WS_BORDER: u32 = 0x0080_0000;
pub const WS_DLGFRAME: u32 = 0x0040_0000;
pub const WS_VSCROLL: u32 = 0x0020_0000;
pub const WS_HSCROLL: u32 = 0x0010_0000;
pub const WS_SYSMENU: u32 = 0x0008_0000;
pub const WS_THICKFRAME: u32 = 0x0004_0000;
pub const WS_GROUP: u32 = 0x0002_0000;
pub const WS_TABSTOP: u32 = 0x0001_0000;
pub const WS_MINIMIZEBOX: u32 = 0x0002_0000;
pub const WS_MAXIMIZEBOX: u32 = 0x0001_0000;
pub const WS_OVERLAPPEDWINDOW: u32 =
    WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX;

// Extended styles
pub const WS_EX_DLGMODALFRAME: u32 = 0x0000_0001;
pub const WS_EX_NOPARENTNOTIFY: u32 = 0x0000_0004;
pub const WS_EX_TOPMOST: u32 = 0x0000_0008;
pub const WS_EX_ACCEPTFILES: u32 = 0x0000_0010;
pub const WS_EX_TRANSPARENT: u32 = 0x0000_0020;
pub const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
pub const WS_EX_WINDOWEDGE: u32 = 0x0000_0100;
pub const WS_EX_CLIENTEDGE: u32 = 0x0000_0200;
pub const WS_EX_CONTROLPARENT: u32 = 0x0001_0000;
pub const WS_EX_APPWINDOW: u32 = 0x0004_0000;
pub const WS_EX_LAYERED: u32 = 0x0008_0000;
pub const WS_EX_NOACTIVATE: u32 = 0x0800_0000;
pub const WS_EX_NOREDIRECTIONBITMAP: u32 = 0x0020_0000;

// GWL indices
pub const GWL_STYLE: i32 = -16;
pub const GWL_EXSTYLE: i32 = -20;

// GetAncestor flags
pub const GA_PARENT: u32 = 1;
pub const GA_ROOT: u32 = 2;
pub const GA_ROOTOWNER: u32 = 3;

// GetWindow flags
pub const GW_OWNER: u32 = 4;

// ShowWindow flags
pub const SW_HIDE: i32 = 0;
pub const SW_SHOWNORMAL: i32 = 1;
pub const SW_SHOWMINIMIZED: i32 = 2;
pub const SW_SHOWMAXIMIZED: i32 = 3;
pub const SW_SHOWNOACTIVATE: i32 = 4;
pub const SW_SHOW: i32 = 5;
pub const SW_RESTORE: i32 = 9;

// SetWindowPos flags
pub const SWP_NOSIZE: u32 = 0x0001;
pub const SWP_NOMOVE: u32 = 0x0002;
pub const SWP_NOZORDER: u32 = 0x0004;
pub const SWP_NOREDRAW: u32 = 0x0008;
pub const SWP_NOACTIVATE: u32 = 0x0010;
pub const SWP_FRAMECHANGED: u32 = 0x0020;
pub const SWP_SHOWWINDOW: u32 = 0x0040;
pub const SWP_NOOWNERZORDER: u32 = 0x0200;
pub const SWP_ASYNCWINDOWPOS: u32 = 0x4000;

// Window messages
pub const WM_CREATE: u32 = 0x0001;
pub const WM_DESTROY: u32 = 0x0002;
pub const WM_MOVE: u32 = 0x0003;
pub const WM_SIZE: u32 = 0x0005;
pub const WM_ACTIVATE: u32 = 0x0006;
pub const WM_SETFOCUS: u32 = 0x0007;
pub const WM_KILLFOCUS: u32 = 0x0008;
pub const WM_PAINT: u32 = 0x000F;
pub const WM_CLOSE: u32 = 0x0010;
pub const WM_QUIT: u32 = 0x0012;
pub const WM_SHOWWINDOW: u32 = 0x0018;
pub const WM_MOUSEACTIVATE: u32 = 0x0021;
pub const WM_GETTEXT: u32 = 0x000D;
pub const WM_GETTEXTLENGTH: u32 = 0x000E;
pub const WM_SETTEXT: u32 = 0x000C;
pub const WM_NCDESTROY: u32 = 0x0082;
pub const WM_NCCALCSIZE: u32 = 0x0083;
pub const WM_NCHITTEST: u32 = 0x0084;
pub const WM_NCACTIVATE: u32 = 0x0086;
pub const WM_KEYDOWN: u32 = 0x0100;
pub const WM_KEYUP: u32 = 0x0101;
pub const WM_CHAR: u32 = 0x0102;
pub const WM_IME_STARTCOMPOSITION: u32 = 0x010D;
pub const WM_IME_ENDCOMPOSITION: u32 = 0x010E;
pub const WM_IME_COMPOSITION: u32 = 0x010F;
pub const WM_COMMAND: u32 = 0x0111;
pub const WM_TIMER: u32 = 0x0113;
pub const WM_LBUTTONDOWN: u32 = 0x0201;
pub const WM_LBUTTONUP: u32 = 0x0202;
pub const WM_RBUTTONUP: u32 = 0x0205;
pub const WM_MOUSEWHEEL: u32 = 0x020A;
pub const WM_DROPFILES: u32 = 0x0233;
pub const WM_IME_SETCONTEXT: u32 = 0x0281;
pub const WM_IME_NOTIFY: u32 = 0x0282;
pub const WM_DPICHANGED: u32 = 0x02E0;
pub const WM_DPICHANGED_BEFOREPARENT: u32 = 0x02E2;
pub const WM_DPICHANGED_AFTERPARENT: u32 = 0x02E3;
pub const WM_USER: u32 = 0x0400;
pub const WM_APP: u32 = 0x8000;

// Edit styles
pub const ES_LEFT: u32 = 0x0000;
pub const ES_MULTILINE: u32 = 0x0004;
pub const ES_AUTOVSCROLL: u32 = 0x0040;
pub const ES_AUTOHSCROLL: u32 = 0x0080;

// Button styles
pub const BS_PUSHBUTTON: u32 = 0x0000;

// Class styles
pub const CS_VREDRAW: u32 = 0x0001;
pub const CS_HREDRAW: u32 = 0x0002;

// PeekMessage
pub const PM_REMOVE: u32 = 0x0001;

// SendMessageTimeout flags
pub const SMTO_ABORTIFHUNG: u32 = 0x0002;

// TrackPopupMenu flags
pub const TPM_LEFTALIGN: u32 = 0x0000;
pub const TPM_TOPALIGN: u32 = 0x0000;
pub const TPM_NONOTIFY: u32 = 0x0080;
pub const TPM_RETURNCMD: u32 = 0x0100;
pub const MF_STRING: u32 = 0x0000;

// DPI Awareness Contexts
pub const DPI_AWARENESS_CONTEXT_UNAWARE: DPI_AWARENESS_CONTEXT = -1;
pub const DPI_AWARENESS_CONTEXT_SYSTEM_AWARE: DPI_AWARENESS_CONTEXT = -2;
pub const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE: DPI_AWARENESS_CONTEXT = -3;
pub const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: DPI_AWARENESS_CONTEXT = -4;
pub const DPI_AWARENESS_CONTEXT_UNAWARE_GDISCALED: DPI_AWARENESS_CONTEXT = -5;

// DPI Hosting Behavior
pub const DPI_HOSTING_BEHAVIOR_INVALID: i32 = -1;
pub const DPI_HOSTING_BEHAVIOR_DEFAULT: i32 = 0;
pub const DPI_HOSTING_BEHAVIOR_MIXED: i32 = 1;

// Monitor DPI
pub const MDT_EFFECTIVE_DPI: u32 = 0;
pub const MONITOR_DEFAULTTONEAREST: u32 = 2;

// Process / Token constants
pub const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
pub const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
pub const PROCESS_TERMINATE: u32 = 0x0001;
pub const SYNCHRONIZE: u32 = 0x0010_0000;
pub const TOKEN_QUERY: u32 = 0x0008;
pub const TOKEN_INTEGRITY_LEVEL: u32 = 25;
pub const INVALID_HANDLE_VALUE: HANDLE = (-1_isize) as HANDLE;
pub const WAIT_OBJECT_0: u32 = 0;
pub const WAIT_TIMEOUT: u32 = 0x0000_0102;
pub const STILL_ACTIVE: u32 = 259;

// DWM attributes (read-only)
pub const DWMWA_NCRENDERING_ENABLED: u32 = 1;
pub const DWMWA_EXTENDED_FRAME_BOUNDS: u32 = 9;
pub const DWMWA_CLOAKED: u32 = 14;
pub const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;

// SendInput
pub const INPUT_MOUSE: u32 = 0;
pub const INPUT_KEYBOARD: u32 = 1;
pub const KEYEVENTF_KEYUP: u32 = 0x0002;
pub const KEYEVENTF_UNICODE: u32 = 0x0004;
pub const MOUSEEVENTF_MOVE: u32 = 0x0001;
pub const MOUSEEVENTF_LEFTDOWN: u32 = 0x0002;
pub const MOUSEEVENTF_LEFTUP: u32 = 0x0004;
pub const MOUSEEVENTF_RIGHTDOWN: u32 = 0x0008;
pub const MOUSEEVENTF_RIGHTUP: u32 = 0x0010;
pub const MOUSEEVENTF_WHEEL: u32 = 0x0800;
pub const MOUSEEVENTF_ABSOLUTE: u32 = 0x8000;
pub const MOUSEEVENTF_VIRTUALDESK: u32 = 0x4000;

pub const SM_XVIRTUALSCREEN: i32 = 76;
pub const SM_YVIRTUALSCREEN: i32 = 77;
pub const SM_CXVIRTUALSCREEN: i32 = 78;
pub const SM_CYVIRTUALSCREEN: i32 = 79;

// IMM32
pub const SCS_SETSTR: u32 = 0x0009;
pub const CFS_POINT: u32 = 0x0002;
pub const CFS_FORCE_POSITION: u32 = 0x0020;

// OFN flags
pub const OFN_NOCHANGEDIR: u32 = 0x0000_0008;
pub const OFN_FILEMUSTEXIST: u32 = 0x0000_1000;
pub const OFN_EXPLORER: u32 = 0x0008_0000;

// Cursor
pub const IDC_ARROW: *const u16 = 32512 as *const u16;
pub const COLOR_WINDOW: usize = 5;
pub const COLOR_BTNFACE: usize = 15;

// Queue / GUIThreadInfo
pub const GUI_CARETBLINKING: u32 = 0x0000_0001;
pub const GUI_INMOVESIZE: u32 = 0x0000_0002;
pub const GUI_INMENUMODE: u32 = 0x0000_0004;
pub const GUI_POPUPMENUMODE: u32 = 0x0000_0010;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RECT {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl RECT {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct POINT {
    pub x: i32,
    pub y: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MSG {
    pub hwnd: HWND,
    pub message: u32,
    pub wParam: WPARAM,
    pub lParam: LPARAM,
    pub time: u32,
    pub pt: POINT,
    pub lPrivate: u32,
}

#[repr(C)]
pub struct WNDCLASSEXW {
    pub cbSize: u32,
    pub style: u32,
    pub lpfnWndProc: Option<unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT>,
    pub cbClsExtra: i32,
    pub cbWndExtra: i32,
    pub hInstance: HINSTANCE,
    pub hIcon: HICON,
    pub hCursor: HCURSOR,
    pub hbrBackground: HBRUSH,
    pub lpszMenuName: *const u16,
    pub lpszClassName: *const u16,
    pub hIconSm: HICON,
}

#[repr(C)]
pub struct PAINTSTRUCT {
    pub hdc: HDC,
    pub fErase: BOOL,
    pub rcPaint: RECT,
    pub fRestore: BOOL,
    pub fIncUpdate: BOOL,
    pub rgbReserved: [u8; 32],
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct WINDOWPLACEMENT {
    pub length: u32,
    pub flags: u32,
    pub showCmd: u32,
    pub ptMinPosition: POINT,
    pub ptMaxPosition: POINT,
    pub rcNormalPosition: RECT,
}

impl Default for WINDOWPLACEMENT {
    fn default() -> Self {
        let mut wp: Self = unsafe { zeroed() };
        wp.length = size_of::<Self>() as u32;
        wp
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct GUITHREADINFO {
    pub cbSize: u32,
    pub flags: u32,
    pub hwndActive: HWND,
    pub hwndFocus: HWND,
    pub hwndCapture: HWND,
    pub hwndMenuOwner: HWND,
    pub hwndMoveSize: HWND,
    pub hwndCaret: HWND,
    pub rcCaret: RECT,
}

impl Default for GUITHREADINFO {
    fn default() -> Self {
        let mut g: Self = unsafe { zeroed() };
        g.cbSize = size_of::<Self>() as u32;
        g
    }
}

#[repr(C)]
pub struct PROCESSENTRY32W {
    pub dwSize: u32,
    pub cntUsage: u32,
    pub th32ProcessID: u32,
    pub th32DefaultHeapID: usize,
    pub th32ModuleID: u32,
    pub cntThreads: u32,
    pub th32ParentProcessID: u32,
    pub pcPriClassBase: i32,
    pub dwFlags: u32,
    pub szExeFile: [u16; 260],
}

#[repr(C)]
pub struct SID_AND_ATTRIBUTES {
    pub Sid: *mut c_void,
    pub Attributes: u32,
}

#[repr(C)]
pub struct TOKEN_MANDATORY_LABEL {
    pub Label: SID_AND_ATTRIBUTES,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MOUSEINPUT {
    pub dx: i32,
    pub dy: i32,
    pub mouseData: u32,
    pub dwFlags: u32,
    pub time: u32,
    pub dwExtraInfo: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct KEYBDINPUT {
    pub wVk: u16,
    pub wScan: u16,
    pub dwFlags: u32,
    pub time: u32,
    pub dwExtraInfo: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct HARDWAREINPUT {
    pub uMsg: u32,
    pub wParamL: u16,
    pub wParamH: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union INPUT_UNION {
    pub mi: MOUSEINPUT,
    pub ki: KEYBDINPUT,
    pub hi: HARDWAREINPUT,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct INPUT {
    pub r#type: u32,
    pub u: INPUT_UNION,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct COMPOSITIONFORM {
    pub dwStyle: u32,
    pub ptCurrentPos: POINT,
    pub rcArea: RECT,
}

#[repr(C)]
pub struct OPENFILENAMEW {
    pub lStructSize: u32,
    pub hwndOwner: HWND,
    pub hInstance: HINSTANCE,
    pub lpstrFilter: *const u16,
    pub lpstrCustomFilter: *mut u16,
    pub nMaxCustFilter: u32,
    pub nFilterIndex: u32,
    pub lpstrFile: *mut u16,
    pub nMaxFile: u32,
    pub lpstrFileTitle: *mut u16,
    pub nMaxFileTitle: u32,
    pub lpstrInitialDir: *const u16,
    pub lpstrTitle: *const u16,
    pub Flags: u32,
    pub nFileOffset: u16,
    pub nFileExtension: u16,
    pub lpstrDefExt: *const u16,
    pub lCustData: LPARAM,
    pub lpfnHook: *mut c_void,
    pub lpTemplateName: *const u16,
    pub pvReserved: *mut c_void,
    pub dwReserved: u32,
    pub FlagsEx: u32,
}

#[link(name = "user32")]
extern "system" {
    pub fn EnumWindows(
        lpEnumFunc: unsafe extern "system" fn(HWND, LPARAM) -> BOOL,
        lParam: LPARAM,
    ) -> BOOL;
    pub fn EnumChildWindows(
        hWndParent: HWND,
        lpEnumFunc: unsafe extern "system" fn(HWND, LPARAM) -> BOOL,
        lParam: LPARAM,
    ) -> BOOL;
    pub fn EnumThreadWindows(
        dwThreadId: u32,
        lpfn: unsafe extern "system" fn(HWND, LPARAM) -> BOOL,
        lParam: LPARAM,
    ) -> BOOL;
    pub fn GetWindowThreadProcessId(hWnd: HWND, lpdwProcessId: *mut u32) -> u32;
    pub fn GetClassNameW(hWnd: HWND, lpClassName: *mut u16, nMaxCount: i32) -> i32;
    pub fn GetWindowTextW(hWnd: HWND, lpString: *mut u16, nMaxCount: i32) -> i32;
    pub fn GetWindowLongPtrW(hWnd: HWND, nIndex: i32) -> isize;
    pub fn SetWindowLongPtrW(hWnd: HWND, nIndex: i32, dwNewLong: isize) -> isize;
    pub fn GetWindowRect(hWnd: HWND, lpRect: *mut RECT) -> BOOL;
    pub fn GetClientRect(hWnd: HWND, lpRect: *mut RECT) -> BOOL;
    pub fn ClientToScreen(hWnd: HWND, lpPoint: *mut POINT) -> BOOL;
    pub fn ScreenToClient(hWnd: HWND, lpPoint: *mut POINT) -> BOOL;
    pub fn IsWindow(hWnd: HWND) -> BOOL;
    pub fn IsWindowVisible(hWnd: HWND) -> BOOL;
    pub fn IsWindowEnabled(hWnd: HWND) -> BOOL;
    pub fn IsIconic(hWnd: HWND) -> BOOL;
    pub fn IsZoomed(hWnd: HWND) -> BOOL;
    pub fn GetParent(hWnd: HWND) -> HWND;
    pub fn SetParent(hWndChild: HWND, hWndNewParent: HWND) -> HWND;
    pub fn GetAncestor(hwnd: HWND, gaFlags: u32) -> HWND;
    pub fn GetWindow(hWnd: HWND, uCmd: u32) -> HWND;
    pub fn GetWindowPlacement(hWnd: HWND, lpwndpl: *mut WINDOWPLACEMENT) -> BOOL;
    pub fn SetWindowPlacement(hWnd: HWND, lpwndpl: *const WINDOWPLACEMENT) -> BOOL;
    pub fn SetWindowPos(
        hWnd: HWND,
        hWndInsertAfter: HWND,
        X: i32,
        Y: i32,
        cx: i32,
        cy: i32,
        uFlags: u32,
    ) -> BOOL;
    pub fn MoveWindow(
        hWnd: HWND,
        X: i32,
        Y: i32,
        nWidth: i32,
        nHeight: i32,
        bRepaint: BOOL,
    ) -> BOOL;
    pub fn ShowWindow(hWnd: HWND, nCmdShow: i32) -> BOOL;
    pub fn UpdateWindow(hWnd: HWND) -> BOOL;
    pub fn InvalidateRect(hWnd: HWND, lpRect: *const RECT, bErase: BOOL) -> BOOL;
    pub fn DestroyWindow(hWnd: HWND) -> BOOL;
    pub fn SetForegroundWindow(hWnd: HWND) -> BOOL;
    pub fn GetForegroundWindow() -> HWND;
    pub fn SetFocus(hWnd: HWND) -> HWND;
    pub fn GetFocus() -> HWND;
    pub fn GetActiveWindow() -> HWND;
    pub fn GetGUIThreadInfo(idThread: u32, pgui: *mut GUITHREADINFO) -> BOOL;
    pub fn AttachThreadInput(idAttach: u32, idAttachTo: u32, fAttach: BOOL) -> BOOL;
    pub fn WindowFromPoint(Point: POINT) -> HWND;
    pub fn GetPropW(hWnd: HWND, lpString: *const u16) -> HANDLE;
    pub fn EnumPropsW(
        hWnd: HWND,
        lpEnumFunc: unsafe extern "system" fn(HWND, *const u16, HANDLE) -> BOOL,
    ) -> i32;

    // DPI APIs
    pub fn SetProcessDpiAwarenessContext(value: DPI_AWARENESS_CONTEXT) -> BOOL;
    pub fn SetThreadDpiAwarenessContext(dpiContext: DPI_AWARENESS_CONTEXT) -> DPI_AWARENESS_CONTEXT;
    pub fn SetThreadDpiHostingBehavior(value: i32) -> i32;
    pub fn GetWindowDpiAwarenessContext(hwnd: HWND) -> DPI_AWARENESS_CONTEXT;
    pub fn GetAwarenessFromDpiAwarenessContext(value: DPI_AWARENESS_CONTEXT) -> i32;
    pub fn AreDpiAwarenessContextsEqual(
        dpiContextA: DPI_AWARENESS_CONTEXT,
        dpiContextB: DPI_AWARENESS_CONTEXT,
    ) -> BOOL;
    pub fn GetDpiForWindow(hwnd: HWND) -> u32;
    pub fn GetDpiForSystem() -> u32;
    pub fn GetWindowDpiHostingBehavior(hwnd: HWND) -> i32;
    pub fn MonitorFromWindow(hwnd: HWND, dwFlags: u32) -> HMONITOR;
    pub fn GetSystemMetrics(nIndex: i32) -> i32;

    // Window creation / message loop
    pub fn RegisterClassExW(lpwcx: *const WNDCLASSEXW) -> ATOM;
    pub fn CreateWindowExW(
        dwExStyle: u32,
        lpClassName: *const u16,
        lpWindowName: *const u16,
        dwStyle: u32,
        X: i32,
        Y: i32,
        nWidth: i32,
        nHeight: i32,
        hWndParent: HWND,
        hMenu: HMENU,
        hInstance: HINSTANCE,
        lpParam: *mut c_void,
    ) -> HWND;
    pub fn DefWindowProcW(hWnd: HWND, Msg: u32, wParam: WPARAM, lParam: LPARAM) -> LRESULT;
    pub fn GetMessageW(lpMsg: *mut MSG, hWnd: HWND, wMsgFilterMin: u32, wMsgFilterMax: u32) -> BOOL;
    pub fn PeekMessageW(
        lpMsg: *mut MSG,
        hWnd: HWND,
        wMsgFilterMin: u32,
        wMsgFilterMax: u32,
        wRemoveMsg: u32,
    ) -> BOOL;
    pub fn TranslateMessage(lpMsg: *const MSG) -> BOOL;
    pub fn DispatchMessageW(lpMsg: *const MSG) -> LRESULT;
    pub fn PostQuitMessage(nExitCode: i32);
    pub fn PostMessageW(hWnd: HWND, Msg: u32, wParam: WPARAM, lParam: LPARAM) -> BOOL;
    pub fn SendMessageW(hWnd: HWND, Msg: u32, wParam: WPARAM, lParam: LPARAM) -> LRESULT;
    pub fn SendMessageTimeoutW(
        hWnd: HWND,
        Msg: u32,
        wParam: WPARAM,
        lParam: LPARAM,
        fuFlags: u32,
        uTimeout: u32,
        lpdwResult: *mut usize,
    ) -> LRESULT;
    pub fn LoadCursorW(hInstance: HINSTANCE, lpCursorName: *const u16) -> HCURSOR;
    pub fn BeginPaint(hWnd: HWND, lpPaint: *mut PAINTSTRUCT) -> HDC;
    pub fn EndPaint(hWnd: HWND, lpPaint: *const PAINTSTRUCT) -> BOOL;
    pub fn FillRect(hDC: HDC, lprc: *const RECT, hbr: HBRUSH) -> i32;
    pub fn CreatePopupMenu() -> HMENU;
    pub fn AppendMenuW(
        hMenu: HMENU,
        uFlags: u32,
        uIDNewItem: usize,
        lpNewItem: *const u16,
    ) -> BOOL;
    pub fn TrackPopupMenu(
        hMenu: HMENU,
        uFlags: u32,
        x: i32,
        y: i32,
        nReserved: i32,
        hWnd: HWND,
        prcRect: *const RECT,
    ) -> BOOL;
    pub fn DestroyMenu(hMenu: HMENU) -> BOOL;
    pub fn SendInput(cInputs: u32, pInputs: *const INPUT, cbSize: i32) -> u32;
    pub fn SetCursorPos(X: i32, Y: i32) -> BOOL;
    pub fn GetKeyboardLayout(idThread: u32) -> isize;
    pub fn GetKeyboardLayoutList(nBuff: i32, lpList: *mut isize) -> i32;
    pub fn SetTimer(
        hWnd: HWND,
        nIDEvent: usize,
        uElapse: u32,
        lpTimerFunc: *mut c_void,
    ) -> usize;
    pub fn KillTimer(hWnd: HWND, uIDEvent: usize) -> BOOL;
}

#[link(name = "gdi32")]
extern "system" {
    pub fn CreateSolidBrush(color: COLORREF) -> HBRUSH;
    pub fn DeleteObject(ho: *mut c_void) -> BOOL;
    pub fn SetBkMode(hdc: HDC, mode: i32) -> i32;
    pub fn SetTextColor(hdc: HDC, color: COLORREF) -> COLORREF;
    pub fn TextOutW(hdc: HDC, x: i32, y: i32, lpString: *const u16, c: i32) -> BOOL;
}

#[link(name = "kernel32")]
extern "system" {
    pub fn GetModuleHandleW(lpModuleName: *const u16) -> HMODULE;
    pub fn GetLastError() -> u32;
    pub fn SetLastError(dwErrCode: u32);
    pub fn GetCurrentProcessId() -> u32;
    pub fn GetCurrentThreadId() -> u32;
    pub fn CreateToolhelp32Snapshot(dwFlags: u32, th32ProcessID: u32) -> HANDLE;
    pub fn Process32FirstW(hSnapshot: HANDLE, lppe: *mut PROCESSENTRY32W) -> BOOL;
    pub fn Process32NextW(hSnapshot: HANDLE, lppe: *mut PROCESSENTRY32W) -> BOOL;
    pub fn OpenProcess(dwDesiredAccess: u32, bInheritHandle: BOOL, dwProcessId: u32) -> HANDLE;
    pub fn TerminateProcess(hProcess: HANDLE, uExitCode: u32) -> BOOL;
    pub fn WaitForSingleObject(hHandle: HANDLE, dwMilliseconds: u32) -> u32;
    pub fn GetExitCodeProcess(hProcess: HANDLE, lpExitCode: *mut u32) -> BOOL;
    pub fn CloseHandle(hObject: HANDLE) -> BOOL;
    pub fn QueryFullProcessImageNameW(
        hProcess: HANDLE,
        dwFlags: u32,
        lpExeName: *mut u16,
        lpdwSize: *mut u32,
    ) -> BOOL;
}

#[link(name = "advapi32")]
extern "system" {
    pub fn OpenProcessToken(
        ProcessHandle: HANDLE,
        DesiredAccess: u32,
        TokenHandle: *mut HANDLE,
    ) -> BOOL;
    pub fn GetTokenInformation(
        TokenHandle: HANDLE,
        TokenInformationClass: u32,
        TokenInformation: *mut c_void,
        TokenInformationLength: u32,
        ReturnLength: *mut u32,
    ) -> BOOL;
    pub fn GetSidSubAuthorityCount(pSid: *mut c_void) -> *mut u8;
    pub fn GetSidSubAuthority(pSid: *mut c_void, nSubAuthority: u32) -> *mut u32;
}

#[link(name = "dwmapi")]
extern "system" {
    pub fn DwmGetWindowAttribute(
        hwnd: HWND,
        dwAttribute: u32,
        pvAttribute: *mut c_void,
        cbAttribute: u32,
    ) -> i32;
}

#[link(name = "shcore")]
extern "system" {
    pub fn GetDpiForMonitor(
        hmonitor: HMONITOR,
        dpiType: u32,
        dpiX: *mut u32,
        dpiY: *mut u32,
    ) -> i32;
}

#[link(name = "shell32")]
extern "system" {
    pub fn DragAcceptFiles(hWnd: HWND, fAccept: BOOL);
    pub fn DragQueryFileW(hDrop: HDROP, iFile: u32, lpszFile: *mut u16, cch: u32) -> u32;
    pub fn DragFinish(hDrop: HDROP);
}

#[link(name = "imm32")]
extern "system" {
    pub fn ImmGetContext(hWnd: HWND) -> HIMC;
    pub fn ImmReleaseContext(hWnd: HWND, hIMC: HIMC) -> BOOL;
    pub fn ImmGetCompositionWindow(hIMC: HIMC, lpCompForm: *mut COMPOSITIONFORM) -> BOOL;
    pub fn ImmSetCompositionWindow(hIMC: HIMC, lpCompForm: *const COMPOSITIONFORM) -> BOOL;
    pub fn ImmSetCompositionStringW(
        hIMC: HIMC,
        dwIndex: u32,
        lpComp: *const c_void,
        dwCompLen: u32,
        lpRead: *const c_void,
        dwReadLen: u32,
    ) -> BOOL;
    pub fn ImmGetOpenStatus(hIMC: HIMC) -> BOOL;
}

#[link(name = "comdlg32")]
extern "system" {
    pub fn GetOpenFileNameW(lpofn: *mut OPENFILENAMEW) -> BOOL;
}

pub fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn from_wide_buf(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

pub fn get_class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if len <= 0 {
        String::new()
    } else {
        String::from_utf16_lossy(&buf[..len as usize])
    }
}

pub fn get_window_text_readonly(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    let len = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if len <= 0 {
        String::new()
    } else {
        String::from_utf16_lossy(&buf[..len as usize])
    }
}

pub fn describe_dpi_context(ctx: DPI_AWARENESS_CONTEXT) -> String {
    unsafe {
        if AreDpiAwarenessContextsEqual(ctx, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) != 0 {
            return "PER_MONITOR_AWARE_V2".to_string();
        }
        if AreDpiAwarenessContextsEqual(ctx, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE) != 0 {
            return "PER_MONITOR_AWARE_V1".to_string();
        }
        if AreDpiAwarenessContextsEqual(ctx, DPI_AWARENESS_CONTEXT_SYSTEM_AWARE) != 0 {
            return "SYSTEM_AWARE".to_string();
        }
        if AreDpiAwarenessContextsEqual(ctx, DPI_AWARENESS_CONTEXT_UNAWARE_GDISCALED) != 0 {
            return "UNAWARE_GDISCALED".to_string();
        }
        if AreDpiAwarenessContextsEqual(ctx, DPI_AWARENESS_CONTEXT_UNAWARE) != 0 {
            return "UNAWARE".to_string();
        }
        let aw = GetAwarenessFromDpiAwarenessContext(ctx);
        match aw {
            0 => format!("UNAWARE(raw={ctx})"),
            1 => format!("SYSTEM_AWARE(raw={ctx})"),
            2 => format!("PER_MONITOR_AWARE(raw={ctx})"),
            _ => format!("UNKNOWN(aw={aw},raw={ctx})"),
        }
    }
}

pub fn describe_style(style: u32) -> String {
    let flags = [
        (WS_POPUP, "WS_POPUP"),
        (WS_CHILD, "WS_CHILD"),
        (WS_MINIMIZE, "WS_MINIMIZE"),
        (WS_VISIBLE, "WS_VISIBLE"),
        (WS_DISABLED, "WS_DISABLED"),
        (WS_CLIPSIBLINGS, "WS_CLIPSIBLINGS"),
        (WS_CLIPCHILDREN, "WS_CLIPCHILDREN"),
        (WS_MAXIMIZE, "WS_MAXIMIZE"),
        (WS_BORDER, "WS_BORDER"),
        (WS_DLGFRAME, "WS_DLGFRAME"),
        (WS_SYSMENU, "WS_SYSMENU"),
        (WS_THICKFRAME, "WS_THICKFRAME"),
        (WS_MINIMIZEBOX, "WS_MINIMIZEBOX"),
        (WS_MAXIMIZEBOX, "WS_MAXIMIZEBOX"),
    ];
    let names: Vec<&str> = flags
        .iter()
        .filter(|(bit, _)| (style & *bit) != 0)
        .map(|(_, name)| *name)
        .collect();
    format!("0x{style:08X} [{}]", names.join("|"))
}

pub fn describe_exstyle(ex: u32) -> String {
    let flags = [
        (WS_EX_DLGMODALFRAME, "DLGMODALFRAME"),
        (WS_EX_NOPARENTNOTIFY, "NOPARENTNOTIFY"),
        (WS_EX_TOPMOST, "TOPMOST"),
        (WS_EX_ACCEPTFILES, "ACCEPTFILES"),
        (WS_EX_TRANSPARENT, "TRANSPARENT"),
        (WS_EX_TOOLWINDOW, "TOOLWINDOW"),
        (WS_EX_WINDOWEDGE, "WINDOWEDGE"),
        (WS_EX_CLIENTEDGE, "CLIENTEDGE"),
        (WS_EX_CONTROLPARENT, "CONTROLPARENT"),
        (WS_EX_APPWINDOW, "APPWINDOW"),
        (WS_EX_LAYERED, "LAYERED"),
        (WS_EX_NOREDIRECTIONBITMAP, "NOREDIRECTIONBITMAP"),
        (WS_EX_NOACTIVATE, "NOACTIVATE"),
    ];
    let names: Vec<&str> = flags
        .iter()
        .filter(|(bit, _)| (ex & *bit) != 0)
        .map(|(_, name)| *name)
        .collect();
    format!("0x{ex:08X} [{}]", names.join("|"))
}

pub fn query_process_integrity(pid: u32) -> String {
    unsafe {
        let hproc = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid);
        if hproc.is_null() {
            return format!("OPEN_FAILED(err={})", GetLastError());
        }
        let mut htok: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(hproc, TOKEN_QUERY, &mut htok) == 0 {
            let err = GetLastError();
            CloseHandle(hproc);
            return format!("TOKEN_FAILED(err={err})");
        }
        let mut needed: u32 = 0;
        GetTokenInformation(htok, TOKEN_INTEGRITY_LEVEL, std::ptr::null_mut(), 0, &mut needed);
        let mut buf = vec![0u8; needed.max(64) as usize];
        let ok = GetTokenInformation(
            htok,
            TOKEN_INTEGRITY_LEVEL,
            buf.as_mut_ptr() as *mut c_void,
            buf.len() as u32,
            &mut needed,
        );
        let result = if ok != 0 {
            let tml = &*(buf.as_ptr() as *const TOKEN_MANDATORY_LABEL);
            let count = *GetSidSubAuthorityCount(tml.Label.Sid);
            let rid = *GetSidSubAuthority(tml.Label.Sid, (count as u32).saturating_sub(1));
            match rid {
                0x0000 => "Untrusted(0x0000)".to_string(),
                0x1000 => "Low(0x1000)".to_string(),
                0x2000 => "Medium(0x2000)".to_string(),
                0x2100 => "MediumPlus(0x2100)".to_string(),
                0x3000 => "High(0x3000)".to_string(),
                0x4000 => "System(0x4000)".to_string(),
                other => format!("RID(0x{other:04X})"),
            }
        } else {
            format!("GETINFO_FAILED(err={})", GetLastError())
        };
        CloseHandle(htok);
        CloseHandle(hproc);
        result
    }
}

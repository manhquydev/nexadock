//! Isolated Zalo Web workspace powered by raw `wry::WebViewBuilder::build_as_child`.
//!
//! Security & lifecycle invariants:
//! - Zero Tauri IPC (`with_ipc_handler` and Tauri init scripts are NEVER attached).
//! - Strict URL allowlist (`https:` on `zalo.me`, `*.zalo.me`, `*.zaloapp.com`, plus `about:blank`).
//! - Disallowed `https:` links and `window.open` requests hand off cleanly to the OS browser.
//! - Unprompted downloads are denied by default; non-media/notification permissions are denied.
//! - `wry::WebView` is `!Send`/`!Sync` and lives in a main-thread `thread_local!`, with `WebView`
//!   dropped before `WebContext`, and all children cleared before window close or tray quit.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, LazyLock, Mutex, OnceLock};
use std::thread::ThreadId;

use dim_core::{DimError, WebSessionRecord};
#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use serde::Serialize;
use tauri::{AppHandle, Manager, WebviewWindow};
use url::{Host, Url};
use wry::{
    dpi::{PhysicalPosition, PhysicalSize},
    NewWindowResponse, PermissionKind, PermissionResponse, Rect, WebContext, WebView,
    WebViewBuilder,
};

#[cfg(target_os = "macos")]
use wry::{WebViewBuilderExtDarwin, WebViewExtDarwin};

#[cfg(target_os = "windows")]
use webview2_com::{
    take_pwstr,
    Microsoft::Web::WebView2::Win32::{
        ICoreWebView2Environment, ICoreWebView2Profile8, ICoreWebView2_13,
        COREWEBVIEW2_PROCESS_FAILED_KIND, COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE,
    },
    NavigationStartingEventHandler, ProcessFailedEventHandler, ProfileDeletedEventHandler,
};
#[cfg(target_os = "windows")]
use windows_core::{Interface, PWSTR};
#[cfg(target_os = "windows")]
use wry::{WebViewBuilderExtWindows, WebViewExtWindows};

pub const ZALO_WEB_ENTRY_URL: &str = "https://chat.zalo.me";

static MAIN_THREAD_ID: OnceLock<ThreadId> = OnceLock::new();
static MOUNTED_SESSION_IDS: LazyLock<Mutex<HashSet<String>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

/// Records the main/UI thread ID during Tauri `setup()` so main-thread dispatch never deadlocks.
pub fn register_main_thread() {
    let _ = MAIN_THREAD_ID.set(std::thread::current().id());
}

/// Thread-safe query returning whether an isolated web session currently has a live mounted WebView.
pub fn is_session_mounted(id: &str) -> bool {
    MOUNTED_SESSION_IDS
        .lock()
        .map(|guard| guard.contains(id))
        .unwrap_or(false)
}

fn set_session_mounted_flag(id: &str, mounted: bool) {
    if let Ok(mut guard) = MOUNTED_SESSION_IDS.lock() {
        if mounted {
            guard.insert(id.to_owned());
        } else {
            guard.remove(id);
        }
    }
}

fn clear_all_mounted_flags() {
    if let Ok(mut guard) = MOUNTED_SESSION_IDS.lock() {
        guard.clear();
    }
}

#[cfg(test)]
pub(crate) fn set_session_mounted_for_test(id: &str, mounted: bool) {
    set_session_mounted_flag(id, mounted);
}

fn ensure_valid_session_id(id: &str) -> Result<(), DimError> {
    if !crate::state::is_valid_web_session_id(id) {
        return Err(DimError::InvalidProfileName(format!(
            "invalid web session id '{id}'"
        )));
    }
    Ok(())
}

fn is_on_main_thread() -> bool {
    MAIN_THREAD_ID
        .get()
        .is_some_and(|id| *id == std::thread::current().id())
}

fn run_on_main_sync<R, T, F>(app: &AppHandle<R>, f: F) -> Result<T, DimError>
where
    R: tauri::Runtime,
    T: Send + 'static,
    F: FnOnce() -> Result<T, DimError> + Send + 'static,
{
    if is_on_main_thread() {
        return f();
    }
    let (tx, rx) = mpsc::sync_channel(1);
    app.run_on_main_thread(move || {
        let res = f();
        let _ = tx.send(res);
    })
    .map_err(|e| DimError::ProcessError(format!("failed to schedule on main thread: {e}")))?;

    rx.recv().map_err(|_| {
        DimError::ProcessError("main thread dropped web workspace task before replying".to_owned())
    })?
}

/// Platform capability status for the isolated WebWorkspace.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WebWorkspaceSupport {
    pub supported: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
}

/// Detects whether raw child `wry::WebView` embedding is supported on the current OS/display.
pub fn detect_support(window: Option<&WebviewWindow>) -> WebWorkspaceSupport {
    #[cfg(target_os = "windows")]
    {
        let _ = window;
        WebWorkspaceSupport {
            supported: true,
            reason_code: None,
        }
    }

    #[cfg(target_os = "macos")]
    {
        let _ = window;
        if macos_major_version().is_some_and(|major| major >= 14) {
            WebWorkspaceSupport {
                supported: true,
                reason_code: None,
            }
        } else {
            WebWorkspaceSupport {
                supported: false,
                reason_code: Some("WEB_WORKSPACE_REQUIRES_MACOS_14".to_owned()),
            }
        }
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        if let Some(win) = window {
            if let Ok(handle) = win.window_handle() {
                if matches!(handle.as_raw(), RawWindowHandle::Xlib(_)) {
                    return WebWorkspaceSupport {
                        supported: true,
                        reason_code: None,
                    };
                }
            }
        }
        WebWorkspaceSupport {
            supported: false,
            reason_code: Some("WEB_WORKSPACE_WAYLAND_UNSUPPORTED".to_owned()),
        }
    }
}

#[cfg(target_os = "macos")]
fn macos_major_version() -> Option<u32> {
    static MACOS_MAJOR: OnceLock<Option<u32>> = OnceLock::new();
    *MACOS_MAJOR.get_or_init(|| {
        let output = std::process::Command::new("sw_vers")
            .arg("-productVersion")
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        text.trim().split('.').next()?.parse::<u32>().ok()
    })
}

/// Returns `true` only for `about:blank` or `https://` URLs on `zalo.me`, `*.zalo.me`, or `*.zaloapp.com`.
pub fn is_allowed_zalo_web_url(raw: &str) -> bool {
    if raw == "about:blank" {
        return true;
    }
    let Ok(parsed) = Url::parse(raw) else {
        return false;
    };
    if !parsed.scheme().eq_ignore_ascii_case("https") {
        return false;
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return false;
    }
    let Some(Host::Domain(domain)) = parsed.host() else {
        return false;
    };
    let host = domain.trim_end_matches('.').to_ascii_lowercase();
    host == "zalo.me" || host.ends_with(".zalo.me") || host.ends_with(".zaloapp.com")
}

/// Returns `true` for subframe navigations permitted inside Zalo Web (`about:blank`, `about:srcdoc`,
/// allowed Zalo `https://` origins, or `blob:` URLs scoped to an allowed Zalo `https://` origin).
#[allow(dead_code)]
pub fn is_allowed_zalo_web_subframe_url(raw: &str) -> bool {
    if raw == "about:srcdoc" || is_allowed_zalo_web_url(raw) {
        return true;
    }
    if let Some(inner) = raw.strip_prefix("blob:") {
        return is_allowed_zalo_web_url(inner);
    }
    false
}

/// Validates and canonicalizes an external `https://` URL before handing off to the OS browser.
///
/// Rejects URLs > 2048 bytes, control/whitespace/quote characters, non-`https` schemes,
/// embedded credentials, `localhost` / `*.localhost`, and loopback/unspecified/link-local IPs.
pub fn sanitize_external_https_url(raw: &str) -> Option<String> {
    if raw.is_empty() || raw.len() > 2048 {
        return None;
    }
    if raw
        .chars()
        .any(|c| c.is_control() || c.is_whitespace() || c == '"' || c == '\'' || c == '`')
    {
        return None;
    }
    let parsed = Url::parse(raw).ok()?;
    if !parsed.scheme().eq_ignore_ascii_case("https") {
        return None;
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return None;
    }
    match parsed.host()? {
        Host::Domain(domain) => {
            let host = domain.trim_end_matches('.').to_ascii_lowercase();
            if host.is_empty() || host == "localhost" || host.ends_with(".localhost") {
                return None;
            }
        }
        Host::Ipv4(ip) => {
            if ip.is_loopback() || ip.is_unspecified() || ip.is_link_local() {
                return None;
            }
        }
        Host::Ipv6(ip) => {
            let is_link_local = (ip.segments()[0] & 0xffc0) == 0xfe80;
            if ip.is_loopback() || ip.is_unspecified() || is_link_local {
                return None;
            }
            if let Some(v4) = ip.to_ipv4_mapped() {
                if v4.is_loopback() || v4.is_unspecified() || v4.is_link_local() {
                    return None;
                }
            }
        }
    }
    let canonical = parsed.as_str();
    if canonical.len() > 2048 {
        return None;
    }
    Some(canonical.to_owned())
}

/// Returns `true` for external `https://` URLs that may be opened safely in the OS default browser.
pub fn is_safe_external_https_url(raw: &str) -> bool {
    sanitize_external_https_url(raw).is_some()
}

#[cfg(target_os = "windows")]
fn open_url_windows_shell_execute(url: &str) {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;

    #[link(name = "shell32")]
    extern "system" {
        fn ShellExecuteW(
            hwnd: *mut std::ffi::c_void,
            lp_operation: *const u16,
            lp_file: *const u16,
            lp_parameters: *const u16,
            lp_directory: *const u16,
            n_show_cmd: i32,
        ) -> *mut std::ffi::c_void;
    }

    const SW_SHOWNORMAL: i32 = 1;
    let op: Vec<u16> = OsStr::new("open").encode_wide().chain(Some(0)).collect();
    let file: Vec<u16> = OsStr::new(url).encode_wide().chain(Some(0)).collect();
    unsafe {
        let _ = ShellExecuteW(
            ptr::null_mut(),
            op.as_ptr(),
            file.as_ptr(),
            ptr::null(),
            ptr::null(),
            SW_SHOWNORMAL,
        );
    }
}

fn open_in_os_browser(raw: &str) {
    let Some(sanitized) = sanitize_external_https_url(raw) else {
        return;
    };
    #[cfg(target_os = "windows")]
    {
        open_url_windows_shell_execute(&sanitized);
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(&sanitized).spawn();
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open")
            .arg(&sanitized)
            .spawn();
    }
}

/// Computes the Windows WebView2 profile name (`khoang_web_<id>_g<gen>`, `[a-z0-9_-]{1,64}`).
#[allow(dead_code)]
pub fn windows_profile_name(id: &str, generation: u32) -> String {
    let sanitized: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    let suffix = format!("_g{generation}");
    let prefix = "khoang_web_";
    let max_id_len = 64usize.saturating_sub(prefix.len() + suffix.len());
    let truncated: String = sanitized.chars().take(max_id_len).collect();
    format!("{prefix}{truncated}{suffix}")
}

#[allow(dead_code)]
fn is_valid_windows_profile_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.starts_with("khoang_web_")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[cfg(target_os = "macos")]
fn parse_data_store_identifier(record: &WebSessionRecord) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    if let Some(ref hex) = record.data_store_uuid {
        let clean: String = hex.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        if clean.len() >= 32 {
            for (i, slot) in bytes.iter_mut().enumerate() {
                if let Ok(val) = u8::from_str_radix(&clean[i * 2..i * 2 + 2], 16) {
                    *slot = val;
                }
            }
            return bytes;
        }
    }
    for (i, b) in record.id.as_bytes().iter().take(12).enumerate() {
        bytes[i] = *b;
    }
    bytes[12..16].copy_from_slice(&record.profile_generation.to_be_bytes());
    bytes
}

struct ChildEntry {
    // Field order matters: `webview` MUST drop before `context`.
    webview: WebView,
    #[allow(dead_code)]
    context: WebContext,
    generation: u32,
    bounds: Rect,
    visible: bool,
    crashed: bool,
}

#[derive(Default)]
struct WebWorkspaceManager {
    children: HashMap<String, ChildEntry>,
    active_id: Option<String>,
    main_window_visible: bool,
    overlay_blocked: bool,
    deleting_profiles: HashSet<String>,
    #[cfg(target_os = "windows")]
    cached_env: Option<ICoreWebView2Environment>,
    initialized: bool,
}

impl WebWorkspaceManager {
    fn ensure_initialized(&mut self) {
        if !self.initialized {
            self.main_window_visible = true;
            self.overlay_blocked = false;
            self.initialized = true;
        }
    }

    fn sync_visibility(&mut self) {
        self.ensure_initialized();
        let allow_visible = self.main_window_visible && !self.overlay_blocked;
        let active = self.active_id.clone();
        for (id, entry) in &mut self.children {
            let should_show =
                allow_visible && !entry.crashed && active.as_deref() == Some(id.as_str());
            if entry.visible != should_show {
                let _ = entry.webview.set_visible(should_show);
                entry.visible = should_show;
            }
        }
    }

    fn take_all_for_clear(&mut self) -> Vec<ChildEntry> {
        self.active_id = None;
        clear_all_mounted_flags();
        let mut entries = Vec::with_capacity(self.children.len());
        for (_, mut entry) in self.children.drain() {
            let _ = entry.webview.set_visible(false);
            entry.visible = false;
            entries.push(entry);
        }
        #[cfg(target_os = "windows")]
        {
            self.cached_env = None;
        }
        entries
    }
}

thread_local! {
    static WEB_MANAGER: RefCell<WebWorkspaceManager> = RefCell::new(WebWorkspaceManager::default());
}

fn make_physical_rect(x: i32, y: i32, width: u32, height: u32) -> Rect {
    Rect {
        position: PhysicalPosition::new(x, y).into(),
        size: PhysicalSize::new(width.max(1), height.max(1)).into(),
    }
}

#[allow(dead_code)]
fn shared_env_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("WebSessions").join("_shared_env")
}

#[cfg(not(target_os = "windows"))]
fn sanitize_session_dir_component(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(not(target_os = "windows"))]
fn per_session_dir(data_dir: &Path, id: &str, generation: u32) -> PathBuf {
    let safe_id = sanitize_session_dir_component(id);
    data_dir
        .join("WebSessions")
        .join(format!("{safe_id}_g{generation}"))
}

#[cfg(not(target_os = "windows"))]
fn legacy_per_session_dir(data_dir: &Path, id: &str) -> PathBuf {
    data_dir
        .join("WebSessions")
        .join(sanitize_session_dir_component(id))
}

#[cfg(target_os = "windows")]
fn is_child_hwnd_alive(webview: &WebView) -> bool {
    #[link(name = "user32")]
    extern "system" {
        fn IsWindow(h_wnd: *mut std::ffi::c_void) -> i32;
    }
    let raw = webview.hwnd().0;
    !raw.is_null() && unsafe { IsWindow(raw) != 0 }
}

#[cfg(not(target_os = "windows"))]
fn is_child_hwnd_alive(_webview: &WebView) -> bool {
    true
}

#[cfg(target_os = "windows")]
fn delete_webview2_profile_via_api(webview: &WebView, profile_name: &str) -> bool {
    let Ok(wv13) = webview.webview().cast::<ICoreWebView2_13>() else {
        return false;
    };
    let Ok(profile) = (unsafe { wv13.Profile() }) else {
        return false;
    };
    let Ok(profile8) = profile.cast::<ICoreWebView2Profile8>() else {
        return false;
    };
    let name_owned = profile_name.to_owned();
    let mut token: i64 = 0;
    let _ = unsafe {
        profile8.add_Deleted(
            &ProfileDeletedEventHandler::create(Box::new(move |_, _| {
                WEB_MANAGER.with(|cell| {
                    if let Ok(mut mgr) = cell.try_borrow_mut() {
                        mgr.deleting_profiles.remove(&name_owned);
                    }
                });
                Ok(())
            })),
            &mut token,
        )
    };
    if unsafe { profile8.Delete() }.is_ok() {
        WEB_MANAGER.with(|cell| {
            if let Ok(mut mgr) = cell.try_borrow_mut() {
                mgr.deleting_profiles.insert(profile_name.to_owned());
            }
        });
        true
    } else {
        false
    }
}

#[cfg(target_os = "windows")]
fn delete_unmounted_webview2_profile_via_api(
    main_window: &WebviewWindow,
    data_dir: &Path,
    profile_name: &str,
) -> bool {
    if !is_valid_windows_profile_name(profile_name) {
        return true;
    }
    let env_dir = shared_env_dir(data_dir);
    if !env_dir.exists() {
        return true;
    }
    let cached_env = WEB_MANAGER.with(|cell| {
        cell.try_borrow()
            .ok()
            .and_then(|mgr| mgr.cached_env.clone())
    });

    let mut context = WebContext::new(Some(env_dir));
    let mut builder = WebViewBuilder::new_with_web_context(&mut context)
        .with_url("about:blank")
        .with_bounds(make_physical_rect(0, 0, 1, 1))
        .with_visible(false)
        .with_clipboard(false)
        .with_general_autofill_enabled(false)
        .with_devtools(false)
        .with_profile_name(profile_name.to_owned());
    if let Some(env) = cached_env {
        builder = builder.with_environment(env);
    }

    let Ok(temp_wv) = builder.build_as_child(main_window) else {
        return false;
    };
    WEB_MANAGER.with(|cell| {
        if let Ok(mut mgr) = cell.try_borrow_mut() {
            if mgr.cached_env.is_none() {
                mgr.cached_env = Some(temp_wv.environment());
            }
        }
    });
    let deleted = delete_webview2_profile_via_api(&temp_wv, profile_name);
    drop(temp_wv);
    drop(context);
    deleted
}

/// Sweeps any queued `pending_profile_deletes` via `ICoreWebView2Profile8::Delete()` without ever
/// calling raw `fs::remove_dir_all` inside WebView2's `EBWebView` folder.
pub fn sweep_pending_profile_deletes(
    main_window: Option<&WebviewWindow>,
    data_dir: &Path,
    store: &crate::state::WebSessionStore,
) -> Result<(), DimError> {
    let pending = store.pending_profile_deletes();
    if pending.is_empty() {
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        let mut cleared = Vec::new();
        let env_exists = shared_env_dir(data_dir).exists();
        for name in pending {
            if !is_valid_windows_profile_name(&name) || !env_exists {
                cleared.push(name);
                continue;
            }
            if let Some(win) = main_window {
                if delete_unmounted_webview2_profile_via_api(win, data_dir, &name) {
                    cleared.push(name);
                }
            }
        }
        store.remove_pending_profile_deletes(&cleared)?;
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (main_window, data_dir);
        store.remove_pending_profile_deletes(&pending)?;
        Ok(())
    }
}

fn build_child_webview(
    main_window: &WebviewWindow,
    data_dir: &Path,
    record: &WebSessionRecord,
    bounds: Rect,
    #[cfg(target_os = "windows")] cached_env: Option<ICoreWebView2Environment>,
) -> Result<(WebView, WebContext), DimError> {
    ensure_valid_session_id(&record.id)?;

    #[cfg(target_os = "windows")]
    let context_dir = shared_env_dir(data_dir);
    #[cfg(not(target_os = "windows"))]
    let context_dir = per_session_dir(data_dir, &record.id, record.profile_generation);

    fs::create_dir_all(&context_dir)?;
    let mut context = WebContext::new(Some(context_dir));

    #[allow(unused_mut)]
    let mut builder = WebViewBuilder::new_with_web_context(&mut context)
        .with_url(ZALO_WEB_ENTRY_URL)
        .with_bounds(bounds)
        .with_visible(false)
        .with_clipboard(false)
        .with_general_autofill_enabled(false)
        .with_devtools(cfg!(debug_assertions))
        .with_navigation_handler(|nav_url| {
            if is_allowed_zalo_web_url(&nav_url) {
                return true;
            }
            if is_safe_external_https_url(&nav_url) {
                open_in_os_browser(&nav_url);
            }
            false
        })
        .with_new_window_req_handler(|new_url, _features| {
            if is_safe_external_https_url(&new_url) {
                open_in_os_browser(&new_url);
            }
            NewWindowResponse::Deny
        })
        .with_download_started_handler(|_url, _dest| false)
        .with_permission_handler(|kind| match kind {
            PermissionKind::Microphone | PermissionKind::Camera | PermissionKind::Notifications => {
                PermissionResponse::Default
            }
            _ => PermissionResponse::Deny,
        });

    #[cfg(target_os = "windows")]
    {
        let profile_name = windows_profile_name(&record.id, record.profile_generation);
        builder = builder.with_profile_name(profile_name);
        if let Some(env) = cached_env {
            builder = builder.with_environment(env);
        }
    }

    #[cfg(target_os = "macos")]
    {
        let session_id_for_term = record.id.clone();
        builder = builder
            .with_data_store_identifier(parse_data_store_identifier(record))
            .with_on_web_content_process_terminate_handler(move || {
                set_session_mounted_flag(&session_id_for_term, false);
                WEB_MANAGER.with(|cell| {
                    if let Ok(mut mgr) = cell.try_borrow_mut() {
                        if let Some(entry) = mgr.children.get_mut(&session_id_for_term) {
                            entry.crashed = true;
                        }
                    }
                });
            });
    }

    let webview = builder
        .build_as_child(main_window)
        .map_err(|e| DimError::ProcessError(format!("failed to build isolated webview: {e}")))?;

    #[cfg(target_os = "windows")]
    {
        let mut frame_nav_token: i64 = 0;
        let _ = unsafe {
            webview.webview().add_FrameNavigationStarting(
                &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                    let Some(args) = args else {
                        return Ok(());
                    };
                    let uri = {
                        let mut pw = PWSTR::null();
                        args.Uri(&mut pw)?;
                        take_pwstr(pw)
                    };
                    if !is_allowed_zalo_web_subframe_url(&uri) {
                        args.SetCancel(true)?;
                    }
                    Ok(())
                })),
                &mut frame_nav_token,
            )
        };

        let session_id_for_crash = record.id.clone();
        let mut proc_failed_token: i64 = 0;
        let _ = unsafe {
            webview.webview().add_ProcessFailed(
                &ProcessFailedEventHandler::create(Box::new(move |_, args| {
                    let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
                    if let Some(ref a) = args {
                        let _ = a.ProcessFailedKind(&mut kind);
                    }
                    if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED {
                        clear_all_mounted_flags();
                        WEB_MANAGER.with(|cell| {
                            if let Ok(mut mgr) = cell.try_borrow_mut() {
                                mgr.cached_env = None;
                                for entry in mgr.children.values_mut() {
                                    entry.crashed = true;
                                    entry.visible = false;
                                }
                            }
                        });
                    } else if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED
                        || kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE
                    {
                        set_session_mounted_flag(&session_id_for_crash, false);
                        WEB_MANAGER.with(|cell| {
                            if let Ok(mut mgr) = cell.try_borrow_mut() {
                                if let Some(entry) = mgr.children.get_mut(&session_id_for_crash) {
                                    entry.crashed = true;
                                }
                            }
                        });
                    }
                    Ok(())
                })),
                &mut proc_failed_token,
            )
        };
    }

    Ok((webview, context))
}

/// Shows (or mounts + shows) an isolated web session at physical pixel bounds.
#[allow(clippy::too_many_arguments)]
pub fn show_session(
    app: &AppHandle,
    data_dir: &Path,
    record: &WebSessionRecord,
    pending_deletes: &[String],
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<(), DimError> {
    ensure_valid_session_id(&record.id)?;
    let data_dir = data_dir.to_path_buf();
    let record = record.clone();
    let pending_deletes = pending_deletes.to_vec();
    let app_handle = app.clone();

    run_on_main_sync(app, move || {
        let main_window = app_handle
            .get_webview_window("main")
            .ok_or_else(|| DimError::ProcessError("main window is unavailable".to_owned()))?;
        let support = detect_support(Some(&main_window));
        if !support.supported {
            return Err(DimError::UnsupportedPlatform(
                support
                    .reason_code
                    .unwrap_or_else(|| "WEB_WORKSPACE_UNSUPPORTED".to_owned()),
            ));
        }

        let bounds = make_physical_rect(x, y, width, height);

        #[allow(unused_variables)]
        let (needs_rebuild, old_entry, cached_env) =
            WEB_MANAGER.with(|cell| -> Result<_, DimError> {
                let mut mgr = cell.borrow_mut();
                mgr.ensure_initialized();

                let prof_name = windows_profile_name(&record.id, record.profile_generation);
                if mgr.deleting_profiles.contains(&prof_name)
                    || pending_deletes.iter().any(|p| p == &prof_name)
                {
                    return Err(DimError::ProcessError(format!(
                        "[WEB_PROFILE_DELETION_PENDING] WebView2 profile '{prof_name}' is pending deletion"
                    )));
                }

                let rebuild = match mgr.children.get(&record.id) {
                    Some(existing) => {
                        existing.generation != record.profile_generation
                            || existing.crashed
                            || !is_child_hwnd_alive(&existing.webview)
                    }
                    None => true,
                };
                let old = if rebuild {
                    mgr.children.remove(&record.id)
                } else {
                    None
                };
                #[cfg(target_os = "windows")]
                let env = mgr.cached_env.clone();
                #[cfg(not(target_os = "windows"))]
                let env = ();
                Ok((rebuild, old, env))
            })?;

        if let Some(old) = old_entry {
            set_session_mounted_flag(&record.id, false);
            drop(old);
        }

        let built = if needs_rebuild {
            Some(build_child_webview(
                &main_window,
                &data_dir,
                &record,
                bounds,
                #[cfg(target_os = "windows")]
                cached_env,
            )?)
        } else {
            None
        };

        WEB_MANAGER.with(|cell| -> Result<(), DimError> {
            let mut mgr = cell.borrow_mut();
            if let Some((webview, context)) = built {
                #[cfg(target_os = "windows")]
                if mgr.cached_env.is_none() {
                    mgr.cached_env = Some(webview.environment());
                }

                mgr.children.insert(
                    record.id.clone(),
                    ChildEntry {
                        webview,
                        context,
                        generation: record.profile_generation,
                        bounds,
                        visible: false,
                        crashed: false,
                    },
                );
                set_session_mounted_flag(&record.id, true);
            } else if let Some(entry) = mgr.children.get_mut(&record.id) {
                if entry.bounds != bounds {
                    let _ = entry.webview.set_bounds(bounds);
                    entry.bounds = bounds;
                }
            }

            mgr.active_id = Some(record.id.clone());
            mgr.sync_visibility();
            Ok(())
        })?;

        notify_parent_moved_on_current_thread();
        Ok(())
    })
}

/// Updates physical pixel bounds for a mounted web session.
pub fn set_bounds(
    app: &AppHandle,
    id: &str,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<(), DimError> {
    ensure_valid_session_id(id)?;
    let id = id.to_owned();
    run_on_main_sync(app, move || {
        let bounds = make_physical_rect(x, y, width, height);
        WEB_MANAGER.with(|cell| {
            let mut mgr = cell.borrow_mut();
            if let Some(entry) = mgr.children.get_mut(&id) {
                if entry.bounds != bounds {
                    entry
                        .webview
                        .set_bounds(bounds)
                        .map_err(|e| DimError::ProcessError(format!("set_bounds failed: {e}")))?;
                    entry.bounds = bounds;
                }
            }
            Ok(())
        })
    })
}

/// Hides a specific web session if it is currently active.
pub fn hide_session(app: &AppHandle, id: &str) -> Result<(), DimError> {
    ensure_valid_session_id(id)?;
    let id = id.to_owned();
    run_on_main_sync(app, move || {
        WEB_MANAGER.with(|cell| {
            let mut mgr = cell.borrow_mut();
            if mgr.active_id.as_deref() == Some(id.as_str()) {
                mgr.active_id = None;
            }
            mgr.sync_visibility();
            Ok(())
        })
    })
}

/// Hides all mounted web sessions (used before opening overlays/dialogs or switching to desktop).
pub fn hide_all_sessions<R: tauri::Runtime>(app: &AppHandle<R>) -> Result<(), DimError> {
    run_on_main_sync(app, move || {
        WEB_MANAGER.with(|cell| {
            let mut mgr = cell.borrow_mut();
            mgr.active_id = None;
            mgr.sync_visibility();
            Ok(())
        })
    })
}

/// Reloads a mounted web session (rebuilding it if its renderer/browser crashed or HWND died),
/// or no-ops cleanly if not mounted yet.
pub fn reload_session(
    app: &AppHandle,
    data_dir: &Path,
    record: &WebSessionRecord,
    pending_deletes: &[String],
) -> Result<(), DimError> {
    ensure_valid_session_id(&record.id)?;
    let data_dir = data_dir.to_path_buf();
    let record = record.clone();
    let pending_deletes = pending_deletes.to_vec();
    let app_handle = app.clone();

    run_on_main_sync(app, move || {
        let action = WEB_MANAGER.with(|cell| -> Result<_, DimError> {
            let mut mgr = cell.borrow_mut();
            let Some(entry) = mgr.children.get(&record.id) else {
                return Ok(None);
            };
            let prof_name = windows_profile_name(&record.id, record.profile_generation);
            if mgr.deleting_profiles.contains(&prof_name)
                || pending_deletes.iter().any(|p| p == &prof_name)
            {
                return Err(DimError::ProcessError(format!(
                    "[WEB_PROFILE_DELETION_PENDING] WebView2 profile '{prof_name}' is pending deletion"
                )));
            }
            let healthy = !entry.crashed
                && entry.generation == record.profile_generation
                && is_child_hwnd_alive(&entry.webview);
            if healthy && entry.webview.reload().is_ok() {
                return Ok(None);
            }
            let bounds = entry.bounds;
            let old = mgr.children.remove(&record.id);
            #[cfg(target_os = "windows")]
            let env = mgr.cached_env.clone();
            #[cfg(not(target_os = "windows"))]
            let env = ();
            Ok(Some((bounds, old, env)))
        })?;

        #[allow(unused_variables)]
        let Some((bounds, old_entry, cached_env)) = action
        else {
            return Ok(());
        };

        if let Some(old) = old_entry {
            set_session_mounted_flag(&record.id, false);
            drop(old);
        }

        let main_window = app_handle
            .get_webview_window("main")
            .ok_or_else(|| DimError::ProcessError("main window is unavailable".to_owned()))?;
        let (webview, context) = build_child_webview(
            &main_window,
            &data_dir,
            &record,
            bounds,
            #[cfg(target_os = "windows")]
            cached_env,
        )?;

        WEB_MANAGER.with(|cell| {
            let mut mgr = cell.borrow_mut();
            #[cfg(target_os = "windows")]
            if mgr.cached_env.is_none() {
                mgr.cached_env = Some(webview.environment());
            }
            mgr.children.insert(
                record.id.clone(),
                ChildEntry {
                    webview,
                    context,
                    generation: record.profile_generation,
                    bounds,
                    visible: false,
                    crashed: false,
                },
            );
            set_session_mounted_flag(&record.id, true);
            mgr.sync_visibility();
        });

        notify_parent_moved_on_current_thread();
        Ok(())
    })
}

/// Tears down a mounted web session and deletes its profile data safely.
///
/// Returns `Some(pending_profile_name)` if Windows fallback deletion should be swept later.
pub fn teardown_and_delete_profile(
    app: &AppHandle,
    data_dir: &Path,
    record: &WebSessionRecord,
) -> Result<Option<String>, DimError> {
    ensure_valid_session_id(&record.id)?;
    let data_dir = data_dir.to_path_buf();
    let record = record.clone();
    let app_handle = app.clone();

    run_on_main_sync(app, move || {
        let removed_entry = WEB_MANAGER.with(|cell| {
            let mut mgr = cell.borrow_mut();
            if mgr.active_id.as_deref() == Some(record.id.as_str()) {
                mgr.active_id = None;
            }
            let entry = mgr.children.remove(&record.id);
            mgr.sync_visibility();
            entry
        });
        set_session_mounted_flag(&record.id, false);

        #[cfg(target_os = "windows")]
        let pending_win_profile = {
            let prof_name = windows_profile_name(&record.id, record.profile_generation);
            if let Some(entry) = removed_entry {
                let _ = entry.webview.set_visible(false);
                let deleted = delete_webview2_profile_via_api(&entry.webview, &prof_name);
                if !deleted {
                    let _ = entry.webview.clear_all_browsing_data();
                }
                drop(entry);
                if deleted {
                    None
                } else {
                    Some(prof_name)
                }
            } else if let Some(main_win) = app_handle.get_webview_window("main") {
                if delete_unmounted_webview2_profile_via_api(&main_win, &data_dir, &prof_name) {
                    None
                } else {
                    Some(prof_name)
                }
            } else if !shared_env_dir(&data_dir).exists() {
                None
            } else {
                Some(prof_name)
            }
        };

        #[cfg(not(target_os = "windows"))]
        let pending_win_profile: Option<String> = {
            let _ = &app_handle;
            if let Some(entry) = removed_entry {
                let _ = entry.webview.set_visible(false);
                let _ = entry.webview.clear_all_browsing_data();
                drop(entry);
            }
            #[cfg(target_os = "macos")]
            {
                let uuid = parse_data_store_identifier(&record);
                WebView::remove_data_store(&uuid, |_| {});
            }
            let dir = per_session_dir(&data_dir, &record.id, record.profile_generation);
            if dir.exists() {
                let _ = fs::remove_dir_all(&dir);
            }
            let legacy_dir = legacy_per_session_dir(&data_dir, &record.id);
            if legacy_dir.exists() {
                let _ = fs::remove_dir_all(&legacy_dir);
            }
            None
        };

        Ok(pending_win_profile)
    })
}

/// Updates main window visibility state for the child WebView visibility predicate.
pub fn set_main_window_visible_on_current_thread(visible: bool) {
    WEB_MANAGER.with(|cell| {
        if let Ok(mut mgr) = cell.try_borrow_mut() {
            mgr.main_window_visible = visible;
            mgr.sync_visibility();
        }
    });
}

/// Notifies active Windows WebView2 child controllers that the parent window moved or resized.
pub fn notify_parent_moved_on_current_thread() {
    #[cfg(target_os = "windows")]
    WEB_MANAGER.with(|cell| {
        if let Ok(mgr) = cell.try_borrow() {
            for entry in mgr.children.values() {
                if entry.visible && !entry.crashed {
                    let _ = unsafe {
                        entry
                            .webview
                            .controller()
                            .NotifyParentWindowPositionChanged()
                    };
                }
            }
        }
    });
}

/// Drops all child WebViews before their WebContexts on the main thread prior to exit.
pub fn clear_all_on_current_thread() {
    let entries = WEB_MANAGER.with(|cell| {
        cell.try_borrow_mut()
            .ok()
            .map(|mut mgr| mgr.take_all_for_clear())
            .unwrap_or_default()
    });
    drop(entries);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_allowlist_permits_zalo_web_and_blocks_unsafe_schemes_or_hosts() {
        assert!(is_allowed_zalo_web_url("about:blank"));
        assert!(is_allowed_zalo_web_url("https://chat.zalo.me"));
        assert!(is_allowed_zalo_web_url("https://chat.zalo.me/login?foo=1"));
        assert!(is_allowed_zalo_web_url("https://zalo.me/"));
        assert!(is_allowed_zalo_web_url("https://id.zalo.me/account"));
        assert!(is_allowed_zalo_web_url(
            "https://stc-zaloprofile.zaloapp.com/avatar.jpg"
        ));

        assert!(!is_allowed_zalo_web_url("http://chat.zalo.me"));
        assert!(!is_allowed_zalo_web_url("javascript:alert(1)"));
        assert!(!is_allowed_zalo_web_url("file:///C:/Windows/win.ini"));
        assert!(!is_allowed_zalo_web_url(
            "data:text/html,<script>alert(1)</script>"
        ));
        assert!(!is_allowed_zalo_web_url("https://evil.com/zalo.me"));
        assert!(!is_allowed_zalo_web_url("https://evilzalo.me/"));
        assert!(!is_allowed_zalo_web_url("https://evilzaloapp.com/"));
        assert!(!is_allowed_zalo_web_url("https://user:pass@chat.zalo.me/"));

        assert!(is_allowed_zalo_web_subframe_url("about:blank"));
        assert!(is_allowed_zalo_web_subframe_url("about:srcdoc"));
        assert!(is_allowed_zalo_web_subframe_url(
            "blob:https://chat.zalo.me/550e8400-e29b-41d4-a716-446655440000"
        ));
        assert!(!is_allowed_zalo_web_subframe_url(
            "blob:https://evil.com/550e8400-e29b-41d4-a716-446655440000"
        ));
        assert!(!is_allowed_zalo_web_subframe_url("https://tauri.localhost"));
        assert!(!is_allowed_zalo_web_subframe_url("http://ipc.localhost"));
        assert!(!is_allowed_zalo_web_subframe_url("javascript:alert(1)"));
    }

    #[test]
    fn sanitize_external_https_url_blocks_loopback_localhost_credentials_and_quotes() {
        assert_eq!(
            sanitize_external_https_url("https://example.com/path?a=1&b=2").as_deref(),
            Some("https://example.com/path?a=1&b=2")
        );

        assert!(sanitize_external_https_url("http://example.com").is_none());
        assert!(sanitize_external_https_url("https://localhost/admin").is_none());
        assert!(sanitize_external_https_url("https://tauri.localhost/").is_none());
        assert!(sanitize_external_https_url("https://ipc.localhost/").is_none());
        assert!(sanitize_external_https_url("https://127.0.0.1:8080/").is_none());
        assert!(sanitize_external_https_url("https://0.0.0.0/").is_none());
        assert!(sanitize_external_https_url("https://169.254.169.254/latest/meta-data").is_none());
        assert!(sanitize_external_https_url("https://[::1]/").is_none());
        assert!(sanitize_external_https_url("https://[fe80::1]/").is_none());
        assert!(sanitize_external_https_url("https://[::ffff:127.0.0.1]/").is_none());
        assert!(sanitize_external_https_url("https://user:secret@example.com/").is_none());
        assert!(sanitize_external_https_url("https://example.com/\"--flag").is_none());
        assert!(sanitize_external_https_url("https://example.com/ space").is_none());
        assert!(sanitize_external_https_url("https://example.com/\x07bell").is_none());
        assert!(
            sanitize_external_https_url(&format!("https://example.com/{}", "a".repeat(2050)))
                .is_none()
        );
    }

    #[test]
    fn windows_profile_name_is_bounded_and_sanitized() {
        let name = windows_profile_name("web-a1b2c3d4", 3);
        assert_eq!(name, "khoang_web_web-a1b2c3d4_g3");
        assert!(name.len() <= 64);
        assert!(is_valid_windows_profile_name(&name));

        let long = windows_profile_name(&"x".repeat(100), 12);
        assert!(long.len() <= 64);
        assert!(long.ends_with("_g12"));
        assert!(!is_valid_windows_profile_name("../escape"));
    }

    #[test]
    fn sweep_pending_profile_deletes_never_removes_ebwebview_directory_directly() {
        let data_dir = std::env::temp_dir().join(format!(
            "khoang-web-sweep-test-{}-{}",
            std::process::id(),
            dim_core::generate_profile_id()
        ));
        let _ = fs::remove_dir_all(&data_dir);
        let eb_profile_dir = shared_env_dir(&data_dir)
            .join("EBWebView")
            .join("khoang_web_web-a1b2c3d4_g1");
        fs::create_dir_all(&eb_profile_dir).expect("create fake EBWebView profile dir");
        let sentinel = eb_profile_dir.join("Preferences");
        fs::write(&sentinel, b"{}").expect("write sentinel file");

        let store = crate::state::WebSessionStore::initialize(&data_dir);
        let created = store
            .create("Sweep Test", None)
            .expect("create web session");
        store
            .bump_generation_for_reset(&created.id, Some("khoang_web_web-a1b2c3d4_g1".to_owned()))
            .expect("queue pending profile delete");

        sweep_pending_profile_deletes(None, &data_dir, &store).expect("sweep without main window");
        assert!(
            sentinel.exists(),
            "sweep_pending_profile_deletes must NEVER run fs::remove_dir_all on EBWebView"
        );

        let _ = fs::remove_dir_all(&data_dir);
    }
}

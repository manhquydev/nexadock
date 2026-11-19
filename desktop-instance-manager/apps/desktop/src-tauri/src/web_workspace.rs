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
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, OnceLock};
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

#[cfg(target_os = "windows")]
use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2Environment, ICoreWebView2Profile8, ICoreWebView2_13,
};
#[cfg(target_os = "windows")]
use windows_core::Interface;
#[cfg(target_os = "windows")]
use wry::{WebViewBuilderExtWindows, WebViewExtWindows};

pub const ZALO_WEB_ENTRY_URL: &str = "https://chat.zalo.me";

static MAIN_THREAD_ID: OnceLock<ThreadId> = OnceLock::new();

/// Records the main/UI thread ID during Tauri `setup()` so main-thread dispatch never deadlocks.
pub fn register_main_thread() {
    let _ = MAIN_THREAD_ID.set(std::thread::current().id());
}

fn is_on_main_thread() -> bool {
    MAIN_THREAD_ID
        .get()
        .is_some_and(|id| *id == std::thread::current().id())
}

fn run_on_main_sync<T, F>(app: &AppHandle, f: F) -> Result<T, DimError>
where
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
    let Some(Host::Domain(domain)) = parsed.host() else {
        return false;
    };
    let host = domain.trim_end_matches('.').to_ascii_lowercase();
    host == "zalo.me" || host.ends_with(".zalo.me") || host.ends_with(".zaloapp.com")
}

/// Returns `true` for external `https://` URLs that may be opened safely in the OS default browser.
pub fn is_safe_external_https_url(raw: &str) -> bool {
    Url::parse(raw).is_ok_and(|u| u.scheme().eq_ignore_ascii_case("https") && u.has_host())
}

fn open_in_os_browser(raw: &str) {
    if !is_safe_external_https_url(raw) {
        return;
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("rundll32")
            .arg("url.dll,FileProtocolHandler")
            .arg(raw)
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(raw).spawn();
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(raw).spawn();
    }
}

/// Computes the Windows WebView2 profile name (`khoang_web_<id>_g<gen>`, `[a-z0-9_-]{1,64}`).
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
}

#[derive(Default)]
struct WebWorkspaceManager {
    children: HashMap<String, ChildEntry>,
    active_id: Option<String>,
    main_window_visible: bool,
    overlay_blocked: bool,
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
            let should_show = allow_visible && active.as_deref() == Some(id.as_str());
            if entry.visible != should_show {
                let _ = entry.webview.set_visible(should_show);
                entry.visible = should_show;
            }
        }
    }

    fn clear_all(&mut self) {
        self.active_id = None;
        for (_, mut entry) in self.children.drain() {
            let _ = entry.webview.set_visible(false);
            entry.visible = false;
            drop(entry);
        }
        #[cfg(target_os = "windows")]
        {
            self.cached_env = None;
        }
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

fn shared_env_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("WebSessions").join("_shared_env")
}

#[cfg(not(target_os = "windows"))]
fn per_session_dir(data_dir: &Path, id: &str) -> PathBuf {
    let safe_id: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    data_dir.join("WebSessions").join(safe_id)
}

/// Sweeps any queued `pending_profile_deletes` on Windows when zero child WebViews are open.
pub fn sweep_pending_profile_deletes(
    data_dir: &Path,
    store: &crate::state::WebSessionStore,
) -> Result<(), DimError> {
    let has_children = WEB_MANAGER.with(|cell| !cell.borrow().children.is_empty());
    if has_children {
        return Ok(());
    }

    let pending = store.pending_profile_deletes();
    if pending.is_empty() {
        return Ok(());
    }

    let eb_root = shared_env_dir(data_dir).join("EBWebView");
    let mut cleared = Vec::new();
    for name in pending {
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            cleared.push(name);
            continue;
        }
        let target = eb_root.join(&name);
        if !target.exists() || fs::remove_dir_all(&target).is_ok() {
            cleared.push(name);
        }
    }
    store.remove_pending_profile_deletes(&cleared)?;
    Ok(())
}

fn build_child_webview(
    main_window: &WebviewWindow,
    data_dir: &Path,
    record: &WebSessionRecord,
    bounds: Rect,
    #[cfg(target_os = "windows")] cached_env: Option<ICoreWebView2Environment>,
) -> Result<(WebView, WebContext), DimError> {
    #[cfg(target_os = "windows")]
    let context_dir = shared_env_dir(data_dir);
    #[cfg(not(target_os = "windows"))]
    let context_dir = per_session_dir(data_dir, &record.id);

    fs::create_dir_all(&context_dir)?;
    let mut context = WebContext::new(Some(context_dir));

    let mut builder = WebViewBuilder::new_with_web_context(&mut context)
        .with_url(ZALO_WEB_ENTRY_URL)
        .with_bounds(bounds)
        .with_visible(false)
        .with_clipboard(true)
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
        builder = builder.with_data_store_identifier(parse_data_store_identifier(record));
    }

    let webview = builder
        .build_as_child(main_window)
        .map_err(|e| DimError::ProcessError(format!("failed to build isolated webview: {e}")))?;

    Ok((webview, context))
}

/// Shows (or mounts + shows) an isolated web session at physical pixel bounds.
pub fn show_session(
    app: &AppHandle,
    data_dir: &Path,
    record: &WebSessionRecord,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<(), DimError> {
    let data_dir = data_dir.to_path_buf();
    let record = record.clone();
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

        WEB_MANAGER.with(|cell| {
            let mut mgr = cell.borrow_mut();
            mgr.ensure_initialized();

            let needs_rebuild = match mgr.children.get(&record.id) {
                Some(existing) => existing.generation != record.profile_generation,
                None => true,
            };

            if needs_rebuild {
                if let Some(old) = mgr.children.remove(&record.id) {
                    drop(old);
                }
                #[cfg(target_os = "windows")]
                let cached_env = mgr.cached_env.clone();

                let (webview, context) = build_child_webview(
                    &main_window,
                    &data_dir,
                    &record,
                    bounds,
                    #[cfg(target_os = "windows")]
                    cached_env,
                )?;

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
                    },
                );
            } else if let Some(entry) = mgr.children.get_mut(&record.id) {
                if entry.bounds != bounds {
                    let _ = entry.webview.set_bounds(bounds);
                    entry.bounds = bounds;
                }
            }

            mgr.active_id = Some(record.id.clone());
            mgr.sync_visibility();
            Ok(())
        })
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
pub fn hide_all_sessions(app: &AppHandle) -> Result<(), DimError> {
    run_on_main_sync(app, move || {
        WEB_MANAGER.with(|cell| {
            let mut mgr = cell.borrow_mut();
            mgr.active_id = None;
            mgr.sync_visibility();
            Ok(())
        })
    })
}

/// Reloads a mounted web session, or no-ops cleanly if not mounted yet.
pub fn reload_session(app: &AppHandle, id: &str) -> Result<(), DimError> {
    let id = id.to_owned();
    run_on_main_sync(app, move || {
        WEB_MANAGER.with(|cell| {
            let mgr = cell.borrow();
            if let Some(entry) = mgr.children.get(&id) {
                entry
                    .webview
                    .reload()
                    .map_err(|e| DimError::ProcessError(format!("reload failed: {e}")))?;
            }
            Ok(())
        })
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
    let data_dir = data_dir.to_path_buf();
    let record = record.clone();

    run_on_main_sync(app, move || {
        let pending_win_profile = WEB_MANAGER.with(|cell| {
            let mut mgr = cell.borrow_mut();
            if mgr.active_id.as_deref() == Some(record.id.as_str()) {
                mgr.active_id = None;
            }
            let mut pending: Option<String> = None;
            if let Some(entry) = mgr.children.remove(&record.id) {
                let _ = entry.webview.set_visible(false);
                #[cfg(target_os = "windows")]
                {
                    let mut deleted_via_api = false;
                    if let Ok(wv13) = entry.webview.webview().cast::<ICoreWebView2_13>() {
                        if let Ok(profile) = unsafe { wv13.Profile() } {
                            if let Ok(profile8) = profile.cast::<ICoreWebView2Profile8>() {
                                if unsafe { profile8.Delete() }.is_ok() {
                                    deleted_via_api = true;
                                }
                            }
                        }
                    }
                    if !deleted_via_api {
                        let _ = entry.webview.clear_all_browsing_data();
                        pending = Some(windows_profile_name(&record.id, record.profile_generation));
                    }
                }
                #[cfg(not(target_os = "windows"))]
                {
                    let _ = entry.webview.clear_all_browsing_data();
                }
                drop(entry);
            } else {
                #[cfg(target_os = "windows")]
                {
                    pending = Some(windows_profile_name(&record.id, record.profile_generation));
                }
            }
            mgr.sync_visibility();
            pending
        });

        #[cfg(not(target_os = "windows"))]
        {
            let dir = per_session_dir(&data_dir, &record.id);
            if dir.exists() {
                let _ = fs::remove_dir_all(&dir);
            }
        }
        #[cfg(target_os = "windows")]
        let _ = &data_dir;

        Ok(pending_win_profile)
    })
}

/// Updates main window visibility state for the child WebView visibility predicate.
pub fn set_main_window_visible_on_current_thread(visible: bool) {
    WEB_MANAGER.with(|cell| {
        let mut mgr = cell.borrow_mut();
        mgr.main_window_visible = visible;
        mgr.sync_visibility();
    });
}

/// Notifies active Windows WebView2 child controllers that the parent window moved.
pub fn notify_parent_moved_on_current_thread() {
    #[cfg(target_os = "windows")]
    WEB_MANAGER.with(|cell| {
        let mgr = cell.borrow();
        for entry in mgr.children.values() {
            if entry.visible {
                let _ = unsafe {
                    entry
                        .webview
                        .controller()
                        .NotifyParentWindowPositionChanged()
                };
            }
        }
    });
}

/// Drops all child WebViews before their WebContexts on the main thread prior to exit.
pub fn clear_all_on_current_thread() {
    WEB_MANAGER.with(|cell| {
        cell.borrow_mut().clear_all();
    });
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
    }

    #[test]
    fn windows_profile_name_is_bounded_and_sanitized() {
        let name = windows_profile_name("web-a1b2c3d4", 3);
        assert_eq!(name, "khoang_web_web-a1b2c3d4_g3");
        assert!(name.len() <= 64);

        let long = windows_profile_name(&"x".repeat(100), 12);
        assert!(long.len() <= 64);
        assert!(long.ends_with("_g12"));
    }
}

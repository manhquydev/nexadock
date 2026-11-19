//! Tauri 2 IPC commands delegating directly to `dim-core`, `dim-profile-store`,
//! `dim-process-manager`, and the active OS platform adapter.

use std::fs;
use std::path::PathBuf;

use dim_core::{
    ensure_not_primary_zalo_path, redact_sensitive_text, resolve_safe_child,
    validate_directory_input, AccountActivationResult, AccountKind, ActionableRemediation,
    ActivationOutcome, ApplicationLocator, BackupManager, BackupManifest, CompanionSnapResult,
    DimError, InstanceLauncher, InstanceMonitor, LaunchTarget, PlatformCapabilities, PlatformKind,
    PrimaryWindowState, ProfileStore, WebSessionRecord,
};
#[cfg(target_os = "windows")]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::autostart;
use crate::preferences::{DesktopPreferences, DesktopPrefsState};
pub use crate::state::AppState;
use crate::tray;
use crate::web_workspace::{self, WebWorkspaceSupport};

fn current_user_home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

#[allow(dead_code)]
const OFFICIAL_FIXTURE_SHA256_ALLOWLIST: &[&str] = &[
    // fixtures/cross-platform/xf-fixture.mjs (LF and CRLF)
    "1f30e1d26d01b69deacf217610ea9c9a30aeed06fd089968f15084eaa2cda9f4",
    "eca38c7efb3d08191dfddad4e82e99a83066eb40c59919225e5b642753a95281",
    // fixtures/test-desktop-app/fixture-app.mjs (LF and CRLF)
    "46d3cb9bafc192bdc31147b8469a89417f7702f0d57d2ade59752f273cc33fbf",
    "724971c2b15ae45406ece208b2d11e3e7c663b503dc49b609914f06ced7a4aa2",
];

#[allow(dead_code)]
fn is_allowlisted_fixture_script(path: &std::path::Path) -> bool {
    match dim_profile_store::sha256_file(path) {
        Ok(digest) => OFFICIAL_FIXTURE_SHA256_ALLOWLIST.contains(&digest.as_str()),
        Err(_) => false,
    }
}

fn resolve_tauri_launch_target() -> (Option<PathBuf>, LaunchTarget) {
    if let Some(raw) = std::env::var_os("DIM_NODE_FIXTURE_SCRIPT") {
        let candidate = PathBuf::from(raw);
        if !candidate.as_os_str().is_empty() && validate_directory_input(&candidate).is_ok() {
            #[cfg(not(debug_assertions))]
            let allowed = is_allowlisted_fixture_script(&candidate);
            #[cfg(debug_assertions)]
            let allowed = true;
            if allowed {
                return (
                    Some(candidate.clone()),
                    LaunchTarget::NodeFixture {
                        node_path: None,
                        script_path: candidate,
                        extra_args: Vec::new(),
                    },
                );
            }
        }
    }
    (
        None,
        LaunchTarget::OfficialZalo {
            custom_install_root: None,
        },
    )
}

fn redact_err_text(err: impl std::fmt::Display) -> String {
    let home = current_user_home();
    redact_sensitive_text(&err.to_string(), home.as_deref())
}

fn redact_dim_err(err: DimError) -> String {
    let code = err.code();
    format!("[{code}] {}", redact_err_text(err))
}

fn join_err_envelope<T: Serialize>(err: impl std::fmt::Display) -> ApiEnvelope<T> {
    ApiEnvelope::err(&DimError::ProcessError(format!(
        "background command worker failed: {err}"
    )))
}

#[derive(Debug, Serialize)]
pub struct ApiEnvelope<T: Serialize> {
    pub ok: bool,
    pub error_code: Option<String>,
    pub message: String,
    pub data: Option<T>,
}

impl<T: Serialize> ApiEnvelope<T> {
    fn ok(message: impl Into<String>, data: T) -> Self {
        Self {
            ok: true,
            error_code: None,
            message: message.into(),
            data: Some(data),
        }
    }

    fn err(err: &DimError) -> Self {
        Self {
            ok: false,
            error_code: Some(err.code().to_owned()),
            message: redact_err_text(err),
            data: None,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct BackupListItem {
    pub backup_name: String,
    pub backup_dir: String,
    pub backup_id: String,
    pub created_at: String,
    pub file_count: u64,
    pub total_bytes: u64,
    pub source_profile_id: String,
    pub source_profile_name: String,
}

#[derive(Debug, Serialize)]
pub struct BackupsResponse {
    pub ok: bool,
    pub backups: Vec<BackupListItem>,
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub ok: bool,
    pub app_version: String,
    pub platform: String,
    pub data_dir: String,
    pub installation: serde_json::Value,
    pub primary_zalo: serde_json::Value,
    pub profiles: Vec<serde_json::Value>,
}

#[tauri::command]
pub async fn get_status(
    state: State<'_, AppState>,
    with_size: Option<bool>,
) -> Result<StatusResponse, String> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let measure_size = with_size.unwrap_or(false);
        let (fixture_script_opt, _) = resolve_tauri_launch_target();
        let installation_json = if let Some(ref script_path) = fixture_script_opt {
            let exists = script_path.exists();
            serde_json::json!({
                "valid": exists,
                "mode": "node-fixture",
                "version": "xf-fixture-1.0.0",
                "authenticode_status": if exists { "Signed (Node.js Fixture)" } else { "Missing Fixture" },
                "signer_subject": serde_json::Value::Null,
                "error": if exists { serde_json::Value::Null } else { serde_json::json!("Fixture script not found") },
            })
        } else {
            let installation = app
                .adapter
                .discover_installation(None)
                .map_err(redact_dim_err)?;
            serde_json::json!({
                "valid": installation.is_valid,
                "mode": "official-zalo",
                "version": installation.version,
                "authenticode_status": installation.signature_status,
                "signer_subject": installation.signer_subject,
                "error": installation.error,
            })
        };
        let primary = app.adapter.inspect_primary().map_err(redact_dim_err)?;
        let records = app.store.list_profiles().map_err(redact_dim_err)?;

        let mut profiles = Vec::with_capacity(records.len());
        for rec in records {
            let dir = app.store.profile_dir_for(&rec).map_err(redact_dim_err)?;
            let rt = app
                .coordinator
                .inspect_profile_record(&rec, &dir)
                .map_err(redact_dim_err)?;
            let data_size = if measure_size {
                Some(rt.disk_usage_bytes)
            } else {
                None
            };
            let lifecycle_state = rt.lifecycle_state();
            profiles.push(serde_json::json!({
                "id": rec.id,
                "name": rec.name,
                "directory_name": rec.directory_name,
                "created_at": rec.created_at,
                "last_launched_at": rec.last_launched_at,
                "zalo_user_id": rt.detected_zalo_user_id.or(rec.zalo_user_id),
                "notes": rec.notes,
                "running": rt.running,
                "lifecycle_state": lifecycle_state,
                "status_label": rt.status_label,
                "main_pid": rt.main_pid,
                "process_count": rt.process_count,
                "window_handle": rt.window_handle.unwrap_or(0),
                "window_title": rt.window_title,
                "total_working_set_bytes": rt.working_set_bytes,
                "data_size_bytes": data_size,
                "stale_lockfile_detected": rt.stale_lock_detected,
            }));
        }

        Ok(StatusResponse {
            ok: true,
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
            platform: app.adapter.platform().as_str().to_owned(),
            data_dir: app.store.data_dir().display().to_string(),
            installation: installation_json,
            primary_zalo: serde_json::json!({
                "is_running": primary.running,
                "locking_pids": primary.protected_pids,
                "process_count": primary.process_count,
                "total_working_set_bytes": primary.working_set_bytes,
                "zalo_user_id": primary.detected_zalo_user_id,
            }),
            profiles,
        })
    })
    .await
    .map_err(redact_err_text)?
}

#[tauri::command]
pub async fn create_profile(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    name: String,
    notes: Option<String>,
) -> Result<ApiEnvelope<dim_core::ProfileRecord>, String> {
    let app = state.inner().clone();
    let res = tauri::async_runtime::spawn_blocking(move || {
        match app
            .store
            .create_profile(&name, notes.as_deref().unwrap_or(""))
        {
            Ok(rec) => {
                if let Ok(dir) = app.store.profile_dir_for(&rec) {
                    let _ = app.adapter.ensure_profile_layout(&rec, &dir);
                }
                ApiEnvelope::ok(format!("Created profile '{}'.", rec.name), rec)
            }
            Err(err) => ApiEnvelope::err(&err),
        }
    })
    .await
    .unwrap_or_else(join_err_envelope);
    if res.ok {
        tray::refresh_tray_accounts(&app_handle);
        emit_snapshot_changed(&app_handle);
    }
    Ok(res)
}

#[tauri::command]
pub async fn rename_profile(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    id: String,
    new_name: String,
) -> Result<ApiEnvelope<dim_core::ProfileRecord>, String> {
    let app = state.inner().clone();
    let res = tauri::async_runtime::spawn_blocking(move || {
        match app.store.rename_profile(&id, &new_name) {
            Ok(rec) => ApiEnvelope::ok(format!("Renamed profile to '{}'.", rec.name), rec),
            Err(err) => ApiEnvelope::err(&err),
        }
    })
    .await
    .unwrap_or_else(join_err_envelope);
    if res.ok {
        tray::refresh_tray_accounts(&app_handle);
        emit_snapshot_changed(&app_handle);
    }
    Ok(res)
}

#[tauri::command]
pub async fn delete_profile(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    prefs: State<'_, DesktopPrefsState>,
    id: String,
    confirm: Option<bool>,
) -> Result<ApiEnvelope<serde_json::Value>, String> {
    let app = state.inner().clone();
    let prefs_inner = prefs.inner().clone();
    let res = tauri::async_runtime::spawn_blocking(move || {
        let canonical_id = app
            .store
            .get_profile(&id)
            .map(|p| p.id)
            .unwrap_or_else(|_| id.clone());
        match app
            .coordinator
            .delete_profile(&id, confirm.unwrap_or(false))
        {
            Ok(()) => {
                let _ = prefs_inner.set_account_pinned(&canonical_id, false);
                if canonical_id != id {
                    let _ = prefs_inner.set_account_pinned(&id, false);
                }
                if let Ok(mut guard) = app.companion_snapped.lock() {
                    guard.remove(&canonical_id);
                    guard.remove(&id);
                }
                ApiEnvelope::ok(
                    "Deleted profile.",
                    serde_json::json!({ "deleted_profile_id": canonical_id }),
                )
            }
            Err(err) => ApiEnvelope::err(&err),
        }
    })
    .await
    .unwrap_or_else(join_err_envelope);
    if res.ok {
        tray::refresh_tray_accounts(&app_handle);
        emit_snapshot_changed(&app_handle);
    }
    Ok(res)
}

#[tauri::command]
pub async fn launch_profile(
    state: State<'_, AppState>,
    id: String,
) -> Result<ApiEnvelope<dim_core::ProfileRuntimeState>, String> {
    let app = state.inner().clone();
    Ok(tauri::async_runtime::spawn_blocking(move || {
        let (_, target) = resolve_tauri_launch_target();
        match app.coordinator.launch_profile(&id, &target) {
            Ok(rt) => ApiEnvelope::ok("Launched profile.", rt),
            Err(err) => ApiEnvelope::err(&err),
        }
    })
    .await
    .unwrap_or_else(join_err_envelope))
}

#[tauri::command]
pub async fn focus_profile(
    state: State<'_, AppState>,
    id: String,
) -> Result<ApiEnvelope<dim_core::ProfileRuntimeState>, String> {
    let app = state.inner().clone();
    Ok(tauri::async_runtime::spawn_blocking(move || {
        let (_, target) = resolve_tauri_launch_target();
        match app.coordinator.focus_profile(&id, &target) {
            Ok(rt) => ApiEnvelope::ok("Focused profile.", rt),
            Err(err) => ApiEnvelope::err(&err),
        }
    })
    .await
    .unwrap_or_else(join_err_envelope))
}

#[tauri::command]
pub async fn stop_profile(
    state: State<'_, AppState>,
    id: String,
) -> Result<ApiEnvelope<u32>, String> {
    let app = state.inner().clone();
    Ok(
        tauri::async_runtime::spawn_blocking(move || match app.coordinator.stop_profile(&id) {
            Ok(count) => ApiEnvelope::ok("Stopped profile.", count),
            Err(err) => ApiEnvelope::err(&err),
        })
        .await
        .unwrap_or_else(join_err_envelope),
    )
}

#[tauri::command]
pub async fn stop_all_profiles(
    state: State<'_, AppState>,
) -> Result<ApiEnvelope<serde_json::Value>, String> {
    let app = state.inner().clone();
    Ok(tauri::async_runtime::spawn_blocking(move || {
        match app.coordinator.stop_all_secondary_profiles() {
            Ok(stopped) => {
                let items: Vec<serde_json::Value> = stopped
                    .iter()
                    .map(|(pid, count)| {
                        serde_json::json!({ "profile_id": pid, "terminated_process_count": count })
                    })
                    .collect();
                ApiEnvelope::ok(
                    format!("Stopped {} secondary profile(s).", items.len()),
                    serde_json::json!({
                        "stopped_profiles_count": items.len(),
                        "stopped_profiles": items,
                    }),
                )
            }
            Err(err) => ApiEnvelope::err(&err),
        }
    })
    .await
    .unwrap_or_else(join_err_envelope))
}

#[tauri::command]
pub async fn backup_profile(
    state: State<'_, AppState>,
    id: String,
    allow_external_db_omit: Option<bool>,
) -> Result<ApiEnvelope<dim_core::BackupResult>, String> {
    let app = state.inner().clone();
    Ok(tauri::async_runtime::spawn_blocking(move || {
        match app.backup_profile_synchronized(&id, allow_external_db_omit.unwrap_or(false)) {
            Ok(bk) => ApiEnvelope::ok("Created offline atomic backup.", bk),
            Err(err) => ApiEnvelope::err(&err),
        }
    })
    .await
    .unwrap_or_else(join_err_envelope))
}

#[tauri::command]
pub fn list_backups(state: State<'_, AppState>) -> BackupsResponse {
    let mut items = Vec::new();
    let backups_root = state.store.backups_dir();
    if let Ok(entries) = fs::read_dir(backups_root) {
        for entry in entries.flatten() {
            let dir = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(".partial") || !dir.is_dir() {
                continue;
            }
            let manifest_path = dir.join("backup.json");
            let Ok(raw) = fs::read_to_string(&manifest_path) else {
                continue;
            };
            let Ok(manifest) = serde_json::from_str::<BackupManifest>(&raw) else {
                continue;
            };
            if manifest.schema_version != 1 {
                continue;
            }
            items.push(BackupListItem {
                backup_name: name,
                backup_dir: dir.display().to_string(),
                backup_id: manifest.backup_id,
                created_at: manifest.created_at,
                file_count: manifest.file_count,
                total_bytes: manifest.total_bytes,
                source_profile_id: manifest.source_profile_id,
                source_profile_name: manifest.source_profile_name,
            });
        }
    }
    BackupsResponse {
        ok: true,
        backups: items,
    }
}

#[tauri::command]
pub async fn restore_backup(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    backup_dir: String,
    name: Option<String>,
) -> Result<ApiEnvelope<dim_core::ProfileRecord>, String> {
    let app = state.inner().clone();
    let res = tauri::async_runtime::spawn_blocking(move || {
        let trimmed_dir = backup_dir.trim();
        let candidate = PathBuf::from(trimmed_dir);
        if let Err(err) = validate_directory_input(&candidate) {
            return ApiEnvelope::err(&err);
        }

        let backups_root = app.store.backups_dir();
        let resolved_dir = if candidate.is_absolute() {
            let Ok(canon_root) = fs::canonicalize(backups_root) else {
                return ApiEnvelope::err(&DimError::PathTraversal(
                    "backups root unavailable".to_owned(),
                ));
            };
            let Ok(canon_cand) = fs::canonicalize(&candidate) else {
                return ApiEnvelope::err(&DimError::PathTraversal(
                    "backup directory must exist inside managed Backups root".to_owned(),
                ));
            };
            if !canon_cand.starts_with(&canon_root) || canon_cand == canon_root {
                return ApiEnvelope::err(&DimError::PathTraversal(
                    "backup directory escapes managed Backups root".to_owned(),
                ));
            }
            canon_cand
        } else {
            match resolve_safe_child(backups_root, trimmed_dir) {
                Ok(p) => p,
                Err(err) => return ApiEnvelope::err(&err),
            }
        };

        if let Err(err) = ensure_not_primary_zalo_path(&resolved_dir) {
            return ApiEnvelope::err(&err);
        }

        let trimmed = name.as_deref().map(str::trim).filter(|s| !s.is_empty());
        match app
            .backups
            .restore_backup_as_new_profile(&resolved_dir, trimmed)
        {
            Ok(rec) => ApiEnvelope::ok(format!("Restored backup into '{}'.", rec.name), rec),
            Err(err) => ApiEnvelope::err(&err),
        }
    })
    .await
    .unwrap_or_else(join_err_envelope);
    if res.ok {
        tray::refresh_tray_accounts(&app_handle);
        emit_snapshot_changed(&app_handle);
    }
    Ok(res)
}

#[tauri::command]
pub fn get_diagnostics(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let caps = state.adapter.capabilities();
    let free = state
        .adapter
        .free_disk_bytes(state.store.data_dir())
        .unwrap_or(0);
    let is_elevated = state.adapter.is_elevated();
    let sac_state = state.adapter.smart_app_control_state();
    let profiles_acl_safe = state
        .adapter
        .check_profiles_acl_safe(state.store.profiles_dir());
    let format_status = |s: dim_core::SupportStatus| -> &'static str {
        match s {
            dim_core::SupportStatus::SupportedAndTested => "SUPPORTED_AND_TESTED",
            dim_core::SupportStatus::PartiallySupported => "PARTIALLY_SUPPORTED",
            dim_core::SupportStatus::Experimental => "EXPERIMENTAL",
            dim_core::SupportStatus::Blocked => "BLOCKED",
            dim_core::SupportStatus::Unsupported => "UNSUPPORTED",
            dim_core::SupportStatus::NotTested => "NOT_TESTED",
        }
    };
    let platform_status = format_status(caps.overall_status);
    let live_limits = caps.limitations.join("; ");
    let win_row = if caps.platform == PlatformKind::Windows {
        serde_json::json!({
            "platform": "Windows 10/11",
            "status": platform_status,
            "strategy": "Native Rust core + CreateProcessW USERPROFILE/APPDATA/TEMP isolation",
            "limits": live_limits,
        })
    } else {
        serde_json::json!({
            "platform": "Windows 10/11",
            "status": "PARTIALLY_SUPPORTED",
            "strategy": "Native Rust core + CreateProcessW USERPROFILE/APPDATA/TEMP isolation",
            "limits": "Concurrent calls limited by PipeZCallRecv/PipeZCallSend; prefer in-app notiWin over shared com.vng.zalo Action Center toast.",
        })
    };
    let mac_row = if caps.platform == PlatformKind::MacOs {
        serde_json::json!({
            "platform": "macOS (Apple Silicon & Intel)",
            "status": platform_status,
            "strategy": "open -n -a /Applications/Zalo.app --env CFFIXED_USER_HOME=<profile_home>",
            "limits": live_limits,
        })
    } else {
        serde_json::json!({
            "platform": "macOS (Apple Silicon & Intel)",
            "status": "NOT_TESTED",
            "strategy": "open -n -a /Applications/Zalo.app --env CFFIXED_USER_HOME=<profile_home>",
            "limits": "Requires physical Mac verification; /tmp/socketzalorecv2021 call socket conflict.",
        })
    };
    let linux_appimage_row = if caps.platform == PlatformKind::Linux {
        serde_json::json!({
            "platform": "Ubuntu LTS (Fixture / Local AppImage)",
            "status": format_status(caps.appimage_status.unwrap_or(caps.overall_status)),
            "strategy": "XDG_CONFIG_HOME / HOME / optional bwrap",
            "limits": live_limits,
        })
    } else {
        serde_json::json!({
            "platform": "Ubuntu LTS (Fixture / Local AppImage)",
            "status": "EXPERIMENTAL",
            "strategy": "XDG_CONFIG_HOME / HOME / optional bwrap",
            "limits": "On Ubuntu 24.04+, configure an AppArmor userns profile in /etc/apparmor.d/ for local AppImages (never pass --no-sandbox); never bundle proprietary VNG app.asar.",
        })
    };
    let user_home = current_user_home();
    let sanitized_logs: Vec<String> = caps
        .diagnostics
        .iter()
        .map(|line| redact_sensitive_text(line, user_home.as_deref()))
        .collect();
    let redacted_redir = caps
        .folder_redirection_path
        .as_deref()
        .map(|p| redact_sensitive_text(p, user_home.as_deref()));

    Ok(serde_json::json!({
        "ok": true,
        "is_elevated": is_elevated,
        "smart_app_control_state": sac_state,
        "disk_free_bytes": free,
        "disk_free_ok": free >= dim_core::MIN_FREE_DISK_BYTES,
        "profiles_acl_safe": profiles_acl_safe,
        "folder_redirection": {
            "redirected": caps.folder_redirection_detected,
            "warning": redacted_redir,
        },
        "platform_support_matrix": [
            win_row,
            mac_row,
            {
                "platform": "Ubuntu LTS (Official Zalo)",
                "status": "UNSUPPORTED",
                "strategy": "No official VNG Linux desktop build exists",
                "limits": "Official VNG Linux package does not exist.",
            },
            linux_appimage_row,
            {
                "platform": "WebIsolated (Zalo Web Child WebView)",
                "status": "EXPERIMENTAL",
                "strategy": "Raw wry::WebViewBuilder::build_as_child (zero Tauri IPC; per-profile WebView2 / WKWebsiteDataStore / WebKitGTK data dir)",
                "limits": "Requires Windows WebView2, macOS 14+ WKWebsiteDataStore, or Linux X11; Wayland unsupported.",
            },
        ],
        "sanitized_logs": sanitized_logs,
    }))
}

#[tauri::command]
pub async fn verify_isolation(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (fixture_opt, target) = resolve_tauri_launch_target();
        let fixture_ref = if fixture_opt.is_some() {
            Some(&target)
        } else {
            None
        };
        let report = app
            .coordinator
            .verify_isolation(&*app.adapter, None, fixture_ref)
            .map_err(redact_dim_err)?;
        serde_json::to_value(report).map_err(redact_err_text)
    })
    .await
    .map_err(redact_err_text)?
}

#[tauri::command]
pub async fn get_desktop_preferences(
    prefs: State<'_, DesktopPrefsState>,
) -> Result<DesktopPreferences, String> {
    Ok(prefs.snapshot())
}

#[tauri::command]
pub async fn set_keep_in_tray(
    enabled: bool,
    prefs: State<'_, DesktopPrefsState>,
) -> Result<DesktopPreferences, String> {
    prefs.set_keep_in_tray(enabled).map_err(redact_dim_err)
}

#[tauri::command]
pub async fn set_tray_locale(
    locale: String,
    app: tauri::AppHandle,
    prefs: State<'_, DesktopPrefsState>,
) -> Result<DesktopPreferences, String> {
    let updated = prefs.set_locale(&locale).map_err(redact_dim_err)?;
    tray::update_tray_locale(&app, &updated.locale).map_err(redact_err_text)?;
    Ok(updated)
}

#[tauri::command]
pub async fn get_autostart_enabled() -> Result<bool, String> {
    autostart::is_autostart_enabled().map_err(redact_dim_err)
}

#[tauri::command]
pub async fn set_autostart_enabled(enabled: bool) -> Result<bool, String> {
    autostart::set_autostart_enabled(enabled).map_err(redact_dim_err)
}

#[derive(Debug, Serialize)]
pub struct WorkspacePrimarySnapshot {
    pub detected: bool,
    pub running: bool,
    pub window_state: PrimaryWindowState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_title: Option<String>,
    pub pinned: bool,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceAccountItem {
    pub id: String,
    pub kind: AccountKind,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_token: Option<String>,
    pub pinned: bool,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_state: Option<PrimaryWindowState>,
    pub companion_snapped: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zalo_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_title: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceSnapshot {
    pub primary: WorkspacePrimarySnapshot,
    pub accounts: Vec<WorkspaceAccountItem>,
    pub web_workspace: WebWorkspaceSupport,
}

fn build_workspace_snapshot_core(
    app: &AppState,
    prefs: &DesktopPrefsState,
    web_workspace: WebWorkspaceSupport,
) -> Result<WorkspaceSnapshot, DimError> {
    let primary = app.adapter.inspect_primary()?;
    let desktop_profiles = app.coordinator.inspect_all_profiles_fast()?;
    let web_sessions = app.web_sessions.list();
    let prefs_snap = prefs.snapshot();

    let mut snapped_map = app
        .companion_snapped
        .lock()
        .map_err(|_| DimError::Io(std::io::Error::other("companion_snapped lock poisoned")))?;

    let mut accounts = Vec::with_capacity(desktop_profiles.len() + web_sessions.len());
    for (rec, rt) in desktop_profiles {
        let lifecycle = rt.lifecycle_state();
        if !rt.running {
            snapped_map.remove(&rec.id);
        }
        let is_snapped = snapped_map.contains_key(&rec.id);
        let window_state = if rt.running {
            if rt.window_handle.is_some() {
                Some(PrimaryWindowState::Visible)
            } else {
                Some(PrimaryWindowState::HiddenInTray)
            }
        } else {
            Some(PrimaryWindowState::None)
        };
        accounts.push(WorkspaceAccountItem {
            id: rec.id.clone(),
            kind: AccountKind::DesktopIsolated,
            display_name: rec.name,
            color_token: None,
            pinned: prefs_snap.pinned_accounts.iter().any(|p| p == &rec.id),
            state: lifecycle.as_str().to_owned(),
            window_state,
            companion_snapped: is_snapped,
            zalo_user_id: rt.detected_zalo_user_id.or(rec.zalo_user_id),
            window_title: rt.window_title,
        });
    }

    for ws in web_sessions {
        let state_str = if web_workspace::is_session_mounted(&ws.id) {
            "running"
        } else {
            "stopped"
        };
        accounts.push(WorkspaceAccountItem {
            id: ws.id.clone(),
            kind: AccountKind::WebIsolated,
            display_name: ws.display_name,
            color_token: ws.color_token,
            pinned: prefs_snap.pinned_accounts.iter().any(|p| p == &ws.id),
            state: state_str.to_owned(),
            window_state: None,
            companion_snapped: false,
            zalo_user_id: None,
            window_title: None,
        });
    }

    Ok(WorkspaceSnapshot {
        primary: WorkspacePrimarySnapshot {
            detected: primary.running || primary.zalo_data_dir.exists(),
            running: primary.running,
            window_state: primary.window_state,
            window_title: primary.window_title,
            pinned: prefs_snap.pinned_accounts.iter().any(|p| p == "primary"),
        },
        accounts,
        web_workspace,
    })
}

#[cfg(target_os = "windows")]
fn host_window_handle_u64(app: &AppHandle) -> u64 {
    let Some(win) = app.get_webview_window("main") else {
        return 0;
    };
    let Ok(handle) = win.window_handle() else {
        return 0;
    };
    match handle.as_raw() {
        RawWindowHandle::Win32(h) => h.hwnd.get() as u64,
        _ => 0,
    }
}

#[cfg(not(target_os = "windows"))]
fn host_window_handle_u64(_app: &AppHandle) -> u64 {
    0
}

fn emit_snapshot_changed(app: &AppHandle) {
    let _ = app.emit("khoang://snapshot-changed", ());
}

#[tauri::command]
pub async fn get_workspace_snapshot(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    prefs: State<'_, DesktopPrefsState>,
) -> Result<ApiEnvelope<WorkspaceSnapshot>, String> {
    let app = state.inner().clone();
    let prefs = prefs.inner().clone();
    let support = web_workspace::detect_support(app_handle.get_webview_window("main").as_ref());
    Ok(tauri::async_runtime::spawn_blocking(move || {
        match build_workspace_snapshot_core(&app, &prefs, support) {
            Ok(snapshot) => ApiEnvelope::ok("Workspace snapshot ready.", snapshot),
            Err(err) => ApiEnvelope::err(&err),
        }
    })
    .await
    .unwrap_or_else(join_err_envelope))
}

#[tauri::command]
pub async fn activate_account(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    id: String,
    kind: AccountKind,
) -> Result<ApiEnvelope<AccountActivationResult>, String> {
    let app = state.inner().clone();
    if kind == AccountKind::WebIsolated {
        let support = web_workspace::detect_support(app_handle.get_webview_window("main").as_ref());
        let rec = match app.web_sessions.get(&id) {
            Ok(r) => r,
            Err(err) => return Ok(ApiEnvelope::err(&err)),
        };
        if !support.supported {
            let code = support
                .reason_code
                .unwrap_or_else(|| "WEB_WORKSPACE_UNSUPPORTED".to_owned());
            return Ok(ApiEnvelope::ok(
                "Web session is unavailable on this platform.",
                AccountActivationResult {
                    account_id: rec.id,
                    account_kind: AccountKind::WebIsolated,
                    outcome: ActivationOutcome::Blocked,
                    method: Some("platform_gate".to_owned()),
                    os_foreground_confirmed: Some(false),
                    window_handle: None,
                    window_title: Some(rec.display_name),
                    message: format!("Web workspace is not supported ({code})."),
                    remediation: Some(ActionableRemediation {
                        code,
                        message: "Use an isolated Desktop account on this platform.".to_owned(),
                        suggested_action: Some("use_desktop_isolated".to_owned()),
                    }),
                },
            ));
        }
        return Ok(ApiEnvelope::ok(
            "Web session ready.",
            AccountActivationResult {
                account_id: rec.id,
                account_kind: AccountKind::WebIsolated,
                outcome: ActivationOutcome::Focused,
                method: Some("web_workspace".to_owned()),
                os_foreground_confirmed: Some(true),
                window_handle: None,
                window_title: Some(rec.display_name),
                message: "Web session activated.".to_owned(),
                remediation: None,
            },
        ));
    }

    let _ = web_workspace::hide_all_sessions(&app_handle);

    let target_id = if kind == AccountKind::DesktopPrimary || id.eq_ignore_ascii_case("primary") {
        "primary".to_owned()
    } else {
        id
    };

    let res = tauri::async_runtime::spawn_blocking(move || {
        let (_, launch_target) = resolve_tauri_launch_target();
        let target_opt = if target_id == "primary" {
            None
        } else {
            Some(&launch_target)
        };
        match app
            .coordinator
            .activate_desktop_account(&target_id, target_opt)
        {
            Ok(result) => ApiEnvelope::ok(result.message.clone(), result),
            Err(err) => ApiEnvelope::err(&err),
        }
    })
    .await
    .unwrap_or_else(join_err_envelope);

    emit_snapshot_changed(&app_handle);
    Ok(res)
}

#[tauri::command]
pub async fn set_account_pinned(
    app_handle: AppHandle,
    prefs: State<'_, DesktopPrefsState>,
    id: String,
    pinned: bool,
) -> Result<ApiEnvelope<DesktopPreferences>, String> {
    match prefs.set_account_pinned(&id, pinned) {
        Ok(updated) => {
            emit_snapshot_changed(&app_handle);
            Ok(ApiEnvelope::ok("Updated pinned accounts.", updated))
        }
        Err(err) => Ok(ApiEnvelope::err(&err)),
    }
}

fn companion_snap_primary_rejected() -> ApiEnvelope<CompanionSnapResult> {
    ApiEnvelope {
        ok: false,
        error_code: Some("PRIMARY_PROTECTED".to_owned()),
        message: "Companion snap is strictly forbidden on primary Zalo.".to_owned(),
        data: None,
    }
}

fn clamp_rail_width_px(rail_width_px: Option<u32>) -> i32 {
    rail_width_px.unwrap_or(240).clamp(48, 480) as i32
}

#[tauri::command]
pub async fn set_companion_snap(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
    rail_width_px: Option<u32>,
) -> Result<ApiEnvelope<CompanionSnapResult>, String> {
    if id.trim().eq_ignore_ascii_case("primary") {
        return Ok(companion_snap_primary_rejected());
    }

    let app = state.inner().clone();
    let host_hwnd = host_window_handle_u64(&app_handle);
    let rail_width = clamp_rail_width_px(rail_width_px);

    let res = tauri::async_runtime::spawn_blocking(move || {
        if enabled {
            match app
                .coordinator
                .companion_snap_profile(&id, host_hwnd, rail_width)
            {
                Ok(snap_res) => {
                    if snap_res.snapped {
                        if let Some(prev) = snap_res.previous_rect {
                            if let Ok(mut guard) = app.companion_snapped.lock() {
                                guard.insert(snap_res.profile_id.clone(), prev);
                            }
                        }
                    }
                    ApiEnvelope::ok(snap_res.message.clone(), snap_res)
                }
                Err(err) => ApiEnvelope::err(&err),
            }
        } else {
            let restore_rect = app
                .companion_snapped
                .lock()
                .ok()
                .and_then(|g| g.get(&id).copied());
            let Some(rect) = restore_rect else {
                return ApiEnvelope::ok(
                    "Profile is not companion-snapped.",
                    CompanionSnapResult {
                        profile_id: id,
                        snapped: false,
                        previous_rect: None,
                        applied_rect: None,
                        message: "Profile is not companion-snapped.".to_owned(),
                    },
                );
            };
            match app.coordinator.companion_unsnap_profile(&id, rect) {
                Ok(unsnap_res) => {
                    if let Ok(mut guard) = app.companion_snapped.lock() {
                        guard.remove(&unsnap_res.profile_id);
                    }
                    ApiEnvelope::ok(unsnap_res.message.clone(), unsnap_res)
                }
                Err(err) => ApiEnvelope::err(&err),
            }
        }
    })
    .await
    .unwrap_or_else(join_err_envelope);

    emit_snapshot_changed(&app_handle);
    Ok(res)
}

#[tauri::command]
pub async fn web_session_create(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    display_name: String,
    color_token: Option<String>,
) -> Result<ApiEnvelope<WebSessionRecord>, String> {
    match state
        .web_sessions
        .create(&display_name, color_token.as_deref())
    {
        Ok(rec) => {
            tray::refresh_tray_accounts(&app_handle);
            emit_snapshot_changed(&app_handle);
            Ok(ApiEnvelope::ok(
                format!("Created web session '{}'.", rec.display_name),
                rec,
            ))
        }
        Err(err) => Ok(ApiEnvelope::err(&err)),
    }
}

#[tauri::command]
pub async fn web_session_show(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    id: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<ApiEnvelope<WebSessionRecord>, String> {
    let rec = match state.web_sessions.get(&id) {
        Ok(r) => r,
        Err(err) => return Ok(ApiEnvelope::err(&err)),
    };
    let pending = state.web_sessions.pending_profile_deletes();
    match web_workspace::show_session(
        &app_handle,
        state.store.data_dir(),
        &rec,
        &pending,
        x,
        y,
        width,
        height,
    ) {
        Ok(()) => {
            emit_snapshot_changed(&app_handle);
            Ok(ApiEnvelope::ok("Web session shown.", rec))
        }
        Err(err) => Ok(ApiEnvelope::err(&err)),
    }
}

#[tauri::command]
pub async fn web_session_set_bounds(
    app_handle: AppHandle,
    id: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<ApiEnvelope<serde_json::Value>, String> {
    match web_workspace::set_bounds(&app_handle, &id, x, y, width, height) {
        Ok(()) => Ok(ApiEnvelope::ok(
            "Updated web session bounds.",
            serde_json::json!({ "id": id, "x": x, "y": y, "width": width, "height": height }),
        )),
        Err(err) => Ok(ApiEnvelope::err(&err)),
    }
}

#[tauri::command]
pub async fn web_session_hide(
    app_handle: AppHandle,
    id: String,
) -> Result<ApiEnvelope<serde_json::Value>, String> {
    match web_workspace::hide_session(&app_handle, &id) {
        Ok(()) => Ok(ApiEnvelope::ok(
            "Hid web session.",
            serde_json::json!({ "id": id }),
        )),
        Err(err) => Ok(ApiEnvelope::err(&err)),
    }
}

#[tauri::command]
pub async fn web_session_hide_all(
    app_handle: AppHandle,
) -> Result<ApiEnvelope<serde_json::Value>, String> {
    match web_workspace::hide_all_sessions(&app_handle) {
        Ok(()) => Ok(ApiEnvelope::ok(
            "Hid all web sessions.",
            serde_json::json!({ "hidden": true }),
        )),
        Err(err) => Ok(ApiEnvelope::err(&err)),
    }
}

#[tauri::command]
pub async fn web_session_reload(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<ApiEnvelope<serde_json::Value>, String> {
    let rec = match state.web_sessions.get(&id) {
        Ok(r) => r,
        Err(err) => return Ok(ApiEnvelope::err(&err)),
    };
    let pending = state.web_sessions.pending_profile_deletes();
    match web_workspace::reload_session(&app_handle, state.store.data_dir(), &rec, &pending) {
        Ok(()) => Ok(ApiEnvelope::ok(
            "Reloaded web session.",
            serde_json::json!({ "id": id }),
        )),
        Err(err) => Ok(ApiEnvelope::err(&err)),
    }
}

#[tauri::command]
pub async fn web_session_reset(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<ApiEnvelope<WebSessionRecord>, String> {
    let rec = match state.web_sessions.get(&id) {
        Ok(r) => r,
        Err(err) => return Ok(ApiEnvelope::err(&err)),
    };
    let pending =
        match web_workspace::teardown_and_delete_profile(&app_handle, state.store.data_dir(), &rec)
        {
            Ok(p) => p,
            Err(err) => return Ok(ApiEnvelope::err(&err)),
        };
    match state.web_sessions.bump_generation_for_reset(&id, pending) {
        Ok(updated) => {
            emit_snapshot_changed(&app_handle);
            Ok(ApiEnvelope::ok("Reset web session.", updated))
        }
        Err(err) => Ok(ApiEnvelope::err(&err)),
    }
}

#[tauri::command]
pub async fn web_session_delete(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    prefs: State<'_, DesktopPrefsState>,
    id: String,
) -> Result<ApiEnvelope<serde_json::Value>, String> {
    let rec = match state.web_sessions.get(&id) {
        Ok(r) => r,
        Err(err) => return Ok(ApiEnvelope::err(&err)),
    };
    let pending =
        match web_workspace::teardown_and_delete_profile(&app_handle, state.store.data_dir(), &rec)
        {
            Ok(p) => p,
            Err(err) => return Ok(ApiEnvelope::err(&err)),
        };
    match state.web_sessions.delete(&id, pending) {
        Ok(_) => {
            let _ = prefs.set_account_pinned(&id, false);
            tray::refresh_tray_accounts(&app_handle);
            emit_snapshot_changed(&app_handle);
            Ok(ApiEnvelope::ok(
                "Deleted web session.",
                serde_json::json!({ "deleted_session_id": id }),
            ))
        }
        Err(err) => Ok(ApiEnvelope::err(&err)),
    }
}

#[tauri::command]
pub async fn web_session_rename(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    id: String,
    display_name: String,
) -> Result<ApiEnvelope<WebSessionRecord>, String> {
    match state.web_sessions.rename(&id, &display_name) {
        Ok(updated) => {
            tray::refresh_tray_accounts(&app_handle);
            emit_snapshot_changed(&app_handle);
            Ok(ApiEnvelope::ok(
                format!("Renamed web session to '{}'.", updated.display_name),
                updated,
            ))
        }
        Err(err) => Ok(ApiEnvelope::err(&err)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arbitrary_fixture_script_is_rejected_by_sha256_allowlist() {
        let dir = std::env::temp_dir().join(format!("dim-cmd-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let arbitrary = dir.join("evil-arbitrary.mjs");
        fs::write(&arbitrary, "console.log('arbitrary script');\n")
            .expect("write arbitrary script");
        assert!(!is_allowlisted_fixture_script(&arbitrary));

        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let official_xf = manifest_dir
            .join("..")
            .join("..")
            .join("..")
            .join("fixtures")
            .join("cross-platform")
            .join("xf-fixture.mjs");
        if official_xf.exists() {
            assert!(is_allowlisted_fixture_script(&official_xf));
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn redact_dim_err_preserves_error_code_prefix() {
        let formatted = redact_dim_err(DimError::SchemaError("bad schema_version".to_owned()));
        assert!(formatted.starts_with("[SCHEMA_ERROR] "));
    }

    #[test]
    fn workspace_snapshot_fast_path_includes_desktop_web_and_pinned() {
        let data_dir = std::env::temp_dir().join(format!("khoang-cmd-snap-{}", std::process::id()));
        let _ = fs::remove_dir_all(&data_dir);
        fs::create_dir_all(&data_dir).expect("create temp dir");

        let state = AppState::from_data_dir(Some(&data_dir)).expect("init AppState");
        let prefs = DesktopPrefsState::initialize(&data_dir);
        let desktop_prof = state
            .store
            .create_profile("Snapshot Desktop", "")
            .expect("create desktop profile");
        let web_sess = state
            .web_sessions
            .create("Snapshot Web", Some("emerald"))
            .expect("create web session");
        prefs
            .set_account_pinned(&web_sess.id, true)
            .expect("pin web session");

        let snap = build_workspace_snapshot_core(
            &state,
            &prefs,
            WebWorkspaceSupport {
                supported: true,
                reason_code: None,
            },
        )
        .expect("build workspace snapshot");

        assert!(snap.web_workspace.supported);
        assert_eq!(snap.accounts.len(), 2);
        let d_item = snap
            .accounts
            .iter()
            .find(|a| a.id == desktop_prof.id)
            .expect("desktop account present");
        assert_eq!(d_item.kind, AccountKind::DesktopIsolated);
        assert!(!d_item.pinned);

        let w_item = snap
            .accounts
            .iter()
            .find(|a| a.id == web_sess.id)
            .expect("web account present");
        assert_eq!(w_item.kind, AccountKind::WebIsolated);
        assert!(w_item.pinned);
        assert_eq!(w_item.color_token.as_deref(), Some("emerald"));
        assert_eq!(w_item.state, "stopped");

        web_workspace::set_session_mounted_for_test(&web_sess.id, true);
        let snap_mounted = build_workspace_snapshot_core(
            &state,
            &prefs,
            WebWorkspaceSupport {
                supported: true,
                reason_code: None,
            },
        )
        .expect("build workspace snapshot when mounted");
        let w_mounted = snap_mounted
            .accounts
            .iter()
            .find(|a| a.id == web_sess.id)
            .expect("web account present");
        assert_eq!(w_mounted.state, "running");
        web_workspace::set_session_mounted_for_test(&web_sess.id, false);

        let _ = fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn companion_snap_rejects_primary_with_primary_protected() {
        let env = companion_snap_primary_rejected();
        assert!(!env.ok);
        assert_eq!(env.error_code.as_deref(), Some("PRIMARY_PROTECTED"));
        assert_eq!(clamp_rail_width_px(None), 240);
        assert_eq!(clamp_rail_width_px(Some(12)), 48);
        assert_eq!(clamp_rail_width_px(Some(320)), 320);
        assert_eq!(clamp_rail_width_px(Some(999)), 480);
    }

    #[test]
    fn capabilities_default_json_grants_only_event_listen_and_unlisten() {
        let cap_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("capabilities")
            .join("default.json");
        let raw = fs::read_to_string(&cap_path).expect("read capabilities/default.json");
        let parsed: serde_json::Value =
            serde_json::from_str(&raw).expect("parse capabilities/default.json");
        assert_eq!(parsed["windows"], serde_json::json!(["main"]));
        assert_eq!(
            parsed["permissions"],
            serde_json::json!(["core:event:allow-listen", "core:event:allow-unlisten"])
        );
    }
}

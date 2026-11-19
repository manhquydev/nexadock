//! Tauri 2 IPC commands delegating directly to `dim-core`, `dim-profile-store`,
//! `dim-process-manager`, and the active OS platform adapter.

use std::fs;
use std::path::PathBuf;

use dim_core::{
    ensure_not_primary_zalo_path, redact_sensitive_text, resolve_safe_child,
    validate_directory_input, ApplicationLocator, BackupManager, BackupManifest, DimError,
    InstanceLauncher, InstanceMonitor, LaunchTarget, PlatformCapabilities, PlatformKind,
    ProfileStore,
};
use serde::Serialize;
use tauri::State;

use crate::autostart;
use crate::preferences::{DesktopPreferences, DesktopPrefsState};
pub use crate::state::AppState;
use crate::tray;

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
    state: State<'_, AppState>,
    name: String,
    notes: Option<String>,
) -> Result<ApiEnvelope<dim_core::ProfileRecord>, String> {
    let app = state.inner().clone();
    Ok(tauri::async_runtime::spawn_blocking(move || {
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
    .unwrap_or_else(join_err_envelope))
}

#[tauri::command]
pub async fn rename_profile(
    state: State<'_, AppState>,
    id: String,
    new_name: String,
) -> Result<ApiEnvelope<dim_core::ProfileRecord>, String> {
    let app = state.inner().clone();
    Ok(tauri::async_runtime::spawn_blocking(move || {
        match app.store.rename_profile(&id, &new_name) {
            Ok(rec) => ApiEnvelope::ok(format!("Renamed profile to '{}'.", rec.name), rec),
            Err(err) => ApiEnvelope::err(&err),
        }
    })
    .await
    .unwrap_or_else(join_err_envelope))
}

#[tauri::command]
pub async fn delete_profile(
    state: State<'_, AppState>,
    id: String,
    confirm: Option<bool>,
) -> Result<ApiEnvelope<serde_json::Value>, String> {
    let app = state.inner().clone();
    Ok(tauri::async_runtime::spawn_blocking(move || {
        match app
            .coordinator
            .delete_profile(&id, confirm.unwrap_or(false))
        {
            Ok(()) => ApiEnvelope::ok(
                "Deleted profile.",
                serde_json::json!({ "deleted_profile_id": id }),
            ),
            Err(err) => ApiEnvelope::err(&err),
        }
    })
    .await
    .unwrap_or_else(join_err_envelope))
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
    state: State<'_, AppState>,
    backup_dir: String,
    name: Option<String>,
) -> Result<ApiEnvelope<dim_core::ProfileRecord>, String> {
    let app = state.inner().clone();
    Ok(tauri::async_runtime::spawn_blocking(move || {
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
    .unwrap_or_else(join_err_envelope))
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
}

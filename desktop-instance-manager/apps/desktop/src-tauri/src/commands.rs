//! Tauri 2 IPC commands delegating directly to `dim-core`, `dim-profile-store`,
//! `dim-process-manager`, and the active OS platform adapter.

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use dim_core::{
    ensure_not_primary_zalo_path, redact_sensitive_text, resolve_safe_child,
    validate_directory_input, ApplicationLocator, BackupManager, BackupManifest, DimError,
    InstanceLauncher, InstanceMonitor, LaunchTarget, PlatformCapabilities, PlatformKind,
    ProfileStore,
};
use dim_process_manager::InstanceCoordinator;
use dim_profile_store::{FileBackupManager, JsonProfileStore};
use serde::Serialize;
use tauri::State;

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
use dim_platform_linux::LinuxPlatformAdapter as ActiveAdapter;
#[cfg(target_os = "macos")]
use dim_platform_macos::MacOsPlatformAdapter as ActiveAdapter;
#[cfg(target_os = "windows")]
use dim_platform_windows::WindowsPlatformAdapter as ActiveAdapter;

/// Shared application state managed by Tauri.
pub struct AppState {
    pub store: Arc<JsonProfileStore>,
    pub adapter: Arc<ActiveAdapter>,
    pub coordinator: InstanceCoordinator<ActiveAdapter>,
    pub backups: FileBackupManager,
    pub op_lock: Mutex<()>,
}

impl AppState {
    /// Initializes store, platform adapter, coordinator, and backup manager from `DIM_DATA_DIR` or OS defaults.
    pub fn initialize() -> Result<Self, DimError> {
        let store = Arc::new(JsonProfileStore::from_default_or_env(None)?);
        store.ensure_initialized()?;

        let adapter = Arc::new(ActiveAdapter::new());
        let coordinator = InstanceCoordinator::new(Arc::clone(&store), Arc::clone(&adapter));

        let adapter_for_probe = Arc::clone(&adapter);
        let backups = FileBackupManager::new(Arc::clone(&store)).with_disk_space_probe(Arc::new(
            move |path| adapter_for_probe.free_disk_bytes(path),
        ));

        Ok(Self {
            store,
            adapter,
            coordinator,
            backups,
            op_lock: Mutex::new(()),
        })
    }
}

fn current_user_home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

fn redact_err_text(err: impl std::fmt::Display) -> String {
    let home = current_user_home();
    redact_sensitive_text(&err.to_string(), home.as_deref())
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
pub fn get_status(
    state: State<'_, AppState>,
    with_size: Option<bool>,
) -> Result<StatusResponse, String> {
    let measure_size = with_size.unwrap_or(false);
    let installation = state
        .adapter
        .discover_installation(None)
        .map_err(redact_err_text)?;
    let primary = state.adapter.inspect_primary().map_err(redact_err_text)?;
    let records = state.store.list_profiles().map_err(redact_err_text)?;

    let mut profiles = Vec::with_capacity(records.len());
    for rec in records {
        let dir = state.store.profile_dir_for(&rec).map_err(redact_err_text)?;
        let rt = state
            .adapter
            .inspect_profile(&rec, &dir)
            .map_err(redact_err_text)?;
        let data_size = if measure_size {
            Some(rt.disk_usage_bytes)
        } else {
            None
        };
        profiles.push(serde_json::json!({
            "id": rec.id,
            "name": rec.name,
            "directory_name": rec.directory_name,
            "created_at": rec.created_at,
            "last_launched_at": rec.last_launched_at,
            "zalo_user_id": rt.detected_zalo_user_id.or(rec.zalo_user_id),
            "notes": rec.notes,
            "running": rt.running,
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
        platform: state.adapter.platform().as_str().to_owned(),
        data_dir: state.store.data_dir().display().to_string(),
        installation: serde_json::json!({
            "valid": installation.is_valid,
            "mode": "official-zalo",
            "version": installation.version,
            "authenticode_status": installation.signature_status,
            "signer_subject": installation.signer_subject,
            "error": installation.error,
        }),
        primary_zalo: serde_json::json!({
            "is_running": primary.running,
            "locking_pids": primary.protected_pids,
            "process_count": primary.process_count,
            "total_working_set_bytes": primary.working_set_bytes,
            "zalo_user_id": primary.detected_zalo_user_id,
        }),
        profiles,
    })
}

fn acquire_op_lock(state: &AppState) -> Result<std::sync::MutexGuard<'_, ()>, DimError> {
    state
        .op_lock
        .lock()
        .map_err(|_| DimError::Io(std::io::Error::other("operation mutex poisoned")))
}

#[tauri::command]
pub fn create_profile(
    state: State<'_, AppState>,
    name: String,
    notes: Option<String>,
) -> ApiEnvelope<dim_core::ProfileRecord> {
    let _op = match acquire_op_lock(&state) {
        Ok(g) => g,
        Err(err) => return ApiEnvelope::err(&err),
    };
    match state
        .store
        .create_profile(&name, notes.as_deref().unwrap_or(""))
    {
        Ok(rec) => {
            if let Ok(dir) = state.store.profile_dir_for(&rec) {
                let _ = state.adapter.ensure_profile_layout(&rec, &dir);
            }
            ApiEnvelope::ok(format!("Created profile '{}'.", rec.name), rec)
        }
        Err(err) => ApiEnvelope::err(&err),
    }
}

#[tauri::command]
pub fn rename_profile(
    state: State<'_, AppState>,
    id: String,
    new_name: String,
) -> ApiEnvelope<dim_core::ProfileRecord> {
    let _op = match acquire_op_lock(&state) {
        Ok(g) => g,
        Err(err) => return ApiEnvelope::err(&err),
    };
    match state.store.rename_profile(&id, &new_name) {
        Ok(rec) => ApiEnvelope::ok(format!("Renamed profile to '{}'.", rec.name), rec),
        Err(err) => ApiEnvelope::err(&err),
    }
}

#[tauri::command]
pub fn delete_profile(
    state: State<'_, AppState>,
    id: String,
    confirm: Option<bool>,
) -> ApiEnvelope<serde_json::Value> {
    let _op = match acquire_op_lock(&state) {
        Ok(g) => g,
        Err(err) => return ApiEnvelope::err(&err),
    };
    match state
        .coordinator
        .delete_profile(&id, confirm.unwrap_or(false))
    {
        Ok(()) => ApiEnvelope::ok(
            "Deleted profile.",
            serde_json::json!({ "deleted_profile_id": id }),
        ),
        Err(err) => ApiEnvelope::err(&err),
    }
}

#[tauri::command]
pub fn launch_profile(
    state: State<'_, AppState>,
    id: String,
) -> ApiEnvelope<dim_core::ProfileRuntimeState> {
    let _op = match acquire_op_lock(&state) {
        Ok(g) => g,
        Err(err) => return ApiEnvelope::err(&err),
    };
    let target = LaunchTarget::OfficialZalo {
        custom_install_root: None,
    };
    match state.coordinator.launch_profile(&id, &target) {
        Ok(rt) => ApiEnvelope::ok("Launched profile.", rt),
        Err(err) => ApiEnvelope::err(&err),
    }
}

#[tauri::command]
pub fn focus_profile(
    state: State<'_, AppState>,
    id: String,
) -> ApiEnvelope<dim_core::ProfileRuntimeState> {
    let target = LaunchTarget::OfficialZalo {
        custom_install_root: None,
    };
    match state.coordinator.focus_profile(&id, &target) {
        Ok(rt) => ApiEnvelope::ok("Focused profile.", rt),
        Err(err) => ApiEnvelope::err(&err),
    }
}

#[tauri::command]
pub fn stop_profile(state: State<'_, AppState>, id: String) -> ApiEnvelope<u32> {
    let _op = match acquire_op_lock(&state) {
        Ok(g) => g,
        Err(err) => return ApiEnvelope::err(&err),
    };
    match state.coordinator.stop_profile(&id) {
        Ok(count) => ApiEnvelope::ok("Stopped profile.", count),
        Err(err) => ApiEnvelope::err(&err),
    }
}

#[tauri::command]
pub fn backup_profile(
    state: State<'_, AppState>,
    id: String,
) -> ApiEnvelope<dim_core::BackupResult> {
    let _op = match acquire_op_lock(&state) {
        Ok(g) => g,
        Err(err) => return ApiEnvelope::err(&err),
    };
    let result = (|| -> Result<dim_core::BackupResult, DimError> {
        let profile = state.store.get_profile(&id)?;
        let dir = state.store.profile_dir_for(&profile)?;
        let rt = state.adapter.inspect_profile(&profile, &dir)?;
        state
            .backups
            .backup_profile(&profile, &dir, None, rt.running)
    })();
    match result {
        Ok(bk) => ApiEnvelope::ok("Created offline atomic backup.", bk),
        Err(err) => ApiEnvelope::err(&err),
    }
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
pub fn restore_backup(
    state: State<'_, AppState>,
    backup_dir: String,
    name: Option<String>,
) -> ApiEnvelope<dim_core::ProfileRecord> {
    let _op = match acquire_op_lock(&state) {
        Ok(g) => g,
        Err(err) => return ApiEnvelope::err(&err),
    };
    let trimmed_dir = backup_dir.trim();
    let candidate = PathBuf::from(trimmed_dir);
    if let Err(err) = validate_directory_input(&candidate) {
        return ApiEnvelope::err(&err);
    }

    let backups_root = state.store.backups_dir();
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
    match state
        .backups
        .restore_backup_as_new_profile(&resolved_dir, trimmed)
    {
        Ok(rec) => ApiEnvelope::ok(format!("Restored backup into '{}'.", rec.name), rec),
        Err(err) => ApiEnvelope::err(&err),
    }
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
    let platform_status = match caps.overall_status {
        dim_core::SupportStatus::SupportedAndTested => "SUPPORTED_AND_TESTED",
        dim_core::SupportStatus::PartiallySupported => "PARTIALLY_SUPPORTED",
        dim_core::SupportStatus::Experimental => "EXPERIMENTAL",
        dim_core::SupportStatus::Blocked => "BLOCKED",
        dim_core::SupportStatus::Unsupported => "UNSUPPORTED",
        dim_core::SupportStatus::NotTested => "NOT_TESTED",
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
            {
                "platform": match caps.platform {
                    PlatformKind::Windows => "Windows 10/11",
                    PlatformKind::MacOs => "macOS (Apple Silicon & Intel)",
                    PlatformKind::Linux => "Ubuntu / Linux",
                },
                "status": platform_status,
                "strategy": "Native Rust core + OS process isolation",
                "limits": caps.limitations.join("; "),
            },
            {
                "platform": "macOS (Apple Silicon & Intel)",
                "status": "NOT_TESTED",
                "strategy": "open -n -a /Applications/Zalo.app --env CFFIXED_USER_HOME=<profile_home>",
                "limits": "Requires physical Mac verification; /tmp/socketzalorecv2021 call socket conflict.",
            },
            {
                "platform": "Ubuntu LTS (Official Zalo)",
                "status": "UNSUPPORTED",
                "strategy": "No official VNG Linux desktop build exists",
                "limits": "Official VNG Linux package does not exist.",
            },
            {
                "platform": "Ubuntu LTS (Fixture / Local AppImage)",
                "status": "EXPERIMENTAL",
                "strategy": "XDG_CONFIG_HOME / HOME / optional bwrap",
                "limits": "Ubuntu 24.04 AppArmor may require --no-sandbox on local AppImages; never bundle proprietary VNG app.asar.",
            }
        ],
        "sanitized_logs": sanitized_logs,
    }))
}

#[tauri::command]
pub fn verify_isolation(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let report = state
        .coordinator
        .verify_isolation(&*state.adapter, None, None)
        .map_err(redact_err_text)?;
    serde_json::to_value(report).map_err(redact_err_text)
}

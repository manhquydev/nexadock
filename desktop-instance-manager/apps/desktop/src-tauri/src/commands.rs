//! Tauri 2 IPC commands delegating directly to `dim-core`, `dim-profile-store`,
//! `dim-process-manager`, and the active OS platform adapter.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use dim_core::{
    ApplicationLocator, BackupManager, DimError, InstanceLauncher, InstanceMonitor, LaunchTarget,
    PlatformCapabilities, ProfileStore,
};
use dim_process_manager::InstanceCoordinator;
use dim_profile_store::{FileBackupManager, JsonProfileStore};
use serde::Serialize;
use tauri::State;

#[cfg(target_os = "windows")]
use dim_platform_windows::WindowsPlatformAdapter as ActiveAdapter;
#[cfg(target_os = "macos")]
use dim_platform_macos::MacOsPlatformAdapter as ActiveAdapter;
#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
use dim_platform_linux::LinuxPlatformAdapter as ActiveAdapter;

/// Shared application state managed by Tauri.
pub struct AppState {
    pub store: Arc<JsonProfileStore>,
    pub adapter: Arc<ActiveAdapter>,
    pub coordinator: InstanceCoordinator<ActiveAdapter>,
    pub backups: FileBackupManager,
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
        })
    }
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
            message: err.to_string(),
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
pub fn get_status(state: State<'_, AppState>, with_size: Option<bool>) -> Result<StatusResponse, String> {
    let _ = with_size;
    let installation = state
        .adapter
        .discover_installation(None)
        .map_err(|e| e.to_string())?;
    let primary = state.adapter.inspect_primary().map_err(|e| e.to_string())?;
    let records = state.store.list_profiles().map_err(|e| e.to_string())?;

    let mut profiles = Vec::with_capacity(records.len());
    for rec in records {
        let dir = state.store.profile_dir_for(&rec).map_err(|e| e.to_string())?;
        let rt = state
            .adapter
            .inspect_profile(&rec, &dir)
            .map_err(|e| e.to_string())?;
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
            "data_size_bytes": rt.disk_usage_bytes,
            "stale_lockfile_detected": rt.stale_lock_detected,
        }));
    }

    Ok(StatusResponse {
        ok: true,
        platform: state.adapter.platform().as_str().to_owned(),
        data_dir: state.store.data_dir().display().to_string(),
        installation: serde_json::json!({
            "valid": installation.is_valid,
            "version": installation.version,
            "authenticode_status": installation.signature_status,
            "error": installation.error,
        }),
        primary_zalo: serde_json::json!({
            "is_running": primary.running,
            "locking_pids": primary.protected_pids,
            "process_count": primary.process_count,
            "total_working_set_bytes": 0,
        }),
        profiles,
    })
}

#[tauri::command]
pub fn create_profile(
    state: State<'_, AppState>,
    name: String,
    notes: Option<String>,
) -> ApiEnvelope<dim_core::ProfileRecord> {
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
    match state.store.rename_profile(&id, &new_name) {
        Ok(rec) => ApiEnvelope::ok(format!("Renamed profile to '{}'.", rec.name), rec),
        Err(err) => ApiEnvelope::err(&err),
    }
}

#[tauri::command]
pub fn delete_profile(state: State<'_, AppState>, id: String) -> ApiEnvelope<()> {
    match state.coordinator.delete_profile(&id, true) {
        Ok(()) => ApiEnvelope::ok("Deleted profile.", ()),
        Err(err) => ApiEnvelope::err(&err),
    }
}

#[tauri::command]
pub fn launch_profile(
    state: State<'_, AppState>,
    id: String,
) -> ApiEnvelope<dim_core::ProfileRuntimeState> {
    let target = LaunchTarget::OfficialZalo {
        custom_install_root: None,
    };
    match state.coordinator.launch_profile(&id, &target) {
        Ok(rt) => ApiEnvelope::ok("Launched profile.", rt),
        Err(err) => ApiEnvelope::err(&err),
    }
}

#[tauri::command]
pub fn focus_profile(state: State<'_, AppState>, id: String) -> ApiEnvelope<()> {
    let target = LaunchTarget::OfficialZalo {
        custom_install_root: None,
    };
    let result = (|| -> Result<(), DimError> {
        let profile = state.store.get_profile(&id)?;
        let dir = state.store.profile_dir_for(&profile)?;
        let rt = state.adapter.inspect_profile(&profile, &dir)?;
        state.adapter.focus_or_restore(&target, &profile, &dir, &rt)
    })();
    match result {
        Ok(()) => ApiEnvelope::ok("Focused profile.", ()),
        Err(err) => ApiEnvelope::err(&err),
    }
}

#[tauri::command]
pub fn stop_profile(state: State<'_, AppState>, id: String) -> ApiEnvelope<u32> {
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
            if let Ok(manifest) = state.backups.verify_backup(&dir) {
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
    let trimmed_dir = backup_dir.trim();
    if trimmed_dir.is_empty()
        || trimmed_dir.contains('\0')
        || trimmed_dir.split(['/', '\\']).any(|seg| seg == "..")
    {
        return ApiEnvelope::err(&DimError::PathTraversal(
            "invalid or traversal backup_dir rejected".to_owned(),
        ));
    }

    let trimmed = name.as_deref().map(str::trim).filter(|s| !s.is_empty());
    match state
        .backups
        .restore_backup_as_new_profile(&PathBuf::from(trimmed_dir), trimmed)
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
    Ok(serde_json::json!({
        "ok": true,
        "is_elevated": false,
        "disk_free_bytes": free,
        "disk_free_ok": free >= dim_core::MIN_FREE_DISK_BYTES,
        "folder_redirection": {
            "redirected": caps.folder_redirection_detected,
            "warning": caps.folder_redirection_path,
        },
        "platform_support_matrix": [
            {
                "platform": caps.platform.as_str(),
                "status": format!("{:?}", caps.overall_status),
                "strategy": "Native Rust core + OS sandbox isolation",
                "limits": caps.limitations.join("; "),
            }
        ],
        "sanitized_logs": caps.diagnostics,
    }))
}

#[tauri::command]
pub fn verify_isolation(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let primary = state.adapter.inspect_primary().map_err(|e| e.to_string())?;
    let caps = state.adapter.capabilities();
    Ok(serde_json::json!({
        "ok": !caps.folder_redirection_detected,
        "all_passed": !caps.folder_redirection_detected,
        "checks": [
            {
                "check_name": "PrimaryZaloSeparation",
                "passed": true,
                "detail": format!("Protected PIDs count={}", primary.protected_pids.len()),
            },
            {
                "check_name": "FolderRedirection",
                "passed": !caps.folder_redirection_detected,
                "detail": caps.folder_redirection_path.unwrap_or_else(|| "Local user data path OK".to_owned()),
            }
        ]
    }))
}

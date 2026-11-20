//! Shared Tauri application state with per-profile synchronization.
//!
//! `AppState` intentionally avoids any coarse global operation `Mutex`.
//! - Profile metadata mutations synchronize via `<data_dir>/profiles.lock` inside `JsonProfileStore`.
//! - Profile lifecycle operations (`launch`, `stop`, `focus`, `delete`) synchronize per-profile
//!   inside `Arc<InstanceCoordinator>` (in-process keyed table + cross-process `.dim-profile.lock`),
//!   allowing distinct profiles to launch, stop, and back up concurrently without blocking one another.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dim_core::{
    generate_profile_id, now_rfc3339_utc, write_bytes_atomic, AdvisoryFileLock, BackupResult,
    DimError, PlatformCapabilities, ProfileLifecycleState, ProfileStore, WebSessionRecord,
    WindowRect,
};
use dim_process_manager::InstanceCoordinator;
use dim_profile_store::{FileBackupManager, JsonProfileStore};
use serde::{Deserialize, Serialize};

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
pub use dim_platform_linux::LinuxPlatformAdapter as ActiveAdapter;
#[cfg(target_os = "macos")]
pub use dim_platform_macos::MacOsPlatformAdapter as ActiveAdapter;
#[cfg(target_os = "windows")]
pub use dim_platform_windows::WindowsPlatformAdapter as ActiveAdapter;

const PROFILE_BACKUP_LOCK_TIMEOUT: Duration = Duration::from_secs(15);
pub const WEB_SESSIONS_FILE_NAME: &str = "web-sessions.json";
pub const WEB_SESSIONS_SCHEMA_VERSION: u32 = 1;

/// Root document persisted in `<data_dir>/web-sessions.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct WebSessionsDocument {
    pub schema_version: u32,
    pub sessions: Vec<WebSessionRecord>,
    /// Windows WebView2 profile folder names queued for cleanup before opening `_shared_env`.
    pub pending_profile_deletes: Vec<String>,
}

impl Default for WebSessionsDocument {
    fn default() -> Self {
        Self {
            schema_version: WEB_SESSIONS_SCHEMA_VERSION,
            sessions: Vec::new(),
            pending_profile_deletes: Vec::new(),
        }
    }
}

/// Thread-safe store for isolated web session records (`<data_dir>/web-sessions.json`).
pub struct WebSessionStore {
    data_dir: PathBuf,
    doc: Mutex<WebSessionsDocument>,
}

impl WebSessionStore {
    pub fn initialize(data_dir: &Path) -> Self {
        let loaded = load_web_sessions(data_dir);
        Self {
            data_dir: data_dir.to_path_buf(),
            doc: Mutex::new(loaded),
        }
    }

    pub fn list(&self) -> Vec<WebSessionRecord> {
        self.doc
            .lock()
            .map(|g| g.sessions.clone())
            .unwrap_or_default()
    }

    pub fn get(&self, id: &str) -> Result<WebSessionRecord, DimError> {
        if !is_valid_web_session_id(id) {
            return Err(DimError::InvalidProfileName(format!(
                "invalid web session id '{id}'"
            )));
        }
        let guard = self
            .doc
            .lock()
            .map_err(|_| DimError::Io(std::io::Error::other("web sessions lock poisoned")))?;
        guard
            .sessions
            .iter()
            .find(|s| s.id == id)
            .cloned()
            .ok_or_else(|| DimError::ProfileNotFound(format!("web session '{id}' not found")))
    }

    pub fn create(
        &self,
        display_name: &str,
        color_token: Option<&str>,
    ) -> Result<WebSessionRecord, DimError> {
        let normalized_name = validate_web_session_name(display_name)?;
        let normalized_color = normalize_color_token(color_token);
        let mut guard = self
            .doc
            .lock()
            .map_err(|_| DimError::Io(std::io::Error::other("web sessions lock poisoned")))?;

        let lower = normalized_name.to_lowercase();
        if guard
            .sessions
            .iter()
            .any(|s| s.display_name.to_lowercase() == lower)
        {
            return Err(DimError::DuplicateProfileName(normalized_name));
        }

        let now = now_rfc3339_utc();
        let mut id = format!("web-{}", generate_profile_id());
        while guard.sessions.iter().any(|s| s.id == id) {
            id = format!("web-{}", generate_profile_id());
        }
        let record = WebSessionRecord {
            id,
            display_name: normalized_name,
            color_token: normalized_color,
            profile_generation: 1,
            data_store_uuid: Some(generate_uuid_hex()),
            created_at: now.clone(),
            updated_at: now,
        };
        let mut next = guard.clone();
        next.sessions.push(record.clone());
        save_web_sessions(&self.data_dir, &next)?;
        *guard = next;
        Ok(record)
    }

    pub fn rename(&self, id: &str, display_name: &str) -> Result<WebSessionRecord, DimError> {
        if !is_valid_web_session_id(id) {
            return Err(DimError::InvalidProfileName(format!(
                "invalid web session id '{id}'"
            )));
        }
        let normalized_name = validate_web_session_name(display_name)?;
        let mut guard = self
            .doc
            .lock()
            .map_err(|_| DimError::Io(std::io::Error::other("web sessions lock poisoned")))?;
        let lower = normalized_name.to_lowercase();
        if guard
            .sessions
            .iter()
            .any(|s| s.id != id && s.display_name.to_lowercase() == lower)
        {
            return Err(DimError::DuplicateProfileName(normalized_name));
        }
        let mut next = guard.clone();
        let Some(entry) = next.sessions.iter_mut().find(|s| s.id == id) else {
            return Err(DimError::ProfileNotFound(format!(
                "web session '{id}' not found"
            )));
        };
        entry.display_name = normalized_name;
        entry.updated_at = now_rfc3339_utc();
        let updated = entry.clone();
        save_web_sessions(&self.data_dir, &next)?;
        *guard = next;
        Ok(updated)
    }

    pub fn bump_generation_for_reset(
        &self,
        id: &str,
        pending_profile_delete: Option<String>,
    ) -> Result<WebSessionRecord, DimError> {
        if !is_valid_web_session_id(id) {
            return Err(DimError::InvalidProfileName(format!(
                "invalid web session id '{id}'"
            )));
        }
        let mut guard = self
            .doc
            .lock()
            .map_err(|_| DimError::Io(std::io::Error::other("web sessions lock poisoned")))?;
        let mut next = guard.clone();
        let Some(entry) = next.sessions.iter_mut().find(|s| s.id == id) else {
            return Err(DimError::ProfileNotFound(format!(
                "web session '{id}' not found"
            )));
        };
        entry.profile_generation = entry.profile_generation.saturating_add(1);
        entry.data_store_uuid = Some(generate_uuid_hex());
        entry.updated_at = now_rfc3339_utc();
        let updated = entry.clone();
        if let Some(name) = pending_profile_delete {
            if !name.is_empty() && !next.pending_profile_deletes.iter().any(|p| p == &name) {
                next.pending_profile_deletes.push(name);
            }
        }
        save_web_sessions(&self.data_dir, &next)?;
        *guard = next;
        Ok(updated)
    }

    pub fn delete(
        &self,
        id: &str,
        pending_profile_delete: Option<String>,
    ) -> Result<WebSessionRecord, DimError> {
        if !is_valid_web_session_id(id) {
            return Err(DimError::InvalidProfileName(format!(
                "invalid web session id '{id}'"
            )));
        }
        let mut guard = self
            .doc
            .lock()
            .map_err(|_| DimError::Io(std::io::Error::other("web sessions lock poisoned")))?;
        let mut next = guard.clone();
        let Some(pos) = next.sessions.iter().position(|s| s.id == id) else {
            return Err(DimError::ProfileNotFound(format!(
                "web session '{id}' not found"
            )));
        };
        let removed = next.sessions.remove(pos);
        if let Some(name) = pending_profile_delete {
            if !name.is_empty() && !next.pending_profile_deletes.iter().any(|p| p == &name) {
                next.pending_profile_deletes.push(name);
            }
        }
        save_web_sessions(&self.data_dir, &next)?;
        *guard = next;
        Ok(removed)
    }

    pub fn pending_profile_deletes(&self) -> Vec<String> {
        self.doc
            .lock()
            .map(|g| g.pending_profile_deletes.clone())
            .unwrap_or_default()
    }

    pub fn remove_pending_profile_deletes(&self, cleared: &[String]) -> Result<(), DimError> {
        if cleared.is_empty() {
            return Ok(());
        }
        let mut guard = self
            .doc
            .lock()
            .map_err(|_| DimError::Io(std::io::Error::other("web sessions lock poisoned")))?;
        let mut next = guard.clone();
        next.pending_profile_deletes
            .retain(|name| !cleared.iter().any(|c| c == name));
        save_web_sessions(&self.data_dir, &next)?;
        *guard = next;
        Ok(())
    }
}

/// Validates that a web session ID has the canonical `web-<8 lowercase hex>` format (12 ASCII bytes).
pub fn is_valid_web_session_id(id: &str) -> bool {
    id.strip_prefix("web-")
        .is_some_and(dim_core::is_valid_profile_id)
}

fn validate_web_session_name(raw: &str) -> Result<String, DimError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(DimError::InvalidProfileName(
            "Web session display name cannot be empty.".to_owned(),
        ));
    }
    if trimmed.chars().count() > 80 {
        return Err(DimError::InvalidProfileName(
            "Web session display name must be at most 80 characters.".to_owned(),
        ));
    }
    if trimmed
        .chars()
        .any(|c| c == '\0' || c.is_control() || c == '/' || c == '\\')
        || trimmed.contains("..")
    {
        return Err(DimError::InvalidProfileName(
            "Web session display name contains invalid control or path characters.".to_owned(),
        ));
    }
    Ok(trimmed.to_owned())
}

fn normalize_color_token(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|s| !s.is_empty() && s.len() <= 32)
        .map(str::to_owned)
}

fn generate_uuid_hex() -> String {
    format!(
        "{}{}{}{}",
        generate_profile_id(),
        generate_profile_id(),
        generate_profile_id(),
        generate_profile_id()
    )
}

fn load_web_sessions(data_dir: &Path) -> WebSessionsDocument {
    let path = data_dir.join(WEB_SESSIONS_FILE_NAME);
    let Ok(raw) = fs::read_to_string(&path) else {
        return WebSessionsDocument::default();
    };
    let mut parsed: WebSessionsDocument = serde_json::from_str(&raw).unwrap_or_default();
    if parsed.schema_version == 0 {
        parsed.schema_version = WEB_SESSIONS_SCHEMA_VERSION;
    }
    parsed.sessions.retain(|s| is_valid_web_session_id(&s.id));
    parsed
}

fn save_web_sessions(data_dir: &Path, doc: &WebSessionsDocument) -> Result<(), DimError> {
    fs::create_dir_all(data_dir)?;
    let target = data_dir.join(WEB_SESSIONS_FILE_NAME);
    let payload = serde_json::to_string_pretty(doc)?;
    write_bytes_atomic(&target, payload.as_bytes())
}

/// Cloneable shared application state managed by Tauri.
#[derive(Clone)]
pub struct AppState {
    pub store: Arc<JsonProfileStore>,
    pub adapter: Arc<ActiveAdapter>,
    pub coordinator: Arc<InstanceCoordinator<ActiveAdapter>>,
    pub backups: Arc<FileBackupManager>,
    pub web_sessions: Arc<WebSessionStore>,
    /// Original `WindowRect` before companion snapping, keyed by secondary profile ID.
    pub companion_snapped: Arc<Mutex<HashMap<String, WindowRect>>>,
}

impl AppState {
    /// Initializes store, platform adapter, coordinator, and backup manager from `DIM_DATA_DIR` or OS defaults.
    pub fn initialize() -> Result<Self, DimError> {
        Self::from_data_dir(None)
    }

    /// Initializes state rooted at an explicit `data_dir` override (or `DIM_DATA_DIR` / OS default when `None`).
    pub fn from_data_dir(data_dir: Option<&Path>) -> Result<Self, DimError> {
        let store = Arc::new(JsonProfileStore::from_default_or_env(data_dir)?);
        let _ = store.ensure_initialized();

        let adapter = Arc::new(ActiveAdapter::new());
        let coordinator = Arc::new(InstanceCoordinator::new(
            Arc::clone(&store),
            Arc::clone(&adapter),
        ));

        let adapter_for_probe = Arc::clone(&adapter);
        let backups = Arc::new(
            FileBackupManager::new(Arc::clone(&store)).with_disk_space_probe(Arc::new(
                move |path| adapter_for_probe.free_disk_bytes(path),
            )),
        );

        let web_sessions = Arc::new(WebSessionStore::initialize(store.data_dir()));

        Ok(Self {
            store,
            adapter,
            coordinator,
            backups,
            web_sessions,
            companion_snapped: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Creates an offline atomic backup synchronized on `<data_dir>/locks/<profile_id>.lock`
    /// (outside `Profiles/`) so concurrent launches/deletes on the same profile serialize cleanly
    /// without blocking other profiles or resurrecting a deleted profile directory.
    pub fn backup_profile_synchronized(
        &self,
        id: &str,
        allow_external_db_omit: bool,
    ) -> Result<BackupResult, DimError> {
        let initial_profile = self.store.get_profile(id)?;
        let _profile_op_guard = AdvisoryFileLock::acquire_exclusive(
            &self.store.profile_lock_path(&initial_profile.id),
            PROFILE_BACKUP_LOCK_TIMEOUT,
        )?;

        let profile = self.store.get_profile(&initial_profile.id)?;
        let dir = self.store.profile_dir_for(&profile)?;
        let rt = self.coordinator.inspect_profile_record(&profile, &dir)?;
        let is_active_or_unclean =
            rt.running || rt.lifecycle_state() != ProfileLifecycleState::Stopped;
        self.backups.backup_profile_with_options(
            &profile,
            &dir,
            None,
            is_active_or_unclean,
            allow_external_db_omit,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Barrier;

    use dim_core::LaunchTarget;

    fn fixture_script_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .expect("resolve workspace root from src-tauri CARGO_MANIFEST_DIR")
            .join("fixtures")
            .join("cross-platform")
            .join("xf-fixture.mjs")
    }

    fn temp_sandbox_dir(prefix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "dim-tauri-state-{prefix}-{}-{}",
            std::process::id(),
            dim_core::generate_profile_id()
        ));
        fs::create_dir_all(&dir).expect("create temp sandbox");
        dir
    }

    #[test]
    fn async_same_profile_concurrent_launches_coalesce_to_single_pid() {
        let script = fixture_script_path();
        assert!(
            script.exists(),
            "missing xf-fixture.mjs at {}",
            script.display()
        );

        let data_dir = temp_sandbox_dir("same-prof");
        let state = AppState::from_data_dir(Some(&data_dir)).expect("init AppState");
        let prof = state
            .store
            .create_profile("Tauri Async Same", "")
            .expect("create profile");
        let pdir = state.store.profile_dir_for(&prof).expect("profile dir");

        tauri::async_runtime::block_on(async {
            let barrier = Arc::new(Barrier::new(4));
            let mut tasks = Vec::with_capacity(4);
            for _ in 0..4 {
                let st = state.clone();
                let b = Arc::clone(&barrier);
                let pid_str = prof.id.clone();
                let sp = script.clone();
                tasks.push(tauri::async_runtime::spawn_blocking(move || {
                    let target = LaunchTarget::NodeFixture {
                        node_path: None,
                        script_path: sp,
                        extra_args: vec![
                            "--user-id=911001".to_owned(),
                            "--slow-startup-pre-lock-ms=200".to_owned(),
                        ],
                    };
                    b.wait();
                    st.coordinator.launch_profile(&pid_str, &target)
                }));
            }

            let mut pids = HashSet::new();
            for task in tasks {
                let rt = task
                    .await
                    .expect("join blocking task")
                    .expect("launch_profile should succeed");
                assert!(rt.running);
                pids.insert(rt.main_pid.expect("main_pid must be populated"));
            }
            assert_eq!(
                pids.len(),
                1,
                "4 concurrent Tauri launches on same profile must coalesce into 1 PID, got {pids:?}"
            );

            let zdata = pdir.join("AppData").join("Roaming").join("ZaloData");
            if let Ok(log) = fs::read_to_string(zdata.join("startup.log")) {
                let collisions = log
                    .lines()
                    .filter(|l| l.contains("SECOND_INSTANCE_REJECTED"))
                    .count();
                assert_eq!(collisions, 0, "expected 0 second-instance collisions");
            }

            let stopped = state
                .coordinator
                .stop_profile(&prof.id)
                .expect("stop_profile");
            assert!(stopped >= 1);
        });

        let _ = fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn async_distinct_profiles_launch_in_parallel_without_global_lock() {
        let script = fixture_script_path();
        assert!(
            script.exists(),
            "missing xf-fixture.mjs at {}",
            script.display()
        );

        let data_dir = temp_sandbox_dir("distinct-prof");
        let state = AppState::from_data_dir(Some(&data_dir)).expect("init AppState");
        let p_a = state
            .store
            .create_profile("Tauri Parallel A", "")
            .expect("create A");
        let p_b = state
            .store
            .create_profile("Tauri Parallel B", "")
            .expect("create B");

        tauri::async_runtime::block_on(async {
            let barrier = Arc::new(Barrier::new(2));
            let st_a = state.clone();
            let b_a = Arc::clone(&barrier);
            let id_a = p_a.id.clone();
            let sp_a = script.clone();

            let st_b = state.clone();
            let b_b = Arc::clone(&barrier);
            let id_b = p_b.id.clone();
            let sp_b = script.clone();

            let t_a = tauri::async_runtime::spawn_blocking(move || {
                let target = LaunchTarget::NodeFixture {
                    node_path: None,
                    script_path: sp_a,
                    extra_args: vec![
                        "--user-id=912001".to_owned(),
                        "--slow-startup-pre-lock-ms=250".to_owned(),
                    ],
                };
                b_a.wait();
                st_a.coordinator.launch_profile(&id_a, &target)
            });

            let t_b = tauri::async_runtime::spawn_blocking(move || {
                let target = LaunchTarget::NodeFixture {
                    node_path: None,
                    script_path: sp_b,
                    extra_args: vec![
                        "--user-id=912002".to_owned(),
                        "--slow-startup-pre-lock-ms=250".to_owned(),
                    ],
                };
                b_b.wait();
                st_b.coordinator.launch_profile(&id_b, &target)
            });

            let rt_a = t_a.await.expect("join A").expect("launch A");
            let rt_b = t_b.await.expect("join B").expect("launch B");
            assert!(rt_a.running && rt_b.running);
            assert_ne!(rt_a.main_pid, rt_b.main_pid);

            let _ = state.coordinator.stop_profile(&p_a.id);
            let _ = state.coordinator.stop_profile(&p_b.id);
        });

        let _ = fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn web_session_store_crud_reset_and_pending_deletes_persist() {
        let data_dir = temp_sandbox_dir("web-store");
        let store = WebSessionStore::initialize(&data_dir);
        assert!(store.list().is_empty());

        let created = store
            .create("  Zalo Web Work  ", Some("ocean"))
            .expect("create web session");
        assert_eq!(created.display_name, "Zalo Web Work");
        assert_eq!(created.color_token.as_deref(), Some("ocean"));
        assert_eq!(created.profile_generation, 1);
        assert!(created
            .data_store_uuid
            .as_ref()
            .is_some_and(|u| u.len() == 32));

        let renamed = store
            .rename(&created.id, "Zalo Web Sales")
            .expect("rename web session");
        assert_eq!(renamed.display_name, "Zalo Web Sales");

        let reset = store
            .bump_generation_for_reset(&created.id, Some("nexadock_web_old_g1".to_owned()))
            .expect("reset web session");
        assert_eq!(reset.profile_generation, 2);
        assert_ne!(reset.data_store_uuid, created.data_store_uuid);
        assert_eq!(
            store.pending_profile_deletes(),
            vec!["nexadock_web_old_g1".to_owned()]
        );

        let reloaded = WebSessionStore::initialize(&data_dir);
        assert_eq!(reloaded.list().len(), 1);
        assert_eq!(reloaded.list()[0].profile_generation, 2);
        assert_eq!(
            reloaded.pending_profile_deletes(),
            vec!["nexadock_web_old_g1".to_owned()]
        );

        reloaded
            .remove_pending_profile_deletes(&["nexadock_web_old_g1".to_owned()])
            .expect("clear pending");
        assert!(reloaded.pending_profile_deletes().is_empty());

        let deleted = reloaded
            .delete(&created.id, Some("nexadock_web_old_g2".to_owned()))
            .expect("delete web session");
        assert_eq!(deleted.id, created.id);
        assert!(reloaded.list().is_empty());
        assert_eq!(
            reloaded.pending_profile_deletes(),
            vec!["nexadock_web_old_g2".to_owned()]
        );

        let _ = fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn web_session_store_rejects_duplicate_names_and_invalid_ids() {
        let data_dir = temp_sandbox_dir("web-store-valid");
        let store = WebSessionStore::initialize(&data_dir);

        let first = store
            .create("Zalo Web Alpha", Some("ocean"))
            .expect("create first web session");
        assert!(is_valid_web_session_id(&first.id));
        assert!(!is_valid_web_session_id("web-../../etc"));
        assert!(!is_valid_web_session_id("web-UPPER123"));
        assert!(!is_valid_web_session_id("web-1234567890"));

        let dup = store
            .create("  zalo web alpha  ", None)
            .expect_err("case-insensitive duplicate display_name must fail");
        assert_eq!(dup.code(), "DUPLICATE_NAME");

        let second = store
            .create("Zalo Web Beta", None)
            .expect("create second web session");
        let rename_dup = store
            .rename(&second.id, "ZALO WEB ALPHA")
            .expect_err("rename to existing display_name must fail");
        assert_eq!(rename_dup.code(), "DUPLICATE_NAME");

        // Self-rename with casing change is allowed.
        let self_renamed = store
            .rename(&first.id, "Zalo Web ALPHA")
            .expect("self rename allowed");
        assert_eq!(self_renamed.display_name, "Zalo Web ALPHA");

        assert!(store.create("Bad/Name", None).is_err());
        assert!(store.create("Bad..Name", None).is_err());
        assert!(store.create("Bad\x07Name", None).is_err());

        assert_eq!(
            store.get("web-notvalid!").unwrap_err().code(),
            "INVALID_NAME"
        );
        assert_eq!(
            store.rename("../escape", "Valid").unwrap_err().code(),
            "INVALID_NAME"
        );
        assert_eq!(
            store
                .bump_generation_for_reset("web-123", None)
                .unwrap_err()
                .code(),
            "INVALID_NAME"
        );
        assert_eq!(
            store.delete("web-123", None).unwrap_err().code(),
            "INVALID_NAME"
        );

        let _ = fs::remove_dir_all(&data_dir);
    }
}

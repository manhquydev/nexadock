//! Shared Tauri application state with per-profile synchronization.
//!
//! `AppState` intentionally avoids any coarse global operation `Mutex`.
//! - Profile metadata mutations synchronize via `<data_dir>/profiles.lock` inside `JsonProfileStore`.
//! - Profile lifecycle operations (`launch`, `stop`, `focus`, `delete`) synchronize per-profile
//!   inside `Arc<InstanceCoordinator>` (in-process keyed table + cross-process `.dim-profile.lock`),
//!   allowing distinct profiles to launch, stop, and back up concurrently without blocking one another.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use dim_core::{
    AdvisoryFileLock, BackupResult, DimError, InstanceMonitor, PlatformCapabilities,
    ProfileLifecycleState, ProfileStore, PROFILE_OP_LOCK_FILE,
};
use dim_process_manager::InstanceCoordinator;
use dim_profile_store::{FileBackupManager, JsonProfileStore};

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
pub use dim_platform_linux::LinuxPlatformAdapter as ActiveAdapter;
#[cfg(target_os = "macos")]
pub use dim_platform_macos::MacOsPlatformAdapter as ActiveAdapter;
#[cfg(target_os = "windows")]
pub use dim_platform_windows::WindowsPlatformAdapter as ActiveAdapter;

const PROFILE_BACKUP_LOCK_TIMEOUT: Duration = Duration::from_secs(15);

/// Cloneable shared application state managed by Tauri.
#[derive(Clone)]
pub struct AppState {
    pub store: Arc<JsonProfileStore>,
    pub adapter: Arc<ActiveAdapter>,
    pub coordinator: Arc<InstanceCoordinator<ActiveAdapter>>,
    pub backups: Arc<FileBackupManager>,
}

impl AppState {
    /// Initializes store, platform adapter, coordinator, and backup manager from `DIM_DATA_DIR` or OS defaults.
    pub fn initialize() -> Result<Self, DimError> {
        Self::from_data_dir(None)
    }

    /// Initializes state rooted at an explicit `data_dir` override (or `DIM_DATA_DIR` / OS default when `None`).
    pub fn from_data_dir(data_dir: Option<&Path>) -> Result<Self, DimError> {
        let store = Arc::new(JsonProfileStore::from_default_or_env(data_dir)?);
        store.ensure_initialized()?;

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

        Ok(Self {
            store,
            adapter,
            coordinator,
            backups,
        })
    }

    /// Creates an offline atomic backup synchronized on the target profile's `.dim-profile.lock`
    /// so concurrent launches/deletes on the same profile serialize cleanly without blocking other profiles.
    pub fn backup_profile_synchronized(
        &self,
        id: &str,
        allow_external_db_omit: bool,
    ) -> Result<BackupResult, DimError> {
        let initial_profile = self.store.get_profile(id)?;
        let dir = self.store.profile_dir_for(&initial_profile)?;

        let _profile_op_guard = if dir.exists() {
            Some(AdvisoryFileLock::acquire_exclusive(
                &dir.join(PROFILE_OP_LOCK_FILE),
                PROFILE_BACKUP_LOCK_TIMEOUT,
            )?)
        } else {
            None
        };

        let profile = self.store.get_profile(id)?;
        let rt = self.adapter.inspect_profile(&profile, &dir)?;
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
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("..")
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
}

//! Controlled synthetic integration test suite for `TC-H7` (`IR-WIN-04` / `F-CORE-01` & `IR-WIN-03` / `F-WIN-04`).
//! Uses 100% isolated `tempfile` directories — ZERO real Zalo paths or user folders are touched.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::process::Command;
use std::sync::{Arc, Mutex};

use dim_core::{
    external_paths_outside_profile, is_path_truly_contained_in_profile, BackupManager, DimError,
    InstanceLauncher, InstanceMonitor, LaunchTarget, PlatformCapabilities,
    PlatformCapabilityReport, PlatformKind, PrimaryInstanceStatus, ProfileRecord,
    ProfileRuntimeState, ProfileStore, SupportStatus, MIN_FREE_DISK_BYTES,
};
use dim_process_manager::InstanceCoordinator;
use dim_profile_store::{
    unlink_reparse_points_safe, write_profile_id_marker, FileBackupManager, JsonProfileStore,
};

#[derive(Default)]
struct SyntheticRuntime {
    running_pids: Mutex<HashMap<String, u32>>,
    next_pid: Mutex<u32>,
    primary_dir: PathBuf,
}

impl SyntheticRuntime {
    fn new(primary_dir: PathBuf) -> Self {
        Self {
            running_pids: Mutex::new(HashMap::new()),
            next_pid: Mutex::new(70_000),
            primary_dir,
        }
    }
}

impl PlatformCapabilities for SyntheticRuntime {
    fn platform(&self) -> PlatformKind {
        PlatformKind::Windows
    }

    fn capabilities(&self) -> PlatformCapabilityReport {
        PlatformCapabilityReport {
            platform: PlatformKind::Windows,
            overall_status: SupportStatus::SupportedAndTested,
            official_zalo_status: SupportStatus::SupportedAndTested,
            fixture_status: SupportStatus::SupportedAndTested,
            appimage_status: None,
            concurrent_calls_status: SupportStatus::Blocked,
            folder_redirection_detected: false,
            folder_redirection_path: None,
            limitations: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    fn free_disk_bytes(&self, _path: &Path) -> Result<u64, DimError> {
        Ok(MIN_FREE_DISK_BYTES * 4)
    }

    fn unlink_profile_junctions(&self, profile_dir: &Path) -> Result<usize, DimError> {
        unlink_reparse_points_safe(profile_dir)
    }
}

impl InstanceMonitor for SyntheticRuntime {
    fn inspect_profile(
        &self,
        profile: &ProfileRecord,
        profile_dir: &Path,
    ) -> Result<ProfileRuntimeState, DimError> {
        let zalo_data_dir = profile_dir.join("AppData").join("Roaming").join("ZaloData");
        let pid = self.running_pids.lock().unwrap().get(&profile.id).copied();
        Ok(ProfileRuntimeState {
            profile_id: profile.id.clone(),
            profile_name: profile.name.clone(),
            profile_dir: profile_dir.to_path_buf(),
            lockfile_path: zalo_data_dir.join("lockfile"),
            zalo_data_dir,
            running: pid.is_some(),
            stale_lock_detected: false,
            main_pid: pid,
            process_ids: pid.into_iter().collect(),
            process_count: usize::from(pid.is_some()),
            working_set_bytes: 0,
            window_handle: None,
            window_title: None,
            detected_zalo_user_id: profile.zalo_user_id.clone(),
            disk_usage_bytes: 0,
            status_label: if pid.is_some() {
                "Running".to_owned()
            } else {
                "Stopped".to_owned()
            },
        })
    }

    fn inspect_primary(&self) -> Result<PrimaryInstanceStatus, DimError> {
        Ok(PrimaryInstanceStatus {
            zalo_data_dir: self.primary_dir.clone(),
            lockfile_path: self.primary_dir.join("lockfile"),
            running: false,
            protected_pids: Vec::new(),
            process_count: 0,
            working_set_bytes: 0,
            detected_zalo_user_id: None,
            notes: Vec::new(),
        })
    }
}

impl InstanceLauncher for SyntheticRuntime {
    fn ensure_profile_layout(
        &self,
        profile: &ProfileRecord,
        profile_dir: &Path,
    ) -> Result<(), DimError> {
        let zdata = profile_dir.join("AppData").join("Roaming").join("ZaloData");
        fs::create_dir_all(zdata.join("Database").join("_production"))?;
        write_profile_id_marker(&zdata, &profile.id)?;
        Ok(())
    }

    fn launch(
        &self,
        _target: &LaunchTarget,
        profile: &ProfileRecord,
        profile_dir: &Path,
    ) -> Result<ProfileRuntimeState, DimError> {
        let mut next = self.next_pid.lock().unwrap();
        *next += 1;
        let pid = *next;
        drop(next);
        self.running_pids
            .lock()
            .unwrap()
            .insert(profile.id.clone(), pid);
        self.inspect_profile(profile, profile_dir)
    }

    fn focus_or_restore(
        &self,
        _target: &LaunchTarget,
        _profile: &ProfileRecord,
        _profile_dir: &Path,
        _state: &ProfileRuntimeState,
    ) -> Result<(), DimError> {
        Ok(())
    }

    fn stop(
        &self,
        state: &ProfileRuntimeState,
        _protected_primary_pids: &[u32],
        _timeout_ms: u64,
    ) -> Result<u32, DimError> {
        let existed = self
            .running_pids
            .lock()
            .unwrap()
            .remove(&state.profile_id)
            .is_some();
        Ok(u32::from(existed))
    }
}

fn create_dir_link_synthetic(link: &Path, target: &Path) {
    #[cfg(windows)]
    {
        let status = Command::new("cmd.exe")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .expect("invoke mklink /J");
        assert!(
            status.status.success(),
            "mklink /J failed: {}",
            String::from_utf8_lossy(&status.stderr)
        );
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).expect("create unix symlink");
    }
}

fn escape_json_path(p: &Path) -> String {
    p.display().to_string().replace('\\', "\\\\")
}

fn write_zalo_config(zalo_data_dir: &Path, db_loc: Option<&str>, res_loc: Option<&str>) {
    fs::create_dir_all(zalo_data_dir).unwrap();
    let db_part = db_loc
        .map(|s| format!(r#""db_path": {{ "location": "{s}" }}"#))
        .unwrap_or_default();
    let res_part = res_loc
        .map(|s| format!(r#""resource": {{ "location": {{ "path": "{s}" }} }}"#))
        .unwrap_or_default();
    let body = match (!db_part.is_empty(), !res_part.is_empty()) {
        (true, true) => format!("{{ {db_part}, {res_part} }}\n"),
        (true, false) => format!("{{ {db_part} }}\n"),
        (false, true) => format!("{{ {res_part} }}\n"),
        (false, false) => "{}\n".to_owned(),
    };
    fs::write(zalo_data_dir.join("config.json"), body.as_bytes()).unwrap();
}

#[test]
fn tc_h7_abc_path_containment_catches_absolute_traversal_and_junction_escape() {
    let tmp = tempfile::tempdir().unwrap();
    let profile_dir = tmp.path().join("Profiles").join("alpha_11223344");
    let zdata = profile_dir.join("AppData").join("Roaming").join("ZaloData");
    let external_abs = tmp.path().join("ext-storage-outside");
    fs::create_dir_all(&zdata).unwrap();
    fs::create_dir_all(&external_abs).unwrap();

    // Internal path stays contained:
    let internal_db = zdata.join("Database");
    fs::create_dir_all(&internal_db).unwrap();
    assert!(is_path_truly_contained_in_profile(
        &profile_dir,
        &internal_db
    ));

    // (a) External absolute `db_path.location`:
    write_zalo_config(&zdata, Some(&escape_json_path(&external_abs)), None);
    let ext_a = external_paths_outside_profile(&profile_dir, &zdata);
    assert_eq!(ext_a.len(), 1, "absolute external DB must be flagged");

    // (b) Relative `..` path traversal escaping `<profile_dir>`:
    write_zalo_config(&zdata, Some("../../../../../ext-storage-outside"), None);
    let ext_b = external_paths_outside_profile(&profile_dir, &zdata);
    assert_eq!(ext_b.len(), 1, "relative .. escape must be flagged");

    // (c) NTFS junction / symlink inside `<profile_dir>` pointing to `external_abs`:
    // Naive `Path::starts_with(&profile_dir)` returns `true`, whereas real containment resolves the junction!
    let junction_inside_profile = zdata.join("SneakyJunctionDb");
    create_dir_link_synthetic(&junction_inside_profile, &external_abs);
    assert!(
        junction_inside_profile.starts_with(&profile_dir),
        "precondition: lexical starts_with would be fooled by in-profile junction"
    );
    assert!(
        !is_path_truly_contained_in_profile(&profile_dir, &junction_inside_profile),
        "true containment must detect junction escaping outside profile_dir"
    );
    let child_through_junction = junction_inside_profile.join("NestedFutureDir");
    assert!(
        !is_path_truly_contained_in_profile(&profile_dir, &child_through_junction),
        "true containment must also detect non-existent leaf beneath escaping ancestor junction"
    );

    write_zalo_config(
        &zdata,
        Some(&escape_json_path(&junction_inside_profile)),
        None,
    );
    let ext_c = external_paths_outside_profile(&profile_dir, &zdata);
    assert_eq!(
        ext_c.len(),
        1,
        "config.json pointing through in-profile junction to external dir must be flagged"
    );

    // Unlink junction safely before TempDir drop:
    let unlinked = unlink_reparse_points_safe(&profile_dir).unwrap();
    assert_eq!(unlinked, 1);
}

#[test]
fn tc_h7_de_shared_external_db_launch_guard_and_backup_fail_closed() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("dim-data");
    let synth_primary = tmp.path().join("synth-primary-zalodata");
    let shared_ext_db = tmp.path().join("ext-db-shared");
    let shared_ext_media = tmp.path().join("ext-media-shared");
    let primary_ext_db = tmp.path().join("ext-db-primary-owned");
    fs::create_dir_all(
        shared_ext_db
            .join("Database")
            .join("_production")
            .join("8888888"),
    )
    .unwrap();
    fs::create_dir_all(&shared_ext_media).unwrap();
    fs::create_dir_all(&primary_ext_db).unwrap();

    let ext_sentinel = shared_ext_db.join("sqlite-sentinel.db");
    let sentinel_bytes = b"EXTERNAL_SQLITE_PAYLOAD_DO_NOT_TOUCH_V1";
    fs::write(&ext_sentinel, sentinel_bytes).unwrap();

    // Seed synthetic primary ZaloData (stopped!) owning `primary_ext_db`:
    write_zalo_config(
        &synth_primary,
        Some(&escape_json_path(&primary_ext_db)),
        None,
    );

    let store = Arc::new(JsonProfileStore::new(data_dir).unwrap());
    store.ensure_initialized().unwrap();
    let adapter = Arc::new(SyntheticRuntime::new(synth_primary.clone()));
    let coordinator = InstanceCoordinator::new(Arc::clone(&store), Arc::clone(&adapter))
        .with_protected_zalo_data_dir(synth_primary);
    let backups = FileBackupManager::new(Arc::clone(&store))
        .with_disk_space_probe(Arc::new(|_| Ok(MIN_FREE_DISK_BYTES * 4)));

    let prof_c = store.create_profile("Profile C Ext", "").unwrap();
    let prof_d = store.create_profile("Profile D Ext", "").unwrap();
    let prof_p = store.create_profile("Profile Primary Collide", "").unwrap();

    let dir_c = store.profile_dir_for(&prof_c).unwrap();
    let dir_d = store.profile_dir_for(&prof_d).unwrap();
    let dir_p = store.profile_dir_for(&prof_p).unwrap();

    adapter.ensure_profile_layout(&prof_c, &dir_c).unwrap();
    adapter.ensure_profile_layout(&prof_d, &dir_d).unwrap();
    adapter.ensure_profile_layout(&prof_p, &dir_p).unwrap();

    // Configure C & D with distinct UIDs so ONLY ExternalStorageGuard blocks D!
    store.set_zalo_user_id(&prof_c.id, Some("8880001")).unwrap();
    store.set_zalo_user_id(&prof_d.id, Some("8880002")).unwrap();
    write_zalo_config(
        &dir_c.join("AppData").join("Roaming").join("ZaloData"),
        Some(&escape_json_path(&shared_ext_db)),
        Some(&escape_json_path(&shared_ext_media)),
    );
    // D uses an in-profile NTFS junction pointing to the same `shared_ext_db`!
    let d_zdata = dir_d.join("AppData").join("Roaming").join("ZaloData");
    let d_junction = d_zdata.join("LinkedSharedDb");
    create_dir_link_synthetic(&d_junction, &shared_ext_db);
    write_zalo_config(&d_zdata, Some(&escape_json_path(&d_junction)), None);

    // P points to stopped primary's `primary_ext_db`:
    write_zalo_config(
        &dir_p.join("AppData").join("Roaming").join("ZaloData"),
        Some(&escape_json_path(&primary_ext_db)),
        None,
    );

    let target = LaunchTarget::CustomExecutable {
        path: PathBuf::from("synthetic-fixture"),
        args: Vec::new(),
    };

    // (d1) Profile C launches cleanly:
    let rt_c = coordinator.launch_profile(&prof_c.id, &target).unwrap();
    assert!(rt_c.running);

    // (d2) Profile D sharing external DB (even via junction!) MUST fail with `SHARED_EXTERNAL_DB_PATH`:
    let err_d = coordinator
        .launch_profile(&prof_d.id, &target)
        .expect_err("concurrent shared external DB launch must be rejected");
    assert_eq!(err_d.code(), "SHARED_EXTERNAL_DB_PATH");

    // (d3) Profile P colliding with stopped synthetic primary's external DB MUST ALSO fail with `SHARED_EXTERNAL_DB_PATH`:
    let err_p = coordinator
        .launch_profile(&prof_p.id, &target)
        .expect_err("collision with primary external DB must be rejected");
    assert_eq!(err_p.code(), "SHARED_EXTERNAL_DB_PATH");

    // `stop_all_secondary_profiles` stops Profile C cleanly:
    let stopped_all = coordinator.stop_all_secondary_profiles().unwrap();
    assert_eq!(stopped_all.len(), 1);
    assert_eq!(stopped_all[0].0, prof_c.id);

    // (e1) Default `backup_profile` on Profile C & Profile D (junction escape) MUST fail closed with `EXTERNAL_DB_OUTSIDE_PROFILE`:
    let bk_err_c = backups
        .backup_profile(&prof_c, &dir_c, None, false)
        .expect_err("unacknowledged external DB backup must fail closed");
    assert_eq!(bk_err_c.code(), "EXTERNAL_DB_OUTSIDE_PROFILE");

    let bk_err_d = backups
        .backup_profile(&prof_d, &dir_d, None, false)
        .expect_err("junction-escaped external DB backup must fail closed");
    assert_eq!(bk_err_d.code(), "EXTERNAL_DB_OUTSIDE_PROFILE");

    // Zero false backup directories created on failure:
    assert_eq!(fs::read_dir(store.backups_dir()).unwrap().count(), 0);

    // (e2) Explicit acknowledgement (`allow_external_db_omit = true`) backs up profile-local files only
    // and marks `"external_db_omitted": true` in `backup.json`, leaving external DB 100% untouched!
    let bk_ok = backups
        .backup_profile_with_options(&prof_c, &dir_c, None, false, true)
        .unwrap();
    let manifest_raw = fs::read_to_string(&bk_ok.manifest_path).unwrap();
    assert!(manifest_raw.contains(r#""external_db_omitted": true"#));
    assert_eq!(fs::read(&ext_sentinel).unwrap(), sentinel_bytes);

    let _ = unlink_reparse_points_safe(&dir_d);
}

#[test]
fn tc_h7_f_mock_host_canary_files_survive_junction_backup_and_delete_intact() {
    let tmp = tempfile::tempdir().unwrap();
    let mock_host = tmp.path().join("mock-host");
    let host_downloads = mock_host.join("Downloads");
    let host_desktop = mock_host.join("Desktop");
    fs::create_dir_all(host_downloads.join("sub")).unwrap();
    fs::create_dir_all(&host_desktop).unwrap();

    let canary_1 = host_downloads.join("canary-1.pdf");
    let canary_2 = host_desktop.join("canary-2.png");
    let canary_3 = host_downloads.join("sub").join("canary-3.txt");
    let bytes_1 = b"CANARY_PDF_HOST_DOWNLOADS_DO_NOT_DELETE_1";
    let bytes_2 = b"CANARY_PNG_HOST_DESKTOP_DO_NOT_DELETE_2";
    let bytes_3 = b"CANARY_TXT_HOST_NESTED_SUB_DO_NOT_DELETE_3";
    fs::write(&canary_1, bytes_1).unwrap();
    fs::write(&canary_2, bytes_2).unwrap();
    fs::write(&canary_3, bytes_3).unwrap();

    let store = Arc::new(JsonProfileStore::new(tmp.path().join("dim-data")).unwrap());
    store.ensure_initialized().unwrap();
    let adapter = Arc::new(SyntheticRuntime::new(tmp.path().join("synth-primary")));
    let coordinator = InstanceCoordinator::new(Arc::clone(&store), Arc::clone(&adapter));
    let backups = FileBackupManager::new(Arc::clone(&store))
        .with_disk_space_probe(Arc::new(|_| Ok(MIN_FREE_DISK_BYTES * 4)));

    let prof_a = store.create_profile("Canary Junction Profile", "").unwrap();
    let dir_a = store.profile_dir_for(&prof_a).unwrap();
    adapter.ensure_profile_layout(&prof_a, &dir_a).unwrap();

    // Link `<dir_a>/Downloads` and `<dir_a>/Desktop` to `mock-host`, keeping `Documents/Zalo Received Files` isolated!
    create_dir_link_synthetic(&dir_a.join("Downloads"), &host_downloads);
    create_dir_link_synthetic(&dir_a.join("Desktop"), &host_desktop);
    let recv_dir = dir_a.join("Documents").join("Zalo Received Files");
    fs::create_dir_all(&recv_dir).unwrap();
    fs::write(recv_dir.join("auto-recv-a.pdf"), b"ISOLATED_RECEIVED_FILE").unwrap();

    // Backup skips junctions without copying host canary files:
    let bk = backups
        .backup_profile(&prof_a, &dir_a, None, false)
        .unwrap();
    assert!(bk
        .manifest
        .files
        .iter()
        .any(|f| f.relative_path.ends_with("auto-recv-a.pdf")));
    assert!(!bk
        .manifest
        .files
        .iter()
        .any(|f| f.relative_path.contains("canary-")));

    // Confirmed delete unlinks junctions safely (verified two-phase zero-reparse-points check) and removes `dir_a`:
    coordinator.delete_profile(&prof_a.id, true).unwrap();
    assert!(!dir_a.exists(), "profile directory must be deleted");

    // 100% of mock-host canary files survive byte-for-byte intact!
    assert_eq!(fs::read(&canary_1).unwrap(), bytes_1);
    assert_eq!(fs::read(&canary_2).unwrap(), bytes_2);
    assert_eq!(fs::read(&canary_3).unwrap(), bytes_3);
}

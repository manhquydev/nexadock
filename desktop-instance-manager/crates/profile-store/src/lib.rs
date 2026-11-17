//! Profile storage, atomic `profiles.json`, identity markers, and offline verified backup/restore.

pub mod backup;
pub mod store;

pub use backup::{
    sweep_abandoned_staging_dirs, DiskSpaceProbe, FileBackupManager, STAGING_MAX_AGE,
};
pub use store::{
    is_symlink_or_reparse, unlink_reparse_points_safe, verify_profile_id_marker,
    verify_zero_reparse_points_remaining, write_profile_id_marker, JsonProfileStore,
};

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::fs;
    use std::sync::Arc;

    use dim_core::{BackupManager, ProfileStore, MIN_FREE_DISK_BYTES, ZALO_PROFILE_ID_FILE};

    use super::*;

    #[test]
    fn atomic_profile_crud_and_identity_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let store = JsonProfileStore::new(tmp.path().to_path_buf()).unwrap();

        let created = store
            .create_profile("Đức - Công việc", "Work account")
            .unwrap();
        assert!(created.directory_name.starts_with("duc-cong-viec_"));
        assert!(tmp.path().join("profiles.json").exists());
        assert!(!tmp.path().join("profiles.json.tmp").exists());

        let loaded = store.get_profile(&created.id).unwrap();
        assert_eq!(loaded.name, "Đức - Công việc");

        let renamed = store.rename_profile(&created.id, "Công việc 2").unwrap();
        assert_eq!(renamed.name, "Công việc 2");
        assert_eq!(renamed.directory_name, created.directory_name);

        let zalo_data = store
            .profile_dir_for(&renamed)
            .unwrap()
            .join("AppData")
            .join("Roaming")
            .join("ZaloData");
        write_profile_id_marker(&zalo_data, &renamed.id).unwrap();
        assert!(verify_profile_id_marker(&zalo_data, &renamed.id, true).is_ok());
        assert_eq!(
            verify_profile_id_marker(&zalo_data, "deadbeef", true)
                .unwrap_err()
                .code(),
            "PROFILE_MARKER_MISMATCH"
        );

        assert_eq!(
            store.delete_profile(&renamed.id, false).unwrap_err().code(),
            "CONFIRM_REQUIRED"
        );
        store.delete_profile(&renamed.id, true).unwrap();
        assert!(store.list_profiles().unwrap().is_empty());
    }

    #[test]
    fn offline_backup_and_restore_creates_new_profile_with_sha256_verification() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Arc::new(JsonProfileStore::new(tmp.path().to_path_buf()).unwrap());
        let profile = store.create_profile("Main Work", "").unwrap();
        let profile_dir = store.profile_dir_for(&profile).unwrap();
        let zalo_data = profile_dir.join("AppData").join("Roaming").join("ZaloData");
        fs::create_dir_all(&zalo_data).unwrap();
        write_profile_id_marker(&zalo_data, &profile.id).unwrap();
        fs::write(zalo_data.join("config.json"), b"{\"theme\":\"dark\"}").unwrap();
        fs::write(zalo_data.join("lockfile"), b"stale").unwrap();

        let backup_mgr = FileBackupManager::new(Arc::clone(&store))
            .with_disk_space_probe(Arc::new(|_| Ok(MIN_FREE_DISK_BYTES + 10_000_000)));

        // Running profile must be rejected
        let running_err = backup_mgr
            .backup_profile(&profile, &profile_dir, None, true)
            .unwrap_err();
        assert_eq!(running_err.code(), "PROFILE_RUNNING");

        let backup = backup_mgr
            .backup_profile(&profile, &profile_dir, None, false)
            .unwrap();
        assert!(backup.manifest_path.exists());
        assert!(!backup.backup_dir.to_string_lossy().ends_with(".partial"));
        // lockfile is excluded from backup payload
        assert!(backup
            .manifest
            .files
            .iter()
            .all(|f| !f.relative_path.ends_with("lockfile")));

        // Verify succeeds
        let verified = backup_mgr.verify_backup(&backup.backup_dir).unwrap();
        assert_eq!(verified.source_profile_id, profile.id);

        // Restore always creates a NEW profile with a fresh ID and updated .zalo-profile-id
        let restored = backup_mgr
            .restore_backup_as_new_profile(&backup.backup_dir, None)
            .unwrap();
        assert_ne!(restored.id, profile.id);
        assert_ne!(restored.directory_name, profile.directory_name);
        assert_eq!(restored.name, "Main Work (Restored)");

        // No .restore-* staging directory remains after promotion
        assert!(fs::read_dir(store.profiles_dir())
            .unwrap()
            .filter_map(Result::ok)
            .all(|e| !e.file_name().to_string_lossy().starts_with(".restore-")));

        // Abandoned .partial / .restore-* sweep removes stale staging directories
        let stale_partial = store.backups_dir().join("abandoned_1234.partial");
        let stale_restore = store.profiles_dir().join(".restore-old-deadbeef");
        fs::create_dir_all(&stale_partial).unwrap();
        fs::create_dir_all(&stale_restore).unwrap();
        assert_eq!(
            sweep_abandoned_staging_dirs(store.backups_dir(), std::time::Duration::ZERO).unwrap(),
            1
        );
        assert_eq!(
            sweep_abandoned_staging_dirs(store.profiles_dir(), std::time::Duration::ZERO).unwrap(),
            1
        );
        assert!(!stale_partial.exists());
        assert!(!stale_restore.exists());

        let restored_zalo = store
            .profile_dir_for(&restored)
            .unwrap()
            .join("AppData")
            .join("Roaming")
            .join("ZaloData");
        assert_eq!(
            fs::read_to_string(restored_zalo.join(ZALO_PROFILE_ID_FILE)).unwrap(),
            restored.id
        );
        assert_eq!(
            fs::read_to_string(restored_zalo.join("config.json")).unwrap(),
            "{\"theme\":\"dark\"}"
        );

        // Tamper detection
        fs::write(
            backup
                .backup_dir
                .join("AppData")
                .join("Roaming")
                .join("ZaloData")
                .join("config.json"),
            b"{\"theme\":\"tampered\"}",
        )
        .unwrap();
        assert_eq!(
            backup_mgr
                .verify_backup(&backup.backup_dir)
                .unwrap_err()
                .code(),
            "BACKUP_ERROR"
        );
    }

    #[test]
    fn backup_enforces_five_gib_free_space_threshold() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Arc::new(JsonProfileStore::new(tmp.path().to_path_buf()).unwrap());
        let profile = store.create_profile("LowDisk", "").unwrap();
        let profile_dir = store.profile_dir_for(&profile).unwrap();

        let backup_mgr = FileBackupManager::new(Arc::clone(&store))
            .with_disk_space_probe(Arc::new(|_| Ok(MIN_FREE_DISK_BYTES - 1)));

        let err = backup_mgr
            .backup_profile(&profile, &profile_dir, None, false)
            .unwrap_err();
        assert_eq!(err.code(), "INSUFFICIENT_DISK_SPACE");
    }

    #[test]
    fn delete_profile_unlinks_junctions_or_symlinks_without_touching_external_target() {
        let tmp = tempfile::tempdir().unwrap();
        let store = JsonProfileStore::new(tmp.path().join("dim-data")).unwrap();
        let profile = store.create_profile("LinkTest", "").unwrap();
        let profile_dir = store.profile_dir_for(&profile).unwrap();

        let outside_dir = tmp.path().join("outside-target");
        fs::create_dir_all(&outside_dir).unwrap();
        let sentinel = outside_dir.join("preserve.txt");
        fs::write(&sentinel, "DO_NOT_DELETE").unwrap();

        let link_parent = profile_dir.join("Desktop");
        fs::create_dir_all(&link_parent).unwrap();
        let link_path = link_parent.join("ExtLink");

        #[cfg(windows)]
        {
            let status = std::process::Command::new("cmd.exe")
                .args([
                    "/c",
                    "mklink",
                    "/J",
                    &link_path.to_string_lossy(),
                    &outside_dir.to_string_lossy(),
                ])
                .status()
                .unwrap();
            assert!(status.success());
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside_dir, &link_path).unwrap();
        }

        store.delete_profile(&profile.id, true).unwrap();
        assert!(!profile_dir.exists());
        assert_eq!(fs::read_to_string(&sentinel).unwrap(), "DO_NOT_DELETE");
    }
}

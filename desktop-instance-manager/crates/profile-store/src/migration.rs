//! Safe, idempotent, staging-verified one-time migration from legacy `MultiZaloDesktop`
//! data directories and `EBWebView` folders to canonical `NexaDock` paths.

use std::fs;
use std::path::{Path, PathBuf};

use dim_core::{now_rfc3339_utc, write_bytes_atomic, DimError};

/// Marker file written inside the migrated `NexaDock` directory upon verified completion.
pub const MIGRATED_MARKER_FILE: &str = ".migrated-from-legacy";

/// Marker file written inside staging while a migration copy is in progress.
pub const MIGRATION_IN_PROGRESS_FILE: &str = ".migration-in-progress";

/// Canonical preferences filename.
pub const CANONICAL_PREFERENCES_FILE: &str = "nexadock-preferences.json";

/// Legacy preferences filename.
pub const LEGACY_PREFERENCES_FILE: &str = "khoang-preferences.json";

/// Canonical Tauri bundle identifier for Windows `EBWebView` user data.
pub const CANONICAL_BUNDLE_ID: &str = "io.github.manhquydev.nexadock";

/// Legacy Tauri bundle identifier for Windows `EBWebView` user data.
pub const LEGACY_BUNDLE_ID: &str = "com.multizalodesktop.manager";

/// Outcome of a one-time legacy data directory migration check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationOutcome {
    /// Neither legacy nor target required migration (clean install).
    NoLegacyData,
    /// Target already contains `.migrated-from-legacy`; migration skipped idempotently.
    AlreadyMigrated,
    /// Both legacy and target exist without `.migrated-from-legacy`; target preserved untouched.
    TargetAlreadyExists,
    /// Legacy data was copied to staging, verified, and atomically promoted to target.
    Migrated,
}

/// Outcome of a one-time Windows `EBWebView` directory migration check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EbWebViewMigrationOutcome {
    /// Legacy `EBWebView` directory does not exist.
    NoLegacyData,
    /// Target `EBWebView` directory already exists; left untouched.
    TargetAlreadyExists,
    /// Legacy `EBWebView` directory was copied to target before WebView2 initialization.
    Migrated,
}

/// Migrates legacy `MultiZaloDesktop` data directory to `NexaDock` if applicable.
///
/// Safety guarantees:
/// - Clean install (no legacy dir): returns `NoLegacyData` without creating files.
/// - Reinstall / already migrated (`.migrated-from-legacy` in target): idempotent no-op.
/// - Both exist without `.migrated-from-legacy`: never overwrites target.
/// - Interrupted migration (`.migration-in-progress` in staging or target): cleans incomplete
///   staging/partial target and retries cleanly.
/// - Corrupt `profiles.json` or preferences JSON: fails with `DimError::SchemaError` without
///   modifying legacy data or creating target.
/// - Legacy directory is never auto-deleted.
pub fn migrate_legacy_data_dir(
    legacy_dir: &Path,
    target_dir: &Path,
) -> Result<MigrationOutcome, DimError> {
    let migrated_marker = target_dir.join(MIGRATED_MARKER_FILE);
    if target_dir.exists() && migrated_marker.exists() {
        return Ok(MigrationOutcome::AlreadyMigrated);
    }

    let staging_dir = staging_dir_for(target_dir);

    // Clean up any interrupted migration state before evaluating source/target.
    if target_dir.join(MIGRATION_IN_PROGRESS_FILE).exists() && legacy_dir.exists() {
        fs::remove_dir_all(target_dir)?;
    }
    if staging_dir.exists() {
        fs::remove_dir_all(&staging_dir)?;
    }

    if !legacy_dir.exists() {
        return Ok(MigrationOutcome::NoLegacyData);
    }

    if target_dir.exists() {
        return Ok(MigrationOutcome::TargetAlreadyExists);
    }

    // Validate legacy metadata BEFORE copying anything.
    validate_metadata_dir(legacy_dir, LEGACY_PREFERENCES_FILE)?;

    if let Some(parent) = target_dir.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir_all(&staging_dir)?;

    let in_progress_marker = staging_dir.join(MIGRATION_IN_PROGRESS_FILE);
    write_bytes_atomic(&in_progress_marker, now_rfc3339_utc().as_bytes())?;

    let copy_result = (|| -> Result<(), DimError> {
        copy_dir_recursive(legacy_dir, &staging_dir)?;

        // Migrate khoang-preferences.json -> nexadock-preferences.json inside staging.
        let legacy_prefs = staging_dir.join(LEGACY_PREFERENCES_FILE);
        let canonical_prefs = staging_dir.join(CANONICAL_PREFERENCES_FILE);
        if legacy_prefs.exists() && !canonical_prefs.exists() {
            fs::copy(&legacy_prefs, &canonical_prefs)?;
        }

        // Verify staged metadata integrity before promoting staging -> target.
        validate_metadata_dir(&staging_dir, CANONICAL_PREFERENCES_FILE)?;
        Ok(())
    })();

    if let Err(err) = copy_result {
        let _ = fs::remove_dir_all(&staging_dir);
        return Err(err);
    }

    if in_progress_marker.exists() {
        let _ = fs::remove_file(&in_progress_marker);
    }

    let marker_payload = serde_json::json!({
        "migrated_at_utc": now_rfc3339_utc(),
        "source_dir_name": legacy_dir.file_name().and_then(|n| n.to_str()).unwrap_or("MultiZaloDesktop"),
    });
    if let Err(err) = write_bytes_atomic(
        &staging_dir.join(MIGRATED_MARKER_FILE),
        marker_payload.to_string().as_bytes(),
    ) {
        let _ = fs::remove_dir_all(&staging_dir);
        return Err(err);
    }

    if let Err(err) = fs::rename(&staging_dir, target_dir) {
        let _ = fs::remove_dir_all(&staging_dir);
        return Err(DimError::Io(err));
    }

    Ok(MigrationOutcome::Migrated)
}

/// Migrates Windows `EBWebView` data from `com.multizalodesktop.manager\EBWebView` to
/// `io.github.manhquydev.nexadock\EBWebView` before WebView2 initializes.
///
/// Never calls `fs::remove_dir_all` inside any `EBWebView/` directory.
pub fn migrate_legacy_ebwebview_dir(
    legacy_ebwebview: &Path,
    target_ebwebview: &Path,
) -> Result<EbWebViewMigrationOutcome, DimError> {
    if target_ebwebview.exists() {
        return Ok(EbWebViewMigrationOutcome::TargetAlreadyExists);
    }
    if !legacy_ebwebview.exists() {
        return Ok(EbWebViewMigrationOutcome::NoLegacyData);
    }

    let Some(target_parent) = target_ebwebview.parent() else {
        return Err(DimError::InvalidProfileName(
            "invalid target EBWebView path".to_owned(),
        ));
    };
    fs::create_dir_all(target_parent)?;

    let staging_dir = target_parent.join(format!(
        ".EBWebView.migration-staging-{}",
        std::process::id()
    ));
    if staging_dir.exists() {
        // Rename away any abandoned staging dir rather than deleting inside EBWebView/.
        let abandoned = target_parent.join(format!(
            ".EBWebView.migration-abandoned-{}-{}",
            std::process::id(),
            dim_core::generate_profile_id()
        ));
        fs::rename(&staging_dir, &abandoned)?;
    }

    fs::create_dir_all(&staging_dir)?;
    copy_dir_recursive(legacy_ebwebview, &staging_dir)?;
    fs::rename(&staging_dir, target_ebwebview)?;
    Ok(EbWebViewMigrationOutcome::Migrated)
}

fn staging_dir_for(target_dir: &Path) -> PathBuf {
    let name = target_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("NexaDock");
    target_dir.with_file_name(format!(".{name}.migration-staging-{}", std::process::id()))
}

fn validate_metadata_dir(dir: &Path, prefs_file: &str) -> Result<(), DimError> {
    let profiles_path = dir.join("profiles.json");
    if profiles_path.exists() {
        let raw = fs::read_to_string(&profiles_path)?;
        crate::store::parse_profiles_json_str(&profiles_path, &raw)?;
    }

    let prefs_path = dir.join(prefs_file);
    if prefs_path.exists() {
        let raw = fs::read_to_string(&prefs_path)?;
        let value: serde_json::Value = serde_json::from_str(&raw).map_err(|err| {
            DimError::SchemaError(format!(
                "corrupt {prefs_file} in '{}': {err}",
                dir.display()
            ))
        })?;
        if !value.is_object() {
            return Err(DimError::SchemaError(format!(
                "{prefs_file} in '{}' must be a JSON object",
                dir.display()
            )));
        }
    }

    let web_sessions_path = dir.join("web-sessions.json");
    if web_sessions_path.exists() {
        let raw = fs::read_to_string(&web_sessions_path)?;
        let value: serde_json::Value = serde_json::from_str(&raw).map_err(|err| {
            DimError::SchemaError(format!(
                "corrupt web-sessions.json in '{}': {err}",
                dir.display()
            ))
        })?;
        if !value.is_object() {
            return Err(DimError::SchemaError(format!(
                "web-sessions.json in '{}' must be a JSON object",
                dir.display()
            )));
        }
    }

    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), DimError> {
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str == MIGRATION_IN_PROGRESS_FILE || name_str.ends_with(".lock") {
            continue;
        }
        let src_path = entry.path();
        let dst_path = dst.join(&name);
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            return Err(DimError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "refusing to migrate symlink or reparse point '{}'",
                    src_path.display()
                ),
            )));
        }
        if file_type.is_dir() {
            fs::create_dir_all(&dst_path)?;
            copy_dir_recursive(&src_path, &dst_path)?;
        } else if file_type.is_file() {
            fs::copy(&src_path, &dst_path)?;
        } else {
            return Err(DimError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "unsupported file type during migration at '{}'",
                    src_path.display()
                ),
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use dim_core::ProfileStore;

    use crate::JsonProfileStore;

    fn seed_legacy_dir(legacy: &Path) {
        let store = JsonProfileStore::new(legacy.to_path_buf()).unwrap();
        let prof = store.create_profile("Tài khoản 1", "Work").unwrap();
        let pdir = store.profile_dir_for(&prof).unwrap();
        fs::create_dir_all(pdir.join("AppData").join("Roaming").join("ZaloData")).unwrap();
        fs::write(
            pdir.join("AppData")
                .join("Roaming")
                .join("ZaloData")
                .join("sentinel.txt"),
            b"ok",
        )
        .unwrap();
        fs::write(
            legacy.join(LEGACY_PREFERENCES_FILE),
            br#"{"keep_in_tray":true,"locale":"en-US","pinned_accounts":["primary"]}"#,
        )
        .unwrap();
    }

    #[test]
    fn fixture_1_clean_install_uses_nexadock_without_migration() {
        let tmp = tempfile::tempdir().unwrap();
        let legacy = tmp.path().join("MultiZaloDesktop");
        let target = tmp.path().join("NexaDock");

        let outcome = migrate_legacy_data_dir(&legacy, &target).unwrap();
        assert_eq!(outcome, MigrationOutcome::NoLegacyData);
        assert!(!legacy.exists());
        assert!(!target.exists());
    }

    #[test]
    fn fixture_2_legacy_only_migrates_verifies_and_preserves_legacy() {
        let tmp = tempfile::tempdir().unwrap();
        let legacy = tmp.path().join("MultiZaloDesktop");
        let target = tmp.path().join("NexaDock");
        seed_legacy_dir(&legacy);

        let outcome = migrate_legacy_data_dir(&legacy, &target).unwrap();
        assert_eq!(outcome, MigrationOutcome::Migrated);
        assert!(target.join(MIGRATED_MARKER_FILE).exists());
        assert!(target.join(CANONICAL_PREFERENCES_FILE).exists());
        assert!(target.join("profiles.json").exists());
        assert!(legacy.exists(), "legacy directory must NOT be auto-deleted");

        let migrated_store = JsonProfileStore::new(target).unwrap();
        let list = migrated_store.list_profiles().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Tài khoản 1");
    }

    #[test]
    fn fixture_3_reinstall_is_idempotent_after_migration() {
        let tmp = tempfile::tempdir().unwrap();
        let legacy = tmp.path().join("MultiZaloDesktop");
        let target = tmp.path().join("NexaDock");
        seed_legacy_dir(&legacy);

        assert_eq!(
            migrate_legacy_data_dir(&legacy, &target).unwrap(),
            MigrationOutcome::Migrated
        );

        // Modify target after migration to prove second run does not overwrite.
        fs::write(target.join("post-migration-sentinel.txt"), b"keep").unwrap();

        let second = migrate_legacy_data_dir(&legacy, &target).unwrap();
        assert_eq!(second, MigrationOutcome::AlreadyMigrated);
        assert!(target.join("post-migration-sentinel.txt").exists());
    }

    #[test]
    fn fixture_4_interrupted_migration_cleans_staging_and_retries_cleanly() {
        let tmp = tempfile::tempdir().unwrap();
        let legacy = tmp.path().join("MultiZaloDesktop");
        let target = tmp.path().join("NexaDock");
        seed_legacy_dir(&legacy);

        // Simulate both an abandoned staging dir and an incomplete target with .migration-in-progress
        let staging = staging_dir_for(&target);
        fs::create_dir_all(&staging).unwrap();
        fs::write(staging.join("partial.tmp"), b"stale").unwrap();
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join(MIGRATION_IN_PROGRESS_FILE), b"interrupted").unwrap();

        let outcome = migrate_legacy_data_dir(&legacy, &target).unwrap();
        assert_eq!(outcome, MigrationOutcome::Migrated);
        assert!(!staging.exists());
        assert!(!target.join(MIGRATION_IN_PROGRESS_FILE).exists());
        assert!(target.join(MIGRATED_MARKER_FILE).exists());
        assert!(target.join(CANONICAL_PREFERENCES_FILE).exists());
    }

    #[test]
    fn fixture_5_corrupt_metadata_fails_safely_without_destroying_legacy() {
        let tmp = tempfile::tempdir().unwrap();
        let legacy = tmp.path().join("MultiZaloDesktop");
        let target = tmp.path().join("NexaDock");
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("profiles.json"), b"{not valid json").unwrap();

        let err = migrate_legacy_data_dir(&legacy, &target).unwrap_err();
        assert_eq!(err.code(), "JSON_ERROR");
        assert!(!target.exists());
        assert!(!staging_dir_for(&target).exists());
        assert_eq!(
            fs::read(legacy.join("profiles.json")).unwrap(),
            b"{not valid json"
        );
    }

    #[test]
    fn fixture_6_both_exist_without_marker_never_overwrites_nexadock() {
        let tmp = tempfile::tempdir().unwrap();
        let legacy = tmp.path().join("MultiZaloDesktop");
        let target = tmp.path().join("NexaDock");
        seed_legacy_dir(&legacy);

        let target_store = JsonProfileStore::new(target.clone()).unwrap();
        target_store
            .create_profile("Existing NexaDock Profile", "Keep")
            .unwrap();

        let outcome = migrate_legacy_data_dir(&legacy, &target).unwrap();
        assert_eq!(outcome, MigrationOutcome::TargetAlreadyExists);

        let list = target_store.list_profiles().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Existing NexaDock Profile");
    }

    #[test]
    fn ebwebview_migration_copies_when_target_missing_and_skips_when_present() {
        let tmp = tempfile::tempdir().unwrap();
        let legacy_eb = tmp.path().join(LEGACY_BUNDLE_ID).join("EBWebView");
        let target_eb = tmp.path().join(CANONICAL_BUNDLE_ID).join("EBWebView");

        assert_eq!(
            migrate_legacy_ebwebview_dir(&legacy_eb, &target_eb).unwrap(),
            EbWebViewMigrationOutcome::NoLegacyData
        );

        fs::create_dir_all(legacy_eb.join("Default")).unwrap();
        fs::write(legacy_eb.join("Default").join("Cookies"), b"cookie-data").unwrap();

        assert_eq!(
            migrate_legacy_ebwebview_dir(&legacy_eb, &target_eb).unwrap(),
            EbWebViewMigrationOutcome::Migrated
        );
        assert_eq!(
            fs::read(target_eb.join("Default").join("Cookies")).unwrap(),
            b"cookie-data"
        );
        assert!(legacy_eb.join("Default").join("Cookies").exists());

        assert_eq!(
            migrate_legacy_ebwebview_dir(&legacy_eb, &target_eb).unwrap(),
            EbWebViewMigrationOutcome::TargetAlreadyExists
        );
    }

    #[test]
    fn legacy_storage_version_1_profiles_json_migrates_cleanly() {
        let tmp = tempfile::tempdir().unwrap();
        let legacy = tmp.path().join("MultiZaloDesktop");
        let target = tmp.path().join("NexaDock");
        fs::create_dir_all(legacy.join("Profiles").join("tai-khoan-cu_p1")).unwrap();
        fs::write(
            legacy.join("profiles.json"),
            r#"{
  "StorageVersion": 1,
  "Profiles": [
    {
      "Id": "p1",
      "Name": "Tài khoản cũ",
      "DirectoryName": "tai-khoan-cu_p1",
      "CreatedAt": "2026-01-01T00:00:00Z"
    }
  ]
}"#
            .as_bytes(),
        )
        .unwrap();

        let outcome = migrate_legacy_data_dir(&legacy, &target).unwrap();
        assert_eq!(outcome, MigrationOutcome::Migrated);
        assert!(target.join(MIGRATED_MARKER_FILE).exists());

        let migrated_store = JsonProfileStore::new(target).unwrap();
        let list = migrated_store.list_profiles().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "p1");
        assert_eq!(list[0].name, "Tài khoản cũ");
    }
}

//! Core domain types, traits, path-traversal guards, and identity checks for `MultiZaloDesktop`.

pub mod error;
pub mod models;
pub mod path_guard;
pub mod sanitize;
pub mod slug;
pub mod time_util;
pub mod traits;
pub mod zalo_identity;

pub use error::{DimError, OperationOutcome};
pub use models::{
    resolve_data_dir, resolve_data_dir_for_platform, ApplicationInstallation, BackupFileEntry,
    BackupManifest, BackupResult, LaunchTarget, PlatformCapabilityReport, PlatformKind,
    PrimaryInstanceStatus, ProfileRecord, ProfileRuntimeState, ProfilesDocument, RecoveryReport,
    SupportStatus, APP_DIR_NAME, BACKUP_SCHEMA_VERSION, DIM_DATA_DIR_ENV, MIN_FREE_DISK_BYTES,
    PROFILES_SCHEMA_VERSION, ZALO_PROFILE_ID_FILE,
};
pub use path_guard::{check_disk_space_bytes, ensure_min_free_disk_space, resolve_safe_child};
pub use sanitize::redact_sensitive_text;
pub use slug::{
    build_directory_name, generate_profile_id, is_valid_profile_id, slugify_profile_name,
    validate_profile_name, MAX_PROFILE_NAME_LEN, MAX_SLUG_LEN,
};
pub use time_util::{
    format_unix_seconds_compact, format_unix_seconds_rfc3339, now_compact_utc, now_rfc3339_utc,
};
pub use traits::{
    ApplicationLocator, BackupManager, InstanceLauncher, InstanceMonitor, PlatformCapabilities,
    ProfileStore, RecoveryManager,
};
pub use zalo_identity::{
    check_duplicate_zalo_user_id, detect_zalo_user_id, is_valid_zalo_user_id,
    MIN_ZALO_USER_ID_LEN,
};

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::collections::HashMap;
    use std::fs;
    use std::path::Path;

    use super::*;

    #[test]
    fn vietnamese_slug_normalizes_diacritics_and_d_stroke() {
        assert_eq!(slugify_profile_name("Đức - Công việc"), "duc-cong-viec");
        assert_eq!(slugify_profile_name("Tài Khoản Cá Nhân #2"), "tai-khoan-ca-nhan-2");
        assert_eq!(slugify_profile_name("   !!!   "), "profile");
        assert_eq!(slugify_profile_name("Đặng Thị Ánh Tuyết"), "dang-thi-anh-tuyet");
    }

    #[test]
    fn profile_id_is_eight_lowercase_hex_chars() {
        let id1 = generate_profile_id();
        let id2 = generate_profile_id();
        assert!(is_valid_profile_id(&id1));
        assert!(is_valid_profile_id(&id2));
        assert_ne!(id1, id2);
    }

    #[test]
    fn resolve_safe_child_rejects_traversal_and_reserved_names() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(resolve_safe_child(tmp.path(), "../escape").is_err());
        assert!(resolve_safe_child(tmp.path(), "..\\escape").is_err());
        assert!(resolve_safe_child(tmp.path(), "/etc/passwd").is_err());
        assert!(resolve_safe_child(tmp.path(), "C:\\Windows").is_err());
        assert!(resolve_safe_child(tmp.path(), "CON").is_err());
        assert!(resolve_safe_child(tmp.path(), "sub/nul.txt").is_err());
        assert!(resolve_safe_child(tmp.path(), "bad\0name").is_err());

        let valid = resolve_safe_child(tmp.path(), "duc-cong-viec_a1b2c3d4").unwrap();
        assert_eq!(valid, tmp.path().join("duc-cong-viec_a1b2c3d4"));
    }

    #[test]
    fn data_dir_resolution_matches_platform_contracts() {
        let mut env = HashMap::new();
        env.insert("LOCALAPPDATA", "C:\\Users\\demo\\AppData\\Local");
        env.insert("HOME", "/home/demo");
        env.insert("XDG_DATA_HOME", "/home/demo/.xdg");

        let lookup = |k: &str| env.get(k).map(|v| (*v).to_owned());

        let win = resolve_data_dir_for_platform(PlatformKind::Windows, None, &lookup).unwrap();
        assert_eq!(win, Path::new("C:\\Users\\demo\\AppData\\Local").join("MultiZaloDesktop"));

        let mac = resolve_data_dir_for_platform(PlatformKind::MacOs, None, &lookup).unwrap();
        assert_eq!(
            mac,
            Path::new("/home/demo")
                .join("Library")
                .join("Application Support")
                .join("MultiZaloDesktop")
        );

        let linux = resolve_data_dir_for_platform(PlatformKind::Linux, None, &lookup).unwrap();
        assert_eq!(linux, Path::new("/home/demo/.xdg").join("MultiZaloDesktop"));

        env.insert("DIM_DATA_DIR", "D:\\CustomDimData");
        let env_over = resolve_data_dir_for_platform(PlatformKind::Windows, None, &lookup).unwrap();
        assert_eq!(env_over, Path::new("D:\\CustomDimData"));

        let explicit =
            resolve_data_dir_for_platform(PlatformKind::Windows, Some(Path::new("E:\\Explicit")), &lookup)
                .unwrap();
        assert_eq!(explicit, Path::new("E:\\Explicit"));
    }

    #[test]
    fn duplicate_zalo_user_id_guard_detects_running_collision() {
        let tmp = tempfile::tempdir().unwrap();
        let prod_dir = tmp.path().join("Database").join("_production").join("849123456");
        std::fs::create_dir_all(&prod_dir).unwrap();

        let detected = detect_zalo_user_id(tmp.path());
        assert_eq!(detected.as_deref(), Some("849123456"));

        let target = ProfileRecord {
            id: "1111aaaa".to_owned(),
            name: "Work".to_owned(),
            directory_name: "work_1111aaaa".to_owned(),
            created_at: "2026-11-17T00:00:00Z".to_owned(),
            last_launched_at: None,
            zalo_user_id: Some("849123456".to_owned()),
            notes: String::new(),
        };
        let other = ProfileRecord {
            id: "2222bbbb".to_owned(),
            name: "Personal".to_owned(),
            directory_name: "personal_2222bbbb".to_owned(),
            created_at: "2026-11-17T00:00:00Z".to_owned(),
            last_launched_at: None,
            zalo_user_id: None,
            notes: String::new(),
        };

        let err = check_duplicate_zalo_user_id(
            &target,
            detected.as_deref(),
            &[(other, Some("849123456".to_owned()))],
        )
        .unwrap_err();
        assert_eq!(err.code(), "DUPLICATE_ZALO_USER_ID");
    }

    #[test]
    fn disk_space_threshold_enforces_5_gib() {
        let tmp = tempfile::tempdir().unwrap();
        let below = MIN_FREE_DISK_BYTES - 1;
        let err = ensure_min_free_disk_space(tmp.path(), below, 0).unwrap_err();
        assert_eq!(err.code(), "INSUFFICIENT_DISK_SPACE");
        assert!(ensure_min_free_disk_space(tmp.path(), MIN_FREE_DISK_BYTES, 0).is_ok());
    }

    #[test]
    fn timestamps_format_correctly() {
        assert_eq!(format_unix_seconds_rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_unix_seconds_rfc3339(951_827_696), "2000-02-29T12:34:56Z");
        assert_eq!(format_unix_seconds_compact(951_827_696), "20000229T123456Z");
    }

    #[test]
    fn log_sanitizer_redacts_home_paths_and_tokens() {
        let msg = "Opened C:\\Users\\alice\\AppData\\Roaming\\ZaloData\ntoken=abc123secret\npassword=hunter2\nsession_key=xyz999\n\"token\": \"secret_tok\",\n\"zpw_sek\": \"sek_val\",\n\"secret\": \"s1\"\n\"cookie\": \"c1\"\n";
        let clean = redact_sensitive_text(msg, Some(Path::new("C:\\Users\\alice")));
        assert!(!clean.contains("alice"));
        assert!(!clean.contains("abc123secret"));
        assert!(!clean.contains("hunter2"));
        assert!(!clean.contains("xyz999"));
        assert!(!clean.contains("secret_tok"));
        assert!(!clean.contains("sek_val"));
        assert!(!clean.contains("\"s1\""));
        assert!(!clean.contains("\"c1\""));
        assert!(clean.contains("<USER_HOME>"));
        assert!(clean.contains("token=<REDACTED>"));
        assert!(clean.contains("password=<REDACTED>"));
        assert!(clean.contains("session_key=<REDACTED>"));
        assert!(clean.contains("\"token\": \"<REDACTED>\","));
        assert!(clean.contains("\"zpw_sek\": \"<REDACTED>\","));
    }

    #[test]
    fn detect_zalo_user_id_honors_config_json_db_path_location() {
        let tmp = tempfile::tempdir().unwrap();
        let zalo_data = tmp.path().join("ZaloData");
        let external_db = tmp.path().join("ExternalDb");
        fs::create_dir_all(&zalo_data).unwrap();
        fs::create_dir_all(external_db.join("Database").join("_production").join("777888999")).unwrap();
        fs::write(
            zalo_data.join("config.json"),
            serde_json::json!({
                "db_path": {
                    "location": external_db.to_string_lossy()
                }
            })
            .to_string(),
        )
        .unwrap();

        let detected = detect_zalo_user_id(&zalo_data);
        assert_eq!(detected.as_deref(), Some("777888999"));
    }

    #[cfg(unix)]
    #[test]
    fn resolve_safe_child_blocks_nonexistent_leaf_under_symlinked_subdir() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        let outside = tmp.path().join("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("sub")).unwrap();

        let err = resolve_safe_child(&root, "sub/new_file.txt").unwrap_err();
        assert_eq!(err.code(), "PATH_TRAVERSAL");
    }
}

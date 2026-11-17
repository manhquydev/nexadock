//! Windows 10/11 platform adapter crate (`dim-platform-windows`).

pub mod adapter;
pub mod layout;
#[cfg(windows)]
pub mod win32_ffi;

pub use adapter::WindowsPlatformAdapter;
pub use layout::{
    build_isolated_env_map, build_windows_command_line, compare_versions,
    encode_windows_env_block_utf16, ensure_windows_profile_layout, has_pe_authenticode_table,
    is_appdata_redirected, parse_version_parts, quote_windows_arg, windows_zalo_data_dir,
};

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::fs;
    use std::path::Path;

    use dim_core::{ApplicationLocator, ProfileRecord, ZALO_PROFILE_ID_FILE};

    use super::*;

    fn sample_profile() -> ProfileRecord {
        ProfileRecord {
            id: "a1b2c3d4".to_owned(),
            name: "Đức - Công việc".to_owned(),
            directory_name: "duc-cong-viec_a1b2c3d4".to_owned(),
            created_at: "2026-11-17T00:00:00Z".to_owned(),
            last_launched_at: None,
            zalo_user_id: None,
            notes: String::new(),
        }
    }

    #[test]
    fn windows_layout_creates_preseeded_markers_and_profile_id() {
        let tmp = tempfile::tempdir().unwrap();
        let profile = sample_profile();
        let profile_dir = tmp.path().join(&profile.directory_name);

        ensure_windows_profile_layout(&profile, &profile_dir).unwrap();

        let roaming = profile_dir.join("AppData").join("Roaming");
        let zalo_data = roaming.join("ZaloData");
        assert_eq!(
            fs::read_to_string(roaming.join("zmeta.json")).unwrap(),
            r#"{"failedMigrateCount":0}"#
        );
        assert_eq!(
            fs::read_to_string(zalo_data.join("migrate-config.json")).unwrap(),
            r#"{"zalopc_m_c":true}"#
        );
        assert_eq!(
            fs::read_to_string(zalo_data.join(ZALO_PROFILE_ID_FILE)).unwrap(),
            "a1b2c3d4"
        );
        assert!(profile_dir.join("AppData").join("Local").join("Temp").is_dir());
        assert!(profile_dir
            .join("Documents")
            .join("Zalo Received Files")
            .is_dir());
    }

    #[test]
    fn env_block_overrides_profile_paths_and_deduplicates_case_insensitively() {
        let profile = sample_profile();
        let profile_dir = Path::new("C:\\Users\\alice\\AppData\\Local\\MultiZaloDesktop\\Profiles\\duc-cong-viec_a1b2c3d4");
        let base = vec![
            ("Path".to_owned(), "C:\\Windows\\System32".to_owned()),
            ("appdata".to_owned(), "C:\\Users\\alice\\AppData\\Roaming".to_owned()),
            ("TEMP".to_owned(), "C:\\Users\\alice\\AppData\\Local\\Temp".to_owned()),
        ];

        let map = build_isolated_env_map(&base, &profile, profile_dir);
        assert_eq!(
            map.get("APPDATA").unwrap().1,
            profile_dir.join("AppData").join("Roaming").to_string_lossy()
        );
        assert_eq!(
            map.get("LOCALAPPDATA").unwrap().1,
            profile_dir.join("AppData").join("Local").to_string_lossy()
        );
        assert_eq!(map.get("HOMEDRIVE").unwrap().1, "C:");

        let utf16 = encode_windows_env_block_utf16(&map);
        assert!(utf16.ends_with(&[0, 0]));
    }

    #[test]
    fn folder_redirection_detector_flags_non_userprofile_appdata() {
        assert!(!is_appdata_redirected("%USERPROFILE%\\AppData\\Roaming"));
        assert!(is_appdata_redirected("\\\\fileserver\\users\\alice\\AppData"));
        assert!(is_appdata_redirected("D:\\OneDrive\\AppData\\Roaming"));
    }

    #[test]
    fn windows_arg_quoting_and_version_discovery_work() {
        assert_eq!(quote_windows_arg("simple"), "simple");
        assert_eq!(
            quote_windows_arg("C:\\Program Files\\App\\app.exe"),
            "\"C:\\Program Files\\App\\app.exe\""
        );
        assert_eq!(
            quote_windows_arg("a\"b\\"),
            "\"a\\\"b\\\\\""
        );

        let tmp = tempfile::tempdir().unwrap();
        let v1 = tmp.path().join("Zalo-26.9.5");
        let v2 = tmp.path().join("Zalo-26.10.20");
        fs::create_dir_all(v1.join("resources")).unwrap();
        fs::create_dir_all(v2.join("resources")).unwrap();
        fs::write(v1.join("Zalo.exe"), b"MZ").unwrap();
        fs::write(v1.join("resources").join("app.asar"), b"asar").unwrap();
        fs::write(v2.join("Zalo.exe"), b"MZ").unwrap();
        fs::write(v2.join("resources").join("app.asar"), b"asar").unwrap();

        let adapter = WindowsPlatformAdapter::new();
        let info = adapter.discover_installation(Some(tmp.path())).unwrap();
        assert!(info.is_valid);
        assert_eq!(info.version.as_deref(), Some("26.10.20"));
    }
}

//! macOS platform adapter crate (`dim-platform-macos`, status `NotTested`).

pub mod adapter;
pub mod layout;

pub use adapter::{read_singleton_or_json_pid, MacOsPlatformAdapter};
pub use layout::{
    build_open_args, ensure_macos_profile_layout, ensure_macos_user_symlinks,
    extract_plist_short_version, inspect_call_sockets_at, macos_zalo_data_dir,
    parse_singleton_lock_target, MACOS_CALL_SOCKETS, MACOS_SYMLINK_DIRS,
};

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::fs;
    use std::path::Path;

    use dim_core::{
        ApplicationLocator, PlatformCapabilities, ProfileRecord, SupportStatus,
        ZALO_PROFILE_ID_FILE,
    };

    use super::*;

    #[test]
    fn singleton_lock_parser_handles_hyphenated_hostnames() {
        assert_eq!(
            parse_singleton_lock_target("my-mac-book.local-48213"),
            Some(("my-mac-book.local".to_owned(), 48213))
        );
        assert_eq!(parse_singleton_lock_target("invalid-0"), None);
        assert_eq!(parse_singleton_lock_target("-123"), None);
    }

    #[test]
    fn open_command_builder_sets_cffixed_user_home() {
        let args = build_open_args(
            Path::new("/Applications/Zalo.app"),
            Path::new("/Users/alice/Library/Application Support/MultiZaloDesktop/Profiles/work_11223344"),
            &[],
        );
        assert_eq!(args[0], "-n");
        assert_eq!(args[1], "-a");
        assert!(args
            .iter()
            .any(|a| a.starts_with("CFFIXED_USER_HOME=/Users/alice/")));
    }

    #[test]
    fn macos_layout_and_plist_extraction_and_not_tested_status() {
        let tmp = tempfile::tempdir().unwrap();
        let profile = ProfileRecord {
            id: "11223344".to_owned(),
            name: "Mac Profile".to_owned(),
            directory_name: "mac-profile_11223344".to_owned(),
            created_at: "2026-11-17T00:00:00Z".to_owned(),
            last_launched_at: None,
            zalo_user_id: None,
            notes: String::new(),
        };
        let pdir = tmp.path().join(&profile.directory_name);
        ensure_macos_profile_layout(&profile, &pdir).unwrap();
        assert_eq!(
            fs::read_to_string(macos_zalo_data_dir(&pdir).join(ZALO_PROFILE_ID_FILE)).unwrap(),
            "11223344"
        );
        assert!(pdir.join("tmp").is_dir());

        let ver = extract_plist_short_version(
            "<dict><key>CFBundleShortVersionString</key><string>26.10.20</string></dict>",
        );
        assert_eq!(ver.as_deref(), Some("26.10.20"));

        let adapter = MacOsPlatformAdapter::new();
        assert_eq!(adapter.capabilities().official_zalo_status, SupportStatus::NotTested);
        let missing = adapter
            .discover_installation(Some(&tmp.path().join("MissingZalo.app")))
            .unwrap();
        assert!(!missing.is_valid);
        assert_eq!(missing.support_status, SupportStatus::NotTested);
    }
}

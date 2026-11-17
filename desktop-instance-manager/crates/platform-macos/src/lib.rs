//! macOS platform adapter crate (`dim-platform-macos`, official Zalo status `NotTested` / `LIVE_ZALO_NOT_TESTED`).
//!
//! Provides production-grade `.app` bundle & Mach-O slice validation (`bundle`), `codesign`/`spctl`
//! trust verification (`codesign`), `CFFIXED_USER_HOME` + `SUN_LEN`-safe `TMPDIR` isolation (`layout`),
//! `ps`-verified process-tree ownership & primary Zalo protection (`process`), TCC-aware window
//! inspection/focus (`window`), and an injectable OS seam (`ops`).

pub mod adapter;
pub mod bundle;
pub mod codesign;
pub mod layout;
pub mod ops;
pub mod process;
pub mod window;

pub use adapter::{
    read_singleton_or_json_pid, MacOsAdapterConfig, MacOsPlatformAdapter, LIVE_ZALO_STATUS_MARKER,
};
pub use bundle::{
    discover_zalo_bundle, inspect_bundle_at, inspect_macho_bytes, inspect_macho_file,
    parse_info_plist_xml, validate_macos_path, MacOsArchSlice, MachOInspection, PlistMetadata,
    ValidatedMacOsBundle, MACOS_PATH_MAX,
};
pub use codesign::{
    enforce_signature_or_err, parse_codesign_dv_output, parse_spctl_output,
    verify_macos_code_signature, CodeSignReport, CodeSignTrustStatus,
};
pub use layout::{
    build_isolated_child_env, build_open_args, build_open_launch_args, ensure_macos_profile_layout,
    ensure_macos_profile_layout_with_ops, ensure_macos_user_symlinks,
    ensure_macos_user_symlinks_with_options, ensure_not_macos_primary_path,
    extract_plist_short_version, inspect_call_sockets_at, macos_zalo_data_dir,
    normalize_macos_path_key, normalize_macos_unicode_key, parse_singleton_lock_target,
    resolve_effective_tmpdir, MacOsLayoutOptions, MacOsResolvedLayout, MACOS_CALL_SOCKETS,
    MACOS_PROFILE_META_FILE, MACOS_SAFE_TMPDIR_MAX_BYTES, MACOS_SUN_PATH_MAX, MACOS_SYMLINK_DIRS,
};
pub use ops::{
    CommandOutput, HostFixtureMacOsOps, MacOsSystemOps, MockMacOsSystemOps, NativeMacOsSystemOps,
    ProcessEntry, SpawnDetachedRequest, UnsupportedHostSystemOps, WindowInfo,
};
pub use process::{
    collect_descendant_pids, compute_profile_disk_usage_bytes, inspect_primary_macos,
    parse_ps_table, recover_macos_stale_artifacts, remove_file_no_follow,
    resolve_owned_profile_tree, stop_isolated_profile_tree, ProfileOperationGuard,
    VerifiedProfileProcessTree,
};
pub use window::{
    activate_profile_window, inspect_profile_window, parse_jxa_windows_json,
    request_graceful_app_terminate, MACOS_WINDOW_STATE_FILE,
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
            Path::new(
                "/Users/alice/Library/Application Support/MultiZaloDesktop/Profiles/work_11223344",
            ),
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
        assert!(pdir.join(MACOS_PROFILE_META_FILE).is_file());

        let ver = extract_plist_short_version(
            "<dict><key>CFBundleShortVersionString</key><string>26.10.20</string></dict>",
        );
        assert_eq!(ver.as_deref(), Some("26.10.20"));

        let adapter = MacOsPlatformAdapter::new();
        assert_eq!(
            adapter.capabilities().official_zalo_status,
            SupportStatus::NotTested
        );
        let missing = adapter
            .discover_installation(Some(&tmp.path().join("MissingZalo.app")))
            .unwrap();
        assert!(!missing.is_valid);
        assert_eq!(missing.support_status, SupportStatus::NotTested);
    }
}

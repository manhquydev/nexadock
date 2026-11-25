//! macOS platform adapter crate (`dim-platform-macos`, official Zalo status `NotTested` / `LIVE_ZALO_NOT_TESTED`).
//!
//! Provides production-grade `.app` bundle & Mach-O slice validation (`bundle`), 5-tier `codesign`/`spctl`
//! and Apple Designated Requirement `-R=...` publisher pinning (`codesign`), 12-surface `CFFIXED_USER_HOME`
//! and hardened `/tmp/dz-<uid>/<id>` short `TMPDIR` isolation (`layout`), `lstart`-verified process-tree
//! ownership & primary Zalo protection (`process`), zero-TCC macOS 14+ cooperative window activation
//! (`window`), and an injectable OS seam (`ops`).

pub mod adapter;
pub mod bundle;
pub mod codesign;
pub mod layout;
pub mod ops;
pub mod process;
pub mod window;

pub use adapter::{
    read_singleton_or_json_pid, verify_12_surface_runtime_isolation, MacOsAdapterConfig,
    MacOsPlatformAdapter, LIVE_ZALO_STATUS_MARKER,
};
pub use bundle::{
    discover_zalo_bundle, ensure_bundle_no_symlink_escape, inspect_bundle_at, inspect_macho_bytes,
    inspect_macho_file, parse_info_plist_xml, read_bundle_plist_checked, validate_macos_path,
    validate_plist_xml_structure, AppIdentityVerificationTier, MacOsArchSlice, MachOInspection,
    PlistMetadata, ValidatedMacOsBundle, MACOS_PATH_MAX,
};
pub use codesign::{
    enforce_signature_or_err, enforce_signature_tier_or_err, parse_codesign_dv_output,
    parse_spctl_output, verify_macos_bundle_identity, verify_macos_code_signature, CodeSignReport,
    CodeSignTrustStatus, PinnedPublisherPolicy,
};
pub use layout::{
    build_isolated_child_env, build_open_args, build_open_launch_args,
    cleanup_profile_short_tmpdir, derive_fixture_user_id, ensure_macos_profile_layout,
    ensure_macos_profile_layout_with_ops, ensure_macos_user_symlinks,
    ensure_macos_user_symlinks_with_options, ensure_not_macos_primary_path,
    extract_plist_short_version, inspect_call_socket_owners, inspect_call_sockets_at,
    macos_zalo_data_dir, normalize_macos_path_key, normalize_macos_unicode_key,
    parse_lsof_socket_owners, parse_singleton_lock_target, resolve_effective_tmpdir,
    verify_external_db_path_isolated, verify_no_forbidden_isolation_symlinks,
    verify_no_forbidden_isolation_symlinks_with_home,
    verify_no_forbidden_isolation_symlinks_with_options, MacOsLayoutOptions, MacOsResolvedLayout,
    DARWIN_SUN_PATH_MAX, MACOS_12_ISOLATED_SURFACES, MACOS_CALL_SOCKETS,
    MACOS_OPT_IN_USER_SYMLINKS, MACOS_PROFILE_META_FILE, MACOS_SAFE_TMPDIR_MAX_BYTES,
    MACOS_SUN_PATH_MAX, MACOS_SYMLINK_DIRS, MAC_CHROMIUM_TMPDIR_MAX_BYTES,
};
pub use ops::{
    CommandOutput, HostFixtureMacOsOps, MacOsSystemOps, MockMacOsSystemOps, MockSigtermHandler,
    NativeMacOsSystemOps, ProcessEntry, SpawnDetachedRequest, UnsupportedHostSystemOps, WindowInfo,
};
pub use process::{
    clear_runtime_launch_files, collect_descendant_pids, collect_profile_owned_pids,
    compute_profile_disk_usage_bytes, has_active_sqlite_wal, inspect_primary_macos, parse_ps_table,
    read_runtime_launch_record, read_runtime_pid, recover_macos_stale_artifacts,
    recover_macos_stale_artifacts_with_ops, remove_file_no_follow, resolve_owned_profile_tree,
    stop_isolated_profile_tree, symlink_or_file_present, write_runtime_launch_record,
    ProfileOperationGuard, RuntimeLaunchRecord, VerifiedProfileProcessTree,
    RUNTIME_LAUNCH_RECORD_FILE, RUNTIME_PID_FILE,
};
pub use window::{
    activate_macos_app_by_pid, activate_profile_window, inspect_profile_window,
    parse_activation_stdout, parse_jxa_windows_json, request_graceful_app_terminate,
    signal_fixture_focus_only, WindowActivationOutcome, MACOS_WINDOW_STATE_FILE,
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
            Path::new("/Users/alice/Library/Application Support/NexaDock/Profiles/work_11223344"),
            &[],
        );
        assert_eq!(args[0], "-n");
        assert_eq!(args[1], "-a");
        assert!(args
            .iter()
            .any(|a| a.starts_with("CFFIXED_USER_HOME=/Users/alice/")));
    }

    #[test]
    fn macos_layout_defaults_isolated_and_not_tested_status() {
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
        assert!(pdir.join("Downloads").is_dir());
        assert!(pdir.join("Library").join("Keychains").is_dir());
        assert!(pdir.join(MACOS_PROFILE_META_FILE).is_file());

        let ver = extract_plist_short_version(
            "<dict><key>CFBundleShortVersionString</key><string>26.10.20</string></dict>",
        );
        assert_eq!(ver.as_deref(), Some("26.10.20"));

        let adapter = MacOsPlatformAdapter::new();
        assert!(adapter.config().pinned_publisher_policy.is_none());
        assert!(!adapter.config().layout_options.symlink_user_dirs);
        assert!(!adapter.config().layout_options.symlink_keychain);
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

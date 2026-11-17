//! Linux platform adapter crate (`dim-platform-linux`).
//!
//! Provides isolated `HOME` + XDG Base Directory management, `/proc` process-tree ownership
//! tracking, ELF64/AppImage provenance & SHA-256 verification, startup-script hazard auditing,
//! `bubblewrap` (`bwrap`) containerization, and Ubuntu 24.04 AppArmor `SECURITY_BLOCKED` enforcement.

pub mod adapter;
pub mod discovery;
pub mod isolation;
pub mod layout;
pub mod procfs;
pub mod status;

pub use adapter::{read_linux_pid, LinuxPlatformAdapter};
pub use discovery::{
    analyze_startup_script, classify_binary_bytes, discover_compatible_apps_from_env,
    discover_compatible_apps_in_roots, inspect_user_binary, map_soname_to_ubuntu_packages,
    match_known_community_release, parse_desktop_entry, parse_ldd_missing_libraries,
    parse_version_from_filename, sha256_bytes, sha256_file, BinaryFormatKind,
    BinaryInspectionReport, CommunityReleaseReference, DesktopEntryFields, DiscoveredLinuxApp,
    ElfCpuArch, MissingLibraryDiagnostic, StartupScriptAudit, KNOWN_COMMUNITY_RELEASES,
};
pub use isolation::{
    compare_linux_isolation_mechanisms, current_effective_uid, enforce_launch_security_policy,
    find_executable_in_path, has_targeted_apparmor_userns_profile, probe_live_ubuntu_runtime,
    probe_ubuntu_runtime_at, render_recommended_apparmor_profile, DisplaySessionKind,
    IsolationComparisonEntry, LinuxIsolationMechanism, UbuntuRuntimeProbe,
};
pub use layout::{
    build_bwrap_args, build_bwrap_desktop_args, build_linux_isolated_env,
    build_sanitized_linux_spawn_env, ensure_linux_profile_layout, linux_zalo_data_dir,
    looks_like_appimage, measure_profile_disk_usage_bytes, verify_directory_ownership_and_mode,
    APPARMOR_NO_SANDBOX_NOTE, PASSTHROUGH_SESSION_ENV_KEYS,
};
pub use procfs::{
    collect_primary_process_tree, collect_profile_process_tree, entry_matches_profile,
    parse_proc_cmdline, parse_proc_environ, parse_proc_stat, parse_proc_status,
    parse_singleton_lock_target, parse_wmctrl_windows_for_pids, read_lock_pid, read_proc_entry,
    read_runtime_pid_hint, scan_proc_table, OwnedX11Window, ProcEntry, ProfileProcessTree,
};
pub use status::{SecurityBlockReason, UbuntuTargetTier, VerificationLabel};

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::fs;
    use std::path::Path;

    use dim_core::{
        ApplicationLocator, InstanceLauncher, LaunchTarget, PlatformCapabilities, ProfileRecord,
        SupportStatus, ZALO_PROFILE_ID_FILE,
    };

    use super::*;

    fn sample_profile() -> ProfileRecord {
        ProfileRecord {
            id: "cafebabe".to_owned(),
            name: "Linux Test".to_owned(),
            directory_name: "linux-test_cafebabe".to_owned(),
            created_at: "2026-11-17T00:00:00Z".to_owned(),
            last_launched_at: None,
            zalo_user_id: None,
            notes: String::new(),
        }
    }

    #[test]
    fn linux_reports_official_unsupported_and_appimage_experimental() {
        let tmp = tempfile::tempdir().unwrap();
        let appimage = tmp.path().join("ZaloUserSupplied.AppImage");
        fs::write(&appimage, b"appimage").unwrap();

        let adapter = LinuxPlatformAdapter::new();
        let caps = adapter.capabilities();
        assert_eq!(caps.official_zalo_status, SupportStatus::Unsupported);
        assert_eq!(caps.fixture_status, SupportStatus::Experimental);
        assert_eq!(caps.appimage_status, Some(SupportStatus::Experimental));
        assert!(caps
            .limitations
            .iter()
            .any(|l| l.contains("--no-sandbox") && l.contains("AppArmor")));

        let official = adapter.discover_installation(None).unwrap();
        assert!(!official.is_valid);
        assert_eq!(official.support_status, SupportStatus::Unsupported);

        let local_appimage = adapter.discover_installation(Some(&appimage)).unwrap();
        assert!(local_appimage.is_valid);
        assert_eq!(local_appimage.support_status, SupportStatus::Experimental);
    }

    #[test]
    fn linux_layout_and_bwrap_args_isolate_xdg_and_home() {
        let tmp = tempfile::tempdir().unwrap();
        let profile = sample_profile();
        let pdir = tmp.path().join(&profile.directory_name);
        ensure_linux_profile_layout(&profile, &pdir).unwrap();

        assert_eq!(
            fs::read_to_string(linux_zalo_data_dir(&pdir).join(ZALO_PROFILE_ID_FILE)).unwrap(),
            "cafebabe"
        );
        assert!(pdir.join(".local").join("share").is_dir());
        assert!(pdir.join(".local").join("state").is_dir());
        assert!(pdir.join(".cache").is_dir());
        assert!(pdir.join("tmp").is_dir());
        assert!(pdir.join("Downloads").is_dir());
        assert!(pdir.join("Documents").join("Zalo Received Files").is_dir());
        assert!(pdir.join(".config").join("user-dirs.dirs").is_file());
        assert!(pdir.join(".config").join("zmeta.json").is_file());

        let env = build_linux_isolated_env(&profile, &pdir);
        assert_eq!(
            env.get("HOME").unwrap(),
            &pdir.to_string_lossy().into_owned()
        );
        assert!(env.get("XDG_CONFIG_HOME").unwrap().ends_with(".config"));
        assert!(env.get("XDG_DOWNLOAD_DIR").unwrap().ends_with("Downloads"));
        assert!(env.get("XDG_DOCUMENTS_DIR").unwrap().ends_with("Documents"));

        let bwrap = build_bwrap_args(&pdir, Path::new("/tmp/fixture.AppImage"), &[]);
        assert!(bwrap.contains(&"--die-with-parent".to_owned()));
        assert!(bwrap.contains(&"--unshare-ipc".to_owned()));
        assert!(!bwrap.contains(&"--no-sandbox".to_owned()));
    }

    #[test]
    fn official_zalo_launch_is_rejected_on_linux() {
        let tmp = tempfile::tempdir().unwrap();
        let profile = sample_profile();
        let pdir = tmp.path().join(&profile.directory_name);
        let adapter = LinuxPlatformAdapter::new();
        let err = adapter
            .launch(
                &LaunchTarget::OfficialZalo {
                    custom_install_root: None,
                },
                &profile,
                &pdir,
            )
            .unwrap_err();
        assert_eq!(err.code(), "UNSUPPORTED_PLATFORM");
    }

    #[test]
    fn sha256_matches_nist_fips_180_4_vectors() {
        assert_eq!(
            sha256_bytes(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_bytes(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }
}

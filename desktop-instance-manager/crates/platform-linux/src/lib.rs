//! Linux platform adapter crate (`dim-platform-linux`).

pub mod adapter;
pub mod layout;

pub use adapter::{read_linux_pid, LinuxPlatformAdapter};
pub use layout::{
    build_bwrap_args, build_linux_isolated_env, ensure_linux_profile_layout,
    linux_zalo_data_dir, looks_like_appimage, APPARMOR_NO_SANDBOX_NOTE,
};

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
        assert!(pdir.join(".cache").is_dir());
        assert!(pdir.join("tmp").is_dir());

        let env = build_linux_isolated_env(&profile, &pdir);
        assert_eq!(env.get("HOME").unwrap(), &pdir.to_string_lossy().into_owned());
        assert!(env.get("XDG_CONFIG_HOME").unwrap().ends_with(".config"));

        let bwrap = build_bwrap_args(&pdir, Path::new("/tmp/fixture.AppImage"), &["--no-sandbox".to_owned()]);
        assert!(bwrap.contains(&"--die-with-parent".to_owned()));
        assert!(bwrap.contains(&"--no-sandbox".to_owned()));
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
}

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
        assert!(profile_dir
            .join("AppData")
            .join("Local")
            .join("Temp")
            .is_dir());
        assert!(profile_dir
            .join("Documents")
            .join("Zalo Received Files")
            .is_dir());
    }

    #[test]
    fn env_block_overrides_profile_paths_and_deduplicates_case_insensitively() {
        let profile = sample_profile();
        let profile_dir = Path::new(
            "C:\\Users\\alice\\AppData\\Local\\MultiZaloDesktop\\Profiles\\duc-cong-viec_a1b2c3d4",
        );
        let base = vec![
            ("Path".to_owned(), "C:\\Windows\\System32".to_owned()),
            (
                "appdata".to_owned(),
                "C:\\Users\\alice\\AppData\\Roaming".to_owned(),
            ),
            (
                "TEMP".to_owned(),
                "C:\\Users\\alice\\AppData\\Local\\Temp".to_owned(),
            ),
        ];

        let map = build_isolated_env_map(&base, &profile, profile_dir);
        assert_eq!(
            map.get("APPDATA").unwrap().1,
            profile_dir
                .join("AppData")
                .join("Roaming")
                .to_string_lossy()
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
        assert!(is_appdata_redirected(
            "\\\\fileserver\\users\\alice\\AppData"
        ));
        assert!(is_appdata_redirected("D:\\OneDrive\\AppData\\Roaming"));
    }

    #[test]
    fn windows_arg_quoting_and_version_discovery_work() {
        assert_eq!(quote_windows_arg("simple"), "simple");
        assert_eq!(
            quote_windows_arg("C:\\Program Files\\App\\app.exe"),
            "\"C:\\Program Files\\App\\app.exe\""
        );
        assert_eq!(quote_windows_arg("a\"b\\"), "\"a\\\"b\\\\\"");

        let tmp = tempfile::tempdir().unwrap();
        let v1 = tmp.path().join("Zalo-26.9.5");
        let v2 = tmp.path().join("Zalo-26.10.20");
        fs::create_dir_all(v1.join("resources")).unwrap();
        fs::create_dir_all(v2.join("resources")).unwrap();
        fs::write(v1.join("Zalo.exe"), b"MZ").unwrap();
        fs::write(v1.join("resources").join("app.asar"), b"asar").unwrap();
        fs::write(v2.join("Zalo.exe"), b"MZ").unwrap();
        fs::write(v2.join("resources").join("app.asar"), b"asar").unwrap();

        let strict_adapter = WindowsPlatformAdapter::new();
        let strict_info = strict_adapter
            .discover_installation(Some(tmp.path()))
            .unwrap();
        assert!(!strict_info.is_valid);
        assert_eq!(strict_info.signature_status, "Unsigned");

        let layout_adapter = WindowsPlatformAdapter::new().with_signature_enforcement(false);
        let info = layout_adapter
            .discover_installation(Some(tmp.path()))
            .unwrap();
        assert!(info.is_valid);
        assert_eq!(info.version.as_deref(), Some("26.10.20"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_authenticode_verification_covers_valid_unsigned_tampered_wrong_publisher_and_missing(
    ) {
        let tmp = tempfile::tempdir().unwrap();

        let candidates = [
            std::path::PathBuf::from(
                r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
            ),
            std::path::PathBuf::from(r"C:\Program Files\Microsoft\Edge\Application\msedge.exe"),
            std::path::PathBuf::from(r"C:\Program Files\nodejs\node.exe"),
            std::path::PathBuf::from(r"C:\Program Files\Git\cmd\git.exe"),
        ];
        let signed_sys_bin = candidates
            .into_iter()
            .find(|p| p.exists())
            .expect("expected at least one embedded-Authenticode system/runtime binary (msedge.exe, node.exe, or git.exe)");

        // 1. Valid embedded-Authenticode binary passes WinVerifyTrust when no VNG constraint is imposed
        match win32_ffi::verify_authenticode_signature(&signed_sys_bin, None) {
            win32_ffi::AuthenticodeTrustOutcome::Trusted { subject, .. } => {
                let subj = subject.unwrap_or_default();
                assert!(
                    !subj.is_empty(),
                    "expected signer subject for {signed_sys_bin:?}"
                );
                assert!(!subj.to_ascii_uppercase().contains("VNG"));
            }
            other => panic!("expected Trusted for {signed_sys_bin:?}, got {other:?}"),
        }

        // 2. Validly signed binary with wrong publisher is rejected when VNG is required
        match win32_ffi::verify_authenticode_signature(&signed_sys_bin, Some("VNG")) {
            win32_ffi::AuthenticodeTrustOutcome::WrongPublisher { .. } => {}
            other => {
                panic!("expected WrongPublisher for {signed_sys_bin:?} against VNG, got {other:?}")
            }
        }

        let wrong_pub_root = tmp.path().join("wrong_pub");
        let wrong_ver = wrong_pub_root.join("Zalo-26.10.20");
        fs::create_dir_all(wrong_ver.join("resources")).unwrap();
        fs::copy(&signed_sys_bin, wrong_ver.join("Zalo.exe")).unwrap();
        fs::write(wrong_ver.join("resources").join("app.asar"), b"asar").unwrap();
        let wrong_install = WindowsPlatformAdapter::new()
            .discover_installation(Some(&wrong_pub_root))
            .unwrap();
        assert!(!wrong_install.is_valid);
        assert!(wrong_install
            .signature_status
            .contains("UntrustedPublisher"));

        // 3. Unsigned binary is rejected
        let unsigned_bin = tmp.path().join("unsigned.exe");
        fs::write(&unsigned_bin, b"MZ\x90\x00unsigned-placeholder").unwrap();
        assert_eq!(
            win32_ffi::verify_authenticode_signature(&unsigned_bin, None),
            win32_ffi::AuthenticodeTrustOutcome::Unsigned
        );

        // 4. Tampered copy of embedded-Authenticode binary is rejected by WinVerifyTrust
        let tampered_bin = tmp.path().join("tampered.exe");
        let mut bytes = fs::read(&signed_sys_bin).unwrap();
        let tamper_idx = bytes.len() / 2;
        bytes[tamper_idx] ^= 0xff;
        fs::write(&tampered_bin, &bytes).unwrap();
        match win32_ffi::verify_authenticode_signature(&tampered_bin, None) {
            win32_ffi::AuthenticodeTrustOutcome::UntrustedOrTampered { .. } => {}
            other => panic!("expected UntrustedOrTampered for modified PE, got {other:?}"),
        }

        // 5. Missing file & forged PE security table fail closed
        assert_eq!(
            win32_ffi::verify_authenticode_signature(&tmp.path().join("missing.exe"), Some("VNG")),
            win32_ffi::AuthenticodeTrustOutcome::Missing
        );
        let mut forged_pe = vec![0u8; 512];
        forged_pe[0] = b'M';
        forged_pe[1] = b'Z';
        forged_pe[0x3C] = 0x80; // e_lfanew = 0x80
        forged_pe[0x80..0x84].copy_from_slice(b"PE\0\0");
        let opt = 0x80 + 24;
        forged_pe[opt..opt + 2].copy_from_slice(&0x020Bu16.to_le_bytes()); // PE32+
        forged_pe[opt + 144..opt + 148].copy_from_slice(&0x1000u32.to_le_bytes());
        forged_pe[opt + 148..opt + 152].copy_from_slice(&64u32.to_le_bytes());
        let forged_bin = tmp.path().join("forged.exe");
        fs::write(&forged_bin, &forged_pe).unwrap();
        match win32_ffi::verify_authenticode_signature(&forged_bin, None) {
            win32_ffi::AuthenticodeTrustOutcome::UntrustedOrTampered { .. } => {}
            other => {
                panic!("expected UntrustedOrTampered for forged PE cert directory, got {other:?}")
            }
        }

        // 6. Real OS elevation & ACL probes return concrete values (never hardcoded)
        assert!(win32_ffi::is_current_process_elevated().is_some());
        let user_owned_dir = tmp.path().join("acl_probe");
        fs::create_dir_all(&user_owned_dir).unwrap();
        assert!(win32_ffi::check_directory_acl_safe(&user_owned_dir).is_some());
    }

    #[cfg(windows)]
    #[test]
    fn publisher_pinning_requires_exact_vng_rdn_not_substring() {
        assert!(win32_ffi::subject_matches_publisher(
            "CN=VNG GROUP JSC, O=VNG GROUP JSC, S=Ho Chi Minh City, C=VN, SERIALNUMBER=0303490096",
            "VNG"
        ));
        assert!(win32_ffi::subject_matches_publisher(
            "O=\"VNG GROUP JSC\", CN=\"VNG GROUP JSC\", C=VN",
            "VNG"
        ));
        assert!(!win32_ffi::subject_matches_publisher(
            "CN=Other VNG Corp, O=Other Corp, C=US",
            "VNG"
        ));
        assert!(!win32_ffi::subject_matches_publisher(
            "OU=Devng Team, CN=Evil Signer, O=Evil Corp",
            "VNG"
        ));
    }
}

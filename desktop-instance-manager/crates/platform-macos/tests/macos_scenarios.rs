//! Comprehensive Rust integration & safety regression suite for `dim-platform-macos` covering:
//!   - `P0-1`: 5-tier bundle & cryptographic publisher verification + all 7 identity failure modes + `XP-MAC-01`
//!   - `P0-2`: 12-surface profile data isolation, `Downloads`/`Keychains` default isolation, `F-CORE-01` DB guard, `CT-03`
//!   - `P0-3`: Multi-evidence `lstart`-bound PID ownership across all 8 PID-reuse, primary, unmanaged, restart & tree cases
//!   - `P0-4`: `DEFECT-01`..`DEFECT-06` (`XP-MAC-03`, `LC-06`, `LC-07`, `LC-08`, `LC-09`) regressions
//!   - `P1-1`: Darwin `SUN_LEN` (`104B`) `/tmp/dz-<uid>/<id>` symlink-squatting & permission hardening
//!   - `P1-2`: All 7 graceful shutdown, delayed-exit, hung-process fail-closed, and `ppid == 1` orphan helper cases
//!   - `P1-3`: Zero-TCC macOS 14+ cooperative `NSRunningApplication` activation across all 4 focus cases
//!   - All 13 baseline multi-instance reliability, Unicode NFC/NFD, `PATH_MAX`, symlink unlink, and permission scenarios

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::time::Duration;

use dim_core::{
    ApplicationLocator, InstanceLauncher, InstanceMonitor, LaunchTarget, PlatformCapabilities,
    ProfileRecord, ProfileStore, RecoveryManager, SupportStatus, ZALO_PROFILE_ID_FILE,
};
use dim_platform_macos::bundle::{
    CPU_TYPE_ARM64, CPU_TYPE_X86_64, FAT_MAGIC, FAT_MAGIC_64, MH_MAGIC_64,
};
use dim_platform_macos::ops::SIMULATED_POSIX_MODE_FILE;
use dim_platform_macos::window::{APPLESCRIPT_ACTIVATE_BY_PID, JXA_QUERY_WINDOWS_SCRIPT};
use dim_platform_macos::{
    cleanup_profile_short_tmpdir, ensure_not_macos_primary_path, inspect_macho_bytes,
    macos_zalo_data_dir, normalize_macos_path_key, normalize_macos_unicode_key,
    parse_codesign_dv_output, parse_jxa_windows_json, parse_ps_table, parse_spctl_output,
    read_runtime_launch_record, resolve_effective_tmpdir, validate_macos_path,
    verify_12_surface_runtime_isolation, verify_external_db_path_isolated,
    verify_macos_bundle_identity, verify_macos_code_signature, write_runtime_launch_record,
    AppIdentityVerificationTier, CodeSignTrustStatus, CommandOutput, HostFixtureMacOsOps,
    MacOsArchSlice, MacOsLayoutOptions, MacOsPlatformAdapter, MacOsSystemOps, MockMacOsSystemOps,
    PinnedPublisherPolicy, ProcessEntry, RuntimeLaunchRecord, WindowInfo, DARWIN_SUN_PATH_MAX,
    LIVE_ZALO_STATUS_MARKER, MACOS_12_ISOLATED_SURFACES, MACOS_PATH_MAX, MACOS_PROFILE_META_FILE,
    MACOS_SAFE_TMPDIR_MAX_BYTES,
};
use dim_profile_store::JsonProfileStore;

fn make_macho_thin_64_le(cpu_type: i32) -> Vec<u8> {
    let mut buf = Vec::with_capacity(32);
    buf.extend_from_slice(&MH_MAGIC_64.to_le_bytes());
    buf.extend_from_slice(&cpu_type.to_le_bytes());
    buf.extend_from_slice(&[0u8; 24]);
    buf
}

fn make_macho_universal2_fat() -> Vec<u8> {
    let mut buf = Vec::with_capacity(64);
    buf.extend_from_slice(&FAT_MAGIC.to_be_bytes());
    buf.extend_from_slice(&2u32.to_be_bytes());
    buf.extend_from_slice(&CPU_TYPE_X86_64.to_be_bytes());
    buf.extend_from_slice(&[0u8; 16]);
    buf.extend_from_slice(&CPU_TYPE_ARM64.to_be_bytes());
    buf.extend_from_slice(&[0u8; 16]);
    buf
}

fn create_synthetic_zalo_app(root: &Path, macho_bytes: &[u8], binary_plist: bool) -> PathBuf {
    let app = root.join("Zalo.app");
    let contents = app.join("Contents");
    let macos_dir = contents.join("MacOS");
    let res_dir = contents.join("Resources");
    let helper_macos = contents
        .join("Frameworks")
        .join("ZaloHelper.app")
        .join("Contents")
        .join("MacOS");

    fs::create_dir_all(&macos_dir).unwrap();
    fs::create_dir_all(&res_dir).unwrap();
    fs::create_dir_all(&helper_macos).unwrap();

    fs::write(macos_dir.join("Zalo"), macho_bytes).unwrap();
    fs::write(res_dir.join("app.asar"), b"synthetic-asar").unwrap();
    fs::write(helper_macos.join("ZaloCall"), macho_bytes).unwrap();

    if binary_plist {
        let mut bplist = b"bplist00".to_vec();
        bplist.extend_from_slice(b"\0\x01\x02\x03binary-plist-payload");
        fs::write(contents.join("Info.plist"), bplist).unwrap();
    } else {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>com.vng.zalo</string>
    <key>CFBundleExecutable</key>
    <string>Zalo</string>
    <key>CFBundleShortVersionString</key>
    <string>26.10.20</string>
    <key>CFBundleVersion</key>
    <string>261020</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>LSMinimumSystemVersion</key>
    <string>12.0</string>
    <key>LSMultipleInstancesProhibited</key>
    <true/>
</dict>
</plist>"#;
        fs::write(contents.join("Info.plist"), xml).unwrap();
    }
    app
}

fn make_profile(id: &str, name: &str, dir_name: &str) -> ProfileRecord {
    ProfileRecord {
        id: id.to_owned(),
        name: name.to_owned(),
        directory_name: dir_name.to_owned(),
        created_at: "2026-11-17T00:00:00Z".to_owned(),
        last_launched_at: None,
        zalo_user_id: None,
        notes: String::new(),
    }
}

fn locate_fixture_script() -> PathBuf {
    let raw = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("platform-macos")
        .join("fixture-macos-app.mjs");
    let canon = fs::canonicalize(&raw).unwrap();
    PathBuf::from(
        canon
            .to_string_lossy()
            .trim_start_matches(r"\\?\")
            .to_owned(),
    )
}

#[test]
fn macho_inspector_parses_arm64_x86_64_universal2_and_fat64() {
    let arm64 = inspect_macho_bytes(&make_macho_thin_64_le(CPU_TYPE_ARM64));
    assert!(arm64.is_macho && !arm64.is_universal_fat);
    assert!(arm64.supports_arm64() && !arm64.supports_x86_64());
    assert_eq!(arm64.architectures, vec![MacOsArchSlice::Arm64]);

    let x64 = inspect_macho_bytes(&make_macho_thin_64_le(CPU_TYPE_X86_64));
    assert!(x64.is_macho && x64.supports_x86_64() && !x64.supports_arm64());

    let univ = inspect_macho_bytes(&make_macho_universal2_fat());
    assert!(univ.is_macho && univ.is_universal_fat);
    assert!(univ.supports_arm64() && univ.supports_x86_64());
    assert_eq!(univ.summary_label(), "Universal2 (x86_64 + arm64)");

    let mut fat64 = Vec::new();
    fat64.extend_from_slice(&FAT_MAGIC_64.to_be_bytes());
    fat64.extend_from_slice(&1u32.to_be_bytes());
    fat64.extend_from_slice(&CPU_TYPE_ARM64.to_be_bytes());
    fat64.extend_from_slice(&[0u8; 28]);
    let parsed_fat64 = inspect_macho_bytes(&fat64);
    assert!(
        parsed_fat64.is_macho && parsed_fat64.is_universal_fat && parsed_fat64.supports_arm64()
    );

    assert!(!inspect_macho_bytes(b"#!/bin/sh\necho hi\n").is_macho);
    assert!(!inspect_macho_bytes(&[0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 99]).is_macho);
}

#[test]
fn p0_1_all_seven_bundle_and_codesign_identity_failure_modes_plus_tier5_verified_publisher() {
    let tmp = tempfile::tempdir().unwrap();
    let app = create_synthetic_zalo_app(tmp.path(), &make_macho_universal2_fat(), false);
    let prof = make_profile("aabbccdd", "Identity Check", "id-check_aabbccdd");
    let pdir = tmp.path().join("Profiles").join(&prof.directory_name);

    // XP-MAC-01 non-Darwin baseline compatibility: structural valid, support_status = NotTested.
    #[cfg(not(target_os = "macos"))]
    {
        let non_mac = MacOsPlatformAdapter::new();
        let disc = non_mac.discover_installation(Some(&app)).unwrap();
        assert!(disc.is_valid);
        assert_eq!(disc.support_status, SupportStatus::NotTested);
    }

    // Failure Mode 1: Invalid / Unsigned & Ad-hoc signature -> UNVERIFIED_BINARY.
    let parsed_adhoc = parse_codesign_dv_output(
        "Identifier=com.vng.zalo\nSignature=adhoc\nTeamIdentifier=not set\n",
    );
    assert!(parsed_adhoc.is_adhoc && parsed_adhoc.team_identifier.is_none());
    let (accepted, src) = parse_spctl_output(
        true,
        "/Applications/Zalo.app: accepted\nsource=Notarized Developer ID\n",
    );
    assert_eq!(accepted, Some(true));
    assert_eq!(src.as_deref(), Some("Notarized Developer ID"));

    let unsigned_mock =
        MockMacOsSystemOps::new_macos(tmp.path().join("h_u"), tmp.path().join("s_u"))
            .with_command_handler(Arc::new(|prog, args, _| {
                if prog.to_string_lossy().ends_with("codesign")
                    && args.iter().any(|a| a == "--verify")
                {
                    return Ok(CommandOutput {
                        status_code: Some(1),
                        stdout: Vec::new(),
                        stderr: b"Zalo.app: code object is not signed at all\n".to_vec(),
                    });
                }
                Ok(CommandOutput {
                    status_code: Some(0),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            }));
    let unsigned_sig = verify_macos_code_signature(&unsigned_mock, &app, None);
    assert_eq!(unsigned_sig.status, CodeSignTrustStatus::Unsigned);
    let unsigned_adapter = MacOsPlatformAdapter::with_system_ops(Arc::new(unsigned_mock))
        .with_pinned_publisher("com.vng.zalo", "99VNGTEAM1");
    let err_u = unsigned_adapter
        .launch(
            &LaunchTarget::OfficialZalo {
                custom_install_root: Some(app.clone()),
            },
            &prof,
            &pdir,
        )
        .unwrap_err();
    assert_eq!(err_u.code(), "UNVERIFIED_BINARY");

    // Failure Mode 2: Tampered bundle / tampered nested ZaloHelper.app after signing -> TamperedSignature.
    let tampered_helper_mock =
        MockMacOsSystemOps::new_macos(tmp.path().join("h_t"), tmp.path().join("s_t"))
            .with_command_handler(Arc::new(|prog, args, _| {
                if prog.to_string_lossy().ends_with("codesign")
                    && args.iter().any(|a| a == "--verify")
                    && args.iter().any(|a| a.contains("ZaloHelper.app"))
                {
                    return Ok(CommandOutput {
                        status_code: Some(1),
                        stdout: Vec::new(),
                        stderr: b"ZaloHelper.app: a sealed resource is missing or invalid\n"
                            .to_vec(),
                    });
                }
                Ok(CommandOutput {
                    status_code: Some(0),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            }));
    let inspected_bundle =
        dim_platform_macos::inspect_bundle_at(&tampered_helper_mock, &app).unwrap();
    let tampered_sig = verify_macos_bundle_identity(
        &tampered_helper_mock,
        &inspected_bundle,
        Some(&PinnedPublisherPolicy::new("com.vng.zalo", "99VNGTEAM1")),
    );
    assert_eq!(tampered_sig.status, CodeSignTrustStatus::TamperedSignature);

    // Failure Mode 3: Wrong publisher (even if Authority contains "VNG", mismatched TeamIdentifier is rejected!).
    let wrong_pub_mock =
        MockMacOsSystemOps::new_macos(tmp.path().join("h_w"), tmp.path().join("s_w"))
            .with_command_handler(Arc::new(|prog, args, _| {
                let p = prog.to_string_lossy();
                if p.ends_with("codesign") && args.iter().any(|a| a == "-dv") {
                    return Ok(CommandOutput {
                        status_code: Some(0),
                        stdout: Vec::new(),
                        stderr: b"Identifier=com.vng.zalo\nTeamIdentifier=EVIL999999\nAuthority=Developer ID Application: Fake VNG Corp (EVIL999999)\n".to_vec(),
                    });
                }
                if p.ends_with("spctl") {
                    return Ok(CommandOutput {
                        status_code: Some(0),
                        stdout: Vec::new(),
                        stderr: b"Zalo.app: accepted\nsource=Notarized Developer ID\n".to_vec(),
                    });
                }
                Ok(CommandOutput {
                    status_code: Some(0),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            }));
    let wrong_pub_adapter = MacOsPlatformAdapter::with_system_ops(Arc::new(wrong_pub_mock))
        .with_pinned_publisher("com.vng.zalo", "99VNGTEAM1");
    let wrong_disc = wrong_pub_adapter.discover_installation(Some(&app)).unwrap();
    assert!(!wrong_disc.is_valid);
    assert!(wrong_disc.signature_status.contains("WrongPublisher"));

    // Failure Mode 4: Missing bundle -> NotFound.
    let missing_disc = wrong_pub_adapter
        .discover_installation(Some(&tmp.path().join("DoesNotExist.app")))
        .unwrap();
    assert!(!missing_disc.is_valid);
    assert_eq!(missing_disc.signature_status, "NotFound");

    // Failure Mode 5: Malformed Info.plist (corrupt XML AND failed bplist00 plutil) -> InvalidBundle.
    let bad_xml_dir = tmp.path().join("BadXml");
    let bad_xml_app = create_synthetic_zalo_app(&bad_xml_dir, &make_macho_universal2_fat(), false);
    fs::write(
        bad_xml_app.join("Contents/Info.plist"),
        "<plist><dict><key>CFBundleIdentifier</key><string>com.vng.zalo</dict>",
    )
    .unwrap();
    let bad_xml_disc = wrong_pub_adapter
        .discover_installation(Some(&bad_xml_app))
        .unwrap();
    assert!(!bad_xml_disc.is_valid);
    assert_eq!(bad_xml_disc.signature_status, "InvalidBundle");

    let bad_bplist_dir = tmp.path().join("BadBplist");
    let bad_bplist_app =
        create_synthetic_zalo_app(&bad_bplist_dir, &make_macho_universal2_fat(), true);
    let fail_plutil_mock =
        MockMacOsSystemOps::new_macos(tmp.path().join("h_bp"), tmp.path().join("s_bp"))
            .with_command_handler(Arc::new(|prog, _, _| {
                if prog.to_string_lossy().ends_with("plutil") {
                    return Ok(CommandOutput {
                        status_code: Some(1),
                        stdout: Vec::new(),
                        stderr: b"corrupt binary plist\n".to_vec(),
                    });
                }
                Ok(CommandOutput {
                    status_code: Some(0),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            }));
    let fail_plutil_adapter = MacOsPlatformAdapter::with_system_ops(Arc::new(fail_plutil_mock));
    let bad_bp_disc = fail_plutil_adapter
        .discover_installation(Some(&bad_bplist_app))
        .unwrap();
    assert!(!bad_bp_disc.is_valid);
    assert_eq!(bad_bp_disc.signature_status, "InvalidBundle");

    // Failure Mode 6: Wrong Mach-O architecture (`arm64`-only on Intel `x86_64` host).
    let arm_only_dir = tmp.path().join("ArmOnly");
    let arm_only_app =
        create_synthetic_zalo_app(&arm_only_dir, &make_macho_thin_64_le(CPU_TYPE_ARM64), false);
    let mut intel_mock =
        MockMacOsSystemOps::new_macos(tmp.path().join("h_in"), tmp.path().join("s_in"));
    intel_mock.arch = "x86_64";
    let intel_adapter = MacOsPlatformAdapter::with_system_ops(Arc::new(intel_mock));
    let arch_disc = intel_adapter
        .discover_installation(Some(&arm_only_app))
        .unwrap();
    assert!(!arch_disc.is_valid);
    assert!(arch_disc
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("arm64-only Mach-O bundle cannot execute on Intel x86_64"));

    // Failure Mode 7: Unverifiable publisher metadata (default unpinned policy NEVER auto-trusts GatekeeperAssessed!).
    let valid_vng_handler = Arc::new(|prog: &Path, args: &[String], _: &[(String, String)]| {
        let p = prog.to_string_lossy();
        if p.ends_with("codesign") && args.iter().any(|a| a == "-dv") {
            return Ok(CommandOutput {
                status_code: Some(0),
                stdout: Vec::new(),
                stderr: b"Identifier=com.vng.zalo\nTeamIdentifier=99VNGTEAM1\nAuthority=Developer ID Application: VNG ONLINE CO.,LTD (99VNGTEAM1)\nAuthority=Developer ID Certification Authority\nAuthority=Apple Root CA\n".to_vec(),
            });
        }
        if p.ends_with("codesign") && args.iter().any(|a| a.starts_with("-R=")) {
            let ok = args
                .iter()
                .any(|a| a.contains("identifier \"com.vng.zalo\"") && a.contains("\"99VNGTEAM1\""));
            return Ok(CommandOutput {
                status_code: Some(if ok { 0 } else { 3 }),
                stdout: Vec::new(),
                stderr: if ok {
                    Vec::new()
                } else {
                    b"test-requirement: code failed to satisfy specified code requirement(s)\n"
                        .to_vec()
                },
            });
        }
        if p.ends_with("spctl") {
            return Ok(CommandOutput {
                status_code: Some(0),
                stdout: Vec::new(),
                stderr: b"/Applications/Zalo.app: accepted\nsource=Notarized Developer ID\n"
                    .to_vec(),
            });
        }
        if p.ends_with("xattr") {
            return Ok(CommandOutput {
                status_code: Some(0),
                stdout: b"0183;673a0000;Safari;\n".to_vec(),
                stderr: Vec::new(),
            });
        }
        Ok(CommandOutput {
            status_code: Some(0),
            stdout: Vec::new(),
            stderr: Vec::new(),
        })
    });

    let unpinned_mock =
        MockMacOsSystemOps::new_macos(tmp.path().join("h_unp"), tmp.path().join("s_unp"))
            .with_command_handler(valid_vng_handler.clone());
    let unpinned_adapter = MacOsPlatformAdapter::with_system_ops(Arc::new(unpinned_mock));
    let unpinned_disc = unpinned_adapter.discover_installation(Some(&app)).unwrap();
    assert!(
        !unpinned_disc.is_valid,
        "Default unpinned adapter must NOT claim VerifiedPublisher!"
    );
    assert!(unpinned_disc
        .signature_status
        .contains("UnverifiedPublisher"));

    // Positive Tier 5 (`VerifiedPublisherIdentity`): explicit pinned policy + `-R=...` + nested helpers verified.
    let pinned_mock =
        MockMacOsSystemOps::new_macos(tmp.path().join("h_pin"), tmp.path().join("s_pin"))
            .with_command_handler(valid_vng_handler);
    let pinned_adapter = MacOsPlatformAdapter::with_system_ops(Arc::new(pinned_mock))
        .with_pinned_publisher("com.vng.zalo", "99VNGTEAM1");
    let tier5_disc = pinned_adapter.discover_installation(Some(&app)).unwrap();
    assert!(tier5_disc.is_valid);
    assert_eq!(tier5_disc.support_status, SupportStatus::NotTested);
    assert!(tier5_disc.signature_status.starts_with("VerifiedPublisher"));
    assert!(tier5_disc.notes.iter().any(|n| n
        == &format!(
            "identity_tier={}",
            AppIdentityVerificationTier::VerifiedPublisherIdentity.as_str()
        )));
    assert!(tier5_disc
        .notes
        .iter()
        .any(|n| n.contains("com.apple.quarantine")));
    assert!(pinned_adapter
        .capabilities()
        .diagnostics
        .iter()
        .any(|d| d.contains(LIVE_ZALO_STATUS_MARKER)));
}

#[test]
fn p0_2_twelve_surface_data_isolation_ct03_and_external_db_path_guard() {
    let tmp = tempfile::tempdir().unwrap();
    let sim_home = tmp.path().join("sim_home");
    let primary_zalo = macos_zalo_data_dir(&sim_home);
    fs::create_dir_all(&primary_zalo).unwrap();
    fs::write(
        primary_zalo.join("primary-sentinel.txt"),
        "PRIMARY_UNTOUCHED",
    )
    .unwrap();

    let store = JsonProfileStore::new(tmp.path().join("dim_data")).unwrap();
    store.ensure_initialized().unwrap();
    let adapter = MacOsPlatformAdapter::for_fixture_host(sim_home.clone(), tmp.path().join("stmp"));

    let p1 = store.create_profile("Đức - Công việc", "S1").unwrap();
    let p2 = store.create_profile("Tài khoản Cá nhân", "S2").unwrap();
    let p3 = store.create_profile("Hỗ trợ Khách hàng", "S3").unwrap();
    let d1 = store.profile_dir_for(&p1).unwrap();
    let d2 = store.profile_dir_for(&p2).unwrap();
    let d3 = store.profile_dir_for(&p3).unwrap();

    // CT-03 contract: default layout creates ZERO symlinks/junctions (`Downloads` & `Keychains` isolated!).
    adapter.ensure_profile_layout(&p1, &d1).unwrap();
    assert_eq!(adapter.unlink_profile_junctions(&d1).unwrap(), 0);

    let target = LaunchTarget::NodeFixture {
        node_path: None,
        script_path: locate_fixture_script(),
        extra_args: vec!["--spawn-helper".to_owned()],
    };

    let rt1 = adapter.launch(&target, &p1, &d1).unwrap();
    let rt2 = adapter.launch(&target, &p2, &d2).unwrap();
    let rt3 = adapter.launch(&target, &p3, &d3).unwrap();
    assert!(rt1.running && rt2.running && rt3.running);

    // Verify all 12 isolated surfaces across all 3 live profiles (`P0-2`).
    let report = verify_12_surface_runtime_isolation(
        &**adapter.system_ops(),
        &[
            (p1.id.clone(), d1.clone()),
            (p2.id.clone(), d2.clone()),
            (p3.id.clone(), d3.clone()),
        ],
        Some(&sim_home),
    )
    .unwrap();
    assert_eq!(
        report["verified_surfaces_count"].as_u64(),
        Some(MACOS_12_ISOLATED_SURFACES.len() as u64)
    );
    assert_eq!(report["distinct_tmpdirs"].as_u64(), Some(3));
    assert_eq!(report["distinct_caches"].as_u64(), Some(3));
    assert_eq!(report["primary_zalo_data_untouched"].as_bool(), Some(true));

    // Closing Profile 1 leaves Profile 2 and Profile 3 intact; restarting Profile 1 preserves Profile 1 data.
    let _ = adapter.stop(&rt1, &[], 3_000).unwrap();
    assert!(!adapter.inspect_profile(&p1, &d1).unwrap().running);
    assert!(adapter.inspect_profile(&p2, &d2).unwrap().running);
    assert!(adapter.inspect_profile(&p3, &d3).unwrap().running);

    let rt1_re = adapter.launch(&target, &p1, &d1).unwrap();
    assert!(rt1_re.running);
    assert_eq!(rt1_re.detected_zalo_user_id, rt1.detected_zalo_user_id);

    // Primary ZaloData untouched throughout
    assert_eq!(
        fs::read_to_string(primary_zalo.join("primary-sentinel.txt")).unwrap(),
        "PRIMARY_UNTOUCHED"
    );
    assert!(!primary_zalo.join(ZALO_PROFILE_ID_FILE).exists());

    // F-CORE-01 guard: config.json `db_path.location` pointing to primary ZaloData or sibling profile rejected.
    fs::write(
        rt1_re.zalo_data_dir.join("config.json"),
        serde_json::json!({ "db_path": { "location": primary_zalo.to_string_lossy() } })
            .to_string(),
    )
    .unwrap();
    assert_eq!(
        verify_external_db_path_isolated(&rt1_re.zalo_data_dir, &d1, Some(&sim_home))
            .unwrap_err()
            .code(),
        "PROTECTED_PRIMARY_PATH"
    );
    fs::write(
        rt1_re.zalo_data_dir.join("config.json"),
        serde_json::json!({ "db_path": { "location": d2.join("CustomDb").to_string_lossy() } })
            .to_string(),
    )
    .unwrap();
    assert_eq!(
        verify_external_db_path_isolated(&rt1_re.zalo_data_dir, &d1, Some(&sim_home))
            .unwrap_err()
            .code(),
        "PATH_TRAVERSAL"
    );
    let _ = fs::remove_file(rt1_re.zalo_data_dir.join("config.json"));

    let _ = adapter.stop(&rt1_re, &[], 3_000).unwrap();
    let _ = adapter
        .stop(&adapter.inspect_profile(&p2, &d2).unwrap(), &[], 3_000)
        .unwrap();
    let _ = adapter
        .stop(&adapter.inspect_profile(&p3, &d3).unwrap(), &[], 3_000)
        .unwrap();
}

#[test]
fn p0_3_all_eight_process_ownership_pid_reuse_and_primary_protection_cases() {
    let sample_ps = "\
  4100     1   501 Ss   184320 Mon Nov 17 09:05:10 2026 /Applications/Zalo.app/Contents/MacOS/Zalo --user-data-dir=/Users/alice/Library/Application Support/MultiZaloDesktop/Profiles/work_11112222/Library/Application Support/ZaloData
  4101  4100   501 S     92160 Mon Nov 17 09:05:11 2026 /Applications/Zalo.app/Contents/Frameworks/Zalo Helper (Renderer).app/Contents/MacOS/Zalo Helper (Renderer) --type=renderer
  4102  4100   501 Z         0 Mon Nov 17 09:05:12 2026 (Zalo Helper) <defunct>
";
    let entries = parse_ps_table(sample_ps);
    assert_eq!(entries.len(), 2);
    assert_eq!(
        entries[0].start_token.as_deref(),
        Some("Mon Nov 17 09:05:10 2026")
    );
    assert_eq!(entries[1].ppid, 4100);

    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("Users").join("alice");
    let primary_zalo = macos_zalo_data_dir(&home);
    fs::create_dir_all(&primary_zalo).unwrap();
    fs::write(primary_zalo.join("lockfile"), r#"{"pid": 5000}"#).unwrap();

    let prof_recycled = make_profile("1234abcd", "Recycled Check", "recycled_1234abcd");
    let pdir_recycled = tmp
        .path()
        .join("Profiles")
        .join(&prof_recycled.directory_name);
    let prof_restart = make_profile("5678ef01", "Restart Reattach", "restart_5678ef01");
    let pdir_restart = tmp
        .path()
        .join("Profiles")
        .join(&prof_restart.directory_name);
    let zdata_restart = macos_zalo_data_dir(&pdir_restart);

    let mock = Arc::new(MockMacOsSystemOps::new_macos(
        home.clone(),
        tmp.path().join("stmp"),
    ));
    // Process table containing:
    // - 5000 + 5001: Primary Zalo running before manager starts (Cases 3 & 6)
    // - 5500: External unmanaged Zalo.app launched outside manager (Case 2)
    // - 6000: Foreign Safari PID that recycled stale `.runtime-pid` (Case 5)
    // - 6100: Same-UID, same-binary `Zalo.app` that recycled `prof_recycled`'s old PID with NEW `lstart` (Case 1)
    // - 6200 + 6201 + 6299 (ppid=1 orphan): Managed `prof_restart` surviving manager restart (Cases 4, 7, 8)
    mock.set_processes(vec![
        ProcessEntry {
            pid: 5000,
            ppid: 1,
            uid: 501,
            rss_kib: 150_000,
            start_token: Some("Mon Nov 17 07:00:00 2026".to_owned()),
            executable: "/Applications/Zalo.app/Contents/MacOS/Zalo".to_owned(),
            args: "/Applications/Zalo.app/Contents/MacOS/Zalo".to_owned(),
        },
        ProcessEntry {
            pid: 5001,
            ppid: 5000,
            uid: 501,
            rss_kib: 80_000,
            start_token: Some("Mon Nov 17 07:00:01 2026".to_owned()),
            executable: "Zalo Helper (Renderer)".to_owned(),
            args: "/Applications/Zalo.app/Contents/Frameworks/Zalo Helper (Renderer).app/Contents/MacOS/Zalo Helper (Renderer) --type=renderer".to_owned(),
        },
        ProcessEntry {
            pid: 5500,
            ppid: 1,
            uid: 501,
            rss_kib: 110_000,
            start_token: Some("Mon Nov 17 08:00:00 2026".to_owned()),
            executable: "/Users/alice/Downloads/Zalo.app/Contents/MacOS/Zalo".to_owned(),
            args: "/Users/alice/Downloads/Zalo.app/Contents/MacOS/Zalo".to_owned(),
        },
        ProcessEntry {
            pid: 6000,
            ppid: 1,
            uid: 501,
            rss_kib: 90_000,
            start_token: Some("Mon Nov 17 09:00:00 2026".to_owned()),
            executable: "/Applications/Safari.app/Contents/MacOS/Safari".to_owned(),
            args: "/Applications/Safari.app/Contents/MacOS/Safari".to_owned(),
        },
        ProcessEntry {
            pid: 6100,
            ppid: 1,
            uid: 501,
            rss_kib: 120_000,
            start_token: Some("Mon Nov 17 11:30:00 2026".to_owned()),
            executable: "/Applications/Zalo.app/Contents/MacOS/Zalo".to_owned(),
            args: format!(
                "/Applications/Zalo.app/Contents/MacOS/Zalo --user-data-dir={}",
                macos_zalo_data_dir(&pdir_recycled).display()
            ),
        },
        ProcessEntry {
            pid: 6200,
            ppid: 1,
            uid: 501,
            rss_kib: 130_000,
            start_token: Some("Mon Nov 17 10:15:00 2026".to_owned()),
            executable: "/Applications/Zalo.app/Contents/MacOS/Zalo".to_owned(),
            args: format!(
                "/Applications/Zalo.app/Contents/MacOS/Zalo --user-data-dir={}",
                zdata_restart.display()
            ),
        },
        ProcessEntry {
            pid: 6201,
            ppid: 6200,
            uid: 501,
            rss_kib: 60_000,
            start_token: Some("Mon Nov 17 10:15:01 2026".to_owned()),
            executable: "Zalo Helper".to_owned(),
            args: "Zalo Helper --type=gpu-process".to_owned(),
        },
        ProcessEntry {
            pid: 6299,
            ppid: 1,
            uid: 501,
            rss_kib: 40_000,
            start_token: Some("Mon Nov 17 10:15:02 2026".to_owned()),
            executable: "Zalo Helper (Renderer)".to_owned(),
            args: format!(
                "Zalo Helper (Renderer) --type=renderer --user-data-dir={}",
                zdata_restart.display()
            ),
        },
    ]);

    let ops_dyn: Arc<dyn MacOsSystemOps> = mock.clone();
    let adapter = MacOsPlatformAdapter::with_system_ops(ops_dyn).with_signature_enforcement(false);

    // Cases 2, 3, 6: Primary + unmanaged external Zalo.app are in `protected_pids`, while managed 6200 is NOT!
    let primary = adapter.inspect_primary().unwrap();
    assert!(primary.running);
    assert!(primary.protected_pids.contains(&5000));
    assert!(primary.protected_pids.contains(&5001));
    assert!(primary.protected_pids.contains(&5500));
    assert!(!primary.protected_pids.contains(&6200));

    // Case 1: Same-UID, same-binary `Zalo.app` PID 6100 with mismatched `lstart` MUST be rejected!
    adapter
        .ensure_profile_layout(&prof_recycled, &pdir_recycled)
        .unwrap();
    let zdata_recycled = macos_zalo_data_dir(&pdir_recycled);
    write_runtime_launch_record(
        &pdir_recycled,
        &RuntimeLaunchRecord {
            schema_version: 1,
            pid: 6100,
            process_start_token: Some("Mon Nov 17 08:00:00 2026".to_owned()), // OLD start time!
            profile_id: prof_recycled.id.clone(),
            profile_dir: pdir_recycled.clone(),
            zalo_data_dir: zdata_recycled.clone(),
            executable_path: "/Applications/Zalo.app".to_owned(),
            launched_at_utc: "2026-11-17T08:00:00Z".to_owned(),
        },
    )
    .unwrap();
    fs::write(zdata_recycled.join("lockfile"), r#"{"pid": 6100}"#).unwrap();

    let st_recycled = adapter
        .inspect_profile(&prof_recycled, &pdir_recycled)
        .unwrap();
    assert!(
        !st_recycled.running && st_recycled.stale_lock_detected,
        "Same-binary Zalo PID reuse with mismatched lstart must NEVER be treated as owned!"
    );
    assert_eq!(
        adapter
            .stop(&st_recycled, &primary.protected_pids, 300)
            .unwrap(),
        0
    );
    assert!(mock.terminated_pids().is_empty());

    // Case 5: Stale `.runtime-pid` pointing to foreign Safari 6000 rejected; unmanaged 5500 rejected!
    fs::remove_file(pdir_recycled.join(".runtime-launch.json")).unwrap();
    fs::write(pdir_recycled.join(".runtime-pid"), "6000\n").unwrap();
    fs::write(zdata_recycled.join("lockfile"), r#"{"pid": 5500}"#).unwrap();
    let st_foreign = adapter
        .inspect_profile(&prof_recycled, &pdir_recycled)
        .unwrap();
    assert!(!st_foreign.running && st_foreign.stale_lock_detected);
    let _ = adapter
        .recover_stale_state(&prof_recycled, &pdir_recycled, &st_foreign)
        .unwrap();

    // Case 6: Stopping primary PID 5001 fails with PROTECTED_PRIMARY_PROCESS!
    let mut forged = st_foreign;
    forged.running = true;
    forged.main_pid = Some(5001);
    forged.process_ids = vec![5001];
    assert_eq!(
        adapter
            .stop(&forged, &primary.protected_pids, 300)
            .unwrap_err()
            .code(),
        "PROTECTED_PRIMARY_PROCESS"
    );
    assert_eq!(
        ensure_not_macos_primary_path(&primary_zalo, Some(&home))
            .unwrap_err()
            .code(),
        "PROTECTED_PRIMARY_PATH"
    );

    // Cases 4, 7, 8: Manager restart re-attaches cleanly to `prof_restart` (6200 + 6201 + orphan 6299) and shuts down full tree!
    adapter
        .ensure_profile_layout(&prof_restart, &pdir_restart)
        .unwrap();
    write_runtime_launch_record(
        &pdir_restart,
        &RuntimeLaunchRecord {
            schema_version: 1,
            pid: 6200,
            process_start_token: Some("Mon Nov 17 10:15:00 2026".to_owned()),
            profile_id: prof_restart.id.clone(),
            profile_dir: pdir_restart.clone(),
            zalo_data_dir: zdata_restart.clone(),
            executable_path: "/Applications/Zalo.app".to_owned(),
            launched_at_utc: "2026-11-17T10:15:00Z".to_owned(),
        },
    )
    .unwrap();
    fs::write(zdata_restart.join("lockfile"), r#"{"pid": 6200}"#).unwrap();

    // Fresh adapter instance simulating manager restart
    let fresh_adapter =
        MacOsPlatformAdapter::with_system_ops(mock.clone()).with_signature_enforcement(false);
    let st_reattached = fresh_adapter
        .inspect_profile(&prof_restart, &pdir_restart)
        .unwrap();
    assert!(st_reattached.running);
    assert_eq!(st_reattached.main_pid, Some(6200));
    assert_eq!(st_reattached.process_ids, vec![6200, 6201, 6299]);
    assert_eq!(
        st_reattached.working_set_bytes,
        (130_000 + 60_000 + 40_000) * 1024
    );

    let terminated = fresh_adapter
        .stop(&st_reattached, &primary.protected_pids, 400)
        .unwrap();
    assert_eq!(terminated, 3);
    let sig_pids = mock.terminated_pids();
    assert!(sig_pids.contains(&6200) && sig_pids.contains(&6201) && sig_pids.contains(&6299));
    assert!(!sig_pids.contains(&5000) && !sig_pids.contains(&5500) && !sig_pids.contains(&6000));
}

#[test]
fn p0_4_defects_01_to_06_xp_mac_03_static_and_runtime_contracts() {
    // 1. XP-MAC-03 static source contract verification on `src/adapter.rs` & `src/layout.rs`.
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let adapter_src = fs::read_to_string(src_dir.join("adapter.rs")).unwrap();
    let layout_src = fs::read_to_string(src_dir.join("layout.rs")).unwrap();
    assert!(
        !adapter_src.contains("singleton_lock.exists()"),
        "DEFECT-05: must use symlink_metadata, never singleton_lock.exists()"
    );
    let inspect_start = adapter_src.find("fn inspect_profile").unwrap();
    let inspect_end = adapter_src[inspect_start..]
        .find("impl InstanceLauncher")
        .map(|off| inspect_start + off)
        .unwrap();
    let inspect_slice = &adapter_src[inspect_start..inspect_end];
    assert!(
        inspect_slice.contains("runtime_pid"),
        "DEFECT-03: inspect_profile must read runtime_pid"
    );
    assert!(
        adapter_src.contains("--user-id=") && layout_src.contains("derive_fixture_user_id"),
        "DEFECT-02: deterministic per-profile --user-id derivation required"
    );

    // 2. DEFECT-04 (`LC-08`): 8 threads concurrently calling `ensure_profile_layout` on the same profile.
    let tmp = tempfile::tempdir().unwrap();
    let adapter = MacOsPlatformAdapter::for_fixture_host(
        tmp.path().join("sim_home"),
        tmp.path().join("stmp"),
    )
    .with_launch_poll_ms(350);

    let shared_prof = make_profile("88776655", "Concurrent Layout", "conc_88776655");
    let shared_dir = tmp
        .path()
        .join("Profiles")
        .join(&shared_prof.directory_name);
    let barrier = Arc::new(Barrier::new(8));
    std::thread::scope(|s| {
        for _ in 0..8 {
            let a = adapter.clone();
            let p = shared_prof.clone();
            let d = shared_dir.clone();
            let b = Arc::clone(&barrier);
            s.spawn(move || {
                b.wait();
                a.ensure_profile_layout(&p, &d).unwrap();
            });
        }
    });
    assert!(shared_dir.join(MACOS_PROFILE_META_FILE).is_file());

    // 3. DEFECT-02 + DEFECT-03 (`XP-MAC-03` & `LC-07` runtime): slow pre-lock startup > poll_ms AND empty extra_args!
    let store = JsonProfileStore::new(tmp.path().join("dim_data")).unwrap();
    store.ensure_initialized().unwrap();
    let p1 = store.create_profile("Slow Lock P1", "").unwrap();
    let p2 = store.create_profile("Auto UID P2", "").unwrap();
    let d1 = store.profile_dir_for(&p1).unwrap();
    let d2 = store.profile_dir_for(&p2).unwrap();

    let slow_target = LaunchTarget::NodeFixture {
        node_path: None,
        script_path: locate_fixture_script(),
        extra_args: vec!["--slow-startup-pre-lock-ms=900".to_owned()],
    };
    let default_target = LaunchTarget::NodeFixture {
        node_path: None,
        script_path: locate_fixture_script(),
        extra_args: Vec::new(),
    };

    let rt1 = adapter.launch(&slow_target, &p1, &d1).unwrap();
    assert!(
        rt1.running && rt1.main_pid.is_some(),
        "DEFECT-03: slow-starting child must report running=true immediately via .runtime-launch.json before SingletonLock!"
    );

    let rt2 = adapter.launch(&default_target, &p2, &d2).unwrap();
    assert!(rt2.running);

    std::thread::sleep(Duration::from_millis(750));
    let st1 = adapter.inspect_profile(&p1, &d1).unwrap();
    let st2 = adapter.inspect_profile(&p2, &d2).unwrap();
    assert!(st1.detected_zalo_user_id.is_some() && st2.detected_zalo_user_id.is_some());
    assert_ne!(
        st1.detected_zalo_user_id, st2.detected_zalo_user_id,
        "DEFECT-02: distinct profiles with empty extra_args must never collide on user_id=100001!"
    );

    let _ = adapter.stop(&st1, &[], 3_000).unwrap();
    let _ = adapter.stop(&st2, &[], 3_000).unwrap();

    // 4. DEFECT-05 (`LC-06`): dangling SingletonLock symlink detected via `symlink_metadata` & cleaned safely.
    #[cfg(unix)]
    {
        let zdata = macos_zalo_data_dir(&d1);
        std::os::unix::fs::symlink("mac-host.local-999999", zdata.join("SingletonLock")).unwrap();
        let dangling_state = adapter.inspect_profile(&p1, &d1).unwrap();
        assert!(!dangling_state.running && dangling_state.stale_lock_detected);
        let rec = adapter
            .recover_stale_state(&p1, &d1, &dangling_state)
            .unwrap();
        assert!(rec.recovered);
        assert!(fs::symlink_metadata(zdata.join("SingletonLock")).is_err());
    }
}

#[test]
fn p1_1_sun_len_short_tmpdir_symlink_squatting_and_foreign_slot_hardening() {
    assert_eq!(DARWIN_SUN_PATH_MAX, 104);
    assert_eq!(MACOS_SAFE_TMPDIR_MAX_BYTES, 49);

    let tmp = tempfile::tempdir().unwrap();
    let short_root = tmp.path().join("st");
    fs::create_dir_all(&short_root).unwrap();
    let mut mock = MockMacOsSystemOps::new_macos(tmp.path().join("h"), short_root.clone());
    mock.short_tmp_root = short_root.clone();

    let long_profile_dir = tmp
        .path()
        .join("Library")
        .join("Application Support")
        .join("MultiZaloDesktop")
        .join("Profiles")
        .join("cong-viec-doanh-nghiep-chi-nhanh-mien-nam_a1b2c3d4");
    assert!(long_profile_dir.join("tmp").to_string_lossy().len() > MACOS_SAFE_TMPDIR_MAX_BYTES);

    let (eff_tmp, used_fallback) =
        resolve_effective_tmpdir(&mock, "a1b2c3d4", &long_profile_dir).unwrap();
    assert!(used_fallback);
    assert!(eff_tmp.ends_with(Path::new("dz-501").join("a1b2c3d4")));

    // Verify Darwin equivalent `/tmp/dz-501/a1b2c3d4/.org.chromium.Chromium.123456/SingletonSocket` fits well inside 103B!
    let darwin_equiv = "/tmp/dz-501/a1b2c3d4/.org.chromium.Chromium.123456/SingletonSocket";
    assert!(darwin_equiv.len() <= 103);

    // Reject path traversal / non-ASCII `profile_id` in short tmp slot
    assert_eq!(
        resolve_effective_tmpdir(&mock, "../escape", &long_profile_dir)
            .unwrap_err()
            .code(),
        "PATH_TRAVERSAL"
    );
    assert_eq!(
        resolve_effective_tmpdir(&mock, "việt-id", &long_profile_dir)
            .unwrap_err()
            .code(),
        "PATH_TRAVERSAL"
    );

    // Reject insecure world-writable `0o777` mode on short tmp slot
    fs::write(eff_tmp.join(SIMULATED_POSIX_MODE_FILE), "0777\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&eff_tmp, fs::Permissions::from_mode(0o777)).unwrap();
    }
    assert!(resolve_effective_tmpdir(&mock, "a1b2c3d4", &long_profile_dir).is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&eff_tmp, fs::Permissions::from_mode(0o700));
    }
    let _ = fs::remove_file(eff_tmp.join(SIMULATED_POSIX_MODE_FILE));

    // Reject pre-squatted symlink/junction on `<short_tmp>/dz-501/squatted1`!
    let outside = tmp.path().join("attacker_dir");
    fs::create_dir_all(&outside).unwrap();
    let squatted_slot = short_root.join("dz-501").join("squatted1");
    #[cfg(windows)]
    {
        let status = std::process::Command::new("cmd.exe")
            .args([
                "/c",
                "mklink",
                "/J",
                &squatted_slot.to_string_lossy(),
                &outside.to_string_lossy(),
            ])
            .status()
            .unwrap();
        assert!(status.success());
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside, &squatted_slot).unwrap();
    }
    assert_eq!(
        resolve_effective_tmpdir(&mock, "squatted1", &long_profile_dir)
            .unwrap_err()
            .code(),
        "PATH_TRAVERSAL"
    );

    // Cleanup removes ONLY `<short_tmp>/dz-501/a1b2c3d4` and NEVER touches simulated global ZaloCall socket!
    let global_call_socket = short_root.join("socketzalorecv2021");
    fs::write(&global_call_socket, "GLOBAL_ZALOCALL_SOCKET").unwrap();
    let cleaned = cleanup_profile_short_tmpdir(&mock, "a1b2c3d4").unwrap();
    assert_eq!(cleaned, 1);
    assert!(!eff_tmp.exists());
    assert_eq!(
        fs::read_to_string(&global_call_socket).unwrap(),
        "GLOBAL_ZALOCALL_SOCKET"
    );
}

#[test]
fn p1_2_all_seven_graceful_shutdown_timeout_hung_and_orphan_helper_scenarios() {
    let tmp = tempfile::tempdir().unwrap();
    let store = JsonProfileStore::new(tmp.path().join("dim_data")).unwrap();
    store.ensure_initialized().unwrap();
    let adapter = MacOsPlatformAdapter::for_fixture_host(
        tmp.path().join("sim_home"),
        tmp.path().join("stmp"),
    );
    let script = locate_fixture_script();

    // 1 & 2. Cooperative delayed exit (`--delayed-exit-ms=160`) exits cleanly inside graceful window before SIGTERM!
    let p_delayed = store.create_profile("Delayed Exit", "").unwrap();
    let d_delayed = store.profile_dir_for(&p_delayed).unwrap();
    let rt_delayed = adapter
        .launch(
            &LaunchTarget::NodeFixture {
                node_path: None,
                script_path: script.clone(),
                extra_args: vec!["--delayed-exit-ms=160".to_owned()],
            },
            &p_delayed,
            &d_delayed,
        )
        .unwrap();
    let stopped_delayed = adapter.stop(&rt_delayed, &[], 1_800).unwrap();
    assert!(stopped_delayed >= 1);
    let ls_delayed: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(rt_delayed.zalo_data_dir.join("launch-state.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        ls_delayed["exit_reason"].as_str(),
        Some("graceful_quit_signal"),
        "Delayed cooperative child must exit via graceful_quit_signal without premature SIGTERM!"
    );

    // 3. Hung process (`--hang-on-quit`) fails closed with `PROCESS_ERROR` and preserves `.runtime-launch.json`!
    let p_hung = store.create_profile("Hung Process", "").unwrap();
    let d_hung = store.profile_dir_for(&p_hung).unwrap();
    let rt_hung = adapter
        .launch(
            &LaunchTarget::NodeFixture {
                node_path: None,
                script_path: script.clone(),
                extra_args: vec!["--hang-on-quit".to_owned(), "--stay-ms=1400".to_owned()],
            },
            &p_hung,
            &d_hung,
        )
        .unwrap();
    let hung_err = adapter.stop(&rt_hung, &[], 350).unwrap_err();
    assert_eq!(hung_err.code(), "PROCESS_ERROR");
    assert!(
        read_runtime_launch_record(&d_hung).is_some(),
        "Hung process must keep .runtime-launch.json intact so profile is never falsely marked stopped!"
    );
    std::thread::sleep(Duration::from_millis(1_300));
    assert!(!adapter.inspect_profile(&p_hung, &d_hung).unwrap().running);
    let _ = adapter
        .recover_stale_state(
            &p_hung,
            &d_hung,
            &adapter.inspect_profile(&p_hung, &d_hung).unwrap(),
        )
        .unwrap();

    // 4 & 7. Crashed / stale process -> `stop` returns `Ok(0)` without signaling.
    let p_crash = store.create_profile("Crash Stop", "").unwrap();
    let d_crash = store.profile_dir_for(&p_crash).unwrap();
    let _ = adapter
        .launch(
            &LaunchTarget::NodeFixture {
                node_path: None,
                script_path: script.clone(),
                extra_args: vec!["--crash".to_owned(), "--crash-delay-ms=60".to_owned()],
            },
            &p_crash,
            &d_crash,
        )
        .unwrap();
    std::thread::sleep(Duration::from_millis(260));
    let st_crashed = adapter.inspect_profile(&p_crash, &d_crash).unwrap();
    assert!(!st_crashed.running && st_crashed.stale_lock_detected);
    assert_eq!(adapter.stop(&st_crashed, &[], 400).unwrap(), 0);

    // 5 & 6. Orphan helper (`--orphan-helper`, `ppid == 1`) + fresh adapter (manager restart) reaps orphan via SIGTERM!
    let p_orphan = store.create_profile("Orphan Helper", "").unwrap();
    let d_orphan = store.profile_dir_for(&p_orphan).unwrap();
    let rt_orphan = adapter
        .launch(
            &LaunchTarget::NodeFixture {
                node_path: None,
                script_path: script,
                extra_args: vec!["--orphan-helper".to_owned()],
            },
            &p_orphan,
            &d_orphan,
        )
        .unwrap();
    let ls_orphan: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(rt_orphan.zalo_data_dir.join("launch-state.json")).unwrap(),
    )
    .unwrap();
    let orphan_pid = u32::try_from(ls_orphan["orphan_helper_pid"].as_u64().unwrap()).unwrap();
    assert!(adapter.system_ops().is_pid_alive(orphan_pid));

    // Fresh adapter instance simulating manager restart before stop
    let restarted_adapter = MacOsPlatformAdapter::for_fixture_host(
        tmp.path().join("sim_home"),
        tmp.path().join("stmp"),
    );
    let st_before_stop = restarted_adapter
        .inspect_profile(&p_orphan, &d_orphan)
        .unwrap();
    assert!(st_before_stop.running);
    assert!(st_before_stop.process_ids.contains(&orphan_pid));

    let reaped = restarted_adapter.stop(&st_before_stop, &[], 2_500).unwrap();
    assert!(reaped >= 2);
    assert!(
        !restarted_adapter.system_ops().is_pid_alive(orphan_pid),
        "Orphaned ppid=1 helper must be reaped during stop()!"
    );
}

#[test]
fn p1_3_all_four_macos_14_window_activation_and_zero_tcc_cases() {
    // Zero `System Events` AppleEvents in production window/focus scripts (`P1-3`).
    assert!(!APPLESCRIPT_ACTIVATE_BY_PID.contains("System Events"));
    assert!(!JXA_QUERY_WINDOWS_SCRIPT.contains("System Events"));

    let jxa_sample =
        r#"[{"window_id":9142,"owner_pid":4100,"title":"Zalo — Công việc","is_on_screen":true}]"#;
    let wins = parse_jxa_windows_json(jxa_sample);
    assert_eq!(wins.len(), 1);
    assert_eq!(wins[0].window_id, 9142);

    let tmp = tempfile::tempdir().unwrap();
    let prof = make_profile("4100aaaa", "Work Mac", "work-mac_4100aaaa");
    let pdir = tmp.path().join("p1");
    let zdata = macos_zalo_data_dir(&pdir);
    let target = LaunchTarget::OfficialZalo {
        custom_install_root: None,
    };

    // Case 2: `activated=true` & `active=true` + window verified -> `Ok(())`.
    let mock_ok = Arc::new(MockMacOsSystemOps::new_macos(
        tmp.path().join("home"),
        tmp.path().join("stmp"),
    ));
    mock_ok.set_processes(vec![ProcessEntry {
        pid: 4100,
        ppid: 1,
        uid: 501,
        rss_kib: 100_000,
        start_token: Some("Mon Nov 17 12:00:00 2026".to_owned()),
        executable: "/Applications/Zalo.app/Contents/MacOS/Zalo".to_owned(),
        args: format!(
            "/Applications/Zalo.app/Contents/MacOS/Zalo --user-data-dir={}",
            zdata.display()
        ),
    }]);
    mock_ok.set_windows(vec![WindowInfo {
        window_id: 9142,
        owner_pid: 4100,
        title: Some("Zalo — Công việc".to_owned()),
        is_on_screen: true,
    }]);
    let adapter_ok =
        MacOsPlatformAdapter::with_system_ops(mock_ok.clone()).with_signature_enforcement(false);
    adapter_ok.ensure_profile_layout(&prof, &pdir).unwrap();
    let rt_ok = adapter_ok.inspect_profile(&prof, &pdir).unwrap();
    assert_eq!(rt_ok.window_handle, Some(9142));
    adapter_ok
        .focus_or_restore(&target, &prof, &pdir, &rt_ok)
        .unwrap();
    assert!(mock_ok
        .commands()
        .iter()
        .any(|c| c.program.to_string_lossy().ends_with("osascript")
            && c.args.contains(&"4100".to_owned())));

    // Case 1: macOS 14+ cooperative activation declined (`activated=false active=false`) -> `PROCESS_ERROR`!
    let mock_declined = Arc::new(
        MockMacOsSystemOps::new_macos(tmp.path().join("home2"), tmp.path().join("stmp2"))
            .with_command_handler(Arc::new(|prog, _, _| {
                if prog.to_string_lossy().ends_with("osascript") {
                    return Ok(CommandOutput {
                        status_code: Some(0),
                        stdout: b"status=ok activated=false active=false\n".to_vec(),
                        stderr: Vec::new(),
                    });
                }
                Ok(CommandOutput {
                    status_code: Some(0),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            })),
    );
    mock_declined.set_processes(mock_ok.list_processes().unwrap());
    let adapter_declined =
        MacOsPlatformAdapter::with_system_ops(mock_declined).with_signature_enforcement(false);
    let declined_err = adapter_declined
        .focus_or_restore(&target, &prof, &pdir, &rt_ok)
        .unwrap_err();
    assert_eq!(declined_err.code(), "PROCESS_ERROR");

    // Case 3: Unowned live foreign PID in `state.main_pid` -> rejected with `PROCESS_ERROR` BEFORE calling `osascript`!
    let mock_foreign = Arc::new(MockMacOsSystemOps::new_macos(
        tmp.path().join("home3"),
        tmp.path().join("stmp3"),
    ));
    mock_foreign.set_processes(vec![ProcessEntry {
        pid: 4100,
        ppid: 1,
        uid: 501,
        rss_kib: 50_000,
        start_token: Some("Mon Nov 17 12:30:00 2026".to_owned()),
        executable: "/Applications/Safari.app/Contents/MacOS/Safari".to_owned(),
        args: "/Applications/Safari.app/Contents/MacOS/Safari".to_owned(),
    }]);
    let adapter_foreign = MacOsPlatformAdapter::with_system_ops(mock_foreign.clone())
        .with_signature_enforcement(false);
    let foreign_err = adapter_foreign
        .focus_or_restore(&target, &prof, &pdir, &rt_ok)
        .unwrap_err();
    assert_eq!(foreign_err.code(), "PROCESS_ERROR");
    assert!(
        mock_foreign.commands().is_empty(),
        "Unowned PID must NEVER reach osascript!"
    );

    // Case 4: Missing/dead PID -> `NOT_RUNNING`.
    let mock_dead = Arc::new(MockMacOsSystemOps::new_macos(
        tmp.path().join("home4"),
        tmp.path().join("stmp4"),
    ));
    let adapter_dead =
        MacOsPlatformAdapter::with_system_ops(mock_dead).with_signature_enforcement(false);
    let dead_err = adapter_dead
        .focus_or_restore(&target, &prof, &pdir, &rt_ok)
        .unwrap_err();
    assert_eq!(dead_err.code(), "NOT_RUNNING");
}

#[test]
fn scenario_7_concurrent_duplicate_launch_guard_blocks_parallel_race() {
    let tmp = tempfile::tempdir().unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let b_for_cmd = Arc::clone(&barrier);

    let app = create_synthetic_zalo_app(tmp.path(), &make_macho_universal2_fat(), false);
    let mock = Arc::new(
        MockMacOsSystemOps::new_macos(tmp.path().join("home"), tmp.path().join("stmp"))
            .with_command_handler(Arc::new(move |prog, _args, _| {
                if prog.to_string_lossy().ends_with("/open") {
                    let _ = b_for_cmd.wait();
                    std::thread::sleep(Duration::from_millis(80));
                }
                Ok(CommandOutput {
                    status_code: Some(0),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            })),
    );

    let adapter = MacOsPlatformAdapter::with_system_ops(mock)
        .with_signature_enforcement(false)
        .with_launch_poll_ms(40);
    let prof = make_profile("77778888", "Race Guard", "race-guard_77778888");
    let pdir = tmp.path().join("Profiles").join(&prof.directory_name);
    adapter.ensure_profile_layout(&prof, &pdir).unwrap();

    let target = LaunchTarget::OfficialZalo {
        custom_install_root: Some(app),
    };

    let (r1, r2) = std::thread::scope(|s| {
        let a1 = adapter.clone();
        let p1 = prof.clone();
        let d1 = pdir.clone();
        let t1 = target.clone();
        let h1 = s.spawn(move || a1.launch(&t1, &p1, &d1));

        let _ = barrier.wait();
        let r2 = adapter.launch(&target, &prof, &pdir);
        let r1 = h1.join().unwrap();
        (r1, r2)
    });

    assert!(r1.is_ok());
    let dup_err = r2.unwrap_err();
    assert_eq!(dup_err.code(), "PROCESS_ERROR");
    assert!(dup_err
        .to_string()
        .contains("duplicate concurrent operation"));
}

#[test]
fn scenario_10_11_12_13_symlink_unlink_unicode_nfc_nfd_path_max_and_permissions() {
    let nfc_vn = "/Users/Đức Nguyễn/Cửa hàng Điện tử/ZaloData";
    let nfd_vn = "/Users/Đu\u{031B}\u{0301}c Nguye\u{0302}\u{0303}n/Cu\u{031B}\u{0309}a ha\u{0300}ng Đie\u{0323}\u{0302}n tu\u{031B}\u{0309}/ZaloData";
    assert_eq!(
        normalize_macos_unicode_key(nfc_vn),
        normalize_macos_unicode_key(nfd_vn)
    );
    assert_eq!(
        normalize_macos_path_key(Path::new(nfc_vn)),
        normalize_macos_path_key(Path::new(nfd_vn))
    );

    let over_max = PathBuf::from(format!("/Users/{}", "a".repeat(MACOS_PATH_MAX)));
    assert_eq!(
        validate_macos_path(&over_max).unwrap_err().code(),
        "PATH_TRAVERSAL"
    );

    let tmp = tempfile::tempdir().unwrap();
    let fixture_ops = Arc::new(HostFixtureMacOsOps::new(
        tmp.path().join("sim_home"),
        tmp.path().join("sim_stmp"),
    ));
    let adapter = MacOsPlatformAdapter::with_system_ops(fixture_ops)
        .with_signature_enforcement(false)
        .with_layout_options(MacOsLayoutOptions::default());

    let store = JsonProfileStore::new(tmp.path().join("dim_data")).unwrap();
    store.ensure_initialized().unwrap();
    assert_eq!(
        adapter.check_profiles_acl_safe(store.profiles_dir()),
        Some(true)
    );

    fs::write(
        store.profiles_dir().join(SIMULATED_POSIX_MODE_FILE),
        "0777\n",
    )
    .unwrap();
    assert_eq!(
        adapter.check_profiles_acl_safe(store.profiles_dir()),
        Some(false)
    );
    fs::remove_file(store.profiles_dir().join(SIMULATED_POSIX_MODE_FILE)).unwrap();

    let ro_parent = tmp.path().join("readonly_root");
    fs::create_dir_all(&ro_parent).unwrap();
    fs::write(ro_parent.join(SIMULATED_POSIX_MODE_FILE), "0500\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&ro_parent, fs::Permissions::from_mode(0o500)).unwrap();
    }
    let prof = make_profile("99887766", "Perm Fail", "perm-fail_99887766");
    let perm_res = adapter.ensure_profile_layout(&prof, &ro_parent.join("child_profile"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&ro_parent, fs::Permissions::from_mode(0o700));
    }
    if !cfg!(unix) || adapter.is_elevated() != Some(true) {
        assert!(perm_res.is_err());
    }

    // Safe symlink/junction unlinking before profile deletion preserves outside host files
    let p_link = store.create_profile("Symlink Safe", "").unwrap();
    let d_link = store.profile_dir_for(&p_link).unwrap();
    adapter.ensure_profile_layout(&p_link, &d_link).unwrap();

    let outside = tmp.path().join("outside_real_docs");
    fs::create_dir_all(&outside).unwrap();
    let sentinel = outside.join("keep_me.txt");
    fs::write(&sentinel, "HOST_DATA_INTACT").unwrap();

    let link_target = d_link.join("Downloads").join("SharedHostFolder");
    #[cfg(windows)]
    {
        let status = std::process::Command::new("cmd.exe")
            .args([
                "/c",
                "mklink",
                "/J",
                &link_target.to_string_lossy(),
                &outside.to_string_lossy(),
            ])
            .status()
            .unwrap();
        assert!(status.success());
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside, &link_target).unwrap();
    }

    store
        .delete_profile_with_guard(&p_link.id, true, false, &|dir| {
            adapter.unlink_profile_junctions(dir)
        })
        .unwrap();
    assert!(!d_link.exists());
    assert_eq!(fs::read_to_string(&sentinel).unwrap(), "HOST_DATA_INTACT");
}

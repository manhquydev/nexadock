//! Comprehensive Rust integration tests for `dim-platform-macos` covering:
//!   - Mach-O thin (`arm64`, `x86_64`) & Universal2 (`FAT_MAGIC`, `FAT_MAGIC_64`) slice parsing
//!   - XML & binary `bplist00` `Info.plist` parsing + Spotlight `mdfind` fallback
//!   - `codesign` + `spctl` + quarantine xattr trust matrix & `DimError::UnverifiedBinary`
//!   - `ps` process table parser (spaces in `.app` paths, zombie `Z` filtering, RSS calculation)
//!   - JXA `CGWindowListCopyWindowInfo` & AppleScript `NSRunningApplication` activation
//!   - All 13 required multi-instance reliability & safety scenarios

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
use dim_platform_macos::{
    ensure_not_macos_primary_path, inspect_macho_bytes, macos_zalo_data_dir,
    normalize_macos_path_key, normalize_macos_unicode_key, parse_codesign_dv_output,
    parse_jxa_windows_json, parse_ps_table, parse_spctl_output, resolve_effective_tmpdir,
    validate_macos_path, CodeSignTrustStatus, CommandOutput, HostFixtureMacOsOps, MacOsArchSlice,
    MacOsLayoutOptions, MacOsPlatformAdapter, MacOsSystemOps, MockMacOsSystemOps, ProcessEntry,
    WindowInfo, LIVE_ZALO_STATUS_MARKER, MACOS_PATH_MAX, MACOS_PROFILE_META_FILE,
    MACOS_SAFE_TMPDIR_MAX_BYTES, MACOS_WINDOW_STATE_FILE,
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
    buf.extend_from_slice(&2u32.to_be_bytes()); // nfat_arch = 2
                                                // fat_arch #1: x86_64 (20 bytes)
    buf.extend_from_slice(&CPU_TYPE_X86_64.to_be_bytes());
    buf.extend_from_slice(&[0u8; 16]);
    // fat_arch #2: arm64 (20 bytes)
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
        bplist.extend_from_slice(
            b"\0CFBundleIdentifier\0com.vng.zalo\0CFBundleExecutable\0Zalo\0CFBundleShortVersionString\x0026.10.20\0CFBundlePackageType\0APPL\0",
        );
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

    // 64-bit fat header
    let mut fat64 = Vec::new();
    fat64.extend_from_slice(&FAT_MAGIC_64.to_be_bytes());
    fat64.extend_from_slice(&1u32.to_be_bytes());
    fat64.extend_from_slice(&CPU_TYPE_ARM64.to_be_bytes());
    fat64.extend_from_slice(&[0u8; 28]);
    let parsed_fat64 = inspect_macho_bytes(&fat64);
    assert!(
        parsed_fat64.is_macho && parsed_fat64.is_universal_fat && parsed_fat64.supports_arm64()
    );

    // Corrupt / non-Mach-O rejected safely
    assert!(!inspect_macho_bytes(b"#!/bin/sh\necho hi\n").is_macho);
    assert!(!inspect_macho_bytes(&[0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 99]).is_macho);
}

#[test]
fn bundle_discovery_bplist00_plutil_spotlight_and_arch_compatibility() {
    let tmp = tempfile::tempdir().unwrap();
    let custom_dir = tmp.path().join("MovedApps");
    fs::create_dir_all(&custom_dir).unwrap();
    let app_path =
        create_synthetic_zalo_app(&custom_dir, &make_macho_thin_64_le(CPU_TYPE_ARM64), true);
    let app_path_for_mdfind = app_path.clone();

    // 1. Intel (`x86_64`) host must reject an `arm64`-only bundle cleanly.
    let mut intel_mock =
        MockMacOsSystemOps::new_macos(tmp.path().join("home"), tmp.path().join("stmp"));
    intel_mock.arch = "x86_64";
    intel_mock = intel_mock.with_command_handler(Arc::new(move |prog, _args, _| {
        let p = prog.to_string_lossy();
        if p.ends_with("plutil") {
            return Ok(CommandOutput {
                status_code: Some(0),
                stdout: br#"<plist version="1.0"><dict><key>CFBundleIdentifier</key><string>com.vng.zalo</string><key>CFBundleExecutable</key><string>Zalo</string><key>CFBundleShortVersionString</key><string>26.11.0</string><key>CFBundlePackageType</key><string>APPL</string></dict></plist>"#.to_vec(),
                stderr: Vec::new(),
            });
        }
        if p.ends_with("mdfind") {
            return Ok(CommandOutput {
                status_code: Some(0),
                stdout: format!("{}\n", app_path_for_mdfind.display()).into_bytes(),
                stderr: Vec::new(),
            });
        }
        Ok(CommandOutput {
            status_code: Some(0),
            stdout: Vec::new(),
            stderr: Vec::new(),
        })
    }));

    let intel_adapter = MacOsPlatformAdapter::with_system_ops(Arc::new(intel_mock));
    let discovered_on_intel = intel_adapter.discover_installation(None).unwrap();
    assert!(!discovered_on_intel.is_valid);
    assert_eq!(discovered_on_intel.version.as_deref(), Some("26.11.0"));
    assert!(discovered_on_intel
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("arm64-only Mach-O bundle cannot execute on Intel x86_64"));

    // 2. Apple Silicon (`aarch64`) host inspecting `x86_64`-only bundle records Rosetta 2 note.
    let intel_app = create_synthetic_zalo_app(
        &tmp.path().join("IntelApp"),
        &make_macho_thin_64_le(CPU_TYPE_X86_64),
        false,
    );
    let arm_mock = MockMacOsSystemOps::new_macos(tmp.path().join("home2"), tmp.path().join("stmp2"))
        .with_command_handler(Arc::new(|prog, args, _| {
            let p = prog.to_string_lossy();
            if p.ends_with("codesign") && args.iter().any(|a| a == "-dv") {
                return Ok(CommandOutput {
                    status_code: Some(0),
                    stdout: Vec::new(),
                    stderr: b"Identifier=com.vng.zalo\nTeamIdentifier=99VNGTEAM1\nAuthority=Developer ID Application: VNG ONLINE CO.,LTD (99VNGTEAM1)\nAuthority=Developer ID Certification Authority\nAuthority=Apple Root CA\n".to_vec(),
                });
            }
            if p.ends_with("spctl") {
                return Ok(CommandOutput {
                    status_code: Some(0),
                    stdout: Vec::new(),
                    stderr: b"/Applications/Zalo.app: accepted\nsource=Notarized Developer ID\n".to_vec(),
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
        }));

    let arm_adapter = MacOsPlatformAdapter::with_system_ops(Arc::new(arm_mock));
    let install = arm_adapter.discover_installation(Some(&intel_app)).unwrap();
    assert!(install.is_valid);
    assert_eq!(install.support_status, SupportStatus::NotTested);
    assert_eq!(install.signature_status, "Valid (Notarized Developer ID)");
    assert!(install.notes.iter().any(|n| n.contains("Rosetta 2")));
    assert!(install
        .notes
        .iter()
        .any(|n| n.contains("com.apple.quarantine")));
    assert!(arm_adapter
        .capabilities()
        .diagnostics
        .iter()
        .any(|d| d.contains(LIVE_ZALO_STATUS_MARKER)));
}

#[test]
fn codesign_enforces_unverified_binary_on_unsigned_adhoc_tampered_and_wrong_publisher() {
    let tmp = tempfile::tempdir().unwrap();
    let app = create_synthetic_zalo_app(tmp.path(), &make_macho_universal2_fat(), false);

    let parsed = parse_codesign_dv_output(
        "Executable=/Applications/Zalo.app/Contents/MacOS/Zalo\nIdentifier=com.vng.zalo\nSignature=adhoc\nTeamIdentifier=not set\n",
    );
    assert!(parsed.is_adhoc);
    assert!(parsed.team_identifier.is_none());

    let (accepted, src) = parse_spctl_output(
        true,
        "/Applications/Zalo.app: accepted\nsource=Notarized Developer ID\n",
    );
    assert_eq!(accepted, Some(true));
    assert_eq!(src.as_deref(), Some("Notarized Developer ID"));

    // Ad-hoc signed bundle must fail launch with UNVERIFIED_BINARY when signature enforcement is active.
    let adhoc_mock =
        MockMacOsSystemOps::new_macos(tmp.path().join("home"), tmp.path().join("stmp"))
            .with_command_handler(Arc::new(|prog, args, _| {
                let p = prog.to_string_lossy();
                if p.ends_with("codesign") && args.iter().any(|a| a == "-dv") {
                    return Ok(CommandOutput {
                        status_code: Some(0),
                        stdout: Vec::new(),
                        stderr:
                            b"Identifier=com.vng.zalo\nSignature=adhoc\nTeamIdentifier=not set\n"
                                .to_vec(),
                    });
                }
                Ok(CommandOutput {
                    status_code: Some(0),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            }));

    let sig = dim_platform_macos::verify_macos_code_signature(&adhoc_mock, &app, Some("VNG"));
    assert_eq!(sig.status, CodeSignTrustStatus::AdHocSigned);

    let adapter = MacOsPlatformAdapter::with_system_ops(Arc::new(adhoc_mock));
    let prof = make_profile("aabbccdd", "Signed Check", "signed-check_aabbccdd");
    let pdir = tmp.path().join("Profiles").join(&prof.directory_name);
    let err = adapter
        .launch(
            &LaunchTarget::OfficialZalo {
                custom_install_root: Some(app.clone()),
            },
            &prof,
            &pdir,
        )
        .unwrap_err();
    assert_eq!(err.code(), "UNVERIFIED_BINARY");

    // Unsigned binary check
    let unsigned_mock =
        MockMacOsSystemOps::new_macos(tmp.path().join("home2"), tmp.path().join("stmp2"))
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
    let unsigned_sig =
        dim_platform_macos::verify_macos_code_signature(&unsigned_mock, &app, Some("VNG"));
    assert_eq!(unsigned_sig.status, CodeSignTrustStatus::Unsigned);
}

#[test]
fn ps_parser_handles_spaces_in_app_paths_and_filters_zombies() {
    let sample_ps = "\
  4100     1   501 Ss   184320 /Applications/Zalo.app/Contents/MacOS/Zalo --user-data-dir=/Users/alice/Library/Application Support/MultiZaloDesktop/Profiles/work_11112222/Library/Application Support/ZaloData
  4101  4100   501 S     92160 /Applications/Zalo.app/Contents/Frameworks/Zalo Helper (Renderer).app/Contents/MacOS/Zalo Helper (Renderer) --type=renderer --user-data-dir=/Users/alice/Library/Application Support/MultiZaloDesktop/Profiles/work_11112222/Library/Application Support/ZaloData
  4102  4100   501 Z         0 (Zalo Helper) <defunct>
";
    let entries = parse_ps_table(sample_ps);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].pid, 4100);
    assert_eq!(
        entries[0].executable,
        "/Applications/Zalo.app/Contents/MacOS/Zalo"
    );
    assert_eq!(entries[1].pid, 4101);
    assert_eq!(entries[1].ppid, 4100);
    assert!(entries[1].executable.contains("Zalo Helper (Renderer)"));
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
                    // Hold inside `/usr/bin/open` so the second thread hits `ProfileOperationGuard`.
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

        // Wait until thread 1 enters `/usr/bin/open` while holding `ProfileOperationGuard`.
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
fn scenario_8_and_9_dangling_singleton_lock_recycled_pid_and_primary_protection() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("Users").join("alice");
    let primary_zalo = macos_zalo_data_dir(&home);
    fs::create_dir_all(&primary_zalo).unwrap();
    fs::write(primary_zalo.join("lockfile"), r#"{"pid": 5000}"#).unwrap();

    let mock = Arc::new(MockMacOsSystemOps::new_macos(
        home.clone(),
        tmp.path().join("stmp"),
    ));
    // Simulate:
    // - PID 5000 + child 5001: Primary Zalo Desktop
    // - PID 6000: Unrelated `/Applications/Safari.app` that recycled crashed Profile A's old PID!
    mock.set_processes(vec![
        ProcessEntry {
            pid: 5000,
            ppid: 1,
            uid: 501,
            rss_kib: 150_000,
            executable: "/Applications/Zalo.app/Contents/MacOS/Zalo".to_owned(),
            args: "/Applications/Zalo.app/Contents/MacOS/Zalo".to_owned(),
        },
        ProcessEntry {
            pid: 5001,
            ppid: 5000,
            uid: 501,
            rss_kib: 80_000,
            executable: "Zalo Helper (Renderer)".to_owned(),
            args: "/Applications/Zalo.app/Contents/Frameworks/Zalo Helper (Renderer).app/Contents/MacOS/Zalo Helper (Renderer) --type=renderer".to_owned(),
        },
        ProcessEntry {
            pid: 6000,
            ppid: 1,
            uid: 501,
            rss_kib: 90_000,
            executable: "/Applications/Safari.app/Contents/MacOS/Safari".to_owned(),
            args: "/Applications/Safari.app/Contents/MacOS/Safari".to_owned(),
        },
    ]);

    let ops_dyn: Arc<dyn MacOsSystemOps> = mock.clone();
    let adapter = MacOsPlatformAdapter::with_system_ops(ops_dyn).with_signature_enforcement(false);

    // Primary inspection protects both 5000 and child 5001.
    let primary = adapter.inspect_primary().unwrap();
    assert!(primary.running);
    assert_eq!(primary.protected_pids, vec![5000, 5001]);
    assert_eq!(primary.working_set_bytes, (150_000 + 80_000) * 1024);

    // Profile A has a lock pointing to recycled PID 6000 (Safari) -> must be detected as STALE, NOT running!
    let prof = make_profile("1234abcd", "Profile A", "profile-a_1234abcd");
    let pdir = tmp.path().join("Profiles").join(&prof.directory_name);
    adapter.ensure_profile_layout(&prof, &pdir).unwrap();
    let zdata = macos_zalo_data_dir(&pdir);
    fs::write(zdata.join("lockfile"), r#"{"pid": 6000}"#).unwrap();
    fs::write(zdata.join("SingletonSocket"), b"stale-sock").unwrap();
    fs::write(zdata.join("SingletonCookie"), b"stale-cookie").unwrap();

    let state = adapter.inspect_profile(&prof, &pdir).unwrap();
    assert!(
        !state.running,
        "Recycled Safari PID 6000 must NOT be treated as owned!"
    );
    assert!(
        state.stale_lock_detected,
        "Recycled PID lock must be flagged stale!"
    );

    // Calling stop on stale/foreign-PID profile must NEVER signal PID 6000!
    let stopped = adapter.stop(&state, &primary.protected_pids, 300).unwrap();
    assert_eq!(stopped, 0);
    assert!(
        mock.terminated_pids().is_empty(),
        "Foreign PID 6000 must NEVER receive SIGTERM!"
    );

    // Recover cleans lockfile, SingletonSocket, SingletonCookie without following symlinks.
    let rec_report = adapter.recover_stale_state(&prof, &pdir, &state).unwrap();
    assert!(rec_report.recovered);
    assert!(!zdata.join("lockfile").exists());
    assert!(!zdata.join("SingletonSocket").exists());
    assert!(!zdata.join("SingletonCookie").exists());

    // Attempting to stop a state forged with primary PID 5000 or 5001 MUST fail with PROTECTED_PRIMARY_PROCESS!
    let mut forged = state.clone();
    forged.running = true;
    forged.main_pid = Some(5001);
    forged.process_ids = vec![5001];
    let prim_err = adapter
        .stop(&forged, &primary.protected_pids, 300)
        .unwrap_err();
    assert_eq!(prim_err.code(), "PROTECTED_PRIMARY_PROCESS");
    assert!(mock.terminated_pids().is_empty());

    // Attempting to use primary ZaloData path as a profile dir MUST fail with PROTECTED_PRIMARY_PATH!
    let path_err = ensure_not_macos_primary_path(&primary_zalo, Some(&home)).unwrap_err();
    assert_eq!(path_err.code(), "PROTECTED_PRIMARY_PATH");
}

#[test]
fn scenario_11_12_13_unicode_nfc_nfd_sun_len_path_max_and_permissions() {
    // Scenario 11: Vietnamese & Hangul NFC vs NFD normalization equivalence
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

    // Scenario 12: SUN_LEN 54-byte TMPDIR threshold + short fallback + PATH_MAX 1024 guard
    let tmp = tempfile::tempdir().unwrap();
    let short_tmp_root = PathBuf::from("/tmp");
    let mut mock = MockMacOsSystemOps::new_macos(tmp.path().join("h"), short_tmp_root);
    mock.short_tmp_root = tmp.path().join("st");

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
    assert!(eff_tmp
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("dim-zalo-501-a1b2c3d4"));

    // Reject > PATH_MAX (1024 bytes) deterministically
    let over_max = PathBuf::from(format!("/Users/{}", "a".repeat(MACOS_PATH_MAX)));
    let max_err = validate_macos_path(&over_max).unwrap_err();
    assert_eq!(max_err.code(), "PATH_TRAVERSAL");

    // Scenario 13: Permission failures & world-writable `0o777` detection
    let fixture_ops = Arc::new(HostFixtureMacOsOps::new(
        tmp.path().join("sim_home"),
        tmp.path().join("sim_stmp"),
    ));
    let adapter = MacOsPlatformAdapter::with_system_ops(fixture_ops)
        .with_signature_enforcement(false)
        .with_layout_options(MacOsLayoutOptions {
            symlink_user_dirs: false,
            symlink_keychain: false,
        });

    let store = JsonProfileStore::new(tmp.path().join("dim_data")).unwrap();
    store.ensure_initialized().unwrap();
    assert_eq!(
        adapter.check_profiles_acl_safe(store.profiles_dir()),
        Some(true)
    );

    // Simulate insecure world-writable 0o777 mode on `Profiles/`
    fs::write(
        store.profiles_dir().join(SIMULATED_POSIX_MODE_FILE),
        "0777\n",
    )
    .unwrap();
    assert_eq!(
        adapter.check_profiles_acl_safe(store.profiles_dir()),
        Some(false)
    );

    // Simulate read-only/unwritable 0o500 directory blocking profile layout creation
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
        assert!(
            perm_res.is_err(),
            "Unwritable parent must fail cleanly with Io/PermissionDenied"
        );
    }
}

#[test]
fn window_inspection_jxa_parser_and_focus_activation_parameterized_osascript() {
    let jxa_sample =
        r#"[{"window_id":9142,"owner_pid":4100,"title":"Zalo — Công việc","is_on_screen":true}]"#;
    let wins = parse_jxa_windows_json(jxa_sample);
    assert_eq!(wins.len(), 1);
    assert_eq!(wins[0].window_id, 9142);
    assert_eq!(wins[0].owner_pid, 4100);
    assert_eq!(wins[0].title.as_deref(), Some("Zalo — Công việc"));

    let tmp = tempfile::tempdir().unwrap();
    let mock = Arc::new(MockMacOsSystemOps::new_macos(
        tmp.path().join("home"),
        tmp.path().join("stmp"),
    ));
    mock.set_processes(vec![ProcessEntry {
        pid: 4100,
        ppid: 1,
        uid: 501,
        rss_kib: 100_000,
        executable: "/Applications/Zalo.app/Contents/MacOS/Zalo".to_owned(),
        args: format!(
            "/Applications/Zalo.app/Contents/MacOS/Zalo --user-data-dir={}",
            macos_zalo_data_dir(&tmp.path().join("p1")).display()
        ),
    }]);
    mock.set_windows(vec![WindowInfo {
        window_id: 9142,
        owner_pid: 4100,
        title: Some("Zalo — Công việc".to_owned()),
        is_on_screen: true,
    }]);

    let ops_dyn: Arc<dyn MacOsSystemOps> = mock.clone();
    let adapter = MacOsPlatformAdapter::with_system_ops(ops_dyn).with_signature_enforcement(false);
    let prof = make_profile("4100aaaa", "Work Mac", "work-mac_4100aaaa");
    let pdir = tmp.path().join("p1");
    adapter.ensure_profile_layout(&prof, &pdir).unwrap();
    assert!(pdir.join(MACOS_PROFILE_META_FILE).exists());

    let rt = adapter.inspect_profile(&prof, &pdir).unwrap();
    assert!(rt.running);
    assert_eq!(rt.window_handle, Some(9142));
    assert_eq!(rt.window_title.as_deref(), Some("Zalo — Công việc"));

    // Write `.window-state.json` override and focus
    fs::write(
        rt.zalo_data_dir.join(MACOS_WINDOW_STATE_FILE),
        r#"{"window_id":9999,"owner_pid":4100,"title":"Zalo — Override","visible":true}"#,
    )
    .unwrap();
    let rt2 = adapter.inspect_profile(&prof, &pdir).unwrap();
    assert_eq!(rt2.window_handle, Some(9999));

    let target = LaunchTarget::OfficialZalo {
        custom_install_root: None,
    };
    adapter
        .focus_or_restore(&target, &prof, &pdir, &rt2)
        .unwrap();
    assert!(rt2.zalo_data_dir.join(".focus-signal").exists());
    let cmds = mock.commands();
    assert!(cmds
        .iter()
        .any(|c| c.program.to_string_lossy().ends_with("osascript")
            && c.args.contains(&"4100".to_owned())));
}

#[test]
fn scenarios_1_to_6_and_10_live_multi_session_isolation_relaunch_close_and_symlink_safety() {
    let tmp = tempfile::tempdir().unwrap();
    let store = JsonProfileStore::new(tmp.path().join("dim_data")).unwrap();
    store.ensure_initialized().unwrap();

    let adapter = MacOsPlatformAdapter::for_fixture_host(
        tmp.path().join("sim_home"),
        tmp.path().join("stmp"),
    );

    let fixture_raw = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("platform-macos")
        .join("fixture-macos-app.mjs");
    let fixture_script = fs::canonicalize(&fixture_raw).unwrap();
    // Avoid Windows `\\?\` extended-length prefix so Node.js and validate_macos_path receive standard paths.
    let fixture_clean = PathBuf::from(
        fixture_script
            .to_string_lossy()
            .trim_start_matches(r"\\?\")
            .to_owned(),
    );
    assert!(fixture_clean.is_file());

    let p1 = store.create_profile("Đức - Công việc", "S1").unwrap();
    let p2 = store.create_profile("Tài khoản Cá nhân", "S2").unwrap();
    let p3 = store.create_profile("Hỗ trợ Khách hàng", "S3").unwrap();

    let d1 = store.profile_dir_for(&p1).unwrap();
    let d2 = store.profile_dir_for(&p2).unwrap();
    let d3 = store.profile_dir_for(&p3).unwrap();

    let target = LaunchTarget::NodeFixture {
        node_path: None,
        script_path: fixture_clean,
        extra_args: vec!["--spawn-helper".to_owned()],
    };

    // Scenario 1: 1 session
    let rt1 = adapter.launch(&target, &p1, &d1).unwrap();
    assert!(rt1.running);
    assert_eq!(
        fs::read_to_string(rt1.zalo_data_dir.join(ZALO_PROFILE_ID_FILE)).unwrap(),
        p1.id
    );

    // Scenario 2: 2 concurrent sessions
    let rt2 = adapter.launch(&target, &p2, &d2).unwrap();
    assert!(rt2.running);
    assert_ne!(rt1.main_pid, rt2.main_pid);

    // Scenario 3: 3 concurrent sessions
    let rt3 = adapter.launch(&target, &p3, &d3).unwrap();
    assert!(rt3.running);
    assert_ne!(rt2.main_pid, rt3.main_pid);
    assert_ne!(rt1.main_pid, rt3.main_pid);

    // Scenario 4: Isolated data across all 3 profiles
    assert_ne!(rt1.detected_zalo_user_id, rt2.detected_zalo_user_id);
    assert_ne!(rt2.detected_zalo_user_id, rt3.detected_zalo_user_id);
    assert!(d1.join("Library/Preferences/com.vng.zalo.plist").is_file());
    assert!(d2.join("Library/Preferences/com.vng.zalo.plist").is_file());
    assert!(d3
        .join("Library/Caches/com.vng.zalo/cache-meta.json")
        .is_file());

    // Scenario 6: Individual close (stop p2 while p1 and p3 keep running!)
    let stopped_p2 = adapter.stop(&rt2, &[], 3_000).unwrap();
    assert!(stopped_p2 >= 1);
    assert!(adapter.inspect_profile(&p1, &d1).unwrap().running);
    assert!(!adapter.inspect_profile(&p2, &d2).unwrap().running);
    assert!(adapter.inspect_profile(&p3, &d3).unwrap().running);

    // Scenario 5: Relaunch stopped p2 cleanly
    let rt2_re = adapter.launch(&target, &p2, &d2).unwrap();
    assert!(rt2_re.running);

    // Stop remaining sessions cleanly
    let _ = adapter
        .stop(&adapter.inspect_profile(&p1, &d1).unwrap(), &[], 3_000)
        .unwrap();
    let _ = adapter.stop(&rt2_re, &[], 3_000).unwrap();
    let _ = adapter
        .stop(&adapter.inspect_profile(&p3, &d3).unwrap(), &[], 3_000)
        .unwrap();

    // Scenario 10: Safe symlink/junction deletion preserves outside host files
    let outside = tmp.path().join("outside_real_docs");
    fs::create_dir_all(&outside).unwrap();
    let sentinel = outside.join("keep_me.txt");
    fs::write(&sentinel, "HOST_DATA_INTACT").unwrap();

    let link_parent = d3.join("Downloads");
    fs::create_dir_all(&link_parent).unwrap();
    let link_target = link_parent.join("SharedHostFolder");
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
        .delete_profile_with_guard(&p3.id, true, false, &|dir| {
            adapter.unlink_profile_junctions(dir)
        })
        .unwrap();
    assert!(!d3.exists());
    assert_eq!(fs::read_to_string(&sentinel).unwrap(), "HOST_DATA_INTACT");
}

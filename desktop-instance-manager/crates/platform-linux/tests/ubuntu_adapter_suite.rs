//! Comprehensive 15-scenario integration test suite for `dim-platform-linux`.
//!
//! Covers all 15 required Ubuntu Linux verification areas:
//! 1. Application discovery & ELF/AppImage provenance
//! 2. Multiple concurrent fixture instances
//! 3. Process tree separation & primary PID protection
//! 4. Data directory separation & backup/restore roundtrip
//! 5. XDG Base Directory & sanitized child spawn environment
//! 6. File ownership & POSIX `0o700` permission checks
//! 7. Safe graceful shutdown (zero `killall`/`pkill`)
//! 8. Crash recovery
//! 9. Stale lock & recycled-PID spoofing detection
//! 10. Path traversal & protected primary path guards
//! 11. Symlink attack protection & safe symlink unlinking on delete
//! 12. AppImage filename/path compatibility (spaces, `+ZaDark`, Vietnamese Unicode, `.desktop` quoting)
//! 13. Missing dependency handling & Ubuntu 24.04 (`t64`) / 22.04 package mapping
//! 14. Non-root execution (`UID != 0`) & unsafe startup-script blocking
//! 15. Ubuntu 24.04 AppArmor `userns` restrictions & `--no-sandbox` `SECURITY_BLOCKED` enforcement

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use dim_core::{
    ApplicationLocator, BackupManager, InstanceLauncher, InstanceMonitor, LaunchTarget,
    PlatformCapabilities, ProfileRecord, ProfileStore, SupportStatus, ZALO_PROFILE_ID_FILE,
};
use dim_platform_linux::{
    analyze_startup_script, build_bwrap_desktop_args, build_sanitized_linux_spawn_env,
    classify_binary_bytes, compare_linux_isolation_mechanisms, discover_compatible_apps_in_roots,
    ensure_linux_profile_layout, linux_zalo_data_dir, parse_desktop_entry,
    parse_ldd_missing_libraries, parse_wmctrl_windows_for_pids,
    render_recommended_apparmor_profile, sha256_bytes, verify_directory_ownership_and_mode,
    BinaryFormatKind, ElfCpuArch, LinuxIsolationMechanism, LinuxPlatformAdapter, UbuntuTargetTier,
    KNOWN_COMMUNITY_RELEASES,
};
use dim_profile_store::{FileBackupManager, JsonProfileStore};

fn fixture_script_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("fixtures")
        .join("test-desktop-app")
        .join("fixture-app.mjs")
}

/// Builds a synthetic 64-byte ELF64 header with optional Type-2 AppImage magic (`AI\x02` at offset 8).
fn make_elf64_stub(arch_machine: u16, appimage_type2: bool, marker: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0u8; 128];
    bytes[0..4].copy_from_slice(b"\x7fELF");
    bytes[4] = 2; // ELFCLASS64
    bytes[5] = 1; // ELFDATA2LSB
    bytes[6] = 1; // EV_CURRENT
    if appimage_type2 {
        bytes[8..11].copy_from_slice(b"AI\x02");
    }
    bytes[16..18].copy_from_slice(&2u16.to_le_bytes()); // ET_EXEC
    bytes[18..20].copy_from_slice(&arch_machine.to_le_bytes());
    bytes.extend_from_slice(marker);
    bytes
}

fn host_elf_machine() -> u16 {
    if cfg!(target_arch = "aarch64") {
        0xB7
    } else {
        0x3E
    }
}

fn write_executable_file(path: &Path, content: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

#[allow(clippy::too_many_arguments)]
fn write_fake_proc_entry(
    proc_root: &Path,
    pid: u32,
    comm: &str,
    state: char,
    ppid: u32,
    uid: u32,
    rss_kib: u64,
    environ: &[(&str, &str)],
    cmdline: &[&str],
) {
    let dir = proc_root.join(pid.to_string());
    fs::create_dir_all(&dir).unwrap();
    let stat_line = format!(
        "{pid} ({comm}) {state} {ppid} {pid} {pid} 0 -1 4194304 100 0 0 0 10 5 0 0 20 0 1 0 1000 10485760 256 18446744073709551615\n"
    );
    fs::write(dir.join("stat"), stat_line).unwrap();
    let status_text = format!(
        "Name:\t{comm}\nState:\t{state}\nTgid:\t{pid}\nPid:\t{pid}\nPPid:\t{ppid}\nUid:\t{uid}\t{uid}\t{uid}\t{uid}\nVmRSS:\t{rss_kib} kB\n"
    );
    fs::write(dir.join("status"), status_text).unwrap();

    let mut env_bytes = Vec::new();
    for (k, v) in environ {
        env_bytes.extend_from_slice(format!("{k}={v}").as_bytes());
        env_bytes.push(0);
    }
    fs::write(dir.join("environ"), env_bytes).unwrap();

    let mut cmd_bytes = Vec::new();
    for arg in cmdline {
        cmd_bytes.extend_from_slice(arg.as_bytes());
        cmd_bytes.push(0);
    }
    fs::write(dir.join("cmdline"), cmd_bytes).unwrap();
}

struct ChildGuard(Option<Child>);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut c) = self.0.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

fn spawn_linux_env_fixture(
    profile: &ProfileRecord,
    profile_dir: &Path,
    extra_args: &[&str],
) -> ChildGuard {
    ensure_linux_profile_layout(profile, profile_dir).unwrap();
    let script = fixture_script_path();
    assert!(
        script.is_file(),
        "fixture-app.mjs must exist at {}",
        script.display()
    );
    let env_map = build_sanitized_linux_spawn_env(profile, profile_dir, &|k| {
        if k == "PATH" {
            std::env::var("PATH").ok()
        } else {
            None
        }
    });
    let mut cmd = Command::new("node");
    cmd.arg(&script)
        .args(extra_args)
        .env_clear()
        .envs(&env_map)
        // Ensure Windows Node.js also has SYSTEMROOT for Winsock/crypto initialization while keeping APPDATA unset!
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Ok(sysroot) = std::env::var("SYSTEMROOT") {
        cmd.env("SYSTEMROOT", sysroot);
    }
    let child = cmd.spawn().expect("node must spawn fixture-app.mjs");
    ChildGuard(Some(child))
}

fn wait_for_file(path: &Path, timeout_ms: u64) -> bool {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    while Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        thread::sleep(Duration::from_millis(25));
    }
    path.exists()
}

// ---------------------------------------------------------------------------
// 1. Application discovery & ELF/AppImage provenance
// ---------------------------------------------------------------------------
#[test]
fn test_01_application_discovery_and_elf_appimage_provenance() {
    let tmp = tempfile::tempdir().unwrap();
    let apps_dir = tmp.path().join("Applications");
    let desktop_dir = tmp.path().join("applications");
    fs::create_dir_all(&apps_dir).unwrap();
    fs::create_dir_all(&desktop_dir).unwrap();

    // Official VNG Zalo discovery must always be Unsupported (`OFFICIAL_ZALO_UNSUPPORTED`).
    let adapter = LinuxPlatformAdapter::new();
    let official = adapter.discover_installation(None).unwrap();
    assert!(!official.is_valid);
    assert_eq!(official.support_status, SupportStatus::Unsupported);
    assert!(official
        .error
        .as_deref()
        .unwrap()
        .contains("OFFICIAL_ZALO_UNSUPPORTED"));

    // Type-2 ELF64 AppImage + SHA-256 verification.
    let type2_bytes = make_elf64_stub(host_elf_machine(), true, b"payload");
    let expected_sha = sha256_bytes(&type2_bytes);
    let appimage_path = apps_dir.join("Zalo-26.11.10-Original-e080c70-x86_64.AppImage");
    write_executable_file(&appimage_path, &type2_bytes);

    let (fmt, arch) = classify_binary_bytes(&type2_bytes, b"", true);
    assert_eq!(fmt, BinaryFormatKind::AppImageType2);
    assert!(arch.is_some());

    let pinned_ok = LinuxPlatformAdapter::new().with_expected_sha256(&expected_sha);
    let discovered = pinned_ok
        .discover_installation(Some(&appimage_path))
        .unwrap();
    assert!(discovered.is_valid);
    assert_eq!(discovered.support_status, SupportStatus::Experimental);
    assert_eq!(discovered.version.as_deref(), Some("26.11.10"));
    assert!(discovered
        .signature_status
        .contains("format=appimage-type2-elf64"));

    // Wrong SHA-256 must fail closed with `UNVERIFIED_BINARY`.
    let pinned_bad = LinuxPlatformAdapter::new()
        .with_expected_sha256("0000000000000000000000000000000000000000000000000000000000000000");
    let bad_err = pinned_bad
        .discover_installation(Some(&appimage_path))
        .unwrap_err();
    assert_eq!(bad_err.code(), "UNVERIFIED_BINARY");

    // `uruntime` + DwarFS AppImage detection (stage 2 of `zalo-for-linux`).
    let dwarfs_bytes = make_elf64_stub(0xB7, false, b"uruntime-DWARFS-gh-releases-zsync");
    let (d_fmt, d_arch) = classify_binary_bytes(&dwarfs_bytes, &dwarfs_bytes, true);
    assert_eq!(d_fmt, BinaryFormatKind::AppImageUruntimeDwarfs);
    assert_eq!(d_arch, Some(ElfCpuArch::Aarch64));

    // Known community release catalog check.
    assert!(!KNOWN_COMMUNITY_RELEASES.is_empty());
    assert_eq!(KNOWN_COMMUNITY_RELEASES[0].version, "26.11.10");

    // XDG `.desktop` + directory discovery without hardcoded user paths.
    let desktop_file = desktop_dir.join("zalo-community.desktop");
    fs::write(
        &desktop_file,
        format!(
            "[Desktop Entry]\nType=Application\nName=Zalo (Community)\nExec=\"{}\" %U\nStartupWMClass=Zalo\n",
            appimage_path.display()
        ),
    )
    .unwrap();

    let cands = discover_compatible_apps_in_roots(&[desktop_dir], &[apps_dir]);
    assert_eq!(cands.len(), 1);
    assert_eq!(cands[0].name, "Zalo (Community)");
    assert_eq!(cands[0].executable_path, appimage_path);
    assert_eq!(cands[0].startup_wm_class.as_deref(), Some("Zalo"));
}

// ---------------------------------------------------------------------------
// 2. Multiple concurrent fixture instances
// ---------------------------------------------------------------------------
#[test]
fn test_02_multiple_fixture_instances_concurrent_runtime() {
    let tmp = tempfile::tempdir().unwrap();
    let store = JsonProfileStore::new(tmp.path().join("DimData")).unwrap();
    let p1 = store.create_profile("Ubuntu Work", "").unwrap();
    let p2 = store.create_profile("Ubuntu Personal", "").unwrap();
    let dir1 = store.profile_dir_for(&p1).unwrap();
    let dir2 = store.profile_dir_for(&p2).unwrap();

    let _c1 = spawn_linux_env_fixture(&p1, &dir1, &["--user-id=849000111", "--stay-ms=8000"]);
    let _c2 = spawn_linux_env_fixture(&p2, &dir2, &["--user-id=849000222", "--stay-ms=8000"]);

    let lock1 = linux_zalo_data_dir(&dir1).join("lockfile");
    let lock2 = linux_zalo_data_dir(&dir2).join("lockfile");
    assert!(wait_for_file(&lock1, 4_000), "profile 1 lockfile missing");
    assert!(wait_for_file(&lock2, 4_000), "profile 2 lockfile missing");

    let pid1 = dim_platform_linux::read_linux_pid(&linux_zalo_data_dir(&dir1)).unwrap();
    let pid2 = dim_platform_linux::read_linux_pid(&linux_zalo_data_dir(&dir2)).unwrap();
    assert_ne!(
        pid1, pid2,
        "concurrent instances must have distinct OS PIDs"
    );

    assert_eq!(
        dim_core::detect_zalo_user_id(&linux_zalo_data_dir(&dir1)).as_deref(),
        Some("849000111")
    );
    assert_eq!(
        dim_core::detect_zalo_user_id(&linux_zalo_data_dir(&dir2)).as_deref(),
        Some("849000222")
    );

    // Cleanly signal both fixtures to exit.
    fs::write(
        linux_zalo_data_dir(&dir1).join(".quit-signal"),
        "{\"requested_at\":\"now\"}\n",
    )
    .unwrap();
    fs::write(
        linux_zalo_data_dir(&dir2).join(".quit-signal"),
        "{\"requested_at\":\"now\"}\n",
    )
    .unwrap();
}

// ---------------------------------------------------------------------------
// 3. Process tree separation & primary PID protection
// ---------------------------------------------------------------------------
#[test]
fn test_03_process_tree_separation_and_primary_protection() {
    let tmp = tempfile::tempdir().unwrap();
    let fs_root = tmp.path().join("fs");
    let proc_root = tmp.path().join("proc");
    let primary_cfg = tmp.path().join("primary_config");
    let profiles_root = tmp.path().join("profiles");
    fs::create_dir_all(&fs_root).unwrap();
    fs::create_dir_all(&proc_root).unwrap();
    fs::create_dir_all(primary_cfg.join("ZaloData")).unwrap();

    // Primary Zalo lockfile + process tree (PID 1000 -> renderer 1001).
    fs::write(
        primary_cfg.join("ZaloData").join("lockfile"),
        "{\"pid\":1000}\n",
    )
    .unwrap();
    write_fake_proc_entry(
        &proc_root,
        1000,
        "zalo",
        'S',
        1,
        1000,
        120_000,
        &[("HOME", "/home/hostuser")],
        &["/opt/zalo/zalo"],
    );
    write_fake_proc_entry(
        &proc_root,
        1001,
        "zalo (renderer)",
        'S',
        1000,
        1000,
        80_000,
        &[("HOME", "/home/hostuser")],
        &["/opt/zalo/zalo", "--type=renderer"],
    );

    let store = JsonProfileStore::new(profiles_root).unwrap();
    let p_a = store.create_profile("Profile A", "").unwrap();
    let p_b = store.create_profile("Profile B", "").unwrap();
    let dir_a = store.profile_dir_for(&p_a).unwrap();
    let dir_b = store.profile_dir_for(&p_b).unwrap();
    ensure_linux_profile_layout(&p_a, &dir_a).unwrap();
    ensure_linux_profile_layout(&p_b, &dir_b).unwrap();

    fs::write(
        linux_zalo_data_dir(&dir_a).join("lockfile"),
        "{\"pid\":2000}\n",
    )
    .unwrap();
    fs::write(
        linux_zalo_data_dir(&dir_b).join("lockfile"),
        "{\"pid\":3000}\n",
    )
    .unwrap();

    // Profile A: root 2000 -> zygote 2001 -> renderer 2002, zombie 2050 ('Z'), reparented helper 2099 (ppid=1)
    write_fake_proc_entry(
        &proc_root,
        2000,
        "zalo",
        'S',
        1,
        1000,
        100_000,
        &[("DIM_PROFILE_ID", &p_a.id)],
        &["zalo"],
    );
    write_fake_proc_entry(
        &proc_root,
        2001,
        "zalo (zygote)",
        'S',
        2000,
        1000,
        40_000,
        &[("DIM_PROFILE_ID", &p_a.id)],
        &["zalo", "--type=zygote"],
    );
    write_fake_proc_entry(
        &proc_root,
        2002,
        "Web Content (1)",
        'R',
        2001,
        1000,
        60_000,
        &[("DIM_PROFILE_ID", &p_a.id)],
        &["zalo", "--type=renderer"],
    );
    write_fake_proc_entry(
        &proc_root,
        2050,
        "zalo <defunct>",
        'Z',
        2000,
        1000,
        0,
        &[("DIM_PROFILE_ID", &p_a.id)],
        &[],
    );
    write_fake_proc_entry(
        &proc_root,
        2099,
        "zocr-host",
        'S',
        1,
        1000,
        30_000,
        &[("DIM_PROFILE_ID", &p_a.id)],
        &["zocr-host"],
    );

    // Profile B: root 3000
    write_fake_proc_entry(
        &proc_root,
        3000,
        "zalo",
        'S',
        1,
        1000,
        90_000,
        &[("DIM_PROFILE_ID", &p_b.id)],
        &["zalo"],
    );

    let adapter = LinuxPlatformAdapter::with_virtual_roots(
        fs_root,
        proc_root,
        primary_cfg,
        1000,
        "x86_64",
        None,
    );

    let primary = adapter.inspect_primary().unwrap();
    assert!(primary.running);
    assert_eq!(primary.protected_pids, vec![1000, 1001]);
    assert_eq!(primary.working_set_bytes, (120_000 + 80_000) * 1024);

    let rt_a = adapter.inspect_profile(&p_a, &dir_a).unwrap();
    assert!(rt_a.running);
    assert_eq!(rt_a.main_pid, Some(2000));
    assert!(rt_a.process_ids.contains(&2000));
    assert!(rt_a.process_ids.contains(&2001));
    assert!(rt_a.process_ids.contains(&2002));
    assert!(
        rt_a.process_ids.contains(&2099),
        "reparented helper with DIM_PROFILE_ID must be tracked"
    );
    assert!(
        !rt_a.process_ids.contains(&2050),
        "zombie task must be excluded"
    );
    assert!(
        !rt_a.process_ids.contains(&3000),
        "sibling profile B PID must never leak into profile A"
    );
    assert_eq!(
        rt_a.working_set_bytes,
        (100_000 + 40_000 + 60_000 + 30_000) * 1024
    );

    // Stopping a state whose process tree overlaps primary PIDs must return PROTECTED_PRIMARY_PROCESS.
    let mut forged = rt_a;
    forged.process_ids.push(1001);
    let stop_err = adapter
        .stop(&forged, &primary.protected_pids, 500)
        .unwrap_err();
    assert_eq!(stop_err.code(), "PROTECTED_PRIMARY_PROCESS");
}

// ---------------------------------------------------------------------------
// 4. Data directory separation & backup/restore roundtrip
// ---------------------------------------------------------------------------
#[test]
fn test_04_data_directory_separation_and_backup_restore_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonProfileStore::new(tmp.path().join("DimStore")).unwrap());
    let adapter = LinuxPlatformAdapter::new();

    let p1 = store.create_profile("Kế Toán", "Accounting").unwrap();
    let p2 = store.create_profile("Kinh Doanh", "Sales").unwrap();
    let dir1 = store.profile_dir_for(&p1).unwrap();
    let dir2 = store.profile_dir_for(&p2).unwrap();

    adapter.ensure_profile_layout(&p1, &dir1).unwrap();
    adapter.ensure_profile_layout(&p2, &dir2).unwrap();
    assert_ne!(dir1, dir2);

    // Attempting to initialize Profile 2 onto Profile 1's directory must fail with PROFILE_MARKER_MISMATCH!
    let cross_err = adapter.ensure_profile_layout(&p2, &dir1).unwrap_err();
    assert_eq!(cross_err.code(), "PROFILE_MARKER_MISMATCH");

    // Populate simulated Zalo Linux session data + transient temp file in Profile 1.
    let z1 = linux_zalo_data_dir(&dir1);
    let db_dir = z1.join("Database").join("_production").join("849887766");
    fs::create_dir_all(&db_dir).unwrap();
    fs::write(db_dir.join("ftsm_849887766.db"), b"sqlite-payload-v1").unwrap();
    fs::write(dir1.join("tmp").join("ephemeral.tmp"), b"skip-me").unwrap();
    let p1 = store
        .update_last_launched(&p1.id, "2026-11-17T10:00:00Z", Some("849887766"))
        .unwrap();

    let backups = FileBackupManager::new(Arc::clone(&store));
    let bk = backups.backup_profile(&p1, &dir1, None, false).unwrap();
    let verified = backups.verify_backup(&bk.backup_dir).unwrap();
    assert!(verified
        .files
        .iter()
        .any(|f| f.relative_path.ends_with("ftsm_849887766.db")));
    assert!(
        !verified
            .files
            .iter()
            .any(|f| f.relative_path.starts_with("tmp/")),
        "tmp/ must be excluded from backup"
    );

    let restored = backups
        .restore_backup_as_new_profile(&bk.backup_dir, Some("Kế Toán Khôi Phục"))
        .unwrap();
    assert_ne!(restored.id, p1.id);
    let restored_dir = store.profile_dir_for(&restored).unwrap();
    adapter
        .prepare_restored_profile(&restored, &restored_dir)
        .unwrap();

    // Restored profile must carry the new `.zalo-profile-id` and recreated `tmp/`.
    assert_eq!(
        fs::read_to_string(linux_zalo_data_dir(&restored_dir).join(ZALO_PROFILE_ID_FILE)).unwrap(),
        restored.id
    );
    assert!(restored_dir.join("tmp").is_dir());
    assert!(linux_zalo_data_dir(&restored_dir)
        .join("Database")
        .join("_production")
        .join("849887766")
        .join("ftsm_849887766.db")
        .is_file());
}

// ---------------------------------------------------------------------------
// 5. XDG Base Directory & sanitized child spawn environment
// ---------------------------------------------------------------------------
#[test]
fn test_05_xdg_environment_and_sanitized_spawn_env() {
    let tmp = tempfile::tempdir().unwrap();
    let p = ProfileRecord {
        id: "1234abcd".to_owned(),
        name: "XDG Check".to_owned(),
        directory_name: "xdg-check_1234abcd".to_owned(),
        created_at: "2026-11-17T00:00:00Z".to_owned(),
        last_launched_at: None,
        zalo_user_id: None,
        notes: String::new(),
    };
    let pdir = tmp.path().join(&p.directory_name);
    ensure_linux_profile_layout(&p, &pdir).unwrap();

    let mut host_env = HashMap::new();
    host_env.insert("HOME", "/home/realuser");
    host_env.insert("APPDATA", "C:\\Leak");
    host_env.insert("LD_PRELOAD", "/tmp/evil.so");
    host_env.insert("NODE_OPTIONS", "--require /tmp/hook.js");
    host_env.insert("WAYLAND_DISPLAY", "wayland-0");
    host_env.insert("XDG_RUNTIME_DIR", "/run/user/1000");
    host_env.insert("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/user/1000/bus");
    host_env.insert("PULSE_SERVER", "unix:/run/user/1000/pulse/native");

    let spawn_env =
        build_sanitized_linux_spawn_env(&p, &pdir, &|k| host_env.get(k).map(|v| (*v).to_owned()));

    assert_eq!(spawn_env.get("HOME").unwrap(), &pdir.to_string_lossy());
    assert_eq!(
        spawn_env.get("XDG_CONFIG_HOME").unwrap(),
        &pdir.join(".config").to_string_lossy()
    );
    assert_eq!(
        spawn_env.get("XDG_DATA_HOME").unwrap(),
        &pdir.join(".local").join("share").to_string_lossy()
    );
    assert_eq!(
        spawn_env.get("XDG_STATE_HOME").unwrap(),
        &pdir.join(".local").join("state").to_string_lossy()
    );
    assert_eq!(
        spawn_env.get("XDG_CACHE_HOME").unwrap(),
        &pdir.join(".cache").to_string_lossy()
    );
    assert_eq!(
        spawn_env.get("TMPDIR").unwrap(),
        &pdir.join("tmp").to_string_lossy()
    );

    // Host-contaminating & code-injection variables MUST be stripped!
    assert!(!spawn_env.contains_key("APPDATA"));
    assert!(!spawn_env.contains_key("LD_PRELOAD"));
    assert!(!spawn_env.contains_key("NODE_OPTIONS"));

    // Session display/audio/portal variables MUST be preserved!
    assert_eq!(spawn_env.get("WAYLAND_DISPLAY").unwrap(), "wayland-0");
    assert_eq!(spawn_env.get("XDG_RUNTIME_DIR").unwrap(), "/run/user/1000");
    assert_eq!(
        spawn_env.get("PULSE_SERVER").unwrap(),
        "unix:/run/user/1000/pulse/native"
    );

    let user_dirs = fs::read_to_string(pdir.join(".config").join("user-dirs.dirs")).unwrap();
    assert!(user_dirs.contains("XDG_DOWNLOAD_DIR=\"$HOME/Downloads\""));
    assert!(user_dirs.contains("XDG_DOCUMENTS_DIR=\"$HOME/Documents\""));
}

// ---------------------------------------------------------------------------
// 6. File ownership & POSIX `0o700` permission checks
// ---------------------------------------------------------------------------
#[test]
fn test_06_file_ownership_and_permissions() {
    let tmp = tempfile::tempdir().unwrap();
    let p = ProfileRecord {
        id: "deadbeef".to_owned(),
        name: "Perm Test".to_owned(),
        directory_name: "perm-test_deadbeef".to_owned(),
        created_at: "2026-11-17T00:00:00Z".to_owned(),
        last_launched_at: None,
        zalo_user_id: None,
        notes: String::new(),
    };
    let profiles_dir = tmp.path().join("Profiles");
    let pdir = profiles_dir.join(&p.directory_name);
    ensure_linux_profile_layout(&p, &pdir).unwrap();

    let euid = dim_platform_linux::current_effective_uid();
    assert!(verify_directory_ownership_and_mode(&pdir, euid, true).is_ok());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&pdir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "profile_dir must be strictly 0o700 on Unix");

        // World-writable directory must fail permission verification!
        let bad_dir = tmp.path().join("WorldWritable");
        fs::create_dir_all(&bad_dir).unwrap();
        fs::set_permissions(&bad_dir, fs::Permissions::from_mode(0o777)).unwrap();
        let adapter = LinuxPlatformAdapter::new();
        assert_eq!(adapter.check_profiles_acl_safe(&bad_dir), Some(false));
        assert_eq!(adapter.check_profiles_acl_safe(&profiles_dir), Some(true));
    }
}

// ---------------------------------------------------------------------------
// 7. Safe graceful shutdown (zero `killall`/`pkill`)
// ---------------------------------------------------------------------------
#[test]
fn test_07_safe_graceful_shutdown_leaves_sibling_running() {
    let tmp = tempfile::tempdir().unwrap();
    let store = JsonProfileStore::new(tmp.path().join("DimStop")).unwrap();
    let p_a = store.create_profile("Stop Target", "").unwrap();
    let p_b = store.create_profile("Keep Alive", "").unwrap();
    let dir_a = store.profile_dir_for(&p_a).unwrap();
    let dir_b = store.profile_dir_for(&p_b).unwrap();

    let mut guard_a =
        spawn_linux_env_fixture(&p_a, &dir_a, &["--user-id=849111001", "--stay-ms=10000"]);
    let _guard_b =
        spawn_linux_env_fixture(&p_b, &dir_b, &["--user-id=849111002", "--stay-ms=10000"]);

    let z_a = linux_zalo_data_dir(&dir_a);
    let z_b = linux_zalo_data_dir(&dir_b);
    assert!(wait_for_file(&z_a.join("lockfile"), 4_000));
    assert!(wait_for_file(&z_b.join("lockfile"), 4_000));

    // Request graceful shutdown on Profile A ONLY.
    fs::write(z_a.join(".quit-signal"), "{\"requested_at\":\"now\"}\n").unwrap();
    if let Some(ref mut child_a) = guard_a.0 {
        let deadline = Instant::now() + Duration::from_secs(4);
        while Instant::now() < deadline {
            if child_a.try_wait().unwrap().is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(25));
        }
        assert!(
            child_a.try_wait().unwrap().is_some(),
            "Profile A must exit gracefully"
        );
    }

    let state_a: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(z_a.join("launch-state.json")).unwrap()).unwrap();
    assert_eq!(state_a["clean_shutdown"], true);
    assert!(!z_a.join("lockfile").exists());

    // Sibling Profile B must still hold its lockfile!
    assert!(
        z_b.join("lockfile").exists(),
        "Profile B must remain untouched"
    );
    fs::write(z_b.join(".quit-signal"), "{\"requested_at\":\"now\"}\n").unwrap();
}

// ---------------------------------------------------------------------------
// 8. Crash recovery
// ---------------------------------------------------------------------------
#[test]
fn test_08_crash_simulation_and_stale_lock_recovery() {
    let tmp = tempfile::tempdir().unwrap();
    let store = JsonProfileStore::new(tmp.path().join("DimCrash")).unwrap();
    let p = store.create_profile("Crash Profile", "").unwrap();
    let pdir = store.profile_dir_for(&p).unwrap();
    let zdir = linux_zalo_data_dir(&pdir);

    let mut crash_child = spawn_linux_env_fixture(
        &p,
        &pdir,
        &["--crash", "--crash-delay-ms=60", "--user-id=849333444"],
    );
    if let Some(ref mut c) = crash_child.0 {
        let status = c.wait().unwrap();
        assert!(!status.success(), "simulated crash must exit non-zero");
    }

    // Crashed fixture leaves stale `lockfile` (and `SingletonLock` on POSIX).
    assert!(zdir.join("lockfile").exists());
    let launch_state: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(zdir.join("launch-state.json")).unwrap()).unwrap();
    assert_eq!(launch_state["crashed"], true);
    assert_eq!(launch_state["clean_shutdown"], false);

    // Relaunching recovers stale lock and increments launch_count to 2.
    let _recovered = spawn_linux_env_fixture(&p, &pdir, &["--user-id=849333444", "--stay-ms=4000"]);
    let deadline = Instant::now() + Duration::from_secs(4);
    let mut recovered_count = 0u64;
    while Instant::now() < deadline {
        if let Ok(raw) = fs::read_to_string(zdir.join("launch-state.json")) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                recovered_count = v["launch_count"].as_u64().unwrap_or(0);
                if recovered_count >= 2 {
                    break;
                }
            }
        }
        thread::sleep(Duration::from_millis(30));
    }
    assert_eq!(recovered_count, 2, "relaunch after crash must succeed");
    fs::write(zdir.join(".quit-signal"), "{\"requested_at\":\"now\"}\n").unwrap();
}

// ---------------------------------------------------------------------------
// 9. Stale lock & recycled-PID spoofing detection
// ---------------------------------------------------------------------------
#[test]
fn test_09_stale_lock_and_recycled_pid_spoofing_detection() {
    let tmp = tempfile::tempdir().unwrap();
    let fs_root = tmp.path().join("fs");
    let proc_root = tmp.path().join("proc");
    let primary_cfg = tmp.path().join("primary");
    fs::create_dir_all(&fs_root).unwrap();
    fs::create_dir_all(&proc_root).unwrap();
    fs::create_dir_all(&primary_cfg).unwrap();

    let p = ProfileRecord {
        id: "778899aa".to_owned(),
        name: "Recycled Check".to_owned(),
        directory_name: "recycled_778899aa".to_owned(),
        created_at: "2026-11-17T00:00:00Z".to_owned(),
        last_launched_at: None,
        zalo_user_id: None,
        notes: String::new(),
    };
    let pdir = tmp.path().join(&p.directory_name);
    ensure_linux_profile_layout(&p, &pdir).unwrap();
    let zdir = linux_zalo_data_dir(&pdir);

    // Write SingletonLock / lockfile pointing to PID 4242, while PID 4242 in /proc is actually `gnome-shell`!
    #[cfg(unix)]
    std::os::unix::fs::symlink("ubuntu-24-04-desktop-4242", zdir.join("SingletonLock")).unwrap();
    #[cfg(not(unix))]
    fs::write(zdir.join("SingletonLock"), "ubuntu-24-04-desktop-4242").unwrap();

    write_fake_proc_entry(
        &proc_root,
        4242,
        "gnome-shell",
        'S',
        1,
        1000,
        250_000,
        &[
            ("HOME", "/home/hostuser"),
            ("XDG_CONFIG_HOME", "/home/hostuser/.config"),
        ],
        &["/usr/bin/gnome-shell"],
    );

    let adapter = LinuxPlatformAdapter::with_virtual_roots(
        fs_root,
        proc_root,
        primary_cfg,
        1000,
        "x86_64",
        None,
    );
    let rt = adapter.inspect_profile(&p, &pdir).unwrap();
    assert!(
        !rt.running,
        "recycled unrelated PID (gnome-shell) must NEVER be misidentified as running Zalo"
    );
    assert!(
        rt.stale_lock_detected,
        "dangling SingletonLock for recycled PID must be flagged as stale lock"
    );
    assert_eq!(rt.main_pid, None);
}

// ---------------------------------------------------------------------------
// 10. Path traversal & primary path guards
// ---------------------------------------------------------------------------
#[test]
fn test_10_path_traversal_and_primary_path_guards() {
    let adapter = LinuxPlatformAdapter::new();
    let p = ProfileRecord {
        id: "abcd1234".to_owned(),
        name: "Traversal Test".to_owned(),
        directory_name: "traversal_abcd1234".to_owned(),
        created_at: "2026-11-17T00:00:00Z".to_owned(),
        last_launched_at: None,
        zalo_user_id: None,
        notes: String::new(),
    };

    let err1 = adapter
        .discover_installation(Some(Path::new("/opt/zalo/../../etc/passwd")))
        .unwrap_err();
    assert_eq!(err1.code(), "PATH_TRAVERSAL");

    let err2 = adapter
        .ensure_profile_layout(&p, Path::new("/tmp/profiles/../escape"))
        .unwrap_err();
    assert_eq!(err2.code(), "PATH_TRAVERSAL");

    if let Some(primary) = dim_core::resolve_primary_zalo_data_dir() {
        let err3 = adapter
            .ensure_profile_layout(&p, &primary.join("nested"))
            .unwrap_err();
        assert_eq!(err3.code(), "PROTECTED_PRIMARY_PATH");
    }
}

// ---------------------------------------------------------------------------
// 11. Symlink attack protection & safe unlink on profile delete
// ---------------------------------------------------------------------------
#[cfg(unix)]
#[test]
fn test_11_unix_symlink_attack_and_safe_unlink() {
    let tmp = tempfile::tempdir().unwrap();
    let victim = tmp.path().join("ExternalVictim");
    fs::create_dir_all(&victim).unwrap();
    fs::write(victim.join("precious.txt"), b"keep-safe").unwrap();

    let p = ProfileRecord {
        id: "99887766".to_owned(),
        name: "Symlink Attack".to_owned(),
        directory_name: "symlink_99887766".to_owned(),
        created_at: "2026-11-17T00:00:00Z".to_owned(),
        last_launched_at: None,
        zalo_user_id: None,
        notes: String::new(),
    };

    // 1. Pre-planted symlinked `.config/ZaloData` inside profile_dir must be rejected!
    let planted = tmp.path().join("planted_profile");
    fs::create_dir_all(planted.join(".config")).unwrap();
    std::os::unix::fs::symlink(&victim, planted.join(".config").join("ZaloData")).unwrap();
    let err = ensure_linux_profile_layout(&p, &planted).unwrap_err();
    assert_eq!(err.code(), "PATH_TRAVERSAL");

    // 2. Deleting a profile containing an internal symlink must unlink ONLY the symlink, keeping `precious.txt`!
    let store = JsonProfileStore::new(tmp.path().join("SafeDeleteStore")).unwrap();
    let created = store.create_profile("Delete Me", "").unwrap();
    let cdir = store.profile_dir_for(&created).unwrap();
    ensure_linux_profile_layout(&created, &cdir).unwrap();
    std::os::unix::fs::symlink(&victim, cdir.join("linked_victim")).unwrap();

    let adapter = LinuxPlatformAdapter::new();
    store
        .delete_profile_with_guard(&created.id, true, false, &|d| {
            adapter.unlink_profile_junctions(d)
        })
        .unwrap();
    assert!(!cdir.exists());
    assert!(
        victim.join("precious.txt").is_file(),
        "external symlink target must remain 100% intact"
    );
}

#[cfg(not(unix))]
#[test]
fn test_11_portable_unlink_reparse_points_safe() {
    let tmp = tempfile::tempdir().unwrap();
    let store = JsonProfileStore::new(tmp.path().join("SafeDeleteStore")).unwrap();
    let created = store.create_profile("Delete Me", "").unwrap();
    let cdir = store.profile_dir_for(&created).unwrap();
    ensure_linux_profile_layout(&created, &cdir).unwrap();
    let adapter = LinuxPlatformAdapter::new();
    assert_eq!(adapter.unlink_profile_junctions(&cdir).unwrap(), 0);
    store
        .delete_profile_with_guard(&created.id, true, false, &|d| {
            adapter.unlink_profile_junctions(d)
        })
        .unwrap();
    assert!(!cdir.exists());
}

// ---------------------------------------------------------------------------
// 12. AppImage filename/path compatibility (spaces, +ZaDark, Unicode, .desktop)
// ---------------------------------------------------------------------------
#[test]
fn test_12_appimage_filename_spaces_unicode_and_desktop_exec() {
    let tmp = tempfile::tempdir().unwrap();
    let elf = make_elf64_stub(host_elf_machine(), true, b"appimage");
    let adapter = LinuxPlatformAdapter::new();

    let tricky_paths = [
        tmp.path()
            .join("Zalo-26.11.10+ZaDark-26.3-e080c70-x86_64.AppImage"),
        tmp.path()
            .join("My Linux Apps")
            .join("Zalo (Community) 26.11.10.AppImage"),
        tmp.path()
            .join("Ứng dụng Ubuntu")
            .join("Zalo Thử Nghiệm 26.10.20.appimage"),
    ];

    for p in &tricky_paths {
        write_executable_file(p, &elf);
        let res = adapter.discover_installation(Some(p)).unwrap();
        assert!(res.is_valid, "failed for path {}", p.display());
        assert_eq!(res.support_status, SupportStatus::Experimental);
        assert!(res.version.is_some());
    }

    let desktop = parse_desktop_entry(
        "[Desktop Entry]\nName=Zalo Tiếng Việt\nExec=\"/home/demo/Ứng dụng Ubuntu/Zalo 26.11.10.AppImage\" --enable-features=WebRTCPipeWireCapturer %U\nStartupWMClass=Zalo\n",
    );
    assert_eq!(
        desktop.exec_program.as_deref(),
        Some(Path::new(
            "/home/demo/Ứng dụng Ubuntu/Zalo 26.11.10.AppImage"
        ))
    );
    assert_eq!(
        desktop.exec_args,
        vec!["--enable-features=WebRTCPipeWireCapturer".to_owned()]
    );
}

// ---------------------------------------------------------------------------
// 13. Missing dependency handling & Ubuntu 24.04 (t64) / 22.04 package mapping
// ---------------------------------------------------------------------------
#[test]
fn test_13_missing_dependency_handling_and_ubuntu_package_mapping() {
    let ldd_sample = "\
        linux-vdso.so.1 (0x00007ffd31bf8000)\n\
        libfuse.so.2 => not found\n\
        libopus.so.0 => not found\n\
        libpulse-simple.so.0 => not found\n\
        libasound.so.2 => not found\n\
        libwebkit2gtk-4.1.so.0 => not found\n\
        libc.so.6 => /lib/x86_64-linux-gnu/libc.so.6 (0x00007f0c9a200000)\n";

    let missing = parse_ldd_missing_libraries(ldd_sample);
    assert_eq!(missing.len(), 5);
    assert_eq!(missing[0].soname, "libfuse.so.2");
    assert_eq!(missing[0].ubuntu_2404_package, "libfuse2t64");
    assert_eq!(missing[0].ubuntu_2204_package, "libfuse2");
    assert_eq!(missing[1].ubuntu_2404_package, "libopus0");
    assert_eq!(missing[2].ubuntu_2404_package, "libpulse0");
    assert_eq!(missing[3].ubuntu_2404_package, "libasound2t64");
    assert_eq!(missing[3].ubuntu_2204_package, "libasound2");
    assert_eq!(missing[4].ubuntu_2404_package, "libwebkit2gtk-4.1-0");

    // Missing fixture script & missing custom binary must return `INSTALL_NOT_FOUND`.
    let tmp = tempfile::tempdir().unwrap();
    let p = ProfileRecord {
        id: "55667788".to_owned(),
        name: "Missing Dep".to_owned(),
        directory_name: "missing_55667788".to_owned(),
        created_at: "2026-11-17T00:00:00Z".to_owned(),
        last_launched_at: None,
        zalo_user_id: None,
        notes: String::new(),
    };
    let pdir = tmp.path().join(&p.directory_name);
    let adapter = LinuxPlatformAdapter::new();

    let err_missing_script = adapter
        .launch(
            &LaunchTarget::NodeFixture {
                node_path: None,
                script_path: tmp.path().join("does-not-exist.mjs"),
                extra_args: Vec::new(),
            },
            &p,
            &pdir,
        )
        .unwrap_err();
    assert_eq!(err_missing_script.code(), "INSTALL_NOT_FOUND");

    let err_missing_bin = adapter
        .launch(
            &LaunchTarget::CustomExecutable {
                path: tmp.path().join("MissingZalo.AppImage"),
                args: Vec::new(),
            },
            &p,
            &pdir,
        )
        .unwrap_err();
    assert_eq!(err_missing_bin.code(), "INSTALL_NOT_FOUND");
}

// ---------------------------------------------------------------------------
// 14. Non-root execution & startup-script hazard blocking
// ---------------------------------------------------------------------------
#[test]
fn test_14_non_root_execution_and_startup_script_security() {
    let tmp = tempfile::tempdir().unwrap();
    let root_adapter = LinuxPlatformAdapter::with_virtual_roots(
        tmp.path().join("fs"),
        tmp.path().join("proc"),
        tmp.path().join("cfg"),
        0, // UID 0 (root)!
        "x86_64",
        None,
    );
    assert_eq!(root_adapter.is_elevated(), Some(true));

    let p = ProfileRecord {
        id: "00001111".to_owned(),
        name: "Root Blocked".to_owned(),
        directory_name: "root_00001111".to_owned(),
        created_at: "2026-11-17T00:00:00Z".to_owned(),
        last_launched_at: None,
        zalo_user_id: None,
        notes: String::new(),
    };
    let pdir = tmp.path().join(&p.directory_name);
    let root_err = root_adapter
        .launch(
            &LaunchTarget::NodeFixture {
                node_path: None,
                script_path: fixture_script_path(),
                extra_args: Vec::new(),
            },
            &p,
            &pdir,
        )
        .unwrap_err();
    assert_eq!(root_err.code(), "PROCESS_ERROR");
    assert!(root_err.to_string().contains("SECURITY_BLOCKED"));

    // Hazardous wrapper script must be caught and blocked!
    let unsafe_script = "#!/usr/bin/env bash\nkillall zalo\nsudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0\nexec /home/alice/Zalo.AppImage --no-sandbox\n";
    let audit = analyze_startup_script(unsafe_script);
    assert!(!audit.safe);
    assert!(audit.hazards.len() >= 4);

    let wrapper_path = tmp.path().join("unsafe-launcher.sh");
    write_executable_file(&wrapper_path, unsafe_script.as_bytes());
    let normal_adapter = LinuxPlatformAdapter::new();
    let disc = normal_adapter
        .discover_installation(Some(&wrapper_path))
        .unwrap();
    assert!(!disc.is_valid);
    assert_eq!(disc.support_status, SupportStatus::Blocked);
    assert_eq!(disc.signature_status, "SECURITY_BLOCKED");
}

// ---------------------------------------------------------------------------
// 15. Ubuntu 24.04 AppArmor restrictions, bwrap readiness & --no-sandbox block
// ---------------------------------------------------------------------------
#[test]
fn test_15_ubuntu_2404_apparmor_restrictions_and_no_sandbox_block() {
    let tmp = tempfile::tempdir().unwrap();
    let fs_root = tmp.path().join("ubuntu2404");
    let proc_root = fs_root.join("proc");
    let primary_cfg = tmp.path().join("primary");

    fs::create_dir_all(fs_root.join("etc/apparmor.d")).unwrap();
    fs::create_dir_all(fs_root.join("sys/module/apparmor/parameters")).unwrap();
    fs::create_dir_all(fs_root.join("proc/sys/kernel")).unwrap();
    fs::create_dir_all(&primary_cfg).unwrap();

    fs::write(
        fs_root.join("etc/os-release"),
        "ID=ubuntu\nVERSION_ID=\"24.04\"\nVERSION_CODENAME=noble\n",
    )
    .unwrap();
    fs::write(
        fs_root.join("sys/module/apparmor/parameters/enabled"),
        "Y\n",
    )
    .unwrap();
    fs::write(
        fs_root.join("proc/sys/kernel/apparmor_restrict_unprivileged_userns"),
        "1\n",
    )
    .unwrap();
    fs::write(
        fs_root.join("proc/sys/kernel/unprivileged_userns_clone"),
        "1\n",
    )
    .unwrap();
    fs::write(
        fs_root.join("etc/apparmor.d/bwrap-userns-restrict"),
        "profile bwrap /usr/bin/bwrap flags=(unconfined) { userns, }\n",
    )
    .unwrap();

    let fake_bwrap = tmp.path().join("bwrap");
    write_executable_file(&fake_bwrap, b"#!/bin/sh\n");

    let adapter = LinuxPlatformAdapter::with_virtual_roots(
        fs_root.clone(),
        proc_root.clone(),
        primary_cfg.clone(),
        1000,
        "x86_64",
        Some(fake_bwrap),
    );
    let probe = adapter.probe_ubuntu_environment();
    assert_eq!(probe.target_tier, UbuntuTargetTier::Ubuntu2404X86_64);
    assert!(probe.unconfined_userns_restricted());
    assert!(probe.evaluate_bwrap_readiness().is_ok());

    let appimage = tmp
        .path()
        .join("Zalo-26.11.10-Original-e080c70-x86_64.AppImage");
    write_executable_file(
        &appimage,
        &make_elf64_stub(host_elf_machine(), true, b"appimage"),
    );

    let p = ProfileRecord {
        id: "2404aaaa".to_owned(),
        name: "Ubuntu 24.04".to_owned(),
        directory_name: "ubuntu_2404aaaa".to_owned(),
        created_at: "2026-11-17T00:00:00Z".to_owned(),
        last_launched_at: None,
        zalo_user_id: None,
        notes: String::new(),
    };
    let pdir = tmp.path().join(&p.directory_name);

    // 15a. Explicit `--no-sandbox` must ALWAYS be rejected with `SECURITY_BLOCKED`!
    let no_sandbox_err = adapter
        .launch(
            &LaunchTarget::CustomExecutable {
                path: appimage.clone(),
                args: vec!["--no-sandbox".to_owned()],
            },
            &p,
            &pdir,
        )
        .unwrap_err();
    assert!(no_sandbox_err
        .to_string()
        .contains("SECURITY_BLOCKED: flag '--no-sandbox'"));

    // 15b. Unconfined Electron AppImage without targeted `/etc/apparmor.d/` profile must be `SECURITY_BLOCKED`!
    let unconfined_err = adapter
        .launch(
            &LaunchTarget::CustomExecutable {
                path: appimage.clone(),
                args: Vec::new(),
            },
            &p,
            &pdir,
        )
        .unwrap_err();
    assert!(unconfined_err
        .to_string()
        .contains("SECURITY_BLOCKED: Ubuntu AppArmor restricts unprivileged user namespaces"));

    // 15c. Provisioning a targeted AppArmor `userns` profile satisfies the security preflight cleanly!
    let profile_snippet = render_recommended_apparmor_profile(&appimage);
    fs::write(
        fs_root.join("etc/apparmor.d/local-zalo-appimage"),
        &profile_snippet,
    )
    .unwrap();
    assert!(dim_platform_linux::has_targeted_apparmor_userns_profile(
        &fs_root, &appimage
    ));

    // 15d. Verify isolation comparison matrix & X11 wmctrl parser & bwrap AppImage bind.
    let matrix = compare_linux_isolation_mechanisms();
    assert_eq!(matrix.len(), 5);
    let home_xdg = matrix
        .iter()
        .find(|m| m.mechanism == LinuxIsolationMechanism::SeparateHomeAndXdgProfile)
        .unwrap();
    assert!(home_xdg.separates_profile_data);
    assert!(
        !home_xdg.is_true_security_sandbox,
        "HOME/XDG separation must never be misrepresented as a true OS sandbox"
    );

    let bwrap_args = build_bwrap_desktop_args(&pdir, &appimage, &[], &|_k| None);
    assert!(
        bwrap_args.windows(3).any(|w| w[0] == "--ro-bind"
            && w[1] == appimage.to_string_lossy()
            && w[2] == appimage.to_string_lossy()),
        "bwrap must read-only bind an external AppImage without exposing real $HOME"
    );

    let wmctrl_out = "\
0x01800003  0 1000   ubuntu Zalo\n\
0x03a00001  0 2000   ubuntu Shared Worker\n\
0x03a00007  0 2000   ubuntu Zalo - Tài khoản 2\n";
    let matched_wins = parse_wmctrl_windows_for_pids(wmctrl_out, &[2000, 2001]);
    assert_eq!(matched_wins.len(), 1);
    assert_eq!(matched_wins[0].window_id, 0x03a00007);
    assert_eq!(matched_wins[0].title, "Zalo - Tài khoản 2");
}

// ---------------------------------------------------------------------------
// Native Linux kernel end-to-end runtime verification (`#[cfg(target_os = "linux")]`)
// ---------------------------------------------------------------------------
#[cfg(target_os = "linux")]
#[test]
fn test_16_native_linux_adapter_end_to_end_launch_focus_and_stop() {
    let tmp = tempfile::tempdir().unwrap();
    let store = JsonProfileStore::new(tmp.path().join("NativeLinuxDim")).unwrap();
    let p1 = store.create_profile("Native Ubuntu 1", "").unwrap();
    let p2 = store.create_profile("Native Ubuntu 2", "").unwrap();
    let dir1 = store.profile_dir_for(&p1).unwrap();
    let dir2 = store.profile_dir_for(&p2).unwrap();

    let adapter = LinuxPlatformAdapter::new();
    let target1 = LaunchTarget::NodeFixture {
        node_path: None,
        script_path: fixture_script_path(),
        extra_args: vec![
            "--user-id=849777001".to_owned(),
            "--stay-ms=15000".to_owned(),
        ],
    };
    let target2 = LaunchTarget::NodeFixture {
        node_path: None,
        script_path: fixture_script_path(),
        extra_args: vec![
            "--user-id=849777002".to_owned(),
            "--stay-ms=15000".to_owned(),
        ],
    };

    let rt1 = adapter.launch(&target1, &p1, &dir1).unwrap();
    let rt2 = adapter.launch(&target2, &p2, &dir2).unwrap();
    assert!(rt1.running);
    assert!(rt2.running);
    assert_ne!(rt1.main_pid, rt2.main_pid);
    assert!(rt1.working_set_bytes > 0);
    assert!(adapter.free_disk_bytes(&dir1).unwrap() > 0);

    adapter
        .focus_or_restore(&target1, &p1, &dir1, &rt1)
        .unwrap();
    thread::sleep(Duration::from_millis(120));

    let stopped1 = adapter.stop(&rt1, &[], 3_000).unwrap();
    assert!(stopped1 >= 1);
    let after1 = adapter.inspect_profile(&p1, &dir1).unwrap();
    assert!(!after1.running);

    let still2 = adapter.inspect_profile(&p2, &dir2).unwrap();
    assert!(
        still2.running,
        "stopping profile 1 must leave profile 2 alive"
    );

    let stopped2 = adapter.stop(&still2, &[], 3_000).unwrap();
    assert!(stopped2 >= 1);
}

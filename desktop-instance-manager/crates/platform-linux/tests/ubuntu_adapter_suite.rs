//! Comprehensive Ubuntu/Linux integration, security, and P1 regression test suite for `dim-platform-linux`.
//!
//! Covers all 15 core Ubuntu Linux verification areas (`test_01`..`test_16`) PLUS all 14 P1
//! regression & security scenarios (`regression_01`..`regression_14`):
//! 1. Launcher exit while detached child remains alive (`PersistentDetachedSession` / `--new-session`)
//! 2. Manager graceful shutdown (`.quit-signal` + `SIGTERM` + `bwrap` leaves-first tree ordering)
//! 3. Unexpected parent termination (`DieWithParent` + orphaned `ppid=1` helper capture)
//! 4. Stale lockfile (`lockfile` JSON + `SingletonLock` + `.runtime-pid` + clean recovery)
//! 5. PID reuse (`(pid, starttime_ticks)` + `DIM_INSTANCE_NONCE` defense against recycled PIDs)
//! 6. Dangling symlinks (`SingletonLock` `symlink_metadata` + broken binary/profile symlink rejection)
//! 7. `HOME`/XDG isolation & hostile loader/runtime environment stripping
//! 8. File chooser (`GTK_USE_PORTAL=1`) & `Downloads`/`Documents`/`user-dirs.dirs` confinement
//! 9. Runtime socket permissions, system D-Bus / abstract socket rejection & minimal `bwrap` binds
//! 10. Honest `UNSUPPORTED_OPERATION` error on Wayland & Headless GUI focus
//! 11. Invalid & truncated `.AppImage` rejection across `inspect`, `preflight`, `discover`, and `launch`
//! 12. Zero-execution static ELF64 `PT_INTERP`/`DT_NEEDED` inspection & FIPS 180-4 SHA-256 boundary vectors
//! 13. Permission denial (`!is_executable`, `/proc/mounts` `noexec`, `0o777` `Profiles`, `UID == 0` root)
//! 14. Restart persistence (`launch_count == 2`, `zalo_user_id`, and session DB across stop/relaunch)

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
    OperationOutcome, PlatformCapabilities, ProfileRecord, ProfileRuntimeState, ProfileStore,
    SupportStatus, ZALO_PROFILE_ID_FILE,
};
use dim_platform_linux::{
    analyze_startup_script, build_bwrap_args, build_bwrap_desktop_args,
    build_bwrap_desktop_args_with_policy, build_sanitized_linux_spawn_env,
    build_sanitized_linux_spawn_env_with_nonce, classify_binary_bytes,
    collect_profile_process_tree, compare_linux_isolation_mechanisms,
    discover_compatible_apps_in_roots, enforce_launch_security_policy, ensure_linux_profile_layout,
    evaluate_appimage_preflight, find_mount_for_path, generate_instance_nonce,
    has_targeted_apparmor_userns_profile, inspect_elf64_needed_libraries_at, inspect_user_binary,
    is_safe_bindable_socket_or_file, linux_zalo_data_dir, parse_dbus_session_socket_path,
    parse_desktop_entry, parse_elf64_dynamic_metadata, parse_ldd_missing_libraries,
    parse_proc_mounts, parse_singleton_lock_target, parse_wmctrl_windows_for_pids,
    parse_x11_display_socket_path, plan_bwrap_desktop_sandbox, probe_ubuntu_runtime_at,
    read_elf64_dynamic_metadata, read_proc_entry, read_runtime_pid_hint,
    render_recommended_apparmor_profile, resolve_elf64_missing_libraries_at,
    resolve_pipewire_socket_path, resolve_pulse_socket_path, resolve_wayland_socket_path,
    runtime_hint_matches_entry, sha256_bytes, sha256_file, verify_directory_ownership_and_mode,
    BinaryFormatKind, BwrapSandboxOptions, DesktopSocketSurface, Elf64ParseError, ElfCpuArch,
    LauncherLifetimePolicy, LinuxIsolationMechanism, LinuxPlatformAdapter, PreflightCategory,
    PreflightDiagnosticKind, PreflightSeverity, SecurityBlockReason, UbuntuTargetTier,
    VerificationLabel, DIM_INSTANCE_NONCE_ENV, DIM_PROFILE_ID_ENV, KNOWN_COMMUNITY_RELEASES,
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
        .delete_profile_with_guard(
            &created.id,
            true,
            dim_core::VerifiedDeletionState::ConfirmedStopped,
            &|d| adapter.unlink_profile_junctions(d),
        )
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
        .delete_profile_with_guard(
            &created.id,
            true,
            dim_core::VerifiedDeletionState::ConfirmedStopped,
            &|d| adapter.unlink_profile_junctions(d),
        )
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

// ===========================================================================
// P1 REGRESSION & SECURITY SUITE EXPANSION (14 REQUIRED SCENARIOS)
// ===========================================================================

struct QuitSignalGuard {
    zalo_data_dir: PathBuf,
}

impl Drop for QuitSignalGuard {
    fn drop(&mut self) {
        if self.zalo_data_dir.join("lockfile").exists()
            || fs::symlink_metadata(self.zalo_data_dir.join("SingletonLock")).is_ok()
        {
            let _ = fs::write(
                self.zalo_data_dir.join(".quit-signal"),
                "{\"requested_at\":\"drop-cleanup\"}\n",
            );
            let deadline = Instant::now() + Duration::from_millis(1_500);
            while Instant::now() < deadline {
                if !self.zalo_data_dir.join("lockfile").exists() {
                    break;
                }
                thread::sleep(Duration::from_millis(25));
            }
        }
    }
}

fn wait_for_file_absent(path: &Path, timeout_ms: u64) -> bool {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    while Instant::now() < deadline {
        if fs::symlink_metadata(path).is_err() {
            return true;
        }
        thread::sleep(Duration::from_millis(25));
    }
    fs::symlink_metadata(path).is_err()
}

#[allow(clippy::too_many_arguments)]
fn write_fake_proc_entry_with_starttime(
    proc_root: &Path,
    pid: u32,
    comm: &str,
    state: char,
    ppid: u32,
    starttime_ticks: u64,
    uid: u32,
    rss_kib: u64,
    environ: Option<&[(&str, &str)]>,
    cmdline: &[&str],
) {
    let dir = proc_root.join(pid.to_string());
    fs::create_dir_all(&dir).unwrap();
    let stat_line = format!(
        "{pid} ({comm}) {state} {ppid} {pid} {pid} 0 -1 4194304 100 0 0 0 10 5 0 0 20 0 1 0 {starttime_ticks} 10485760 256 18446744073709551615\n"
    );
    fs::write(dir.join("stat"), stat_line).unwrap();
    let status_text = format!(
        "Name:\t{comm}\nState:\t{state}\nTgid:\t{pid}\nPid:\t{pid}\nPPid:\t{ppid}\nUid:\t{uid}\t{uid}\t{uid}\t{uid}\nVmRSS:\t{rss_kib} kB\n"
    );
    fs::write(dir.join("status"), status_text).unwrap();

    if let Some(pairs) = environ {
        let mut env_bytes = Vec::new();
        for (k, v) in pairs {
            env_bytes.extend_from_slice(format!("{k}={v}").as_bytes());
            env_bytes.push(0);
        }
        fs::write(dir.join("environ"), env_bytes).unwrap();
    } else {
        let _ = fs::remove_file(dir.join("environ"));
    }

    let mut cmd_bytes = Vec::new();
    for arg in cmdline {
        cmd_bytes.extend_from_slice(arg.as_bytes());
        cmd_bytes.push(0);
    }
    fs::write(dir.join("cmdline"), cmd_bytes).unwrap();
}

/// Builds a spec-accurate little-endian ELF64 dynamic executable in memory with `PT_LOAD`
/// (`vaddr = 0x0040_1200 != file_offset = 0x200`, `p_memsz = p_filesz + 0x2000` BSS tail),
/// optional `PT_INTERP`, and `PT_DYNAMIC` (`DT_STRTAB`, `DT_STRSZ`, `DT_SONAME`, `DT_NEEDED`,
/// `DT_RPATH`, `DT_RUNPATH`).
fn build_synthetic_elf64_dynamic_binary(
    machine: u16,
    appimage_type2: bool,
    interp: Option<&str>,
    soname: Option<&str>,
    needed: &[&str],
    rpaths: &[&str],
    runpaths: &[&str],
) -> Vec<u8> {
    const PT_LOAD_TAG: u32 = 1;
    const PT_DYNAMIC_TAG: u32 = 2;
    const PT_INTERP_TAG: u32 = 3;
    const DT_NULL_TAG: u64 = 0;
    const DT_NEEDED_TAG: u64 = 1;
    const DT_STRTAB_TAG: u64 = 5;
    const DT_STRSZ_TAG: u64 = 10;
    const DT_SONAME_TAG: u64 = 14;
    const DT_RPATH_TAG: u64 = 15;
    const DT_RUNPATH_TAG: u64 = 29;

    let mut dynstr = vec![0u8];
    let push_str = |table: &mut Vec<u8>, s: &str| -> u64 {
        let off = table.len() as u64;
        table.extend_from_slice(s.as_bytes());
        table.push(0);
        off
    };

    let soname_off = soname.map(|s| push_str(&mut dynstr, s));
    let needed_offs: Vec<u64> = needed.iter().map(|s| push_str(&mut dynstr, s)).collect();
    let rpath_offs: Vec<u64> = rpaths.iter().map(|s| push_str(&mut dynstr, s)).collect();
    let runpath_offs: Vec<u64> = runpaths.iter().map(|s| push_str(&mut dynstr, s)).collect();

    let load_file_off: u64 = 0x200;
    let load_base_vaddr: u64 = 0x0040_1200;

    let mut payload = Vec::new();
    let mut interp_range = None;
    if let Some(ip) = interp {
        let start = payload.len() as u64;
        payload.extend_from_slice(ip.as_bytes());
        payload.push(0);
        interp_range = Some((load_file_off + start, payload.len() as u64 - start));
    }
    while payload.len() % 8 != 0 {
        payload.push(0);
    }

    let dynstr_rel = payload.len() as u64;
    let dynstr_vaddr = load_base_vaddr + dynstr_rel;
    let dynstr_sz = dynstr.len() as u64;
    payload.extend_from_slice(&dynstr);
    while payload.len() % 8 != 0 {
        payload.push(0);
    }

    let dyn_rel = payload.len() as u64;
    let mut push_dyn = |tag: u64, val: u64| {
        payload.extend_from_slice(&tag.to_le_bytes());
        payload.extend_from_slice(&val.to_le_bytes());
    };
    push_dyn(DT_STRTAB_TAG, dynstr_vaddr);
    push_dyn(DT_STRSZ_TAG, dynstr_sz);
    if let Some(off) = soname_off {
        push_dyn(DT_SONAME_TAG, off);
    }
    for off in needed_offs {
        push_dyn(DT_NEEDED_TAG, off);
    }
    for off in rpath_offs {
        push_dyn(DT_RPATH_TAG, off);
    }
    for off in runpath_offs {
        push_dyn(DT_RUNPATH_TAG, off);
    }
    push_dyn(DT_NULL_TAG, 0);
    let dyn_sz = payload.len() as u64 - dyn_rel;

    let load_filesz = payload.len() as u64;
    let make_phdr = |p_type: u32, p_flags: u32, off: u64, vaddr: u64, filesz: u64, memsz: u64| {
        let mut p = [0u8; 56];
        p[0..4].copy_from_slice(&p_type.to_le_bytes());
        p[4..8].copy_from_slice(&p_flags.to_le_bytes());
        p[8..16].copy_from_slice(&off.to_le_bytes());
        p[16..24].copy_from_slice(&vaddr.to_le_bytes());
        p[24..32].copy_from_slice(&vaddr.to_le_bytes());
        p[32..40].copy_from_slice(&filesz.to_le_bytes());
        p[40..48].copy_from_slice(&memsz.to_le_bytes());
        p[48..56].copy_from_slice(&0x1000u64.to_le_bytes());
        p
    };

    let mut phdrs = vec![make_phdr(
        PT_LOAD_TAG,
        5,
        load_file_off,
        load_base_vaddr,
        load_filesz,
        load_filesz + 0x2000,
    )];
    if let Some((ioff, isz)) = interp_range {
        phdrs.push(make_phdr(
            PT_INTERP_TAG,
            4,
            ioff,
            load_base_vaddr + (ioff - load_file_off),
            isz,
            isz,
        ));
    }
    phdrs.push(make_phdr(
        PT_DYNAMIC_TAG,
        6,
        load_file_off + dyn_rel,
        load_base_vaddr + dyn_rel,
        dyn_sz,
        dyn_sz,
    ));

    let mut file = vec![0u8; load_file_off as usize];
    file[0..4].copy_from_slice(b"\x7fELF");
    file[4] = 2; // ELFCLASS64
    file[5] = 1; // ELFDATA2LSB
    file[6] = 1; // EV_CURRENT
    if appimage_type2 {
        file[8..11].copy_from_slice(b"AI\x02");
    }
    file[16..18].copy_from_slice(&3u16.to_le_bytes()); // ET_DYN (PIE)
    file[18..20].copy_from_slice(&machine.to_le_bytes());
    file[20..24].copy_from_slice(&1u32.to_le_bytes());
    file[32..40].copy_from_slice(&64u64.to_le_bytes()); // e_phoff = 64
    file[52..54].copy_from_slice(&64u16.to_le_bytes()); // e_ehsize = 64
    file[54..56].copy_from_slice(&56u16.to_le_bytes()); // e_phentsize = 56
    file[56..58].copy_from_slice(&(phdrs.len() as u16).to_le_bytes());

    for (i, ph) in phdrs.iter().enumerate() {
        let start = 64 + i * 56;
        file[start..start + 56].copy_from_slice(ph);
    }
    file.extend_from_slice(&payload);
    file
}

fn deterministic_pattern_bytes(len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| ((i.wrapping_mul(31).wrapping_add(17)) & 0xFF) as u8)
        .collect()
}

// ---------------------------------------------------------------------------
// Regression 1. Launcher exit while detached child remains alive
// ---------------------------------------------------------------------------
#[test]
fn regression_01_launcher_exit_while_detached_child_remains_alive() {
    let tmp = tempfile::tempdir().unwrap();
    let store = JsonProfileStore::new(tmp.path().join("DetachedStore")).unwrap();
    let p = store.create_profile("Detached Survivor", "").unwrap();
    let pdir = store.profile_dir_for(&p).unwrap();
    ensure_linux_profile_layout(&p, &pdir).unwrap();
    let zdir = linux_zalo_data_dir(&pdir);
    let _cleanup = QuitSignalGuard {
        zalo_data_dir: zdir.clone(),
    };

    // 1a. Verify PersistentDetachedSession policy generates `--new-session` WITHOUT `--die-with-parent`.
    let adapter = LinuxPlatformAdapter::new();
    assert_eq!(
        adapter.lifetime_policy(),
        LauncherLifetimePolicy::PersistentDetachedSession
    );
    assert!(LauncherLifetimePolicy::PersistentDetachedSession.detaches_from_launcher());
    assert_eq!(
        LauncherLifetimePolicy::PersistentDetachedSession.as_str(),
        "persistent-detached-session"
    );

    let nonce = generate_instance_nonce();
    assert_eq!(nonce.len(), 32);

    let bwrap_default = build_bwrap_args(&pdir, Path::new("/opt/zalo/Zalo.AppImage"), &[]);
    assert!(
        bwrap_default.contains(&"--new-session".to_owned()),
        "PersistentDetachedSession must emit --new-session"
    );
    assert!(
        !bwrap_default.contains(&"--die-with-parent".to_owned()),
        "PersistentDetachedSession must NEVER emit --die-with-parent"
    );

    let bwrap_policy = build_bwrap_desktop_args_with_policy(
        &pdir,
        Path::new("/opt/zalo/Zalo.AppImage"),
        &[],
        LauncherLifetimePolicy::PersistentDetachedSession,
        Some(&p.id),
        Some(&nonce),
        &|_| None,
    );
    assert!(bwrap_policy.contains(&"--new-session".to_owned()));
    assert!(!bwrap_policy.contains(&"--die-with-parent".to_owned()));

    let plan = adapter.plan_sandbox(
        &p,
        &pdir,
        Path::new("/opt/zalo/Zalo.AppImage"),
        &[],
        Some(&nonce),
    );
    assert_eq!(
        plan.lifetime_policy,
        LauncherLifetimePolicy::PersistentDetachedSession
    );
    assert!(plan.args.contains(&"--new-session".to_owned()));
    assert!(!plan.args.contains(&"--die-with-parent".to_owned()));

    // 1b. Spawn a short-lived intermediate launcher process (`node -e ...`) that spawns
    //     `fixture-app.mjs` detached (`detached: true` + `unref()`) and exits immediately.
    let meta_path = pdir.join(".launcher-exit-meta.json");
    let script = fixture_script_path();
    let spawn_env =
        build_sanitized_linux_spawn_env_with_nonce(&p, &pdir, Some(&nonce), true, &|k| {
            if k == "PATH" {
                std::env::var("PATH").ok()
            } else {
                None
            }
        });

    let launcher_js = r#"
const { spawn } = require('node:child_process');
const fs = require('node:fs');
const [, fixtureScript, metaFile, userId] = process.argv;
const child = spawn(
  process.execPath,
  [fixtureScript, `--user-id=${userId}`, '--stay-ms=15000'],
  { detached: true, stdio: 'ignore', env: process.env }
);
child.unref();
fs.writeFileSync(
  metaFile,
  JSON.stringify({ launcher_pid: process.pid, grandchild_pid: child.pid }) + '\n',
  'utf8'
);
process.exit(0);
"#;

    let mut cmd = Command::new("node");
    cmd.arg("-e")
        .arg(launcher_js)
        .arg(&script)
        .arg(&meta_path)
        .arg("849801001")
        .env_clear()
        .envs(&spawn_env)
        .current_dir(&pdir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Ok(sysroot) = std::env::var("SYSTEMROOT") {
        cmd.env("SYSTEMROOT", sysroot);
    }

    let mut launcher_child = cmd.spawn().expect("intermediate launcher must spawn");
    let launcher_os_pid = launcher_child.id();
    let launcher_status = launcher_child
        .wait()
        .expect("intermediate launcher must exit cleanly");
    assert!(
        launcher_status.success(),
        "short-lived launcher must exit with status 0"
    );

    assert!(
        wait_for_file(&meta_path, 4_000),
        "intermediate launcher must write metadata before exit"
    );
    let meta_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&meta_path).unwrap()).unwrap();
    let recorded_launcher_pid = meta_json["launcher_pid"].as_u64().unwrap() as u32;
    let grandchild_pid = meta_json["grandchild_pid"].as_u64().unwrap() as u32;
    assert_eq!(recorded_launcher_pid, launcher_os_pid);
    assert_ne!(grandchild_pid, launcher_os_pid);

    let lockfile = zdir.join("lockfile");
    assert!(
        wait_for_file(&lockfile, 5_000) && wait_for_file(&zdir.join("launch-state.json"), 5_000),
        "detached grandchild must acquire lockfile and initialize launch-state.json"
    );

    // Wait 250ms AFTER intermediate launcher exit so any broken PDEATHSIG/SIGHUP/job kill manifests.
    thread::sleep(Duration::from_millis(250));

    let lock_val: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lockfile).unwrap()).unwrap();
    assert_eq!(lock_val["pid"].as_u64(), Some(u64::from(grandchild_pid)));
    assert_eq!(lock_val["profile_id"].as_str(), Some(p.id.as_str()));

    // Prove grandchild event loop is genuinely alive AFTER launcher exit via roundtrip `.focus-signal`.
    fs::write(
        zdir.join(".focus-signal"),
        "{\"focused_at\":\"2026-11-18T00:00:00Z\"}\n",
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(4);
    let mut focus_observed = false;
    while Instant::now() < deadline {
        if let Ok(raw) = fs::read_to_string(zdir.join("launch-state.json")) {
            if let Ok(st) = serde_json::from_str::<serde_json::Value>(&raw) {
                if st["focus_count"].as_u64().unwrap_or(0) >= 1 {
                    focus_observed = true;
                    break;
                }
            }
        }
        thread::sleep(Duration::from_millis(25));
    }
    assert!(
        focus_observed,
        "detached grandchild must remain alive and process .focus-signal after intermediate launcher exited"
    );

    // Cleanly stop grandchild via `.quit-signal`.
    fs::write(zdir.join(".quit-signal"), "{\"requested_at\":\"now\"}\n").unwrap();
    assert!(
        wait_for_file_absent(&lockfile, 5_000),
        "detached grandchild must exit cleanly and unlink lockfile on .quit-signal"
    );
    let final_state: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(zdir.join("launch-state.json")).unwrap()).unwrap();
    assert_eq!(final_state["clean_shutdown"], true);
}

// ---------------------------------------------------------------------------
// Regression 2. Manager graceful shutdown & virtual /proc bwrap tree ordering
// ---------------------------------------------------------------------------
#[test]
fn regression_02_manager_graceful_shutdown_and_bwrap_tree_ordering() {
    let tmp = tempfile::tempdir().unwrap();
    let store = JsonProfileStore::new(tmp.path().join("GracefulTreeStore")).unwrap();
    let p_stop = store.create_profile("Stop Me", "").unwrap();
    let p_keep = store.create_profile("Keep Running", "").unwrap();
    let dir_stop = store.profile_dir_for(&p_stop).unwrap();
    let dir_keep = store.profile_dir_for(&p_keep).unwrap();
    let z_stop = linux_zalo_data_dir(&dir_stop);
    let z_keep = linux_zalo_data_dir(&dir_keep);

    // 2a. Real multi-instance graceful shutdown leaving sibling actively responsive.
    let mut guard_stop = spawn_linux_env_fixture(
        &p_stop,
        &dir_stop,
        &["--user-id=849802001", "--stay-ms=12000"],
    );
    let _guard_keep = spawn_linux_env_fixture(
        &p_keep,
        &dir_keep,
        &["--user-id=849802002", "--stay-ms=12000"],
    );
    assert!(wait_for_file(&z_stop.join("lockfile"), 4_000));
    assert!(wait_for_file(&z_stop.join("launch-state.json"), 4_000));
    assert!(wait_for_file(&z_keep.join("lockfile"), 4_000));
    assert!(wait_for_file(&z_keep.join("launch-state.json"), 4_000));

    fs::write(z_stop.join(".quit-signal"), "{\"requested_at\":\"now\"}\n").unwrap();
    if let Some(ref mut c) = guard_stop.0 {
        let deadline = Instant::now() + Duration::from_secs(4);
        while Instant::now() < deadline && c.try_wait().unwrap().is_none() {
            thread::sleep(Duration::from_millis(25));
        }
        assert!(
            c.try_wait().unwrap().is_some(),
            "target profile must exit cleanly"
        );
    }
    assert!(!z_stop.join("lockfile").exists());
    let stop_state: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(z_stop.join("launch-state.json")).unwrap())
            .unwrap();
    assert_eq!(stop_state["clean_shutdown"], true);

    // Prove sibling is genuinely alive (not merely leaving a stale file) by roundtripping `.focus-signal`.
    fs::write(
        z_keep.join(".focus-signal"),
        "{\"focused_at\":\"2026-11-18T00:01:00Z\"}\n",
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(4);
    let mut keep_focused = false;
    while Instant::now() < deadline {
        if let Ok(raw) = fs::read_to_string(z_keep.join("launch-state.json")) {
            if let Ok(st) = serde_json::from_str::<serde_json::Value>(&raw) {
                if st["focus_count"].as_u64().unwrap_or(0) >= 1 {
                    keep_focused = true;
                    break;
                }
            }
        }
        thread::sleep(Duration::from_millis(25));
    }
    assert!(
        keep_focused,
        "sibling profile must remain alive and responsive after target profile shutdown"
    );
    fs::write(z_keep.join(".quit-signal"), "{\"requested_at\":\"now\"}\n").unwrap();
    assert!(wait_for_file_absent(&z_keep.join("lockfile"), 4_000));

    #[cfg(unix)]
    {
        // On Unix, also verify direct SIGTERM triggers clean shutdown & lock cleanup.
        let p_sig = store.create_profile("Sigterm Target", "").unwrap();
        let dir_sig = store.profile_dir_for(&p_sig).unwrap();
        let z_sig = linux_zalo_data_dir(&dir_sig);
        let mut guard_sig = spawn_linux_env_fixture(
            &p_sig,
            &dir_sig,
            &["--user-id=849802003", "--stay-ms=12000"],
        );
        assert!(wait_for_file(&z_sig.join("lockfile"), 4_000));
        assert!(wait_for_file(&z_sig.join("startup.log"), 4_000));
        thread::sleep(Duration::from_millis(60));
        if let Some(ref mut c) = guard_sig.0 {
            let pid = libc::pid_t::try_from(c.id()).unwrap();
            // SAFETY: Sends SIGTERM to our own freshly spawned fixture child process.
            unsafe {
                assert_eq!(libc::kill(pid, libc::SIGTERM), 0);
            }
            let status = c.wait().unwrap();
            assert!(status.success(), "fixture must exit 0 on SIGTERM");
        }
        assert!(!z_sig.join("lockfile").exists());
        assert!(fs::symlink_metadata(z_sig.join("SingletonLock")).is_err());
        let sig_state: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(z_sig.join("launch-state.json")).unwrap())
                .unwrap();
        assert_eq!(sig_state["clean_shutdown"], true);
    }

    // 2b. Virtual `/proc` outer-`bwrap` + inner-Electron + zygote + renderer + reparented helper ordering,
    //     exercising BOTH `dumpable=0` (unreadable environ) AND `dumpable=1` (non-empty host HOME in bwrap environ).
    for dumpable_host_env in [false, true] {
        let vproc = tmp.path().join(format!("proc_bwrap_{dumpable_host_env}"));
        fs::create_dir_all(&vproc).unwrap();
        let p_tree = store
            .create_profile(&format!("Tree_{dumpable_host_env}"), "")
            .unwrap();
        let dir_tree = store.profile_dir_for(&p_tree).unwrap();
        ensure_linux_profile_layout(&p_tree, &dir_tree).unwrap();
        let z_tree = linux_zalo_data_dir(&dir_tree);

        let nonce = generate_instance_nonce();
        fs::write(
            dir_tree.join(".runtime-pid"),
            serde_json::json!({
                "pid": 2000,
                "profile_id": p_tree.id,
                "instance_nonce": nonce,
                "starttime_ticks": 41_000_u64,
                "lifetime_policy": "persistent-detached-session",
                "uses_bwrap": true,
            })
            .to_string(),
        )
        .unwrap();
        fs::write(z_tree.join("lockfile"), "{\"pid\":2001}\n").unwrap();

        let dir_tree_str = dir_tree.to_string_lossy().into_owned();
        let host_env_slice: &[(&str, &str)] = &[
            ("HOME", "/home/hostuser"),
            ("XDG_CONFIG_HOME", "/home/hostuser/.config"),
        ];
        write_fake_proc_entry_with_starttime(
            &vproc,
            2000,
            "bwrap",
            'S',
            1,
            41_000,
            1000,
            3_500,
            if dumpable_host_env {
                Some(host_env_slice)
            } else {
                None
            },
            &[
                "bwrap",
                "--new-session",
                "--bind",
                &dir_tree_str,
                &dir_tree_str,
                "--setenv",
                DIM_PROFILE_ID_ENV,
                &p_tree.id,
                "--setenv",
                DIM_INSTANCE_NONCE_ENV,
                &nonce,
                "--",
                "/opt/zalo/zalo",
            ],
        );
        write_fake_proc_entry_with_starttime(
            &vproc,
            2001,
            "zalo",
            'S',
            2000,
            41_005,
            1000,
            120_000,
            Some(&[
                (DIM_PROFILE_ID_ENV, &p_tree.id),
                (DIM_INSTANCE_NONCE_ENV, &nonce),
            ]),
            &["/opt/zalo/zalo"],
        );
        write_fake_proc_entry_with_starttime(
            &vproc,
            2002,
            "zalo (zygote)",
            'S',
            2001,
            41_010,
            1000,
            32_000,
            None,
            &["/opt/zalo/zalo", "--type=zygote"],
        );
        write_fake_proc_entry_with_starttime(
            &vproc,
            2003,
            "zalo (renderer)",
            'R',
            2002,
            41_015,
            1000,
            64_000,
            None,
            &["/opt/zalo/zalo", "--type=renderer"],
        );
        write_fake_proc_entry_with_starttime(
            &vproc,
            2099,
            "zocr-host",
            'S',
            1,
            41_020,
            1000,
            18_000,
            Some(&[
                (DIM_PROFILE_ID_ENV, &p_tree.id),
                (DIM_INSTANCE_NONCE_ENV, &nonce),
            ]),
            &["zocr-host"],
        );

        let tree = collect_profile_process_tree(&vproc, &p_tree.id, &dir_tree, &z_tree, Some(1000));
        // Phase-1 graceful SIGTERM must target inner Electron `2001` (not outer `bwrap` `2000`).
        assert_eq!(tree.main_pid, Some(2001));
        assert_eq!(tree.process_ids.len(), 5);
        // Outer `bwrap` (`2000`) must be index 0 so reverse escalation (`iter().rev()`) kills leaves
        // (`renderer` 2003 -> `zygote` 2002 -> `zalo` 2001) BEFORE terminating outer `bwrap` `2000`!
        let pos_bwrap = tree.process_ids.iter().position(|&p| p == 2000).unwrap();
        let pos_main = tree.process_ids.iter().position(|&p| p == 2001).unwrap();
        let pos_zygote = tree.process_ids.iter().position(|&p| p == 2002).unwrap();
        let pos_renderer = tree.process_ids.iter().position(|&p| p == 2003).unwrap();
        assert_eq!(pos_bwrap, 0);
        assert!(pos_main < pos_zygote);
        assert!(pos_zygote < pos_renderer);

        let rev_kill_order: Vec<u32> = tree.process_ids.iter().copied().rev().collect();
        assert_eq!(
            *rev_kill_order.last().unwrap(),
            2000,
            "outer bwrap must be terminated LAST during reverse SIGKILL escalation"
        );
        assert_eq!(tree.process_starttimes.get(&2000), Some(&41_000));
        assert_eq!(tree.process_starttimes.get(&2001), Some(&41_005));
        assert_eq!(tree.process_starttimes.get(&2003), Some(&41_015));
    }
}

// ---------------------------------------------------------------------------
// Regression 3. Unexpected parent termination & reparented orphan capture
// ---------------------------------------------------------------------------
#[test]
fn regression_03_unexpected_parent_termination_and_orphan_capture() {
    let tmp = tempfile::tempdir().unwrap();
    let fs_root = tmp.path().join("fs");
    let proc_root = tmp.path().join("proc");
    let primary_cfg = tmp.path().join("primary");
    fs::create_dir_all(&fs_root).unwrap();
    fs::create_dir_all(&proc_root).unwrap();
    fs::create_dir_all(&primary_cfg).unwrap();

    let p = ProfileRecord {
        id: "3300aabb".to_owned(),
        name: "Orphan Recovery".to_owned(),
        directory_name: "orphan_3300aabb".to_owned(),
        created_at: "2026-11-18T00:00:00Z".to_owned(),
        last_launched_at: None,
        zalo_user_id: None,
        notes: String::new(),
    };
    let pdir = tmp.path().join(&p.directory_name);
    ensure_linux_profile_layout(&p, &pdir).unwrap();
    let zdir = linux_zalo_data_dir(&pdir);

    // 3a. Verify `LauncherLifetimePolicy::DieWithParent` emits `--die-with-parent` and omits `--new-session`.
    assert!(!LauncherLifetimePolicy::DieWithParent.detaches_from_launcher());
    assert_eq!(
        LauncherLifetimePolicy::DieWithParent.as_str(),
        "die-with-parent"
    );
    let nonce = generate_instance_nonce();
    let die_adapter = LinuxPlatformAdapter::with_virtual_roots(
        fs_root.clone(),
        proc_root.clone(),
        primary_cfg.clone(),
        1000,
        "x86_64",
        None,
    )
    .with_lifetime_policy(LauncherLifetimePolicy::DieWithParent);
    assert_eq!(
        die_adapter.lifetime_policy(),
        LauncherLifetimePolicy::DieWithParent
    );
    let die_plan = die_adapter.plan_sandbox(
        &p,
        &pdir,
        Path::new("/opt/zalo/Zalo.AppImage"),
        &[],
        Some(&nonce),
    );
    assert!(die_plan.args.contains(&"--die-with-parent".to_owned()));
    assert!(!die_plan.args.contains(&"--new-session".to_owned()));

    let die_args = build_bwrap_desktop_args_with_policy(
        &pdir,
        Path::new("/opt/zalo/Zalo.AppImage"),
        &[],
        LauncherLifetimePolicy::DieWithParent,
        Some(&p.id),
        Some(&nonce),
        &|_| None,
    );
    assert!(die_args.contains(&"--die-with-parent".to_owned()));
    assert!(!die_args.contains(&"--new-session".to_owned()));

    // 3b. Simulate unexpected parent termination: outer launcher PID 3100 recorded in `.runtime-pid`
    //     died abruptly (absent from `/proc`), while reparented child `3105` (`ppid=1`), its renderer
    //     descendant `3106` (`ppid=3105`), and reparented helper `3199` (`ppid=1`, `zcall-native`)
    //     remain alive with `DIM_PROFILE_ID` + matching `DIM_INSTANCE_NONCE`.
    fs::write(
        pdir.join(".runtime-pid"),
        serde_json::json!({
            "pid": 3100,
            "profile_id": p.id,
            "instance_nonce": nonce,
            "starttime_ticks": 70_000_u64,
        })
        .to_string(),
    )
    .unwrap();
    fs::write(zdir.join("lockfile"), "{\"pid\":3105}\n").unwrap();

    write_fake_proc_entry_with_starttime(
        &proc_root,
        3105,
        "zalo",
        'S',
        1, // reparented to init/systemd (ppid=1)!
        70_010,
        1000,
        95_000,
        Some(&[
            (DIM_PROFILE_ID_ENV, &p.id),
            (DIM_INSTANCE_NONCE_ENV, &nonce),
        ]),
        &["/opt/zalo/zalo"],
    );
    write_fake_proc_entry_with_starttime(
        &proc_root,
        3106,
        "zalo (renderer)",
        'S',
        3105,
        70_015,
        1000,
        55_000,
        None,
        &["/opt/zalo/zalo", "--type=renderer"],
    );
    write_fake_proc_entry_with_starttime(
        &proc_root,
        3199,
        "zcall-native",
        'S',
        1, // reparented orphan helper (ppid=1)!
        70_020,
        1000,
        24_000,
        Some(&[
            (DIM_PROFILE_ID_ENV, &p.id),
            (DIM_INSTANCE_NONCE_ENV, &nonce),
        ]),
        &["zcall-native"],
    );
    // Stale orphan from a DIFFERENT nonce must NOT be captured!
    write_fake_proc_entry_with_starttime(
        &proc_root,
        3200,
        "zocr-host",
        'S',
        1,
        10_000,
        1000,
        12_000,
        Some(&[
            (DIM_PROFILE_ID_ENV, &p.id),
            (DIM_INSTANCE_NONCE_ENV, "stale_foreign_nonce_9999"),
        ]),
        &["zocr-host"],
    );

    let tree = collect_profile_process_tree(&proc_root, &p.id, &pdir, &zdir, Some(1000));
    assert_eq!(tree.main_pid, Some(3105));
    assert!(tree.process_ids.contains(&3105));
    assert!(tree.process_ids.contains(&3106));
    assert!(tree.process_ids.contains(&3199));
    assert!(!tree.process_ids.contains(&3100));
    assert!(
        !tree.process_ids.contains(&3200),
        "mismatched-nonce orphan must be excluded"
    );

    let rt = die_adapter.inspect_profile(&p, &pdir).unwrap();
    assert!(rt.running);
    assert!(!rt.stale_lock_detected);
    assert_eq!(rt.main_pid, Some(3105));
    assert_eq!(rt.process_count, 3);
    assert_eq!(rt.working_set_bytes, (95_000 + 55_000 + 24_000) * 1024);
}

// ---------------------------------------------------------------------------
// Regression 4. Stale lockfile (`lockfile` JSON + `SingletonLock` + clean recovery)
// ---------------------------------------------------------------------------
#[test]
fn regression_04_stale_lockfile_and_singleton_lock_recovery() {
    let tmp = tempfile::tempdir().unwrap();
    let fs_root = tmp.path().join("fs");
    let proc_root = tmp.path().join("proc");
    let primary_cfg = tmp.path().join("primary");
    fs::create_dir_all(&fs_root).unwrap();
    fs::create_dir_all(&proc_root).unwrap();
    fs::create_dir_all(&primary_cfg).unwrap();

    let store = JsonProfileStore::new(tmp.path().join("StaleLockStore")).unwrap();
    let p = store.create_profile("Stale Recovery", "").unwrap();
    let pdir = store.profile_dir_for(&p).unwrap();
    ensure_linux_profile_layout(&p, &pdir).unwrap();
    let zdir = linux_zalo_data_dir(&pdir);
    let _cleanup = QuitSignalGuard {
        zalo_data_dir: zdir.clone(),
    };

    let adapter = LinuxPlatformAdapter::with_virtual_roots(
        fs_root,
        proc_root,
        primary_cfg,
        1000,
        "x86_64",
        None,
    );

    // 4a. Clean stopped profile has neither running processes nor stale lock.
    let initial = adapter.inspect_profile(&p, &pdir).unwrap();
    assert!(!initial.running);
    assert!(!initial.stale_lock_detected);
    assert_eq!(initial.status_label, "Stopped");

    // 4b. Dead-PID `lockfile` JSON alone triggers stale lock detection.
    fs::write(
        zdir.join("lockfile"),
        "{\"pid\":999888,\"profile_id\":\"dead\"}\n",
    )
    .unwrap();
    let st_lockfile = adapter.inspect_profile(&p, &pdir).unwrap();
    assert!(!st_lockfile.running);
    assert!(st_lockfile.stale_lock_detected);
    assert_eq!(st_lockfile.main_pid, None);
    assert_eq!(st_lockfile.status_label, "Stopped (Stale Lock)");
    assert_eq!(st_lockfile.lockfile_path, zdir.join("lockfile"));

    // 4c. Add dead-PID `SingletonLock` alongside dead `lockfile`.
    #[cfg(unix)]
    std::os::unix::fs::symlink("ubuntu-24-04-999888", zdir.join("SingletonLock")).unwrap();
    #[cfg(not(unix))]
    fs::write(zdir.join("SingletonLock"), "ubuntu-24-04-999888").unwrap();

    let st_both = adapter.inspect_profile(&p, &pdir).unwrap();
    assert!(!st_both.running);
    assert!(st_both.stale_lock_detected);
    assert_eq!(st_both.lockfile_path, zdir.join("SingletonLock"));

    #[cfg(not(unix))]
    fs::remove_file(zdir.join("SingletonLock")).unwrap();

    // 4d. Spawn real `fixture-app.mjs` over the stale lockfile and prove automatic recovery + clean exit.
    let mut recovered =
        spawn_linux_env_fixture(&p, &pdir, &["--user-id=849804004", "--stay-ms=8000"]);
    let live_pid = recovered.0.as_ref().unwrap().id();
    let deadline = Instant::now() + Duration::from_secs(4);
    let mut lock_swapped = false;
    let mut startup_log_recovered = false;
    while Instant::now() < deadline {
        if let Ok(raw) = fs::read_to_string(zdir.join("lockfile")) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                if v["pid"].as_u64() == Some(u64::from(live_pid)) {
                    lock_swapped = true;
                }
            }
        }
        if let Ok(log_raw) = fs::read_to_string(zdir.join("startup.log")) {
            if log_raw.contains("STALE_LOCK_RECOVERED") && zdir.join("launch-state.json").is_file()
            {
                startup_log_recovered = true;
            }
        }
        if lock_swapped && startup_log_recovered {
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }
    assert!(
        lock_swapped,
        "fixture must replace dead-PID lockfile with its own live PID"
    );
    assert!(
        startup_log_recovered,
        "startup.log must record STALE_LOCK_RECOVERED"
    );

    fs::write(zdir.join(".quit-signal"), "{\"requested_at\":\"now\"}\n").unwrap();
    if let Some(ref mut c) = recovered.0 {
        let _ = c.wait().unwrap();
    }
    assert!(!zdir.join("lockfile").exists());
    assert!(fs::symlink_metadata(zdir.join("SingletonLock")).is_err());

    let after_clean = adapter.inspect_profile(&p, &pdir).unwrap();
    assert!(!after_clean.running);
    assert!(!after_clean.stale_lock_detected);
    assert_eq!(after_clean.status_label, "Stopped");
}

// ---------------------------------------------------------------------------
// Regression 5. PID reuse (`(pid, starttime_ticks)` + `DIM_INSTANCE_NONCE`)
// ---------------------------------------------------------------------------
#[test]
fn regression_05_pid_reuse_starttime_ticks_and_nonce_defense() {
    let tmp = tempfile::tempdir().unwrap();
    let fs_root = tmp.path().join("fs");
    let proc_root = tmp.path().join("proc");
    let primary_cfg = tmp.path().join("primary");
    fs::create_dir_all(&fs_root).unwrap();
    fs::create_dir_all(&proc_root).unwrap();
    fs::create_dir_all(&primary_cfg).unwrap();

    let p = ProfileRecord {
        id: "5500ccdd".to_owned(),
        name: "PID Reuse Guard".to_owned(),
        directory_name: "pid-reuse_5500ccdd".to_owned(),
        created_at: "2026-11-18T00:00:00Z".to_owned(),
        last_launched_at: None,
        zalo_user_id: None,
        notes: String::new(),
    };
    let pdir = tmp.path().join(&p.directory_name);
    ensure_linux_profile_layout(&p, &pdir).unwrap();
    let zdir = linux_zalo_data_dir(&pdir);

    let orig_nonce = "11112222333344445555666677778888";
    fs::write(
        pdir.join(".runtime-pid"),
        serde_json::json!({
            "pid": 4500,
            "profile_id": p.id,
            "instance_nonce": orig_nonce,
            "starttime_ticks": 12_345_u64,
        })
        .to_string(),
    )
    .unwrap();
    fs::write(zdir.join("lockfile"), "{\"pid\":4500}\n").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("ubuntu-4500", zdir.join("SingletonLock")).unwrap();
    #[cfg(not(unix))]
    fs::write(zdir.join("SingletonLock"), "ubuntu-4500").unwrap();

    let hint = read_runtime_pid_hint(&pdir).expect(".runtime-pid must parse");
    assert_eq!(hint.pid, 4500);
    assert_eq!(hint.starttime_ticks, Some(12_345));
    assert_eq!(hint.instance_nonce.as_deref(), Some(orig_nonce));

    // 5a. Recycled PID 4500 with the EXACT SAME binary name (`zalo`) & UID (`1000`),
    //     but NEWER `starttime_ticks` (`67_890 != 12_345`)!
    let pdir_str = pdir.to_string_lossy().into_owned();
    write_fake_proc_entry_with_starttime(
        &proc_root,
        4500,
        "zalo",
        'S',
        1,
        67_890, // Newer boot ticks after PID wraparound!
        1000,
        110_000,
        Some(&[("HOME", &pdir_str)]),
        &["/opt/zalo/zalo"],
    );
    let reused_entry = read_proc_entry(&proc_root, 4500).unwrap();
    assert!(
        !runtime_hint_matches_entry(&hint, &reused_entry, &p.id, &pdir, Some(1000)),
        "newer starttime_ticks on recycled PID must be rejected even when binary name is identical"
    );

    let adapter = LinuxPlatformAdapter::with_virtual_roots(
        fs_root,
        proc_root.clone(),
        primary_cfg,
        1000,
        "x86_64",
        None,
    );
    let rt_ticks_mismatch = adapter.inspect_profile(&p, &pdir).unwrap();
    assert!(!rt_ticks_mismatch.running);
    assert!(rt_ticks_mismatch.stale_lock_detected);
    assert_eq!(rt_ticks_mismatch.main_pid, None);
    assert!(rt_ticks_mismatch.process_ids.is_empty());

    // 5b. Same `starttime_ticks` (`12_345`) and same `DIM_PROFILE_ID`, but MISMATCHED `DIM_INSTANCE_NONCE`!
    write_fake_proc_entry_with_starttime(
        &proc_root,
        4500,
        "zalo",
        'S',
        1,
        12_345,
        1000,
        110_000,
        Some(&[
            (DIM_PROFILE_ID_ENV, &p.id),
            (DIM_INSTANCE_NONCE_ENV, "99998888777766665555444433332222"),
        ]),
        &["/opt/zalo/zalo"],
    );
    let wrong_nonce_entry = read_proc_entry(&proc_root, 4500).unwrap();
    assert!(
        !runtime_hint_matches_entry(&hint, &wrong_nonce_entry, &p.id, &pdir, Some(1000)),
        "mismatched DIM_INSTANCE_NONCE must reject PID reuse"
    );
    let rt_nonce_mismatch = adapter.inspect_profile(&p, &pdir).unwrap();
    assert!(!rt_nonce_mismatch.running);
    assert!(rt_nonce_mismatch.stale_lock_detected);
    assert_eq!(rt_nonce_mismatch.main_pid, None);

    // 5c. Control check: when BOTH `starttime_ticks == 12_345` AND `DIM_INSTANCE_NONCE == orig_nonce` match,
    //     ownership is accepted!
    write_fake_proc_entry_with_starttime(
        &proc_root,
        4500,
        "zalo",
        'S',
        1,
        12_345,
        1000,
        110_000,
        Some(&[
            (DIM_PROFILE_ID_ENV, &p.id),
            (DIM_INSTANCE_NONCE_ENV, orig_nonce),
        ]),
        &["/opt/zalo/zalo"],
    );
    let valid_entry = read_proc_entry(&proc_root, 4500).unwrap();
    assert!(runtime_hint_matches_entry(
        &hint,
        &valid_entry,
        &p.id,
        &pdir,
        Some(1000)
    ));
    let rt_valid = adapter.inspect_profile(&p, &pdir).unwrap();
    assert!(rt_valid.running);
    assert!(!rt_valid.stale_lock_detected);
    assert_eq!(rt_valid.main_pid, Some(4500));

    // 5d. Native Linux live `/proc` check: plant stale `.runtime-pid` + `lockfile` pointing at OUR OWN
    //     test runner PID (`std::process::id()`) and prove `inspect_profile` reports `!running` and
    //     `stop()` returns `Ok(0)` without signaling our process!
    #[cfg(target_os = "linux")]
    {
        let self_pid = std::process::id();
        let live_p = ProfileRecord {
            id: "5511live".to_owned(),
            name: "Live Self PID Spoof".to_owned(),
            directory_name: "live-spoof_5511live".to_owned(),
            created_at: "2026-11-18T00:00:00Z".to_owned(),
            last_launched_at: None,
            zalo_user_id: None,
            notes: String::new(),
        };
        let live_pdir = tmp.path().join(&live_p.directory_name);
        ensure_linux_profile_layout(&live_p, &live_pdir).unwrap();
        let live_zdir = linux_zalo_data_dir(&live_pdir);
        fs::write(
            live_pdir.join(".runtime-pid"),
            serde_json::json!({
                "pid": self_pid,
                "profile_id": live_p.id,
                "instance_nonce": "00000000000000000000000000000001",
                "starttime_ticks": 1_u64,
            })
            .to_string(),
        )
        .unwrap();
        fs::write(
            live_zdir.join("lockfile"),
            format!("{{\"pid\":{self_pid}}}\n"),
        )
        .unwrap();

        let live_adapter = LinuxPlatformAdapter::new();
        let spoof_rt = live_adapter.inspect_profile(&live_p, &live_pdir).unwrap();
        assert!(!spoof_rt.running);
        assert!(spoof_rt.stale_lock_detected);

        let mut stale_state = spoof_rt;
        stale_state.main_pid = Some(self_pid);
        let killed = live_adapter.stop(&stale_state, &[], 200).unwrap();
        assert_eq!(
            killed, 0,
            "stop() must NEVER signal a live process whose starttime_ticks/nonce failed verification"
        );
        assert!(!live_pdir.join(".runtime-pid").exists());
    }
}

// ---------------------------------------------------------------------------
// Regression 6. Dangling symlinks (`SingletonLock`, custom binary, profile dir)
// ---------------------------------------------------------------------------
#[test]
fn regression_06_dangling_symlinks_detection_and_rejection() {
    // 6a. `SingletonLock` target parser boundary checks.
    assert_eq!(
        parse_singleton_lock_target("ubuntu-24-04-desktop-7777"),
        Some(7777)
    );
    assert_eq!(parse_singleton_lock_target("-7777"), None);
    assert_eq!(parse_singleton_lock_target("ubuntu-0"), None);
    assert_eq!(parse_singleton_lock_target("ubuntu-notapid"), None);
    assert_eq!(parse_singleton_lock_target(""), None);

    let tmp = tempfile::tempdir().unwrap();
    let missing_sock = tmp.path().join("nonexistent-wayland-0");
    assert!(!is_safe_bindable_socket_or_file(&missing_sock));
    assert!(!is_safe_bindable_socket_or_file(tmp.path()));

    let regular_auth = tmp.path().join("xauth_token");
    fs::write(&regular_auth, b"cookie").unwrap();
    assert!(is_safe_bindable_socket_or_file(&regular_auth));

    // Directory masquerading as `.runtime-pid` must return `None`.
    let fake_hint_dir = tmp.path().join("hint_dir_prof");
    fs::create_dir_all(fake_hint_dir.join(".runtime-pid")).unwrap();
    assert_eq!(read_runtime_pid_hint(&fake_hint_dir), None);

    // Missing directory fails `verify_directory_ownership_and_mode`.
    assert!(
        verify_directory_ownership_and_mode(&tmp.path().join("missing_dir"), 1000, true).is_err()
    );

    #[cfg(unix)]
    {
        let fs_root = tmp.path().join("fs");
        let proc_root = tmp.path().join("proc");
        let primary_cfg = tmp.path().join("primary");
        fs::create_dir_all(&fs_root).unwrap();
        fs::create_dir_all(&proc_root).unwrap();
        fs::create_dir_all(&primary_cfg).unwrap();

        let p = ProfileRecord {
            id: "6600ddee".to_owned(),
            name: "Dangling Check".to_owned(),
            directory_name: "dangling_6600ddee".to_owned(),
            created_at: "2026-11-18T00:00:00Z".to_owned(),
            last_launched_at: None,
            zalo_user_id: None,
            notes: String::new(),
        };
        let pdir = tmp.path().join(&p.directory_name);
        ensure_linux_profile_layout(&p, &pdir).unwrap();
        let zdir = linux_zalo_data_dir(&pdir);

        // 6b. Dangling `SingletonLock` symlink (`Path::exists()` is FALSE, `symlink_metadata()` is OK!).
        let singleton = zdir.join("SingletonLock");
        std::os::unix::fs::symlink("nonexistent-host-554433", &singleton).unwrap();
        assert!(
            !singleton.exists(),
            "dangling SingletonLock target must not exist on disk"
        );
        assert!(fs::symlink_metadata(&singleton).is_ok());

        let v_adapter = LinuxPlatformAdapter::with_virtual_roots(
            fs_root,
            proc_root,
            primary_cfg,
            1000,
            "x86_64",
            None,
        );
        let rt = v_adapter.inspect_profile(&p, &pdir).unwrap();
        assert!(!rt.running);
        assert!(
            rt.stale_lock_detected,
            "dangling SingletonLock symlink must be detected via symlink_metadata"
        );

        // Dangling `.runtime-pid` symlink must be rejected (`None`).
        std::os::unix::fs::symlink("/nonexistent/runtime-pid", pdir.join(".runtime-pid")).unwrap();
        assert_eq!(read_runtime_pid_hint(&pdir), None);

        // 6c. Broken symlink custom binary rejected across `inspect_user_binary`, `discover_installation`, and `launch`.
        let broken_bin = tmp.path().join("BrokenZalo.AppImage");
        std::os::unix::fs::symlink("/nonexistent/target/Zalo.AppImage", &broken_bin).unwrap();
        assert!(!is_safe_bindable_socket_or_file(&broken_bin));
        assert_eq!(
            inspect_user_binary(&broken_bin, None).unwrap_err().code(),
            "PATH_TRAVERSAL"
        );
        assert_eq!(
            v_adapter
                .discover_installation(Some(&broken_bin))
                .unwrap_err()
                .code(),
            "PATH_TRAVERSAL"
        );
        assert_eq!(
            v_adapter
                .launch(
                    &LaunchTarget::CustomExecutable {
                        path: broken_bin,
                        args: Vec::new(),
                    },
                    &p,
                    &pdir,
                )
                .unwrap_err()
                .code(),
            "PATH_TRAVERSAL"
        );

        // 6d. Dangling symlink profile root rejected by `ensure_linux_profile_layout` & `verify_directory_ownership_and_mode`.
        let dangling_profile = tmp.path().join("dangling_profile_root");
        std::os::unix::fs::symlink("/nonexistent/profile_escape", &dangling_profile).unwrap();
        assert_eq!(
            ensure_linux_profile_layout(&p, &dangling_profile)
                .unwrap_err()
                .code(),
            "PATH_TRAVERSAL"
        );
        assert!(verify_directory_ownership_and_mode(&dangling_profile, 1000, true).is_err());
    }
}

// ---------------------------------------------------------------------------
// Regression 7. HOME/XDG isolation & hostile loader/runtime env stripping
// ---------------------------------------------------------------------------
#[test]
fn regression_07_home_xdg_isolation_and_hostile_env_stripping() {
    let tmp = tempfile::tempdir().unwrap();
    let canary_appdata = tmp.path().join("HostAppDataCanary");
    fs::create_dir_all(&canary_appdata).unwrap();

    let store = JsonProfileStore::new(tmp.path().join("EnvIsoStore")).unwrap();
    let p = store.create_profile("Isolated XDG", "").unwrap();
    let pdir = store.profile_dir_for(&p).unwrap();
    ensure_linux_profile_layout(&p, &pdir).unwrap();
    let zdir = linux_zalo_data_dir(&pdir);

    let canary_str = canary_appdata.to_string_lossy().into_owned();
    let path_str = std::env::var("PATH").unwrap_or_default();
    let mut hostile_env: HashMap<&str, &str> = HashMap::new();
    hostile_env.insert("PATH", &path_str);
    hostile_env.insert("HOME", "/home/hostvictim");
    hostile_env.insert("XDG_CONFIG_HOME", "/home/hostvictim/.config");
    hostile_env.insert("XDG_DATA_HOME", "/home/hostvictim/.local/share");
    hostile_env.insert("XDG_STATE_HOME", "/home/hostvictim/.local/state");
    hostile_env.insert("XDG_CACHE_HOME", "/home/hostvictim/.cache");
    hostile_env.insert("TMPDIR", "/tmp");
    hostile_env.insert("LD_PRELOAD", "/tmp/libevil_hook.so");
    hostile_env.insert("LD_AUDIT", "/tmp/libaudit_hook.so");
    hostile_env.insert("LD_LIBRARY_PATH", "/tmp/hijack_libs");
    // Note: if `NODE_OPTIONS` leaked into `node`, `--definitely-invalid-node-flag` would abort Node on startup!
    hostile_env.insert("NODE_OPTIONS", "--definitely-invalid-node-flag");
    hostile_env.insert("ELECTRON_RUN_AS_NODE", "1");
    hostile_env.insert("APPDATA", &canary_str);
    hostile_env.insert("USERPROFILE", &canary_str);
    hostile_env.insert("CFFIXED_USER_HOME", "/Users/hostvictim");

    let nonce = generate_instance_nonce();
    let sanitized =
        build_sanitized_linux_spawn_env_with_nonce(&p, &pdir, Some(&nonce), true, &|k| {
            hostile_env.get(k).map(|v| (*v).to_owned())
        });

    assert_eq!(sanitized.get("HOME").unwrap(), &pdir.to_string_lossy());
    assert_eq!(
        sanitized.get("XDG_CONFIG_HOME").unwrap(),
        &pdir.join(".config").to_string_lossy()
    );
    assert_eq!(
        sanitized.get("XDG_DATA_HOME").unwrap(),
        &pdir.join(".local").join("share").to_string_lossy()
    );
    assert_eq!(
        sanitized.get("XDG_STATE_HOME").unwrap(),
        &pdir.join(".local").join("state").to_string_lossy()
    );
    assert_eq!(
        sanitized.get("XDG_CACHE_HOME").unwrap(),
        &pdir.join(".cache").to_string_lossy()
    );
    assert_eq!(
        sanitized.get("TMPDIR").unwrap(),
        &pdir.join("tmp").to_string_lossy()
    );
    assert_eq!(
        sanitized.get("TEMP").unwrap(),
        &pdir.join("tmp").to_string_lossy()
    );
    assert_eq!(
        sanitized.get("TMP").unwrap(),
        &pdir.join("tmp").to_string_lossy()
    );
    assert_eq!(sanitized.get(DIM_PROFILE_ID_ENV), Some(&p.id));
    assert_eq!(sanitized.get(DIM_INSTANCE_NONCE_ENV), Some(&nonce));

    for forbidden in [
        "LD_PRELOAD",
        "LD_AUDIT",
        "LD_LIBRARY_PATH",
        "NODE_OPTIONS",
        "ELECTRON_RUN_AS_NODE",
        "APPDATA",
        "USERPROFILE",
        "CFFIXED_USER_HOME",
    ] {
        assert!(
            !sanitized.contains_key(forbidden),
            "dangerous variable {forbidden} MUST be stripped from sanitized child environment"
        );
    }

    // Runtime proof: spawn `node fixture-app.mjs` with `sanitized`; it must NOT crash on `NODE_OPTIONS`
    // and must NOT create `ZaloData` inside `canary_appdata`!
    let mut cmd = Command::new("node");
    cmd.arg(fixture_script_path())
        .args(["--user-id=849807007", "--stay-ms=6000"])
        .env_clear()
        .envs(&sanitized)
        .current_dir(&pdir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Ok(sysroot) = std::env::var("SYSTEMROOT") {
        cmd.env("SYSTEMROOT", sysroot);
    }
    let mut guard = ChildGuard(Some(cmd.spawn().unwrap()));
    assert!(
        wait_for_file(&zdir.join("lockfile"), 4_000)
            && wait_for_file(&zdir.join("launch-state.json"), 4_000),
        "sanitized child must boot cleanly without NODE_OPTIONS crash and write inside isolated XDG_CONFIG_HOME"
    );
    assert!(
        !canary_appdata.join("ZaloData").exists(),
        "host APPDATA canary directory must remain untouched"
    );

    fs::write(zdir.join(".quit-signal"), "{\"requested_at\":\"now\"}\n").unwrap();
    if let Some(ref mut c) = guard.0 {
        let _ = c.wait();
    }
}

// ---------------------------------------------------------------------------
// Regression 8. File chooser (`GTK_USE_PORTAL=1`) & Downloads/user-dirs.dirs confinement
// ---------------------------------------------------------------------------
#[test]
fn regression_08_portal_file_chooser_and_downloads_confinement() {
    let tmp = tempfile::tempdir().unwrap();
    let p = ProfileRecord {
        id: "8800eeff".to_owned(),
        name: "Portal & Downloads".to_owned(),
        directory_name: "portal_8800eeff".to_owned(),
        created_at: "2026-11-18T00:00:00Z".to_owned(),
        last_launched_at: None,
        zalo_user_id: None,
        notes: String::new(),
    };
    let pdir = tmp.path().join(&p.directory_name);
    ensure_linux_profile_layout(&p, &pdir).unwrap();

    assert!(pdir.join("Downloads").is_dir());
    assert!(pdir.join("Documents").is_dir());
    assert!(pdir.join("Documents").join("Zalo Received Files").is_dir());
    assert!(pdir.join(".config").join("user-dirs.locale").is_file());

    // 8a. Every `XDG_*_DIR` entry in `.config/user-dirs.dirs` must resolve strictly inside `$HOME/`.
    let user_dirs_raw = fs::read_to_string(pdir.join(".config").join("user-dirs.dirs")).unwrap();
    let mut xdg_keys_seen = Vec::new();
    for line in user_dirs_raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (key, val) = trimmed.split_once('=').expect("valid KEY=VAL line");
        assert!(
            val == "\"$HOME/Downloads\"" || val == "\"$HOME/Documents\"",
            "unexpected unconfined user-dirs target {key}={val}"
        );
        xdg_keys_seen.push(key.to_owned());
    }
    for required_key in [
        "XDG_DESKTOP_DIR",
        "XDG_DOWNLOAD_DIR",
        "XDG_TEMPLATES_DIR",
        "XDG_PUBLICSHARE_DIR",
        "XDG_DOCUMENTS_DIR",
        "XDG_MUSIC_DIR",
        "XDG_PICTURES_DIR",
        "XDG_VIDEOS_DIR",
    ] {
        assert!(
            xdg_keys_seen.iter().any(|k| k == required_key),
            "user-dirs.dirs missing {required_key}"
        );
    }

    // 8b. `GTK_USE_PORTAL=1` enabled by default (`with_portal_file_chooser(true)`), omitted when disabled.
    let env_portal_on =
        build_sanitized_linux_spawn_env_with_nonce(&p, &pdir, None, true, &|_| None);
    assert_eq!(
        env_portal_on.get("GTK_USE_PORTAL").map(String::as_str),
        Some("1")
    );
    assert_eq!(
        env_portal_on.get("XDG_DOWNLOAD_DIR").unwrap(),
        &pdir.join("Downloads").to_string_lossy()
    );
    assert_eq!(
        env_portal_on.get("XDG_DOCUMENTS_DIR").unwrap(),
        &pdir.join("Documents").to_string_lossy()
    );

    let env_portal_off =
        build_sanitized_linux_spawn_env_with_nonce(&p, &pdir, None, false, &|_| None);
    assert!(!env_portal_off.contains_key("GTK_USE_PORTAL"));

    // Explicit host `GTK_USE_PORTAL=0` is preserved rather than overwritten.
    let env_host_override =
        build_sanitized_linux_spawn_env_with_nonce(&p, &pdir, None, true, &|k| {
            (k == "GTK_USE_PORTAL").then(|| "0".to_owned())
        });
    assert_eq!(
        env_host_override.get("GTK_USE_PORTAL").map(String::as_str),
        Some("0")
    );

    let adapter_on = LinuxPlatformAdapter::with_virtual_roots(
        tmp.path().join("fs"),
        tmp.path().join("proc"),
        tmp.path().join("primary"),
        1000,
        "x86_64",
        None,
    );
    let plan_on = adapter_on.plan_sandbox(&p, &pdir, Path::new("/opt/zalo/zalo"), &[], None);
    assert!(plan_on
        .args
        .windows(3)
        .any(|w| w == ["--setenv", "GTK_USE_PORTAL", "1"]));
    assert!(plan_on.args.windows(3).any(|w| {
        w[0] == "--setenv"
            && w[1] == "XDG_DOWNLOAD_DIR"
            && w[2] == pdir.join("Downloads").to_string_lossy()
    }));
    assert!(plan_on.args.windows(3).any(|w| {
        w[0] == "--setenv"
            && w[1] == "XDG_DOCUMENTS_DIR"
            && w[2] == pdir.join("Documents").to_string_lossy()
    }));

    let adapter_off = adapter_on.with_portal_file_chooser(false);
    let plan_off = adapter_off.plan_sandbox(&p, &pdir, Path::new("/opt/zalo/zalo"), &[], None);
    assert!(!plan_off
        .args
        .windows(3)
        .any(|w| w[0] == "--setenv" && w[1] == "GTK_USE_PORTAL"));
}

// ---------------------------------------------------------------------------
// Regression 9. Runtime socket permissions & minimal `bwrap` binds
// ---------------------------------------------------------------------------
#[test]
fn regression_09_runtime_socket_permissions_and_minimal_bwrap_binds() {
    let tmp = tempfile::tempdir().unwrap();
    let p = ProfileRecord {
        id: "9900ff11".to_owned(),
        name: "Bwrap Minimal Sockets".to_owned(),
        directory_name: "bwrap_9900ff11".to_owned(),
        created_at: "2026-11-18T00:00:00Z".to_owned(),
        last_launched_at: None,
        zalo_user_id: None,
        notes: String::new(),
    };
    let pdir = tmp.path().join(&p.directory_name);
    ensure_linux_profile_layout(&p, &pdir).unwrap();

    // 9a. Socket parser unit/edge cases: system D-Bus, abstract sockets, TCP displays, and `..` rejected.
    assert_eq!(
        parse_dbus_session_socket_path("unix:path=/run/user/1000/bus"),
        Some(PathBuf::from("/run/user/1000/bus"))
    );
    assert_eq!(
        parse_dbus_session_socket_path("unix:path=/tmp/custom%20dbus/session,guid=deadbeef"),
        Some(PathBuf::from("/tmp/custom dbus/session"))
    );
    for forbidden_dbus in [
        "unix:path=/run/dbus/system_bus_socket",
        "unix:path=/var/run/dbus/system_bus_socket",
        "unix:path=/run/dbus/other",
        "unix:abstract=/tmp/dbus-123456",
        "unix:abstract=/run/user/1000/bus",
        "tcp:host=127.0.0.1,port=55555",
        "unix:path=/run/user/1000/../etc/shadow",
        "unix:path=/",
        "unix:path=/dev",
    ] {
        assert_eq!(
            parse_dbus_session_socket_path(forbidden_dbus),
            None,
            "must reject unsafe D-Bus address: {forbidden_dbus}"
        );
    }

    assert_eq!(
        parse_x11_display_socket_path(":0"),
        Some(PathBuf::from("/tmp/.X11-unix/X0"))
    );
    assert_eq!(
        parse_x11_display_socket_path("unix:3.0"),
        Some(PathBuf::from("/tmp/.X11-unix/X3"))
    );
    for bad_x11 in ["localhost:10.0", "10.0.0.1:0", ":70000", ":/0", ""] {
        assert_eq!(parse_x11_display_socket_path(bad_x11), None);
    }

    assert_eq!(
        resolve_wayland_socket_path("wayland-0", Some("/run/user/1000")),
        Some(PathBuf::from("/run/user/1000/wayland-0"))
    );
    assert_eq!(
        resolve_wayland_socket_path("/run/user/1000/wayland-1", None),
        Some(PathBuf::from("/run/user/1000/wayland-1"))
    );
    assert_eq!(
        resolve_wayland_socket_path("../wayland-0", Some("/run/user/1000")),
        None
    );
    assert_eq!(
        resolve_wayland_socket_path("/run/dbus/system_bus_socket", Some("/run/user/1000")),
        None
    );

    assert_eq!(
        resolve_pipewire_socket_path(None, Some("/run/user/1000")),
        Some(PathBuf::from("/run/user/1000/pipewire-0"))
    );
    assert_eq!(
        resolve_pulse_socket_path(
            Some("{0123456789abcdef}unix:/run/user/1000/pulse/native"),
            Some("/run/user/1000")
        ),
        Some(PathBuf::from("/run/user/1000/pulse/native"))
    );
    assert_eq!(
        resolve_pulse_socket_path(Some("tcp:127.0.0.1:4713"), Some("/run/user/1000")),
        None
    );

    // 9b. `plan_bwrap_desktop_sandbox` with hostile system-bus `DBUS_SESSION_BUS_ADDRESS`:
    //     must mount `0700` `$XDG_RUNTIME_DIR` tmpfs, reject system bus, and never bind `$HOME`/`/dev`/`$XDG_RUNTIME_DIR`.
    let mut sys_dbus_env = HashMap::new();
    sys_dbus_env.insert("HOME", "/home/alice");
    sys_dbus_env.insert("XDG_RUNTIME_DIR", "/run/user/1000");
    sys_dbus_env.insert("WAYLAND_DISPLAY", "wayland-0");
    sys_dbus_env.insert(
        "DBUS_SESSION_BUS_ADDRESS",
        "unix:path=/run/dbus/system_bus_socket",
    );

    let opts = BwrapSandboxOptions {
        lifetime_policy: LauncherLifetimePolicy::PersistentDetachedSession,
        profile_id: Some(&p.id),
        instance_nonce: Some("nonce_9900"),
        prefer_portal_file_chooser: true,
        extra_ro_files: &[],
    };
    let safe_plan = plan_bwrap_desktop_sandbox(
        &pdir,
        Path::new("/home/alice/Applications/Zalo-26.11.10.AppImage"),
        &[],
        &opts,
        &|k| sys_dbus_env.get(k).map(|v| (*v).to_owned()),
        &|_| true,
    );

    // Isolated `0700` `$XDG_RUNTIME_DIR` directory creation inside namespace:
    assert!(
        safe_plan.args.windows(5).any(|w| {
            w == [
                "--dir",
                "/run/user/1000",
                "--chmod",
                "0700",
                "/run/user/1000",
            ]
        }),
        "bwrap plan must create isolated 0700 tmpfs dir at $XDG_RUNTIME_DIR"
    );
    // Isolated `0700` `$XDG_RUNTIME_DIR/pulse` subdir for `pulse/native`:
    assert!(safe_plan.args.windows(5).any(|w| {
        w == [
            "--dir",
            "/run/user/1000/pulse",
            "--chmod",
            "0700",
            "/run/user/1000/pulse",
        ]
    }));
    // System D-Bus MUST NOT be bound or exposed!
    assert!(!safe_plan
        .exposed_surfaces
        .contains(&DesktopSocketSurface::SessionDbus));
    assert!(!safe_plan
        .args
        .iter()
        .any(|a| a.contains("/run/dbus") || a.contains("system_bus_socket")));

    // Never bind full `$HOME`, full `/dev`, full `$XDG_RUNTIME_DIR`, or full `/tmp/.X11-unix`!
    for w in safe_plan.args.windows(3) {
        if matches!(
            w[0].as_str(),
            "--bind" | "--ro-bind" | "--ro-bind-try" | "--dev-bind" | "--dev-bind-try"
        ) {
            assert!(
                !matches!(
                    w[1].as_str(),
                    "/home/alice"
                        | "/dev"
                        | "/run/user/1000"
                        | "/run/user/1000/pulse"
                        | "/tmp"
                        | "/tmp/.X11-unix"
                ),
                "forbidden broad directory bind detected in bwrap plan: {w:?}"
            );
        }
    }

    // 9c. Verify X11 + D-Bus + PipeWire + PulseAudio security tradeoff notes.
    assert!(!DesktopSocketSurface::WaylandCompositor.reduces_sandbox_isolation());
    assert!(DesktopSocketSurface::X11Server.reduces_sandbox_isolation());
    assert!(DesktopSocketSurface::SessionDbus.reduces_sandbox_isolation());
    assert!(DesktopSocketSurface::PipeWireDaemon.reduces_sandbox_isolation());
    assert!(DesktopSocketSurface::PulseAudioServer.reduces_sandbox_isolation());

    let mut full_desktop_env = HashMap::new();
    full_desktop_env.insert("HOME", "/home/alice");
    full_desktop_env.insert("XDG_RUNTIME_DIR", "/run/user/1000");
    full_desktop_env.insert("WAYLAND_DISPLAY", "wayland-0");
    full_desktop_env.insert("DISPLAY", ":0");
    full_desktop_env.insert("XAUTHORITY", "/run/user/1000/.mutter-Xwaylandauth.ABC");
    full_desktop_env.insert("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/user/1000/bus");

    let full_plan = plan_bwrap_desktop_sandbox(
        &pdir,
        Path::new("/opt/zalo/Zalo.AppImage"),
        &[],
        &opts,
        &|k| full_desktop_env.get(k).map(|v| (*v).to_owned()),
        &|_| true,
    );
    for expected_tag in [
        "SANDBOX_TRADEOFF [x11]",
        "SANDBOX_TRADEOFF [dbus-session]",
        "SANDBOX_TRADEOFF [pipewire]",
        "SANDBOX_TRADEOFF [pulseaudio]",
    ] {
        assert!(
            full_plan
                .security_tradeoff_notes
                .iter()
                .any(|n| n.contains(expected_tag)),
            "missing security_tradeoff_note for {expected_tag}"
        );
    }
}

// ---------------------------------------------------------------------------
// Regression 10. Unsupported Wayland & Headless focus honesty (`UNSUPPORTED_OPERATION`)
// ---------------------------------------------------------------------------
#[test]
fn regression_10_unsupported_wayland_and_headless_focus_honesty() {
    let tmp = tempfile::tempdir().unwrap();
    let p = ProfileRecord {
        id: "aa10bb20".to_owned(),
        name: "Wayland Honest Focus".to_owned(),
        directory_name: "wayland_aa10bb20".to_owned(),
        created_at: "2026-11-18T00:00:00Z".to_owned(),
        last_launched_at: None,
        zalo_user_id: None,
        notes: String::new(),
    };
    let pdir = tmp.path().join(&p.directory_name);
    ensure_linux_profile_layout(&p, &pdir).unwrap();
    let zdir = linux_zalo_data_dir(&pdir);

    let gui_target = LaunchTarget::CustomExecutable {
        path: PathBuf::from("/opt/zalo/Zalo-26.11.10.AppImage"),
        args: Vec::new(),
    };

    let mut state = ProfileRuntimeState {
        profile_id: p.id.clone(),
        profile_name: p.name.clone(),
        profile_dir: pdir.clone(),
        zalo_data_dir: zdir.clone(),
        lockfile_path: zdir.join("SingletonLock"),
        running: false,
        stale_lock_detected: false,
        main_pid: None,
        process_ids: Vec::new(),
        process_count: 0,
        working_set_bytes: 0,
        window_handle: None,
        window_title: None,
        detected_zalo_user_id: None,
        disk_usage_bytes: 0,
        status_label: "Stopped".to_owned(),
    };

    let wayland_adapter = LinuxPlatformAdapter::with_virtual_roots(
        tmp.path().join("fs"),
        tmp.path().join("proc"),
        tmp.path().join("primary"),
        1000,
        "x86_64",
        None,
    )
    .with_virtual_env_var("XDG_SESSION_TYPE", "wayland")
    .with_virtual_env_var("WAYLAND_DISPLAY", "wayland-0");

    // 10a. Focusing a non-running profile must return `NOT_RUNNING`.
    let not_running_err = wayland_adapter
        .focus_or_restore(&gui_target, &p, &pdir, &state)
        .unwrap_err();
    assert_eq!(not_running_err.code(), "NOT_RUNNING");

    // 10b. Running `CustomExecutable` on Wayland must return `Err` (`UNSUPPORTED_PLATFORM`
    //      containing `"UNSUPPORTED_OPERATION"`), NEVER a fake `Ok(())`!
    state.running = true;
    state.main_pid = Some(6100);
    state.process_ids = vec![6100, 6101];
    state.process_count = 2;
    state.status_label = "Running".to_owned();

    let wl_err = wayland_adapter
        .focus_or_restore(&gui_target, &p, &pdir, &state)
        .unwrap_err();
    assert_eq!(wl_err.code(), "UNSUPPORTED_PLATFORM");
    assert!(wl_err.to_string().contains("UNSUPPORTED_OPERATION"));
    assert!(wl_err.to_string().contains("Wayland"));
    assert!(wl_err.to_string().contains("xdg-activation-v1"));

    // Verify `dim-core` `OperationOutcome` serialization contract: `DimError::UnsupportedPlatform`
    // currently maps `error_code = "UNSUPPORTED_PLATFORM"` while preserving `"UNSUPPORTED_OPERATION:"`
    // in `message` (documenting the exact boundary if a dedicated `DimError::UnsupportedOperation`
    // variant is ever added in `dim-core`).
    let outcome = OperationOutcome::<()>::from_err(&wl_err);
    assert!(!outcome.success);
    assert_eq!(outcome.error_code.as_deref(), Some("UNSUPPORTED_PLATFORM"));
    assert!(outcome.message.contains("UNSUPPORTED_OPERATION:"));

    // 10c. Running `CustomExecutable` in a Headless session must also fail closed with `UNSUPPORTED_OPERATION`.
    let headless_adapter = LinuxPlatformAdapter::with_virtual_roots(
        tmp.path().join("fs"),
        tmp.path().join("proc"),
        tmp.path().join("primary"),
        1000,
        "x86_64",
        None,
    );
    let hl_err = headless_adapter
        .focus_or_restore(&gui_target, &p, &pdir, &state)
        .unwrap_err();
    assert_eq!(hl_err.code(), "UNSUPPORTED_PLATFORM");
    assert!(hl_err.to_string().contains("UNSUPPORTED_OPERATION"));
    assert!(hl_err.to_string().contains("headless"));

    // 10d. `NodeFixture` focus on Wayland uses deterministic `.focus-signal` IPC cleanly.
    let fixture_target = LaunchTarget::NodeFixture {
        node_path: None,
        script_path: fixture_script_path(),
        extra_args: Vec::new(),
    };
    wayland_adapter
        .focus_or_restore(&fixture_target, &p, &pdir, &state)
        .unwrap();
    assert!(zdir.join(".focus-signal").is_file());
}

// ---------------------------------------------------------------------------
// Regression 11. Invalid & truncated `.AppImage` rejection
// ---------------------------------------------------------------------------
#[test]
fn regression_11_invalid_and_truncated_appimage_rejection() {
    let tmp = tempfile::tempdir().unwrap();
    let apps_dir = tmp.path().join("Apps");
    let desktop_dir = tmp.path().join("applications");
    let fs_root = tmp.path().join("ubuntu2404");
    fs::create_dir_all(&apps_dir).unwrap();
    fs::create_dir_all(&desktop_dir).unwrap();
    fs::create_dir_all(&fs_root).unwrap();

    // Build 4 invalid `.AppImage` files:
    // (1) Non-ELF random/HTML garbage
    let garbage_app = apps_dir.join("Zalo-26.11.10-Garbage.AppImage");
    write_executable_file(
        &garbage_app,
        b"<html>502 Bad Gateway - not an ELF binary</html>",
    );

    // (2) Truncated 30-byte ELF64 Type-2 header (< 64-byte `sizeof(Elf64_Ehdr)`)
    let truncated_ehdr = apps_dir.join("Zalo-26.11.10-TruncatedEhdr.AppImage");
    let mut short_hdr = vec![0u8; 30];
    short_hdr[0..4].copy_from_slice(b"\x7fELF");
    short_hdr[4] = 2;
    short_hdr[5] = 1;
    short_hdr[6] = 1;
    short_hdr[8..11].copy_from_slice(b"AI\x02");
    write_executable_file(&truncated_ehdr, &short_hdr);

    // (3) Valid 64-byte ELF64 Type-2 header with out-of-bounds `e_phoff` (`TruncatedProgramHeaderTable`)
    let truncated_phdr = apps_dir.join("Zalo-26.11.10-TruncatedPhdr.AppImage");
    let mut bad_phdr_elf = make_elf64_stub(host_elf_machine(), true, b"short");
    bad_phdr_elf[32..40].copy_from_slice(&4096u64.to_le_bytes()); // e_phoff = 4096 > file len!
    bad_phdr_elf[54..56].copy_from_slice(&56u16.to_le_bytes());
    bad_phdr_elf[56..58].copy_from_slice(&4u16.to_le_bytes());
    write_executable_file(&truncated_phdr, &bad_phdr_elf);

    // (4) Shell script masquerading with `.AppImage` extension
    let script_masquerade = apps_dir.join("Zalo-26.11.10-Script.AppImage");
    write_executable_file(&script_masquerade, b"#!/bin/sh\necho fake appimage\n");

    let p = ProfileRecord {
        id: "1100badd".to_owned(),
        name: "Invalid AppImage".to_owned(),
        directory_name: "invalid-app_1100badd".to_owned(),
        created_at: "2026-11-18T00:00:00Z".to_owned(),
        last_launched_at: None,
        zalo_user_id: None,
        notes: String::new(),
    };
    let pdir = tmp.path().join(&p.directory_name);
    ensure_linux_profile_layout(&p, &pdir).unwrap();

    let mut env_map = HashMap::new();
    env_map.insert("WAYLAND_DISPLAY", "wayland-0");
    let probe = probe_ubuntu_runtime_at(
        &fs_root,
        std::env::consts::ARCH,
        1000,
        &|k| env_map.get(k).map(|v| (*v).to_owned()),
        None,
    );
    let adapter = LinuxPlatformAdapter::with_virtual_roots(
        fs_root.clone(),
        tmp.path().join("proc"),
        tmp.path().join("primary"),
        1000,
        std::env::consts::ARCH,
        None,
    );

    for invalid_path in [
        &garbage_app,
        &truncated_ehdr,
        &truncated_phdr,
        &script_masquerade,
    ] {
        let rep = inspect_user_binary(invalid_path, None).unwrap();
        assert!(
            !rep.has_valid_executable_header,
            "invalid .AppImage must have has_valid_executable_header == false: {}",
            invalid_path.display()
        );
        assert!(!rep.is_valid_appimage_elf64());
        let fmt_err = rep.validate_executable_format().unwrap_err();
        assert_eq!(fmt_err.code(), "UNVERIFIED_BINARY");

        let preflight =
            evaluate_appimage_preflight(&probe, &fs_root, invalid_path, Some(&pdir), &[], false);
        assert!(!preflight.ready_to_launch);
        assert!(
            preflight.diagnostics.iter().any(|d| {
                d.severity == PreflightSeverity::Blocking
                    && d.kind.code() == "INVALID_EXECUTABLE_HEADER"
                    && d.category() == PreflightCategory::RuntimeEnvironment
            }),
            "preflight must report blocking InvalidExecutableHeader for {}",
            invalid_path.display()
        );

        // `adapter.launch()` must reject invalid `.AppImage` upfront with `UNVERIFIED_BINARY`!
        let launch_err = adapter
            .launch(
                &LaunchTarget::CustomExecutable {
                    path: invalid_path.clone(),
                    args: Vec::new(),
                },
                &p,
                &pdir,
            )
            .unwrap_err();
        assert_eq!(
            launch_err.code(),
            "UNVERIFIED_BINARY",
            "adapter.launch must reject invalid .AppImage '{}' with UNVERIFIED_BINARY",
            invalid_path.display()
        );
    }

    // `.desktop` + directory auto-discovery must skip ALL invalid `.AppImage` files!
    fs::write(
        desktop_dir.join("corrupt-zalo.desktop"),
        format!(
            "[Desktop Entry]\nType=Application\nName=Zalo Corrupt\nExec=\"{}\"\n",
            garbage_app.display()
        ),
    )
    .unwrap();
    let discovered_cands = discover_compatible_apps_in_roots(&[desktop_dir], &[apps_dir]);
    assert!(
        discovered_cands.is_empty(),
        "discover_compatible_apps_in_roots must skip invalid/corrupt .AppImage files"
    );

    // Explicit `discover_installation(Some(&garbage_app))` surfaces format=`unknown` + explicit header warnings.
    let explicit_disc = adapter.discover_installation(Some(&garbage_app)).unwrap();
    assert!(explicit_disc.signature_status.contains("format=unknown"));
    assert!(explicit_disc
        .notes
        .iter()
        .any(|n| n.contains("Invalid .AppImage executable header")));
}

// ---------------------------------------------------------------------------
// Regression 12. Untrusted ELF64 static metadata inspection & FIPS SHA-256 vectors
// ---------------------------------------------------------------------------
#[test]
fn regression_12_untrusted_elf64_static_metadata_and_fips_sha256_vectors() {
    let tmp = tempfile::tempdir().unwrap();
    let fs_root = tmp.path().join("ubuntu_fs");
    let sys_multiarch = fs_root.join("usr/lib/x86_64-linux-gnu");
    let app_dir = tmp.path().join("untrusted_bundle");
    let app_lib = app_dir.join("lib");
    fs::create_dir_all(&sys_multiarch).unwrap();
    fs::create_dir_all(&app_lib).unwrap();

    // Plant an executable canary script at `<tmp>/untrusted-interp` so if anything ever tried to
    // execute the binary or its interpreter (like `ldd` does!), `pwned_canary` would be created.
    let canary_output = tmp.path().join("pwned_canary.txt");
    let fake_interp_on_disk = tmp.path().join("untrusted-interp");
    write_executable_file(
        &fake_interp_on_disk,
        format!("#!/bin/sh\necho PWNED > \"{}\"\n", canary_output.display()).as_bytes(),
    );

    // Populate ONLY `libc.so.6` (system multiarch) and `libbundled_zalo.so.1` (`$ORIGIN/lib`).
    // Leave `libopus.so.0` and `libpulse-simple.so.0` missing!
    fs::write(sys_multiarch.join("libc.so.6"), b"elf-stub").unwrap();
    fs::write(app_lib.join("libbundled_zalo.so.1"), b"elf-stub").unwrap();

    let untrusted_elf = build_synthetic_elf64_dynamic_binary(
        0x3E, // EM_X86_64
        true,
        Some("/tmp/untrusted-interp"),
        Some("libzalo_untrusted_payload.so"),
        &[
            "linux-vdso.so.1",
            "libc.so.6",
            "libbundled_zalo.so.1",
            "libopus.so.0",
            "libpulse-simple.so.0",
        ],
        &["/ignored/rpath/because/runpath/overrides"],
        &["$ORIGIN/lib:/opt/custom/lib"],
    );

    let bin_path = app_dir.join("Zalo-26.11.10-Untrusted.AppImage");
    write_executable_file(&bin_path, &untrusted_elf);

    // 12a. Static slice & file ELF64 parsers extract `PT_INTERP`, `DT_NEEDED`, and `DT_RUNPATH` identically.
    let slice_meta = parse_elf64_dynamic_metadata(&untrusted_elf).unwrap();
    let file_meta = read_elf64_dynamic_metadata(&bin_path).unwrap();
    assert_eq!(slice_meta, file_meta);
    assert_eq!(slice_meta.arch, ElfCpuArch::X86_64);
    assert_eq!(
        slice_meta.interpreter.as_deref(),
        Some("/tmp/untrusted-interp")
    );
    assert_eq!(
        slice_meta.soname.as_deref(),
        Some("libzalo_untrusted_payload.so")
    );
    assert_eq!(
        slice_meta.needed_libraries,
        vec![
            "linux-vdso.so.1",
            "libc.so.6",
            "libbundled_zalo.so.1",
            "libopus.so.0",
            "libpulse-simple.so.0"
        ]
    );
    assert_eq!(
        slice_meta.rpath_entries,
        vec!["/ignored/rpath/because/runpath/overrides"]
    );
    assert_eq!(
        slice_meta.runpath_entries,
        vec!["$ORIGIN/lib", "/opt/custom/lib"]
    );

    // 12b. Static missing-library resolution honors `$ORIGIN/lib` (`DT_RUNPATH`) + multiarch with ZERO execution.
    let resolved_missing = resolve_elf64_missing_libraries_at(&slice_meta, &bin_path, &fs_root);
    let (inspected_meta, inspected_missing) =
        inspect_elf64_needed_libraries_at(&bin_path, &fs_root).unwrap();
    assert_eq!(inspected_meta, slice_meta);
    assert_eq!(resolved_missing, inspected_missing);
    assert_eq!(resolved_missing.len(), 2);
    assert_eq!(resolved_missing[0].soname, "libopus.so.0");
    assert_eq!(resolved_missing[0].ubuntu_2404_package, "libopus0");
    assert_eq!(resolved_missing[1].soname, "libpulse-simple.so.0");
    assert_eq!(resolved_missing[1].ubuntu_2404_package, "libpulse0");

    // 12c. `inspect_user_binary` flags non-standard `PT_INTERP` and states SHA-256 integrity-only provenance!
    let full_report = inspect_user_binary(&bin_path, None).unwrap();
    assert!(full_report.has_valid_executable_header);
    assert!(full_report.is_valid_appimage_elf64());
    assert!(full_report
        .notes
        .iter()
        .any(|n| n.contains("/tmp/untrusted-interp") && n.contains("without executing")));
    assert!(full_report
        .notes
        .iter()
        .any(|n| n.contains("byte-level integrity") && n.contains("NEVER publisher identity")));
    assert!(
        !canary_output.exists(),
        "untrusted binary or interpreter must NEVER be executed during metadata inspection"
    );

    // 12d. Malformed / truncated / adversarial ELF64 inputs rejected cleanly without panic or OOM.
    assert_eq!(
        parse_elf64_dynamic_metadata(b""),
        Err(Elf64ParseError::TruncatedElfHeader { actual_bytes: 0 })
    );
    assert_eq!(
        parse_elf64_dynamic_metadata(&untrusted_elf[..24]),
        Err(Elf64ParseError::TruncatedElfHeader { actual_bytes: 24 })
    );

    let mut elf32 = untrusted_elf.clone();
    elf32[4] = 1; // ELFCLASS32
    assert_eq!(
        parse_elf64_dynamic_metadata(&elf32),
        Err(Elf64ParseError::UnsupportedClass(1))
    );

    let mut et_rel = untrusted_elf.clone();
    et_rel[16..18].copy_from_slice(&1u16.to_le_bytes()); // ET_REL
    assert_eq!(
        parse_elf64_dynamic_metadata(&et_rel),
        Err(Elf64ParseError::NotExecutableType(1))
    );

    let mut huge_phnum = untrusted_elf.clone();
    huge_phnum[56..58].copy_from_slice(&9999u16.to_le_bytes());
    assert_eq!(
        parse_elf64_dynamic_metadata(&huge_phnum),
        Err(Elf64ParseError::TooManyProgramHeaders(9999))
    );

    let mut overflow_phoff = untrusted_elf.clone();
    overflow_phoff[32..40].copy_from_slice(&(u64::MAX - 4).to_le_bytes());
    assert!(matches!(
        parse_elf64_dynamic_metadata(&overflow_phoff),
        Err(Elf64ParseError::TruncatedProgramHeaderTable { .. })
    ));

    // Unterminated `.dynstr` (strip NUL bytes from `.dynstr` region) rejected with `UnterminatedDynamicString`.
    let mut no_nul_elf =
        build_synthetic_elf64_dynamic_binary(0x3E, false, None, None, &["libopus.so.0"], &[], &[]);
    // `.dynstr` starts at `0x200`: `[0, b'l', ..., b'0', 0]`. Overwrite trailing NUL with `'X'`.
    let dynstr_end = 0x200 + 1 + "libopus.so.0".len();
    assert_eq!(no_nul_elf[dynstr_end], 0);
    no_nul_elf[dynstr_end] = b'X';
    assert_eq!(
        parse_elf64_dynamic_metadata(&no_nul_elf),
        Err(Elf64ParseError::UnterminatedDynamicString { offset: 1 })
    );

    // 12e. Verify FIPS 180-4 SHA-256 across 0B, 55B, 56B, 63B, 64B, 65B, 1000B, and 1 MiB (`sha256_bytes` & `sha256_file`).
    let sha_vectors: &[(usize, &str)] = &[
        (
            0,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            55,
            "c096925d8ee622ec75c396b1712bcb599952e05c2cd1f3e33032514893edbc83",
        ),
        (
            56,
            "4e3bd718369bf855312b6fae0e874c697a5c910c0270ebab390f15872a7878a5",
        ),
        (
            63,
            "e5e8e5e7481532428a3d324d4163fe92fea5f52e85fbe33abfa5aa9235c5addf",
        ),
        (
            64,
            "f616afd08efb65e91c7fa856fff5e2b991980c5b3fbdffb81f1e4d9dbdb509d7",
        ),
        (
            65,
            "8ce44bf6e55d58f6f5fa326e9fa574cf8fd80adcaf299d4b2f93e9b7414a9821",
        ),
        (
            1_000,
            "157bac17a97efb50756d4af6b73d2c6e41fee6c4012fa21a6c638eb2febae3cb",
        ),
        (
            1_048_576,
            "042d84d3772b8cc9080a8f388c428c11d88f8566e5a38fda322fc8ea33492cfc",
        ),
    ];

    for &(byte_len, expected_hex) in sha_vectors {
        let payload = deterministic_pattern_bytes(byte_len);
        assert_eq!(
            sha256_bytes(&payload),
            expected_hex,
            "sha256_bytes mismatch at length {byte_len}"
        );
        let vec_file = tmp.path().join(format!("sha256_vec_{byte_len}.bin"));
        fs::write(&vec_file, &payload).unwrap();
        assert_eq!(
            sha256_file(&vec_file).unwrap(),
            expected_hex,
            "sha256_file mismatch at length {byte_len}"
        );
    }
}

// ---------------------------------------------------------------------------
// Regression 13. Permission denial (`!is_executable`, `noexec` mount, `0o777`, `UID == 0`)
// ---------------------------------------------------------------------------
#[test]
fn regression_13_permission_denial_non_exec_noexec_mount_0777_and_root() {
    let tmp = tempfile::tempdir().unwrap();
    let fs_root = tmp.path().join("ubuntu2404");
    fs::create_dir_all(fs_root.join("etc/apparmor.d")).unwrap();
    fs::create_dir_all(fs_root.join("sys/module/apparmor/parameters")).unwrap();
    fs::create_dir_all(fs_root.join("proc/sys/kernel")).unwrap();
    fs::create_dir_all(fs_root.join("dev")).unwrap();
    fs::create_dir_all(fs_root.join("tmp/noexec downloads")).unwrap();
    fs::create_dir_all(fs_root.join("var/tmp/noexec_profiles")).unwrap();
    fs::create_dir_all(fs_root.join("opt/apps")).unwrap();

    fs::write(
        fs_root.join("etc/os-release"),
        "ID=ubuntu\nVERSION_ID=\"24.04\"\n",
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
    fs::write(fs_root.join("dev/fuse"), b"").unwrap();
    fs::write(
        fs_root.join("proc/mounts"),
        concat!(
            "/dev/nvme0n1p2 / ext4 rw,relatime 0 0\n",
            "tmpfs /tmp/noexec\\040downloads tmpfs rw,nosuid,nodev,noexec 0 0\n",
            "tmpfs /var/tmp/noexec_profiles tmpfs rw,nosuid,nodev,noexec 0 0\n",
        ),
    )
    .unwrap();

    // 13a. Verify `/proc/mounts` `noexec` parsing, octal `\040` unescaping, and `MountOptionNoexec` preflight!
    let mounts = parse_proc_mounts(&fs::read_to_string(fs_root.join("proc/mounts")).unwrap());
    assert_eq!(mounts.len(), 3);
    let m_noexec = find_mount_for_path(
        &mounts,
        &fs_root.join("tmp/noexec downloads/Zalo.AppImage"),
        &fs_root,
    )
    .unwrap();
    assert_eq!(m_noexec.mount_point, PathBuf::from("/tmp/noexec downloads"));
    assert!(m_noexec.has_option("noexec"));

    let valid_elf = build_synthetic_elf64_dynamic_binary(
        host_elf_machine(),
        true,
        Some("/lib64/ld-linux-x86-64.so.2"),
        None,
        &[],
        &[],
        &[],
    );
    let bin_on_noexec = fs_root.join("tmp/noexec downloads/Zalo-26.11.10.AppImage");
    write_executable_file(&bin_on_noexec, &valid_elf);

    let mut wl_env = HashMap::new();
    wl_env.insert("WAYLAND_DISPLAY", "wayland-0");
    let user_probe = probe_ubuntu_runtime_at(
        &fs_root,
        std::env::consts::ARCH,
        1000,
        &|k| wl_env.get(k).map(|v| (*v).to_owned()),
        None,
    );

    let preflight_noexec_bin =
        evaluate_appimage_preflight(&user_probe, &fs_root, &bin_on_noexec, None, &[], false);
    assert!(!preflight_noexec_bin.ready_to_launch);
    assert!(preflight_noexec_bin.diagnostics.iter().any(|d| {
        d.severity == PreflightSeverity::Blocking
            && matches!(
                &d.kind,
                PreflightDiagnosticKind::MountOptionNoexec { mount_point, .. }
                    if mount_point == "/tmp/noexec downloads"
            )
    }));

    // AppImage on exec-enabled `/opt/apps`, while `profile_dir/tmp` is on `noexec` mount:
    let bin_on_exec = fs_root.join("opt/apps/Zalo-26.11.10.AppImage");
    write_executable_file(&bin_on_exec, &valid_elf);
    let prof_on_noexec = fs_root.join("var/tmp/noexec_profiles/work_1300aaaa");
    let p_noexec = ProfileRecord {
        id: "1300aaaa".to_owned(),
        name: "Noexec Scratch".to_owned(),
        directory_name: "work_1300aaaa".to_owned(),
        created_at: "2026-11-18T00:00:00Z".to_owned(),
        last_launched_at: None,
        zalo_user_id: None,
        notes: String::new(),
    };
    ensure_linux_profile_layout(&p_noexec, &prof_on_noexec).unwrap();

    let preflight_noexec_tmp = evaluate_appimage_preflight(
        &user_probe,
        &fs_root,
        &bin_on_exec,
        Some(&prof_on_noexec),
        &[],
        false,
    );
    assert!(!preflight_noexec_tmp.ready_to_launch);
    assert!(preflight_noexec_tmp.diagnostics.iter().any(|d| {
        d.severity == PreflightSeverity::Blocking
            && matches!(
                &d.kind,
                PreflightDiagnosticKind::MountOptionNoexec { mount_point, .. }
                    if mount_point == "/var/tmp/noexec_profiles"
            )
    }));

    // 13b. AppArmor bare-filename non-bypass check: system `/usr/lib/electron/electron` profile
    //      must NOT authorize `/home/alice/Downloads/electron`!
    fs::write(
        fs_root.join("etc/apparmor.d/electron"),
        "abi <abi/4.0>,\nprofile electron /usr/lib/electron/electron flags=(unconfined) { userns, }\n",
    )
    .unwrap();
    assert!(!has_targeted_apparmor_userns_profile(
        &fs_root,
        Path::new("/home/alice/Downloads/electron")
    ));
    let exact_snippet = render_recommended_apparmor_profile(&bin_on_exec);
    fs::write(fs_root.join("etc/apparmor.d/local-zalo"), &exact_snippet).unwrap();
    assert!(has_targeted_apparmor_userns_profile(&fs_root, &bin_on_exec));

    // 13c. `UID == 0` root execution blocked across `enforce_launch_security_policy`, `evaluate_appimage_preflight`, and `adapter.launch`.
    let root_probe = probe_ubuntu_runtime_at(
        &fs_root,
        std::env::consts::ARCH,
        0,
        &|k| wl_env.get(k).map(|v| (*v).to_owned()),
        None,
    );
    assert_eq!(
        enforce_launch_security_policy(&root_probe, &fs_root, &bin_on_exec, &[], true, false),
        Err(SecurityBlockReason::RunningAsRoot { euid: 0 })
    );
    let root_preflight =
        evaluate_appimage_preflight(&root_probe, &fs_root, &bin_on_exec, None, &[], false);
    assert!(!root_preflight.ready_to_launch);
    assert_eq!(
        root_preflight.verification_label,
        VerificationLabel::SecurityBlocked
    );
    assert!(root_preflight.diagnostics.iter().any(|d| {
        d.severity == PreflightSeverity::Blocking
            && d.kind == PreflightDiagnosticKind::RunningAsRoot { euid: 0 }
    }));

    // Verify `MissingExecutableBit` category/code mapping portably.
    let missing_exec_kind = PreflightDiagnosticKind::MissingExecutableBit {
        path: bin_on_exec.display().to_string(),
    };
    assert_eq!(
        missing_exec_kind.category(),
        PreflightCategory::FilesystemAndMount
    );
    assert_eq!(missing_exec_kind.code(), "MISSING_EXECUTABLE_BIT");

    // 13d. POSIX-specific permission denials (`0o644` `!is_executable`, `0o777` world-writable `Profiles`, wrong UID).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let non_exec_bin = tmp.path().join("NonExecZalo-26.11.10.AppImage");
        fs::write(&non_exec_bin, &valid_elf).unwrap();
        fs::set_permissions(&non_exec_bin, fs::Permissions::from_mode(0o644)).unwrap();

        let non_exec_rep = inspect_user_binary(&non_exec_bin, None).unwrap();
        assert!(!non_exec_rep.is_executable);

        let euid = dim_platform_linux::current_effective_uid();
        let unix_probe = probe_ubuntu_runtime_at(
            &fs_root,
            std::env::consts::ARCH,
            euid,
            &|k| wl_env.get(k).map(|v| (*v).to_owned()),
            None,
        );
        let preflight_non_exec =
            evaluate_appimage_preflight(&unix_probe, &fs_root, &non_exec_bin, None, &[], false);
        assert!(preflight_non_exec
            .diagnostics
            .iter()
            .any(|d| d.kind.code() == "MISSING_EXECUTABLE_BIT"));

        let unix_adapter = LinuxPlatformAdapter::with_virtual_roots(
            fs_root.clone(),
            tmp.path().join("proc"),
            tmp.path().join("primary"),
            euid,
            std::env::consts::ARCH,
            None,
        );
        let launch_non_exec_err = unix_adapter
            .launch(
                &LaunchTarget::CustomExecutable {
                    path: non_exec_bin,
                    args: Vec::new(),
                },
                &p_noexec,
                &prof_on_noexec,
            )
            .unwrap_err();
        assert_eq!(launch_non_exec_err.code(), "PROCESS_ERROR");
        assert!(launch_non_exec_err
            .to_string()
            .contains("not marked executable (+x)"));

        // `0o777` world-writable `Profiles` directory & `profile_dir` rejected!
        let world_profiles = tmp.path().join("WorldWritableProfiles");
        fs::create_dir_all(&world_profiles).unwrap();
        fs::set_permissions(&world_profiles, fs::Permissions::from_mode(0o777)).unwrap();
        assert_eq!(
            unix_adapter.check_profiles_acl_safe(&world_profiles),
            Some(false)
        );
        assert!(verify_directory_ownership_and_mode(&world_profiles, euid, false).is_err());

        let preflight_bad_prof = evaluate_appimage_preflight(
            &unix_probe,
            &fs_root,
            &bin_on_exec,
            Some(&world_profiles),
            &[],
            false,
        );
        assert_eq!(
            preflight_bad_prof.verification_label,
            VerificationLabel::SecurityBlocked
        );
        assert!(preflight_bad_prof
            .diagnostics
            .iter()
            .any(|d| d.kind.code() == "INSECURE_FILESYSTEM_PERMISSIONS"));

        // Wrong-UID check (`euid + 1`) rejected!
        assert!(
            verify_directory_ownership_and_mode(&prof_on_noexec, euid.saturating_add(1), true)
                .is_err()
        );
    }
}

// ---------------------------------------------------------------------------
// Regression 14. Restart persistence across graceful stop & relaunch
// ---------------------------------------------------------------------------
#[test]
fn regression_14_restart_persistence_across_graceful_stop_and_relaunch() {
    let tmp = tempfile::tempdir().unwrap();
    let store = JsonProfileStore::new(tmp.path().join("RestartPersistStore")).unwrap();
    let p = store.create_profile("Persistent Account", "").unwrap();
    let pdir = store.profile_dir_for(&p).unwrap();
    let zdir = linux_zalo_data_dir(&pdir);
    let _cleanup = QuitSignalGuard {
        zalo_data_dir: zdir.clone(),
    };

    // Launch #1
    let mut run1 = spawn_linux_env_fixture(&p, &pdir, &["--user-id=849666777", "--stay-ms=12000"]);
    assert!(wait_for_file(&zdir.join("lockfile"), 4_000));
    assert!(wait_for_file(&zdir.join("launch-state.json"), 4_000));
    let pid1 = dim_platform_linux::read_linux_pid(&zdir).unwrap();

    let state1: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(zdir.join("launch-state.json")).unwrap()).unwrap();
    assert_eq!(state1["launch_count"], 1);
    assert_eq!(state1["zalo_user_id"], "849666777");

    // Persist session DB & downloaded attachment during run #1.
    let db_file = zdir
        .join("Database")
        .join("_production")
        .join("849666777")
        .join("ftsm_849666777.db");
    let dl_file = pdir.join("Downloads").join("statement_nov2026.pdf");
    fs::write(&db_file, b"sqlite-wal-checkpointed-messages-v1").unwrap();
    fs::write(&dl_file, b"%PDF-1.7-isolated-download").unwrap();

    let detected_uid_1 = dim_core::detect_zalo_user_id(&zdir);
    assert_eq!(detected_uid_1.as_deref(), Some("849666777"));
    let updated_record = store
        .update_last_launched(&p.id, "2026-11-18T01:00:00Z", detected_uid_1.as_deref())
        .unwrap();
    assert_eq!(updated_record.zalo_user_id.as_deref(), Some("849666777"));

    // Graceful stop of Launch #1.
    fs::write(zdir.join(".quit-signal"), "{\"requested_at\":\"now\"}\n").unwrap();
    if let Some(ref mut c1) = run1.0 {
        let status = c1.wait().unwrap();
        assert!(status.success());
    }
    assert!(!zdir.join("lockfile").exists());
    let stopped_state1: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(zdir.join("launch-state.json")).unwrap()).unwrap();
    assert_eq!(stopped_state1["clean_shutdown"], true);

    // Relaunch (#2) on the exact same profile directory!
    let mut run2 = spawn_linux_env_fixture(
        &updated_record,
        &pdir,
        &["--user-id=849666777", "--stay-ms=12000"],
    );
    let deadline = Instant::now() + Duration::from_secs(4);
    let mut launch_count_2 = false;
    while Instant::now() < deadline {
        if let Ok(raw) = fs::read_to_string(zdir.join("launch-state.json")) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                if v["launch_count"].as_u64() == Some(2) && zdir.join("lockfile").exists() {
                    launch_count_2 = true;
                    break;
                }
            }
        }
        thread::sleep(Duration::from_millis(25));
    }
    assert!(
        launch_count_2,
        "relaunch after graceful stop must reach launch_count == 2"
    );

    let pid2 = dim_platform_linux::read_linux_pid(&zdir).unwrap();
    assert_eq!(
        dim_core::detect_zalo_user_id(&zdir).as_deref(),
        Some("849666777")
    );
    let session2: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(
            zdir.join("Database")
                .join("_production")
                .join("849666777")
                .join("session.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(session2["zalo_user_id"], "849666777");
    assert_eq!(session2["pid"].as_u64(), Some(u64::from(pid2)));
    let _ = pid1;

    // Verify custom SQLite DB, Downloads file, and `.zalo-profile-id` survived restart intact!
    assert_eq!(
        fs::read(&db_file).unwrap(),
        b"sqlite-wal-checkpointed-messages-v1"
    );
    assert_eq!(fs::read(&dl_file).unwrap(), b"%PDF-1.7-isolated-download");
    assert_eq!(
        fs::read_to_string(zdir.join(ZALO_PROFILE_ID_FILE)).unwrap(),
        p.id
    );

    // Cleanly stop Launch #2.
    fs::write(zdir.join(".quit-signal"), "{\"requested_at\":\"now\"}\n").unwrap();
    if let Some(ref mut c2) = run2.0 {
        let status = c2.wait().unwrap();
        assert!(status.success());
    }
    assert!(!zdir.join("lockfile").exists());
}

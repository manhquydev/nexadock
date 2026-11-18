//! Linux platform adapter crate (`dim-platform-linux`).
//!
//! Provides isolated `HOME` + XDG Base Directory management, `/proc` process-tree ownership
//! tracking with `(pid, starttime_ticks)` + `DIM_INSTANCE_NONCE` PID-reuse defense,
//! ELF64/AppImage provenance & SHA-256 verification, startup-script hazard auditing,
//! least-privilege `bubblewrap` (`bwrap`) containerization, and Ubuntu 24.04 AppArmor
//! `SECURITY_BLOCKED` enforcement.

pub mod adapter;
pub mod discovery;
pub mod isolation;
pub mod layout;
pub mod procfs;
pub mod status;

pub use adapter::{
    derive_deterministic_fixture_user_id, pid_holds_open_sqlite_db_fd, read_linux_pid,
    LinuxPlatformAdapter,
};
pub use discovery::{
    analyze_startup_script, classify_binary_bytes, discover_compatible_apps_from_env,
    discover_compatible_apps_in_roots, inspect_elf64_needed_libraries,
    inspect_elf64_needed_libraries_at, inspect_user_binary, map_soname_to_ubuntu_packages,
    match_known_community_release, parse_desktop_entry, parse_elf64_dynamic_metadata,
    parse_ldd_missing_libraries, parse_version_from_filename, read_elf64_dynamic_metadata,
    resolve_elf64_missing_libraries_at, sha256_bytes, sha256_file, BinaryFormatKind,
    BinaryInspectionReport, CommunityReleaseReference, DesktopEntryFields, DiscoveredLinuxApp,
    Elf64DynamicMetadata, Elf64ParseError, ElfCpuArch, MissingLibraryDiagnostic,
    StartupScriptAudit, KNOWN_COMMUNITY_RELEASES,
};
pub use isolation::{
    compare_linux_isolation_mechanisms, current_effective_uid, enforce_launch_security_policy,
    evaluate_appimage_preflight, find_executable_in_path, find_mount_for_path,
    has_targeted_apparmor_userns_profile, parse_proc_mounts, preflight_live_appimage,
    probe_live_ubuntu_runtime, probe_ubuntu_runtime_at, render_recommended_apparmor_profile,
    AppImagePreflightReport, DisplaySessionKind, IsolationComparisonEntry, LinuxIsolationMechanism,
    PreflightDiagnostic, ProcMountEntry, UbuntuRuntimeProbe,
    APPARMOR_PROFILE_NON_GUARANTEE_WARNING,
};
pub use layout::{
    build_bwrap_args, build_bwrap_desktop_args, build_bwrap_desktop_args_with_policy,
    build_linux_isolated_env, build_sanitized_linux_spawn_env,
    build_sanitized_linux_spawn_env_with_nonce, ensure_linux_profile_layout,
    generate_instance_nonce, is_safe_bindable_socket_or_file, linux_zalo_data_dir,
    looks_like_appimage, measure_profile_disk_usage_bytes, parse_dbus_session_socket_path,
    parse_x11_display_socket_path, plan_bwrap_desktop_sandbox, resolve_pipewire_socket_path,
    resolve_pulse_socket_path, resolve_wayland_socket_path, verify_directory_ownership_and_mode,
    BwrapSandboxOptions, BwrapSandboxPlan, DesktopSocketSurface, LauncherLifetimePolicy,
    APPARMOR_NO_SANDBOX_NOTE, DIM_INSTANCE_NONCE_ENV, DIM_PROFILE_ID_ENV,
    PASSTHROUGH_SESSION_ENV_KEYS,
};
pub use procfs::{
    collect_primary_process_tree, collect_profile_process_tree, entry_matches_profile,
    parse_proc_cmdline, parse_proc_environ, parse_proc_stat, parse_proc_status,
    parse_singleton_lock_target, parse_wmctrl_windows_for_pids, read_lock_pid, read_proc_entry,
    read_proc_stat_starttime, read_runtime_pid_hint, runtime_hint_matches_entry, scan_proc_table,
    OwnedX11Window, ProcEntry, ProfileProcessTree, RuntimePidHint,
};
pub use status::{
    PreflightCategory, PreflightDiagnosticKind, PreflightSeverity, SecurityBlockReason,
    UbuntuTargetTier, VerificationLabel,
};

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::collections::HashMap;
    use std::fs;
    use std::path::{Path, PathBuf};

    use dim_core::{
        ApplicationLocator, InstanceLauncher, InstanceMonitor, LaunchTarget, PlatformCapabilities,
        ProfileRecord, ProfileRuntimeState, SupportStatus, ZALO_PROFILE_ID_FILE,
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

    #[allow(clippy::too_many_arguments)]
    fn write_synthetic_proc(
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
        if let Some(env_pairs) = environ {
            let mut env_bytes = Vec::new();
            for (k, v) in env_pairs {
                env_bytes.extend_from_slice(format!("{k}={v}").as_bytes());
                env_bytes.push(0);
            }
            fs::write(dir.join("environ"), env_bytes).unwrap();
        }
        let mut cmd_bytes = Vec::new();
        for arg in cmdline {
            cmd_bytes.extend_from_slice(arg.as_bytes());
            cmd_bytes.push(0);
        }
        fs::write(dir.join("cmdline"), cmd_bytes).unwrap();
    }

    #[test]
    fn linux_reports_official_unsupported_and_appimage_experimental() {
        let tmp = tempfile::tempdir().unwrap();
        let appimage = tmp.path().join("ZaloUserSupplied.AppImage");
        fs::write(&appimage, b"appimage").unwrap();

        let adapter = LinuxPlatformAdapter::new();
        assert_eq!(
            adapter.lifetime_policy(),
            LauncherLifetimePolicy::PersistentDetachedSession
        );
        let caps = adapter.capabilities();
        assert_eq!(caps.official_zalo_status, SupportStatus::Unsupported);
        assert_eq!(caps.fixture_status, SupportStatus::Experimental);
        assert_eq!(caps.appimage_status, Some(SupportStatus::Experimental));
        assert!(caps
            .limitations
            .iter()
            .any(|l| l.contains("--no-sandbox") && l.contains("AppArmor")));
        assert!(caps
            .limitations
            .iter()
            .any(|l| l.contains("UNSUPPORTED_OPERATION on Wayland")));
        assert!(caps
            .diagnostics
            .iter()
            .any(|d| d.contains("SANDBOX_TRADEOFF [dbus-session]")));

        let official = adapter.discover_installation(None).unwrap();
        assert!(!official.is_valid);
        assert_eq!(official.support_status, SupportStatus::Unsupported);

        let local_appimage = adapter.discover_installation(Some(&appimage)).unwrap();
        assert!(local_appimage.is_valid);
        assert_eq!(local_appimage.support_status, SupportStatus::Experimental);
    }

    #[test]
    fn linux_layout_and_bwrap_lifetime_policies_isolate_xdg_and_home() {
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

        // Default PersistentDetachedSession uses `--new-session` WITHOUT `--die-with-parent`
        // so desktop instances survive short-lived `dim-cli launch <id>` invocations.
        let detached = build_bwrap_args(&pdir, Path::new("/tmp/fixture.AppImage"), &[]);
        assert!(detached.contains(&"--new-session".to_owned()));
        assert!(!detached.contains(&"--die-with-parent".to_owned()));
        assert!(detached.contains(&"--unshare-ipc".to_owned()));
        assert!(!detached.contains(&"--no-sandbox".to_owned()));

        // Opt-in DieWithParent passes `--die-with-parent` for transient bound harnesses.
        let nonce = generate_instance_nonce();
        assert_eq!(nonce.len(), 32);
        let bound = build_bwrap_desktop_args_with_policy(
            &pdir,
            Path::new("/opt/zalo/Zalo.AppImage"),
            &[],
            LauncherLifetimePolicy::DieWithParent,
            Some(&profile.id),
            Some(&nonce),
            &|_| None,
        );
        assert!(bound.contains(&"--die-with-parent".to_owned()));
        assert!(!bound.contains(&"--new-session".to_owned()));
        assert!(bound
            .windows(3)
            .any(|w| w == ["--setenv", "DIM_PROFILE_ID", "cafebabe"]));
        assert!(bound
            .windows(3)
            .any(|w| w[0] == "--setenv" && w[1] == "DIM_INSTANCE_NONCE" && w[2] == nonce));
    }

    #[test]
    fn bwrap_sandbox_plan_enforces_minimal_socket_binds_and_rejects_system_dbus() {
        let tmp = tempfile::tempdir().unwrap();
        let profile = sample_profile();
        let pdir = tmp.path().join(&profile.directory_name);
        ensure_linux_profile_layout(&profile, &pdir).unwrap();

        // Validate D-Bus session socket parser edge cases and system-D-Bus rejection.
        assert_eq!(
            parse_dbus_session_socket_path(
                "unix:path=/tmp/dbus-custom%20session,guid=0123456789abcdef"
            ),
            Some(PathBuf::from("/tmp/dbus-custom session"))
        );
        assert_eq!(
            parse_dbus_session_socket_path("unix:abstract=/tmp/dbus-abs,guid=1111"),
            None
        );
        assert_eq!(
            parse_dbus_session_socket_path("unix:path=/run/dbus/system_bus_socket"),
            None
        );
        assert_eq!(
            parse_dbus_session_socket_path("unix:path=/var/run/dbus/system_bus_socket"),
            None
        );
        assert_eq!(
            parse_dbus_session_socket_path("unix:path=/run/user/1000/../etc/shadow"),
            None
        );

        // Validate X11 display socket parser.
        assert_eq!(
            parse_x11_display_socket_path(":0.0"),
            Some(PathBuf::from("/tmp/.X11-unix/X0"))
        );
        assert_eq!(
            parse_x11_display_socket_path("unix:2"),
            Some(PathBuf::from("/tmp/.X11-unix/X2"))
        );
        assert_eq!(parse_x11_display_socket_path("localhost:10.0"), None);

        // Pure Wayland + custom D-Bus outside XDG_RUNTIME_DIR/bus + PipeWire + PulseAudio:
        let mut wayland_env = HashMap::new();
        wayland_env.insert("HOME", "/home/alice");
        wayland_env.insert("XDG_RUNTIME_DIR", "/run/user/1000");
        wayland_env.insert("WAYLAND_DISPLAY", "wayland-0");
        wayland_env.insert(
            "DBUS_SESSION_BUS_ADDRESS",
            "unix:path=/tmp/dbus-isolated-bus,guid=abcd",
        );

        let opts = BwrapSandboxOptions {
            lifetime_policy: LauncherLifetimePolicy::PersistentDetachedSession,
            profile_id: Some(&profile.id),
            instance_nonce: Some("nonce123"),
            prefer_portal_file_chooser: true,
            extra_ro_files: &[],
        };
        let wl_plan = plan_bwrap_desktop_sandbox(
            &pdir,
            Path::new("/home/alice/Apps/Zalo.AppImage"),
            &[],
            &opts,
            &|k| wayland_env.get(k).map(|v| (*v).to_owned()),
            &|_| true,
        );

        // Never bind real $HOME, full /run/user/1000, full `pulse` dir, or `/tmp/.X11-unix`!
        assert!(!wl_plan.args.windows(3).any(|w| (w[0] == "--bind"
            || w[0] == "--ro-bind"
            || w[0] == "--ro-bind-try")
            && (w[1] == "/home/alice"
                || w[1] == "/run/user/1000"
                || w[1] == "/run/user/1000/pulse"
                || w[1] == "/tmp/.X11-unix"
                || w[1] == "/dev")));
        assert!(wl_plan.args.windows(3).any(|w| w
            == [
                "--ro-bind-try",
                "/run/user/1000/wayland-0",
                "/run/user/1000/wayland-0"
            ]));
        assert!(wl_plan.args.windows(3).any(|w| w
            == [
                "--ro-bind-try",
                "/tmp/dbus-isolated-bus",
                "/tmp/dbus-isolated-bus"
            ]));
        assert!(wl_plan.args.windows(3).any(|w| w
            == [
                "--ro-bind-try",
                "/run/user/1000/pipewire-0",
                "/run/user/1000/pipewire-0"
            ]));
        assert!(wl_plan.args.windows(3).any(|w| w
            == [
                "--ro-bind-try",
                "/run/user/1000/pulse/native",
                "/run/user/1000/pulse/native"
            ]));
        assert!(wl_plan
            .args
            .windows(3)
            .any(|w| w == ["--setenv", "GTK_USE_PORTAL", "1"]));
        assert!(!wl_plan
            .exposed_surfaces
            .contains(&DesktopSocketSurface::X11Server));
        assert!(wl_plan
            .security_tradeoff_notes
            .iter()
            .any(|n| n.contains("SANDBOX_TRADEOFF [dbus-session]")));
        assert!(wl_plan
            .security_tradeoff_notes
            .iter()
            .any(|n| n.contains("SANDBOX_TRADEOFF [pipewire]")));

        // When X11 DISPLAY=:1 is active, bind ONLY `/tmp/.X11-unix/X1` and surface X11 tradeoff note.
        wayland_env.insert("DISPLAY", ":1");
        wayland_env.insert("XAUTHORITY", "/run/user/1000/xauth_token");
        let x11_plan = plan_bwrap_desktop_sandbox(
            &pdir,
            Path::new("/opt/zalo/zalo"),
            &[],
            &opts,
            &|k| wayland_env.get(k).map(|v| (*v).to_owned()),
            &|p| p != Path::new("/run/user/1000/pipewire-0"),
        );
        assert!(x11_plan
            .args
            .windows(3)
            .any(|w| w == ["--ro-bind-try", "/tmp/.X11-unix/X1", "/tmp/.X11-unix/X1"]));
        assert!(x11_plan.args.windows(3).any(|w| w
            == [
                "--ro-bind-try",
                "/run/user/1000/xauth_token",
                "/run/user/1000/xauth_token"
            ]));
        // Missing `pipewire-0` rejected by `endpoint_exists` must NOT be bound!
        assert!(!x11_plan
            .exposed_surfaces
            .contains(&DesktopSocketSurface::PipeWireDaemon));
        assert!(x11_plan
            .security_tradeoff_notes
            .iter()
            .any(|n| n.contains("SANDBOX_TRADEOFF [x11]")));
    }

    #[test]
    fn proc_stat_starttime_and_nonce_prevent_pid_reuse_and_order_bwrap_leaves_first() {
        let raw_stat = "2001 (Web Content (isolated) 1) S 2000 2000 2000 0 -1 4194304 100 0 0 0 10 5 0 0 20 0 4 0 987654321 10485760 256 18446744073709551615\n";
        let (pid, comm, state, ppid, pgrp, starttime) = parse_proc_stat(raw_stat).unwrap();
        assert_eq!(pid, 2001);
        assert_eq!(comm, "Web Content (isolated) 1");
        assert_eq!(state, 'S');
        assert_eq!(ppid, 2000);
        assert_eq!(pgrp, 2000);
        assert_eq!(starttime, 987_654_321);

        let tmp = tempfile::tempdir().unwrap();
        let fs_root = tmp.path().join("fs");
        let proc_root = tmp.path().join("proc");
        let primary_cfg = tmp.path().join("primary");
        fs::create_dir_all(&fs_root).unwrap();
        fs::create_dir_all(&proc_root).unwrap();
        fs::create_dir_all(&primary_cfg).unwrap();

        let profile = sample_profile();
        let pdir = tmp.path().join(&profile.directory_name);
        ensure_linux_profile_layout(&profile, &pdir).unwrap();
        let zdir = linux_zalo_data_dir(&pdir);

        // Simulate `bwrap` wrapper (PID 2000, dumpable=0 -> unreadable `environ`, `--setenv` in cmdline),
        // inner app main (PID 2001, holding `lockfile`), `zygote` (2002), `renderer` (2003),
        // and reparented `zocr-host` (2099) & `zcall-native` (2100).
        let nonce = "a1b2c3d4e5f60718293a4b5c6d7e8f90";
        fs::write(
            pdir.join(".runtime-pid"),
            serde_json::json!({
                "pid": 2000,
                "profile_id": profile.id,
                "instance_nonce": nonce,
                "starttime_ticks": 50_000_u64,
            })
            .to_string(),
        )
        .unwrap();
        fs::write(zdir.join("lockfile"), "{\"pid\":2001}\n").unwrap();

        let pdir_str = pdir.to_string_lossy().into_owned();
        write_synthetic_proc(
            &proc_root,
            2000,
            "bwrap",
            'S',
            1,
            50_000,
            1000,
            4_000,
            None, // dumpable=0: unreadable `/proc/2000/environ`
            &[
                "bwrap",
                "--new-session",
                "--bind",
                &pdir_str,
                &pdir_str,
                "--setenv",
                "DIM_PROFILE_ID",
                &profile.id,
                "--setenv",
                "DIM_INSTANCE_NONCE",
                nonce,
                "--",
                "/opt/zalo/zalo",
            ],
        );
        write_synthetic_proc(
            &proc_root,
            2001,
            "zalo",
            'S',
            2000,
            50_005,
            1000,
            110_000,
            Some(&[
                ("DIM_PROFILE_ID", &profile.id),
                ("DIM_INSTANCE_NONCE", nonce),
            ]),
            &["/opt/zalo/zalo"],
        );
        write_synthetic_proc(
            &proc_root,
            2002,
            "zalo (zygote)",
            'S',
            2001,
            50_010,
            1000,
            30_000,
            None,
            &["/opt/zalo/zalo", "--type=zygote"],
        );
        write_synthetic_proc(
            &proc_root,
            2003,
            "zalo (renderer)",
            'R',
            2002,
            50_015,
            1000,
            75_000,
            None,
            &["/opt/zalo/zalo", "--type=renderer"],
        );
        write_synthetic_proc(
            &proc_root,
            2099,
            "zocr-host",
            'S',
            1,
            50_020,
            1000,
            20_000,
            Some(&[
                ("DIM_PROFILE_ID", &profile.id),
                ("DIM_INSTANCE_NONCE", nonce),
            ]),
            &["zocr-host"],
        );
        write_synthetic_proc(
            &proc_root,
            2100,
            "zcall-native",
            'S',
            1,
            50_025,
            1000,
            25_000,
            Some(&[
                ("DIM_PROFILE_ID", &profile.id),
                ("DIM_INSTANCE_NONCE", nonce),
            ]),
            &["zcall-native"],
        );

        let tree = collect_profile_process_tree(&proc_root, &profile.id, &pdir, &zdir, Some(1000));
        assert_eq!(tree.main_pid, Some(2001));
        assert_eq!(
            tree.process_ids[0], 2000,
            "outer bwrap must be root/first so reverse order kills it last"
        );
        assert!(tree.process_ids.contains(&2001));
        assert!(tree.process_ids.contains(&2002));
        assert!(tree.process_ids.contains(&2003));
        assert!(tree.process_ids.contains(&2099));
        assert!(tree.process_ids.contains(&2100));

        // Also verify unprivileged `bwrap` (`dumpable=1`) where outer `/proc/2000/environ` is
        // non-empty (`HOME=/home/hostuser` without `DIM_PROFILE_ID`), while `--setenv DIM_PROFILE_ID`
        // is passed in `cmdline`: `entry_matches_profile` must still match `2000`!
        write_synthetic_proc(
            &proc_root,
            2000,
            "bwrap",
            'S',
            1,
            50_000,
            1000,
            4_000,
            Some(&[("HOME", "/home/hostuser")]),
            &[
                "bwrap",
                "--new-session",
                "--bind",
                &pdir_str,
                &pdir_str,
                "--setenv",
                "DIM_PROFILE_ID",
                &profile.id,
                "--setenv",
                "DIM_INSTANCE_NONCE",
                nonce,
                "--",
                "/opt/zalo/zalo",
            ],
        );
        let dumpable1_bwrap = read_proc_entry(&proc_root, 2000).unwrap();
        assert!(entry_matches_profile(
            &dumpable1_bwrap,
            &profile.id,
            &pdir,
            Some(1000)
        ));
        let tree_dumpable1 =
            collect_profile_process_tree(&proc_root, &profile.id, &pdir, &zdir, Some(1000));
        assert_eq!(tree_dumpable1.process_ids[0], 2000);

        // Simulate crash + PID reuse: PIDs 2000 and 2001 are reused by later tasks with NEW `starttime_ticks`
        // (99_999) and lacking the launch `nonce`, even if command names happen to match!
        fs::remove_dir_all(&proc_root).unwrap();
        fs::create_dir_all(&proc_root).unwrap();
        write_synthetic_proc(
            &proc_root,
            2000,
            "bwrap",
            'S',
            1,
            99_999, // Mismatched starttime_ticks!
            1000,
            4_000,
            Some(&[("HOME", "/home/other")]),
            &["bwrap"],
        );
        write_synthetic_proc(
            &proc_root,
            2001,
            "zalo",
            'S',
            1,
            99_999, // Recycled PID without matching nonce!
            1000,
            80_000,
            Some(&[
                ("DIM_PROFILE_ID", &profile.id),
                ("DIM_INSTANCE_NONCE", "different_nonce_from_other_run"),
            ]),
            &["/opt/zalo/zalo"],
        );

        let adapter = LinuxPlatformAdapter::with_virtual_roots(
            fs_root,
            proc_root,
            primary_cfg,
            1000,
            "x86_64",
            None,
        );
        let rt_after_reuse = adapter.inspect_profile(&profile, &pdir).unwrap();
        assert!(
            !rt_after_reuse.running,
            "PID reuse with mismatched starttime_ticks / nonce must NEVER be treated as running"
        );
        assert!(rt_after_reuse.stale_lock_detected);
        assert_eq!(rt_after_reuse.main_pid, None);
    }

    #[test]
    fn focus_or_restore_rejects_wayland_and_headless_gui_focus_but_allows_fixture() {
        let tmp = tempfile::tempdir().unwrap();
        let profile = sample_profile();
        let pdir = tmp.path().join(&profile.directory_name);
        ensure_linux_profile_layout(&profile, &pdir).unwrap();
        let zdir = linux_zalo_data_dir(&pdir);

        let running_state = ProfileRuntimeState {
            profile_id: profile.id.clone(),
            profile_name: profile.name.clone(),
            profile_dir: pdir.clone(),
            zalo_data_dir: zdir.clone(),
            lockfile_path: zdir.join("SingletonLock"),
            running: true,
            stale_lock_detected: false,
            main_pid: Some(4000),
            process_ids: vec![4000],
            process_count: 1,
            working_set_bytes: 1024,
            window_handle: None,
            window_title: None,
            detected_zalo_user_id: None,
            disk_usage_bytes: 0,
            status_label: "Running".to_owned(),
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

        let gui_target = LaunchTarget::CustomExecutable {
            path: PathBuf::from("/opt/zalo/Zalo.AppImage"),
            args: Vec::new(),
        };
        let wl_err = wayland_adapter
            .focus_or_restore(&gui_target, &profile, &pdir, &running_state)
            .unwrap_err();
        assert_eq!(wl_err.code(), "UNSUPPORTED_PLATFORM");
        assert!(wl_err.to_string().contains("UNSUPPORTED_OPERATION"));
        assert!(wl_err.to_string().contains("Wayland"));

        let headless_adapter = LinuxPlatformAdapter::with_virtual_roots(
            tmp.path().join("fs"),
            tmp.path().join("proc"),
            tmp.path().join("primary"),
            1000,
            "x86_64",
            None,
        );
        let hl_err = headless_adapter
            .focus_or_restore(&gui_target, &profile, &pdir, &running_state)
            .unwrap_err();
        assert_eq!(hl_err.code(), "UNSUPPORTED_PLATFORM");
        assert!(hl_err.to_string().contains("UNSUPPORTED_OPERATION"));
        assert!(hl_err.to_string().contains("headless"));

        // NodeFixture target writes `.focus-signal` deterministically even on Wayland.
        let fixture_target = LaunchTarget::NodeFixture {
            node_path: None,
            script_path: PathBuf::from("fixture-app.mjs"),
            extra_args: Vec::new(),
        };
        wayland_adapter
            .focus_or_restore(&fixture_target, &profile, &pdir, &running_state)
            .unwrap();
        assert!(zdir.join(".focus-signal").is_file());
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

        // Invalid non-ELF `.AppImage` must be rejected by `launch()` with `UNVERIFIED_BINARY`.
        let bogus_appimage = tmp.path().join("CorruptGarbage.AppImage");
        fs::write(&bogus_appimage, b"not-an-elf-appimage-at-all").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&bogus_appimage, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let bogus_err = adapter
            .launch(
                &LaunchTarget::CustomExecutable {
                    path: bogus_appimage,
                    args: Vec::new(),
                },
                &profile,
                &pdir,
            )
            .unwrap_err();
        assert_eq!(bogus_err.code(), "UNVERIFIED_BINARY");
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

    #[test]
    fn deterministic_fixture_uid_and_open_sqlite_fd_guard_work() {
        let uid_a = derive_deterministic_fixture_user_id("11223344");
        let uid_b = derive_deterministic_fixture_user_id("55667788");
        assert_ne!(uid_a, uid_b);
        assert_eq!(uid_a.len(), 7);
        assert_eq!(uid_a, derive_deterministic_fixture_user_id("11223344"));

        let tmp = tempfile::tempdir().unwrap();
        let zdata = tmp.path().join(".config").join("ZaloData");
        let prod = zdata.join("Database").join("_production").join("849111222");
        fs::create_dir_all(&prod).unwrap();

        let proc_root = tmp.path().join("proc");
        let fd_dir = proc_root.join("7777").join("fd");
        fs::create_dir_all(&fd_dir).unwrap();
        assert!(!pid_holds_open_sqlite_db_fd(&proc_root, 7777, &zdata));

        fs::write(
            fd_dir.join("4"),
            prod.join("zalo_msg.db-wal").display().to_string(),
        )
        .unwrap();
        assert!(pid_holds_open_sqlite_db_fd(&proc_root, 7777, &zdata));
    }
}

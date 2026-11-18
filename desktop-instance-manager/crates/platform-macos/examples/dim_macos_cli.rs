//! JSON CLI harness (`dim_macos_cli`) driving `dim-platform-macos`, `dim-core`, and
//! `dim-profile-store` directly for end-to-end macOS and cross-platform fixture verification.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use dim_core::{
    check_duplicate_zalo_user_id, detect_zalo_user_id, ensure_min_free_disk_space, now_rfc3339_utc,
    redact_sensitive_text, resolve_safe_child, validate_directory_input, ApplicationLocator,
    BackupManager, DimError, InstanceLauncher, InstanceMonitor, LaunchTarget, PlatformCapabilities,
    ProfileRecord, ProfileStore, RecoveryManager, MIN_FREE_DISK_BYTES,
};
use dim_platform_macos::{
    verify_12_surface_runtime_isolation, write_runtime_launch_record, HostFixtureMacOsOps,
    MacOsLayoutOptions, MacOsPlatformAdapter, MacOsSystemOps, ProcessEntry, RuntimeLaunchRecord,
};
use dim_profile_store::{verify_profile_id_marker, FileBackupManager, JsonProfileStore};
use serde_json::{json, Value};

struct CliArgs {
    data_dir: Option<PathBuf>,
    simulated_home: Option<PathBuf>,
    short_tmp_root: Option<PathBuf>,
    fixture_host: bool,
    node_fixture: bool,
    app_path: Option<PathBuf>,
    with_size: bool,
    confirm: bool,
    spawn_helper: bool,
    orphan_helper: bool,
    enforce_sun_len: bool,
    crash: bool,
    crash_delay_ms: Option<u64>,
    delayed_exit_ms: Option<u64>,
    hang_on_quit: bool,
    stay_ms: Option<u64>,
    slow_startup_pre_lock_ms: Option<u64>,
    user_id: Option<String>,
    account_label: Option<String>,
    hold_ms: Option<u64>,
    timeout_ms: u64,
    dest: Option<PathBuf>,
    name: Option<String>,
    notes: String,
    symlink_user_dirs: bool,
    symlink_keychain: bool,
    pinned_bundle_id: Option<String>,
    pinned_team_id: Option<String>,
    positional: Vec<String>,
}

fn main() -> ExitCode {
    let args = parse_cli(env::args().skip(1));
    let home_for_redact = args
        .simulated_home
        .clone()
        .or_else(|| env::var_os("HOME").map(PathBuf::from));

    let (ok, out_json) = match run_cli(args) {
        Ok(val) => {
            let ok_flag = val.get("ok").and_then(Value::as_bool).unwrap_or(true);
            (ok_flag, val)
        }
        Err(err) => (
            false,
            json!({
                "ok": false,
                "error_code": err.code(),
                "message": redact_sensitive_text(&err.to_string(), home_for_redact.as_deref()),
                "data": Value::Null,
            }),
        ),
    };

    if let Ok(pretty) = serde_json::to_string_pretty(&out_json) {
        println!("{pretty}");
    } else {
        println!(
            "{{\"ok\":false,\"error_code\":\"JSON_ERROR\",\"message\":\"serialization failed\"}}"
        );
    }

    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn parse_cli<I: Iterator<Item = String>>(iter: I) -> CliArgs {
    let raw: Vec<String> = iter.collect();
    let mut parsed = CliArgs {
        data_dir: None,
        simulated_home: None,
        short_tmp_root: None,
        fixture_host: false,
        node_fixture: false,
        app_path: None,
        with_size: false,
        confirm: false,
        spawn_helper: false,
        orphan_helper: false,
        enforce_sun_len: false,
        crash: false,
        crash_delay_ms: None,
        delayed_exit_ms: None,
        hang_on_quit: false,
        stay_ms: None,
        slow_startup_pre_lock_ms: None,
        user_id: None,
        account_label: None,
        hold_ms: None,
        timeout_ms: 3_500,
        dest: None,
        name: None,
        notes: String::new(),
        symlink_user_dirs: false,
        symlink_keychain: false,
        pinned_bundle_id: None,
        pinned_team_id: None,
        positional: Vec::new(),
    };

    let mut passthrough = false;
    let mut i = 0usize;
    while i < raw.len() {
        let arg = &raw[i];
        if passthrough {
            parsed.positional.push(arg.clone());
            i += 1;
            continue;
        }
        if arg == "--" {
            passthrough = true;
            i += 1;
            continue;
        }
        match arg.as_str() {
            "--json" | "--cli" => {}
            "--fixture-host" => parsed.fixture_host = true,
            "--node-fixture" => parsed.node_fixture = true,
            "--with-size" => parsed.with_size = true,
            "--confirm" => parsed.confirm = true,
            "--spawn-helper" => parsed.spawn_helper = true,
            "--orphan-helper" => parsed.orphan_helper = true,
            "--enforce-sun-len" => parsed.enforce_sun_len = true,
            "--crash" => parsed.crash = true,
            "--hang-on-quit" => parsed.hang_on_quit = true,
            "--symlink-user-dirs" => parsed.symlink_user_dirs = true,
            "--symlink-keychain" => parsed.symlink_keychain = true,
            "--no-user-symlinks" => parsed.symlink_user_dirs = false,
            "--no-keychain-symlink" => parsed.symlink_keychain = false,
            "--data-dir" if i + 1 < raw.len() => {
                i += 1;
                parsed.data_dir = Some(PathBuf::from(&raw[i]));
            }
            "--simulated-home" if i + 1 < raw.len() => {
                i += 1;
                parsed.simulated_home = Some(PathBuf::from(&raw[i]));
            }
            "--short-tmp-root" if i + 1 < raw.len() => {
                i += 1;
                parsed.short_tmp_root = Some(PathBuf::from(&raw[i]));
            }
            "--app-path" if i + 1 < raw.len() => {
                i += 1;
                parsed.app_path = Some(PathBuf::from(&raw[i]));
            }
            "--crash-delay-ms" if i + 1 < raw.len() => {
                i += 1;
                parsed.crash_delay_ms = raw[i].parse::<u64>().ok();
            }
            "--delayed-exit-ms" if i + 1 < raw.len() => {
                i += 1;
                parsed.delayed_exit_ms = raw[i].parse::<u64>().ok();
            }
            "--stay-ms" if i + 1 < raw.len() => {
                i += 1;
                parsed.stay_ms = raw[i].parse::<u64>().ok();
            }
            "--slow-startup-pre-lock-ms" if i + 1 < raw.len() => {
                i += 1;
                parsed.slow_startup_pre_lock_ms = raw[i].parse::<u64>().ok();
            }
            "--user-id" if i + 1 < raw.len() => {
                i += 1;
                parsed.user_id = Some(raw[i].clone());
            }
            "--account-label" if i + 1 < raw.len() => {
                i += 1;
                parsed.account_label = Some(raw[i].clone());
            }
            "--hold-ms" if i + 1 < raw.len() => {
                i += 1;
                parsed.hold_ms = raw[i].parse::<u64>().ok();
            }
            "--timeout-ms" if i + 1 < raw.len() => {
                i += 1;
                if let Ok(ms) = raw[i].parse::<u64>() {
                    parsed.timeout_ms = ms;
                }
            }
            "--dest" if i + 1 < raw.len() => {
                i += 1;
                parsed.dest = Some(PathBuf::from(&raw[i]));
            }
            "--name" if i + 1 < raw.len() => {
                i += 1;
                parsed.name = Some(raw[i].clone());
            }
            "--notes" if i + 1 < raw.len() => {
                i += 1;
                parsed.notes = raw[i].clone();
            }
            "--pinned-bundle-id" if i + 1 < raw.len() => {
                i += 1;
                parsed.pinned_bundle_id = Some(raw[i].clone());
            }
            "--pinned-team-id" if i + 1 < raw.len() => {
                i += 1;
                parsed.pinned_team_id = Some(raw[i].clone());
            }
            _ => parsed.positional.push(arg.clone()),
        }
        i += 1;
    }
    parsed
}

fn run_cli(args: CliArgs) -> Result<Value, DimError> {
    let data_dir = match &args.data_dir {
        Some(d) => d.clone(),
        None => dim_core::resolve_data_dir(None)?,
    };

    let layout_opts = MacOsLayoutOptions {
        symlink_user_dirs: args.symlink_user_dirs,
        symlink_keychain: args.symlink_keychain,
    };

    let mut fixture_ops_handle: Option<Arc<HostFixtureMacOsOps>> = None;
    let mut adapter = if args.fixture_host {
        let sim_home = args
            .simulated_home
            .clone()
            .unwrap_or_else(|| data_dir.join("_sim_home"));
        let short_tmp = args
            .short_tmp_root
            .clone()
            .unwrap_or_else(|| env::temp_dir().join("dim-macos-stmp"));
        dim_platform_macos::ensure_not_macos_primary_path(&data_dir, Some(&sim_home))?;
        let ops = Arc::new(HostFixtureMacOsOps::new(sim_home, short_tmp));
        fixture_ops_handle = Some(Arc::clone(&ops));
        MacOsPlatformAdapter::with_system_ops(ops)
            .with_signature_enforcement(false)
            .with_layout_options(layout_opts)
    } else {
        MacOsPlatformAdapter::new().with_layout_options(layout_opts)
    };

    if let (Some(bid), Some(tid)) = (&args.pinned_bundle_id, &args.pinned_team_id) {
        adapter = adapter.with_pinned_publisher(bid.clone(), tid.clone());
    }

    let store = Arc::new(JsonProfileStore::new(data_dir)?);
    store.ensure_initialized()?;

    let adapter_probe = adapter.clone();
    let backups = FileBackupManager::new(Arc::clone(&store))
        .with_disk_space_probe(Arc::new(move |p| adapter_probe.free_disk_bytes(p)));

    let cmd = args
        .positional
        .first()
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "status".to_owned());

    match cmd.as_str() {
        "capabilities" | "diagnostics" => {
            let caps = adapter.capabilities();
            let free = adapter.free_disk_bytes(store.data_dir())?;
            Ok(json!({
                "ok": true,
                "platform": adapter.platform().as_str(),
                "is_elevated": adapter.is_elevated(),
                "profiles_acl_safe": adapter.check_profiles_acl_safe(store.profiles_dir()),
                "free_disk_bytes": free,
                "free_disk_ok": free >= MIN_FREE_DISK_BYTES,
                "capabilities": caps,
            }))
        }
        "discover" => {
            let install = adapter.discover_installation(args.app_path.as_deref())?;
            Ok(json!({
                "ok": true,
                "installation": install,
            }))
        }
        "status" => {
            let primary = adapter.inspect_primary()?;
            let profiles = store.list_profiles()?;
            let mut items = Vec::with_capacity(profiles.len());
            for rec in profiles {
                let pdir = store.profile_dir_for(&rec)?;
                let rt = adapter.inspect_profile(&rec, &pdir)?;
                items.push(json!({
                    "record": rec,
                    "runtime": rt,
                }));
            }
            Ok(json!({
                "ok": true,
                "platform": adapter.platform().as_str(),
                "data_dir": store.data_dir(),
                "profiles_acl_safe": adapter.check_profiles_acl_safe(store.profiles_dir()),
                "primary": primary,
                "profiles": items,
            }))
        }
        "create" => {
            let name = args
                .positional
                .get(1)
                .ok_or_else(|| DimError::InvalidProfileName("missing profile name".to_owned()))?;
            let rec = store.create_profile(name, &args.notes)?;
            let pdir = store.profile_dir_for(&rec)?;
            adapter.ensure_profile_layout(&rec, &pdir)?;
            Ok(json!({
                "ok": true,
                "data": rec,
                "profile_dir": pdir,
            }))
        }
        "rename" => {
            let id = args
                .positional
                .get(1)
                .ok_or_else(|| DimError::ProfileNotFound("missing profile id".to_owned()))?;
            let new_name = args
                .positional
                .get(2)
                .ok_or_else(|| DimError::InvalidProfileName("missing new name".to_owned()))?;
            let rec = store.rename_profile(id, new_name)?;
            Ok(json!({ "ok": true, "data": rec }))
        }
        "delete" => {
            let id = args
                .positional
                .get(1)
                .ok_or_else(|| DimError::ProfileNotFound("missing profile id".to_owned()))?;
            let rec = store.get_profile(id)?;
            let pdir = store.profile_dir_for(&rec)?;
            let rt = adapter.inspect_profile(&rec, &pdir)?;
            let unlinked = std::cell::Cell::new(0usize);
            store.delete_profile_with_guard(id, args.confirm, rt.running, &|dir| {
                let count = adapter.unlink_profile_junctions(dir)?;
                unlinked.set(count);
                Ok(count)
            })?;
            Ok(json!({
                "ok": true,
                "data": {
                    "deleted_profile_id": rec.id,
                    "unlinked_symlinks": unlinked.get(),
                }
            }))
        }
        "launch" => {
            let id = args
                .positional
                .get(1)
                .ok_or_else(|| DimError::ProfileNotFound("missing profile id".to_owned()))?;
            let rec = store.get_profile(id)?;
            let pdir = store.profile_dir_for(&rec)?;
            adapter.ensure_profile_layout(&rec, &pdir)?;

            if let Some(hold_ms) = args.hold_ms {
                let lock_set = Arc::new(std::sync::Mutex::new(std::collections::HashSet::new()));
                let _g = dim_platform_macos::ProfileOperationGuard::acquire(
                    &**adapter.system_ops(),
                    &lock_set,
                    &rec.id,
                    Some(&pdir),
                )?;
                std::thread::sleep(Duration::from_millis(hold_ms));
            }

            let before = adapter.inspect_profile(&rec, &pdir)?;
            verify_profile_id_marker(&before.zalo_data_dir, &rec.id, false)?;

            let free = adapter.free_disk_bytes(&pdir)?;
            ensure_min_free_disk_space(&pdir, free, 0)?;

            if !before.running {
                enforce_unique_account(&store, &adapter, &rec, &before.zalo_data_dir)?;
            }

            let target = build_target(&args)?;
            let crash_recovered = !before.running && before.stale_lock_detected;
            let rt = adapter.launch(&target, &rec, &pdir)?;
            let uid = detect_zalo_user_id(&rt.zalo_data_dir);
            let _ = store.update_last_launched(&rec.id, &now_rfc3339_utc(), uid.as_deref())?;
            Ok(json!({
                "ok": true,
                "data": {
                    "runtime": rt,
                    "crash_recovered": crash_recovered,
                }
            }))
        }
        "focus" => {
            let id = args
                .positional
                .get(1)
                .ok_or_else(|| DimError::ProfileNotFound("missing profile id".to_owned()))?;
            let rec = store.get_profile(id)?;
            let pdir = store.profile_dir_for(&rec)?;
            let state = adapter.inspect_profile(&rec, &pdir)?;
            let target = build_target(&args)?;
            adapter.focus_or_restore(&target, &rec, &pdir, &state)?;
            let refreshed = adapter.inspect_profile(&rec, &pdir)?;
            Ok(json!({ "ok": true, "data": refreshed }))
        }
        "stop" => {
            let id = args
                .positional
                .get(1)
                .ok_or_else(|| DimError::ProfileNotFound("missing profile id".to_owned()))?;
            let rec = store.get_profile(id)?;
            let pdir = store.profile_dir_for(&rec)?;
            let primary = adapter.inspect_primary()?;
            let state = adapter.inspect_profile(&rec, &pdir)?;
            let stopped = adapter.stop(&state, &primary.protected_pids, args.timeout_ms)?;
            Ok(json!({
                "ok": true,
                "data": {
                    "profile_id": rec.id,
                    "terminated_process_count": stopped,
                }
            }))
        }
        "recover" => {
            let id = args
                .positional
                .get(1)
                .ok_or_else(|| DimError::ProfileNotFound("missing profile id".to_owned()))?;
            let rec = store.get_profile(id)?;
            let pdir = store.profile_dir_for(&rec)?;
            let state = adapter.inspect_profile(&rec, &pdir)?;
            let report = adapter.recover_stale_state(&rec, &pdir, &state)?;
            Ok(json!({ "ok": true, "data": report }))
        }
        "backup" => {
            let id = args
                .positional
                .get(1)
                .ok_or_else(|| DimError::ProfileNotFound("missing profile id".to_owned()))?;
            let rec = store.get_profile(id)?;
            let pdir = store.profile_dir_for(&rec)?;
            let rt = adapter.inspect_profile(&rec, &pdir)?;
            let bk = backups.backup_profile(&rec, &pdir, args.dest.as_deref(), rt.running)?;
            Ok(json!({ "ok": true, "data": bk }))
        }
        "restore" => {
            let raw_dir = args
                .positional
                .get(1)
                .ok_or_else(|| DimError::BackupError("missing backup directory path".to_owned()))?;
            let cand = PathBuf::from(raw_dir);
            validate_directory_input(&cand)?;
            let resolved = if cand.is_absolute() {
                cand
            } else {
                resolve_safe_child(store.backups_dir(), raw_dir)?
            };
            let restored =
                backups.restore_backup_as_new_profile(&resolved, args.name.as_deref())?;
            Ok(json!({ "ok": true, "data": restored }))
        }
        "verify-isolation" => {
            let mut pairs = Vec::new();
            for id in args.positional.iter().skip(1) {
                let rec = store.get_profile(id)?;
                let pdir = store.profile_dir_for(&rec)?;
                pairs.push((rec.id, pdir));
            }
            let real_home = adapter.system_ops().user_home_dir();
            let report = verify_12_surface_runtime_isolation(
                &**adapter.system_ops(),
                &pairs,
                real_home.as_deref(),
            )?;
            Ok(json!({
                "ok": true,
                "data": report,
            }))
        }
        "verify-foreign-or-primary-guard" => {
            let Some(ref ops) = fixture_ops_handle else {
                return Err(DimError::UnsupportedPlatform(
                    "--fixture-host is required for verify-foreign-or-primary-guard".to_owned(),
                ));
            };
            let id = args
                .positional
                .get(1)
                .ok_or_else(|| DimError::ProfileNotFound("missing profile id".to_owned()))?;
            let rec = store.get_profile(id)?;
            let pdir = store.profile_dir_for(&rec)?;
            adapter.ensure_profile_layout(&rec, &pdir)?;
            let zalo_data = dim_platform_macos::macos_zalo_data_dir(&pdir);

            // 1. Foreign recycled PID (PID 88_421 belongs to `/Applications/Safari.app`).
            ops.insert_synthetic_process(ProcessEntry {
                pid: 88_421,
                ppid: 1,
                uid: ops.current_uid(),
                rss_kib: 50_000,
                start_token: Some("Mon Nov 17 08:12:00 2026".to_owned()),
                executable: "/Applications/Safari.app/Contents/MacOS/Safari".to_owned(),
                args: "/Applications/Safari.app/Contents/MacOS/Safari".to_owned(),
            });
            fs::write(
                zalo_data.join("lockfile"),
                serde_json::to_string(&json!({ "pid": 88_421 }))?,
            )?;
            let foreign_state = adapter.inspect_profile(&rec, &pdir)?;
            let foreign_not_owned = !foreign_state.running && foreign_state.stale_lock_detected;
            let foreign_stop_count = adapter.stop(&foreign_state, &[], 500)?;
            let safari_still_alive = ops.is_pid_alive(88_421);

            // 2. Same-UID, same-binary `Zalo.app` PID reuse with mismatched `lstart` (`P0-3`).
            ops.insert_synthetic_process(ProcessEntry {
                pid: 91_200,
                ppid: 1,
                uid: ops.current_uid(),
                rss_kib: 95_000,
                start_token: Some("Mon Nov 17 11:45:00 2026".to_owned()),
                executable: "/Applications/Zalo.app/Contents/MacOS/Zalo".to_owned(),
                args: "/Applications/Zalo.app/Contents/MacOS/Zalo".to_owned(),
            });
            write_runtime_launch_record(
                &pdir,
                &RuntimeLaunchRecord {
                    schema_version: 1,
                    pid: 91_200,
                    process_start_token: Some("Mon Nov 17 09:00:00 2026".to_owned()),
                    profile_id: rec.id.clone(),
                    profile_dir: pdir.clone(),
                    zalo_data_dir: zalo_data.clone(),
                    executable_path: "/Applications/Zalo.app".to_owned(),
                    launched_at_utc: "2026-11-17T09:00:00Z".to_owned(),
                },
            )?;
            fs::write(
                zalo_data.join("lockfile"),
                serde_json::to_string(&json!({ "pid": 91_200 }))?,
            )?;
            let recycled_zalo_state = adapter.inspect_profile(&rec, &pdir)?;
            let same_binary_pid_reuse_rejected =
                !recycled_zalo_state.running && recycled_zalo_state.stale_lock_detected;

            let mut forged_focus_state = recycled_zalo_state.clone();
            forged_focus_state.running = true;
            forged_focus_state.main_pid = Some(91_200);
            let unowned_focus_err = adapter
                .focus_or_restore(
                    &LaunchTarget::OfficialZalo {
                        custom_install_root: None,
                    },
                    &rec,
                    &pdir,
                    &forged_focus_state,
                )
                .err()
                .map(|e| e.code().to_owned());

            // 3. Primary Zalo PID protection (PID 77_100 belongs to primary `/Applications/Zalo.app`).
            ops.insert_synthetic_process(ProcessEntry {
                pid: 77_100,
                ppid: 1,
                uid: ops.current_uid(),
                rss_kib: 120_000,
                start_token: Some("Mon Nov 17 07:30:00 2026".to_owned()),
                executable: "/Applications/Zalo.app/Contents/MacOS/Zalo".to_owned(),
                args: "/Applications/Zalo.app/Contents/MacOS/Zalo".to_owned(),
            });
            let primary = adapter.inspect_primary()?;
            let primary_detected = primary.running
                && primary.protected_pids.contains(&77_100)
                && primary.protected_pids.contains(&91_200);

            let mut forged_state = foreign_state;
            forged_state.running = true;
            forged_state.main_pid = Some(77_100);
            forged_state.process_ids = vec![77_100];
            let stop_primary_err = adapter
                .stop(&forged_state, &primary.protected_pids, 500)
                .err()
                .map(|e| e.code().to_owned());
            let _ = dim_platform_macos::remove_file_no_follow(&zalo_data.join("lockfile"));
            let _ = dim_platform_macos::clear_runtime_launch_files(&pdir);

            Ok(json!({
                "ok": true,
                "data": {
                    "foreign_not_owned": foreign_not_owned,
                    "foreign_stop_count": foreign_stop_count,
                    "safari_still_alive": safari_still_alive,
                    "same_binary_pid_reuse_rejected": same_binary_pid_reuse_rejected,
                    "unowned_focus_error_code": unowned_focus_err,
                    "primary_detected": primary_detected,
                    "stop_primary_error_code": stop_primary_err,
                }
            }))
        }
        other => Err(DimError::SchemaError(format!("unknown command '{other}'"))),
    }
}

fn build_target(args: &CliArgs) -> Result<LaunchTarget, DimError> {
    let mut extra = Vec::new();
    if args.spawn_helper {
        extra.push("--spawn-helper".to_owned());
    }
    if args.orphan_helper {
        extra.push("--orphan-helper".to_owned());
    }
    if args.enforce_sun_len {
        extra.push("--enforce-sun-len".to_owned());
    }
    if args.crash {
        extra.push("--crash".to_owned());
        if let Some(delay) = args.crash_delay_ms {
            extra.push(format!("--crash-delay-ms={delay}"));
        }
    }
    if let Some(delay_exit) = args.delayed_exit_ms {
        extra.push(format!("--delayed-exit-ms={delay_exit}"));
    }
    if args.hang_on_quit {
        extra.push("--hang-on-quit".to_owned());
    }
    if let Some(stay) = args.stay_ms {
        extra.push(format!("--stay-ms={stay}"));
    }
    if let Some(slow_ms) = args.slow_startup_pre_lock_ms {
        extra.push(format!("--slow-startup-pre-lock-ms={slow_ms}"));
    }
    if let Some(ref uid) = args.user_id {
        extra.push(format!("--user-id={uid}"));
    }
    if let Some(ref label) = args.account_label {
        extra.push(format!("--account-label={label}"));
    }

    if args.node_fixture
        || args
            .app_path
            .as_ref()
            .is_some_and(|p| p.extension().is_some_and(|e| e == "mjs" || e == "js"))
    {
        let script = args.app_path.clone().unwrap_or_else(default_macos_fixture);
        return Ok(LaunchTarget::NodeFixture {
            node_path: None,
            script_path: script,
            extra_args: extra,
        });
    }

    if let Some(ref p) = args.app_path {
        if p.is_file() {
            return Ok(LaunchTarget::CustomExecutable {
                path: p.clone(),
                args: extra,
            });
        }
        return Ok(LaunchTarget::OfficialZalo {
            custom_install_root: Some(p.clone()),
        });
    }

    Ok(LaunchTarget::OfficialZalo {
        custom_install_root: None,
    })
}

fn default_macos_fixture() -> PathBuf {
    let raw = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("platform-macos")
        .join("fixture-macos-app.mjs");
    match fs::canonicalize(&raw) {
        Ok(canon) => PathBuf::from(
            canon
                .to_string_lossy()
                .trim_start_matches(r"\\?\")
                .to_owned(),
        ),
        Err(_) => raw,
    }
}

fn enforce_unique_account(
    store: &JsonProfileStore,
    adapter: &MacOsPlatformAdapter,
    target: &ProfileRecord,
    target_zalo_dir: &Path,
) -> Result<(), DimError> {
    let target_uid = detect_zalo_user_id(target_zalo_dir);
    let mut running = Vec::new();
    for other in store.list_profiles()? {
        if other.id.eq_ignore_ascii_case(&target.id) {
            continue;
        }
        let Ok(other_dir) = store.profile_dir_for(&other) else {
            continue;
        };
        if let Ok(rt) = adapter.inspect_profile(&other, &other_dir) {
            if rt.running {
                let uid = detect_zalo_user_id(&rt.zalo_data_dir).or(rt.detected_zalo_user_id);
                running.push((other, uid));
            }
        }
    }
    check_duplicate_zalo_user_id(target, target_uid.as_deref(), &running)
}

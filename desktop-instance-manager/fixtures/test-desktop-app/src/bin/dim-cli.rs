//! Native Rust CLI parity runner (`dim-cli`) exposing the same `--cli --json` contract as `scripts/dim-host.ps1`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use dim_core::{
    now_rfc3339_utc, redact_sensitive_text, resolve_safe_child, validate_directory_input,
    ApplicationLocator, BackupManager, DimError, InstanceMonitor, LaunchTarget,
    PlatformCapabilities, PlatformKind, ProfileStore, SupportStatus, MIN_FREE_DISK_BYTES,
};
use dim_process_manager::InstanceCoordinator;
use dim_profile_store::{FileBackupManager, JsonProfileStore};
use serde_json::{json, Value};

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
use dim_platform_linux::LinuxPlatformAdapter as ActiveAdapter;
#[cfg(target_os = "macos")]
use dim_platform_macos::MacOsPlatformAdapter as ActiveAdapter;
#[cfg(target_os = "windows")]
use dim_platform_windows::WindowsPlatformAdapter as ActiveAdapter;

struct CliFlags {
    data_dir: Option<PathBuf>,
    app_path: Option<String>,
    node_fixture: bool,
    with_size: bool,
    confirm: bool,
    allow_external_db_omit: bool,
    crash: bool,
    crash_delay_ms: Option<u64>,
    dest: Option<PathBuf>,
    name: Option<String>,
    notes: String,
    user_id: Option<String>,
    positional: Vec<String>,
}

fn main() -> ExitCode {
    let flags = parse_args(env::args().skip(1));
    let user_home = env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from);

    let (ok, payload) = match execute(flags) {
        Ok(val) => {
            let is_ok = val.get("ok").and_then(Value::as_bool).unwrap_or(true);
            (is_ok, val)
        }
        Err(err) => (
            false,
            json!({
                "ok": false,
                "error_code": err.code(),
                "message": redact_sensitive_text(&err.to_string(), user_home.as_deref()),
                "data": Value::Null,
            }),
        ),
    };

    match serde_json::to_string_pretty(&payload) {
        Ok(text) => println!("{text}"),
        Err(err) => {
            eprintln!("{{\"ok\":false,\"error_code\":\"JSON_ERROR\",\"message\":\"{err}\"}}")
        }
    }

    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn parse_args<I: Iterator<Item = String>>(iter: I) -> CliFlags {
    let args: Vec<String> = iter.collect();
    let mut flags = CliFlags {
        data_dir: None,
        app_path: None,
        node_fixture: false,
        with_size: false,
        confirm: false,
        allow_external_db_omit: false,
        crash: false,
        crash_delay_ms: None,
        dest: None,
        name: None,
        notes: String::new(),
        user_id: None,
        positional: Vec::new(),
    };

    let mut after_double_dash = false;
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if after_double_dash {
            flags.positional.push(a.clone());
            i += 1;
            continue;
        }
        if a == "--" {
            after_double_dash = true;
            i += 1;
            continue;
        }
        match a.as_str() {
            "--cli" | "--json" => {}
            "--data-dir" | "--storage-root" if i + 1 < args.len() => {
                i += 1;
                flags.data_dir = Some(PathBuf::from(&args[i]));
            }
            "--app-path" if i + 1 < args.len() => {
                i += 1;
                flags.app_path = Some(args[i].clone());
            }
            "--node-fixture" => flags.node_fixture = true,
            "--with-size" => flags.with_size = true,
            "--confirm" => flags.confirm = true,
            "--allow-external-db-omit" | "--allow-incomplete-external-db" => {
                flags.allow_external_db_omit = true;
            }
            "--crash" => flags.crash = true,
            "--crash-delay-ms" if i + 1 < args.len() => {
                i += 1;
                flags.crash_delay_ms = args[i].parse::<u64>().ok();
            }
            "--dest" if i + 1 < args.len() => {
                i += 1;
                flags.dest = Some(PathBuf::from(&args[i]));
            }
            "--name" if i + 1 < args.len() => {
                i += 1;
                flags.name = Some(args[i].clone());
            }
            "--notes" if i + 1 < args.len() => {
                i += 1;
                flags.notes = args[i].clone();
            }
            "--user-id" if i + 1 < args.len() => {
                i += 1;
                flags.user_id = Some(args[i].clone());
            }
            _ => flags.positional.push(a.clone()),
        }
        i += 1;
    }

    flags
}

fn execute(flags: CliFlags) -> Result<Value, DimError> {
    let store = Arc::new(JsonProfileStore::from_default_or_env(
        flags.data_dir.as_deref(),
    )?);
    store.ensure_initialized()?;

    let adapter = Arc::new(ActiveAdapter::new());
    let coordinator = InstanceCoordinator::new(Arc::clone(&store), Arc::clone(&adapter));
    let adapter_for_probe = Arc::clone(&adapter);
    let backups = FileBackupManager::new(Arc::clone(&store))
        .with_disk_space_probe(Arc::new(move |p| adapter_for_probe.free_disk_bytes(p)));

    let cmd = flags
        .positional
        .first()
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "status".to_owned());

    match cmd.as_str() {
        "status" => cmd_status(&store, &adapter, &flags),
        "discover" => cmd_discover(&adapter, &flags),
        "create" => {
            let name = flags
                .positional
                .get(1)
                .ok_or_else(|| DimError::InvalidProfileName("missing profile name".to_owned()))?;
            let notes = flags
                .positional
                .get(2)
                .map(String::as_str)
                .unwrap_or(flags.notes.as_str());
            let rec = store.create_profile(name, notes)?;
            let dir = store.profile_dir_for(&rec)?;
            dim_core::InstanceLauncher::ensure_profile_layout(&*adapter, &rec, &dir)?;
            Ok(envelope_ok(
                format!("Created profile '{}'.", rec.name),
                serde_json::to_value(rec)?,
            ))
        }
        "rename" => {
            let id = flags
                .positional
                .get(1)
                .ok_or_else(|| DimError::ProfileNotFound("missing profile id".to_owned()))?;
            let new_name = flags.positional.get(2).ok_or_else(|| {
                DimError::InvalidProfileName("missing new profile name".to_owned())
            })?;
            let rec = store.rename_profile(id, new_name)?;
            Ok(envelope_ok(
                format!("Renamed profile to '{}'.", rec.name),
                serde_json::to_value(rec)?,
            ))
        }
        "delete" => {
            let id = flags
                .positional
                .get(1)
                .ok_or_else(|| DimError::ProfileNotFound("missing profile id".to_owned()))?;
            coordinator.delete_profile(id, flags.confirm)?;
            Ok(envelope_ok(
                format!("Deleted profile '{id}'."),
                json!({ "deleted_profile_id": id }),
            ))
        }
        "launch" => {
            let id = flags
                .positional
                .get(1)
                .ok_or_else(|| DimError::ProfileNotFound("missing profile id".to_owned()))?;
            let target = resolve_launch_target(&flags)?;
            let crash_recovered = store
                .get_profile(id)
                .ok()
                .and_then(|rec| {
                    store
                        .profile_dir_for(&rec)
                        .ok()
                        .and_then(|dir| adapter.inspect_profile(&rec, &dir).ok())
                })
                .is_some_and(|rt_before| !rt_before.running && rt_before.stale_lock_detected);
            let rt = coordinator.launch_profile(id, &target)?;
            Ok(envelope_ok(
                format!("Launched profile '{}'.", rt.profile_name),
                json!({
                    "action": "launched",
                    "profile_id": rt.profile_id,
                    "pid": rt.main_pid,
                    "running": rt.running,
                    "lifecycle_state": rt.lifecycle_state().as_str(),
                    "crash_recovered": crash_recovered,
                    "zalo_data_dir": rt.zalo_data_dir,
                    "detected_zalo_user_id": rt.detected_zalo_user_id,
                }),
            ))
        }
        "focus" => {
            let id = flags
                .positional
                .get(1)
                .ok_or_else(|| DimError::ProfileNotFound("missing profile id".to_owned()))?;
            let target = resolve_launch_target(&flags)?;
            let rt = coordinator.focus_profile(id, &target)?;
            Ok(envelope_ok(
                format!("Focused profile '{}'.", rt.profile_name),
                json!({
                    "action": "focused",
                    "profile_id": rt.profile_id,
                    "pid": rt.main_pid,
                }),
            ))
        }
        "stop" => {
            let id = flags
                .positional
                .get(1)
                .ok_or_else(|| DimError::ProfileNotFound("missing profile id".to_owned()))?;
            let count = coordinator.stop_profile(id)?;
            Ok(envelope_ok(
                "Stopped profile.",
                json!({
                    "profile_id": id,
                    "terminated_process_count": count,
                }),
            ))
        }
        "stop-all" => {
            let stopped = coordinator.stop_all_secondary_profiles()?;
            let items: Vec<Value> = stopped
                .iter()
                .map(|(pid, count)| json!({ "profile_id": pid, "terminated_process_count": count }))
                .collect();
            Ok(envelope_ok(
                format!("Stopped {} secondary profile(s).", items.len()),
                json!({
                    "stopped_profiles_count": items.len(),
                    "stopped_profiles": items,
                }),
            ))
        }
        "backup" => {
            let id = flags
                .positional
                .get(1)
                .ok_or_else(|| DimError::ProfileNotFound("missing profile id".to_owned()))?;
            let profile = store.get_profile(id)?;
            let dir = store.profile_dir_for(&profile)?;
            let rt = adapter.inspect_profile(&profile, &dir)?;
            let external_omitted =
                !dim_core::external_paths_for_profile_all_layouts(&dir).is_empty();
            let bk = backups.backup_profile_with_options(
                &profile,
                &dir,
                flags.dest.as_deref(),
                rt.running,
                flags.allow_external_db_omit,
            )?;
            let backup_name = bk
                .backup_dir
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            Ok(envelope_ok(
                "Created offline atomic backup.",
                json!({
                    "backup_id": bk.manifest.backup_id,
                    "backup_dir": bk.backup_dir,
                    "backup_name": backup_name,
                    "manifest_path": bk.manifest_path,
                    "file_count": bk.manifest.file_count,
                    "total_bytes": bk.manifest.total_bytes,
                    "external_db_omitted": external_omitted,
                }),
            ))
        }
        "restore" => {
            let raw_dir = flags.positional.get(1).ok_or_else(|| {
                DimError::BackupError("missing backup directory argument".to_owned())
            })?;
            let candidate = PathBuf::from(raw_dir.trim());
            validate_directory_input(&candidate)?;
            let resolved_dir = if candidate.is_absolute() {
                candidate
            } else {
                resolve_safe_child(store.backups_dir(), raw_dir.trim())?
            };
            let trimmed_name = flags
                .name
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let rec = backups.restore_backup_as_new_profile(&resolved_dir, trimmed_name)?;
            Ok(envelope_ok(
                format!("Restored backup into new profile '{}'.", rec.name),
                serde_json::to_value(rec)?,
            ))
        }
        "backups" => {
            let mut items = Vec::new();
            if let Ok(entries) = fs::read_dir(store.backups_dir()) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if name.ends_with(".partial") || !path.is_dir() {
                        continue;
                    }
                    if let Ok(manifest) = backups.verify_backup(&path) {
                        items.push(json!({
                            "backup_name": name,
                            "backup_dir": path,
                            "backup_id": manifest.backup_id,
                            "created_at": manifest.created_at,
                            "file_count": manifest.file_count,
                            "total_bytes": manifest.total_bytes,
                            "source_profile_id": manifest.source_profile_id,
                            "source_profile_name": manifest.source_profile_name,
                        }));
                    }
                }
            }
            Ok(json!({
                "ok": true,
                "backups": items,
            }))
        }
        "diagnostics" => cmd_diagnostics(&store, &adapter),
        "verify-isolation" => {
            let fixture_target = if flags.node_fixture || flags.app_path.is_some() {
                Some(resolve_launch_target(&flags)?)
            } else {
                None
            };
            let custom_root = match &fixture_target {
                Some(LaunchTarget::OfficialZalo {
                    custom_install_root,
                }) => custom_install_root.as_deref(),
                _ => None,
            };
            let report =
                coordinator.verify_isolation(&*adapter, custom_root, fixture_target.as_ref())?;
            Ok(serde_json::to_value(report)?)
        }
        other => Ok(json!({
            "ok": false,
            "error_code": "UNKNOWN_COMMAND",
            "message": format!("Unknown command: {other}"),
            "data": Value::Null,
        })),
    }
}

fn envelope_ok(message: impl Into<String>, data: Value) -> Value {
    json!({
        "ok": true,
        "error_code": Value::Null,
        "message": message.into(),
        "data": data,
    })
}

fn cmd_status(
    store: &JsonProfileStore,
    adapter: &ActiveAdapter,
    flags: &CliFlags,
) -> Result<Value, DimError> {
    let caps = adapter.capabilities();
    let installation = cmd_discover(adapter, flags)?;
    let primary = adapter.inspect_primary()?;
    let records = store.list_profiles()?;

    let mut profiles = Vec::with_capacity(records.len());
    for rec in records {
        let dir = store.profile_dir_for(&rec)?;
        let rt = adapter.inspect_profile(&rec, &dir)?;
        let lifecycle_state = rt.lifecycle_state().as_str();
        let mut obj = json!({
            "id": rec.id,
            "name": rec.name,
            "directory_name": rec.directory_name,
            "created_at": rec.created_at,
            "last_launched_at": rec.last_launched_at,
            "zalo_user_id": rt.detected_zalo_user_id.or(rec.zalo_user_id),
            "notes": rec.notes,
            "running": rt.running,
            "lifecycle_state": lifecycle_state,
            "main_pid": rt.main_pid,
            "process_ids": rt.process_ids,
            "process_count": rt.process_count,
            "window_handle": rt.window_handle.unwrap_or(0),
            "window_title": rt.window_title,
            "total_working_set_bytes": rt.working_set_bytes,
            "stale_lockfile_detected": rt.stale_lock_detected,
        });
        if flags.with_size {
            if let Some(map) = obj.as_object_mut() {
                map.insert("data_size_bytes".to_owned(), json!(rt.disk_usage_bytes));
            }
        }
        profiles.push(obj);
    }

    Ok(json!({
        "ok": true,
        "timestamp_utc": now_rfc3339_utc(),
        "platform": adapter.platform().as_str(),
        "data_dir": store.data_dir(),
        "installation": installation,
        "primary_zalo": {
            "is_running": primary.running,
            "zalo_data_path": primary.zalo_data_dir,
            "lockfile_path": primary.lockfile_path,
            "locking_pids": primary.protected_pids,
            "process_count": primary.process_count,
            "total_working_set_bytes": primary.working_set_bytes,
            "zalo_user_id": primary.detected_zalo_user_id,
        },
        "folder_redirection": {
            "redirected": caps.folder_redirection_detected,
            "warning": caps.folder_redirection_path,
        },
        "profiles": profiles,
    }))
}

fn cmd_discover(adapter: &ActiveAdapter, flags: &CliFlags) -> Result<Value, DimError> {
    let app_str = flags.app_path.as_deref();
    let is_js = app_str
        .map(|s| s.ends_with(".mjs") || s.ends_with(".js"))
        .unwrap_or(false);

    if flags.node_fixture || is_js {
        let script = app_str
            .map(PathBuf::from)
            .unwrap_or_else(default_fixture_script_path);
        validate_directory_input(&script)?;
        let valid = script.exists();
        return Ok(json!({
            "ok": true,
            "valid": valid,
            "mode": "node-fixture",
            "script_path": script,
            "authenticode_status": "Signed (node host checked on launch)",
            "error": if valid { Value::Null } else { json!("Fixture script not found") },
        }));
    }

    if let Some(raw_path) = app_str {
        let p = Path::new(raw_path);
        validate_directory_input(p)?;
        if p.is_file() {
            #[cfg(windows)]
            {
                use dim_platform_windows::win32_ffi::{
                    verify_authenticode_signature, AuthenticodeTrustOutcome,
                };
                let (valid, status, signer, err) = match verify_authenticode_signature(p, None) {
                    AuthenticodeTrustOutcome::Trusted {
                        subject,
                        status_label,
                    } => (true, status_label, subject, None),
                    AuthenticodeTrustOutcome::Missing => (
                        false,
                        "Missing".to_owned(),
                        None,
                        Some("Executable not found".to_owned()),
                    ),
                    AuthenticodeTrustOutcome::Unsigned => (
                        false,
                        "Unsigned".to_owned(),
                        None,
                        Some("Executable is unsigned".to_owned()),
                    ),
                    AuthenticodeTrustOutcome::UntrustedOrTampered {
                        subject,
                        status_label,
                        ..
                    } => (false, status_label.clone(), subject, Some(status_label)),
                    AuthenticodeTrustOutcome::WrongPublisher {
                        subject,
                        status_label,
                    } => (
                        false,
                        status_label.clone(),
                        Some(subject),
                        Some(status_label),
                    ),
                    AuthenticodeTrustOutcome::VerificationError { detail } => {
                        (false, format!("Unverified ({detail})"), None, Some(detail))
                    }
                };
                return Ok(json!({
                    "ok": true,
                    "valid": valid,
                    "mode": "custom-exe",
                    "executable": p,
                    "authenticode_status": status,
                    "signer_subject": signer,
                    "error": err,
                }));
            }
            #[cfg(not(windows))]
            {
                return Ok(json!({
                    "ok": true,
                    "valid": p.exists(),
                    "mode": "custom-exe",
                    "executable": p,
                    "authenticode_status": "NotApplicableOnPosix",
                    "error": Value::Null,
                }));
            }
        }
    }

    let install = adapter.discover_installation(app_str.map(Path::new))?;
    Ok(json!({
        "ok": true,
        "valid": install.is_valid,
        "mode": "official-zalo",
        "install_root": install.install_root,
        "version": install.version,
        "executable": install.executable_path,
        "app_asar_path": install.app_asar_path,
        "authenticode_status": install.signature_status,
        "signer_subject": install.signer_subject,
        "error": install.error,
    }))
}

fn cmd_diagnostics(store: &JsonProfileStore, adapter: &ActiveAdapter) -> Result<Value, DimError> {
    let caps = adapter.capabilities();
    let free = adapter.free_disk_bytes(store.data_dir()).unwrap_or(0);
    let user_home = env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from);
    let sanitized_logs: Vec<String> = caps
        .diagnostics
        .iter()
        .map(|s| redact_sensitive_text(s, user_home.as_deref()))
        .collect();

    let redacted_redir = caps
        .folder_redirection_path
        .as_deref()
        .map(|s| redact_sensitive_text(s, user_home.as_deref()));

    let live_limits = caps.limitations.join("; ");
    let win_row = if caps.platform == PlatformKind::Windows {
        json!({
            "platform": "Windows 10/11",
            "status": format_support_status(caps.overall_status),
            "strategy": "Native Rust core + CreateProcessW USERPROFILE/APPDATA/TEMP isolation",
            "limits": live_limits,
        })
    } else {
        json!({
            "platform": "Windows 10/11",
            "status": "PARTIALLY_SUPPORTED",
            "strategy": "Native Rust core + CreateProcessW USERPROFILE/APPDATA/TEMP isolation",
            "limits": "Concurrent calls limited by PipeZCallRecv/PipeZCallSend; prefer in-app notiWin over shared com.vng.zalo Action Center toast.",
        })
    };
    let mac_row = if caps.platform == PlatformKind::MacOs {
        json!({
            "platform": "macOS (Apple Silicon & Intel)",
            "status": format_support_status(caps.overall_status),
            "strategy": "open -n -a /Applications/Zalo.app --env CFFIXED_USER_HOME=<profile_home>",
            "limits": live_limits,
        })
    } else {
        json!({
            "platform": "macOS (Apple Silicon & Intel)",
            "status": "NOT_TESTED",
            "strategy": "open -n -a /Applications/Zalo.app --env CFFIXED_USER_HOME=<profile_home>",
            "limits": "Requires physical Mac verification; /tmp/socketzalorecv2021 call socket conflict.",
        })
    };
    let linux_appimage_row = if caps.platform == PlatformKind::Linux {
        json!({
            "platform": "Ubuntu LTS (Fixture / Local AppImage)",
            "status": format_support_status(caps.appimage_status.unwrap_or(caps.overall_status)),
            "strategy": "XDG_CONFIG_HOME / HOME / optional bwrap",
            "limits": live_limits,
        })
    } else {
        json!({
            "platform": "Ubuntu LTS (Fixture / Local AppImage)",
            "status": "EXPERIMENTAL",
            "strategy": "XDG_CONFIG_HOME / HOME / optional bwrap",
            "limits": "On Ubuntu 24.04+, configure an AppArmor userns profile in /etc/apparmor.d/ for local AppImages (never disable Chromium sandboxing); never bundle proprietary VNG app.asar.",
        })
    };

    Ok(json!({
        "ok": true,
        "timestamp_utc": now_rfc3339_utc(),
        "is_elevated": adapter.is_elevated(),
        "smart_app_control_state": adapter.smart_app_control_state(),
        "folder_redirection": {
            "redirected": caps.folder_redirection_detected,
            "warning": redacted_redir,
        },
        "disk_free_bytes": free,
        "disk_free_ok": free >= MIN_FREE_DISK_BYTES,
        "profiles_acl_safe": adapter.check_profiles_acl_safe(store.profiles_dir()),
        "platform_support_matrix": [
            win_row,
            mac_row,
            {
                "platform": "Ubuntu LTS (Official Zalo)",
                "status": "UNSUPPORTED",
                "strategy": "No official VNG Linux desktop build exists",
                "limits": "Official VNG Linux package does not exist.",
            },
            linux_appimage_row,
        ],
        "sanitized_logs": sanitized_logs,
    }))
}

fn format_support_status(status: SupportStatus) -> &'static str {
    match status {
        SupportStatus::SupportedAndTested => "SUPPORTED_AND_TESTED",
        SupportStatus::PartiallySupported => "PARTIALLY_SUPPORTED",
        SupportStatus::Experimental => "EXPERIMENTAL",
        SupportStatus::Blocked => "BLOCKED",
        SupportStatus::Unsupported => "UNSUPPORTED",
        SupportStatus::NotTested => "NOT_TESTED",
    }
}

fn resolve_launch_target(flags: &CliFlags) -> Result<LaunchTarget, DimError> {
    let app_str = flags.app_path.as_deref();
    let is_js = app_str
        .map(|s| s.ends_with(".mjs") || s.ends_with(".js"))
        .unwrap_or(false);

    if flags.node_fixture || is_js {
        let script = app_str
            .map(PathBuf::from)
            .unwrap_or_else(default_fixture_script_path);
        validate_directory_input(&script)?;
        let mut extra_args = Vec::new();
        if flags.crash {
            extra_args.push("--crash".to_owned());
            if let Some(delay) = flags.crash_delay_ms {
                extra_args.push("--crash-delay-ms".to_owned());
                extra_args.push(delay.to_string());
            }
        }
        if let Some(uid) = &flags.user_id {
            extra_args.push("--user-id".to_owned());
            extra_args.push(uid.clone());
        }
        return Ok(LaunchTarget::NodeFixture {
            node_path: None,
            script_path: script,
            extra_args,
        });
    }

    if let Some(raw) = app_str {
        let p = PathBuf::from(raw);
        validate_directory_input(&p)?;
        if p.is_file() {
            return Ok(LaunchTarget::CustomExecutable {
                path: p,
                args: Vec::new(),
            });
        }
        return Ok(LaunchTarget::OfficialZalo {
            custom_install_root: Some(p),
        });
    }

    Ok(LaunchTarget::OfficialZalo {
        custom_install_root: None,
    })
}

fn default_fixture_script_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixture-app.mjs")
}

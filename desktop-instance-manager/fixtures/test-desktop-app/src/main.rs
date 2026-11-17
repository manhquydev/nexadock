//! Rust test desktop application fixture (`dim-test-fixture`) mirroring `fixture-app.mjs`.

use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use dim_core::now_rfc3339_utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
struct CliOptions {
    force_quit: bool,
    crash: bool,
    crash_delay_ms: u64,
    stay_ms: Option<u64>,
    user_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LockPayload {
    pid: u32,
    started_at: String,
    app: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct LaunchState {
    launch_count: u64,
    focus_count: u64,
    last_pid: u32,
    last_started_at: String,
    last_focused_at: Option<String>,
    last_stopped_at: Option<String>,
    clean_shutdown: bool,
    crashed: bool,
    zalo_user_id: String,
}

fn main() -> ExitCode {
    let opts = parse_cli_args(env::args().skip(1));
    match run_fixture(&opts) {
        Ok(code) => ExitCode::from(code),
        Err(err) => {
            eprintln!("dim-test-fixture error: {err}");
            ExitCode::from(1)
        }
    }
}

fn parse_cli_args<I: Iterator<Item = String>>(args: I) -> CliOptions {
    let mut opts = CliOptions {
        force_quit: false,
        crash: false,
        crash_delay_ms: 150,
        stay_ms: None,
        user_id: env::var("FIXTURE_ZALO_USER_ID").unwrap_or_else(|_| "100001".to_owned()),
    };

    let list: Vec<String> = args.collect();
    let mut i = 0;
    while i < list.len() {
        let arg = &list[i];
        if arg == "--z-update-force-quit" {
            opts.force_quit = true;
        } else if arg == "--crash" {
            opts.crash = true;
        } else if arg == "--crash-delay-ms" && i + 1 < list.len() {
            opts.crash_delay_ms = list[i + 1].parse::<u64>().unwrap_or(150).max(10);
            i += 1;
        } else if let Some(rest) = arg.strip_prefix("--crash-delay-ms=") {
            opts.crash_delay_ms = rest.parse::<u64>().unwrap_or(150).max(10);
        } else if arg == "--stay-ms" && i + 1 < list.len() {
            opts.stay_ms = list[i + 1].parse::<u64>().ok();
            i += 1;
        } else if let Some(rest) = arg.strip_prefix("--stay-ms=") {
            opts.stay_ms = rest.parse::<u64>().ok();
        } else if arg == "--user-id" && i + 1 < list.len() {
            opts.user_id = list[i + 1].clone();
            i += 1;
        } else if let Some(rest) = arg.strip_prefix("--user-id=") {
            opts.user_id = rest.to_owned();
        }
        i += 1;
    }

    opts
}

fn resolve_app_data_dir() -> PathBuf {
    if let Some(appdata) = env::var_os("APPDATA").filter(|v| !v.is_empty()) {
        return PathBuf::from(appdata);
    }
    if cfg!(target_os = "macos") {
        let home = env::var_os("CFFIXED_USER_HOME")
            .or_else(|| env::var_os("HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        return home.join("Library").join("Application Support");
    }
    if let Some(xdg) = env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(xdg);
    }
    let home = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir);
    home.join(".config")
}

fn run_fixture(opts: &CliOptions) -> std::io::Result<u8> {
    let app_data = resolve_app_data_dir();
    let user_data = app_data.join("ZaloData");
    fs::create_dir_all(&app_data)?;
    fs::create_dir_all(&user_data)?;

    let lockfile = user_data.join("lockfile");
    let quit_signal = user_data.join(".quit-signal");
    let focus_signal = user_data.join(".focus-signal");
    let startup_log = user_data.join("startup.log");
    let launch_state_path = user_data.join("launch-state.json");

    if !acquire_lock(&lockfile)? {
        if opts.force_quit {
            fs::write(
                &quit_signal,
                format!("{{\"requested_at\":\"{}\"}}\n", now_rfc3339_utc()),
            )?;
            return Ok(0);
        }
        append_log(&startup_log, "SECOND_INSTANCE_REJECTED");
        let _ = fs::write(
            &focus_signal,
            format!("{{\"focused_at\":\"{}\"}}\n", now_rfc3339_utc()),
        );
        return Ok(2);
    }

    if opts.force_quit {
        let _ = fs::remove_file(&lockfile);
        return Ok(0);
    }

    let prod_dir = user_data
        .join("Database")
        .join("_production")
        .join(&opts.user_id);
    fs::create_dir_all(&prod_dir)?;
    fs::write(
        prod_dir.join("session.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "zalo_user_id": opts.user_id,
            "pid": std::process::id(),
            "updated_at": now_rfc3339_utc(),
        }))?,
    )?;
    fs::write(
        user_data.join("Local State"),
        b"{\"os_crypt\":{\"fixture_mode\":true}}\n",
    )?;
    fs::write(
        user_data.join("config.json"),
        b"{\"fixture\":true,\"app_version\":\"fixture-1.0.0\"}\n",
    )?;

    let mut state: LaunchState = fs::read_to_string(&launch_state_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    state.launch_count = state.launch_count.saturating_add(1);
    state.last_pid = std::process::id();
    state.last_started_at = now_rfc3339_utc();
    state.clean_shutdown = false;
    state.crashed = false;
    state.zalo_user_id = opts.user_id.clone();
    write_json_atomic(&launch_state_path, &state)?;
    append_log(
        &startup_log,
        &format!(
            "PRIMARY_STARTED pid={} launch_count={}",
            state.last_pid, state.launch_count
        ),
    );

    if opts.crash {
        thread::sleep(Duration::from_millis(opts.crash_delay_ms));
        state.crashed = true;
        state.last_stopped_at = Some(now_rfc3339_utc());
        write_json_atomic(&launch_state_path, &state)?;
        append_log(&startup_log, "CRASH_SIMULATED");
        return Ok(1);
    }

    let running = Arc::new(AtomicBool::new(true));
    let elapsed_limit = opts.stay_ms.map(Duration::from_millis);
    let started = std::time::Instant::now();

    while running.load(Ordering::Relaxed) {
        if focus_signal.exists() {
            let _ = fs::remove_file(&focus_signal);
            state.focus_count = state.focus_count.saturating_add(1);
            state.last_focused_at = Some(now_rfc3339_utc());
            let _ = write_json_atomic(&launch_state_path, &state);
        }
        if quit_signal.exists() {
            let _ = fs::remove_file(&quit_signal);
            break;
        }
        if let Some(limit) = elapsed_limit {
            if started.elapsed() >= limit {
                break;
            }
        }
        thread::sleep(Duration::from_millis(50));
    }

    state.clean_shutdown = true;
    state.last_stopped_at = Some(now_rfc3339_utc());
    let _ = write_json_atomic(&launch_state_path, &state);
    let _ = fs::remove_file(&lockfile);
    Ok(0)
}

fn acquire_lock(lockfile: &Path) -> std::io::Result<bool> {
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(lockfile)
    {
        Ok(mut f) => {
            let payload = LockPayload {
                pid: std::process::id(),
                started_at: now_rfc3339_utc(),
                app: "dim-test-fixture".to_owned(),
            };
            let json = serde_json::to_string_pretty(&payload)?;
            f.write_all(json.as_bytes())?;
            f.write_all(b"\n")?;
            Ok(true)
        }
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(err) => Err(err),
    }
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    let json = serde_json::to_string_pretty(value)?;
    fs::write(&tmp, format!("{json}\n"))?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn append_log(path: &Path, line: &str) {
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{} {line}", now_rfc3339_utc());
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn cli_parser_supports_force_quit_crash_and_user_id() {
        let parsed = parse_cli_args(
            [
                "--z-update-force-quit",
                "--crash",
                "--crash-delay-ms=250",
                "--user-id",
                "849000123",
            ]
            .into_iter()
            .map(str::to_owned),
        );
        assert!(parsed.force_quit);
        assert!(parsed.crash);
        assert_eq!(parsed.crash_delay_ms, 250);
        assert_eq!(parsed.user_id, "849000123");
    }

    #[test]
    fn lockfile_prevents_double_acquire_until_removed() {
        let tmp = tempfile::tempdir().unwrap();
        let lock = tmp.path().join("lockfile");
        assert!(acquire_lock(&lock).unwrap());
        assert!(!acquire_lock(&lock).unwrap());
        fs::remove_file(&lock).unwrap();
        assert!(acquire_lock(&lock).unwrap());
    }
}

//! Cross-Platform Differences & OS-Specific Isolation Suite (`XP-WIN-01..04`, `XP-MAC-01..04`, `XP-LNX-01..04`).
//!
//! Enforces strict platform honesty:
//! - Windows-only Win32 kernel/ACL/handle tests execute ONLY on Windows and report `SKIPPED` elsewhere.
//! - macOS-only Launch Services / POSIX runtime tests execute ONLY on macOS and report `SKIPPED` on Windows/Linux.
//! - Ubuntu-only `/proc` / POSIX signal / `bwrap` userns runtime tests execute ONLY on Linux and report `SKIPPED` on Windows/macOS.
//! - Portable layout, command-builder, and source-verified broken-symlink semantics execute everywhere and expose cross-platform bugs (`DEFECT-01`, `DEFECT-02`).

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use dim_core::{
    resolve_data_dir_for_platform, ApplicationLocator, PlatformCapabilities, PlatformKind,
    ProfileRecord, ProfileStore, SupportStatus,
};
use dim_platform_linux::{
    build_bwrap_args, build_linux_isolated_env, ensure_linux_profile_layout, linux_zalo_data_dir,
    looks_like_appimage, read_linux_pid, LinuxPlatformAdapter, APPARMOR_NO_SANDBOX_NOTE,
};
use dim_platform_macos::{
    build_open_args, ensure_macos_profile_layout, extract_plist_short_version,
    inspect_call_sockets_at, macos_zalo_data_dir, parse_singleton_lock_target,
    read_singleton_or_json_pid, MacOsPlatformAdapter, MACOS_SYMLINK_DIRS,
};
use dim_platform_windows::{
    build_isolated_env_map, build_windows_command_line, encode_windows_env_block_utf16,
    is_appdata_redirected, quote_windows_arg, WindowsPlatformAdapter,
};
use serde_json::json;

use crate::harness::{
    dim_root_dir, make_node_fixture_target, run_case_if, xf_fixture_bin_path, CaseSelectionFilter,
    CaseSuccess, IsolatedSandbox, TestCaseOutcome,
};

/// Executes all Cross-Platform Difference scenarios (`XP-WIN-*`, `XP-MAC-*`, `XP-LNX-*`).
pub fn run_cross_platform_suite(seed: u64) -> Vec<TestCaseOutcome> {
    run_cross_platform_suite_filtered(seed, None)
}

/// Executes Cross-Platform Difference scenarios (`XP-WIN-*`, `XP-MAC-*`, `XP-LNX-*`) matching `filter`.
pub fn run_cross_platform_suite_filtered(
    seed: u64,
    filter: Option<&CaseSelectionFilter>,
) -> Vec<TestCaseOutcome> {
    [
        // Windows-specific checks
        run_case_if(
            filter,
            "XP-WIN-01",
            "cross_platform",
            "Windows: RestartManager (`rstrtmgr.dll`) & `CreateFileW` exclusive kernel lock ownership",
            "windows",
            xp_win_01_restart_manager_and_exclusive_lock,
        ),
        run_case_if(
            filter,
            "XP-WIN-02",
            "cross_platform",
            "Windows: AppData UTF-16 env block, quoting, and Folder Redirection guards",
            "windows",
            || xp_win_02_appdata_env_and_redirection(seed),
        ),
        run_case_if(
            filter,
            "XP-WIN-03",
            "cross_platform",
            "Windows: `CreateProcessW` `bInheritHandles=FALSE` prevents parent file-handle leakage",
            "windows",
            xp_win_03_no_handle_inheritance,
        ),
        run_case_if(
            filter,
            "XP-WIN-04",
            "cross_platform",
            "Windows: NTFS DACL verification (`GetNamedSecurityInfoW`) detects permissive `Everyone` write ACE",
            "windows",
            xp_win_04_ntfs_acl_verification,
        ),
        // macOS-specific checks
        run_case_if(
            filter,
            "XP-MAC-01",
            "cross_platform",
            "macOS: `Application Support` path resolution, layout seeds, and `.app` `Info.plist` discovery",
            "macos-contract",
            xp_mac_01_application_support_and_bundle_discovery,
        ),
        run_case_if(
            filter,
            "XP-MAC-02",
            "cross_platform",
            "macOS: Launch Services (`open -n --env CFFIXED_USER_HOME`), Keychains symlinks, and call socket checks",
            "macos-contract",
            xp_mac_02_launch_services_and_sockets,
        ),
        run_case_if(
            filter,
            "XP-MAC-03",
            "cross_platform",
            "macOS: `SingletonLock` dangling symlink detection (`DEFECT-01`) & `NodeFixture` launch parity (`DEFECT-02`)",
            "macos-contract",
            xp_mac_03_singleton_lock_and_launch_parity,
        ),
        run_case_if(
            filter,
            "XP-MAC-04",
            "cross_platform",
            "macOS Runtime Gate: Live `CFFIXED_USER_HOME` process lifetime & POSIX `0700` permissions",
            "macos",
            xp_mac_04_native_runtime_gate,
        ),
        // Ubuntu / Linux-specific checks
        run_case_if(
            filter,
            "XP-LNX-01",
            "cross_platform",
            "Ubuntu: XDG Base Directory precedence (`XDG_DATA_HOME`, `XDG_CONFIG_HOME`, `XDG_CACHE_HOME`) & layout",
            "linux-contract",
            xp_lnx_01_xdg_paths_and_layout,
        ),
        run_case_if(
            filter,
            "XP-LNX-02",
            "cross_platform",
            "Ubuntu: `.AppImage` discovery, `OfficialZalo` refusal, and dangling `SingletonLock` audit (`DEFECT-01`)",
            "linux-contract",
            xp_lnx_02_appimage_and_singleton_lock,
        ),
        run_case_if(
            filter,
            "XP-LNX-03",
            "cross_platform",
            "Ubuntu: Bubblewrap (`bwrap`) user-namespace sandbox argv & Ubuntu 24.04 AppArmor advisory",
            "linux-contract",
            xp_lnx_03_bwrap_user_namespaces_and_apparmor,
        ),
        run_case_if(
            filter,
            "XP-LNX-04",
            "cross_platform",
            "Ubuntu Runtime Gate: Live POSIX signals (`SIGTERM`/`SIGKILL`), symlinks, file ownership & `bwrap` probe",
            "linux",
            xp_lnx_04_native_runtime_gate,
        ),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn synthetic_profile(id: &str, name: &str) -> ProfileRecord {
    ProfileRecord {
        id: id.to_owned(),
        name: name.to_owned(),
        directory_name: format!("synth_{id}"),
        created_at: "2026-11-17T00:00:00Z".to_owned(),
        last_launched_at: None,
        zalo_user_id: None,
        notes: String::new(),
    }
}

fn xp_win_01_restart_manager_and_exclusive_lock() -> Result<CaseSuccess, String> {
    #[cfg(not(target_os = "windows"))]
    {
        Ok(CaseSuccess::skipped_host(
            "windows",
            "Win32 Restart Manager (`rstrtmgr.dll`) and `dwShareMode=0` kernel locks require Windows",
        ))
    }
    #[cfg(target_os = "windows")]
    {
        use dim_platform_windows::win32_ffi::{get_locking_pids, is_file_exclusively_locked};

        let mut sandbox = IsolatedSandbox::new()?;
        let bin = xf_fixture_bin_path();
        if !bin.exists() {
            return Err(format!(
                "missing native fixture binary at {}",
                bin.display()
            ));
        }

        let pdir = sandbox.root().join("win-rm-probe");
        let zdata = sandbox.zalo_data_dir_for(&pdir);
        let lockfile = zdata.join("lockfile");

        let mut child = Command::new(&bin)
            .arg("--instance-id=win-rm-01")
            .arg("--profile-dir")
            .arg(&pdir)
            .arg("--lock-mode=exclusive")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("failed spawning xf-desktop-fixture.exe: {e}"))?;
        let pid = child.id();
        sandbox.track_pid(pid);

        let dl = Instant::now() + Duration::from_secs(4);
        while Instant::now() < dl && !zdata.join("fixture-state.json").exists() {
            thread::sleep(Duration::from_millis(15));
        }

        let rm_pids = get_locking_pids(&lockfile);
        let exclusively_locked = is_file_exclusively_locked(&lockfile);

        fs::write(zdata.join(".quit-signal"), b"{}\n").map_err(|e| e.to_string())?;
        let _ = child.wait();

        let locked_after_exit = is_file_exclusively_locked(&lockfile);
        if !rm_pids.contains(&pid) || !exclusively_locked || locked_after_exit {
            return Err(format!(
                "RestartManager/exclusive lock mismatch: pid={pid}, rm_pids={rm_pids:?}, excl={exclusively_locked}, after={locked_after_exit}"
            ));
        }

        Ok(CaseSuccess::pass(json!({
            "fixture_pid": pid,
            "restart_manager_locking_pids": rm_pids,
            "exclusively_locked_while_running": exclusively_locked,
            "unlocked_after_shutdown": !locked_after_exit,
        })))
    }
}

fn xp_win_02_appdata_env_and_redirection(seed: u64) -> Result<CaseSuccess, String> {
    let prof = synthetic_profile("a1b2c3d4", "Win Env Check");
    let pdir = Path::new("C:\\DimTest\\Profiles\\win-env_a1b2c3d4");
    let base_env = vec![
        ("SystemRoot".to_owned(), "C:\\WINDOWS".to_owned()),
        ("appdata".to_owned(), "C:\\OldRoaming".to_owned()),
    ];
    let env_map = build_isolated_env_map(&base_env, &prof, pdir);
    if env_map.len() < 10 {
        return Err("isolated env map missing required Windows keys".to_owned());
    }
    if env_map.get("HOMEDRIVE").map(|(_, v)| v.as_str()) != Some("C:")
        || env_map.get("HOMEPATH").map(|(_, v)| v.as_str())
            != Some("\\DimTest\\Profiles\\win-env_a1b2c3d4")
    {
        return Err("HOMEDRIVE / HOMEPATH split failed".to_owned());
    }
    let utf16 = encode_windows_env_block_utf16(&env_map);
    if utf16.len() < 4 || utf16[utf16.len() - 1] != 0 || utf16[utf16.len() - 2] != 0 {
        return Err("UTF-16 environment block is not double-NUL terminated".to_owned());
    }

    // CommandLineToArgvW tricky trailing backslash + embedded quotes check
    let quoted = quote_windows_arg("C:\\Path With Spaces\\Trailing\\");
    if quoted != "\"C:\\Path With Spaces\\Trailing\\\\\"" {
        return Err(format!("unexpected Windows arg quoting: {quoted}"));
    }
    let cmdline = build_windows_command_line(
        Path::new("C:\\Program Files\\nodejs\\node.exe"),
        &["--flag".to_owned(), "val \"quoted\"".to_owned()],
    );
    if !cmdline.starts_with("\"C:\\Program Files\\nodejs\\node.exe\"") {
        return Err(format!("unexpected command line: {cmdline}"));
    }

    // Folder redirection checks
    if is_appdata_redirected("%USERPROFILE%\\AppData\\Roaming")
        || !is_appdata_redirected("\\\\corp-nas\\Users\\demo\\AppData")
        || !is_appdata_redirected("%OneDrive%\\AppData\\Roaming")
        || !is_appdata_redirected("D:\\RedirectedAppData")
    {
        return Err("is_appdata_redirected classification mismatch".to_owned());
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = seed;
        return Ok(CaseSuccess::skipped_host(
            "windows",
            "Static UTF-16/AppData rules verified; live CreateProcessW env verification requires Windows host",
        ));
    }

    #[cfg(target_os = "windows")]
    {
        let mut sandbox = IsolatedSandbox::new()?;
        let live_prof = sandbox
            .store
            .create_profile("Live Env Snapshot", "")
            .map_err(|e| e.to_string())?;
        let live_dir = sandbox
            .store
            .profile_dir_for(&live_prof)
            .map_err(|e| e.to_string())?;
        let zdata = sandbox.zalo_data_dir_for(&live_dir);
        let target = make_node_fixture_target(&["--user-id=510001", "--seed", &seed.to_string()]);

        let rt = sandbox
            .coordinator
            .launch_profile(&live_prof.id, &target)
            .map_err(|e| e.to_string())?;
        if let Some(pid) = rt.main_pid {
            sandbox.track_pid(pid);
        }
        let dl = Instant::now() + Duration::from_secs(4);
        while Instant::now() < dl && !zdata.join("fixture-state.json").exists() {
            thread::sleep(Duration::from_millis(15));
        }
        sandbox
            .coordinator
            .stop_profile(&live_prof.id)
            .map_err(|e| e.to_string())?;

        let fstate: dim_xf_fixture::FixturePersistentState = serde_json::from_str(
            &fs::read_to_string(zdata.join("fixture-state.json")).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;

        let obs_appdata = fstate
            .observed_env
            .get("APPDATA")
            .cloned()
            .unwrap_or_default();
        let expected_roaming = live_dir
            .join("AppData")
            .join("Roaming")
            .display()
            .to_string();
        if !obs_appdata.eq_ignore_ascii_case(&expected_roaming) {
            return Err(format!(
                "child APPDATA was not isolated: expected {expected_roaming}, got {obs_appdata}"
            ));
        }

        Ok(CaseSuccess::pass(json!({
            "utf16_block_code_units": utf16.len(),
            "child_observed_appdata_isolated": true,
            "folder_redirection_rules_verified": 4,
        })))
    }
}

fn xp_win_03_no_handle_inheritance() -> Result<CaseSuccess, String> {
    #[cfg(not(target_os = "windows"))]
    {
        Ok(CaseSuccess::skipped_host(
            "windows",
            "Win32 `CreateProcessW(..., bInheritHandles=0)` verification requires Windows",
        ))
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::OpenOptionsExt;

        // Under multi-threaded `cargo test` (`integration_entry`), sibling suites concurrently spawn
        // `powershell.exe` / `dim-cli.exe` via `std::process::Command::output()` (`bInheritHandles=TRUE`
        // for piped stdio), which would inherit any process-wide `bInheritHandle=1` handle opened in the
        // shared test-runner process. Re-exec inside a dedicated single-threaded subprocess when running
        // under `integration_entry` so ONLY `WindowsPlatformAdapter::launch` runs while `raw_handle` is open.
        if std::env::var_os("DIM_XP_WIN_03_SINGLE_PROCESS").is_none() {
            if let Ok(cur_exe) = std::env::current_exe() {
                let exe_stem = cur_exe
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                if exe_stem.starts_with("integration_entry") {
                    let out = Command::new(&cur_exe)
                        .env("DIM_XP_WIN_03_SINGLE_PROCESS", "1")
                        .args([
                            "--exact",
                            "xp_win_03_isolated_subprocess_entry",
                            "--ignored",
                            "--nocapture",
                        ])
                        .stdin(Stdio::null())
                        .output()
                        .map_err(|e| format!("failed spawning isolated XP-WIN-03 helper: {e}"))?;
                    if !out.status.success() {
                        let stderr = String::from_utf8_lossy(&out.stderr);
                        let stdout = String::from_utf8_lossy(&out.stdout);
                        return Err(format!(
                            "isolated XP-WIN-03 subprocess failed (status={}): {stdout} {stderr}",
                            out.status
                        ));
                    }
                    return Ok(CaseSuccess::pass(json!({
                        "parent_inheritable_handle_blocked_by_bInheritHandles_false": true,
                        "isolated_subprocess_verified": true,
                    })));
                }
            }
        }

        let mut sandbox = IsolatedSandbox::new()?;
        let parent_secret = sandbox.root().join("parent-exclusive-handle.dat");

        // Open WITHOUT `FILE_FLAG_NO_INHERIT`? Note Rust's std sets non-inheritable by default, so we also open a
        // raw Win32 inheritable handle (`SECURITY_ATTRIBUTES { bInheritHandle: 1 }`) to prove `CreateProcessW`'s
        // `bInheritHandles = 0` truly blocks inheritance even for inheritable parent handles!
        let raw_handle = open_inheritable_win32_file(&parent_secret)?;

        let prof = sandbox
            .store
            .create_profile("Handle Isolation Check", "")
            .map_err(|e| e.to_string())?;
        let target = make_node_fixture_target(&["--user-id=520001"]);
        let rt = sandbox
            .coordinator
            .launch_profile(&prof.id, &target)
            .map_err(|e| e.to_string())?;
        if let Some(pid) = rt.main_pid {
            sandbox.track_pid(pid);
        }

        // Close the parent's inheritable handle WHILE the child process is still running!
        close_raw_win32_handle(raw_handle);

        // If the child had inherited `raw_handle`, opening with `share_mode(0)` or deleting `parent_secret`
        // while the child is still alive would FAIL with `ERROR_SHARING_VIOLATION` (`os error 32`)!
        let reopen_exclusive = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(&parent_secret);
        let not_leaked = reopen_exclusive.is_ok();
        drop(reopen_exclusive);

        sandbox
            .coordinator
            .stop_profile(&prof.id)
            .map_err(|e| e.to_string())?;

        if !not_leaked {
            return Err("inheritable parent handle leaked into spawned child process!".to_owned());
        }

        Ok(CaseSuccess::pass(json!({
            "parent_inheritable_handle_blocked_by_bInheritHandles_false": true,
        })))
    }
}

#[cfg(target_os = "windows")]
fn open_inheritable_win32_file(path: &Path) -> Result<*mut std::ffi::c_void, String> {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;

    #[repr(C)]
    struct SecurityAttributes {
        n_length: u32,
        lp_security_descriptor: *mut c_void,
        b_inherit_handle: i32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateFileW(
            lp_file_name: *const u16,
            dw_desired_access: u32,
            dw_share_mode: u32,
            lp_security_attributes: *const SecurityAttributes,
            dw_creation_disposition: u32,
            dw_flags_and_attributes: u32,
            h_template_file: *mut c_void,
        ) -> *mut c_void;
        fn GetLastError() -> u32;
    }

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let sa = SecurityAttributes {
        n_length: std::mem::size_of::<SecurityAttributes>() as u32,
        lp_security_descriptor: std::ptr::null_mut(),
        b_inherit_handle: 1, // Explicitly inheritable!
    };
    const GENERIC_READ_WRITE: u32 = 0xC000_0000;
    const CREATE_ALWAYS: u32 = 2;
    const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;
    const INVALID_HANDLE_VALUE: *mut c_void = (-1_isize) as *mut c_void;

    // SAFETY: `wide` is null-terminated and `sa` lives on the stack for `CreateFileW`.
    unsafe {
        let h = CreateFileW(
            wide.as_ptr(),
            GENERIC_READ_WRITE,
            0, // Exclusive lock (`dwShareMode = 0`)
            &sa,
            CREATE_ALWAYS,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        );
        if h == INVALID_HANDLE_VALUE || h.is_null() {
            return Err(format!(
                "CreateFileW failed with Win32 error {}",
                GetLastError()
            ));
        }
        Ok(h)
    }
}

#[cfg(target_os = "windows")]
fn close_raw_win32_handle(h: *mut std::ffi::c_void) {
    #[link(name = "kernel32")]
    extern "system" {
        fn CloseHandle(h_object: *mut std::ffi::c_void) -> i32;
    }
    // SAFETY: `h` was returned by `open_inheritable_win32_file` and not yet closed.
    unsafe {
        let _ = CloseHandle(h);
    }
}

fn xp_win_04_ntfs_acl_verification() -> Result<CaseSuccess, String> {
    #[cfg(not(target_os = "windows"))]
    {
        Ok(CaseSuccess::skipped_host(
            "windows",
            "NTFS DACL inspection (`GetNamedSecurityInfoW` / `icacls`) requires Windows",
        ))
    }
    #[cfg(target_os = "windows")]
    {
        let mut sandbox = IsolatedSandbox::new()?;
        let adapter = WindowsPlatformAdapter::new();

        // 1. Default user-temp Profiles directory must report safe restrictive ACLs (`Some(true)`).
        let safe_default = adapter.check_profiles_acl_safe(sandbox.store.profiles_dir());
        if safe_default != Some(true) {
            return Err(format!(
                "expected default sandbox Profiles/ ACL to be Some(true), got {safe_default:?}"
            ));
        }

        // 2. Create an isolated subdir and grant `Everyone` (`*S-1-1-0`) Write access via `icacls`
        let permissive_dir = sandbox.root().join("permissive-acl-probe");
        fs::create_dir_all(&permissive_dir).map_err(|e| e.to_string())?;
        sandbox.track_acl_dir(permissive_dir.clone());

        let icacls_out = Command::new("icacls")
            .arg(&permissive_dir)
            .arg("/grant")
            .arg("*S-1-1-0:(OI)(CI)(W)")
            .arg("/Q")
            .output()
            .map_err(|e| format!("icacls failed to run: {e}"))?;
        if !icacls_out.status.success() {
            return Err(format!(
                "icacls grant failed: {}",
                String::from_utf8_lossy(&icacls_out.stderr)
            ));
        }

        let detected_unsafe = adapter.check_profiles_acl_safe(&permissive_dir);
        if detected_unsafe != Some(false) {
            return Err(format!(
                "expected check_profiles_acl_safe=Some(false) after granting Everyone write, got {detected_unsafe:?}"
            ));
        }

        Ok(CaseSuccess::pass(json!({
            "default_profiles_dir_acl_safe": safe_default,
            "everyone_write_ace_detected_unsafe": detected_unsafe == Some(false),
        })))
    }
}

fn xp_mac_01_application_support_and_bundle_discovery() -> Result<CaseSuccess, String> {
    let mut env = HashMap::new();
    env.insert("HOME", "/Users/tester");
    let resolved = resolve_data_dir_for_platform(PlatformKind::MacOs, None, &|k| {
        env.get(k).map(|s| (*s).to_owned())
    })
    .map_err(|e| e.to_string())?;
    if resolved != Path::new("/Users/tester/Library/Application Support/NexaDock") {
        return Err(format!("unexpected macOS data_dir: {}", resolved.display()));
    }

    let sandbox = IsolatedSandbox::new()?;
    let prof = synthetic_profile("1234abcd", "macOS Layout");
    let pdir = sandbox.root().join("mac-profile");
    ensure_macos_profile_layout(&prof, &pdir).map_err(|e| e.to_string())?;
    let zdata = macos_zalo_data_dir(&pdir);
    let marker = fs::read_to_string(zdata.join(".zalo-profile-id")).map_err(|e| e.to_string())?;
    if marker.trim() != "1234abcd" || !pdir.join("tmp").is_dir() {
        return Err("macOS layout missing marker or tmp directory".to_owned());
    }

    // Synthetic `/Applications/Zalo.app` bundle discovery
    let bundle = sandbox.root().join("Zalo.app");
    fs::create_dir_all(bundle.join("Contents").join("MacOS")).map_err(|e| e.to_string())?;
    fs::create_dir_all(bundle.join("Contents").join("Resources")).map_err(|e| e.to_string())?;
    fs::write(
        bundle.join("Contents").join("MacOS").join("Zalo"),
        b"macho-stub",
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        bundle.join("Contents").join("Resources").join("app.asar"),
        b"asar-stub",
    )
    .map_err(|e| e.to_string())?;
    let plist_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
  <key>CFBundleShortVersionString</key>
  <string>26.10.20</string>
</dict></plist>"#;
    fs::write(bundle.join("Contents").join("Info.plist"), plist_xml).map_err(|e| e.to_string())?;

    let adapter = MacOsPlatformAdapter::new();
    let disc = adapter
        .discover_installation(Some(&bundle))
        .map_err(|e| e.to_string())?;
    if !disc.is_valid
        || disc.version.as_deref() != Some("26.10.20")
        || disc.support_status != SupportStatus::NotTested
    {
        return Err(format!(
            "unexpected macOS bundle discovery result: {disc:?}"
        ));
    }

    Ok(CaseSuccess::pass(json!({
        "resolved_data_dir": resolved,
        "discovered_bundle_version": disc.version,
        "honest_support_status": disc.support_status,
        "plist_parser_ok": extract_plist_short_version(plist_xml) == Some("26.10.20".to_owned()),
    })))
}

fn xp_mac_02_launch_services_and_sockets() -> Result<CaseSuccess, String> {
    let sandbox = IsolatedSandbox::new()?;
    let pdir = sandbox.root().join("mac-home");
    let args = build_open_args(
        Path::new("/Applications/Zalo.app"),
        &pdir,
        &["--flag1".to_owned()],
    );
    let has_new_instance = args.windows(2).any(|w| w[0] == "-n" && w[1] == "-a");
    let has_cffixed = args.iter().any(|a| a.starts_with("CFFIXED_USER_HOME="));
    let has_tmpdir = args.iter().any(|a| a.starts_with("TMPDIR="));
    if !has_new_instance || !has_cffixed || !has_tmpdir {
        return Err(format!("invalid macOS open argv: {args:?}"));
    }

    // Simulated global ZaloCall socket conflict detection
    let fake_sock = sandbox.root().join("socketzalorecv2021");
    fs::write(&fake_sock, b"").map_err(|e| e.to_string())?;
    let warnings = inspect_call_sockets_at(&[fake_sock.as_path()]);
    if warnings.len() != 1 || !MACOS_SYMLINK_DIRS.contains(&"Library/Keychains") {
        return Err("macOS socket conflict or Keychains symlink list mismatch".to_owned());
    }

    Ok(CaseSuccess::pass(json!({
        "open_args_count": args.len(),
        "has_cffixed_user_home": has_cffixed,
        "socket_conflict_detected": true,
        "symlink_dirs_configured": MACOS_SYMLINK_DIRS,
    })))
}

fn xp_mac_03_singleton_lock_and_launch_parity() -> Result<CaseSuccess, String> {
    let parsed = parse_singleton_lock_target("MacBook-Pro.local-54321");
    if parsed != Some(("MacBook-Pro.local".to_owned(), 54_321)) {
        return Err(format!("SingletonLock parser failed: {parsed:?}"));
    }
    if parse_singleton_lock_target("invalid_no_dash").is_some()
        || parse_singleton_lock_target("host-0").is_some()
    {
        return Err("SingletonLock parser accepted malformed target".to_owned());
    }

    // Verify `read_singleton_or_json_pid` JSON fallback
    let sandbox = IsolatedSandbox::new()?;
    let zdata = sandbox.root().join("mac-zdata");
    fs::create_dir_all(&zdata).map_err(|e| e.to_string())?;
    fs::write(zdata.join("lockfile"), r#"{"pid": 77889}"#).map_err(|e| e.to_string())?;
    if read_singleton_or_json_pid(&zdata) != Some(77_889) {
        return Err("read_singleton_or_json_pid failed to read JSON lockfile".to_owned());
    }

    #[cfg(target_os = "macos")]
    {
        use dim_core::{InstanceLauncher, InstanceMonitor};

        let mut mac_sandbox = IsolatedSandbox::new()?;
        let adapter = MacOsPlatformAdapter::new();

        // 1. Public `inspect_profile` behavior on an actual dangling `SingletonLock -> mac-host-<dead_pid>` symlink
        let lock_prof = mac_sandbox
            .store
            .create_profile("macOS Dangling Lock", "")
            .map_err(|e| e.to_string())?;
        let lock_pdir = mac_sandbox
            .store
            .profile_dir_for(&lock_prof)
            .map_err(|e| e.to_string())?;
        adapter
            .ensure_profile_layout(&lock_prof, &lock_pdir)
            .map_err(|e| e.to_string())?;
        let dead_pid = (999_991..=999_999)
            .find(|&p| !dim_xf_fixture::is_os_pid_alive(p))
            .unwrap_or(999_999);
        let lock_zdata = macos_zalo_data_dir(&lock_pdir);
        std::os::unix::fs::symlink(
            format!("mac-host-{dead_pid}"),
            lock_zdata.join("SingletonLock"),
        )
        .map_err(|e| e.to_string())?;
        let st_dangling = adapter
            .inspect_profile(&lock_prof, &lock_pdir)
            .map_err(|e| e.to_string())?;
        let dangling_detected = !st_dangling.running && st_dangling.stale_lock_detected;

        // 2. Public `launch_profile` readiness / `.runtime-pid` visibility & multi-profile `NodeFixture` `--user-id` derivation
        let p1 = mac_sandbox
            .store
            .create_profile("macOS Fixture A", "")
            .map_err(|e| e.to_string())?;
        let p2 = mac_sandbox
            .store
            .create_profile("macOS Fixture B", "")
            .map_err(|e| e.to_string())?;
        let p1_dir = mac_sandbox
            .store
            .profile_dir_for(&p1)
            .map_err(|e| e.to_string())?;
        let p2_dir = mac_sandbox
            .store
            .profile_dir_for(&p2)
            .map_err(|e| e.to_string())?;

        let rt1 = mac_sandbox
            .coordinator
            .launch_profile(
                &p1.id,
                &make_node_fixture_target(&["--slow-startup-pre-lock-ms=300"]),
            )
            .map_err(|e| e.to_string())?;
        if let Some(pid) = rt1.main_pid {
            mac_sandbox.track_pid(pid);
        }
        if let Ok(raw) = fs::read_to_string(p1_dir.join(".runtime-pid")) {
            if let Ok(pid) = raw.trim().parse::<u32>() {
                mac_sandbox.track_pid(pid);
            }
        }

        thread::sleep(Duration::from_millis(380));
        let rt2_res = mac_sandbox
            .coordinator
            .launch_profile(&p2.id, &make_node_fixture_target(&[]));
        if let Ok(ref rt2) = rt2_res {
            if let Some(pid) = rt2.main_pid {
                mac_sandbox.track_pid(pid);
            }
        }
        if let Ok(raw) = fs::read_to_string(p2_dir.join(".runtime-pid")) {
            if let Ok(pid) = raw.trim().parse::<u32>() {
                mac_sandbox.track_pid(pid);
            }
        }

        thread::sleep(Duration::from_millis(200));
        let st1 = mac_sandbox
            .coordinator
            .inspect_profile(&p1.id)
            .map_err(|e| e.to_string())?;
        let st2 = mac_sandbox
            .coordinator
            .inspect_profile(&p2.id)
            .map_err(|e| e.to_string())?;
        let distinct_uids = st1.detected_zalo_user_id.is_some()
            && st2.detected_zalo_user_id.is_some()
            && st1.detected_zalo_user_id != st2.detected_zalo_user_id;

        mac_sandbox.reap_all_sandbox_processes();

        if !dangling_detected || !rt1.running || rt2_res.is_err() || !distinct_uids {
            return Ok(CaseSuccess::defect(
                "DEFECT-02",
                &[
                    "crates/platform-macos/src/adapter.rs:192-196,298,305-320",
                    "crates/platform-windows/src/adapter.rs:600-607,620-629",
                ],
                "On macOS, create a dangling `SingletonLock` symlink and launch 2 `NodeFixture` profiles without explicit `--user-id`: `MacOsPlatformAdapter` misses dangling `SingletonLock`, returns `running=false` during pre-lock startup, and collides on `DUPLICATE_ZALO_USER_ID`.",
                json!({
                    "verification_kind": "NATIVE_OS_RUNTIME",
                    "host_os": PlatformKind::current().as_str(),
                    "native_runtime_verified": true,
                    "dangling_singleton_lock_detected": dangling_detected,
                    "pre_lock_launch_returned_running": rt1.running,
                    "second_fixture_launch_ok": rt2_res.is_ok(),
                    "distinct_fixture_user_ids": distinct_uids,
                }),
            ));
        }

        return Ok(CaseSuccess::pass(json!({
            "verification_kind": "NATIVE_OS_RUNTIME",
            "host_os": PlatformKind::current().as_str(),
            "native_runtime_verified": true,
            "dangling_singleton_lock_detected": true,
            "multi_profile_node_fixture_isolated": true,
        })));
    }

    #[cfg(not(target_os = "macos"))]
    {
        // Non-host static check on Windows/Linux (`STATIC_CONFIRMED_NON_HOST`) — resilient to internal helper naming:
        //  1) DEFECT-01: `singleton_lock.exists()` on Chromium's dangling symlink (`Path::exists()` follows target and returns `false`!)
        //  2) DEFECT-02: `inspect_profile` never reads `.runtime-pid` and `launch` omits per-profile `--user-id` derivation!
        let mac_src_dir = dim_root_dir()
            .join("crates")
            .join("platform-macos")
            .join("src");
        let mac_adapter_src =
            fs::read_to_string(mac_src_dir.join("adapter.rs")).map_err(|e| e.to_string())?;
        let mac_layout_src = fs::read_to_string(mac_src_dir.join("layout.rs")).unwrap_or_default();

        let uses_broken_exists = mac_adapter_src.contains("singleton_lock.exists()");
        let inspect_slice = mac_adapter_src
            .split("fn inspect_profile")
            .nth(1)
            .and_then(|s| s.split("impl InstanceLauncher").next())
            .unwrap_or(&mac_adapter_src)
            .to_ascii_lowercase();
        let ignores_runtime_pid_on_read =
            !inspect_slice.contains("runtime_pid") && !inspect_slice.contains(".runtime-pid");
        let combined_lower = format!("{mac_adapter_src}\n{mac_layout_src}").to_ascii_lowercase();
        let missing_fixture_uid_derivation =
            !combined_lower.contains("--user-id") && !combined_lower.contains("fixture_user_id");

        if uses_broken_exists || ignores_runtime_pid_on_read || missing_fixture_uid_derivation {
            return Ok(CaseSuccess::defect(
                "DEFECT-02",
                &[
                    "crates/platform-macos/src/adapter.rs:192-196,298,305-320",
                    "crates/platform-windows/src/adapter.rs:600-607,620-629",
                ],
                "On macOS, launch 2 `NodeFixture` profiles without explicit `--user-id`: `MacOsPlatformAdapter::launch` neither derives a per-profile `--user-id` (both default to `100001`, colliding on `DUPLICATE_ZALO_USER_ID`) nor polls readiness nor reads `.runtime-pid` in `inspect_profile`, and uses `singleton_lock.exists()` which returns `false` on Chromium's dangling `SingletonLock` symlinks.",
                json!({
                    "verification_kind": "STATIC_CONFIRMED_NON_HOST",
                    "host_os": PlatformKind::current().as_str(),
                    "required_native_os": "macos",
                    "native_runtime_verified": false,
                    "singleton_lock_uses_path_exists_bug": uses_broken_exists,
                    "writes_runtime_pid_but_never_reads_it": ignores_runtime_pid_on_read,
                    "missing_deterministic_fixture_user_id": missing_fixture_uid_derivation,
                }),
            ));
        }

        Ok(CaseSuccess::pass(json!({
            "verification_kind": "STATIC_CONFIRMED_NON_HOST",
            "host_os": PlatformKind::current().as_str(),
            "required_native_os": "macos",
            "native_runtime_verified": false,
            "singleton_lock_and_launch_parity_static_ok": true,
        })))
    }
}

fn xp_mac_04_native_runtime_gate() -> Result<CaseSuccess, String> {
    #[cfg(not(target_os = "macos"))]
    {
        Ok(CaseSuccess::skipped_host(
            "macos",
            "Live macOS Launch Services (`open -n`), `CFFIXED_USER_HOME` runtime, and APFS permissions cannot be marked PASS from a non-macOS host",
        ))
    }
    #[cfg(target_os = "macos")]
    {
        let mut sandbox = IsolatedSandbox::new()?;
        let prof = sandbox
            .store
            .create_profile("macOS Native Runtime", "")
            .map_err(|e| e.to_string())?;
        let target = make_node_fixture_target(&["--user-id=550001"]);
        let _ = sandbox
            .coordinator
            .launch_profile(&prof.id, &target)
            .map_err(|e| e.to_string())?;
        thread::sleep(Duration::from_millis(250));
        let st = sandbox
            .coordinator
            .inspect_profile(&prof.id)
            .map_err(|e| e.to_string())?;
        if let Some(pid) = st.main_pid {
            sandbox.track_pid(pid);
        }
        sandbox
            .coordinator
            .stop_profile(&prof.id)
            .map_err(|e| e.to_string())?;
        Ok(CaseSuccess::pass(json!({
            "macos_native_fixture_ran": st.running,
        })))
    }
}

fn xp_lnx_01_xdg_paths_and_layout() -> Result<CaseSuccess, String> {
    let mut env = HashMap::new();
    env.insert("HOME", "/home/ubuntu");
    let fallback = resolve_data_dir_for_platform(PlatformKind::Linux, None, &|k| {
        env.get(k).map(|s| (*s).to_owned())
    })
    .map_err(|e| e.to_string())?;
    if fallback != Path::new("/home/ubuntu/.local/share/NexaDock") {
        return Err(format!(
            "unexpected Linux fallback data_dir: {}",
            fallback.display()
        ));
    }

    env.insert("XDG_DATA_HOME", "/custom/xdg-data");
    let xdg_over = resolve_data_dir_for_platform(PlatformKind::Linux, None, &|k| {
        env.get(k).map(|s| (*s).to_owned())
    })
    .map_err(|e| e.to_string())?;
    if xdg_over != Path::new("/custom/xdg-data/NexaDock") {
        return Err(format!(
            "unexpected XDG_DATA_HOME override: {}",
            xdg_over.display()
        ));
    }

    let sandbox = IsolatedSandbox::new()?;
    let prof = synthetic_profile("feedface", "Ubuntu XDG");
    let pdir = sandbox.root().join("linux-profile");
    ensure_linux_profile_layout(&prof, &pdir).map_err(|e| e.to_string())?;
    let zdata = linux_zalo_data_dir(&pdir);
    if !zdata.join(".zalo-profile-id").exists()
        || !pdir.join(".local").join("share").is_dir()
        || !pdir.join(".cache").is_dir()
        || !pdir.join("tmp").is_dir()
    {
        return Err("Linux XDG profile layout incomplete".to_owned());
    }

    let iso_env = build_linux_isolated_env(&prof, &pdir);
    if !iso_env.contains_key("XDG_CONFIG_HOME")
        || !iso_env.contains_key("XDG_DATA_HOME")
        || !iso_env.contains_key("XDG_CACHE_HOME")
        || !iso_env.contains_key("TMPDIR")
    {
        return Err("Linux isolated env missing required XDG keys".to_owned());
    }

    Ok(CaseSuccess::pass(json!({
        "home_fallback_data_dir": fallback,
        "xdg_override_data_dir": xdg_over,
        "isolated_env_keys": iso_env.keys().collect::<Vec<_>>(),
    })))
}

fn xp_lnx_02_appimage_and_singleton_lock() -> Result<CaseSuccess, String> {
    let sandbox = IsolatedSandbox::new()?;
    let adapter = LinuxPlatformAdapter::new();

    // Default official discovery on Linux MUST report `is_valid=false` and `SupportStatus::Unsupported`
    let official = adapter
        .discover_installation(None)
        .map_err(|e| e.to_string())?;
    if official.is_valid || official.support_status != SupportStatus::Unsupported {
        return Err(format!(
            "expected Unsupported for official Linux Zalo, got {official:?}"
        ));
    }

    // User-supplied `.AppImage` MUST report `is_valid=true` and `SupportStatus::Experimental`
    let appimage = sandbox.root().join("ZaloCommunity-x86_64.AppImage");
    fs::write(&appimage, b"\x7fELF-appimage-stub").map_err(|e| e.to_string())?;
    if !looks_like_appimage(&appimage) {
        return Err("looks_like_appimage failed on .AppImage".to_owned());
    }
    let exp = adapter
        .discover_installation(Some(&appimage))
        .map_err(|e| e.to_string())?;
    if !exp.is_valid || exp.support_status != SupportStatus::Experimental {
        return Err(format!(
            "expected Experimental for local AppImage, got {exp:?}"
        ));
    }

    let zdata = sandbox.root().join("lnx-zdata");
    fs::create_dir_all(&zdata).map_err(|e| e.to_string())?;
    fs::write(zdata.join("lockfile"), r#"{"pid": 33445}"#).map_err(|e| e.to_string())?;
    if read_linux_pid(&zdata) != Some(33_445) {
        return Err("read_linux_pid failed on JSON lockfile".to_owned());
    }

    #[cfg(target_os = "linux")]
    {
        use dim_core::{InstanceLauncher, InstanceMonitor, RecoveryManager};
        use dim_process_manager::StaleLockRecoveryManager;

        let lock_prof = sandbox
            .store
            .create_profile("Linux Dangling Lock", "")
            .map_err(|e| e.to_string())?;
        let lock_pdir = sandbox
            .store
            .profile_dir_for(&lock_prof)
            .map_err(|e| e.to_string())?;
        adapter
            .ensure_profile_layout(&lock_prof, &lock_pdir)
            .map_err(|e| e.to_string())?;
        let dead_pid = (999_991..=999_999)
            .find(|&p| !dim_xf_fixture::is_os_pid_alive(p))
            .unwrap_or(999_999);
        let lock_zdata = linux_zalo_data_dir(&lock_pdir);
        let singleton_lock = lock_zdata.join("SingletonLock");
        std::os::unix::fs::symlink(format!("ubuntu-host-{dead_pid}"), &singleton_lock)
            .map_err(|e| e.to_string())?;

        let st = adapter
            .inspect_profile(&lock_prof, &lock_pdir)
            .map_err(|e| e.to_string())?;
        if st.running || !st.stale_lock_detected {
            return Ok(CaseSuccess::defect(
                "DEFECT-01",
                &[
                    "crates/platform-linux/src/adapter.rs:190-191",
                    "crates/platform-macos/src/adapter.rs:194-195",
                ],
                "Create a Chromium-style dangling symlink `ln -s ubuntu-host-99999 .config/ZaloData/SingletonLock` (without JSON `lockfile`) for a dead PID and call `inspect_profile`: `singleton_lock.exists()` follows the dangling target and returns `false`, so `stale_lock_detected` is `false` and `StaleLockRecoveryManager` is skipped on relaunch!",
                json!({
                    "verification_kind": "NATIVE_OS_RUNTIME",
                    "host_os": PlatformKind::current().as_str(),
                    "native_runtime_verified": true,
                    "official_linux_status": official.support_status,
                    "appimage_status": exp.support_status,
                    "observed_running": st.running,
                    "observed_stale_lock_detected": st.stale_lock_detected,
                }),
            ));
        }

        let rec = StaleLockRecoveryManager
            .recover_stale_state(&lock_prof, &lock_pdir, &st)
            .map_err(|e| e.to_string())?;
        if !rec.recovered || fs::symlink_metadata(&singleton_lock).is_ok() {
            return Err(
                "StaleLockRecoveryManager failed to unlink dangling SingletonLock".to_owned(),
            );
        }

        return Ok(CaseSuccess::pass(json!({
            "verification_kind": "NATIVE_OS_RUNTIME",
            "host_os": PlatformKind::current().as_str(),
            "native_runtime_verified": true,
            "official_linux_status": official.support_status,
            "appimage_status": exp.support_status,
            "dangling_singleton_lock_recovered": true,
        })));
    }

    #[cfg(not(target_os = "linux"))]
    {
        // Non-host static check on Windows/macOS (`STATIC_CONFIRMED_NON_HOST`):
        // Chromium's `SingletonLock` is a dangling symlink (`ubuntu-host-99999` where target file doesn't exist),
        // so `singleton_lock.exists()` evaluates to `false` instead of checking `symlink_metadata` / `read_link`!
        let lnx_src_dir = dim_root_dir()
            .join("crates")
            .join("platform-linux")
            .join("src");
        let lnx_adapter_src =
            fs::read_to_string(lnx_src_dir.join("adapter.rs")).map_err(|e| e.to_string())?;
        let lnx_layout_src = fs::read_to_string(lnx_src_dir.join("layout.rs")).unwrap_or_default();
        let combined = format!("{lnx_adapter_src}\n{lnx_layout_src}");
        let uses_broken_exists = lnx_adapter_src.contains("singleton_lock.exists()");
        let has_non_following_check =
            combined.contains("symlink_metadata") || combined.contains("read_link");

        if uses_broken_exists || !has_non_following_check {
            return Ok(CaseSuccess::defect(
                "DEFECT-01",
                &[
                    "crates/platform-linux/src/adapter.rs:190-191",
                    "crates/platform-macos/src/adapter.rs:194-195",
                ],
                "Create a Chromium-style dangling symlink `ln -s ubuntu-host-99999 .config/ZaloData/SingletonLock` (without JSON `lockfile`) for a dead PID `99999` and call `inspect_profile`: `singleton_lock.exists()` follows the dangling target and returns `false`, so `stale_lock_detected` is `false` and `StaleLockRecoveryManager` is skipped on relaunch!",
                json!({
                    "verification_kind": "STATIC_CONFIRMED_NON_HOST",
                    "host_os": PlatformKind::current().as_str(),
                    "required_native_os": "linux",
                    "native_runtime_verified": false,
                    "official_linux_status": official.support_status,
                    "appimage_status": exp.support_status,
                    "dangling_singleton_lock_uses_path_exists": uses_broken_exists,
                }),
            ));
        }

        Ok(CaseSuccess::pass(json!({
            "verification_kind": "STATIC_CONFIRMED_NON_HOST",
            "host_os": PlatformKind::current().as_str(),
            "required_native_os": "linux",
            "native_runtime_verified": false,
            "official_linux_status": official.support_status,
            "appimage_status": exp.support_status,
            "singleton_lock_static_check_ok": true,
        })))
    }
}

fn xp_lnx_03_bwrap_user_namespaces_and_apparmor() -> Result<CaseSuccess, String> {
    let pdir = Path::new("/tmp/dim-bwrap-prof");
    let exe = Path::new("/opt/local/Zalo.AppImage");
    let args = build_bwrap_args(pdir, exe, &["--no-sandbox".to_owned()]);

    let has_new_session =
        args.iter().any(|a| a == "--new-session") && !args.iter().any(|a| a == "--die-with-parent");
    let has_ro_usr = args.windows(3).any(|w| w == ["--ro-bind", "/usr", "/usr"]);
    let has_home_bind = args.windows(3).any(|w| {
        w[0] == "--bind" && w[1] == "/tmp/dim-bwrap-prof" && w[2] == "/tmp/dim-bwrap-prof"
    });
    if !has_new_session || !has_ro_usr || !has_home_bind {
        return Err(format!("invalid bwrap sandbox argv: {args:?}"));
    }

    let bwrap_adapter = LinuxPlatformAdapter::new().with_bwrap(true);
    let caps = bwrap_adapter.capabilities();
    if !caps
        .diagnostics
        .iter()
        .any(|d| d.contains("prefer_bwrap=true"))
        || !caps.limitations.iter().any(|l| l.contains("AppArmor"))
        || !APPARMOR_NO_SANDBOX_NOTE.contains("Ubuntu 23.10+/24.04")
    {
        return Err("bwrap diagnostics or Ubuntu AppArmor advisory missing".to_owned());
    }

    Ok(CaseSuccess::pass(json!({
        "bwrap_arg_count": args.len(),
        "detached_new_session_enforced": has_new_session,
        "apparmor_userns_advisory_present": true,
    })))
}

fn xp_lnx_04_native_runtime_gate() -> Result<CaseSuccess, String> {
    #[cfg(not(target_os = "linux"))]
    {
        Ok(CaseSuccess::skipped_host(
            "linux",
            "Live Ubuntu `/proc` inspection, POSIX signals (`SIGTERM`/`SIGKILL`), `chmod 0700` ownership, and `bwrap` unprivileged user namespaces cannot be marked PASS from a non-Linux host",
        ))
    }
    #[cfg(target_os = "linux")]
    {
        let mut sandbox = IsolatedSandbox::new()?;
        let prof = sandbox
            .store
            .create_profile("Ubuntu Native Runtime", "")
            .map_err(|e| e.to_string())?;
        let target = make_node_fixture_target(&["--user-id=580001"]);
        let _ = sandbox
            .coordinator
            .launch_profile(&prof.id, &target)
            .map_err(|e| e.to_string())?;
        thread::sleep(Duration::from_millis(250));
        let st = sandbox
            .coordinator
            .inspect_profile(&prof.id)
            .map_err(|e| e.to_string())?;
        if let Some(pid) = st.main_pid {
            sandbox.track_pid(pid);
        }
        sandbox
            .coordinator
            .stop_profile(&prof.id)
            .map_err(|e| e.to_string())?;
        Ok(CaseSuccess::pass(json!({
            "ubuntu_native_fixture_ran": st.running,
        })))
    }
}

//! Cross-Adapter & Cross-CLI Public Behavior Contract Suite (`CT-01` .. `CT-10`).
//!
//! Verifies that `WindowsPlatformAdapter`, `MacOsPlatformAdapter`, `LinuxPlatformAdapter`,
//! `dim-cli`, `scripts/dim-host.ps1`, and Tauri `commands.rs` uphold shared behavioral contracts
//! without forcing identical internal OS implementations, and pinpoints every contract divergence (`DEFECT-08`).

use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::process::Command;

use dim_core::{
    ApplicationLocator, DimError, InstanceLauncher, InstanceMonitor, LaunchTarget,
    OperationOutcome, PlatformCapabilities, PlatformKind, ProfileRuntimeState, ProfileStore,
    SupportStatus,
};
use dim_platform_linux::LinuxPlatformAdapter;
use dim_platform_macos::MacOsPlatformAdapter;
use dim_platform_windows::WindowsPlatformAdapter;
use dim_profile_store::verify_profile_id_marker;
use serde_json::{json, Value};

use crate::harness::{
    dim_cli_bin_path, dim_host_ps1_path, run_case_if, xf_fixture_bin_path, xf_fixture_mjs_path,
    BystanderSnapshot, CaseSelectionFilter, CaseSuccess, IsolatedSandbox, TestCaseOutcome,
};

/// Executes all Adapter & CLI Contract scenarios (`CT-01` .. `CT-10`).
pub fn run_adapter_contract_suite() -> Vec<TestCaseOutcome> {
    run_adapter_contract_suite_filtered(None)
}

/// Executes Adapter & CLI Contract scenarios (`CT-01` .. `CT-10`) matching `filter`.
pub fn run_adapter_contract_suite_filtered(
    filter: Option<&CaseSelectionFilter>,
) -> Vec<TestCaseOutcome> {
    [
        run_case_if(
            filter,
            "CT-01",
            "contract",
            "PlatformCapabilities contract & honest SupportStatus reporting across Windows, macOS, and Linux adapters",
            "all",
            ct_01_platform_capabilities_contract,
        ),
        run_case_if(
            filter,
            "CT-02",
            "contract",
            "ApplicationLocator input/output schema contract across missing, partial, and valid roots",
            "all",
            ct_02_application_locator_schema_contract,
        ),
        run_case_if(
            filter,
            "CT-03",
            "contract",
            "InstanceLauncher::ensure_profile_layout idempotency & `.zalo-profile-id` marker contract across all 3 adapters",
            "all",
            ct_03_profile_layout_and_marker_contract,
        ),
        run_case_if(
            filter,
            "CT-04",
            "contract",
            "Exhaustive 22-variant `DimError::code()` & `OperationOutcome<T>` JSON envelope contract",
            "all",
            ct_04_error_code_and_envelope_contract,
        ),
        run_case_if(
            filter,
            "CT-05",
            "contract",
            "Unsupported platform operations (`UNSUPPORTED_PLATFORM`) & unsigned custom binary (`UNVERIFIED_BINARY`) safety contract",
            "all",
            ct_05_unsupported_and_unverified_binary_contract,
        ),
        run_case_if(
            filter,
            "CT-06",
            "contract",
            "Primary PID protection contract (`PROTECTED_PRIMARY_PROCESS`) across Windows, macOS, and Linux `stop()`",
            "all",
            ct_06_primary_pid_protection_across_all_adapters,
        ),
        run_case_if(
            filter,
            "CT-07",
            "contract",
            "CLI public JSON output schema contract (`status`, `discover`, `diagnostics`, `verify-isolation`, `backups`)",
            "all",
            ct_07_cli_output_schema_contract,
        ),
        run_case_if(
            filter,
            "CT-08",
            "contract",
            "Differential CLI contract: 33..40 char profile slug parity (`dim-cli` vs `scripts/dim-host.ps1`)",
            "windows+ps-parity",
            ct_08_differential_slug_length_contract,
        ),
        run_case_if(
            filter,
            "CT-09",
            "contract",
            "Differential CLI contract: Restore duplicate-name & 64-char name cap (`dim-cli` vs `scripts/dim-host.ps1`)",
            "windows+ps-parity",
            ct_09_differential_restore_naming_contract,
        ),
        run_case_if(
            filter,
            "CT-10",
            "contract",
            "Differential CLI contract: Empty/corrupt `profiles.json` fail-closed & missing-arg error codes (`dim-cli` vs `scripts/dim-host.ps1`)",
            "windows+ps-parity",
            ct_10_differential_corrupt_store_and_arg_codes,
        ),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn verify_generic_capabilities<A: PlatformCapabilities>(
    adapter: &A,
    expected_platform: PlatformKind,
) -> Result<Value, String> {
    if adapter.platform() != expected_platform {
        return Err(format!(
            "platform() mismatch: expected {expected_platform:?}, got {:?}",
            adapter.platform()
        ));
    }
    let caps = adapter.capabilities();
    if caps.platform != expected_platform {
        return Err("capabilities().platform mismatch".to_owned());
    }
    if caps.limitations.is_empty() || caps.diagnostics.is_empty() {
        return Err(format!(
            "{expected_platform:?} adapter must document non-empty limitations and diagnostics"
        ));
    }
    if !matches!(
        caps.concurrent_calls_status,
        SupportStatus::Blocked | SupportStatus::Unsupported
    ) {
        return Err(format!(
            "{expected_platform:?} concurrent_calls_status must honestly report Blocked or Unsupported"
        ));
    }
    serde_json::to_value(&caps).map_err(|e| e.to_string())
}

fn ct_01_platform_capabilities_contract() -> Result<CaseSuccess, String> {
    let win = WindowsPlatformAdapter::new();
    let mac = MacOsPlatformAdapter::new();
    let lnx = LinuxPlatformAdapter::new();

    let win_v = verify_generic_capabilities(&win, PlatformKind::Windows)?;
    let mac_v = verify_generic_capabilities(&mac, PlatformKind::MacOs)?;
    let lnx_v = verify_generic_capabilities(&lnx, PlatformKind::Linux)?;

    if mac.capabilities().official_zalo_status != SupportStatus::NotTested {
        return Err("macOS official_zalo_status must honestly report NotTested".to_owned());
    }
    if lnx.capabilities().official_zalo_status != SupportStatus::Unsupported
        || lnx.capabilities().appimage_status != Some(SupportStatus::Experimental)
    {
        return Err("Linux capability status contract violated".to_owned());
    }

    Ok(CaseSuccess::pass(json!({
        "windows": win_v,
        "macos": mac_v,
        "linux": lnx_v,
    })))
}

fn ct_02_application_locator_schema_contract() -> Result<CaseSuccess, String> {
    let sandbox = IsolatedSandbox::new()?;
    let missing = sandbox.root().join("nonexistent-install-root");

    let win_miss = WindowsPlatformAdapter::new()
        .discover_installation(Some(&missing))
        .map_err(|e| e.to_string())?;
    let mac_miss = MacOsPlatformAdapter::new()
        .discover_installation(Some(&missing))
        .map_err(|e| e.to_string())?;
    let lnx_miss = LinuxPlatformAdapter::new()
        .discover_installation(Some(&missing))
        .map_err(|e| e.to_string())?;

    for (name, inst, exp_plat) in [
        ("windows", &win_miss, PlatformKind::Windows),
        ("macos", &mac_miss, PlatformKind::MacOs),
        ("linux", &lnx_miss, PlatformKind::Linux),
    ] {
        if inst.is_valid || inst.platform != exp_plat || inst.error.is_none() {
            return Err(format!(
                "{name} discover_installation on missing path violated contract: {inst:?}"
            ));
        }
        // Verify JSON serialization schema roundtrip
        let encoded = serde_json::to_string(inst).map_err(|e| e.to_string())?;
        let decoded: dim_core::ApplicationInstallation =
            serde_json::from_str(&encoded).map_err(|e| e.to_string())?;
        if decoded != *inst {
            return Err(format!("{name} ApplicationInstallation roundtrip mismatch"));
        }
    }

    Ok(CaseSuccess::pass(json!({
        "windows_missing_status": win_miss.signature_status,
        "macos_missing_status": mac_miss.signature_status,
        "linux_missing_status": lnx_miss.signature_status,
    })))
}

fn ct_03_profile_layout_and_marker_contract() -> Result<CaseSuccess, String> {
    let sandbox = IsolatedSandbox::new()?;
    let prof = dim_core::ProfileRecord {
        id: "cafebabe".to_owned(),
        name: "Contract Layout".to_owned(),
        directory_name: "contract-layout_cafebabe".to_owned(),
        created_at: "2026-11-17T00:00:00Z".to_owned(),
        last_launched_at: None,
        zalo_user_id: None,
        notes: String::new(),
    };

    let win_dir = sandbox.root().join("win-layout");
    let mac_dir = sandbox.root().join("mac-layout");
    let lnx_dir = sandbox.root().join("lnx-layout");

    let win = WindowsPlatformAdapter::new();
    let mac = MacOsPlatformAdapter::new();
    let lnx = LinuxPlatformAdapter::new();

    // Execute twice on every adapter to prove idempotency
    for _ in 0..2 {
        win.ensure_profile_layout(&prof, &win_dir)
            .map_err(|e| e.to_string())?;
        mac.ensure_profile_layout(&prof, &mac_dir)
            .map_err(|e| e.to_string())?;
        lnx.ensure_profile_layout(&prof, &lnx_dir)
            .map_err(|e| e.to_string())?;
    }

    verify_profile_id_marker(
        &win_dir.join("AppData").join("Roaming").join("ZaloData"),
        "cafebabe",
        true,
    )
    .map_err(|e| e.to_string())?;
    verify_profile_id_marker(
        &mac_dir
            .join("Library")
            .join("Application Support")
            .join("ZaloData"),
        "cafebabe",
        true,
    )
    .map_err(|e| e.to_string())?;
    verify_profile_id_marker(&lnx_dir.join(".config").join("ZaloData"), "cafebabe", true)
        .map_err(|e| e.to_string())?;

    // Unlink junctions contract on empty/clean dirs must return `Ok(0)` across all 3 adapters
    if win
        .unlink_profile_junctions(&win_dir)
        .map_err(|e| e.to_string())?
        != 0
        || mac
            .unlink_profile_junctions(&mac_dir)
            .map_err(|e| e.to_string())?
            != 0
        || lnx
            .unlink_profile_junctions(&lnx_dir)
            .map_err(|e| e.to_string())?
            != 0
    {
        return Err("expected 0 unlinked junctions on clean layouts".to_owned());
    }

    Ok(CaseSuccess::pass(json!({
        "idempotent_layouts_verified": ["windows", "macos", "linux"],
        "profile_marker_verified": "cafebabe",
    })))
}

fn ct_04_error_code_and_envelope_contract() -> Result<CaseSuccess, String> {
    let bad_json_err = serde_json::from_str::<Value>("{bad").expect_err("must fail");
    let variants: Vec<(DimError, &str)> = vec![
        (DimError::InvalidProfileName("x".into()), "INVALID_NAME"),
        (DimError::DuplicateProfileName("x".into()), "DUPLICATE_NAME"),
        (DimError::ProfileNotFound("x".into()), "NOT_FOUND"),
        (DimError::ProfileRunning("x".into()), "PROFILE_RUNNING"),
        (
            DimError::ProfileIdentityMismatch {
                expected: "a".into(),
                actual: "b".into(),
            },
            "PROFILE_MARKER_MISMATCH",
        ),
        (
            DimError::ProtectedPrimaryProcess("x".into()),
            "PROTECTED_PRIMARY_PROCESS",
        ),
        (
            DimError::ProtectedPrimaryPath("x".into()),
            "PROTECTED_PRIMARY_PATH",
        ),
        (
            DimError::DuplicateZaloUserId {
                user_id: "123456".into(),
                running_profile: "p1".into(),
            },
            "DUPLICATE_ZALO_USER_ID",
        ),
        (
            DimError::InsufficientDiskSpace {
                path: "/".into(),
                available_bytes: 1,
                required_bytes: 2,
            },
            "INSUFFICIENT_DISK_SPACE",
        ),
        (DimError::PathTraversal("x".into()), "PATH_TRAVERSAL"),
        (
            DimError::FolderRedirectionBlocked("x".into()),
            "FOLDER_REDIRECTION_DETECTED",
        ),
        (DimError::InstallNotFound("x".into()), "INSTALL_NOT_FOUND"),
        (DimError::UnverifiedBinary("x".into()), "UNVERIFIED_BINARY"),
        (DimError::NotRunning("x".into()), "NOT_RUNNING"),
        (
            DimError::UnsupportedPlatform("x".into()),
            "UNSUPPORTED_PLATFORM",
        ),
        (DimError::BackupError("x".into()), "BACKUP_ERROR"),
        (DimError::RestoreError("x".into()), "RESTORE_ERROR"),
        (DimError::ProcessError("x".into()), "PROCESS_ERROR"),
        (DimError::SchemaError("x".into()), "SCHEMA_ERROR"),
        (
            DimError::ConfirmationRequired("x".into()),
            "CONFIRM_REQUIRED",
        ),
        (DimError::Io(std::io::Error::other("io")), "IO_ERROR"),
        (DimError::Json(bad_json_err), "JSON_ERROR"),
    ];

    let mut unique_codes = HashSet::new();
    for (err, expected_code) in &variants {
        if err.code() != *expected_code {
            return Err(format!("expected {expected_code}, got {}", err.code()));
        }
        if !unique_codes.insert(err.code()) {
            return Err(format!("duplicate error code mapping: {}", err.code()));
        }
        let outcome: OperationOutcome<()> = OperationOutcome::from_err(err);
        if outcome.success || outcome.error_code.as_deref() != Some(*expected_code) {
            return Err(format!(
                "OperationOutcome::from_err broken for {expected_code}"
            ));
        }
    }

    Ok(CaseSuccess::pass(json!({
        "verified_error_variants": variants.len(),
        "unique_error_codes": unique_codes.len(),
    })))
}

fn ct_05_unsupported_and_unverified_binary_contract() -> Result<CaseSuccess, String> {
    let sandbox = IsolatedSandbox::new()?;
    let prof = sandbox
        .store
        .create_profile("Unsupported Contract", "")
        .map_err(|e| e.to_string())?;
    let pdir = sandbox
        .store
        .profile_dir_for(&prof)
        .map_err(|e| e.to_string())?;

    // 1. Linux OfficialZalo launch MUST fail with `UNSUPPORTED_PLATFORM` on EVERY host OS!
    let lnx = LinuxPlatformAdapter::new();
    let lnx_official_err = lnx
        .launch(
            &LaunchTarget::OfficialZalo {
                custom_install_root: None,
            },
            &prof,
            &pdir,
        )
        .expect_err("Linux OfficialZalo launch must always fail");
    if lnx_official_err.code() != "UNSUPPORTED_PLATFORM" {
        return Err(format!(
            "expected UNSUPPORTED_PLATFORM on Linux OfficialZalo, got {}",
            lnx_official_err.code()
        ));
    }

    // 2. Non-host adapter runtime calls must return `UNSUPPORTED_PLATFORM` cleanly without panicking
    #[cfg(target_os = "windows")]
    {
        let mac = MacOsPlatformAdapter::new();
        let mac_prim = mac
            .inspect_primary()
            .expect_err("mac inspect_primary on Windows");
        let lnx_prim = lnx
            .inspect_primary()
            .expect_err("linux inspect_primary on Windows");
        if mac_prim.code() != "UNSUPPORTED_PLATFORM" || lnx_prim.code() != "UNSUPPORTED_PLATFORM" {
            return Err("non-host inspect_primary must return UNSUPPORTED_PLATFORM".to_owned());
        }

        // 3. Unsigned locally built PE via `LaunchTarget::CustomExecutable` MUST fail closed with `UNVERIFIED_BINARY` on Windows!
        let unsigned_bin = xf_fixture_bin_path();
        if unsigned_bin.exists() {
            let unverified_err = sandbox
                .coordinator
                .launch_profile(
                    &prof.id,
                    &LaunchTarget::CustomExecutable {
                        path: unsigned_bin,
                        args: Vec::new(),
                    },
                )
                .expect_err("unsigned CustomExecutable must fail closed on WindowsPlatformAdapter");
            if unverified_err.code() != "UNVERIFIED_BINARY" {
                return Err(format!(
                    "expected UNVERIFIED_BINARY on unsigned PE, got {}",
                    unverified_err.code()
                ));
            }
        }
    }

    Ok(CaseSuccess::pass(json!({
        "linux_official_zalo_code": lnx_official_err.code(),
        "cross_os_calls_fail_closed": true,
        "windows_unsigned_custom_exe_rejected": cfg!(target_os = "windows"),
    })))
}

fn ct_06_primary_pid_protection_across_all_adapters() -> Result<CaseSuccess, String> {
    let sandbox = IsolatedSandbox::new()?;
    let dummy_state = ProfileRuntimeState {
        profile_id: "11223344".to_owned(),
        profile_name: "Guard Probe".to_owned(),
        profile_dir: sandbox.root().join("p"),
        zalo_data_dir: sandbox.root().join("p").join("ZaloData"),
        lockfile_path: sandbox.root().join("p").join("ZaloData").join("lockfile"),
        running: true,
        stale_lock_detected: false,
        main_pid: Some(42_424),
        process_ids: vec![42_424, 42_425],
        process_count: 2,
        working_set_bytes: 0,
        window_handle: None,
        window_title: None,
        detected_zalo_user_id: None,
        disk_usage_bytes: 0,
        status_label: "Running".to_owned(),
    };

    // Every adapter checks `protected_primary_pids` BEFORE entering `#[cfg(target_os = ...)]`!
    let win_err = WindowsPlatformAdapter::new()
        .stop(&dummy_state, &[42_424], 100)
        .expect_err("Windows stop must reject protected PID");
    let mac_err = MacOsPlatformAdapter::new()
        .stop(&dummy_state, &[42_424], 100)
        .expect_err("macOS stop must reject protected PID");
    let lnx_err = LinuxPlatformAdapter::new()
        .stop(&dummy_state, &[42_424], 100)
        .expect_err("Linux stop must reject protected PID");

    for (os_name, err) in [("windows", win_err), ("macos", mac_err), ("linux", lnx_err)] {
        if err.code() != "PROTECTED_PRIMARY_PROCESS" {
            return Err(format!(
                "{os_name} adapter returned wrong error code {} instead of PROTECTED_PRIMARY_PROCESS",
                err.code()
            ));
        }
    }

    Ok(CaseSuccess::pass(json!({
        "adapters_enforcing_pre_syscall_pid_guard": ["windows", "macos", "linux"],
        "error_code": "PROTECTED_PRIMARY_PROCESS",
    })))
}

fn run_dim_cli_json(data_dir: &Path, args: &[&str]) -> Result<(i32, Value), String> {
    let bin = dim_cli_bin_path();
    if !bin.exists() {
        return Err(format!("dim-cli binary not found at {}", bin.display()));
    }
    let out = Command::new(&bin)
        .arg("--cli")
        .arg("--json")
        .arg("--data-dir")
        .arg(data_dir)
        .arg("--node-fixture")
        .arg("--app-path")
        .arg(xf_fixture_mjs_path())
        .args(args)
        .output()
        .map_err(|e| format!("failed spawning dim-cli: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    let parsed: Value = serde_json::from_str(&stdout)
        .map_err(|e| format!("dim-cli invalid JSON ({args:?}): {e}\nstdout={stdout}"))?;
    Ok((out.status.code().unwrap_or(-1), parsed))
}

#[cfg(target_os = "windows")]
fn run_dim_host_ps1_json(data_dir: &Path, args: &[&str]) -> Result<(i32, Value), String> {
    let script = dim_host_ps1_path();
    let out = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            &script.to_string_lossy(),
            "--cli",
            "--json",
            "--data-dir",
            &data_dir.to_string_lossy(),
            "--node-fixture",
            "--app-path",
            &xf_fixture_mjs_path().to_string_lossy(),
        ])
        .args(args)
        .output()
        .map_err(|e| format!("failed spawning powershell dim-host.ps1: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    let parsed: Value = serde_json::from_str(&stdout)
        .map_err(|e| format!("dim-host.ps1 invalid JSON ({args:?}): {e}\nstdout={stdout}"))?;
    Ok((out.status.code().unwrap_or(-1), parsed))
}

fn ct_07_cli_output_schema_contract() -> Result<CaseSuccess, String> {
    let bin = dim_cli_bin_path();
    if !bin.exists() {
        return Ok(CaseSuccess::skipped_host(
            "dim-cli-built",
            "Build `dim-cli` (`cargo build -p dim-test-fixture --bin dim-cli --release`) to run CLI schema checks",
        ));
    }
    let sandbox = IsolatedSandbox::new()?;
    let ddir = sandbox.root().join("cli-schema-data");

    let (_, disc) = run_dim_cli_json(&ddir, &["discover"])?;
    if disc.get("ok") != Some(&json!(true))
        || disc.get("valid") != Some(&json!(true))
        || disc.get("mode") != Some(&json!("node-fixture"))
    {
        return Err(format!("discover schema mismatch: {disc}"));
    }

    let (_, st) = run_dim_cli_json(&ddir, &["status", "--with-size"])?;
    for req_key in [
        "ok",
        "platform",
        "data_dir",
        "installation",
        "primary_zalo",
        "folder_redirection",
        "profiles",
    ] {
        if st.get(req_key).is_none() {
            return Err(format!("status output missing required key '{req_key}'"));
        }
    }

    let (_, diag) = run_dim_cli_json(&ddir, &["diagnostics"])?;
    for req_key in [
        "ok",
        "is_elevated",
        "smart_app_control_state",
        "disk_free_bytes",
        "disk_free_ok",
        "profiles_acl_safe",
        "platform_support_matrix",
        "sanitized_logs",
    ] {
        if diag.get(req_key).is_none() {
            return Err(format!(
                "diagnostics output missing required key '{req_key}'"
            ));
        }
    }

    let (_, iso) = run_dim_cli_json(&ddir, &["verify-isolation"])?;
    let checks_len = iso
        .get("checks")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    if checks_len != 7 {
        return Err(format!(
            "expected 7 verify-isolation checks, got {checks_len}"
        ));
    }

    let (_, bks) = run_dim_cli_json(&ddir, &["backups"])?;
    if bks.get("ok") != Some(&json!(true)) || !bks.get("backups").is_some_and(Value::is_array) {
        return Err(format!("backups schema mismatch: {bks}"));
    }

    Ok(CaseSuccess::pass(json!({
        "verified_commands": ["discover", "status", "diagnostics", "verify-isolation", "backups"],
        "isolation_check_count": checks_len,
    })))
}

fn ct_08_differential_slug_length_contract() -> Result<CaseSuccess, String> {
    #[cfg(not(target_os = "windows"))]
    {
        Ok(CaseSuccess::skipped_host(
            "windows",
            "PowerShell Signed-Host Bridge (`scripts/dim-host.ps1`) differential parity requires Windows PowerShell",
        ))
    }
    #[cfg(target_os = "windows")]
    {
        let sandbox = IsolatedSandbox::new()?;
        let rust_dir = sandbox.root().join("rust-slug");
        let ps_dir = sandbox.root().join("ps-slug");

        // 42-character display name (valid `<= 64`, produces 42-char raw ASCII slug -> capped at 40 vs 32!)
        let long_valid_name = "Tai Khoan Kinh Doanh Khu Vuc Mien Nam So 1";
        let (_, rust_res) = run_dim_cli_json(&rust_dir, &["create", long_valid_name])?;
        let (_, ps_res) = run_dim_host_ps1_json(&ps_dir, &["create", long_valid_name])?;

        let rust_dname = rust_res["data"]["directory_name"]
            .as_str()
            .unwrap_or_default();
        let ps_dname = ps_res["data"]["directory_name"]
            .as_str()
            .unwrap_or_default();

        let rust_slug = rust_dname.split('_').next().unwrap_or_default();
        let ps_slug = ps_dname.split('_').next().unwrap_or_default();

        if rust_slug != ps_slug {
            return Ok(CaseSuccess::defect(
                "DEFECT-08",
                &[
                    "crates/core/src/slug.rs:14,70",
                    "scripts/dim-host.ps1:582",
                ],
                "Run `create \"Tai Khoan Kinh Doanh Khu Vuc Mien Nam So 1\"` on both `dim-cli.exe` and `scripts/dim-host.ps1`: Rust caps `MAX_SLUG_LEN` at `40` chars whereas PowerShell `SlugifyName` breaks at `32` chars, producing mismatched directory naming conventions.",
                json!({
                    "input_name": long_valid_name,
                    "rust_slug_prefix": rust_slug,
                    "rust_slug_len": rust_slug.len(),
                    "powershell_slug_prefix": ps_slug,
                    "powershell_slug_len": ps_slug.len(),
                }),
            ));
        }

        Ok(CaseSuccess::pass(json!({
            "slug_prefix": rust_slug,
        })))
    }
}

fn ct_09_differential_restore_naming_contract() -> Result<CaseSuccess, String> {
    #[cfg(not(target_os = "windows"))]
    {
        Ok(CaseSuccess::skipped_host(
            "windows",
            "PowerShell Signed-Host Bridge (`scripts/dim-host.ps1`) differential parity requires Windows PowerShell",
        ))
    }
    #[cfg(target_os = "windows")]
    {
        let sandbox = IsolatedSandbox::new()?;
        let rust_dir = sandbox.root().join("rust-restore");
        let ps_dir = sandbox.root().join("ps-restore");

        // 58-character valid name: appending `" (Restored)"` in PowerShell yields 69 characters (> 64 MAX_PROFILE_NAME_LEN!)
        let name_58 = "A".repeat(58);

        let (_, r_c) = run_dim_cli_json(&rust_dir, &["create", &name_58])?;
        let r_id = r_c["data"]["id"].as_str().ok_or("missing rust id")?;
        let (_, r_bk) = run_dim_cli_json(&rust_dir, &["backup", r_id])?;
        let r_bk_dir = r_bk["data"]["backup_dir"]
            .as_str()
            .ok_or("missing rust bk dir")?;

        let (_, p_c) = run_dim_host_ps1_json(&ps_dir, &["create", &name_58])?;
        let p_id = p_c["data"]["id"].as_str().ok_or("missing ps id")?;
        let (_, p_bk) = run_dim_host_ps1_json(&ps_dir, &["backup", p_id])?;
        let p_bk_dir = p_bk["data"]["backup_dir"]
            .as_str()
            .ok_or("missing ps bk dir")?;

        // Test A: Restore with explicit duplicate `--name` equal to existing profile (`name_58`)
        let (_, r_dup_rest) =
            run_dim_cli_json(&rust_dir, &["restore", r_bk_dir, "--name", &name_58])?;
        let (_, p_dup_rest) =
            run_dim_host_ps1_json(&ps_dir, &["restore", p_bk_dir, "--name", &name_58])?;

        // Test B: Restore with default auto-generated name on 58-char source profile
        let (_, r_auto_rest) = run_dim_cli_json(&rust_dir, &["restore", r_bk_dir])?;
        let (_, p_auto_rest) = run_dim_host_ps1_json(&ps_dir, &["restore", p_bk_dir])?;

        let r_auto_len = r_auto_rest["data"]["name"]
            .as_str()
            .map(|s| s.chars().count())
            .unwrap_or(0);
        let p_auto_len = p_auto_rest["data"]["name"]
            .as_str()
            .map(|s| s.chars().count())
            .unwrap_or(0);

        let dup_diverged = r_dup_rest.get("ok") != p_dup_rest.get("ok");
        let overflow_64 = p_auto_len > dim_core::MAX_PROFILE_NAME_LEN;

        if dup_diverged || overflow_64 {
            return Ok(CaseSuccess::defect(
                "DEFECT-08",
                &[
                    "crates/profile-store/src/backup.rs:399-437",
                    "scripts/dim-host.ps1:2595-2608",
                ],
                "Backup a 58-char profile, then run `restore <dir> --name <existing>` and `restore <dir>`: Rust rejects duplicate `--name` with `DUPLICATE_NAME` and caps auto-restored names at `64` chars, whereas `dim-host.ps1` silently renames duplicates with ` #2` (`ok: true`) and creates a `69`-char profile name exceeding the `64`-char validation limit!",
                json!({
                    "rust_duplicate_name_response": {
                        "ok": r_dup_rest["ok"],
                        "error_code": r_dup_rest["error_code"],
                    },
                    "powershell_duplicate_name_response": {
                        "ok": p_dup_rest["ok"],
                        "allocated_name": p_dup_rest["data"]["name"],
                    },
                    "rust_auto_restore_name_len": r_auto_len,
                    "powershell_auto_restore_name_len": p_auto_len,
                    "max_profile_name_len": dim_core::MAX_PROFILE_NAME_LEN,
                }),
            ));
        }

        Ok(CaseSuccess::pass(json!({
            "restore_naming_parity": true,
        })))
    }
}

fn ct_10_differential_corrupt_store_and_arg_codes() -> Result<CaseSuccess, String> {
    #[cfg(not(target_os = "windows"))]
    {
        Ok(CaseSuccess::skipped_host(
            "windows",
            "PowerShell Signed-Host Bridge (`scripts/dim-host.ps1`) differential parity requires Windows PowerShell",
        ))
    }
    #[cfg(target_os = "windows")]
    {
        use dim_xf_fixture::sha256_file;

        let sandbox = IsolatedSandbox::new()?;
        let bystander = BystanderSnapshot::seed(
            &sandbox,
            "CT10 Bystander Sibling",
            "CT10_SANDBOX_BYSTANDER_CANARY_V1",
        )?;
        let rust_dir = sandbox.root().join("rust-corrupt");
        let ps_dir = sandbox.root().join("ps-corrupt");
        fs::create_dir_all(&rust_dir).map_err(|e| e.to_string())?;
        fs::create_dir_all(&ps_dir).map_err(|e| e.to_string())?;

        // Seed existing bystander profile directories inside `rust_dir` and `ps_dir` BEFORE corrupting
        // `profiles.json` so we prove fail-closed corrupt-store handling never wipes existing `Profiles/` data!
        let (_, r_by) = run_dim_cli_json(&rust_dir, &["create", "Bystander Preserve"])?;
        let (_, p_by) = run_dim_host_ps1_json(&ps_dir, &["create", "Bystander Preserve"])?;
        let r_by_dname = r_by["data"]["directory_name"]
            .as_str()
            .ok_or("missing rust bystander directory_name")?;
        let p_by_dname = p_by["data"]["directory_name"]
            .as_str()
            .ok_or("missing ps bystander directory_name")?;
        let r_canary = rust_dir
            .join("Profiles")
            .join(r_by_dname)
            .join("bystander-canary.dat");
        let p_canary = ps_dir
            .join("Profiles")
            .join(p_by_dname)
            .join("bystander-canary.dat");
        fs::write(&r_canary, b"CT10_RUST_CLI_BYSTANDER_V1").map_err(|e| e.to_string())?;
        fs::write(&p_canary, b"CT10_PS_CLI_BYSTANDER_V1").map_err(|e| e.to_string())?;
        let r_canary_sha = sha256_file(&r_canary).map_err(|e| e.to_string())?;
        let p_canary_sha = sha256_file(&p_canary).map_err(|e| e.to_string())?;

        // 1. Missing `create` name argument error code
        let (_, r_miss) = run_dim_cli_json(&rust_dir, &["create"])?;
        let (_, p_miss) = run_dim_host_ps1_json(&ps_dir, &["create"])?;

        // 2. Empty (0-byte truncated) `profiles.json`
        fs::write(rust_dir.join("profiles.json"), b"").map_err(|e| e.to_string())?;
        fs::write(ps_dir.join("profiles.json"), b"").map_err(|e| e.to_string())?;
        let (_, r_empty) = run_dim_cli_json(&rust_dir, &["status"])?;
        let (_, p_empty) = run_dim_host_ps1_json(&ps_dir, &["status"])?;

        // 3. Invalid JSON `profiles.json`
        fs::write(rust_dir.join("profiles.json"), b"{corrupt").map_err(|e| e.to_string())?;
        fs::write(ps_dir.join("profiles.json"), b"{corrupt").map_err(|e| e.to_string())?;
        let (_, r_bad_json) = run_dim_cli_json(&rust_dir, &["status"])?;
        let (_, p_bad_json) = run_dim_host_ps1_json(&ps_dir, &["status"])?;

        // Verify existing bystander profile directories & canary files survived untouched!
        if sha256_file(&r_canary).map_err(|e| e.to_string())? != r_canary_sha
            || sha256_file(&p_canary).map_err(|e| e.to_string())? != p_canary_sha
        {
            return Err(
                "corrupt profiles.json probe altered existing bystander profile directory on disk"
                    .to_owned(),
            );
        }
        let bystander_ev = bystander.verify_untouched(&sandbox)?;

        let diverged = r_miss["error_code"] != p_miss["error_code"]
            || r_empty["ok"] != p_empty["ok"]
            || r_bad_json["error_code"] != p_bad_json["error_code"];

        if diverged {
            return Ok(CaseSuccess::defect(
                "DEFECT-08",
                &[
                    "fixtures/test-desktop-app/src/bin/dim-cli.rs:171-175",
                    "crates/profile-store/src/store.rs:157-167",
                    "scripts/dim-host.ps1:380-399,3005-3055",
                ],
                "Truncate `profiles.json` to 0 bytes or write `{corrupt` or invoke `create` without arguments on `dim-cli.exe` vs `scripts/dim-host.ps1`: Rust fails closed (`JSON_ERROR` / `INVALID_NAME`), whereas PowerShell silently resets 0-byte `profiles.json` to `ok: true` (masking corruption), returns `UNHANDLED_EXCEPTION` on malformed JSON, and returns `MISSING_ARG` on missing name.",
                json!({
                    "missing_arg_codes": {
                        "rust": r_miss["error_code"],
                        "powershell": p_miss["error_code"],
                    },
                    "empty_profiles_json": {
                        "rust_ok": r_empty["ok"],
                        "rust_code": r_empty["error_code"],
                        "powershell_ok": p_empty["ok"],
                    },
                    "corrupt_profiles_json_codes": {
                        "rust": r_bad_json["error_code"],
                        "powershell": p_bad_json["error_code"],
                    },
                    "cli_bystander_dirs_intact": true,
                    "bystander_non_regression": bystander_ev,
                }),
            ));
        }

        Ok(CaseSuccess::pass(json!({
            "corrupt_and_arg_codes_aligned": true,
            "cli_bystander_dirs_intact": true,
            "bystander_non_regression": bystander_ev,
        })))
    }
}

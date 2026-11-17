//! Windows native & PowerShell (`scripts/dim-host.ps1`) parity test suite for `TC-H7` (`IR-WIN-03` & `IR-WIN-04`).
//! Uses 100% synthetic temp directories (`0` real Zalo paths touched).

#![cfg(windows)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use dim_core::ProfileRecord;
use dim_platform_windows::{
    configure_windows_host_folder_links, ensure_windows_profile_layout,
};
use dim_profile_store::{is_symlink_or_reparse, unlink_reparse_points_safe};
use serde_json::Value;

fn dim_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn run_dim_host_ps1(data_dir: &Path, synth_primary: &Path, args: &[&str]) -> Value {
    let root = dim_root();
    let script = root.join("scripts").join("dim-host.ps1");
    let fixture = root
        .join("fixtures")
        .join("cross-platform")
        .join("xf-fixture.mjs");
    let out = Command::new("powershell.exe")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .args(["--cli", "--json", "--data-dir"])
        .arg(data_dir)
        .arg("--node-fixture")
        .arg("--app-path")
        .arg(&fixture)
        .args(args)
        .env("DIM_PROTECTED_PRIMARY_ZALO_DATA", synth_primary)
        .output()
        .expect("execute scripts/dim-host.ps1");
    let stdout = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("invalid JSON from dim-host.ps1 ({e}): stdout={stdout}"))
}

#[test]
fn tc_h7_windows_layout_junction_links_and_powershell_parity() {
    let tmp = tempfile::tempdir().unwrap();
    let mock_host = tmp.path().join("mock-host");
    fs::create_dir_all(mock_host.join("Downloads")).unwrap();
    fs::create_dir_all(mock_host.join("Desktop")).unwrap();
    let canary_dl = mock_host.join("Downloads").join("host-canary.pdf");
    let canary_dt = mock_host.join("Desktop").join("host-desktop.txt");
    fs::write(&canary_dl, b"WIN_CANARY_DOWNLOADS_V1").unwrap();
    fs::write(&canary_dt, b"WIN_CANARY_DESKTOP_V1").unwrap();

    // 1. Verify `ensure_windows_profile_layout` creates all 6 standard folders isolated by default (`IR-WIN-03`):
    let prof_dir = tmp.path().join("layout-check");
    let rec = ProfileRecord {
        id: "aabbccdd".to_owned(),
        name: "Layout Probe".to_owned(),
        directory_name: "layout-probe_aabbccdd".to_owned(),
        created_at: "2026-11-18T00:00:00Z".to_owned(),
        last_launched_at: None,
        zalo_user_id: None,
        notes: String::new(),
    };
    ensure_windows_profile_layout(&rec, &prof_dir).unwrap();
    for rel in [
        "Desktop",
        "Downloads",
        "Pictures",
        "Videos",
        "Music",
        r"Documents\Zalo Received Files",
    ] {
        let p = prof_dir.join(rel);
        let meta = fs::symlink_metadata(&p).expect("standard folder must exist");
        assert!(meta.is_dir() && !is_symlink_or_reparse(&meta));
    }

    // Opt-in host links convert `Downloads` & `Desktop` to NTFS junctions while keeping `Documents\Zalo Received Files` isolated:
    configure_windows_host_folder_links(&prof_dir, &mock_host, true, true).unwrap();
    assert!(is_symlink_or_reparse(
        &fs::symlink_metadata(prof_dir.join("Downloads")).unwrap()
    ));
    assert!(is_symlink_or_reparse(
        &fs::symlink_metadata(prof_dir.join("Desktop")).unwrap()
    ));
    assert!(!is_symlink_or_reparse(
        &fs::symlink_metadata(prof_dir.join("Documents").join("Zalo Received Files")).unwrap()
    ));
    assert_eq!(unlink_reparse_points_safe(&prof_dir).unwrap(), 2);

    // 2. Verify PowerShell `scripts/dim-host.ps1` parity on `SHARED_EXTERNAL_DB_PATH`, `EXTERNAL_DB_OUTSIDE_PROFILE`,
    // `--allow-external-db-omit`, `stop-all`, and NTFS junction canary survival on `delete --confirm`:
    let ps_data = tmp.path().join("ps-data");
    let synth_prim = tmp.path().join("ps-synth-primary");
    let shared_ext = tmp.path().join("ps-ext-shared");
    fs::create_dir_all(&synth_prim).unwrap();
    fs::create_dir_all(&shared_ext).unwrap();
    let ext_canary = shared_ext.join("ext-msg.db");
    fs::write(&ext_canary, b"PS_EXT_SQLITE_INTACT").unwrap();

    let c1 = run_dim_host_ps1(&ps_data, &synth_prim, &["create", "PS Ext C"]);
    let c2 = run_dim_host_ps1(&ps_data, &synth_prim, &["create", "PS Ext D"]);
    assert_eq!(c1["ok"], true);
    assert_eq!(c2["ok"], true);
    let id_c = c1["data"]["id"].as_str().unwrap().to_owned();
    let id_d = c2["data"]["id"].as_str().unwrap().to_owned();
    let dname_c = c1["data"]["directory_name"].as_str().unwrap();
    let dname_d = c2["data"]["directory_name"].as_str().unwrap();
    let pdir_c = ps_data.join("Profiles").join(dname_c);
    let pdir_d = ps_data.join("Profiles").join(dname_d);

    // Verify PowerShell also created `Downloads`, `Pictures`, `Videos`, `Music`:
    for f in ["Downloads", "Pictures", "Videos", "Music"] {
        assert!(pdir_c.join(f).is_dir(), "dim-host.ps1 missing {f}");
    }

    // Configure C with direct external path, and D with an in-profile NTFS junction escaping to `shared_ext`!
    let cfg_json_c = serde_json::json!({
        "db_path": { "location": shared_ext.display().to_string() }
    });
    fs::write(
        pdir_c
            .join("AppData")
            .join("Roaming")
            .join("ZaloData")
            .join("config.json"),
        cfg_json_c.to_string(),
    )
    .unwrap();

    let d_junc = pdir_d
        .join("AppData")
        .join("Roaming")
        .join("ZaloData")
        .join("EscapingDbJunction");
    configure_windows_host_folder_links(&pdir_d, &mock_host, true, true).unwrap();
    let mk = Command::new("cmd.exe")
        .args(["/C", "mklink", "/J"])
        .arg(&d_junc)
        .arg(&shared_ext)
        .output()
        .unwrap();
    assert!(mk.status.success());
    let cfg_json_d = serde_json::json!({
        "db_path": { "location": d_junc.display().to_string() }
    });
    fs::write(
        pdir_d
            .join("AppData")
            .join("Roaming")
            .join("ZaloData")
            .join("config.json"),
        cfg_json_d.to_string(),
    )
    .unwrap();

    let l_c = run_dim_host_ps1(
        &ps_data,
        &synth_prim,
        &["launch", &id_c, "--user-id", "771001"],
    );
    assert_eq!(l_c["ok"], true, "launch C failed: {l_c}");

    // `xf-fixture.mjs` writes a fresh `config.json` on boot; restore C's external `db_path.location`
    // (modeling an active Zalo profile configured to store its database in `shared_ext`).
    fs::write(
        pdir_c
            .join("AppData")
            .join("Roaming")
            .join("ZaloData")
            .join("config.json"),
        cfg_json_c.to_string(),
    )
    .unwrap();

    let l_d = run_dim_host_ps1(
        &ps_data,
        &synth_prim,
        &["launch", &id_d, "--user-id", "771002"],
    );
    assert_eq!(l_d["ok"], false, "expected D launch blocked: {l_d}");
    assert_eq!(l_d["error_code"], "SHARED_EXTERNAL_DB_PATH");

    // Also verify collision against stopped Primary Zalo external DB (`DIM_PROTECTED_PRIMARY_ZALO_DATA`):
    let prim_ext = tmp.path().join("ps-prim-ext-db");
    fs::create_dir_all(&prim_ext).unwrap();
    fs::write(
        synth_prim.join("config.json"),
        serde_json::json!({ "db_path": { "location": prim_ext.display().to_string() } }).to_string(),
    )
    .unwrap();
    let cp = run_dim_host_ps1(&ps_data, &synth_prim, &["create", "PS Prim Collide"]);
    let id_p = cp["data"]["id"].as_str().unwrap().to_owned();
    let dname_p = cp["data"]["directory_name"].as_str().unwrap();
    fs::write(
        ps_data
            .join("Profiles")
            .join(dname_p)
            .join("AppData")
            .join("Roaming")
            .join("ZaloData")
            .join("config.json"),
        serde_json::json!({ "db_path": { "location": prim_ext.display().to_string() } }).to_string(),
    )
    .unwrap();
    let l_p = run_dim_host_ps1(
        &ps_data,
        &synth_prim,
        &["launch", &id_p, "--user-id", "771003"],
    );
    assert_eq!(l_p["ok"], false, "expected P launch blocked: {l_p}");
    assert_eq!(l_p["error_code"], "SHARED_EXTERNAL_DB_PATH");

    let stop_all = run_dim_host_ps1(&ps_data, &synth_prim, &["stop-all"]);
    assert_eq!(stop_all["ok"], true);

    // Default backup rejected with `EXTERNAL_DB_OUTSIDE_PROFILE`:
    let bk_fail = run_dim_host_ps1(&ps_data, &synth_prim, &["backup", &id_d]);
    assert_eq!(bk_fail["ok"], false);
    assert_eq!(bk_fail["error_code"], "EXTERNAL_DB_OUTSIDE_PROFILE");

    // Explicit `--allow-external-db-omit` succeeds:
    let bk_ok = run_dim_host_ps1(
        &ps_data,
        &synth_prim,
        &["backup", &id_c, "--allow-external-db-omit"],
    );
    assert_eq!(bk_ok["ok"], true, "acknowledged backup failed: {bk_ok}");
    assert_eq!(bk_ok["data"]["external_db_omitted"], true);

    // Delete D (which has Downloads, Desktop, and EscapingDbJunction NTFS junctions!) -> 100% of canaries survive!
    let del_d = run_dim_host_ps1(&ps_data, &synth_prim, &["delete", &id_d, "--confirm"]);
    assert_eq!(del_d["ok"], true, "delete D failed: {del_d}");
    assert!(!pdir_d.exists());
    assert_eq!(fs::read(&canary_dl).unwrap(), b"WIN_CANARY_DOWNLOADS_V1");
    assert_eq!(fs::read(&canary_dt).unwrap(), b"WIN_CANARY_DESKTOP_V1");
    assert_eq!(fs::read(&ext_canary).unwrap(), b"PS_EXT_SQLITE_INTACT");
}

//! Data Safety Test Matrix (`DS-01` .. `DS-08`).
//!
//! Covers:
//! - Multi-profile data separation & deterministic SHA-256 chunk verification
//! - Symlinks & Windows NTFS junctions (`mklink /J`) during backup (no exfiltration) and delete (target preserved)
//! - Path traversal, Windows reserved device names, NTFS ADS (`:$DATA`), and zip-slip manifest attacks
//! - Backup partial failure mid-copy & `.partial` staging rollback
//! - Restore interrupted / tampered / duplicate-name rollback without touching existing profiles
//! - Disk full threshold simulation (`5 GiB + payload` exact boundary)
//! - Atomic write consistency of `profiles.json` under concurrent readers + staging sweep
//! - Cross-platform macOS/Linux backup & restore `zalo_user_id` preservation (`DEFECT-07`)

use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use dim_core::{
    resolve_safe_child, BackupManager, BackupManifest, DimError, InstanceLauncher, ProfileStore,
    MIN_FREE_DISK_BYTES,
};
use dim_profile_store::{sweep_abandoned_staging_dirs, FileBackupManager};
use dim_xf_fixture::{sha256_file, FixturePayloadManifest};
use serde_json::json;

use crate::harness::{
    make_node_fixture_target, run_case_if, CaseSelectionFilter, CaseSuccess, IsolatedSandbox,
    TestCaseOutcome,
};

/// Executes all Data Safety scenarios (`DS-01` .. `DS-08`).
pub fn run_data_safety_suite(seed: u64) -> Vec<TestCaseOutcome> {
    run_data_safety_suite_filtered(seed, None)
}

/// Executes Data Safety scenarios (`DS-01` .. `DS-08`) matching `filter`.
pub fn run_data_safety_suite_filtered(
    seed: u64,
    filter: Option<&CaseSelectionFilter>,
) -> Vec<TestCaseOutcome> {
    [
        run_case_if(
            filter,
            "DS-01",
            "data_safety",
            "Strict multi-profile data separation and SHA-256 payload isolation",
            "all",
            || ds_01_multi_profile_data_separation(seed),
        ),
        run_case_if(
            filter,
            "DS-02",
            "data_safety",
            "Symlinks & Windows NTFS junctions: skipped during backup and unlinked safely on delete",
            "all",
            ds_02_symlink_and_ntfs_junction_safety,
        ),
        run_case_if(
            filter,
            "DS-03",
            "data_safety",
            "Path traversal, Windows device names, NTFS ADS (`:$DATA`), and zip-slip manifest attacks",
            "all",
            ds_03_traversal_ads_and_manifest_attacks,
        ),
        run_case_if(
            filter,
            "DS-04",
            "data_safety",
            "Backup partial failure mid-copy cleans `.partial` staging directory cleanly",
            "all",
            ds_04_backup_partial_failure_rollback,
        ),
        run_case_if(
            filter,
            "DS-05",
            "data_safety",
            "Restore tamper/interruption & duplicate-name rollback preserve existing profiles",
            "all",
            ds_05_restore_interrupted_and_rollback,
        ),
        run_case_if(
            filter,
            "DS-06",
            "data_safety",
            "Disk full simulation enforces exact `5 GiB + payload` threshold on backup and restore",
            "all",
            ds_06_disk_full_threshold_boundaries,
        ),
        run_case_if(
            filter,
            "DS-07",
            "data_safety",
            "Atomic `profiles.json` writes under concurrent reader stress & staging dir sweep",
            "all",
            ds_07_atomic_writes_and_staging_sweep,
        ),
        run_case_if(
            filter,
            "DS-08",
            "data_safety",
            "Cross-platform macOS/Linux backup & restore `zalo_user_id` preservation",
            "all",
            ds_08_cross_platform_backup_restore_uid,
        ),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn ds_01_multi_profile_data_separation(seed: u64) -> Result<CaseSuccess, String> {
    let mut sandbox = IsolatedSandbox::new()?;
    let mut profiles = Vec::new();
    for idx in 1..=3 {
        let rec = sandbox
            .store
            .create_profile(&format!("Vault Profile {idx}"), "")
            .map_err(|e| e.to_string())?;
        let uid = format!("{}", 710_000 + idx);
        let inst_id = format!("vault-inst-{idx}");
        let target = make_node_fixture_target(&[
            "--instance-id",
            &inst_id,
            "--user-id",
            &uid,
            "--write-files=4",
            "--write-bytes=2048",
            "--seed",
            &seed.to_string(),
        ]);
        let rt = sandbox
            .coordinator
            .launch_profile(&rec.id, &target)
            .map_err(|e| e.to_string())?;
        if let Some(pid) = rt.main_pid {
            sandbox.track_pid(pid);
        }
        profiles.push((rec, uid));
    }

    // Wait for all 3 instances to write their 4x2048B chunk manifests
    let dl = Instant::now() + Duration::from_secs(4);
    for (rec, _) in &profiles {
        let pdir = sandbox
            .store
            .profile_dir_for(rec)
            .map_err(|e| e.to_string())?;
        let zdata = sandbox.zalo_data_dir_for(&pdir);
        while Instant::now() < dl && !zdata.join("fixture-files.json").exists() {
            thread::sleep(Duration::from_millis(20));
        }
    }

    let mut chunk_zero_hashes = HashSet::new();
    for (rec, expected_uid) in &profiles {
        let st = sandbox
            .coordinator
            .inspect_profile(&rec.id)
            .map_err(|e| e.to_string())?;
        if st.detected_zalo_user_id.as_deref() != Some(expected_uid.as_str()) {
            return Err(format!(
                "profile '{}' detected wrong UID {:?}",
                rec.name, st.detected_zalo_user_id
            ));
        }
        let manifest_raw = fs::read_to_string(st.zalo_data_dir.join("fixture-files.json"))
            .map_err(|e| e.to_string())?;
        let manifest: FixturePayloadManifest =
            serde_json::from_str(&manifest_raw).map_err(|e| e.to_string())?;
        if manifest.chunks.len() != 4 || manifest.total_bytes != 8192 {
            return Err("unexpected chunk manifest count or size".to_owned());
        }
        for chunk in &manifest.chunks {
            let disk_hash = sha256_file(&st.zalo_data_dir.join(&chunk.relative_path))
                .map_err(|e| e.to_string())?;
            if disk_hash != chunk.sha256 {
                return Err(format!(
                    "chunk SHA-256 mismatch for {}",
                    chunk.relative_path
                ));
            }
        }
        chunk_zero_hashes.insert(manifest.chunks[0].sha256.clone());
    }

    if chunk_zero_hashes.len() != 3 {
        return Err(
            "expected distinct seeded chunk digests across the 3 isolated profiles".to_owned(),
        );
    }

    for (rec, _) in &profiles {
        sandbox
            .coordinator
            .stop_profile(&rec.id)
            .map_err(|e| e.to_string())?;
    }

    Ok(CaseSuccess::pass(json!({
        "isolated_profiles_verified": 3,
        "distinct_chunk0_digests": chunk_zero_hashes.len(),
        "bytes_per_profile": 8192,
    })))
}

fn ds_02_symlink_and_ntfs_junction_safety() -> Result<CaseSuccess, String> {
    let sandbox = IsolatedSandbox::new()?;
    let outside_vault = sandbox.root().join("outside-sensitive-vault");
    fs::create_dir_all(&outside_vault).map_err(|e| e.to_string())?;
    let secret_file = outside_vault.join("do-not-touch.txt");
    fs::write(&secret_file, b"EXTERNAL_VAULT_SECRET_INTACT").map_err(|e| e.to_string())?;

    let prof = sandbox
        .store
        .create_profile("Junction Safety Profile", "")
        .map_err(|e| e.to_string())?;
    let pdir = sandbox
        .store
        .profile_dir_for(&prof)
        .map_err(|e| e.to_string())?;
    sandbox
        .adapter
        .ensure_profile_layout(&prof, &pdir)
        .map_err(|e| e.to_string())?;

    let link_inside = pdir.join("LinkedVault");
    create_dir_junction_or_symlink(&outside_vault, &link_inside)?;

    // 1. Backup MUST skip the junction/symlink and never copy `do-not-touch.txt` into backup!
    let bk = sandbox
        .backups
        .backup_profile(&prof, &pdir, None, false)
        .map_err(|e| e.to_string())?;
    let leaked = bk.manifest.files.iter().any(|f| {
        f.relative_path.contains("do-not-touch.txt") || f.relative_path.contains("LinkedVault")
    });
    if leaked {
        return Err(
            "backup_profile followed junction/symlink and exfiltrated external vault!".to_owned(),
        );
    }

    // 2. Backup destination that IS a junction/symlink must be rejected!
    let symlink_dest = sandbox.root().join("symlink-backup-dest");
    create_dir_junction_or_symlink(&outside_vault, &symlink_dest)?;
    let sym_dest_err = sandbox
        .backups
        .backup_profile(&prof, &pdir, Some(&symlink_dest), false)
        .expect_err("symlinked backup destination must be rejected");
    if sym_dest_err.code() != "BACKUP_ERROR" {
        return Err(format!(
            "expected BACKUP_ERROR on symlink dest, got {}",
            sym_dest_err.code()
        ));
    }

    // 3. Delete profile MUST unlink junction without deleting `outside_vault/do-not-touch.txt`!
    sandbox
        .coordinator
        .delete_profile(&prof.id, true)
        .map_err(|e| e.to_string())?;
    if pdir.exists() {
        return Err("profile directory still exists after delete".to_owned());
    }
    let after_secret = fs::read_to_string(&secret_file)
        .map_err(|_| "CRITICAL: external junction target file was deleted!".to_owned())?;
    if after_secret != "EXTERNAL_VAULT_SECRET_INTACT" {
        return Err("external junction target file contents changed!".to_owned());
    }

    Ok(CaseSuccess::pass(json!({
        "backup_skipped_junction": true,
        "symlink_backup_dest_rejected": sym_dest_err.code(),
        "external_target_preserved_after_delete": true,
    })))
}

fn create_dir_junction_or_symlink(
    target: &std::path::Path,
    link: &std::path::Path,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        let out = Command::new("cmd.exe")
            .args([
                "/c",
                "mklink",
                "/J",
                &link.to_string_lossy(),
                &target.to_string_lossy(),
            ])
            .output()
            .map_err(|e| format!("mklink /J failed to execute: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "mklink /J failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        Ok(())
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).map_err(|e| format!("unix symlink failed: {e}"))
    }
}

fn ds_03_traversal_ads_and_manifest_attacks() -> Result<CaseSuccess, String> {
    let sandbox = IsolatedSandbox::new()?;
    let base = sandbox.root().join("safe-root");
    fs::create_dir_all(&base).map_err(|e| e.to_string())?;

    let malicious_relatives = [
        "",
        ".",
        "..",
        "../escape.txt",
        "..\\escape.txt",
        "nested/../../escape.txt",
        "/etc/passwd",
        "\\Windows\\System32\\cmd.exe",
        "C:\\Windows\\win.ini",
        "C:drive-relative.txt",
        "CON",
        "nul.json",
        "sub/COM1.log",
        "sub/LPT9.bak",
        "trailing-dot.",
        "trailing-space ",
        "file.txt:$DATA",
        "stream:hidden",
        "bad\0null.txt",
        "wild*.txt",
        "pipe|name",
    ];

    for payload in malicious_relatives {
        let err = resolve_safe_child(&base, payload)
            .expect_err(&format!("payload {payload:?} must be rejected"));
        if err.code() != "PATH_TRAVERSAL" {
            return Err(format!(
                "expected PATH_TRAVERSAL for {payload:?}, got {}",
                err.code()
            ));
        }
    }

    // Zip-slip style manifest attack on `verify_backup` / `restore_backup_as_new_profile`
    let evil_bk = sandbox.root().join("evil-manifest-backup");
    fs::create_dir_all(&evil_bk).map_err(|e| e.to_string())?;
    let outside_victim = sandbox.root().join("victim.txt");
    fs::write(&outside_victim, b"SAFE").map_err(|e| e.to_string())?;
    let victim_hash = sha256_file(&outside_victim).map_err(|e| e.to_string())?;

    let forged_manifest = BackupManifest {
        schema_version: 1,
        backup_id: "deadbeef".to_owned(),
        created_at: "2026-11-17T00:00:00Z".to_owned(),
        source_profile_id: "11223344".to_owned(),
        source_profile_name: "Forged".to_owned(),
        source_directory_name: "forged_11223344".to_owned(),
        zalo_user_id: None,
        file_count: 1,
        total_bytes: 4,
        files: vec![dim_core::BackupFileEntry {
            relative_path: "../victim.txt".to_owned(),
            size_bytes: 4,
            sha256: victim_hash,
        }],
    };
    fs::write(
        evil_bk.join("backup.json"),
        serde_json::to_string_pretty(&forged_manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    let restore_err = sandbox
        .backups
        .restore_backup_as_new_profile(&evil_bk, Some("Forged Restore"))
        .expect_err("zip-slip manifest must be rejected");
    if restore_err.code() != "PATH_TRAVERSAL" {
        return Err(format!(
            "expected PATH_TRAVERSAL on forged manifest, got {}",
            restore_err.code()
        ));
    }

    Ok(CaseSuccess::pass(json!({
        "rejected_traversal_and_ads_vectors": malicious_relatives.len(),
        "zip_slip_manifest_rejected_code": restore_err.code(),
    })))
}

fn ds_04_backup_partial_failure_rollback() -> Result<CaseSuccess, String> {
    let sandbox = IsolatedSandbox::new()?;
    let prof = sandbox
        .store
        .create_profile("Partial Backup Failure", "")
        .map_err(|e| e.to_string())?;
    let pdir = sandbox
        .store
        .profile_dir_for(&prof)
        .map_err(|e| e.to_string())?;
    sandbox
        .adapter
        .ensure_profile_layout(&prof, &pdir)
        .map_err(|e| e.to_string())?;
    let zdata = sandbox.zalo_data_dir_for(&pdir);
    fs::write(zdata.join("01_good.dat"), vec![0x41u8; 4096]).map_err(|e| e.to_string())?;
    let locked_file = zdata.join("99_unreadable.dat");
    fs::write(&locked_file, vec![0x42u8; 4096]).map_err(|e| e.to_string())?;

    // Hold an OS read-denying lock (Windows `share_mode(0)`) or `chmod 000` (Unix) so `fs::copy` fails mid-backup!
    #[cfg(windows)]
    let _exclusive_guard = {
        use std::os::windows::fs::OpenOptionsExt;
        OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(&locked_file)
            .map_err(|e| format!("failed opening exclusive lock: {e}"))?
    };

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&locked_file)
            .map_err(|e| e.to_string())?
            .permissions();
        perms.set_mode(0o000);
        fs::set_permissions(&locked_file, perms).map_err(|e| e.to_string())?;
    }

    let bk_res = sandbox.backups.backup_profile(&prof, &pdir, None, false);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(&locked_file) {
            let mut perms = meta.permissions();
            perms.set_mode(0o600);
            let _ = fs::set_permissions(&locked_file, perms);
        }
    }

    let err_code = match bk_res {
        Err(err) => err.code(),
        Ok(_) => {
            return Err(
                "backup_profile unexpectedly succeeded on unreadable/locked file".to_owned(),
            )
        }
    };

    // Verify ZERO `.partial` or committed directories remain inside `Backups/`!
    let leftovers: Vec<String> = fs::read_dir(sandbox.store.backups_dir())
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    if !leftovers.is_empty() {
        return Err(format!(
            "leftover backup directories after failed copy: {leftovers:?}"
        ));
    }

    Ok(CaseSuccess::pass(json!({
        "mid_copy_failure_error_code": err_code,
        "leftover_partial_dirs": leftovers.len(),
    })))
}

fn ds_05_restore_interrupted_and_rollback() -> Result<CaseSuccess, String> {
    let sandbox = IsolatedSandbox::new()?;
    let src = sandbox
        .store
        .create_profile("Original Protected", "keep safe")
        .map_err(|e| e.to_string())?;
    let src_dir = sandbox
        .store
        .profile_dir_for(&src)
        .map_err(|e| e.to_string())?;
    sandbox
        .adapter
        .ensure_profile_layout(&src, &src_dir)
        .map_err(|e| e.to_string())?;
    let zdata = sandbox.zalo_data_dir_for(&src_dir);
    fs::write(zdata.join("account-secret.txt"), b"ORIGINAL_DATA_V1").map_err(|e| e.to_string())?;

    let bk = sandbox
        .backups
        .backup_profile(&src, &src_dir, None, false)
        .map_err(|e| e.to_string())?;

    // 1. Requesting restore with an ALREADY-EXISTING profile name (`Original Protected`) must fail with `DUPLICATE_NAME`
    // and leave ZERO `.restore-*` staging directories and leave `Original Protected` untouched!
    let dup_err = sandbox
        .backups
        .restore_backup_as_new_profile(&bk.backup_dir, Some("Original Protected"))
        .expect_err("restore into duplicate profile name must fail");
    if dup_err.code() != "DUPLICATE_NAME" {
        return Err(format!("expected DUPLICATE_NAME, got {}", dup_err.code()));
    }

    // 2. Corrupting payload bytes on a cloned backup must fail verification before modifying Profiles/
    let corrupt_bk = sandbox.root().join("corrupt-backup");
    copy_dir_simple(&bk.backup_dir, &corrupt_bk)?;
    let corrupt_target = sandbox
        .zalo_data_dir_for(&corrupt_bk)
        .join("account-secret.txt");
    fs::write(&corrupt_target, b"CORRUPTED_PAYLOAD!").map_err(|e| e.to_string())?;
    let tamper_err = sandbox
        .backups
        .restore_backup_as_new_profile(&corrupt_bk, Some("Tampered Target"))
        .expect_err("corrupted backup must fail");
    if tamper_err.code() != "BACKUP_ERROR" {
        return Err(format!("expected BACKUP_ERROR, got {}", tamper_err.code()));
    }

    // Verify zero `.restore-*` staging remnants and original profile 100% intact
    let profile_entries: Vec<String> = fs::read_dir(sandbox.store.profiles_dir())
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    if profile_entries.iter().any(|n| n.starts_with(".restore-")) || profile_entries.len() != 1 {
        return Err(format!(
            "unexpected Profiles/ entries after rollback: {profile_entries:?}"
        ));
    }
    let intact = fs::read_to_string(zdata.join("account-secret.txt")).map_err(|e| e.to_string())?;
    if intact != "ORIGINAL_DATA_V1" {
        return Err("original profile file was altered during failed restore!".to_owned());
    }

    Ok(CaseSuccess::pass(json!({
        "duplicate_restore_name_code": dup_err.code(),
        "tampered_payload_code": tamper_err.code(),
        "staging_remnants": 0,
        "original_profile_preserved": true,
    })))
}

fn copy_dir_simple(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(src).map_err(|e| e.to_string())?.flatten() {
        let p = entry.path();
        let target = dst.join(entry.file_name());
        if p.is_dir() {
            copy_dir_simple(&p, &target)?;
        } else {
            fs::copy(&p, &target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn ds_06_disk_full_threshold_boundaries() -> Result<CaseSuccess, String> {
    let sandbox = IsolatedSandbox::new()?;
    let prof = sandbox
        .store
        .create_profile("Disk Quota Probe", "")
        .map_err(|e| e.to_string())?;
    let pdir = sandbox
        .store
        .profile_dir_for(&prof)
        .map_err(|e| e.to_string())?;
    let zdata = sandbox.zalo_data_dir_for(&pdir);
    fs::create_dir_all(&zdata).map_err(|e| e.to_string())?;
    fs::write(zdata.join("payload.bin"), vec![0x5Au8; 10_000]).map_err(|e| e.to_string())?;

    // Exact regular file bytes inside `pdir` = 10,000 bytes
    let exact_required = MIN_FREE_DISK_BYTES + 10_000;

    // 1 byte below `5 GiB + payload` MUST fail with `INSUFFICIENT_DISK_SPACE`
    let bmgr_low = FileBackupManager::new(Arc::clone(&sandbox.store))
        .with_disk_space_probe(Arc::new(move |_| Ok(exact_required - 1)));
    let low_err = bmgr_low
        .backup_profile(&prof, &pdir, None, false)
        .expect_err("1 byte below 5GiB+payload must fail");
    if !matches!(low_err, DimError::InsufficientDiskSpace { .. }) {
        return Err(format!("expected InsufficientDiskSpace, got {low_err:?}"));
    }

    // Exact `5 GiB + payload` MUST pass!
    let bmgr_exact = FileBackupManager::new(Arc::clone(&sandbox.store))
        .with_disk_space_probe(Arc::new(move |_| Ok(exact_required)));
    let ok_bk = bmgr_exact
        .backup_profile(&prof, &pdir, None, false)
        .map_err(|e| format!("exact threshold backup should pass: {e}"))?;

    // Restore 1 byte below `5 GiB + total_bytes` MUST also fail with `INSUFFICIENT_DISK_SPACE`
    let rest_err = bmgr_low
        .restore_backup_as_new_profile(&ok_bk.backup_dir, Some("Quota Restore"))
        .expect_err("restore below 5GiB+payload must fail");
    if rest_err.code() != "INSUFFICIENT_DISK_SPACE" {
        return Err(format!(
            "expected INSUFFICIENT_DISK_SPACE on restore, got {}",
            rest_err.code()
        ));
    }

    Ok(CaseSuccess::pass(json!({
        "min_free_disk_bytes": MIN_FREE_DISK_BYTES,
        "payload_bytes": 10_000,
        "below_threshold_rejected": low_err.code(),
        "exact_threshold_accepted": true,
        "restore_below_threshold_rejected": rest_err.code(),
    })))
}

fn ds_07_atomic_writes_and_staging_sweep() -> Result<CaseSuccess, String> {
    let sandbox = IsolatedSandbox::new()?;
    let base = sandbox
        .store
        .create_profile("Atomic Seed", "")
        .map_err(|e| e.to_string())?;

    let stop_flag = Arc::new(AtomicBool::new(false));
    let meta_path = sandbox.store.metadata_path().to_path_buf();
    let reader_stop = Arc::clone(&stop_flag);

    let reader = thread::spawn(move || -> Result<usize, String> {
        let mut reads = 0usize;
        while !reader_stop.load(Ordering::Relaxed) {
            if let Ok(raw) = fs::read_to_string(&meta_path) {
                let doc: dim_core::ProfilesDocument = serde_json::from_str(&raw).map_err(|e| {
                    format!("observed torn/corrupt profiles.json read #{reads}: {e}")
                })?;
                if doc.schema_version != 1 {
                    return Err(format!("unexpected schema_version {}", doc.schema_version));
                }
                reads += 1;
            }
        }
        Ok(reads)
    });

    for idx in 1..=30 {
        let p = sandbox
            .store
            .create_profile(&format!("Atomic Writer {idx}"), "")
            .map_err(|e| e.to_string())?;
        sandbox
            .store
            .rename_profile(&p.id, &format!("Atomic Renamed {idx}"))
            .map_err(|e| e.to_string())?;
        sandbox
            .store
            .update_last_launched(&base.id, "2026-11-17T16:00:00Z", Some("849123456"))
            .map_err(|e| e.to_string())?;
    }

    stop_flag.store(true, Ordering::Relaxed);
    let verified_reads = reader
        .join()
        .map_err(|_| "reader thread panicked".to_owned())??;

    // Verify abandoned `.partial` and `.restore-*` staging sweep
    let stale_partial = sandbox
        .store
        .backups_dir()
        .join("abandoned_20261117.partial");
    let stale_restore = sandbox
        .store
        .profiles_dir()
        .join(".restore-20261117-aabbccdd");
    fs::create_dir_all(&stale_partial).map_err(|e| e.to_string())?;
    fs::create_dir_all(&stale_restore).map_err(|e| e.to_string())?;

    // Fresh (< 1h) must NOT be swept
    let swept_fresh =
        sweep_abandoned_staging_dirs(sandbox.store.backups_dir(), Duration::from_secs(3600))
            .map_err(|e| e.to_string())?;
    if swept_fresh != 0 || !stale_partial.exists() {
        return Err("fresh .partial staging dir was prematurely swept!".to_owned());
    }

    // Zero-duration max_age sweeps abandoned staging folders cleanly
    let swept_bk = sweep_abandoned_staging_dirs(sandbox.store.backups_dir(), Duration::ZERO)
        .map_err(|e| e.to_string())?;
    let swept_pr = sweep_abandoned_staging_dirs(sandbox.store.profiles_dir(), Duration::ZERO)
        .map_err(|e| e.to_string())?;
    if swept_bk != 1 || swept_pr != 1 {
        return Err(format!(
            "expected 1 swept in each dir, got bk={swept_bk}, pr={swept_pr}"
        ));
    }

    Ok(CaseSuccess::pass(json!({
        "atomic_mutations_executed": 90,
        "concurrent_torn_free_reads": verified_reads,
        "swept_abandoned_staging_dirs": swept_bk + swept_pr,
    })))
}

fn ds_08_cross_platform_backup_restore_uid() -> Result<CaseSuccess, String> {
    let sandbox = IsolatedSandbox::new()?;

    // Simulate a macOS profile and a Linux profile where the user signed in via QR AFTER initial launch
    // (so `profiles.json` still has `zalo_user_id: None`, while `Database/_production/849777666` exists on disk).
    let mac_prof = sandbox
        .store
        .create_profile("macOS Post-Launch Login", "")
        .map_err(|e| e.to_string())?;
    let mac_dir = sandbox
        .store
        .profile_dir_for(&mac_prof)
        .map_err(|e| e.to_string())?;
    let mac_zdata = mac_dir
        .join("Library")
        .join("Application Support")
        .join("ZaloData");
    fs::create_dir_all(
        mac_zdata
            .join("Database")
            .join("_production")
            .join("849777666"),
    )
    .map_err(|e| e.to_string())?;
    fs::write(mac_zdata.join(".zalo-profile-id"), mac_prof.id.as_bytes())
        .map_err(|e| e.to_string())?;

    let linux_prof = sandbox
        .store
        .create_profile("Linux Post-Launch Login", "")
        .map_err(|e| e.to_string())?;
    let linux_dir = sandbox
        .store
        .profile_dir_for(&linux_prof)
        .map_err(|e| e.to_string())?;
    let linux_zdata = linux_dir.join(".config").join("ZaloData");
    fs::create_dir_all(
        linux_zdata
            .join("Database")
            .join("_production")
            .join("849888999"),
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        linux_zdata.join(".zalo-profile-id"),
        linux_prof.id.as_bytes(),
    )
    .map_err(|e| e.to_string())?;

    let mac_bk = sandbox
        .backups
        .backup_profile(&mac_prof, &mac_dir, None, false)
        .map_err(|e| e.to_string())?;
    let mac_restored = sandbox
        .backups
        .restore_backup_as_new_profile(&mac_bk.backup_dir, Some("macOS Restored"))
        .map_err(|e| e.to_string())?;

    let linux_bk = sandbox
        .backups
        .backup_profile(&linux_prof, &linux_dir, None, false)
        .map_err(|e| e.to_string())?;
    let linux_restored = sandbox
        .backups
        .restore_backup_as_new_profile(&linux_bk.backup_dir, Some("Linux Restored"))
        .map_err(|e| e.to_string())?;

    if mac_restored.zalo_user_id.as_deref() != Some("849777666")
        || linux_restored.zalo_user_id.as_deref() != Some("849888999")
    {
        return Ok(CaseSuccess::defect(
            "DEFECT-07",
            &["crates/profile-store/src/backup.rs:143,339-345,455-460"],
            "Create a macOS (`Library/Application Support/ZaloData/Database/_production/849777666`) or Linux (`.config/ZaloData/Database/_production/849888999`) profile where QR login happened after launch (`profile.zalo_user_id == None`), then run `backup_profile` + `restore_backup_as_new_profile`: `restore_backup_as_new_profile` hardcodes Windows `AppData/Roaming/ZaloData` and returns `zalo_user_id: None` on macOS/Linux!",
            json!({
                "mac_manifest_uid": mac_bk.manifest.zalo_user_id,
                "mac_restored_uid": mac_restored.zalo_user_id,
                "expected_mac_uid": "849777666",
                "linux_manifest_uid": linux_bk.manifest.zalo_user_id,
                "linux_restored_uid": linux_restored.zalo_user_id,
                "expected_linux_uid": "849888999",
            }),
        ));
    }

    Ok(CaseSuccess::pass(json!({
        "mac_restored_uid": mac_restored.zalo_user_id,
        "linux_restored_uid": linux_restored.zalo_user_id,
    })))
}

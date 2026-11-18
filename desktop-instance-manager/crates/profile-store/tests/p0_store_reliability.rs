//! P0 reliability, concurrency, multi-process advisory locking, fail-closed metadata corruption,
//! atomic commit rollback, and verified destructive deletion tests for `dim-profile-store`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use dim_core::{
    build_runtime_lease, remove_runtime_lease, write_runtime_lease, AdvisoryFileLock, ProfileStore,
    RuntimeOwnershipState, VerifiedDeletionState, PROFILES_LOCK_FILE, PROFILE_OP_LOCK_FILE,
    RUNTIME_LEASE_FILE,
};
use dim_profile_store::{unlink_reparse_points_safe, JsonProfileStore};

#[test]
fn concurrent_threads_create_rename_and_delete_without_lost_updates() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("store-thread-race");
    let seed = JsonProfileStore::new(data_dir.clone()).unwrap();
    seed.ensure_initialized().unwrap();

    let bystander = seed.create_profile("Permanent Bystander", "keep").unwrap();

    // 1. Concurrent create across 6 independent store instances (5 profiles each = 30).
    let create_barrier = Arc::new(Barrier::new(6));
    let mut create_handles = Vec::with_capacity(6);
    for t in 0..6usize {
        let dir = data_dir.clone();
        let b = Arc::clone(&create_barrier);
        create_handles.push(thread::spawn(move || {
            let store = JsonProfileStore::new(dir).unwrap();
            let mut ids = Vec::with_capacity(5);
            for r in 0..5usize {
                b.wait();
                let rec = store
                    .create_profile(&format!("Thread {t} Create {r}"), "race")
                    .unwrap();
                ids.push(rec.id);
            }
            ids
        }));
    }

    let mut created_groups: Vec<Vec<String>> = Vec::with_capacity(6);
    for h in create_handles {
        created_groups.push(h.join().unwrap());
    }
    assert_eq!(seed.list_profiles().unwrap().len(), 1 + 30);

    // 2. Concurrent rename + concurrent delete across 6 threads simultaneously:
    // Each thread renames 3 of its profiles and deletes 2 of its profiles.
    let mixed_barrier = Arc::new(Barrier::new(6));
    let mut mixed_handles = Vec::with_capacity(6);
    for (t, ids) in created_groups.into_iter().enumerate() {
        let dir = data_dir.clone();
        let b = Arc::clone(&mixed_barrier);
        mixed_handles.push(thread::spawn(move || {
            let store = JsonProfileStore::new(dir).unwrap();
            b.wait();
            for (idx, id) in ids.iter().enumerate() {
                if idx < 3 {
                    let updated = store
                        .rename_profile(id, &format!("Thread {t} Renamed {idx}"))
                        .unwrap();
                    assert_eq!(updated.name, format!("Thread {t} Renamed {idx}"));
                } else {
                    store.delete_profile(id, true).unwrap();
                }
            }
        }));
    }
    for h in mixed_handles {
        h.join().unwrap();
    }

    let final_profiles = seed.list_profiles().unwrap();
    assert_eq!(final_profiles.len(), 1 + 18);
    assert!(final_profiles.iter().any(|p| p.id == bystander.id));

    let unique_ids: HashSet<&str> = final_profiles.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(unique_ids.len(), final_profiles.len());

    // Zero leftover `.tmp` files in `data_dir`.
    let leftover_tmps: Vec<_> = fs::read_dir(&data_dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(
        leftover_tmps.is_empty(),
        "unexpected leftover tmp files: {leftover_tmps:?}"
    );
}

#[test]
fn multi_process_subprocesses_contend_on_profiles_lock_safely() {
    if env::var("DIM_STORE_SUBPROC_MODE").as_deref() == Ok("worker") {
        let dir = PathBuf::from(env::var("DIM_STORE_SUBPROC_DIR").unwrap());
        let worker_idx: usize = env::var("DIM_STORE_SUBPROC_IDX").unwrap().parse().unwrap();
        let store = JsonProfileStore::new(dir).unwrap();
        let mut created = Vec::new();
        for i in 0..4usize {
            let rec = store
                .create_profile(&format!("Subproc W{worker_idx} P{i}"), "child")
                .unwrap();
            created.push(rec.id);
        }
        store
            .rename_profile(&created[0], &format!("Subproc W{worker_idx} P0 Renamed"))
            .unwrap();
        store
            .rename_profile(&created[1], &format!("Subproc W{worker_idx} P1 Renamed"))
            .unwrap();
        store.delete_profile(&created[3], true).unwrap();
        return;
    }

    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("store-multiprocess-race");
    let store = JsonProfileStore::new(data_dir.clone()).unwrap();
    store.ensure_initialized().unwrap();
    let bystander = store
        .create_profile("Multiprocess Bystander", "keep")
        .unwrap();

    let current_exe = env::current_exe().unwrap();
    let mut children = Vec::with_capacity(4);
    for w in 0..4usize {
        let child = Command::new(&current_exe)
            .args([
                "--exact",
                "multi_process_subprocesses_contend_on_profiles_lock_safely",
                "--nocapture",
            ])
            .env("DIM_STORE_SUBPROC_MODE", "worker")
            .env("DIM_STORE_SUBPROC_DIR", &data_dir)
            .env("DIM_STORE_SUBPROC_IDX", w.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        children.push(child);
    }

    for child in children {
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "subprocess worker failed: stdout={} stderr={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    // 1 bystander + 4 workers * (4 created - 1 deleted) = 13 profiles.
    let profiles = store.list_profiles().unwrap();
    assert_eq!(profiles.len(), 13);
    assert!(profiles.iter().any(|p| p.id == bystander.id));
    for w in 0..4usize {
        assert!(profiles
            .iter()
            .any(|p| p.name == format!("Subproc W{w} P0 Renamed")));
        assert!(profiles
            .iter()
            .any(|p| p.name == format!("Subproc W{w} P1 Renamed")));
        assert!(profiles
            .iter()
            .any(|p| p.name == format!("Subproc W{w} P2")));
        assert!(!profiles
            .iter()
            .any(|p| p.name == format!("Subproc W{w} P3")));
    }
}

#[test]
fn corrupted_metadata_fails_closed_preserving_original_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("corrupt-metadata");
    let store = JsonProfileStore::new(data_dir.clone()).unwrap();
    let bystander = store.create_profile("Corrupt Bystander", "").unwrap();
    let bystander_dir = store.profile_dir_for(&bystander).unwrap();
    let canary = bystander_dir.join("canary.txt");
    fs::write(&canary, b"PRESERVE_ME").unwrap();

    let metadata_path = store.metadata_path().to_path_buf();

    let corrupt_cases: &[(&[u8], &str)] = &[
        (b"", "JSON_ERROR"),
        (b"   \r\n\t  ", "JSON_ERROR"),
        (&[0xFF, 0xFE, 0x00, 0x81], "JSON_ERROR"),
        (b"{\"schema_version\": 1, \"profiles\": [", "JSON_ERROR"),
        (b"{\"profiles\": []}", "SCHEMA_ERROR"),
        (
            b"{\"schema_version\": 99, \"profiles\": []}",
            "SCHEMA_ERROR",
        ),
    ];

    for (payload, expected_code) in corrupt_cases {
        fs::write(&metadata_path, payload).unwrap();

        assert_eq!(
            store.ensure_initialized().unwrap_err().code(),
            *expected_code
        );
        assert_eq!(store.list_profiles().unwrap_err().code(), *expected_code);
        assert_eq!(
            store
                .create_profile("Blocked Write", "")
                .unwrap_err()
                .code(),
            *expected_code
        );

        // Original corrupted bytes MUST remain 100% untouched on disk (never reset to `[]`).
        let disk_bytes = fs::read(&metadata_path).unwrap();
        assert_eq!(&disk_bytes, payload);
        assert_eq!(fs::read(&canary).unwrap(), b"PRESERVE_ME");
    }
}

#[test]
fn interrupted_commit_rollback_stale_tmp_cleanup_and_lock_contention_timeout() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("rollback-and-lock");
    let store = JsonProfileStore::new(data_dir.clone())
        .unwrap()
        .with_lock_timeout(Duration::from_millis(120));
    let p1 = store.create_profile("Initial Valid", "").unwrap();
    let original_bytes = fs::read(store.metadata_path()).unwrap();

    // 1. Stale `.tmp` remnants from a prior crash must be ignored on read and cleaned on write.
    let stale_tmp_a = data_dir.join("profiles.json.tmp");
    let stale_tmp_b = data_dir.join("profiles.json.999999.crashed.tmp");
    fs::write(&stale_tmp_a, b"{truncated").unwrap();
    fs::write(&stale_tmp_b, b"").unwrap();
    assert_eq!(store.list_profiles().unwrap().len(), 1);

    let p1_renamed = store.rename_profile(&p1.id, "Initial Valid V2").unwrap();
    assert_eq!(p1_renamed.name, "Initial Valid V2");
    assert!(!stale_tmp_a.exists());
    assert!(!stale_tmp_b.exists());
    let committed_bytes = fs::read(store.metadata_path()).unwrap();
    assert_ne!(committed_bytes, original_bytes);

    // 2. Simulated atomic commit failure leaves `profiles.json` byte-for-byte intact and cleans `.tmp`.
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let share_read_guard = fs::OpenOptions::new()
            .read(true)
            .share_mode(0x0000_0001) // FILE_SHARE_READ only (blocks MoveFileExW replace)
            .open(store.metadata_path())
            .unwrap();
        let err = store.create_profile("Should Rollback", "").unwrap_err();
        drop(share_read_guard);
        assert_eq!(err.code(), "IO_ERROR");
        assert_eq!(fs::read(store.metadata_path()).unwrap(), committed_bytes);
        assert_eq!(store.list_profiles().unwrap().len(), 1);
    }

    // 3. Cross-process advisory lock contention times out cleanly without touching `profiles.json`.
    let held_lock = AdvisoryFileLock::acquire_exclusive(
        &data_dir.join(PROFILES_LOCK_FILE),
        Duration::from_secs(1),
    )
    .unwrap();
    let contention_err = store.create_profile("Blocked By Lock", "").unwrap_err();
    assert_eq!(contention_err.code(), "IO_ERROR");
    assert_eq!(fs::read(store.metadata_path()).unwrap(), committed_bytes);
    drop(held_lock);

    // Immediately succeeds once lock is released.
    let p2 = store.create_profile("After Lock Release", "").unwrap();
    assert_eq!(store.list_profiles().unwrap().len(), 2);
    assert_eq!(p2.name, "After Lock Release");
}

#[test]
fn verified_deletion_state_and_forged_stopped_defense_in_depth() {
    if env::var("DIM_STORE_SUBPROC_MODE").as_deref() == Ok("sleep_holder") {
        let mut buf = [0u8; 1];
        let _ = std::io::stdin().read(&mut buf);
        return;
    }

    let tmp = tempfile::tempdir().unwrap();
    let store = JsonProfileStore::new(tmp.path().join("delete-guards")).unwrap();
    let prof = store.create_profile("Protected Target", "").unwrap();
    let pdir = store.profile_dir_for(&prof).unwrap();

    // 1. Explicit non-stopped `VerifiedDeletionState` variants must all reject with `PROFILE_RUNNING`.
    for non_stopped in [
        RuntimeOwnershipState::Starting,
        RuntimeOwnershipState::Running,
        RuntimeOwnershipState::Stopping,
        RuntimeOwnershipState::Unknown,
    ] {
        let err = store
            .delete_profile_verified(
                &prof.id,
                true,
                VerifiedDeletionState::NotSafeToDelete(non_stopped),
            )
            .unwrap_err();
        assert_eq!(err.code(), "PROFILE_RUNNING");
        assert!(pdir.exists());
    }

    // 2. Forged `ConfirmedStopped` while `.dim-profile.lock` is held must reject.
    let op_lock = AdvisoryFileLock::acquire_exclusive(
        &pdir.join(PROFILE_OP_LOCK_FILE),
        Duration::from_secs(1),
    )
    .unwrap();
    let lock_err = store
        .delete_profile_verified(&prof.id, true, VerifiedDeletionState::ConfirmedStopped)
        .unwrap_err();
    assert_eq!(lock_err.code(), "PROFILE_RUNNING");
    drop(op_lock);

    // 3. Forged `ConfirmedStopped` while an indeterminate/corrupted `.runtime-lease.json` exists must reject.
    fs::write(pdir.join(RUNTIME_LEASE_FILE), b"{corrupt_lease").unwrap();
    let indet_err = store
        .delete_profile_with_guard(
            &prof.id,
            true,
            VerifiedDeletionState::ConfirmedStopped,
            &unlink_reparse_points_safe,
        )
        .unwrap_err();
    assert_eq!(indet_err.code(), "PROFILE_RUNNING");
    fs::remove_file(pdir.join(RUNTIME_LEASE_FILE)).unwrap();

    // 4. Forged `ConfirmedStopped` while a verified LIVE `.runtime-lease.json` exists must reject.
    let current_exe = env::current_exe().unwrap();
    let mut sleeper = Command::new(&current_exe)
        .args([
            "--exact",
            "verified_deletion_state_and_forged_stopped_defense_in_depth",
            "--nocapture",
        ])
        .env("DIM_STORE_SUBPROC_MODE", "sleep_holder")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let lease = build_runtime_lease(sleeper.id(), &current_exe, &prof.id, None);
    write_runtime_lease(&pdir, &lease).unwrap();

    let live_lease_err = store.delete_profile(&prof.id, true).unwrap_err();
    assert_eq!(live_lease_err.code(), "PROFILE_RUNNING");
    assert!(pdir.exists());

    // Terminate sleeper, clean lease, and confirm `ConfirmedStopped` deletion succeeds cleanly.
    drop(sleeper.stdin.take());
    let _ = sleeper.wait();
    assert!(remove_runtime_lease(&pdir) >= 1);

    store
        .delete_profile_verified(&prof.id, true, VerifiedDeletionState::ConfirmedStopped)
        .unwrap();
    assert!(!pdir.exists());
    assert!(store.list_profiles().unwrap().is_empty());
}

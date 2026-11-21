#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::engine::SyncEngine;
use crate::queue::{QueueStatus, VaultQueue};
use crate::store::{Destination, VaultConfig, VaultConfigStore};
use crate::transport::MockTransport;
use crate::watch::ZaloReceivedWatcher;

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

#[test]
fn watcher_detects_new_file_after_two_size_changes_and_hashes_it() {
    let tmp = tempfile::tempdir().unwrap();
    let received = tmp.path().join("Profiles/work_123/Zalo Received Files");
    fs::create_dir_all(&received).unwrap();
    let mut watcher =
        ZaloReceivedWatcher::with_debounce(&received, std::time::Duration::from_millis(5)).unwrap();
    let file = received.join("document.txt");
    fs::write(&file, b"a").unwrap();
    assert!(watcher.poll().unwrap().is_empty());
    fs::write(&file, b"a growing file").unwrap();
    assert!(watcher.poll().unwrap().is_empty());
    let found = watcher.poll().unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].path, file);
    assert_eq!(found[0].size, 14);
    assert_eq!(found[0].sha256, digest(b"a growing file"));
    assert!(watcher.poll().unwrap().is_empty());
}

#[test]
fn successful_upload_logs_completion_and_duplicate_digest_is_not_enqueued() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("received.bin");
    fs::write(&file, b"archive").unwrap();
    let hash = digest(b"archive");
    let db = tmp.path().join("vault.sqlite");
    let mut queue = VaultQueue::open(&db).unwrap();
    assert!(queue.enqueue("profile-a", &file, &hash, 7).unwrap());
    let transport = MockTransport::default();
    let mut engine = SyncEngine::new("profile-a", queue, transport);
    let completed = engine.process_one(now()).unwrap().unwrap();
    assert_eq!(
        engine.queue().get(completed.id).unwrap().unwrap().status,
        QueueStatus::Done
    );
    assert_eq!(engine.queue().log(10).unwrap().len(), 1);
    assert_eq!(engine.transport().uploads().unwrap().len(), 1);
    assert!(!engine
        .queue_mut()
        .enqueue("profile-a", &file, &hash, 7)
        .unwrap());
    assert!(file.exists(), "the Zalo original must remain untouched");
}

#[test]
fn failed_upload_can_be_retried_after_backoff_and_success_is_logged() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("received.bin");
    fs::write(&file, b"contents").unwrap();
    let mut queue = VaultQueue::open(tmp.path().join("vault.sqlite")).unwrap();
    queue
        .enqueue("profile-a", &file, &digest(b"contents"), 8)
        .unwrap();
    let transport = MockTransport::failing(1);
    let mut engine = SyncEngine::new("profile-a", queue, transport);
    let start = now();
    let attempted = engine.process_one(start).unwrap().unwrap();
    let failed = engine.queue().get(attempted.id).unwrap().unwrap();
    assert_eq!(failed.status, QueueStatus::Failed);
    assert_eq!(failed.attempts, 1);
    assert!(failed.last_error.is_some());
    assert_eq!(engine.queue().log(10).unwrap()[0].outcome, "failed");

    let succeeded = engine.process_one(start + 3).unwrap().unwrap();
    let completed = engine.queue().get(succeeded.id).unwrap().unwrap();
    assert_eq!(completed.status, QueueStatus::Done);
    assert_eq!(completed.attempts, 2);
    assert_eq!(engine.queue().log(10).unwrap()[0].outcome, "done");
    assert_eq!(engine.transport().uploads().unwrap().len(), 2);
}

#[test]
fn reopening_database_keeps_uploading_state_until_explicit_recovery() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("vault.sqlite");
    let file = tmp.path().join("in-flight.bin");
    fs::write(&file, b"in flight").unwrap();
    let id;
    {
        let mut queue = VaultQueue::open(&db).unwrap();
        queue
            .enqueue("profile-a", &file, &digest(b"in flight"), 9)
            .unwrap();
        let claimed = queue.claim_next("profile-a", now()).unwrap().unwrap();
        assert_eq!(claimed.status, QueueStatus::Uploading);
        assert_eq!(claimed.attempts, 1);
        id = claimed.id;
    }
    let queue = VaultQueue::open(&db).unwrap();
    // Plain reopen must not disturb an in-flight upload: UI status/log reads open the same DB.
    assert_eq!(
        queue.get(id).unwrap().unwrap().status,
        QueueStatus::Uploading
    );
    // Explicit, once-per-process crash recovery requeues the interrupted item.
    assert_eq!(queue.recover_interrupted().unwrap(), 1);
    let recovered = queue.get(id).unwrap().unwrap();
    assert_eq!(recovered.status, QueueStatus::Pending);
    assert_eq!(recovered.attempts, 1);
}

#[test]
fn profile_configuration_defaults_to_saved_messages() {
    let tmp = tempfile::tempdir().unwrap();
    let store = VaultConfigStore::new(tmp.path()).unwrap();
    let config = VaultConfig {
        profile_id: "profile-a".into(),
        telegram_session_dir: tmp.path().join("TelegramVault/sessions/profile-a"),
        destination: Destination::default(),
        enabled: true,
    };
    store.save(&config).unwrap();
    assert_eq!(store.load("profile-a").unwrap(), Some(config));
}

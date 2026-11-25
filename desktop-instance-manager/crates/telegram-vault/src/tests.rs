#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::engine::SyncEngine;
use crate::queue::{QueueStatus, VaultQueue};
use crate::store::{Destination, Transport, TransportKind, VaultConfig, VaultConfigStore};
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
    // Without mark_emitted, the watcher does not drop an unacknowledged file.
    assert_eq!(watcher.poll().unwrap().len(), 1);
    watcher.mark_emitted(&found[0].path);
    assert!(watcher.poll().unwrap().is_empty());

    // Same-size replacement with updated mtime must still re-stabilize and re-emit.
    std::thread::sleep(std::time::Duration::from_millis(20));
    fs::write(&file, b"replaced bytes").unwrap();
    assert_eq!(b"replaced bytes".len(), 14);
    assert!(watcher.poll().unwrap().is_empty());
    let replaced = watcher.poll().unwrap();
    assert_eq!(replaced.len(), 1);
    assert_eq!(replaced[0].sha256, digest(b"replaced bytes"));
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
fn profile_configuration_round_trips_telegram_credentials() {
    let tmp = tempfile::tempdir().unwrap();
    let store = VaultConfigStore::new(tmp.path()).unwrap();
    let config = VaultConfig {
        profile_id: "profile-a".into(),
        telegram_session_dir: tmp.path().join("TelegramVault/sessions/profile-a"),
        destination: Destination::default(),
        enabled: true,
        transport: Transport::Mtproto,
        telegram_api_id: Some(12345),
        telegram_api_hash: Some("test-api-hash".into()),
        telegram_bot_token: None,
        telegram_chat_id: None,
        telegram_bot_username: None,
        telegram_bot_pair_nonce: None,
    };
    store.save(&config).unwrap();
    assert_eq!(store.load("profile-a").unwrap(), Some(config));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(store.root().join("profile-a.json"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "vault config with credentials must be owner-only"
        );
    }
}

#[test]
fn bot_configuration_round_trips_token_and_chat() {
    let tmp = tempfile::tempdir().unwrap();
    let store = VaultConfigStore::new(tmp.path()).unwrap();
    let config = VaultConfig {
        profile_id: "profile-b".into(),
        telegram_session_dir: tmp.path().join("TelegramVault/sessions/profile-b"),
        destination: Destination::default(),
        enabled: true,
        transport: Transport::Bot,
        telegram_api_id: None,
        telegram_api_hash: None,
        telegram_bot_token: Some("123456789:AAE_abc-DEFxyz".into()),
        telegram_chat_id: Some(987_654_321),
        telegram_bot_username: Some("vault_bot".into()),
        telegram_bot_pair_nonce: None,
    };
    store.save(&config).unwrap();
    assert_eq!(store.load("profile-b").unwrap(), Some(config.clone()));
    let serialized = std::fs::read_to_string(store.root().join("profile-b.json")).unwrap();
    assert!(serialized.contains("\"transport\": \"bot\""));
    assert!(serialized.contains("\"telegram_bot_username\": \"vault_bot\""));
    // Empty optional fields must stay out of the document.
    let minimal = VaultConfig {
        telegram_bot_token: None,
        telegram_chat_id: None,
        telegram_bot_username: None,
        telegram_bot_pair_nonce: None,
        ..config.clone()
    };
    let json = serde_json::to_string(&minimal).unwrap();
    assert!(!json.contains("telegram_bot_token"));
    assert!(!json.contains("telegram_chat_id"));
    assert!(!json.contains("telegram_bot_username"));
    assert!(!json.contains("telegram_bot_pair_nonce"));
}

#[test]
fn legacy_config_without_transport_field_resolves_mtproto() {
    let tmp = tempfile::tempdir().unwrap();
    let store = VaultConfigStore::new(tmp.path()).unwrap();
    let legacy = serde_json::json!({
        "profile_id": "profile-c",
        "telegram_session_dir": tmp.path().join("sessions/profile-c"),
        "destination": "saved_messages",
        "enabled": true,
        "telegram_api_id": 12345,
        "telegram_api_hash": "test-api-hash",
    });
    std::fs::write(
        store.root().join("profile-c.json"),
        serde_json::to_vec_pretty(&legacy).unwrap(),
    )
    .unwrap();
    let config = store.load("profile-c").unwrap().expect("legacy config");
    assert_eq!(config.transport, Transport::Mtproto);
    assert_eq!(config.transport_kind().unwrap(), TransportKind::Mtproto);
}

#[test]
fn transport_kind_rejects_mixed_and_missing_credentials() {
    let mixed = VaultConfig {
        profile_id: "profile-d".into(),
        telegram_session_dir: "sessions".into(),
        destination: Destination::default(),
        enabled: true,
        transport: Transport::Bot,
        telegram_api_id: Some(12345),
        telegram_api_hash: Some("hash".into()),
        telegram_bot_token: Some("123:secret".into()),
        telegram_chat_id: Some(1),
        telegram_bot_username: None,
        telegram_bot_pair_nonce: None,
    };
    let error = mixed.transport_kind().unwrap_err();
    assert!(
        matches!(
            &error,
            crate::VaultError::InvalidRecord(message)
                if message == "mixed or missing vault transport"
        ),
        "expected InvalidRecord, got {error:?}"
    );
    let neither = VaultConfig {
        telegram_api_id: None,
        telegram_api_hash: None,
        telegram_bot_token: None,
        ..mixed.clone()
    };
    assert!(neither.transport_kind().is_err());
    let whitespace_token = VaultConfig {
        telegram_api_id: None,
        telegram_api_hash: None,
        telegram_bot_token: Some("   ".into()),
        ..mixed.clone()
    };
    assert!(whitespace_token.transport_kind().is_err());
    let mismatched_mtproto_tag = VaultConfig {
        transport: Transport::Mtproto,
        telegram_api_id: None,
        telegram_api_hash: None,
        telegram_bot_token: Some("123:secret".into()),
        ..mixed.clone()
    };
    assert!(mismatched_mtproto_tag.transport_kind().is_err());
    let mismatched_bot_tag = VaultConfig {
        transport: Transport::Bot,
        telegram_api_id: Some(12345),
        telegram_api_hash: Some("hash".into()),
        telegram_bot_token: None,
        telegram_chat_id: None,
        ..mixed
    };
    assert!(mismatched_bot_tag.transport_kind().is_err());
}

#[test]
fn config_store_rejects_invalid_transport_before_writing() {
    let tmp = tempfile::tempdir().unwrap();
    let store = VaultConfigStore::new(tmp.path()).unwrap();
    let invalid = VaultConfig {
        profile_id: "invalid-profile".into(),
        telegram_session_dir: tmp.path().join("sessions"),
        destination: Destination::default(),
        enabled: true,
        transport: Transport::Bot,
        telegram_api_id: Some(12345),
        telegram_api_hash: Some("hash".into()),
        telegram_bot_token: Some("123:secret".into()),
        telegram_chat_id: None,
        telegram_bot_username: Some("bot".into()),
        telegram_bot_pair_nonce: None,
    };
    assert!(matches!(
        store.save(&invalid),
        Err(crate::VaultError::InvalidRecord(message))
            if message == "mixed or missing vault transport"
    ));
    assert!(!store.root().join("invalid-profile.json").exists());
}

#[test]
fn config_store_list_skips_corrupt_and_mismatched_entries() {
    let tmp = tempfile::tempdir().unwrap();
    let store = VaultConfigStore::new(tmp.path()).unwrap();
    let valid = VaultConfig {
        profile_id: "profile-ok".into(),
        telegram_session_dir: tmp.path().join("sessions/profile-ok"),
        destination: Destination::default(),
        enabled: true,
        transport: Transport::Bot,
        telegram_api_id: None,
        telegram_api_hash: None,
        telegram_bot_token: Some("123456:secret".into()),
        telegram_chat_id: Some(42),
        telegram_bot_username: Some("ok_bot".into()),
        telegram_bot_pair_nonce: None,
    };
    store.save(&valid).unwrap();
    std::fs::write(store.root().join("corrupt.json"), b"{not valid json").unwrap();
    std::fs::write(
        store.root().join("wrong-name.json"),
        serde_json::to_vec(&VaultConfig {
            profile_id: "other-id".into(),
            ..valid.clone()
        })
        .unwrap(),
    )
    .unwrap();

    let listed = store.list().unwrap();
    assert_eq!(listed, vec![valid]);
}

#[test]
fn queue_normalizes_digests_filters_by_profile_and_backs_off_renamed_failures() {
    let tmp = tempfile::tempdir().unwrap();
    let mut queue = VaultQueue::open(tmp.path().join("vault.sqlite")).unwrap();
    let upper = "A".repeat(64);
    let lower = "a".repeat(64);
    let path1 = tmp.path().join("first.bin");
    let path2 = tmp.path().join("renamed.bin");
    assert!(queue.enqueue("profile-a", &path1, &upper, 10).unwrap());
    assert!(!queue.enqueue("profile-a", &path2, &lower, 10).unwrap());
    let items = queue.list().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].sha256, lower);
    assert_eq!(items[0].path, path2);

    let claimed = queue.claim_next("profile-a", now()).unwrap().unwrap();
    queue.mark_failed(&claimed, "network error").unwrap();
    let failed_ts = queue.log_for_profile("profile-a", 1).unwrap()[0].ts;
    // Refreshing path after failure must still honor retry backoff via sha256.
    let path3 = tmp.path().join("moved-again.bin");
    assert!(!queue.enqueue("profile-a", &path3, &lower, 10).unwrap());
    let immediate = queue.claim_next("profile-a", failed_ts).unwrap();
    assert!(
        immediate.is_none(),
        "renamed failed item must respect exponential backoff before delay expires"
    );

    // Other profile log entries must not truncate profile-a's per-profile log or last_done_ts.
    let done_digest = "b".repeat(64);
    assert!(queue.enqueue("profile-a", &path1, &done_digest, 5).unwrap());
    // Claim using `failed_ts` so the failed item stays in backoff even across a second boundary.
    let claimed_a = queue.claim_next("profile-a", failed_ts).unwrap().unwrap();
    assert_eq!(claimed_a.sha256, done_digest);
    queue.mark_done(&claimed_a).unwrap();

    for idx in 0..30 {
        let d = format!("{idx:064x}");
        let p = tmp.path().join(format!("b_{idx}.bin"));
        queue.enqueue("profile-b", &p, &d, 5).unwrap();
        let c = queue.claim_next("profile-b", now()).unwrap().unwrap();
        queue.mark_done(&c).unwrap();
    }
    let logs_a = queue.log_for_profile("profile-a", 10).unwrap();
    assert_eq!(logs_a.len(), 2);
    assert!(queue.last_done_ts("profile-a").unwrap().is_some());
    let counts_a = queue.profile_counts("profile-a").unwrap();
    assert_eq!(counts_a.done, 1);
    assert_eq!(counts_a.failed, 1);
    // Attempt 1 backoff is 2^0 = 1s, so at failed_ts + 1 the renamed item is claimable again.
    let retried = queue
        .claim_next("profile-a", failed_ts + 1)
        .unwrap()
        .expect("renamed failed item must become claimable once 1s backoff expires");
    assert_eq!(retried.sha256, lower);
}

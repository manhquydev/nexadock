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
    };
    store.save(&config).unwrap();
    assert_eq!(store.load("profile-b").unwrap(), Some(config.clone()));
    let serialized = std::fs::read_to_string(store.root().join("profile-b.json")).unwrap();
    assert!(serialized.contains("\"transport\": \"bot\""));
    // Empty optional fields must stay out of the document.
    let minimal = VaultConfig {
        telegram_bot_token: None,
        telegram_chat_id: None,
        ..config.clone()
    };
    let json = serde_json::to_string(&minimal).unwrap();
    assert!(!json.contains("telegram_bot_token"));
    assert!(!json.contains("telegram_chat_id"));
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
        ..mixed
    };
    assert!(whitespace_token.transport_kind().is_err());
}

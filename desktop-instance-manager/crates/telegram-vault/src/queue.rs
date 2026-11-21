//! SQLite-backed at-least-once upload queue and synchronization log.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

use crate::{error::VaultError, Result};

/// Durable status of a file waiting for Telegram upload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QueueStatus {
    /// Ready to be uploaded.
    Pending,
    /// Currently being uploaded; reset to pending after an unclean process exit.
    Uploading,
    /// Successfully uploaded.
    Done,
    /// The most recent upload attempt failed and can be retried.
    Failed,
}

impl QueueStatus {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "pending" => Ok(Self::Pending),
            "uploading" => Ok(Self::Uploading),
            "done" => Ok(Self::Done),
            "failed" => Ok(Self::Failed),
            other => Err(VaultError::InvalidRecord(format!(
                "unknown queue status {other}"
            ))),
        }
    }
}

/// One durable file upload work item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueItem {
    /// Queue row identifier.
    pub id: i64,
    /// Associated Zalo profile identifier.
    pub profile_id: String,
    /// Absolute or profile-relative source file path.
    pub path: PathBuf,
    /// Lowercase hexadecimal SHA-256 digest.
    pub sha256: String,
    /// Source file size in bytes.
    pub size: u64,
    /// Current durable state.
    pub status: QueueStatus,
    /// Number of upload attempts started.
    pub attempts: u32,
    /// Most recent transport failure, if any.
    pub last_error: Option<String>,
    /// Unix timestamp in seconds when this work item was created.
    pub created_at: i64,
    /// Unix timestamp in seconds when upload completed.
    pub uploaded_at: Option<i64>,
}

/// One append-only result row from the upload engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultLogEntry {
    /// Unix timestamp in seconds when the result was recorded.
    pub ts: i64,
    /// Associated Zalo profile identifier.
    pub profile_id: String,
    /// Original file path.
    pub file: PathBuf,
    /// Lowercase hexadecimal SHA-256 digest.
    pub sha256: String,
    /// Source file size in bytes.
    pub size: u64,
    /// Outcome label, normally `done` or `failed`.
    pub outcome: String,
    /// Transport error for failed outcomes.
    pub error: Option<String>,
}

/// Durable upload queue stored in a SQLite database.
pub struct VaultQueue {
    connection: Connection,
}

impl VaultQueue {
    /// Opens or creates the queue database without touching existing row states.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent).map_err(|error| VaultError::io(parent, error))?;
        }
        let connection = Connection::open(path)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS vault_queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                profile_id TEXT NOT NULL,
                path TEXT NOT NULL,
                sha256 TEXT NOT NULL,
                size INTEGER NOT NULL CHECK(size >= 0),
                status TEXT NOT NULL CHECK(status IN ('pending','uploading','done','failed')),
                attempts INTEGER NOT NULL DEFAULT 0 CHECK(attempts >= 0),
                last_error TEXT,
                created_at INTEGER NOT NULL,
                uploaded_at INTEGER
            );
            CREATE INDEX IF NOT EXISTS vault_queue_ready_idx
                ON vault_queue(status, created_at, id);
            CREATE INDEX IF NOT EXISTS vault_queue_digest_idx
                ON vault_queue(profile_id, sha256, status);
            CREATE TABLE IF NOT EXISTS vault_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ts INTEGER NOT NULL,
                profile_id TEXT NOT NULL,
                file TEXT NOT NULL,
                sha256 TEXT NOT NULL,
                size INTEGER NOT NULL,
                outcome TEXT NOT NULL,
                error TEXT
            );
            CREATE INDEX IF NOT EXISTS vault_log_profile_ts_idx
                ON vault_log(profile_id, ts, id);",
        )?;
        Ok(Self { connection })
    }
    /// Requeues items left in `uploading` state by an unclean process exit.
    ///
    /// Crash recovery is a deliberate, caller-invoked step (once per process, before any
    /// consumer starts): `open()` itself must stay read-safe so status/log reads from the UI
    /// never disturb an upload already claimed by a running consumer.
    pub fn recover_interrupted(&self) -> Result<u64> {
        let changed = self.connection.execute(
            "UPDATE vault_queue SET status='pending' WHERE status='uploading'",
            [],
        )?;
        u64::try_from(changed).map_err(|_| VaultError::IntegerRange)
    }

    /// Adds a newly discovered file unless the same profile and digest already
    /// has a queue row. Deduplication is intentionally digest-based: a second
    /// copy of identical content under a different file name is not uploaded
    /// twice ("one durable copy per content"). Rows not yet `done` also get
    /// their source path/size refreshed, so a renamed or moved file retries
    /// from its current location.
    pub fn enqueue(
        &mut self,
        profile_id: &str,
        path: &Path,
        sha256: &str,
        size: u64,
    ) -> Result<bool> {
        validate_digest(sha256)?;
        let size = i64::try_from(size).map_err(|_| VaultError::IntegerRange)?;
        let path_text = path.to_string_lossy().into_owned();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing: Option<(i64, String)> = tx
            .query_row(
                "SELECT id, status FROM vault_queue WHERE profile_id=?1 AND sha256=?2
                 ORDER BY id LIMIT 1",
                params![profile_id, sha256],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let inserted = if let Some((id, status)) = existing {
            if status != "done" {
                tx.execute(
                    "UPDATE vault_queue SET path=?2, size=?3 WHERE id=?1",
                    params![id, path_text, size],
                )?;
            }
            false
        } else {
            tx.execute(
                "INSERT INTO vault_queue(profile_id,path,sha256,size,status,created_at)
                 VALUES(?1,?2,?3,?4,'pending',?5)",
                params![profile_id, path_text, sha256, size, now_unix()?],
            )?;
            true
        };
        tx.commit()?;
        Ok(inserted)
    }

    /// Claims the next pending or backoff-eligible failed item for the given profile
    /// and increments its attempt count. Filtering by profile keeps per-profile consumers
    /// from claiming (and failing) another profile's items on the shared queue database.
    pub fn claim_next(&mut self, profile_id: &str, now: i64) -> Result<Option<QueueItem>> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut statement = tx.prepare(
            "SELECT id,profile_id,path,sha256,size,status,attempts,last_error,created_at,uploaded_at
             FROM vault_queue WHERE profile_id=?1 AND status IN ('pending','failed')
             ORDER BY created_at,id",
        )?;
        let rows = statement.query_map([profile_id], decode_item)?;
        let mut candidate = None;
        for row in rows {
            let item = row?;
            if item.status == QueueStatus::Pending
                || retry_is_due(&tx, item.id, item.attempts, now)?
            {
                candidate = Some(item);
                break;
            }
        }
        drop(statement);
        let Some(mut item) = candidate else {
            tx.commit()?;
            return Ok(None);
        };
        let attempts = item
            .attempts
            .checked_add(1)
            .ok_or(VaultError::IntegerRange)?;
        tx.execute(
            "UPDATE vault_queue SET status='uploading', attempts=?2 WHERE id=?1",
            params![item.id, i64::from(attempts)],
        )?;
        item.status = QueueStatus::Uploading;
        item.attempts = attempts;
        tx.commit()?;
        Ok(Some(item))
    }

    /// Marks a claimed item complete and atomically records a successful log entry.
    pub fn mark_done(&mut self, item: &QueueItem) -> Result<()> {
        let now = now_unix()?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute(
            "UPDATE vault_queue SET status='done',last_error=NULL,uploaded_at=?2
             WHERE id=?1 AND status='uploading'",
            params![item.id, now],
        )?;
        if changed != 1 {
            return Err(VaultError::InvalidRecord(format!(
                "queue item {} was not in uploading state",
                item.id
            )));
        }
        tx.execute(
            "INSERT INTO vault_log(ts,profile_id,file,sha256,size,outcome,error)
             VALUES(?1,?2,?3,?4,?5,'done',NULL)",
            params![
                now,
                item.profile_id,
                item.path.to_string_lossy(),
                item.sha256,
                to_i64(item.size)?
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Marks a claimed item failed and atomically records its error in the sync log.
    pub fn mark_failed(&mut self, item: &QueueItem, error: &str) -> Result<()> {
        let now = now_unix()?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute(
            "UPDATE vault_queue SET status='failed',last_error=?2
             WHERE id=?1 AND status='uploading'",
            params![item.id, error],
        )?;
        if changed != 1 {
            return Err(VaultError::InvalidRecord(format!(
                "queue item {} was not in uploading state",
                item.id
            )));
        }
        tx.execute(
            "INSERT INTO vault_log(ts,profile_id,file,sha256,size,outcome,error)
             VALUES(?1,?2,?3,?4,?5,'failed',?6)",
            params![
                now,
                item.profile_id,
                item.path.to_string_lossy(),
                item.sha256,
                to_i64(item.size)?,
                error
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Loads one queue item by its database identifier.
    pub fn get(&self, id: i64) -> Result<Option<QueueItem>> {
        self.connection
            .query_row(
                "SELECT id,profile_id,path,sha256,size,status,attempts,last_error,created_at,uploaded_at
                 FROM vault_queue WHERE id=?1",
                [id],
                decode_item,
            )
            .optional()
            .map_err(Into::into)
    }

    /// Lists all queue items in insertion order.
    pub fn list(&self) -> Result<Vec<QueueItem>> {
        let mut statement = self.connection.prepare(
            "SELECT id,profile_id,path,sha256,size,status,attempts,last_error,created_at,uploaded_at
             FROM vault_queue ORDER BY id",
        )?;
        let rows = statement.query_map([], decode_item)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Returns the most recent log rows, newest first.
    pub fn log(&self, limit: usize) -> Result<Vec<VaultLogEntry>> {
        let limit = i64::try_from(limit).map_err(|_| VaultError::IntegerRange)?;
        let mut statement = self.connection.prepare(
            "SELECT ts,profile_id,file,sha256,size,outcome,error FROM vault_log
             ORDER BY ts DESC,id DESC LIMIT ?1",
        )?;
        let rows = statement.query_map([limit], |row| {
            let size: i64 = row.get(4)?;
            let size = u64::try_from(size)
                .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(4, size))?;
            Ok(VaultLogEntry {
                ts: row.get(0)?,
                profile_id: row.get(1)?,
                file: PathBuf::from(row.get::<_, String>(2)?),
                sha256: row.get(3)?,
                size,
                outcome: row.get(5)?,
                error: row.get(6)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}

fn retry_is_due(connection: &Connection, id: i64, attempts: u32, now: i64) -> Result<bool> {
    let last_attempt: Option<i64> = connection.query_row(
        "SELECT MAX(ts) FROM vault_log WHERE outcome='failed' AND file=(
                SELECT path FROM vault_queue WHERE id=?1
            ) AND profile_id=(SELECT profile_id FROM vault_queue WHERE id=?1)",
        [id],
        |row| row.get(0),
    )?;
    let Some(last_attempt) = last_attempt else {
        return Ok(true);
    };
    let shift = attempts.saturating_sub(1).min(10);
    let delay = 2_i64.pow(shift).min(3600);
    Ok(now.saturating_sub(last_attempt) >= delay)
}

fn decode_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<QueueItem> {
    let size: i64 = row.get(4)?;
    let attempts: i64 = row.get(6)?;
    let status_text: String = row.get(5)?;
    let status = QueueStatus::parse(&status_text).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let size =
        u64::try_from(size).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(4, size))?;
    let attempts = u32::try_from(attempts)
        .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(6, attempts))?;
    Ok(QueueItem {
        id: row.get(0)?,
        profile_id: row.get(1)?,
        path: PathBuf::from(row.get::<_, String>(2)?),
        sha256: row.get(3)?,
        size,
        status,
        attempts,
        last_error: row.get(7)?,
        created_at: row.get(8)?,
        uploaded_at: row.get(9)?,
    })
}

fn validate_digest(sha256: &str) -> Result<()> {
    if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(VaultError::InvalidRecord(
            "SHA-256 must be 64 hexadecimal characters".into(),
        ));
    }
    Ok(())
}

fn to_i64(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| VaultError::IntegerRange)
}

fn now_unix() -> Result<i64> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| VaultError::InvalidRecord(error.to_string()))?
        .as_secs();
    i64::try_from(seconds).map_err(|_| VaultError::IntegerRange)
}

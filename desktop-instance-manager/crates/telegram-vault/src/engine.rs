//! Queue consumer that uploads files and commits outcomes without deleting Zalo originals.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::queue::{QueueItem, VaultQueue};
use crate::transport::TelegramTransport;
use crate::{error::VaultError, Result};

#[derive(Serialize)]
struct FileCaption<'a> {
    file_name: &'a str,
    sha256: &'a str,
    profile_id: &'a str,
    uploaded_at: i64,
}

/// Upload queue consumer for one isolated Zalo profile.
pub struct SyncEngine<T> {
    profile_id: String,
    queue: VaultQueue,
    transport: T,
}

impl<T: TelegramTransport> SyncEngine<T> {
    /// Creates an engine for the profile and its transport.
    pub fn new(profile_id: impl Into<String>, queue: VaultQueue, transport: T) -> Self {
        Self {
            profile_id: profile_id.into(),
            queue,
            transport,
        }
    }

    /// Returns a shared reference to the underlying durable queue.
    #[must_use]
    pub fn queue(&self) -> &VaultQueue {
        &self.queue
    }

    /// Returns a mutable reference to the underlying durable queue.
    pub fn queue_mut(&mut self) -> &mut VaultQueue {
        &mut self.queue
    }

    /// Returns a shared reference to the configured transport.
    #[must_use]
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Scans a profile watcher and enqueues each newly stable file by content digest.
    pub fn scan_received_files(
        &mut self,
        watcher: &mut crate::watch::ZaloReceivedWatcher,
    ) -> Result<usize> {
        let discovered = watcher.poll()?;
        let mut enqueued = 0_usize;
        for file in discovered {
            if self
                .queue
                .enqueue(&self.profile_id, &file.path, &file.sha256, file.size)?
            {
                enqueued = enqueued.checked_add(1).ok_or(VaultError::IntegerRange)?;
            }
        }
        Ok(enqueued)
    }

    /// Uploads one ready item, returning `None` when no work is eligible.
    pub fn process_one(&mut self, now: i64) -> Result<Option<QueueItem>> {
        let Some(item) = self.queue.claim_next(now)? else {
            return Ok(None);
        };
        if item.profile_id != self.profile_id {
            let error = format!(
                "queue item profile {} does not match engine profile {}",
                item.profile_id, self.profile_id
            );
            self.queue.mark_failed(&item, &error)?;
            return Err(VaultError::InvalidRecord(error));
        }
        let caption = make_caption(&item)?;
        match self
            .transport
            .upload_document(&item.profile_id, &item.path, &item.sha256, &caption)
        {
            Ok(_remote) => {
                self.queue.mark_done(&item)?;
                Ok(Some(item))
            }
            Err(error) => {
                self.queue.mark_failed(&item, &error.to_string())?;
                Ok(Some(item))
            }
        }
    }

    /// Drains all work whose retry backoff has expired and returns the number attempted.
    pub fn drain_ready(&mut self, now: i64) -> Result<usize> {
        let mut attempted = 0_usize;
        while self.process_one(now)?.is_some() {
            attempted = attempted.checked_add(1).ok_or(VaultError::IntegerRange)?;
        }
        Ok(attempted)
    }
}

fn make_caption(item: &QueueItem) -> Result<String> {
    let uploaded_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| VaultError::InvalidRecord(error.to_string()))?
        .as_secs();
    let uploaded_at = i64::try_from(uploaded_at).map_err(|_| VaultError::IntegerRange)?;
    let file_name = Path::new(&item.path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    serde_json::to_string(&FileCaption {
        file_name,
        sha256: &item.sha256,
        profile_id: &item.profile_id,
        uploaded_at,
    })
    .map_err(Into::into)
}

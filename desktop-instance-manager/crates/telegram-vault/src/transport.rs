//! Telegram upload interfaces, deterministic test transport, and optional live client.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::{error::VaultError, Result};

/// Remote message and document identity returned by a Telegram upload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteRef {
    /// Telegram peer identifier for the destination.
    pub peer_id: String,
    /// Telegram message identifier containing the uploaded document.
    pub message_id: i64,
}

/// Uploads one local file as a Telegram document.
pub trait TelegramTransport: Send + Sync {
    /// Uploads a document and associates its metadata caption.
    fn upload_document(
        &self,
        profile_id: &str,
        path: &Path,
        sha256: &str,
        caption: &str,
    ) -> Result<RemoteRef>;
}

/// Captured upload call made by [`MockTransport`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockUpload {
    /// Associated profile identifier.
    pub profile_id: String,
    /// Local file path.
    pub path: PathBuf,
    /// Content digest.
    pub sha256: String,
    /// Serialized metadata caption.
    pub caption: String,
}

/// In-memory transport for tests and local engine integration tests.
#[derive(Debug, Default)]
pub struct MockTransport {
    uploads: Mutex<Vec<MockUpload>>,
    failures_remaining: Mutex<usize>,
}

impl MockTransport {
    /// Creates a mock transport that fails the next `count` upload attempts.
    #[must_use]
    pub fn failing(count: usize) -> Self {
        Self {
            uploads: Mutex::new(Vec::new()),
            failures_remaining: Mutex::new(count),
        }
    }

    /// Returns a snapshot of the uploads observed so far.
    pub fn uploads(&self) -> Result<Vec<MockUpload>> {
        self.uploads
            .lock()
            .map(|uploads| uploads.clone())
            .map_err(|_| VaultError::Transport("mock upload lock was poisoned".into()))
    }
}

impl TelegramTransport for MockTransport {
    fn upload_document(
        &self,
        profile_id: &str,
        path: &Path,
        sha256: &str,
        caption: &str,
    ) -> Result<RemoteRef> {
        let should_fail = {
            let mut remaining = self
                .failures_remaining
                .lock()
                .map_err(|_| VaultError::Transport("mock failure lock was poisoned".into()))?;
            if *remaining > 0 {
                *remaining -= 1;
                true
            } else {
                false
            }
        };
        let mut uploads = self
            .uploads
            .lock()
            .map_err(|_| VaultError::Transport("mock upload lock was poisoned".into()))?;
        let message_id = i64::try_from(uploads.len() + 1).map_err(|_| VaultError::IntegerRange)?;
        uploads.push(MockUpload {
            profile_id: profile_id.to_owned(),
            path: path.to_path_buf(),
            sha256: sha256.to_owned(),
            caption: caption.to_owned(),
        });
        if should_fail {
            return Err(VaultError::Transport("mock transport failure".into()));
        }
        Ok(RemoteRef {
            peer_id: "saved_messages".into(),
            message_id,
        })
    }
}
#[cfg(feature = "live")]
mod file_session;

#[cfg(feature = "live")]
mod live;

#[cfg(feature = "live")]
pub use file_session::{FileSession, FileSessionError};

#[cfg(feature = "live")]
pub use live::{LiveTransport, PendingPhoneLogin, TelegramCredentials};

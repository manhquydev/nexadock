//! Errors shared by the vault persistence, watcher, and transport layers.

use std::path::PathBuf;

/// Result type used by the Telegram vault crate.
pub type Result<T, E = VaultError> = std::result::Result<T, E>;
/// Errors returned by vault operations.
#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    /// A filesystem operation failed.
    #[error("filesystem operation failed at {path}: {source}")]
    Io {
        /// Path associated with the operation.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The shared application core rejected an atomic file operation.
    #[error("core storage error: {0}")]
    Core(#[from] dim_core::DimError),
    /// A JSON document could not be encoded or decoded.
    #[error("vault configuration JSON error: {0}")]
    Json(#[from] serde_json::Error),
    /// A SQLite operation failed.
    #[error("vault database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// A file watcher could not be initialized or used.
    #[error("file watcher error: {0}")]
    Watch(String),
    /// A Telegram transport operation failed.
    #[error("Telegram transport error: {0}")]
    Transport(String),
    /// A bounded wait elapsed before the expected Telegram event occurred.
    #[error("timed out waiting for Telegram: {0}")]
    Timeout(String),
    /// A persisted record violates the vault schema.
    #[error("invalid vault record: {0}")]
    InvalidRecord(String),
    /// A database record could not be represented in the requested integer type.
    #[error("integer value is outside the supported range")]
    IntegerRange,
}

impl VaultError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

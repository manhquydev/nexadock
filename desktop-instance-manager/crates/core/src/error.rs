//! Domain error types and stable machine-readable error codes.

use std::fmt;
use thiserror::Error;

/// Canonical error type returned across all `dim-*` crates.
#[derive(Debug, Error)]
pub enum DimError {
    /// Profile display name failed validation (empty, too long, or control characters).
    #[error("invalid profile name: {0}")]
    InvalidProfileName(String),

    /// Another profile already uses the requested display name.
    #[error("duplicate profile name: {0}")]
    DuplicateProfileName(String),

    /// Profile ID or name could not be located in `profiles.json`.
    #[error("profile not found: {0}")]
    ProfileNotFound(String),

    /// Operation requires the profile instance to be stopped first.
    #[error("profile '{0}' is currently running")]
    ProfileRunning(String),

    /// `.zalo-profile-id` inside the profile directory does not match `profiles.json`.
    #[error("profile identity mismatch: expected '{expected}', found '{actual}'")]
    ProfileIdentityMismatch {
        /// Profile ID recorded in `profiles.json`.
        expected: String,
        /// Profile ID stored in `.zalo-profile-id`.
        actual: String,
    },

    /// Refused to stop or mutate a process overlapping the user's primary Zalo Desktop installation.
    #[error("refused to terminate protected primary Zalo process: {0}")]
    ProtectedPrimaryProcess(String),

    /// Refused to point storage, backup, or restore paths inside the primary Zalo data directory.
    #[error("refused to access protected primary Zalo path: {0}")]
    ProtectedPrimaryPath(String),

    /// Another running profile is already using the same logged-in Zalo account ID.
    #[error("zalo_user_id '{user_id}' is already active in running profile '{running_profile}'")]
    DuplicateZaloUserId {
        /// Numeric Zalo account ID (`Database/_production/<6+ digits>`).
        user_id: String,
        /// Name or ID of the profile currently running with this account.
        running_profile: String,
    },

    /// Available disk space is below the required safety threshold (5 GiB + payload).
    #[error("insufficient disk space on '{path}': available {available_bytes} bytes, required {required_bytes} bytes")]
    InsufficientDiskSpace {
        /// Target filesystem path checked.
        path: String,
        /// Free bytes currently available to the unprivileged user.
        available_bytes: u64,
        /// Minimum free bytes required.
        required_bytes: u64,
    },

    /// Path escaped its allowed root directory or contained traversal/reserved components.
    #[error("unsafe path rejected: {0}")]
    PathTraversal(String),

    /// Windows Folder Redirection on `User Shell Folders\\AppData` blocks safe per-process isolation.
    #[error("folder redirection detected for AppData: {0}")]
    FolderRedirectionBlocked(String),

    /// Official Zalo Desktop executable could not be discovered.
    #[error("Zalo installation not found: {0}")]
    InstallNotFound(String),

    /// Target executable failed cryptographic trust or publisher verification.
    #[error("executable signature verification failed: {0}")]
    UnverifiedBinary(String),

    /// Operation requires the profile instance to be running.
    #[error("profile '{0}' is not currently running")]
    NotRunning(String),

    /// Operation or target is unsupported on the current platform.
    #[error("unsupported platform operation: {0}")]
    UnsupportedPlatform(String),

    /// Backup creation or verification failed.
    #[error("backup error: {0}")]
    BackupError(String),

    /// Backup restore failed.
    #[error("restore error: {0}")]
    RestoreError(String),

    /// Process launch, focus, or shutdown failed.
    #[error("process operation failed: {0}")]
    ProcessError(String),

    /// Stored metadata schema is unsupported or corrupt.
    #[error("schema error: {0}")]
    SchemaError(String),

    /// Explicit destructive confirmation flag was not supplied.
    #[error("confirmation required: {0}")]
    ConfirmationRequired(String),

    /// Underlying filesystem I/O failure.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization or deserialization failure.
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

impl DimError {
    /// Returns the stable upper-snake-case error code shared with `dim-host.ps1` and the Tauri UI.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidProfileName(_) => "INVALID_NAME",
            Self::DuplicateProfileName(_) => "DUPLICATE_NAME",
            Self::ProfileNotFound(_) => "NOT_FOUND",
            Self::ProfileRunning(_) => "PROFILE_RUNNING",
            Self::ProfileIdentityMismatch { .. } => "PROFILE_MARKER_MISMATCH",
            Self::ProtectedPrimaryProcess(_) => "PROTECTED_PRIMARY_PROCESS",
            Self::ProtectedPrimaryPath(_) => "PROTECTED_PRIMARY_PATH",
            Self::DuplicateZaloUserId { .. } => "DUPLICATE_ZALO_USER_ID",
            Self::InsufficientDiskSpace { .. } => "INSUFFICIENT_DISK_SPACE",
            Self::PathTraversal(_) => "PATH_TRAVERSAL",
            Self::FolderRedirectionBlocked(_) => "FOLDER_REDIRECTION_DETECTED",
            Self::InstallNotFound(_) => "INSTALL_NOT_FOUND",
            Self::UnverifiedBinary(_) => "UNVERIFIED_BINARY",
            Self::NotRunning(_) => "NOT_RUNNING",
            Self::UnsupportedPlatform(_) => "UNSUPPORTED_PLATFORM",
            Self::BackupError(_) => "BACKUP_ERROR",
            Self::RestoreError(_) => "RESTORE_ERROR",
            Self::ProcessError(_) => "PROCESS_ERROR",
            Self::SchemaError(_) => "SCHEMA_ERROR",
            Self::ConfirmationRequired(_) => "CONFIRM_REQUIRED",
            Self::Io(_) => "IO_ERROR",
            Self::Json(_) => "JSON_ERROR",
        }
    }
}

/// Serializable operation envelope matching CLI/UI JSON response expectations.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OperationOutcome<T> {
    /// Whether the operation succeeded.
    pub success: bool,
    /// Human-readable status or error message.
    pub message: String,
    /// Stable machine-readable error code when `success == false`.
    pub error_code: Option<String>,
    /// Optional structured payload.
    pub data: Option<T>,
}

impl<T> OperationOutcome<T> {
    /// Creates a successful outcome with payload.
    pub fn ok(message: impl Into<String>, data: T) -> Self {
        Self {
            success: true,
            message: message.into(),
            error_code: None,
            data: Some(data),
        }
    }

    /// Creates a failure outcome from a [`DimError`].
    pub fn from_err(err: &DimError) -> Self {
        Self {
            success: false,
            message: err.to_string(),
            error_code: Some(err.code().to_owned()),
            data: None,
        }
    }
}

impl fmt::Display for OperationOutcome<()> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.success {
            write!(f, "OK: {}", self.message)
        } else {
            write!(
                f,
                "ERR [{}]: {}",
                self.error_code.as_deref().unwrap_or("UNKNOWN"),
                self.message
            )
        }
    }
}

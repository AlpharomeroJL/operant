//! Recorder error type.

/// Errors returned by every `operant-recorder` operation.
#[derive(Debug, thiserror::Error)]
pub enum RecorderError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("run not found: {0}")]
    RunNotFound(String),
    #[error("blob not found: {0}")]
    BlobNotFound(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("recorder connection mutex was poisoned by a prior panic")]
    Poisoned,
}

/// Result alias used throughout the crate.
pub type Result<T> = std::result::Result<T, RecorderError>;

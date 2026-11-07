//! Brain module error types.

use thiserror::Error;

/// Domain errors for Brain operations.
#[derive(Debug, Error)]
pub enum BrainError {
    /// Failure spawning or executing the Antigravity subprocess.
    #[error("failed to spawn antigravity process: {0}")]
    ProcessSpawn(#[from] std::io::Error),

    /// Subprocess terminated unexpectedly with non-zero exit code.
    #[error("process terminated prematurely with code {code:?}: {stderr}")]
    ProcessPrematureExit {
        /// Process exit status code.
        code: Option<i32>,
        /// Stderr capture.
        stderr: String,
    },

    /// No tokens received within configured inactivity window (15s).
    #[error("antigravity stream timeout after {duration_secs}s")]
    InactivityTimeout {
        /// Inactivity duration before timeout triggered.
        duration_secs: u64,
    },

    /// Deserialization error for NDJSON event.
    #[error("ndjson serde error: {0}")]
    SerdeError(#[from] serde_json::Error),

    /// Session pointer storage read/write error.
    #[error("session pointer error: {0}")]
    SessionError(String),

    /// Confirmation request timed out after 10s.
    #[error("action confirmation timed out")]
    ConfirmationTimeout,

    /// Fillers configuration error.
    #[error("fillers error: {0}")]
    FillersError(String),

    /// Generic brain error.
    #[error("brain error: {0}")]
    Internal(String),
}

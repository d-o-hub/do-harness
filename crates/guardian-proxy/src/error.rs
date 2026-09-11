//! Typed errors for the guardian-proxy library surface.

use thiserror::Error;

/// Library-level result alias for guardian-proxy.
pub type Result<T> = std::result::Result<T, GuardianError>;

/// Errors raised while opening, appending to, or verifying the audit log,
/// initializing governance, or handling configuration.
#[derive(Debug, Error)]
pub enum GuardianError {
    /// Audit log I/O failure for a specific path.
    #[error("audit io at {path}: {io}")]
    AuditIo {
        /// Path involved in the failed operation.
        path: String,
        /// Underlying I/O error.
        #[source]
        io: std::io::Error,
    },
    /// The audit hash chain failed verification.
    #[error("audit chain tampered at seq {seq}")]
    Tampered {
        /// Sequence number where the chain diverges.
        seq: i64,
    },
    /// AGT governance client initialization failed.
    #[error("governance init failed: {0}")]
    GovernanceInit(String),
    /// Invalid proxy configuration.
    #[error("config: {0}")]
    Config(String),
    /// Audit record serialization or parsing failed.
    #[error("audit serialization: {0}")]
    Serialization(#[from] serde_json::Error),
}

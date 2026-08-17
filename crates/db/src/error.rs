//! Error types for the do-harness persistence layer.

use std::path::PathBuf;

/// Result alias for operations that can fail with a [`DbError`].
pub type Result<T> = std::result::Result<T, DbError>;

/// Errors produced by the do-harness persistence layer.
///
/// All `libsql` failures (execute, query, row access, transactions) funnel
/// through [`DbError::Migrate`] because libSQL surfaces them as a single
/// [`libsql::Error`] type.
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    /// Failed to create the state database's parent directory.
    #[error("failed to create state database directory at {path}: {source}")]
    Io {
        /// Directory that could not be created.
        path: PathBuf,
        /// Underlying filesystem error.
        #[source]
        source: std::io::Error,
    },
    /// Failed to open or connect to the state database.
    #[error("failed to connect or open the state database at {path}: {source}")]
    Connect {
        /// Database path that could not be opened.
        path: PathBuf,
        /// Underlying libSQL error.
        #[source]
        source: libsql::Error,
    },
    /// A SQL statement or migration failed.
    #[error("schema migration failed: {0}")]
    Migrate(#[from] libsql::Error),
    /// A stored task row carried an unrecognized status value.
    #[error("invalid stored task status '{0}'")]
    InvalidTaskStatus(String),
    /// A foreign-key or unique constraint was violated.
    #[error("{0}")]
    Constraint(String),
    /// A record expected to exist was missing.
    #[error("not found: {0}")]
    NotFound(String),
    /// No harness root could be discovered.
    #[error("harness root not found: {0}")]
    RootNotFound(String),
    /// A row count could not be converted.
    #[error("count conversion failed: {0}")]
    IntConversion(#[from] std::num::TryFromIntError),
    /// A fallback for otherwise-untyped failures.
    #[error("{0}")]
    Other(String),
}

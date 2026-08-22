//! Error types for the do-harness persistence layer.

use std::path::PathBuf;

/// Result alias for operations that can fail with a [`DbError`].
pub type Result<T> = std::result::Result<T, DbError>;

/// `SQLite` primary result code for constraint violations; extended codes
/// fold into their primary code, hence the mask in [`DbError::from`].
const SQLITE_CONSTRAINT: std::ffi::c_int = 19;

/// Errors produced by the do-harness persistence layer.
///
/// All `libsql` failures (execute, query, row access, transactions) funnel
/// through [`DbError::Sql`] because libSQL surfaces them as a single
/// [`libsql::Error`] type; constraint violations are detected and surfaced as
/// [`DbError::Constraint`] so callers can match on them.
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
    /// A SQL statement, query, or transaction failed.
    #[error("sql failed: {0}")]
    Sql(libsql::Error),
    /// A stored task row carried an unrecognized status value.
    #[error("invalid stored task status '{0}'")]
    InvalidTaskStatus(String),
    /// A stored workflow-event payload failed to deserialize into a
    /// [`do_harness_types::WorkflowEvent`].
    #[error("invalid stored workflow event payload: {0}")]
    InvalidEventPayload(String),
    /// A terminal-status writer was called with a non-terminal
    /// [`do_harness_types::TaskState`] (only `done`/`failed` map to events).
    #[error("terminal-status writer requires 'done' or 'failed', got '{0}'")]
    InvalidTerminalState(String),
    /// A foreign-key or unique constraint was violated.
    #[error("constraint violated: {0}")]
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
    /// The state database was written by a newer harness; this binary's
    /// migration catalog does not cover it (downgrade guard).
    #[error(
        "state database has migration {applied}, newer than this binary knows (max {known_max})"
    )]
    FutureDatabase {
        /// The applied migration version this binary does not know.
        applied: i64,
        /// The highest version in this binary's embedded catalog.
        known_max: i64,
    },
}

impl From<libsql::Error> for DbError {
    fn from(err: libsql::Error) -> Self {
        if let libsql::Error::SqliteFailure(code, message) = &err {
            if (code & 0xFF) == SQLITE_CONSTRAINT {
                return DbError::Constraint(message.clone());
            }
        }
        DbError::Sql(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Constraint failures surface as `Constraint`, everything else as `Sql`.
    #[test]
    fn libsql_errors_map_by_constraint_code() {
        let constraint = DbError::from(libsql::Error::SqliteFailure(
            SQLITE_CONSTRAINT,
            "UNIQUE constraint failed".to_owned(),
        ));
        assert!(matches!(constraint, DbError::Constraint(_)));

        // An extended constraint code (19 | 0x100 << 8) still maps to
        // `Constraint` after masking to the primary code.
        let extended = DbError::from(libsql::Error::SqliteFailure(
            SQLITE_CONSTRAINT | (1 << 8),
            "constraint failed".to_owned(),
        ));
        assert!(matches!(extended, DbError::Constraint(_)));

        let other = DbError::from(libsql::Error::SqliteFailure(
            1,
            "generic failure".to_owned(),
        ));
        assert!(matches!(other, DbError::Sql(_)));
    }
}

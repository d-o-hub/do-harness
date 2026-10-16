//! Error types for the do-harness persistence layer.

use std::path::PathBuf;

/// Result alias for operations that can fail with a [`DbError`].
pub type Result<T> = std::result::Result<T, DbError>;

/// `SQLite` primary result code for constraint violations; extended codes
/// fold into their primary code, hence the mask in [`DbError::from`].
const SQLITE_CONSTRAINT: std::ffi::c_int = 19;

/// `SQLite` primary result code for a busy handler / transient lock (`database is locked`).
const SQLITE_BUSY: std::ffi::c_int = 5;

/// `SQLite` primary result code for a table/database lock contention (`database table is locked`).
const SQLITE_LOCKED: std::ffi::c_int = 6;

/// `SQLite` extended result code: a `UNIQUE` constraint failed.
const SQLITE_CONSTRAINT_UNIQUE: std::ffi::c_int = 2067;

/// `SQLite` extended result code: a `PRIMARY KEY` constraint failed.
const SQLITE_CONSTRAINT_PRIMARYKEY: std::ffi::c_int = 1555;

/// `SQLite` extended result code: a `rowid` was not unique.
const SQLITE_CONSTRAINT_ROWID: std::ffi::c_int = 2579;

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
    /// Failed to write the pre-migration backup copy.
    #[error("failed to back up state database to {path}: {source}")]
    Backup {
        /// Backup path that could not be written.
        path: PathBuf,
        /// Underlying filesystem error.
        #[source]
        source: std::io::Error,
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
    /// A lifecycle writer targeted a task whose current state is already
    /// terminal (`done`/`failed`). Allowing it would append an out-of-order
    /// event (`TaskAdvanced`/`TaskCompleted` after completion) and break the
    /// "the event stream covers every persisted transition" invariant.
    #[error("invalid task transition: {0}")]
    InvalidTaskTransition(String),
    /// A SQL constraint was violated.
    ///
    /// The extended `SQLite` result code is kept alongside the engine's
    /// message so callers can tell the kinds apart
    /// (`SQLITE_CONSTRAINT_UNIQUE` = 2067 versus `SQLITE_CONSTRAINT_FOREIGNKEY`
    /// = 787); see <https://www.sqlite.org/rescode.html>.
    #[error("constraint violated: {message}")]
    Constraint {
        /// Extended `SQLite` result code, e.g. `SQLITE_CONSTRAINT_UNIQUE`.
        code: std::ffi::c_int,
        /// Constraint text reported by the engine.
        message: String,
    },
    /// A record expected to exist was missing.
    #[error("not found: {0}")]
    NotFound(String),
    /// An assumption the code makes about the database was violated: a
    /// read-back after a write returned no row, or a probe that must always
    /// answer did not.
    ///
    /// Distinct from [`DbError::NotFound`], which means the caller asked for a
    /// record that legitimately does not exist.
    #[error("database invariant violated: {0}")]
    Invariant(String),
    /// A stored skill-eval mode was not one of the persisted wire names.
    #[error("invalid stored eval mode '{0}': expected deterministic|agent")]
    InvalidEvalMode(String),
    /// A workflow gate's required sensor has no passing beat at write time
    /// (re-checked inside the command transaction to close the read-then-write
    /// race with concurrent `verify --record`).
    #[error("task {task_id} gate unsatisfied: sensor '{sensor}' has no passing beat")]
    GateUnsatisfied {
        /// Task whose gate failed.
        task_id: i64,
        /// Sensor that must have a latest `ok` sensor beat.
        sensor: String,
    },
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
                return DbError::Constraint {
                    code: *code,
                    message: message.clone(),
                };
            }
        }
        DbError::Sql(err)
    }
}

impl DbError {
    /// Whether this failure is transient `SQLite` lock contention.
    ///
    /// WAL plus `busy_timeout` make these rare, but a writer that loses the
    /// race can still surface `SQLITE_BUSY` or `SQLITE_LOCKED`; callers wrap
    /// idempotent units of work in [`retry_on_busy`].
    #[must_use]
    pub fn is_busy(&self) -> bool {
        match self {
            DbError::Sql(libsql::Error::SqliteFailure(code, _)) => {
                let primary = code & 0xFF;
                primary == SQLITE_BUSY || primary == SQLITE_LOCKED
            }
            _ => false,
        }
    }

    /// Builds a uniqueness violation with the canonical extended code.
    ///
    /// Used by callers that synthesise a race outcome rather than receiving one
    /// from the engine.
    #[must_use]
    pub fn unique_violation(message: impl Into<String>) -> Self {
        DbError::Constraint {
            code: SQLITE_CONSTRAINT_UNIQUE,
            message: message.into(),
        }
    }

    /// Whether this failure is a uniqueness violation, and therefore the only
    /// constraint failure a concurrent writer can retry its way out of.
    ///
    /// libSQL reports the extended result code (`sqlite3_extended_errcode`), so
    /// a lost insert race is distinguishable from a foreign-key or `CHECK`
    /// failure by the code alone.
    #[must_use]
    pub fn is_unique_violation(&self) -> bool {
        match self {
            DbError::Constraint { code, .. } => matches!(
                *code,
                SQLITE_CONSTRAINT_UNIQUE | SQLITE_CONSTRAINT_PRIMARYKEY | SQLITE_CONSTRAINT_ROWID
            ),
            _ => false,
        }
    }
}

/// Runs `op` up to `attempts` times, retrying only transient `SQLITE_BUSY`
/// failures. Non-busy errors return immediately; `op` must be idempotent
/// (fully transactional) because a retry replays the whole unit of work.
///
/// # Errors
///
/// Returns the last error when every attempt fails (or the first non-busy
/// error).
pub async fn retry_on_busy<T, F, Fut>(attempts: usize, mut op: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut last: Option<DbError> = None;
    for attempt in 0..attempts {
        match op().await {
            Ok(value) => return Ok(value),
            Err(err) if err.is_busy() && attempt + 1 < attempts => {
                last = Some(err);
                // Progressive backoff so the competing writer can finish.
                let backoff = u64::try_from(attempt + 1).unwrap_or(1) * 10;
                tokio::time::sleep(std::time::Duration::from_millis(backoff)).await;
            }
            Err(err) => return Err(err),
        }
    }
    Err(last.unwrap_or_else(|| {
        DbError::Sql(libsql::Error::SqliteFailure(
            SQLITE_BUSY,
            "busy retries exhausted".to_owned(),
        ))
    }))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// Constraint failures surface as `Constraint`, everything else as `Sql`.
    #[test]
    fn libsql_errors_map_by_constraint_code() {
        let constraint = DbError::from(libsql::Error::SqliteFailure(
            SQLITE_CONSTRAINT,
            "UNIQUE constraint failed".to_owned(),
        ));
        assert!(matches!(constraint, DbError::Constraint { .. }));

        // An extended constraint code (19 | 0x100 << 8) still maps to
        // `Constraint` after masking to the primary code.
        let extended = DbError::from(libsql::Error::SqliteFailure(
            SQLITE_CONSTRAINT | (1 << 8),
            "constraint failed".to_owned(),
        ));
        assert!(matches!(extended, DbError::Constraint { .. }));

        let other = DbError::from(libsql::Error::SqliteFailure(
            1,
            "generic failure".to_owned(),
        ));
        assert!(matches!(other, DbError::Sql(_)));
    }

    /// The extended code is preserved, so a retryable `UNIQUE(seq)` race is
    /// distinguishable from a foreign-key or `CHECK` failure. Codes per
    /// <https://www.sqlite.org/rescode.html>.
    #[test]
    fn constraint_kinds_are_distinguished_by_extended_code() {
        const FOREIGN_KEY: std::ffi::c_int = 787;
        const CHECK: std::ffi::c_int = 275;

        let unique = DbError::from(libsql::Error::SqliteFailure(
            SQLITE_CONSTRAINT_UNIQUE,
            "UNIQUE constraint failed: workflow_events.seq".to_owned(),
        ));
        assert!(matches!(unique, DbError::Constraint { code: 2067, .. }));
        assert!(unique.is_unique_violation());

        let primary_key = DbError::from(libsql::Error::SqliteFailure(
            SQLITE_CONSTRAINT_PRIMARYKEY,
            "PRIMARY KEY constraint failed".to_owned(),
        ));
        assert!(primary_key.is_unique_violation());

        let foreign_key = DbError::from(libsql::Error::SqliteFailure(
            FOREIGN_KEY,
            "FOREIGN KEY constraint failed".to_owned(),
        ));
        assert!(matches!(foreign_key, DbError::Constraint { code: 787, .. }));
        assert!(!foreign_key.is_unique_violation());

        let check = DbError::from(libsql::Error::SqliteFailure(
            CHECK,
            "CHECK constraint failed".to_owned(),
        ));
        assert!(!check.is_unique_violation());
        assert!(!DbError::NotFound("gone".to_owned()).is_unique_violation());
        assert!(!DbError::Invariant("probe".to_owned()).is_unique_violation());
    }

    /// A synthesised race uses the canonical extended code, and the new
    /// variants keep their context in the rendered message.
    #[test]
    fn synthesised_errors_render_their_context() {
        let exhausted = DbError::unique_violation("seq retries exhausted");
        assert!(exhausted.is_unique_violation());
        assert!(exhausted.to_string().contains("seq retries exhausted"));

        let invariant = DbError::Invariant("beat id vanished after insert".to_owned());
        assert!(invariant.to_string().contains("beat id vanished"));

        let mode = DbError::InvalidEvalMode("bogus".to_owned());
        assert!(mode.to_string().contains("bogus"));
        assert!(mode.to_string().contains("deterministic|agent"));
    }

    /// `SQLITE_BUSY` and `SQLITE_LOCKED` (and extended codes) are classified as transient.
    #[test]
    fn busy_is_transient_and_other_errors_are_not() {
        let busy = DbError::from(libsql::Error::SqliteFailure(
            SQLITE_BUSY,
            "database is locked".to_owned(),
        ));
        assert!(busy.is_busy());

        let busy_extended = DbError::from(libsql::Error::SqliteFailure(
            SQLITE_BUSY | (1 << 8),
            "database is locked (timeout)".to_owned(),
        ));
        assert!(busy_extended.is_busy());

        let locked = DbError::from(libsql::Error::SqliteFailure(
            SQLITE_LOCKED,
            "database table is locked".to_owned(),
        ));
        assert!(locked.is_busy());

        let locked_extended = DbError::from(libsql::Error::SqliteFailure(
            SQLITE_LOCKED | (1 << 8),
            "database table is locked (shared cache)".to_owned(),
        ));
        assert!(locked_extended.is_busy());

        let constraint = DbError::from(libsql::Error::SqliteFailure(
            SQLITE_CONSTRAINT,
            "constraint".to_owned(),
        ));
        assert!(!constraint.is_busy());
        assert!(!DbError::NotFound("x".to_owned()).is_busy());
    }

    /// The retry wrapper replays only busy failures and returns the value.
    #[tokio::test(flavor = "current_thread")]
    async fn retry_on_busy_replays_transient_then_succeeds() {
        let mut calls = 0usize;
        let result: Result<u32> = retry_on_busy(3, || {
            calls += 1;
            let busy = calls < 2;
            async move {
                if busy {
                    Err(DbError::from(libsql::Error::SqliteFailure(
                        SQLITE_BUSY,
                        "database is locked".to_owned(),
                    )))
                } else {
                    Ok(7)
                }
            }
        })
        .await;
        assert_eq!(result.unwrap(), 7);
        assert_eq!(calls, 2);
    }

    /// Non-busy failures are not retried.
    #[tokio::test(flavor = "current_thread")]
    async fn retry_on_busy_returns_non_busy_immediately() {
        let mut calls = 0usize;
        let result: Result<u32> = retry_on_busy(3, || {
            calls += 1;
            async move { Err(DbError::NotFound("gone".to_owned())) }
        })
        .await;
        assert!(matches!(result, Err(DbError::NotFound(_))));
        assert_eq!(calls, 1);
    }
}

//! State-database health classification for `doctor`.
//!
//! Compares the binary's embedded migration catalog against the local
//! agent-state store without mutating it, so a stale binary is flagged as a
//! doctor failure before any persistence command dies on the downgrade guard
//! (`DbError::FutureDatabase`).

use std::path::Path;

use anyhow::{Context, Result};
use do_harness_db::{connect, db_path, inspect_migrations};

/// Classified health of the state database relative to this binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbHealth {
    /// No database file exists yet (`init-db` has never run).
    Absent,
    /// The database file exists but carries no migration tracking table.
    Uninitialized,
    /// This binary ships migrations the database has not applied yet.
    Pending {
        /// Highest version applied in the database.
        applied: i64,
        /// Highest version in the binary's catalog.
        known: i64,
    },
    /// Applied migrations match the embedded catalog exactly.
    Current,
    /// The database was written by a newer harness; this binary cannot use it.
    Future {
        /// Highest version applied in the database.
        applied: i64,
        /// Highest version in the binary's catalog.
        known: i64,
    },
}

impl DbHealth {
    /// Whether this state blocks persistence commands outright.
    #[must_use]
    pub fn is_blocking(self) -> bool {
        matches!(self, DbHealth::Future { .. })
    }

    /// Renders the doctor line: severity mark plus human-readable message.
    ///
    /// Only [`DbHealth::Future`] is fatal: it means this binary predates the
    /// database schema and every persistence command will refuse to run until
    /// the binary is rebuilt. Absent, uninitialized, and pending states are
    /// advisory.
    #[must_use]
    pub fn render(self) -> (&'static str, String) {
        match self {
            DbHealth::Absent => ("WARN", "absent (run: do-harness init-db)".to_owned()),
            DbHealth::Uninitialized => (
                "WARN",
                "exists but was never migrated (run: do-harness init-db)".to_owned(),
            ),
            DbHealth::Pending { applied, known } => (
                "WARN",
                format!(
                    "pending migrations ({applied} applied, {known} known; run: do-harness init-db)"
                ),
            ),
            DbHealth::Current => ("OK", "migrations aligned".to_owned()),
            DbHealth::Future { applied, known } => (
                "FAIL",
                format!(
                    "newer than this binary ({applied} applied, max {known}); rebuild: cargo build --release -p do-harness"
                ),
            ),
        }
    }
}

/// Probes migration alignment without creating or migrating the database.
///
/// # Errors
///
/// Returns an error when an existing database cannot be opened or its
/// migration state cannot be read.
pub async fn probe(root: &Path) -> Result<DbHealth> {
    let path = db_path(root);
    if !path.exists() {
        return Ok(DbHealth::Absent);
    }
    let conn = connect(&path)
        .await
        .with_context(|| format!("failed to open state database at {}", path.display()))?;
    let skew = inspect_migrations(&conn)
        .await
        .with_context(|| format!("failed to read migration state from {}", path.display()))?;
    // Route through the canonical predicates so classification has a single
    // source of truth in the db crate.
    Ok(match skew.applied_max {
        None => DbHealth::Uninitialized,
        Some(applied) if skew.is_future() => DbHealth::Future {
            applied,
            known: skew.known_max,
        },
        Some(applied) if skew.is_pending() => DbHealth::Pending {
            applied,
            known: skew.known_max,
        },
        Some(_) => DbHealth::Current,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn absent_when_no_database_file() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(probe(dir.path()).await.unwrap(), DbHealth::Absent);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn uninitialized_for_empty_database_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = db_path(dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"").unwrap();

        assert_eq!(probe(dir.path()).await.unwrap(), DbHealth::Uninitialized);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn current_after_connect_and_migrate() {
        let dir = tempfile::tempdir().unwrap();
        do_harness_db::connect_and_migrate(dir.path())
            .await
            .unwrap();

        assert_eq!(probe(dir.path()).await.unwrap(), DbHealth::Current);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn pending_when_catalog_ahead_of_database() {
        let dir = tempfile::tempdir().unwrap();
        do_harness_db::connect_and_migrate(dir.path())
            .await
            .unwrap();
        // Roll the tracking table back to simulate a database written by an
        // older harness whose catalog stopped at migration 1.
        let conn = do_harness_db::connect(db_path(dir.path())).await.unwrap();
        conn.execute("DELETE FROM schema_migrations WHERE version > 1", ())
            .await
            .unwrap();

        match probe(dir.path()).await.unwrap() {
            DbHealth::Pending { applied, known } => {
                assert_eq!(applied, 1);
                assert!(known > 1);
            }
            other => panic!("expected Pending, got {other:?}"),
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn future_when_database_ahead_of_binary() {
        let dir = tempfile::tempdir().unwrap();
        let conn = do_harness_db::connect_and_migrate(dir.path())
            .await
            .unwrap();
        conn.execute(
            "INSERT INTO schema_migrations (version, name, applied_at) VALUES (9999, 'future', 0)",
            (),
        )
        .await
        .unwrap();

        match probe(dir.path()).await.unwrap() {
            DbHealth::Future { applied, known } => {
                assert_eq!(applied, 9999);
                // Relationship, not exact value: the catalog grows with every
                // migration and this test must not rot when it does.
                assert!(applied > known);
            }
            other => panic!("expected Future, got {other:?}"),
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn only_future_states_block_persistence() {
        assert!(
            DbHealth::Future {
                applied: 10,
                known: 9
            }
            .is_blocking()
        );
        for benign in [
            DbHealth::Absent,
            DbHealth::Uninitialized,
            DbHealth::Current,
            DbHealth::Pending {
                applied: 1,
                known: 9,
            },
        ] {
            assert!(!benign.is_blocking());
        }
    }
}

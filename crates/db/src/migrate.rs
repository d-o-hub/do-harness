//! Connection and migration management for the agent-state libSQL store.

use std::path::Path;

use crate::error::{DbError, Result};
use libsql::{Builder, Connection, params::Params};

/// A single versioned schema migration.
struct Migration {
    version: i64,
    name: &'static str,
    sql: &'static str,
}

/// Embedded migration catalog, ordered by ascending version.
const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "init",
        sql: include_str!("../migrations/0001_init.sql"),
    },
    Migration {
        version: 2,
        name: "invariants",
        sql: include_str!("../migrations/0002_invariants.sql"),
    },
    Migration {
        version: 3,
        name: "persist",
        sql: include_str!("../migrations/0003_persist.sql"),
    },
    Migration {
        version: 4,
        name: "scope",
        sql: include_str!("../migrations/0004_scope.sql"),
    },
    Migration {
        version: 5,
        name: "beat_sensor",
        sql: include_str!("../migrations/0005_beat_sensor.sql"),
    },
    Migration {
        version: 6,
        name: "eval_latest",
        sql: include_str!("../migrations/0006_eval_latest.sql"),
    },
    Migration {
        version: 7,
        name: "fk_strike_index",
        sql: include_str!("../migrations/0007_fk_strike_index.sql"),
    },
    Migration {
        version: 8,
        name: "eval_history_and_baselines",
        sql: include_str!("../migrations/0008_eval_history_and_baselines.sql"),
    },
    Migration {
        version: 9,
        name: "workflow_events",
        sql: include_str!("../migrations/0009_workflow_events.sql"),
    },
    Migration {
        version: 10,
        name: "workflow_event_hash_chain",
        sql: include_str!("../migrations/0010_workflow_event_hash_chain.sql"),
    },
];

/// Opens (creating if necessary) the local libSQL database at `path`.
///
/// Creates missing parent directories, enables foreign-key enforcement for
/// the connection, and returns an open [`Connection`].
///
/// # Errors
///
/// Returns an error if the parent directory cannot be created, the database
/// cannot be opened, or the connection cannot be established.
pub async fn connect(path: impl AsRef<Path>) -> Result<Connection> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| DbError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let db = Builder::new_local(path)
        .build()
        .await
        .map_err(|source| DbError::Connect {
            path: path.to_path_buf(),
            source,
        })?;
    let conn = db.connect().map_err(|source| DbError::Connect {
        path: path.to_path_buf(),
        source,
    })?;
    // Foreign keys are off by default in every SQLite session; without this
    // the REFERENCES clauses in the schema are never enforced.
    conn.execute("PRAGMA foreign_keys = ON", Params::None)
        .await?;
    Ok(conn)
}

/// Snapshot of migration alignment between a state database and this binary's
/// embedded catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MigrationSkew {
    /// Highest migration version recorded in the database; [`None`] when the
    /// database exists but has never been migrated (no tracking table).
    pub applied_max: Option<i64>,
    /// Highest version in this binary's embedded catalog.
    pub known_max: i64,
}

impl MigrationSkew {
    /// The database was written by a newer harness than this binary; the
    /// downgrade guard will refuse to touch it.
    #[must_use]
    pub fn is_future(&self) -> bool {
        self.applied_max
            .is_some_and(|applied| applied > self.known_max)
    }

    /// This binary ships migrations the database has not yet applied.
    #[must_use]
    pub fn is_pending(&self) -> bool {
        self.applied_max
            .is_none_or(|applied| applied < self.known_max)
    }
}

/// Highest version in the embedded migration catalog.
fn known_max_version() -> i64 {
    MIGRATIONS
        .iter()
        .map(|migration| migration.version)
        .max()
        .unwrap_or(0)
}

/// Reads applied vs. known migration versions without mutating anything.
///
/// Unlike [`migrate`], this never writes and never fails on a database from a
/// newer binary; it returns the raw versions so diagnostics (e.g. `doctor`)
/// can classify skew before any persistence command hits the downgrade guard.
///
/// # Errors
///
/// Returns an error when the tracking-table probe or version query fails.
pub async fn inspect_migrations(conn: &Connection) -> Result<MigrationSkew> {
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM sqlite_master \
             WHERE type = 'table' AND name = 'schema_migrations'",
            Params::None,
        )
        .await?;
    let tracked = match rows.next().await? {
        Some(row) => row.get::<i64>(0)? > 0,
        None => false,
    };
    if !tracked {
        return Ok(MigrationSkew {
            applied_max: None,
            known_max: known_max_version(),
        });
    }
    let applied = applied_versions(conn).await?;
    Ok(MigrationSkew {
        applied_max: applied.iter().copied().max(),
        known_max: known_max_version(),
    })
}

/// Applies all pending embedded migrations to `conn` in ascending version order.
///
/// Tracked via a `schema_migrations(version, name, applied_at)` table so each
/// migration runs exactly once.
///
/// # Errors
///
/// Returns an error if the tracking table cannot be created, the database was
/// written by a newer binary (fail-fast instead of silently diverging), or
/// any pending migration fails to apply.
pub async fn migrate(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_migrations (\
         version INTEGER PRIMARY KEY,\
         name TEXT NOT NULL,\
         applied_at INTEGER NOT NULL\
         )",
        Params::None,
    )
    .await?;

    let applied = applied_versions(conn).await?;
    let newest_catalog = known_max_version();
    if let Some(future) = applied.iter().filter(|v| **v > newest_catalog).max() {
        return Err(DbError::FutureDatabase {
            applied: *future,
            known_max: newest_catalog,
        });
    }
    for migration in MIGRATIONS {
        if !applied.contains(&migration.version) {
            apply_migration(conn, migration).await?;
        }
    }
    Ok(())
}

/// Connects to the agent-state database under `root` and applies migrations.
///
/// # Errors
///
/// Returns an error if connecting or migrating fails.
pub async fn connect_and_migrate(root: &Path) -> Result<Connection> {
    let conn = connect(crate::root::db_path(root)).await?;
    migrate(&conn).await?;
    Ok(conn)
}

/// Returns the set of migration versions already applied.
async fn applied_versions(conn: &Connection) -> Result<Vec<i64>> {
    let mut rows = conn
        .query("SELECT version FROM schema_migrations", Params::None)
        .await?;
    let mut versions = Vec::new();
    while let Some(row) = rows.next().await? {
        versions.push(row.get::<i64>(0)?);
    }
    Ok(versions)
}

/// Applies a single migration within a transaction.
async fn apply_migration(conn: &Connection, migration: &Migration) -> Result<()> {
    let tx = conn.transaction().await?;
    tx.execute_batch(migration.sql).await?;
    if migration.version == 10 {
        backfill_workflow_events_hash_chain(&tx).await?;
    }
    tx.execute(
        "INSERT INTO schema_migrations (version, name, applied_at) VALUES (?1, ?2, ?3)",
        libsql::params!(migration.version, migration.name, unix_now()),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Backfills `seq` and `chain_hash` for any unchained rows in `workflow_events`.
async fn backfill_workflow_events_hash_chain(tx: &libsql::Transaction) -> Result<()> {
    let mut rows = tx
        .query(
            "SELECT seq, chain_hash FROM workflow_events WHERE seq IS NOT NULL ORDER BY seq DESC LIMIT 1",
            Params::None,
        )
        .await?;
    let (mut last_seq, mut prev_hash): (i64, Option<String>) = match rows.next().await? {
        Some(row) => (row.get(0)?, row.get(1)?),
        None => (0, None),
    };

    let mut unchained = tx
        .query(
            "SELECT id, payload FROM workflow_events WHERE seq IS NULL ORDER BY id ASC",
            Params::None,
        )
        .await?;

    let mut pending_updates = Vec::new();
    while let Some(row) = unchained.next().await? {
        let id: i64 = row.get(0)?;
        let raw_payload: String = row.get(1)?;
        let canonical_payload = crate::repo_workflow::canonicalize_json(&raw_payload)?;
        last_seq += 1;
        let hash = crate::repo_workflow::chain_hash(prev_hash.as_deref(), &canonical_payload);
        prev_hash = Some(hash.clone());
        pending_updates.push((id, last_seq, hash, canonical_payload));
    }

    for (id, seq, hash, canonical_payload) in pending_updates {
        tx.execute(
            "UPDATE workflow_events SET seq = ?1, chain_hash = ?2, payload = ?3 WHERE id = ?4",
            libsql::params!(seq, hash, canonical_payload, id),
        )
        .await?;
    }

    Ok(())
}

/// Current unix time in seconds.
///
/// Never panics: a pre-epoch system clock or an out-of-range value degrades
/// to `0` rather than aborting a verify run that is only trying to record a
/// timestamp.
#[must_use]
pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| i64::try_from(duration.as_secs()).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn migration_catalog_is_strictly_ascending() {
        assert!(!MIGRATIONS.is_empty());
        for pair in MIGRATIONS.windows(2) {
            assert!(
                pair[0].version < pair[1].version,
                "catalog out of order: {} ({}) then {} ({})",
                pair[0].version,
                pair[0].name,
                pair[1].version,
                pair[1].name
            );
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn migrate_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let conn = connect(dir.path().join("state.db")).await.unwrap();
        migrate(&conn).await.unwrap();
        migrate(&conn).await.unwrap();
        let versions = applied_versions(&conn).await.unwrap();
        assert_eq!(versions.len(), MIGRATIONS.len());
    }

    /// A database written by a newer harness fails fast instead of silently
    /// running with a diverged schema.
    #[tokio::test(flavor = "current_thread")]
    async fn migrate_rejects_database_from_newer_binary() {
        let dir = tempfile::tempdir().unwrap();
        let conn = connect(dir.path().join("state.db")).await.unwrap();
        migrate(&conn).await.unwrap();
        conn.execute(
            "INSERT INTO schema_migrations (version, name, applied_at) VALUES (9999, 'future', 0)",
            Params::None,
        )
        .await
        .unwrap();

        let err = migrate(&conn).await.unwrap_err();
        assert!(matches!(err, DbError::FutureDatabase { applied: 9999, .. }));
    }

    /// A fresh connection has no tracking table, so inspection reports an
    /// uninitialized database instead of erroring.
    #[tokio::test(flavor = "current_thread")]
    async fn inspect_reports_uninitialized_database() {
        let dir = tempfile::tempdir().unwrap();
        let conn = connect(dir.path().join("state.db")).await.unwrap();

        let skew = inspect_migrations(&conn).await.unwrap();

        assert_eq!(skew.applied_max, None);
        assert!(!skew.is_future());
        assert!(skew.is_pending());
    }

    /// Inspection is read-only: it classifies a future database without the
    /// downgrade guard aborting, so diagnostics can explain the skew.
    #[tokio::test(flavor = "current_thread")]
    async fn inspect_classifies_future_and_current_databases() {
        let dir = tempfile::tempdir().unwrap();
        let conn = connect(dir.path().join("state.db")).await.unwrap();
        migrate(&conn).await.unwrap();

        let current = inspect_migrations(&conn).await.unwrap();
        assert_eq!(current.applied_max, Some(known_max_version()));
        assert!(!current.is_future());
        assert!(!current.is_pending());

        conn.execute(
            "INSERT INTO schema_migrations (version, name, applied_at) VALUES (9999, 'future', 0)",
            Params::None,
        )
        .await
        .unwrap();

        let future = inspect_migrations(&conn).await.unwrap();
        assert_eq!(future.applied_max, Some(9999));
        assert!(future.is_future());
        assert!(!future.is_pending());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn connect_and_migrate_creates_state_db() {
        let dir = tempfile::tempdir().unwrap();
        let conn = connect_and_migrate(dir.path()).await.unwrap();
        assert!(crate::root::db_path(dir.path()).exists());
        drop(conn);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn migration_backfills_existing_unchained_events() {
        let dir = tempfile::tempdir().unwrap();
        let conn = connect(crate::root::db_path(dir.path())).await.unwrap();

        conn.execute(
            "CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY, name TEXT NOT NULL, applied_at INTEGER NOT NULL)",
            Params::None,
        )
        .await
        .unwrap();

        for v in 1..=9 {
            let m = MIGRATIONS.iter().find(|m| m.version == v).unwrap();
            let tx = conn.transaction().await.unwrap();
            tx.execute_batch(m.sql).await.unwrap();
            tx.execute(
                "INSERT INTO schema_migrations (version, name, applied_at) VALUES (?1, ?2, 0)",
                libsql::params!(m.version, m.name),
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
        }

        conn.execute(
            "INSERT INTO tasks (id, title, status, subtask_index, created_at, updated_at) VALUES (1, 't', 'pending', 0, 100, 100)",
            Params::None,
        )
        .await
        .unwrap();

        conn.execute(
            "INSERT INTO workflow_events (task_id, kind, payload, created_at) VALUES (1, 'TaskAdded', '{\"b\":2,\"a\":1}', 100)",
            Params::None,
        )
        .await
        .unwrap();

        migrate(&conn).await.unwrap();

        let rows = crate::repo_workflow::list_events_ascending(&conn)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].seq, 1);
        assert_eq!(rows[0].canonical_payload, "{\"a\":1,\"b\":2}");
        let expected_hash = crate::repo_workflow::chain_hash(None, "{\"a\":1,\"b\":2}");
        assert_eq!(rows[0].chain_hash.as_deref(), Some(expected_hash.as_str()));
    }
}

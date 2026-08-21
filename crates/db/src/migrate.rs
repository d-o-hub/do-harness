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
    let newest_catalog = MIGRATIONS
        .iter()
        .map(|migration| migration.version)
        .max()
        .unwrap_or(0);
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
    tx.execute(
        "INSERT INTO schema_migrations (version, name, applied_at) VALUES (?1, ?2, ?3)",
        libsql::params!(migration.version, migration.name, unix_now()),
    )
    .await?;
    tx.commit().await?;
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

    #[tokio::test(flavor = "current_thread")]
    async fn connect_and_migrate_creates_state_db() {
        let dir = tempfile::tempdir().unwrap();
        let conn = connect_and_migrate(dir.path()).await.unwrap();
        assert!(crate::root::db_path(dir.path()).exists());
        drop(conn);
    }
}

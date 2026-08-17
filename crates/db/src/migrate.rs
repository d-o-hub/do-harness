//! Connection and migration management for the agent-state libSQL store.

use std::path::Path;

use anyhow::Result;
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
];

/// Opens (creating if necessary) the local libSQL database at `path`.
///
/// Creates missing parent directories and returns an open [`Connection`].
///
/// # Errors
///
/// Returns an error if the parent directory cannot be created, the database
/// cannot be opened, or the connection cannot be established.
pub async fn connect(path: impl AsRef<Path>) -> Result<Connection> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let db = Builder::new_local(path).build().await?;
    Ok(db.connect()?)
}

/// Applies all pending embedded migrations to `conn` in ascending version order.
///
/// Tracked via a `schema_migrations(version, name, applied_at)` table so each
/// migration runs exactly once.
///
/// # Errors
///
/// Returns an error if the tracking table cannot be created or any pending
/// migration fails to apply.
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
/// # Panics
///
/// Panics if the system clock is before the unix epoch or the value does not
/// fit in `i64`.
#[must_use]
pub fn unix_now() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_secs(),
    )
    .expect("unix time fits in i64")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn migrate_is_idempotent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let conn = connect(dir.path().join("state.db")).await.expect("connect");
        migrate(&conn).await.expect("first migrate");
        migrate(&conn).await.expect("second migrate");
        let versions = applied_versions(&conn).await.expect("versions");
        assert_eq!(versions.len(), MIGRATIONS.len());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn connect_and_migrate_creates_state_db() {
        let dir = tempfile::tempdir().expect("tempdir");
        let conn = connect_and_migrate(dir.path())
            .await
            .expect("connect+migrate");
        assert!(crate::root::db_path(dir.path()).exists());
        drop(conn);
    }
}

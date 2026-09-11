//! Connection and migration management for the agent-state libSQL store.

use std::path::Path;

use crate::error::{DbError, Result};
use crate::migrate_catalog::{MIGRATIONS, Migration};
use libsql::{Builder, Connection, params::Params};

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

/// Returns the number of applied schema migrations.
///
/// # Errors
///
/// Returns an error when the tracking-table probe fails.
pub async fn count_migrations(conn: &Connection) -> Result<i64> {
    let mut rows = conn
        .query("SELECT COUNT(*) FROM schema_migrations", Params::None)
        .await?;
    match rows.next().await? {
        Some(row) => Ok(row.get(0)?),
        None => Ok(0),
    }
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
        backfill_workflow_event_chain(&tx).await?;
    }
    tx.execute(
        "INSERT INTO schema_migrations (version, name, applied_at) VALUES (?1, ?2, ?3)",
        libsql::params!(migration.version, migration.name, unix_now()),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Backfills `seq` and `chain_hash` for existing unchained rows in `workflow_events`.
async fn backfill_workflow_event_chain(conn: &Connection) -> Result<()> {
    let mut rows = conn
        .query(
            "SELECT seq, chain_hash FROM workflow_events WHERE seq IS NOT NULL ORDER BY seq DESC LIMIT 1",
            Params::None,
        )
        .await?;
    let (mut last_seq, mut prev_hash): (i64, Option<String>) = match rows.next().await? {
        Some(row) => (row.get(0)?, row.get(1)?),
        None => (0, None),
    };
    drop(rows);

    let mut unchained_rows = conn
        .query(
            "SELECT id, payload FROM workflow_events WHERE seq IS NULL ORDER BY id ASC",
            Params::None,
        )
        .await?;
    let mut unchained = Vec::new();
    while let Some(row) = unchained_rows.next().await? {
        let id: i64 = row.get(0)?;
        let payload: String = row.get(1)?;
        unchained.push((id, payload));
    }
    drop(unchained_rows);

    for (id, payload) in unchained {
        let canonical = crate::repo_workflow::canonical_payload(&payload)?;
        last_seq += 1;
        let hash = crate::repo_workflow::chain_hash(prev_hash.as_deref(), &canonical);
        conn.execute(
            "UPDATE workflow_events SET seq = ?1, chain_hash = ?2, payload = ?3 WHERE id = ?4",
            libsql::params!(last_seq, hash.as_str(), canonical.as_str(), id),
        )
        .await?;
        prev_hash = Some(hash);
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
mod tests;

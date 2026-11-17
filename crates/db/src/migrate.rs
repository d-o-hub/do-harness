//! Connection and migration management for the agent-state libSQL store.

use std::path::Path;

use crate::error::{DbError, Result};
use crate::migrate_catalog::MIGRATIONS;
use libsql::{Builder, Connection, OpenFlags, params::Params};

mod backup;
mod checksum;

use backup::backup_state_file;
use checksum::{
    BUSY_RETRY_ATTEMPTS, applied_checksums, applied_versions, apply_migration,
    ensure_checksum_column, migrate_with_retry, verify_migration_checksum,
};

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
    // Concurrent `verify --record` writers hit SQLITE_BUSY on the default
    // delete journal; WAL lets readers proceed during a write while
    // busy_timeout turns a transient lock into a short wait instead of an
    // immediate error. busy_timeout is set BEFORE switching journal_mode to
    // WAL so that connection setup itself tolerates transient lock contention
    // during initial WAL setup. synchronous=NORMAL stays durable under WAL
    // for this workload (checkpoint coordination preserves crash safety).
    // NOTE: PRAGMA assignments can return the new value as a row, and
    // `execute` rejects row-returning SQL, so every PRAGMA goes through
    // `query` with drained rows.
    for pragma in [
        "PRAGMA busy_timeout = 10000",
        "PRAGMA journal_mode = WAL",
        "PRAGMA synchronous = NORMAL",
    ] {
        let mut rows = conn.query(pragma, Params::None).await?;
        while rows.next().await?.is_some() {}
    }
    Ok(conn)
}

/// Opens an existing local libSQL database read-only.
///
/// Unlike [`connect`], this never creates the file, never changes the journal
/// mode, and sets `query_only` so a projection cannot mutate state by reading
/// it. A missing database is an error; callers decide how to report it.
///
/// # Errors
///
/// Returns an error when the database cannot be opened read-only or the
/// read-only session cannot be configured.
pub async fn connect_read_only(path: impl AsRef<Path>) -> Result<Connection> {
    let path = path.as_ref();
    let db = Builder::new_local(path)
        .flags(OpenFlags::SQLITE_OPEN_READ_ONLY)
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
    // Both pragmas are connection-local: no journal-mode change, no write.
    for pragma in ["PRAGMA busy_timeout = 10000", "PRAGMA query_only = ON"] {
        let mut rows = conn.query(pragma, Params::None).await?;
        while rows.next().await?.is_some() {}
    }
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
    /// The database has user tables but no `schema_migrations` tracking table:
    /// a legacy/foreign database rather than a freshly created empty file.
    pub legacy: bool,
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
/// Unlike [`migrate()`], this never writes and never fails on a database from a
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
        let mut tables = conn
            .query(
                "SELECT COUNT(*) FROM sqlite_master \
                 WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
                Params::None,
            )
            .await?;
        let legacy = match tables.next().await? {
            Some(row) => row.get::<i64>(0)? > 0,
            None => false,
        };
        return Ok(MigrationSkew {
            applied_max: None,
            known_max: known_max_version(),
            legacy,
        });
    }
    let applied = applied_versions(conn).await?;
    Ok(MigrationSkew {
        applied_max: applied.iter().copied().max(),
        known_max: known_max_version(),
        legacy: false,
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
/// Tracked via a `schema_migrations(version, name, applied_at, checksum)`
/// table so each migration runs exactly once. The SHA-256 hex of each
/// migration's SQL is recorded at apply time and re-verified on every startup:
/// a migration file edited after release fails closed instead of silently
/// diverging existing versus fresh databases.
///
/// # Errors
///
/// Returns an error if the tracking table cannot be created, the database was
/// written by a newer binary (fail-fast instead of silently diverging), a
/// previously applied migration no longer matches the embedded catalog, or
/// any pending migration fails to apply.
pub async fn migrate(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_migrations (\
         version INTEGER PRIMARY KEY,\
         name TEXT NOT NULL,\
         applied_at INTEGER NOT NULL,\
         checksum TEXT\
         )",
        Params::None,
    )
    .await?;
    ensure_checksum_column(conn).await?;

    let applied = applied_versions(conn).await?;
    let newest_catalog = known_max_version();
    if let Some(future) = applied.iter().filter(|v| **v > newest_catalog).max() {
        return Err(DbError::FutureDatabase {
            applied: *future,
            known_max: newest_catalog,
        });
    }
    let stored = applied_checksums(conn).await?;
    for migration in MIGRATIONS {
        match stored
            .iter()
            .find(|(version, _)| *version == migration.version)
        {
            Some((_, existing)) => {
                verify_migration_checksum(conn, migration, existing.clone()).await?;
            }
            None => {
                apply_migration(conn, migration).await?;
            }
        }
    }
    Ok(())
}

/// Connects to the agent-state database under `root` and applies migrations.
///
/// When pending migrations exist, the database is first snapshotted to
/// `agent_state.db.bak` so an interrupted or destructive migration (`0006`
/// deletes rows, `0011` rebuilds tables) can be recovered manually. A snapshot
/// that cannot be written aborts the migration rather than proceeding without
/// its recovery copy.
///
/// # Errors
///
/// Returns an error if connecting, backing up, or migrating fails.
pub async fn connect_and_migrate(root: &Path) -> Result<Connection> {
    crate::error::retry_on_busy(BUSY_RETRY_ATTEMPTS, move || {
        let root = root.to_path_buf();
        async move { connect_and_migrate_once(&root).await }
    })
    .await
}

/// Single attempt body for [`connect_and_migrate`], retried as a unit when the
/// connection or migration probe hits transient lock contention.
async fn connect_and_migrate_once(root: &Path) -> Result<Connection> {
    crate::root::validate_state_paths(root)?;
    let path = crate::root::db_path(root);
    let conn = connect(&path).await?;
    let skew = inspect_migrations(&conn).await?;
    if skew
        .applied_max
        .is_some_and(|applied| applied < skew.known_max)
    {
        backup_state_file(&conn, &path).await?;
    }
    migrate_with_retry(&conn).await?;
    Ok(conn)
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

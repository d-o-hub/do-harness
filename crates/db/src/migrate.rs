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

/// Attempts allowed against a `SQLITE_BUSY` database before giving up.
///
/// Both migrators share this policy: a connect that loses to a concurrent
/// migrator and a migration that hits a duplicate `CREATE`/`ALTER`.
const BUSY_RETRY_ATTEMPTS: usize = 5;

/// Connects to the agent-state database under `root` and applies migrations.
///
/// When pending migrations exist, the database file is first copied to
/// `agent_state.db.bak` (best-effort snapshot) so an interrupted or
/// destructive migration (`0006` deletes rows, `0011` rebuilds tables) can be
/// recovered manually.
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

/// Applies migrations, tolerating a concurrent migrator.
///
/// Two processes can pass the pending check together; the loser then fails on
/// a duplicate `CREATE`/`ALTER` mid-migration (rolled back atomically). If the
/// catalog is fully applied afterwards, the loser simply succeeds.
///
/// # Errors
///
/// Returns the last migration error when the catalog is still not applied.
async fn migrate_with_retry(conn: &Connection) -> Result<()> {
    let mut last: Option<DbError> = None;
    for attempt in 0..BUSY_RETRY_ATTEMPTS {
        match migrate(conn).await {
            Ok(()) => return Ok(()),
            Err(err) if err.is_busy() && attempt + 1 < BUSY_RETRY_ATTEMPTS => {
                last = Some(err);
                tokio::task::yield_now().await;
            }
            Err(err) => {
                // A checksum or downgrade failure means the database content
                // genuinely disagrees with this binary: a concurrent migrator
                // finishing the version list must not swallow it into success.
                if matches!(
                    err,
                    DbError::MigrationDiverged { .. } | DbError::FutureDatabase { .. }
                ) {
                    return Err(err);
                }
                let skew = inspect_migrations(conn).await?;
                if skew.applied_max == Some(skew.known_max) {
                    return Ok(());
                }
                return Err(err);
            }
        }
    }
    Err(last.unwrap_or_else(|| DbError::Invariant("migration retry state lost".to_owned())))
}

/// Copies the state database beside itself as `<file>.bak`.
///
/// The connection runs in WAL mode (see [`connect`]), so committed pages can
/// still live in the `-wal` sidecar at the moment of the copy; a plain file
/// copy would then omit them, or capture a torn state mid-write. Checkpoint
/// first so the snapshot is complete — this backup is the recovery path for
/// the destructive migrations (`0006` deletes rows, `0011` rebuilds tables).
/// The checkpoint is best-effort: a concurrent reader can keep it from
/// completing, and a slightly stale snapshot still beats refusing to migrate.
async fn backup_state_file(conn: &Connection, path: &Path) -> Result<()> {
    // A checkpoint returns rows, and `execute` rejects row-returning SQL, so
    // route it through `query` with drained rows (see the `connect` note).
    if let Ok(mut rows) = conn
        .query("PRAGMA wal_checkpoint(TRUNCATE)", Params::None)
        .await
    {
        while rows.next().await.ok().flatten().is_some() {}
    }
    let backup = path.with_extension("db.bak");
    std::fs::copy(path, &backup).map_err(|source| DbError::Backup {
        path: backup,
        source,
    })?;
    Ok(())
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

/// `schema_migrations` column holding the SHA-256 hex of the applied SQL.
const CHECKSUM_COLUMN: &str = "checksum";

/// Adds the checksum column to databases created before it existed.
///
/// Pre-checksum rows keep a `NULL` checksum, which the verifier adopts (see
/// [`verify_migration_checksum`]); only a recorded non-`NULL` value that
/// disagrees with the catalog fails.
async fn ensure_checksum_column(conn: &Connection) -> Result<()> {
    let mut rows = conn
        .query("PRAGMA table_info(schema_migrations)", Params::None)
        .await?;
    let mut has_checksum = false;
    while let Some(row) = rows.next().await? {
        let name: String = row.get(1)?;
        if name == CHECKSUM_COLUMN {
            has_checksum = true;
        }
    }
    drop(rows);
    if !has_checksum {
        conn.execute(
            "ALTER TABLE schema_migrations ADD COLUMN checksum TEXT",
            Params::None,
        )
        .await?;
    }
    Ok(())
}

/// Returns each applied `(version, recorded checksum)` pair.
///
/// Callers must run [`ensure_checksum_column`] first so the `checksum` column
/// exists on databases created before checksums were recorded.
async fn applied_checksums(conn: &Connection) -> Result<Vec<(i64, Option<String>)>> {
    let mut rows = conn
        .query(
            "SELECT version, checksum FROM schema_migrations",
            Params::None,
        )
        .await?;
    let mut applied = Vec::new();
    while let Some(row) = rows.next().await? {
        applied.push((row.get::<i64>(0)?, row.get::<Option<String>>(1)?));
    }
    Ok(applied)
}

/// Verifies one already-applied migration against the embedded catalog.
///
/// A `NULL` recorded checksum predates checksum tracking, so the current
/// catalog value is adopted in place; a recorded value that disagrees means
/// the migration file was edited after this database applied it, and startup
/// fails closed.
async fn verify_migration_checksum(
    conn: &Connection,
    migration: &Migration,
    stored: Option<String>,
) -> Result<()> {
    let expected = migration.checksum();
    match stored {
        None => {
            conn.execute(
                "UPDATE schema_migrations SET checksum = ?1 WHERE version = ?2",
                libsql::params!(expected, migration.version),
            )
            .await?;
            Ok(())
        }
        Some(found) if found == expected => Ok(()),
        Some(found) => Err(DbError::MigrationDiverged {
            version: migration.version,
            name: migration.name.to_owned(),
            expected,
            found,
        }),
    }
}

/// Applies a single migration within a transaction.
async fn apply_migration(conn: &Connection, migration: &Migration) -> Result<()> {
    let checksum = migration.checksum();
    let tx = crate::tx::begin_immediate(conn).await?;
    tx.execute_batch(migration.sql).await?;
    if migration.version == 10 {
        // `&Transaction` derefs to `&Connection`, so the backfill shares the
        // migration transaction and commits or rolls back with it.
        backfill_workflow_event_chain(&tx).await?;
    }
    tx.execute(
        "INSERT INTO schema_migrations (version, name, applied_at, checksum) VALUES (?1, ?2, ?3, ?4)",
        libsql::params!(migration.version, migration.name, unix_now(), checksum),
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

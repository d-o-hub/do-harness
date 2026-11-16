//! Migration retry and checksum verification for the agent-state store.
//!
//! Extracted from `migrate.rs` so connection setup and the table catalog stay
//! under the 500-line ceiling; the split is move-only.

use libsql::{Connection, params::Params};

use crate::error::{DbError, Result};
use crate::migrate_catalog::Migration;

use super::{inspect_migrations, migrate, unix_now};

/// Attempts allowed against a `SQLITE_BUSY` database before giving up.
///
/// Both migrators share this policy: a connect that loses to a concurrent
/// migrator and a migration that hits a duplicate `CREATE`/`ALTER`.
pub(super) const BUSY_RETRY_ATTEMPTS: usize = 5;

/// Applies migrations, tolerating a concurrent migrator.
///
/// Two processes can pass the pending check together; the loser then fails on
/// a duplicate `CREATE`/`ALTER` mid-migration (rolled back atomically). If the
/// catalog is fully applied afterwards, the loser simply succeeds.
///
/// # Errors
///
/// Returns the last migration error when the catalog is still not applied.
pub(super) async fn migrate_with_retry(conn: &Connection) -> Result<()> {
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

/// Returns the set of migration versions already applied.
pub(super) async fn applied_versions(conn: &Connection) -> Result<Vec<i64>> {
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
pub(super) async fn ensure_checksum_column(conn: &Connection) -> Result<()> {
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
pub(super) async fn applied_checksums(conn: &Connection) -> Result<Vec<(i64, Option<String>)>> {
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
pub(super) async fn verify_migration_checksum(
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
pub(super) async fn apply_migration(conn: &Connection, migration: &Migration) -> Result<()> {
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

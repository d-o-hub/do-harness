//! Connection and schema management for the do-harness local libSQL store.

use std::path::{Path, PathBuf};

use anyhow::Result;
use do_harness_types::DecisionHeader;
use libsql::{Builder, Connection, params::Params};

/// Name of the harness config file that marks a workspace root.
pub const CONFIG_FILENAME: &str = "do-harness.toml";

/// Name of the agent contract file used as a secondary root marker.
pub const AGENTS_FILENAME: &str = "AGENTS.md";

/// Name of the local state directory inside a workspace root.
const STATE_DIR: &str = ".do-harness";

/// Relative path of the invariant seed file inside a workspace root.
const INVARIANTS_RELATIVE_PATH: &str = "plans/invariants.json";

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
];

/// Returns the do-harness workspace root containing `start`.
///
/// Walks up from `start` and treats a directory as a root when it contains
/// `do-harness.toml`, or contains `AGENTS.md` together with either
/// `.do-harness/` or `plans/invariants.json`.
///
/// # Errors
///
/// Returns an error when no root marker is found in `start` or any ancestor.
pub fn find_harness_root(start: &Path) -> Result<PathBuf> {
    for dir in start.ancestors() {
        if is_harness_root(dir) {
            return Ok(dir.to_path_buf());
        }
    }
    anyhow::bail!(
        "harness root not found: no {CONFIG_FILENAME} (or {AGENTS_FILENAME} with plans/invariants.json) \
         under {}",
        start.display()
    );
}

/// Whether `dir` carries a harness root marker.
fn is_harness_root(dir: &Path) -> bool {
    if dir.join(CONFIG_FILENAME).exists() {
        return true;
    }
    let agent_contract = dir.join(AGENTS_FILENAME);
    agent_contract.exists()
        && (dir.join(STATE_DIR).exists() || dir.join(INVARIANTS_RELATIVE_PATH).exists())
}

/// Default path of the agent-state database under `root`.
///
/// Resolved to `<root>/.do-harness/agent_state.db`.
#[must_use]
pub fn db_path(root: &Path) -> PathBuf {
    root.join(STATE_DIR).join("agent_state.db")
}

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
    let conn = connect(db_path(root)).await?;
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
fn unix_now() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_secs(),
    )
    .expect("unix time fits in i64")
}

/// Upserts a collection of decision headers into the `invariants` table.
///
/// Existing invariants are matched on their `invariant` text and updated;
/// new ones are inserted. Returns the number of invariants written.
///
/// # Errors
///
/// Returns an error if any upsert statement fails.
pub async fn seed_invariants(conn: &Connection, headers: &[DecisionHeader]) -> Result<usize> {
    let now = unix_now();
    let mut written = 0;
    for header in headers {
        conn.execute(
            "INSERT INTO invariants (invariant, rationale, sensor, category, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5) \
             ON CONFLICT(invariant) DO UPDATE SET \
               rationale = excluded.rationale, \
               sensor = excluded.sensor, \
               category = excluded.category",
            libsql::params!(
                header.invariant.as_str(),
                header.rationale.as_str(),
                header.sensor.as_str(),
                header.category.as_str(),
                now
            ),
        )
        .await?;
        written += 1;
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test(flavor = "current_thread")]
    async fn migrate_is_idempotent() {
        let dir = tempdir().expect("tempdir");
        let conn = connect(dir.path().join("state.db")).await.expect("connect");
        migrate(&conn).await.expect("first migrate");
        migrate(&conn).await.expect("second migrate");
        let versions = applied_versions(&conn).await.expect("versions");
        assert_eq!(versions.len(), MIGRATIONS.len());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn connect_and_migrate_creates_state_db() {
        let dir = tempdir().expect("tempdir");
        let conn = connect_and_migrate(dir.path())
            .await
            .expect("connect+ migrate");
        assert!(db_path(dir.path()).exists());
        drop(conn);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn seed_invariants_upserts_on_invariant_text() {
        let dir = tempdir().expect("tempdir");
        let conn = connect_and_migrate(dir.path())
            .await
            .expect("connect+migrate");
        let first = vec![DecisionHeader::new(
            "inv".into(),
            "r1".into(),
            "s1".into(),
            "contracts".into(),
        )];
        seed_invariants(&conn, &first).await.expect("seed 1");
        let second = vec![DecisionHeader::new(
            "inv".into(),
            "r2".into(),
            "s2".into(),
            "contracts".into(),
        )];
        seed_invariants(&conn, &second).await.expect("seed 2");

        let mut rows = conn
            .query("SELECT COUNT(*) FROM invariants", Params::None)
            .await
            .expect("count query");
        let n: i64 = rows
            .next()
            .await
            .expect("count row")
            .expect("some row")
            .get(0)
            .expect("count col");
        assert_eq!(n, 1);

        let mut rows = conn
            .query(
                "SELECT rationale FROM invariants WHERE invariant = 'inv'",
                Params::None,
            )
            .await
            .expect("select query");
        let rationale: String = rows
            .next()
            .await
            .expect("select row")
            .expect("some row")
            .get(0)
            .expect("rationale col");
        assert_eq!(rationale, "r2");
    }

    #[test]
    fn find_harness_root_detects_config_marker() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("do-harness.toml"), "language = \"rust\"\n").expect("write");
        let nested = dir.path().join("a").join("b");
        fs::create_dir_all(&nested).expect("mkdir");
        let found = find_harness_root(&nested).expect("root");
        assert_eq!(found, dir.path());
    }

    #[test]
    fn find_harness_root_requires_agents_with_state_or_plans() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("AGENTS.md"), "# x\n").expect("write");
        let nested = dir.path().join("sub");
        fs::create_dir_all(&nested).expect("mkdir");
        assert!(find_harness_root(&nested).is_err());

        fs::create_dir_all(dir.path().join("plans")).expect("mkdir plans");
        fs::write(dir.path().join("plans/invariants.json"), "[]").expect("write plans");
        assert!(find_harness_root(&nested).is_ok());
    }

    #[test]
    fn find_harness_root_rejects_plain_directory() {
        let dir = tempdir().expect("tempdir");
        assert!(find_harness_root(dir.path()).is_err());
    }
}

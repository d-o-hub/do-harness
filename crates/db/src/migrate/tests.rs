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
    assert_eq!(rows[0].chain_hash.as_str(), expected_hash.as_str());
}

/// Concurrent `verify --record` writers must not hit `SQLITE_BUSY` on the
/// default delete journal: connections open in WAL with a busy timeout
/// (#37). `synchronous` reads back numeric (NORMAL = 1).
#[tokio::test(flavor = "current_thread")]
async fn connect_enables_wal_concurrency_pragmas() {
    let dir = tempfile::tempdir().unwrap();
    let conn = connect(dir.path().join("state.db")).await.unwrap();
    assert_eq!(pragma_text(&conn, "PRAGMA journal_mode").await, "wal");
    assert_eq!(pragma_int(&conn, "PRAGMA busy_timeout").await, 10000);
    assert_eq!(pragma_int(&conn, "PRAGMA synchronous").await, 1);
}

async fn pragma_text(conn: &Connection, sql: &str) -> String {
    let mut rows = conn.query(sql, Params::None).await.unwrap();
    rows.next()
        .await
        .unwrap()
        .unwrap()
        .get::<String>(0)
        .unwrap()
}

async fn pragma_int(conn: &Connection, sql: &str) -> i64 {
    let mut rows = conn.query(sql, Params::None).await.unwrap();
    rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap()
}

/// Every applied migration records the SHA-256 of its SQL, matching the
/// embedded catalog, so a later edit of a migration file is detectable.
#[tokio::test(flavor = "current_thread")]
async fn migrate_records_migration_checksums() {
    let dir = tempfile::tempdir().unwrap();
    let conn = connect(dir.path().join("state.db")).await.unwrap();
    migrate(&conn).await.unwrap();

    let mut rows = conn
        .query(
            "SELECT version, checksum FROM schema_migrations ORDER BY version",
            Params::None,
        )
        .await
        .unwrap();
    let mut seen = 0;
    while let Some(row) = rows.next().await.unwrap() {
        let version: i64 = row.get(0).unwrap();
        let checksum: Option<String> = row.get(1).unwrap();
        let migration = MIGRATIONS
            .iter()
            .find(|m| m.version == version)
            .expect("applied version from catalog");
        assert_eq!(checksum.as_deref(), Some(migration.checksum()).as_deref());
        seen += 1;
    }
    assert_eq!(seen, MIGRATIONS.len());
}

/// A recorded checksum that no longer matches the binary's migration SQL
/// fails closed: the migration file was edited after this database applied
/// it, so existing and fresh databases would silently diverge.
#[tokio::test(flavor = "current_thread")]
async fn migrate_fails_on_diverged_migration_checksum() {
    let dir = tempfile::tempdir().unwrap();
    let conn = connect(dir.path().join("state.db")).await.unwrap();
    migrate(&conn).await.unwrap();

    let tampered = MIGRATIONS.first().expect("catalog is non-empty");
    conn.execute(
        "UPDATE schema_migrations SET checksum = 'deadbeef' WHERE version = ?1",
        libsql::params!(tampered.version),
    )
    .await
    .unwrap();

    let err = migrate(&conn).await.unwrap_err();
    assert!(
        err.to_string().contains("diverged"),
        "error must name the divergence: {err}"
    );
    match err {
        DbError::MigrationDiverged {
            version,
            name,
            expected,
            found,
        } => {
            assert_eq!(version, tampered.version);
            assert_eq!(name, tampered.name);
            assert_eq!(expected, tampered.checksum());
            assert_eq!(found, "deadbeef");
        }
        other => panic!("expected MigrationDiverged, got {other:?}"),
    }
}

/// Rows written before checksum tracking carry `NULL` and are adopted, not
/// rejected: only a recorded value that disagrees with the catalog fails.
#[tokio::test(flavor = "current_thread")]
async fn migrate_adopts_legacy_rows_without_checksums() {
    let dir = tempfile::tempdir().unwrap();
    let conn = connect(dir.path().join("state.db")).await.unwrap();

    conn.execute(
        "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, name TEXT NOT NULL, applied_at INTEGER NOT NULL)",
        Params::None,
    )
    .await
    .unwrap();
    let first = MIGRATIONS.first().expect("catalog is non-empty");
    let setup = conn.transaction().await.unwrap();
    setup.execute_batch(first.sql).await.unwrap();
    setup
        .execute(
            "INSERT INTO schema_migrations (version, name, applied_at) VALUES (?1, ?2, 0)",
            libsql::params!(first.version, first.name),
        )
        .await
        .unwrap();
    setup.commit().await.unwrap();

    migrate(&conn).await.unwrap();

    let mut rows = conn
        .query(
            "SELECT checksum FROM schema_migrations WHERE version = ?1",
            libsql::params!(first.version),
        )
        .await
        .unwrap();
    let row = rows.next().await.unwrap().expect("adopted row");
    let checksum: Option<String> = row.get(0).unwrap();
    assert_eq!(checksum.as_deref(), Some(first.checksum()).as_deref());
}

/// Pending migrations snapshot the database to `agent_state.db.bak` first, and
/// the snapshot must carry committed state that still lives in the WAL sidecar
/// — `VACUUM INTO` inside `backup_state_file` is what makes it complete.
#[tokio::test(flavor = "current_thread")]
async fn backup_state_file_is_wal_consistent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agent_state.db");
    let conn = connect(&path).await.unwrap();
    conn.execute("CREATE TABLE t (x INTEGER)", Params::None)
        .await
        .unwrap();
    conn.execute("INSERT INTO t (x) VALUES (7)", Params::None)
        .await
        .unwrap();

    backup_state_file(&conn, &path).await.unwrap();

    let backup = path.with_extension("db.bak");
    assert!(backup.exists(), "backup file missing");
    // The snapshot is readable and contains the committed row.
    let restored = connect(&backup).await.unwrap();
    let mut rows = restored
        .query("SELECT x FROM t", Params::None)
        .await
        .unwrap();
    let row = rows.next().await.unwrap().expect("row");
    assert_eq!(row.get::<i64>(0).unwrap(), 7);
}

/// FU-01: a reader holding a snapshot keeps `wal_checkpoint(TRUNCATE)` from
/// completing, so committed pages stay in the `-wal` sidecar at copy time. The
/// backup is the recovery path for the destructive migrations (`0006` deletes
/// rows, `0011` rebuilds tables), so it must still contain those pages rather
/// than fall back to a snapshot that silently predates them.
#[tokio::test(flavor = "current_thread")]
async fn backup_state_file_keeps_committed_pages_under_a_blocked_checkpoint() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agent_state.db");
    let writer = connect(&path).await.unwrap();
    writer
        .execute("CREATE TABLE t (x INTEGER)", Params::None)
        .await
        .unwrap();
    writer
        .execute("INSERT INTO t (x) VALUES (1)", Params::None)
        .await
        .unwrap();

    // The concurrent `verify --record` reader: its open snapshot predates the
    // next commit, which pins the WAL and blocks the truncating checkpoint.
    let reader = connect(&path).await.unwrap();
    let snapshot = reader.transaction().await.unwrap();
    let mut held = snapshot
        .query("SELECT COUNT(*) FROM t", Params::None)
        .await
        .unwrap();
    assert!(
        held.next().await.unwrap().is_some(),
        "snapshot must be open"
    );

    writer
        .execute("INSERT INTO t (x) VALUES (2)", Params::None)
        .await
        .unwrap();

    backup_state_file(&writer, &path).await.unwrap();

    let backup = path.with_extension("db.bak");
    let restored = connect(&backup).await.unwrap();
    let mut rows = restored
        .query("SELECT x FROM t ORDER BY x", Params::None)
        .await
        .unwrap();
    let mut seen = Vec::new();
    while let Some(row) = rows.next().await.unwrap() {
        seen.push(row.get::<i64>(0).unwrap());
    }
    assert_eq!(
        seen,
        vec![1, 2],
        "backup dropped committed pages that were still in the WAL"
    );

    let mut integrity = restored
        .query("PRAGMA integrity_check", Params::None)
        .await
        .unwrap();
    let verdict: String = integrity
        .next()
        .await
        .unwrap()
        .expect("integrity row")
        .get(0)
        .unwrap();
    assert_eq!(verdict, "ok", "backup failed integrity validation");
}

/// FU-01: a snapshot that cannot be written must leave the previous backup
/// untouched. Publishing over the live file would destroy the only recovery
/// copy at exactly the moment it is needed.
#[tokio::test(flavor = "current_thread")]
async fn failed_snapshot_preserves_the_previous_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agent_state.db");
    let conn = connect(&path).await.unwrap();
    conn.execute("CREATE TABLE t (x INTEGER)", Params::None)
        .await
        .unwrap();

    backup_state_file(&conn, &path).await.unwrap();
    let backup = path.with_extension("db.bak");
    let previous = std::fs::read(&backup).unwrap();

    // Failure injection that does not depend on the effective user id: a
    // directory where the staging file must go makes the snapshot unwritable.
    let staging = path.with_extension("db.bak.tmp");
    std::fs::create_dir(&staging).unwrap();

    let err = backup_state_file(&conn, &path).await.unwrap_err();
    assert!(
        matches!(err, DbError::Backup { .. } | DbError::Sql(_)),
        "unexpected error: {err}"
    );
    assert_eq!(
        std::fs::read(&backup).unwrap(),
        previous,
        "a failed snapshot must not clobber the previous backup"
    );
}

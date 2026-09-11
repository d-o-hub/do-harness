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
    assert_eq!(pragma_int(&conn, "PRAGMA busy_timeout").await, 5000);
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

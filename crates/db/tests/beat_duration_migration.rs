//! Migration 0024 (`beat_duration`): an existing sensor beat row survives the
//! additive `duration_ms` column, and the export surfaces its legacy timing
//! honestly — derived from the timestamps when they differ, unknown when the
//! row carries only the shared post-run recording stamp.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use do_harness_db::migrate::{connect, migrate, unix_now};
use do_harness_db::migrate_catalog::MIGRATIONS;
use libsql::params::Params;

/// Applies the real catalog through `0023` to a fresh database, so the state
/// under test is the pre-`0024` schema an adopter actually had.
async fn migrate_through_0023(conn: &libsql::Connection) {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_migrations (\
         version INTEGER PRIMARY KEY,\
         name TEXT NOT NULL,\
         applied_at INTEGER NOT NULL,\
         checksum TEXT\
         )",
        Params::None,
    )
    .await
    .unwrap();
    for migration in MIGRATIONS
        .iter()
        .filter(|migration| migration.version <= 23)
    {
        conn.execute_batch(migration.sql).await.unwrap();
        conn.execute(
            "INSERT INTO schema_migrations (version, name, applied_at, checksum) \
             VALUES (?1, ?2, ?3, ?4)",
            libsql::params!(
                migration.version,
                migration.name,
                unix_now(),
                migration.checksum()
            ),
        )
        .await
        .unwrap();
    }
}

#[tokio::test(flavor = "current_thread")]
async fn migration_0024_keeps_legacy_beats_and_adds_the_duration_column() {
    let dir = tempfile::tempdir().unwrap();
    let conn = connect(dir.path().join("agent_state.db")).await.unwrap();
    migrate_through_0023(&conn).await;

    // A sensor beat as the pre-0024 recorder wrote it: both timestamps hold
    // the same post-run recording stamp and there is no measured duration.
    conn.execute(
        "INSERT INTO beats (task_id, scope, beat_type, status, sensor_exit_code, sensor_name, \
         started_at, completed_at, input_digest) \
         VALUES (NULL, 'global', 'sensor', 'ok', 0, 'legacy', 1000, 1000, NULL)",
        Params::None,
    )
    .await
    .unwrap();

    migrate(&conn).await.unwrap();

    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM pragma_table_info('beats') WHERE name = 'duration_ms'",
            Params::None,
        )
        .await
        .unwrap();
    let row = rows.next().await.unwrap().unwrap();
    assert_eq!(row.get::<i64>(0).unwrap(), 1, "0024 adds duration_ms");

    let executions = do_harness_db::sensor_executions(&conn, None, None, None, None)
        .await
        .unwrap();
    assert_eq!(executions.len(), 1, "the legacy row survives");
    assert_eq!(executions[0].sensor, "legacy");
    assert_eq!(
        executions[0].duration_ms, None,
        "recording-stamp-only rows export unknown, not 0 ms"
    );
    assert_eq!(executions[0].started_at, 1000);
    assert_eq!(executions[0].completed_at, Some(1000));
}

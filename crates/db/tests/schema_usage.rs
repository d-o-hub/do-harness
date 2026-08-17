//! Computational prevention for two classes of db data rot:
//!
//! 1. A table COLUMN that is added to the schema but never written by any
//!    non-test INSERT/UPDATE (dead column, e.g. the `token_efficiency` column
//!    dropped in migration `0006`).
//! 2. The `skill_evals` table losing its one-row-per-skill uniqueness.
//!
//! The mechanism: the test reads the REAL migrations via `connect` + `migrate`
//! on a throwaway temp database, then derives the current column set from
//! `PRAGMA table_info(...)` (authoritative). It then concatenates the current
//! production writer/repo sources read from disk and asserts every current
//! column identifier appears in that production source. A new migration that
//! adds a column with no matching writer code yields a column absent from the
//! production source and the test fails.
#![deny(unsafe_code)]
#![allow(clippy::unwrap_used)]

use std::path::Path;

use anyhow::{Context, Result};
use do_harness_db::migrate::connect;
use libsql::{Connection, params::Params};

/// Production repo sources that contain the writer and reader SQL statements.
/// Read from disk at test time so column-writer coupling can't drift from the
/// actual source.
const PROD_SOURCES: &[&str] = &[
    "/src/repo.rs",
    "/src/repo_exec.rs",
    "/src/repo_learn.rs",
    "/src/repo_scope.rs",
];

/// Opens a fresh, fully-migrated database under a tempdir and returns the
/// connection plus the tempdir handle (kept alive to preserve the file).
async fn migrated_conn(dir: &Path) -> Result<Connection> {
    let conn = connect(dir.join("agent_state.db").as_path()).await?;
    do_harness_db::migrate::migrate(&conn).await?;
    Ok(conn)
}

/// Returns the set of user table names in the schema (excludes sqlite_* and
/// the internal migration tracking table).
async fn table_names(conn: &Connection) -> Result<Vec<String>> {
    let mut rows = conn
        .query(
            "SELECT name FROM sqlite_master WHERE type = 'table' \
             AND name NOT LIKE 'sqlite_%' AND name != 'schema_migrations' \
             ORDER BY name",
            Params::None,
        )
        .await?;
    let mut names = Vec::new();
    while let Some(row) = rows.next().await? {
        names.push(row.get::<String>(0)?);
    }
    Ok(names)
}

/// Returns the set of column names for a table via `PRAGMA table_info`.
async fn table_columns(conn: &Connection, table: &str) -> Result<Vec<String>> {
    let sql = format!("PRAGMA table_info({table})");
    let mut rows = conn.query(sql.as_str(), Params::None).await?;
    let mut cols = Vec::new();
    while let Some(row) = rows.next().await? {
        cols.push(row.get::<String>(1)?);
    }
    Ok(cols)
}

/// Concatenates the production repo sources, stripping `#[cfg(test)]` test
/// modules so test-only column references cannot satisfy the writer guard.
fn production_source() -> String {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let mut combined = String::new();
    for rel in PROD_SOURCES {
        let path = format!("{manifest}{rel}");
        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("read production source {path}"))
            .unwrap_or_default();
        // The tests module is the last block in each file; keep only the
        // non-test portion.
        if let Some(chunk) = contents.split("#[cfg(test)]").next() {
            combined.push_str(chunk);
        }
    }
    combined
}

/// Test 1: every persisted column has a non-test writer.
///
/// Derives the current column set from the real, migrated schema and asserts
/// every column identifier appears in the concatenated production repo source.
#[tokio::test(flavor = "current_thread")]
async fn every_persisted_column_has_a_production_writer() {
    let dir = tempfile::tempdir().expect("tempdir");
    let conn = migrated_conn(dir.path()).await.expect("migrated conn");

    let mut missing = Vec::new();
    for table in table_names(&conn).await.expect("table names") {
        let cols = table_columns(&conn, &table).await.expect("table columns");
        for col in cols {
            // `id` is the autoincrement primary key; production SQL references
            // it in readers even though INSERTs assign it implicitly.
            if col == "id" {
                continue;
            }
            if !production_source().contains(&col) {
                missing.push(format!("{table}.{col}"));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "columns with no non-test production writer: {}",
        missing.join(", ")
    );
}

/// Test 2: `skill_evals` enforces one row per skill.
///
/// Migration `0006` drops the never-written `token_efficiency` column and makes
/// `skill_name` UNIQUE (auto-creating a unique index). Assert both properties
/// hold in the real migrated schema.
#[tokio::test(flavor = "current_thread")]
async fn skill_evals_is_one_row_per_skill_with_no_dead_column() {
    let dir = tempfile::tempdir().expect("tempdir");
    let conn = migrated_conn(dir.path()).await.expect("migrated conn");

    let cols = table_columns(&conn, "skill_evals")
        .await
        .expect("skill_evals columns");
    assert!(
        !cols.contains(&"token_efficiency".to_string()),
        "token_efficiency must have been dropped by migration 0006"
    );
    assert!(
        cols.contains(&"skill_name".to_string()),
        "skill_name column must exist"
    );

    let unique_on_skill = has_unique_index(&conn, "skill_evals", "skill_name")
        .await
        .expect("inspect unique index");
    assert!(
        unique_on_skill,
        "skill_evals must have a unique index on skill_name"
    );
}

/// Returns true if `table` has a unique index that covers `column` (verified
/// via `PRAGMA index_list` + `PRAGMA index_info`).
async fn has_unique_index(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let list = format!("PRAGMA index_list('{table}')");
    let mut rows = conn.query(list.as_str(), Params::None).await?;
    while let Some(row) = rows.next().await? {
        let unique: i64 = row.get(2)?;
        if unique == 0 {
            continue;
        }
        let index: String = row.get(1)?;
        let info = format!("PRAGMA index_info('{index}')");
        let mut cols = conn.query(info.as_str(), Params::None).await?;
        while let Some(ci) = cols.next().await? {
            let colname: String = ci.get(2)?;
            if colname == column {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

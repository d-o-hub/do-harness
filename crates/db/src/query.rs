//! Generic parameterized read-only query helpers.

use libsql::{Connection, params};

use crate::error::Result;

/// Counts rows in `table` where `column` equals `value`.
///
/// The caller is responsible for validating `table` and `column` as SQL
/// identifiers; `value` is always bound as a query parameter.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn count_where(conn: &Connection, table: &str, column: &str, value: &str) -> Result<i64> {
    let sql = format!("SELECT COUNT(*) FROM \"{table}\" WHERE \"{column}\" = ?1");
    let mut rows = conn.query(&sql, params!(value)).await?;
    match rows.next().await? {
        Some(row) => Ok(row.get(0)?),
        None => Ok(0),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn count_where_counts_matching_rows_only() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::migrate::connect_and_migrate(dir.path())
            .await
            .unwrap();
        conn.execute(
            "INSERT INTO invariants (invariant, rationale, sensor, category, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params!("i1", "r", "test", "arch", crate::migrate::unix_now()),
        )
        .await
        .unwrap();

        assert_eq!(
            count_where(&conn, "invariants", "invariant", "i1")
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            count_where(&conn, "invariants", "invariant", "missing")
                .await
                .unwrap(),
            0
        );
    }
}

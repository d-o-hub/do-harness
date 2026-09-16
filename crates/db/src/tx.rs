//! Write-transaction helper that avoids the deferred-upgrade race.
//!
//! `Connection::transaction()` issues `BEGIN DEFERRED`, which takes a **read**
//! lock and only upgrades to a write lock on the first write. Two writers that
//! both hold read locks then both try to upgrade: `SQLite` fails one with
//! `SQLITE_BUSY` immediately, and `busy_timeout` cannot help because waiting
//! cannot resolve a genuine deadlock. Under CI contention this surfaced as a
//! flaky `database is locked` failure in the concurrent `verify --record`
//! test, while local runs almost never reproduced it.
//!
//! [`begin_immediate`] takes the write lock up front, so contention becomes a
//! plain "wait for the other writer" case that `busy_timeout` handles
//! correctly. Every write transaction in this crate should use it; read-only
//! work can keep using a plain deferred transaction.

use crate::error::Result;
use libsql::{Connection, Transaction, TransactionBehavior};

/// Begins a write transaction with `BEGIN IMMEDIATE`.
///
/// Use for any transaction that will write. See the module docs for why the
/// deferred default is unsafe under concurrent writers.
///
/// # Errors
///
/// Returns an error when the transaction cannot be started.
pub async fn begin_immediate(conn: &Connection) -> Result<Transaction> {
    conn.transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// Two `BEGIN IMMEDIATE` writers must serialize rather than deadlock: the
    /// second starts only after the first commits and then succeeds. A deferred
    /// transaction would risk `SQLITE_BUSY` on the lock upgrade instead.
    #[tokio::test(flavor = "current_thread")]
    async fn immediate_writers_serialize_without_busy() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::migrate::connect_and_migrate(dir.path())
            .await
            .unwrap();

        let first = begin_immediate(&conn).await.unwrap();
        first
            .execute(
                "INSERT INTO heuristics (skill_name, pattern, created_at) VALUES ('a', 'p', 0)",
                libsql::params::Params::None,
            )
            .await
            .unwrap();

        // A second immediate writer on the same connection cannot start until
        // the first commits; committing then retrying must succeed.
        first.commit().await.unwrap();
        let second = begin_immediate(&conn).await.unwrap();
        second
            .execute(
                "INSERT INTO heuristics (skill_name, pattern, created_at) VALUES ('b', 'p', 0)",
                libsql::params::Params::None,
            )
            .await
            .unwrap();
        second.commit().await.unwrap();

        let count = crate::query::count_where(&conn, "heuristics", "pattern", "p")
            .await
            .unwrap();
        assert_eq!(count, 2);
    }
}

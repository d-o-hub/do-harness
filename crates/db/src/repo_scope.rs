//! Repository layer for fail-fast signature lifecycle (reset, list, clear).

use crate::error::Result;
use do_harness_types::ErrorSignature;
use libsql::{Connection, params, params::Params};

/// Resets a `(signature, task_id)` pair back to zero attempts by deleting it.
///
/// Called by `verify --record` when a sensor passes, so the fail-fast strike
/// counter starts fresh on the next failure. Returns whether a row was
/// removed.
///
/// # Errors
///
/// Returns an error when the delete statement fails.
pub async fn reset_error_signature(
    conn: &Connection,
    signature: &str,
    task_id: Option<i64>,
) -> Result<bool> {
    let deleted = conn
        .execute(
            "DELETE FROM error_signatures WHERE signature = ?1 AND task_id IS ?2",
            params!(signature, task_id),
        )
        .await?;
    Ok(deleted > 0)
}

/// Lists error signatures, optionally scoped to one task.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn list_error_signatures(
    conn: &Connection,
    task_id: Option<i64>,
) -> Result<Vec<ErrorSignature>> {
    let mut rows = match task_id {
        Some(id) => {
            conn.query(
                "SELECT id, signature, task_id, attempt_count, message, created_at \
                 FROM error_signatures WHERE task_id = ?1 ORDER BY attempt_count DESC",
                params!(id),
            )
            .await?
        }
        None => {
            conn.query(
                "SELECT id, signature, task_id, attempt_count, message, created_at \
                 FROM error_signatures ORDER BY attempt_count DESC",
                Params::None,
            )
            .await?
        }
    };
    let mut signatures = Vec::new();
    while let Some(row) = rows.next().await? {
        signatures.push(ErrorSignature {
            id: row.get(0)?,
            signature: row.get(1)?,
            task_id: row.get(2)?,
            attempt_count: row.get(3)?,
            message: row.get(4)?,
            created_at: row.get(5)?,
        });
    }
    Ok(signatures)
}

/// Clears signatures, optionally limited to one task and/or one signature key.
///
/// Returns the number of rows removed.
///
/// # Errors
///
/// Returns an error when the delete statement fails.
pub async fn clear_error_signatures(
    conn: &Connection,
    task_id: Option<i64>,
    signature: Option<&str>,
) -> Result<usize> {
    match (task_id, signature) {
        (None, None) => Ok(usize::try_from(
            conn.execute("DELETE FROM error_signatures", Params::None)
                .await?,
        )?),
        (Some(id), None) => Ok(usize::try_from(
            conn.execute(
                "DELETE FROM error_signatures WHERE task_id = ?1",
                params!(id),
            )
            .await?,
        )?),
        (None, Some(sig)) => Ok(usize::try_from(
            conn.execute(
                "DELETE FROM error_signatures WHERE signature = ?1",
                params!(sig),
            )
            .await?,
        )?),
        (Some(id), Some(sig)) => Ok(usize::try_from(
            conn.execute(
                "DELETE FROM error_signatures WHERE task_id = ?1 AND signature = ?2",
                params!(id, sig),
            )
            .await?,
        )?),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::repo::NewTask;
    use crate::repo_exec::{bump_error_signature, get_error_signature};

    #[tokio::test(flavor = "current_thread")]
    async fn signatures_scope_by_task_id() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::migrate::connect_and_migrate(dir.path())
            .await
            .unwrap();
        let one = crate::repo::insert_task(
            &conn,
            &NewTask {
                title: "one",
                method: Some("vertical-event-slice"),
                subtask_index: 0,
                precondition: None,
                parent_id: None,
            },
        )
        .await
        .unwrap();
        let two = crate::repo::insert_task(
            &conn,
            &NewTask {
                title: "two",
                method: Some("vertical-event-slice"),
                subtask_index: 0,
                precondition: None,
                parent_id: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(
            bump_error_signature(&conn, "sensor:clippy", Some(one), Some("m"))
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            bump_error_signature(&conn, "sensor:clippy", Some(one), None)
                .await
                .unwrap(),
            2
        );
        // A different task and the workspace-global key start fresh.
        assert_eq!(
            bump_error_signature(&conn, "sensor:clippy", Some(two), None)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            bump_error_signature(&conn, "sensor:clippy", None, None)
                .await
                .unwrap(),
            1
        );

        assert_eq!(
            get_error_signature(&conn, "sensor:clippy", Some(one))
                .await
                .unwrap()
                .unwrap()
                .attempt_count,
            2
        );
        assert_eq!(
            get_error_signature(&conn, "sensor:clippy", Some(two))
                .await
                .unwrap()
                .unwrap()
                .attempt_count,
            1
        );
        assert_eq!(
            list_error_signatures(&conn, Some(one)).await.unwrap().len(),
            1
        );
        assert_eq!(list_error_signatures(&conn, None).await.unwrap().len(), 3);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reset_removes_only_the_scope_key() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::migrate::connect_and_migrate(dir.path())
            .await
            .unwrap();
        let one = crate::repo::insert_task(
            &conn,
            &NewTask {
                title: "one",
                method: Some("vertical-event-slice"),
                subtask_index: 0,
                precondition: None,
                parent_id: None,
            },
        )
        .await
        .unwrap();
        let two = crate::repo::insert_task(
            &conn,
            &NewTask {
                title: "two",
                method: Some("vertical-event-slice"),
                subtask_index: 0,
                precondition: None,
                parent_id: None,
            },
        )
        .await
        .unwrap();
        bump_error_signature(&conn, "sensor:clippy", Some(one), Some("m"))
            .await
            .unwrap();
        bump_error_signature(&conn, "sensor:clippy", Some(two), Some("m"))
            .await
            .unwrap();

        assert!(
            reset_error_signature(&conn, "sensor:clippy", Some(one))
                .await
                .unwrap()
        );
        assert!(
            !reset_error_signature(&conn, "sensor:clippy", Some(one))
                .await
                .unwrap()
        );
        assert!(
            get_error_signature(&conn, "sensor:clippy", Some(one))
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            get_error_signature(&conn, "sensor:clippy", Some(two))
                .await
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn clear_filtered_by_sensor_or_task() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::migrate::connect_and_migrate(dir.path())
            .await
            .unwrap();
        let one = crate::repo::insert_task(
            &conn,
            &NewTask {
                title: "one",
                method: Some("vertical-event-slice"),
                subtask_index: 0,
                precondition: None,
                parent_id: None,
            },
        )
        .await
        .unwrap();
        bump_error_signature(&conn, "sensor:clippy", Some(one), None)
            .await
            .unwrap();
        bump_error_signature(&conn, "sensor:fmt", None, None)
            .await
            .unwrap();

        assert_eq!(
            clear_error_signatures(&conn, None, Some("sensor:clippy"))
                .await
                .unwrap(),
            1
        );
        assert_eq!(list_error_signatures(&conn, None).await.unwrap().len(), 1);
    }
}

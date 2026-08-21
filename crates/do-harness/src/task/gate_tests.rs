#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::tests::write_catalog;
use super::*;

/// `add_task` validates a method against the catalog, so these helpers insert a
/// dangling-method task directly through the db layer, bypassing that gate.
async fn insert_task_with_method(dir: &tempfile::TempDir, method: &str) -> i64 {
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let id = do_harness_db::insert_task(
        &conn,
        &do_harness_db::NewTask {
            title: "dangling",
            method: Some(method),
            subtask_index: 0,
            precondition: None,
            parent_id: None,
        },
    )
    .await
    .unwrap();
    drop(conn);
    id
}

/// A task whose method references a name absent from the catalog cannot advance.
///
/// The catalog check happens on `advance`, so a dangling-method task (created
/// before the catalog changed, or by a db write) must fail loudly rather than
/// silently advance.
#[tokio::test(flavor = "current_thread")]
async fn advance_rejects_unknown_method() {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    let id = insert_task_with_method(&dir, "not-in-catalog").await;

    let err = advance_task(dir.path(), id).await.unwrap_err();
    assert!(
        err.to_string()
            .contains("references unknown method 'not-in-catalog'"),
        "unexpected error: {err}"
    );
}

/// A task whose method is an empty string is not treatable as methodless; it
/// must resolve (and fail) via the catalog path with the unknown-method error,
/// not panic and not silently advance.
#[tokio::test(flavor = "current_thread")]
async fn advance_rejects_empty_method_string() {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    let id = insert_task_with_method(&dir, "").await;

    let err = advance_task(dir.path(), id).await.unwrap_err();
    assert!(
        err.to_string().contains("references unknown method ''"),
        "unexpected error: {err}"
    );
}

/// A task with a dangling (unknown) method cannot be marked done either.
#[tokio::test(flavor = "current_thread")]
async fn done_rejects_unknown_method() {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    let id = insert_task_with_method(&dir, "not-in-catalog").await;

    let err = done_task(dir.path(), id).await.unwrap_err();
    assert!(
        err.to_string()
            .contains("references unknown method 'not-in-catalog'"),
        "unexpected error: {err}"
    );
}

/// An empty-string method is still a method for `done`: it must hit the
/// catalog path, not be mistaken for a methodless task.
#[tokio::test(flavor = "current_thread")]
async fn done_rejects_empty_method_string() {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    let id = insert_task_with_method(&dir, "").await;

    let err = done_task(dir.path(), id).await.unwrap_err();
    assert!(
        err.to_string().contains("references unknown method ''"),
        "unexpected error: {err}"
    );
}

/// Failing a methodless task is legitimate (a stuck task may be failed), but
/// after failing it must still be impossible to advance or mark done.
///
/// Note the asymmetry: `advance_task` rejects `done`/`failed` before the method
/// check, while `done_task` checks the method first — so the two block with
/// different messages for the same task.
#[tokio::test(flavor = "current_thread")]
async fn fail_allows_methodless_then_blocks_advance_and_done() {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    let (id, _event) = add_task(dir.path(), "orphan", None, None, None)
        .await
        .unwrap();

    fail_task(dir.path(), id).await.unwrap();

    let advance_err = advance_task(dir.path(), id).await.unwrap_err();
    assert_eq!(
        advance_err.to_string(),
        format!("task {id} is failed; cannot advance"),
        "unexpected error: {advance_err}"
    );
    let done_err = done_task(dir.path(), id).await.unwrap_err();
    assert_eq!(
        done_err.to_string(),
        format!("task {id} has no method; cannot mark done"),
        "unexpected error: {done_err}"
    );
}

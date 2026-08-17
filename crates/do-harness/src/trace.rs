//! Interaction trace recording for `do-harness trace`.

use std::path::Path;

use anyhow::{Context, Result};

use crate::report::Format;

/// Insert parameters for a new trace.
#[derive(Debug, Clone)]
pub struct TraceOpts<'a> {
    /// Owning task id, when the trace belongs to a task.
    pub task_id: Option<i64>,
    /// Session identifier grouping related traces.
    pub session: &'a str,
    /// The command that was executed.
    pub command: Option<&'a str>,
    /// Error diff or failure output captured.
    pub error_diff: Option<&'a str>,
    /// Steps taken to resolve the failure.
    pub resolution_steps: Option<&'a str>,
}

/// Inserts a trace and returns its id.
///
/// # Errors
///
/// Returns an error when the state database cannot be opened or the insert
/// fails.
pub async fn add_trace(root: &Path, opts: &TraceOpts<'_>) -> Result<i64> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let id = do_harness_db::insert_trace(
        &conn,
        &do_harness_db::NewTrace {
            task_id: opts.task_id,
            session_id: opts.session,
            command: opts.command,
            error_diff: opts.error_diff,
            resolution_steps: opts.resolution_steps,
        },
    )
    .await?;
    Ok(id)
}

/// Prints the traces of a session in the requested format.
///
/// # Errors
///
/// Returns an error when the state database cannot be opened, the listing
/// fails, or the traces cannot be serialized.
pub async fn list_traces(root: &Path, session: &str, format: Format) -> Result<()> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let traces = do_harness_db::list_traces(&conn, session).await?;
    match format {
        Format::Text => {
            for trace in &traces {
                let command = trace.command.as_deref().unwrap_or("(no command)");
                let task = match trace.task_id {
                    Some(id) => id.to_string(),
                    None => "-".to_owned(),
                };
                println!("{}: {command} session={session} task={task}", trace.id);
            }
        }
        Format::Json => {
            println!(
                "{}",
                serde_json::to_string(&traces).context("failed to serialize traces")?
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn add_trace_roundtrips_into_db() {
        let dir = tempfile::tempdir().unwrap();
        let conn = do_harness_db::connect_and_migrate(dir.path())
            .await
            .unwrap();
        let task_id = do_harness_db::insert_task(
            &conn,
            &do_harness_db::NewTask {
                title: "slice",
                method: Some("vertical-event-slice"),
                subtask_index: 0,
                precondition: None,
                parent_id: None,
            },
        )
        .await
        .unwrap();
        drop(conn);
        let opts = TraceOpts {
            task_id: Some(task_id),
            session: "s1",
            command: Some("cargo check"),
            error_diff: Some("E0308"),
            resolution_steps: Some("added lifetime"),
        };

        let id = add_trace(dir.path(), &opts).await.unwrap();

        let conn = do_harness_db::connect_and_migrate(dir.path())
            .await
            .unwrap();
        let traces = do_harness_db::list_traces(&conn, "s1").await.unwrap();
        assert_eq!(traces.len(), 1);
        assert_eq!(traces[0].id, id);
        assert_eq!(traces[0].task_id, Some(task_id));
        assert_eq!(traces[0].command.as_deref(), Some("cargo check"));
        assert_eq!(traces[0].error_diff.as_deref(), Some("E0308"));
        assert_eq!(
            traces[0].resolution_steps.as_deref(),
            Some("added lifetime")
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn add_trace_stores_nullables_when_all_none() {
        let dir = tempfile::tempdir().unwrap();
        let opts = TraceOpts {
            task_id: None,
            session: "s1",
            command: None,
            error_diff: None,
            resolution_steps: None,
        };

        let id = add_trace(dir.path(), &opts).await.unwrap();

        let conn = do_harness_db::connect_and_migrate(dir.path())
            .await
            .unwrap();
        let trace = do_harness_db::get_trace(&conn, id).await.unwrap().unwrap();
        assert_eq!(trace.task_id, None);
        assert_eq!(trace.command, None);
        assert_eq!(trace.error_diff, None);
        assert_eq!(trace.resolution_steps, None);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn list_traces_on_empty_session_lists_nothing() {
        let dir = tempfile::tempdir().unwrap();

        list_traces(dir.path(), "s1", Format::Text).await.unwrap();

        let conn = do_harness_db::connect_and_migrate(dir.path())
            .await
            .unwrap();
        assert!(
            do_harness_db::list_traces(&conn, "s1")
                .await
                .unwrap()
                .is_empty()
        );
    }
}

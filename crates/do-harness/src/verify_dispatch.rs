//! Verify command option assembly and routing.

use std::path::{Path, PathBuf};

use crate::{CliError, cli::VerifyArgs, sensors::VerifyOpts, verify, workspace_verify};

pub(crate) async fn run(
    root: &Path,
    config_path: Option<PathBuf>,
    args: VerifyArgs,
    dry_run: bool,
    quiet: bool,
    output: Option<PathBuf>,
    explicit_config: bool,
) -> Result<(), CliError> {
    let VerifyArgs {
        fail_fast,
        format,
        simulate,
        set,
        changed,
        since,
        only,
        exclude,
        jobs,
        workspaces,
        unchanged,
        record,
        task,
        global,
        evidence,
        strict,
        bless,
        approver,
    } = args;

    let (task_id, raw_task) = match task.as_deref() {
        Some("global") => (None, Some("global".to_string())),
        Some(raw) => {
            let id = raw.parse::<i64>().map_err(|_| {
                CliError::Usage(anyhow::anyhow!(
                    "invalid --task '{raw}': expected an integer task ID or 'global'"
                ))
            })?;
            (Some(id), Some(raw.to_string()))
        }
        None => (None, None),
    };
    let explicit_global = global || raw_task.as_deref() == Some("global");
    let opts = VerifyOpts {
        fail_fast,
        dry_run,
        simulate,
        set,
        changed,
        since,
        only,
        exclude,
        jobs,
        unchanged,
        record,
        task: task_id,
        raw_task,
        global: explicit_global,
        evidence,
        strict,
        bless,
        approver,
        format,
        quiet,
        output,
        config: config_path,
        ..Default::default()
    };

    match workspaces {
        Some(pattern) => workspace_verify::run(root, &pattern, opts, explicit_config).await,
        None => verify::run(root, opts).await,
    }
}

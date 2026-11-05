//! Impact-gated feedback and automated submission (WF-09).

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

pub mod auto;
pub mod draft;
pub mod report;
pub mod submit;
pub mod types;

use crate::cli::FeedbackAction;
use crate::report::Format;
use types::SubmissionResult;

/// Inputs for an explicit defect report.
struct ReportRequest {
    repo: Option<String>,
    error: String,
    log: Option<std::path::PathBuf>,
    warning: bool,
    yes: bool,
    dry_run: bool,
    format: Format,
}

fn run_report(root: &Path, request: ReportRequest) -> Result<()> {
    let log_tail = match request.log {
        Some(path) => report::read_log_tail(&path)?,
        None => String::new(),
    };
    let kind = if request.warning { "warning" } else { "error" };
    let argv = crate::diag::argv();
    let result = report::file_explicit(
        root,
        request.repo.as_deref(),
        kind,
        &request.error,
        &argv,
        &log_tail,
        request.yes,
        request.dry_run,
    )?;
    match request.format {
        Format::Json => {
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Format::Text => match result {
            SubmissionResult::Created {
                issue_number,
                issue_url,
            } => {
                println!("Created issue #{issue_number}: {issue_url}");
            }
            SubmissionResult::LinkedExisting {
                issue_number,
                issue_url,
            } => {
                println!("Linked existing issue #{issue_number}: {issue_url}");
            }
            SubmissionResult::DryRun {
                title,
                target_repo,
                would_create,
            } => {
                println!(
                    "Dry run: would create issue in {target_repo} with title '{title}' (would_create={would_create})"
                );
            }
        },
    }
    Ok(())
}

fn run_submit(
    root: &Path,
    draft: &std::path::Path,
    repo: Option<&str>,
    yes: bool,
    dry_run: bool,
) -> Result<()> {
    let bytes = fs::read(draft).with_context(|| format!("read draft file {}", draft.display()))?;
    let parsed_draft: types::FeedbackDraft =
        serde_json::from_slice(&bytes).context("parse draft JSON")?;
    let res = submit::submit_draft(root, &parsed_draft, repo, yes, dry_run)?;
    match res {
        SubmissionResult::Created {
            issue_number,
            issue_url,
        } => {
            println!("Created issue #{issue_number}: {issue_url}");
        }
        SubmissionResult::LinkedExisting {
            issue_number,
            issue_url,
        } => {
            println!("Linked existing issue #{issue_number}: {issue_url}");
        }
        SubmissionResult::DryRun {
            title,
            target_repo,
            would_create,
        } => {
            println!(
                "Dry run: would create issue in {target_repo} with title '{title}' (would_create={would_create})"
            );
        }
    }
    Ok(())
}

/// Dispatches a feedback action.
///
/// `global_dry_run` is the global `--dry-run`. The `submit` and `auto` subcommands
/// also declare their own `--dry-run`, so the two are combined: a caller that
/// passes the global flag before the subcommand must not publish either.
pub async fn run(root: &Path, action: FeedbackAction, global_dry_run: bool) -> Result<()> {
    match action {
        FeedbackAction::Draft {
            repo,
            days,
            min_fires,
            out,
            format,
        } => {
            let drafts = draft::generate_drafts(root, &repo, days, min_fires).await?;
            if let Some(out_path) = out {
                if global_dry_run {
                    println!(
                        "Dry run: would write {} candidate(s) to {}",
                        drafts.len(),
                        out_path.display()
                    );
                } else {
                    let bytes = serde_json::to_vec_pretty(&drafts).context("serialize drafts")?;
                    fs::write(&out_path, bytes)
                        .with_context(|| format!("write drafts to {}", out_path.display()))?;
                }
            }
            match format {
                Format::Json => {
                    println!("{}", serde_json::json!({ "candidates": drafts }));
                }
                Format::Text => {
                    if drafts.is_empty() {
                        println!("no eligible feedback candidates for {repo}");
                    } else {
                        println!("Drafted {} feedback candidate(s) for {repo}:", drafts.len());
                        for d in &drafts {
                            println!("* [{}] {} ({})", d.fingerprint, d.title, d.impact);
                        }
                    }
                }
            }
            Ok(())
        }
        FeedbackAction::Submit {
            draft,
            repo,
            yes,
            dry_run,
        } => run_submit(
            root,
            &draft,
            repo.as_deref(),
            yes,
            dry_run || global_dry_run,
        ),
        FeedbackAction::Auto {
            repo,
            days,
            min_fires,
            dry_run,
            format,
        } => {
            let dry_run = dry_run || global_dry_run;
            auto::run_auto(root, &repo, days, min_fires, dry_run, format).await
        }
        FeedbackAction::Report {
            repo,
            error,
            log,
            warning,
            yes,
            dry_run,
            format,
        } => run_report(
            root,
            ReportRequest {
                repo,
                error,
                log,
                warning,
                yes,
                dry_run: dry_run || global_dry_run,
                format,
            },
        ),
    }
}

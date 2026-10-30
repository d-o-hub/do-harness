//! Unattended feedback generation, deduplication, and submission (WF-09).

use std::path::Path;

use anyhow::Result;

use super::draft::generate_drafts;
use super::submit::submit_draft;
use super::types::SubmissionResult;
use crate::report::Format;

/// Runs the complete unattended feedback pipeline with fail-closed safety guards.
pub async fn run_auto(
    root: &Path,
    repo: &str,
    days: Option<i64>,
    min_fires: Option<u32>,
    dry_run: bool,
    format: Format,
) -> Result<()> {
    let drafts = generate_drafts(root, repo, days, min_fires).await?;
    if drafts.is_empty() {
        if matches!(format, Format::Json) {
            println!(
                "{}",
                serde_json::json!({
                    "status": "no_candidates",
                    "repo": repo,
                    "candidates": []
                })
            );
        } else {
            println!("no eligible feedback candidates for {repo}");
        }
        return Ok(());
    }

    // Circuit breaker: max 1 automated submission per run
    let draft = &drafts[0];
    let result = submit_draft(root, draft, None, true, dry_run)?;

    match format {
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

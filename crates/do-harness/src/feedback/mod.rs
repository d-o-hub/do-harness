//! Impact-gated feedback and automated submission (WF-09).

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

pub mod auto;
pub mod draft;
pub mod submit;
pub mod types;

use crate::cli::FeedbackAction;
use crate::report::Format;
use types::SubmissionResult;

/// Dispatches a feedback action.
pub async fn run(root: &Path, action: FeedbackAction) -> Result<()> {
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
                let bytes = serde_json::to_vec_pretty(&drafts).context("serialize drafts")?;
                fs::write(&out_path, bytes)
                    .with_context(|| format!("write drafts to {}", out_path.display()))?;
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
        } => {
            let bytes =
                fs::read(&draft).with_context(|| format!("read draft file {}", draft.display()))?;
            let parsed_draft: types::FeedbackDraft =
                serde_json::from_slice(&bytes).context("parse draft JSON")?;
            let res = submit::submit_draft(root, &parsed_draft, repo.as_deref(), yes, dry_run)?;
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
        FeedbackAction::Auto {
            repo,
            days,
            min_fires,
            dry_run,
            format,
        } => auto::run_auto(root, &repo, days, min_fires, dry_run, format).await,
    }
}

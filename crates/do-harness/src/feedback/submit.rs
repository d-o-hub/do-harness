//! Issue submission, deduplication search, and receipt recording (WF-09).

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use super::draft::validate_repo;
use super::types::{FeedbackDraft, SubmissionReceipt, SubmissionResult};

/// Searches for an existing issue with the exact fingerprint marker.
fn find_existing_issue(repo: &str, fingerprint: &str) -> Result<Option<(u64, String)>> {
    let output = Command::new("gh")
        .args([
            "issue",
            "list",
            "--repo",
            repo,
            "--state",
            "all",
            "--search",
            fingerprint,
            "--json",
            "number,url,state,title,body",
        ])
        .output()
        .context("failed to execute gh issue list for deduplication")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("gh issue list failed: {stderr}");
    }

    let parsed: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("parse gh issue list json output")?;

    let Some(issues) = parsed.as_array() else {
        return Ok(None);
    };

    let marker = format!("<!-- do-harness-feedback-fingerprint: {fingerprint} -->");
    for issue in issues {
        let body = issue
            .get("body")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        if body.contains(&marker) {
            let number = issue
                .get("number")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            let url = issue
                .get("url")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_owned();
            if number > 0 && !url.is_empty() {
                return Ok(Some((number, url)));
            }
        }
    }

    Ok(None)
}

fn load_receipts(root: &Path) -> Vec<SubmissionReceipt> {
    let path = root.join(".do-harness/feedback/receipts.json");
    if let Ok(data) = fs::read(path) {
        serde_json::from_slice(&data).unwrap_or_default()
    } else {
        Vec::new()
    }
}

fn store_receipt(root: &Path, receipt: &SubmissionReceipt) -> Result<()> {
    let dir = root.join(".do-harness/feedback");
    fs::create_dir_all(&dir).context("create feedback state directory")?;
    let path = dir.join("receipts.json");
    let mut receipts = load_receipts(root);
    receipts.retain(|r| r.fingerprint != receipt.fingerprint);
    receipts.push(receipt.clone());
    let data = serde_json::to_vec_pretty(&receipts).context("serialize receipts")?;
    fs::write(path, data).context("write feedback receipts")?;
    Ok(())
}

/// Submits or links a feedback draft.
pub fn submit_draft(
    root: &Path,
    draft: &FeedbackDraft,
    explicit_repo: Option<&str>,
    skip_confirmation: bool,
    dry_run: bool,
) -> Result<SubmissionResult> {
    let repo = explicit_repo.unwrap_or(&draft.target_repo);
    validate_repo(repo)?;

    let local_receipts = load_receipts(root);
    if let Some(existing) = local_receipts
        .iter()
        .find(|r| r.fingerprint == draft.fingerprint)
    {
        return Ok(SubmissionResult::LinkedExisting {
            issue_number: existing.issue_number,
            issue_url: existing.issue_url.clone(),
        });
    }

    // Remote deduplication search across open and closed issues
    if let Some((number, url)) = find_existing_issue(repo, &draft.fingerprint)? {
        let receipt = SubmissionReceipt {
            fingerprint: draft.fingerprint.clone(),
            target_repo: repo.to_owned(),
            issue_number: number,
            issue_url: url.clone(),
            timestamp: do_harness_db::unix_now(),
            status: "linked_existing".to_owned(),
        };
        let _ = store_receipt(root, &receipt);
        return Ok(SubmissionResult::LinkedExisting {
            issue_number: number,
            issue_url: url,
        });
    }

    if dry_run {
        return Ok(SubmissionResult::DryRun {
            title: draft.title.clone(),
            target_repo: repo.to_owned(),
            would_create: true,
        });
    }

    if !skip_confirmation {
        bail!("submission requires explicit confirmation; re-run with --yes to publish to {repo}");
    }

    // Issue creation with fixed argv and stdin body
    let mut child = Command::new("gh")
        .args([
            "issue",
            "create",
            "--repo",
            repo,
            "--title",
            &draft.title,
            "--body-file",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawn gh issue create")?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(draft.body.as_bytes())
            .context("write issue body to gh stdin")?;
    }

    let output = child
        .wait_with_output()
        .context("wait for gh issue create")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("gh issue create failed: {stderr}");
    }

    let issue_url = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if issue_url.is_empty() {
        bail!("gh issue create succeeded but returned no issue URL");
    }

    let issue_number = issue_url
        .rsplit('/')
        .next()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let receipt = SubmissionReceipt {
        fingerprint: draft.fingerprint.clone(),
        target_repo: repo.to_owned(),
        issue_number,
        issue_url: issue_url.clone(),
        timestamp: do_harness_db::unix_now(),
        status: "created".to_owned(),
    };
    let _ = store_receipt(root, &receipt);

    Ok(SubmissionResult::Created {
        issue_number,
        issue_url,
    })
}

//! Disposition parsing, formatting, and command execution.

use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use sha2::{Digest, Sha256};

use super::model::{DispositionDecision, DispositionRecord};
use crate::CliError;
use crate::pr::gh;
use crate::report::Format;

/// Marker prefix for structured disposition comments.
pub const DISPOSITION_PREFIX: &str = "<!-- do-harness-disposition:v1\n";
/// Marker suffix for structured disposition comments.
pub const DISPOSITION_SUFFIX: &str = "\n-->";

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Parses a disposition record from comment text if present.
#[must_use]
pub fn parse_disposition(body: &str) -> Option<DispositionRecord> {
    let start = body.find(DISPOSITION_PREFIX)?;
    let rest = &body[start + DISPOSITION_PREFIX.len()..];
    let end = rest.find(DISPOSITION_SUFFIX)?;
    let json_text = &rest[..end];
    serde_json::from_str::<DispositionRecord>(json_text).ok()
}

/// Formats a disposition record into a full markdown PR comment body.
#[must_use]
pub fn format_disposition_comment(record: &DispositionRecord) -> String {
    let json = serde_json::to_string_pretty(record).unwrap_or_default();
    format!(
        "{DISPOSITION_PREFIX}{json}{DISPOSITION_SUFFIX}\n\
         **[do-harness] Disposition recorded**\n\n\
         - **Item:** `{}`\n\
         - **Decision:** `{}`\n\
         - **Reason:** {}\n\
         - **Head SHA:** `{}`\n",
        record.item_ref,
        record.decision.as_str(),
        record.reason,
        record.head_sha
    )
}

/// Executes `do-harness pr disposition`.
///
/// # Errors
///
/// Returns `CliError::Usage` on invalid inputs, missing items, or API errors.
#[allow(clippy::too_many_arguments)]
pub fn run_disposition(
    root: &Path,
    pr: u64,
    item_ref: &str,
    decision: DispositionDecision,
    head: &str,
    reason: &str,
    evidence: Option<&str>,
    format: Format,
    dry_run: bool,
) -> Result<(), CliError> {
    let head = head.trim();
    if head.len() != 40 {
        return Err(CliError::Usage(anyhow!(
            "invalid head SHA '{head}': expected 40-character hex SHA"
        )));
    }
    if reason.trim().is_empty() {
        return Err(CliError::Usage(anyhow!("--reason cannot be empty")));
    }
    if decision == DispositionDecision::Fixed {
        match evidence {
            Some(ev) if ev.trim() == head => {}
            Some(ev) => {
                return Err(CliError::Usage(anyhow!(
                    "FIXED decision requires --evidence equal to current head ({head}), got {ev}"
                )));
            }
            None => {
                return Err(CliError::Usage(anyhow!(
                    "FIXED decision requires --evidence equal to current head ({head})"
                )));
            }
        }
    }

    let view = gh::readiness_view(root, pr).map_err(CliError::Usage)?;
    if view.head_ref_oid != head {
        return Err(CliError::Usage(anyhow!(
            "PR {pr} current head is {}, does not match requested head {head}",
            view.head_ref_oid
        )));
    }

    // Locate item content and calculate item_sha256
    let item_body = resolve_item_body(root, pr, item_ref).map_err(CliError::Usage)?;
    let item_sha256 = sha256_hex(item_body.as_bytes());

    let policy_path = root.join("plans/pr-merge-policy.toml");
    let policy_sha256 = match std::fs::read(&policy_path) {
        Ok(bytes) => sha256_hex(&bytes),
        Err(_) => String::new(),
    };

    let record = DispositionRecord {
        schema_version: 1,
        item_ref: item_ref.to_owned(),
        decision,
        head_sha: head.to_owned(),
        base_sha: view.base_ref_oid,
        item_sha256,
        policy_sha256,
        reason: reason.to_owned(),
        evidence_sha: evidence.map(str::to_owned),
    };

    let comment_body = format_disposition_comment(&record);

    if dry_run {
        if format == Format::Json {
            let json = serde_json::to_string_pretty(&record)
                .map_err(|e| CliError::Usage(anyhow!("{e}")))?;
            println!("{json}");
        } else {
            println!("dry-run disposition for PR {pr}:\n{comment_body}");
        }
        return Ok(());
    }

    let endpoint = format!("repos/{{owner}}/{{repo}}/issues/{pr}/comments");
    crate::gh::output(
        root,
        &[
            "api",
            "-X",
            "POST",
            &endpoint,
            "-f",
            &format!("body={comment_body}"),
        ],
        "api post disposition comment",
    )
    .map_err(CliError::Usage)?;

    if format == Format::Json {
        let json =
            serde_json::to_string_pretty(&record).map_err(|e| CliError::Usage(anyhow!("{e}")))?;
        println!("{json}");
    } else {
        println!("Recorded disposition for {item_ref} on PR {pr}");
    }

    Ok(())
}

fn resolve_item_body(root: &Path, pr: u64, item_ref: &str) -> Result<String> {
    if let Some(id_str) = item_ref.strip_prefix("issue-comment:") {
        let comment_id = id_str
            .parse::<u64>()
            .context("invalid issue-comment numeric id")?;
        let comments = gh::issue_comments(root, pr)?;
        let comment = comments
            .iter()
            .find(|c| c.id == Some(comment_id))
            .ok_or_else(|| anyhow!("issue-comment {comment_id} not found on PR {pr}"))?;
        return Ok(comment.body.clone());
    }

    if let Some(thread_id) = item_ref.strip_prefix("review-thread:") {
        let threads = gh::review_threads(root, pr)?;
        let thread = threads
            .iter()
            .find(|t| t.id == thread_id)
            .ok_or_else(|| anyhow!("review-thread {thread_id} not found on PR {pr}"))?;
        let body = thread
            .comments
            .nodes
            .first()
            .map_or(String::new(), |c| c.body.clone());
        return Ok(body);
    }

    if let Some(review_id) = item_ref.strip_prefix("review:") {
        let reviews = gh::reviews(root, pr)?;
        let review = reviews
            .iter()
            .find(|r| r.node_id.as_deref() == Some(review_id) || r.id.to_string() == review_id)
            .ok_or_else(|| anyhow!("review {review_id} not found on PR {pr}"))?;
        return Ok(review.body.clone());
    }

    if let Some(finding_fp) = item_ref.strip_prefix("finding:") {
        return Ok(finding_fp.to_owned());
    }

    bail!(
        "unrecognized item_ref format '{item_ref}' (expected issue-comment:ID, review-thread:ID, review:ID, or finding:FP)"
    )
}

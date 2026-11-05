//! `GitHub` issue deduplication and bounded creation for actionable findings.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

use super::model::{Finding, FindingClassification, IssuesMode};

/// Marker prefix embedded in issue bodies for idempotent deduplication.
pub const FINDING_MARKER_PREFIX: &str = "<!-- do-harness:finding:v1 sha256=";
/// Marker suffix embedded in issue bodies.
pub const FINDING_MARKER_SUFFIX: &str = " -->";

#[derive(Debug, Clone, Deserialize)]
struct IssueItem {
    number: u64,
    #[serde(default)]
    body: Option<String>,
    html_url: Option<String>,
}

#[derive(Deserialize)]
struct CreatedIssueResponse {
    number: u64,
    html_url: Option<String>,
}

/// Searches all repository issues (open and closed) for finding fingerprint markers.
///
/// An unreadable issue list yields an empty map: creation then reconciles by
/// searching again, so a failed read cannot silently duplicate an issue.
pub fn search_existing_finding_issues(root: &Path) -> BTreeMap<String, (u64, Option<String>)> {
    let endpoint = "repos/{owner}/{repo}/issues?state=all&per_page=100";
    let Ok(output) = crate::gh::output(
        root,
        &["api", "--paginate", endpoint],
        "api list all issues",
    ) else {
        return BTreeMap::new();
    };

    let pages = serde_json::Deserializer::from_slice(&output.stdout).into_iter::<Vec<IssueItem>>();
    let mut map = BTreeMap::new();

    for page in pages.flatten() {
        for issue in page {
            let Some(body) = &issue.body else { continue };
            if let Some(start) = body.find(FINDING_MARKER_PREFIX) {
                let rest = &body[start + FINDING_MARKER_PREFIX.len()..];
                if let Some(end) = rest.find(FINDING_MARKER_SUFFIX) {
                    let fp = rest[..end].trim().to_owned();
                    map.insert(fp, (issue.number, issue.html_url));
                }
            }
        }
    }

    map
}

/// Matches findings against existing issues and idempotently creates new issues up to the cap.
///
/// # Errors
///
/// Returns an error when creating an issue on `GitHub` fails.
pub fn reconcile_and_create_issues(
    root: &Path,
    pr: u64,
    head_sha: &str,
    findings: &mut [Finding],
    mode: IssuesMode,
    max_new_issues: usize,
    dry_run: bool,
) -> Result<Vec<u64>> {
    let existing_map = search_existing_finding_issues(root);
    let mut created_issue_numbers = Vec::new();
    let effective_cap = max_new_issues.min(20);

    for f in findings.iter_mut() {
        if let Some((num, url)) = existing_map.get(&f.fingerprint) {
            f.issue_url = url.clone().or_else(|| Some(format!("#{num}")));
            continue;
        }

        // Only create issues for actionable unlinked findings
        let should_create = match f.classification {
            FindingClassification::Introduced
            | FindingClassification::Regressed
            | FindingClassification::PreExisting => true,
            FindingClassification::Resolved | FindingClassification::Unknown => false,
        };

        if !should_create {
            continue;
        }

        if mode == IssuesMode::Create && !dry_run && created_issue_numbers.len() < effective_cap {
            let marker = format!(
                "{FINDING_MARKER_PREFIX}{}{FINDING_MARKER_SUFFIX}",
                f.fingerprint
            );
            let title = format!("[finding] {}: {} in {}", f.tool, f.rule_id, f.path);
            let body = format!(
                "{marker}\n\
                 ## CI Finding: {}\n\n\
                 - **Tool:** {}\n\
                 - **Severity:** {}\n\
                 - **Path:** {}:{}\n\
                 - **Message:** {}\n\
                 - **Detected in PR:** #{pr} (head `{head_sha}`)\n\
                 - **Occurrences in CI:** {} run(s)\n",
                f.rule_id,
                f.tool,
                f.severity.as_str(),
                f.path,
                f.start_line.unwrap_or(0),
                f.message,
                f.occurrences.len()
            );

            let res: CreatedIssueResponse = crate::gh::json(
                root,
                &[
                    "api",
                    "-X",
                    "POST",
                    "repos/{owner}/{repo}/issues",
                    "-f",
                    &format!("title={title}"),
                    "-f",
                    &format!("body={body}"),
                ],
                "api create finding issue",
            )
            .context("failed to create finding issue")?;

            f.issue_url = res.html_url.or_else(|| Some(format!("#{}", res.number)));
            created_issue_numbers.push(res.number);
        }
    }

    Ok(created_issue_numbers)
}

//! Evidence collection for CI check-run annotations and code scanning.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;
use serde::Deserialize;

use super::fingerprint::compute_fingerprint;
use super::model::{Finding, FindingClassification, FindingOccurrence, FindingSeverity};
use crate::pr::gh;

#[derive(Debug, Clone, Deserialize)]
pub struct CheckRunAnnotation {
    pub path: String,
    pub start_line: Option<u64>,
    pub end_line: Option<u64>,
    pub annotation_level: String,
    pub message: String,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct CodeScanningAlert {
    pub number: Option<u64>,
    pub rule: Option<CodeScanningRule>,
    pub most_recent_instance: Option<CodeScanningInstance>,
    pub html_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct CodeScanningRule {
    pub id: String,
    pub severity: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CodeScanningInstance {
    pub location: Option<CodeScanningLocation>,
    pub message: Option<CodeScanningMessage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CodeScanningLocation {
    pub path: Option<String>,
    pub start_line: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CodeScanningMessage {
    pub text: Option<String>,
}

/// Parses one annotations payload, tolerating both a single array and
/// concatenated page arrays.
fn parse_annotations(bytes: &[u8]) -> Vec<CheckRunAnnotation> {
    if let Ok(annotations) = serde_json::from_slice::<Vec<CheckRunAnnotation>>(bytes) {
        return annotations;
    }
    let pages = serde_json::Deserializer::from_slice(bytes).into_iter::<Vec<CheckRunAnnotation>>();
    pages.flatten().flatten().collect()
}

/// Reads all check run annotations for a commit SHA.
///
/// # Errors
///
/// Returns an error when the check-run listing for `head_sha` cannot be read.
pub fn collect_annotations(root: &Path, head_sha: &str, repo_slug: &str) -> Result<Vec<Finding>> {
    let check_runs = gh::check_runs_page(root, head_sha)?;
    let mut findings_map: BTreeMap<String, Finding> = BTreeMap::new();

    for run in &check_runs.check_runs {
        let Some(run_id) = run.id else { continue };
        let endpoint = format!("repos/{{owner}}/{{repo}}/check-runs/{run_id}/annotations");
        let Ok(output) =
            crate::gh::output(root, &["api", "--paginate", &endpoint], "api annotations")
        else {
            continue;
        };
        let annotations = parse_annotations(&output.stdout);

        let tool_name = run
            .app
            .as_ref()
            .and_then(|a| a.slug.as_deref())
            .unwrap_or(&run.name);

        for ann in annotations {
            let rule_id = ann
                .title
                .clone()
                .unwrap_or_else(|| "compiler_warning".to_owned());
            let fingerprint = compute_fingerprint(
                repo_slug,
                tool_name,
                &rule_id,
                &ann.path,
                ann.start_line,
                &ann.message,
            );

            let occurrence = FindingOccurrence {
                check_run_id: Some(run_id),
                check_name: Some(run.name.clone()),
                url: run.html_url.clone(),
            };
            let severity = FindingSeverity::from_github(&ann.annotation_level);

            findings_map
                .entry(fingerprint.clone())
                .and_modify(|existing| {
                    existing.occurrences.push(occurrence.clone());
                    if severity > existing.severity {
                        existing.severity = severity;
                    }
                })
                .or_insert_with(|| Finding {
                    fingerprint,
                    source_id: "github-check-run-annotations".to_owned(),
                    tool: tool_name.to_owned(),
                    tool_version: None,
                    rule_id,
                    severity,
                    path: ann.path,
                    start_line: ann.start_line,
                    end_line: ann.end_line,
                    message: ann.message,
                    occurrences: vec![occurrence],
                    classification: FindingClassification::Unknown,
                    issue_url: None,
                    disposition: None,
                });
        }
    }

    Ok(findings_map.into_values().collect())
}

/// Reads Code Scanning alerts for a commit.
///
/// An unreadable or unavailable Code Scanning API yields no findings; the
/// caller records completeness separately rather than inventing a clean set.
#[must_use]
pub fn collect_code_scanning(root: &Path, head_sha: &str, repo_slug: &str) -> Vec<Finding> {
    let endpoint = format!("repos/{{owner}}/{{repo}}/code-scanning/alerts?commit_sha={head_sha}");
    let Ok(output) =
        crate::gh::output(root, &["api", "--paginate", &endpoint], "api code-scanning")
    else {
        return Vec::new();
    };
    let Ok(alerts) = serde_json::from_slice::<Vec<CodeScanningAlert>>(&output.stdout) else {
        return Vec::new();
    };

    alerts
        .into_iter()
        .map(|alert| {
            let rule_id = alert
                .rule
                .as_ref()
                .map_or_else(|| "code_scanning".to_owned(), |r| r.id.clone());
            let severity = FindingSeverity::from_github(
                alert
                    .rule
                    .as_ref()
                    .and_then(|r| r.severity.as_deref())
                    .unwrap_or("warning"),
            );
            let location = alert
                .most_recent_instance
                .as_ref()
                .and_then(|i| i.location.as_ref());
            let path = location
                .and_then(|l| l.path.clone())
                .unwrap_or_else(|| "repository".to_owned());
            let start_line = location.and_then(|l| l.start_line);
            let message = alert
                .most_recent_instance
                .as_ref()
                .and_then(|i| i.message.as_ref())
                .and_then(|m| m.text.clone())
                .unwrap_or_default();
            let fingerprint =
                compute_fingerprint(repo_slug, "CodeQL", &rule_id, &path, start_line, &message);

            Finding {
                fingerprint,
                source_id: "github-code-scanning".to_owned(),
                tool: "CodeQL".to_owned(),
                tool_version: None,
                rule_id,
                severity,
                path,
                start_line,
                end_line: None,
                message,
                occurrences: vec![FindingOccurrence {
                    check_run_id: None,
                    check_name: Some("CodeQL".to_owned()),
                    url: alert.html_url.clone(),
                }],
                classification: FindingClassification::Unknown,
                issue_url: alert.html_url,
                disposition: None,
            }
        })
        .collect()
}

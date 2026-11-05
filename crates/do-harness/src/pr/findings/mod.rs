//! Findings provenance and deduplicated issue tracking (#470).

pub mod baseline;
pub mod collector;
pub mod fingerprint;
pub mod issue;
pub mod model;

use std::path::Path;

use anyhow::{Context, Result, anyhow};

#[allow(unused_imports)]
pub use fingerprint::{compute_findings_digest, compute_fingerprint};
#[allow(unused_imports)]
pub use issue::{FINDING_MARKER_PREFIX, FINDING_MARKER_SUFFIX};
#[allow(unused_imports)]
pub use model::{
    Finding, FindingClassification, FindingOccurrence, FindingSeverity, FindingsReport, IssuesMode,
};

use crate::CliError;
use crate::pr::gh;
use crate::pr::policy::POLICY_PATH;
use crate::pr::policy::model::PrMergePolicy;
use crate::report::Format;

/// Collects head and base findings and classifies each head finding.
///
/// Returns the classified head findings and whether the base baseline was
/// verifiable. An empty or unresolvable base SHA yields `false` and leaves
/// every finding `UNKNOWN`: a missing baseline is never reported as clean.
///
/// # Errors
///
/// Returns an error when the head observation cannot be read.
pub fn collect_and_classify(
    root: &Path,
    head_sha: &str,
    base_sha: &str,
    repo_slug: &str,
) -> Result<(Vec<Finding>, bool)> {
    let mut head_findings = collector::collect_annotations(root, head_sha, repo_slug)?;
    head_findings.extend(collector::collect_code_scanning(root, head_sha, repo_slug));

    let base_verified = !base_sha.trim().is_empty();
    let base_findings = if base_verified {
        let mut base =
            collector::collect_annotations(root, base_sha, repo_slug).unwrap_or_default();
        base.extend(collector::collect_code_scanning(root, base_sha, repo_slug));
        base
    } else {
        Vec::new()
    };

    baseline::compare_baseline(&mut head_findings, &base_findings, base_verified);
    Ok((head_findings, base_verified))
}

/// Evaluates findings for PR, comparing against baseline and reconciling issues.
///
/// # Errors
///
/// Returns an error when querying PR view or required APIs fails.
pub fn evaluate_findings(
    root: &Path,
    pr: u64,
    issues_mode: IssuesMode,
    dry_run: bool,
) -> Result<FindingsReport> {
    let view = gh::readiness_view(root, pr).context("failed to fetch PR readiness view")?;
    let repo_slug = "repository"; // normalized neutral default

    let policy_path = root.join(POLICY_PATH);
    let policy_opt: Option<PrMergePolicy> = std::fs::read(&policy_path)
        .ok()
        .and_then(|bytes| toml::from_slice(&bytes).ok());

    let policy_kind = policy_opt
        .as_ref()
        .and_then(|p| p.findings.as_ref())
        .map_or("managed-backlog", |f| f.policy.as_str());

    let max_new_issues = policy_opt
        .as_ref()
        .and_then(|p| p.findings.as_ref())
        .and_then(|f| f.max_new_issues_per_run)
        .unwrap_or(20) as usize;

    let (mut head_findings, base_verified) =
        collect_and_classify(root, &view.head_ref_oid, &view.base_ref_oid, repo_slug)?;

    // Reconcile and create issues
    let issues_created = issue::reconcile_and_create_issues(
        root,
        pr,
        &view.head_ref_oid,
        &mut head_findings,
        issues_mode,
        max_new_issues,
        dry_run,
    )?;

    let warnings = head_findings
        .iter()
        .filter(|f| {
            f.severity == FindingSeverity::Warning
                || f.severity == FindingSeverity::Error
                || f.severity == FindingSeverity::Critical
        })
        .count();

    let baseline_count = head_findings
        .iter()
        .filter(|f| f.classification == FindingClassification::PreExisting)
        .count();

    let untracked_count = head_findings
        .iter()
        .filter(|f| f.issue_url.is_none() && f.disposition.is_none())
        .count();

    let findings_digest = compute_findings_digest(&head_findings);

    Ok(FindingsReport {
        schema_version: 1,
        pr,
        head_sha: view.head_ref_oid,
        base_sha: view.base_ref_oid,
        base_verified,
        policy: policy_kind.to_owned(),
        completeness: "complete".to_owned(),
        warnings,
        baseline_findings: baseline_count,
        untracked_findings: untracked_count,
        issues_created,
        findings: head_findings,
        findings_digest,
    })
}

/// Executes `do-harness pr findings`.
///
/// # Errors
///
/// Returns `CliError::Usage` or `CliError::Verify` on failure.
pub fn run(
    root: &Path,
    pr: u64,
    issues_mode: IssuesMode,
    format: Format,
    dry_run: bool,
) -> Result<(), CliError> {
    let report = evaluate_findings(root, pr, issues_mode, dry_run).map_err(CliError::Usage)?;

    if format == Format::Json {
        let json =
            serde_json::to_string_pretty(&report).map_err(|e| CliError::Usage(anyhow!("{e}")))?;
        println!("{json}");
    } else {
        println!("PR {pr} Findings Report");
        println!("======================");
        println!("Policy: {}", report.policy);
        println!("Base verified: {}", report.base_verified);
        println!("Total findings: {}", report.findings.len());
        println!("Warnings: {}", report.warnings);
        println!("Baseline findings: {}", report.baseline_findings);
        println!("Untracked findings: {}", report.untracked_findings);
        if !report.issues_created.is_empty() {
            println!("Issues created: {:?}", report.issues_created);
        }
    }

    Ok(())
}

//! PR merge policy evaluation and drift detection.
//!
//! Compares server-side ruleset configuration against `plans/pr-merge-policy.toml`.

use std::path::Path;

use anyhow::{Result, anyhow};
use sha2::{Digest, Sha256};

use crate::CliError;
use crate::report::Format;

pub mod check;
pub mod model;

#[allow(unused_imports)]
pub use model::{
    BypassActorConfig, CodeQualityPolicy, CodeScanningToolPolicy, FindingsPolicy,
    FindingsSourceConfig, OtherRulesPolicy, PolicyDecision, PolicyFinding, PrMergePolicy,
    PrPolicyReport, PullRequestPolicy, RequiredStatusCheckItem, RequiredStatusChecksPolicy,
};

use check::{check_branch_rules, check_bypass, check_conditions, check_metadata, check_rules};
use model::{BranchRule, RulesetDetail, RulesetSummary};

/// Path to the repository PR merge policy relative to the workspace root.
pub const POLICY_PATH: &str = "plans/pr-merge-policy.toml";

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn unknown_report(
    ruleset_id: Option<u64>,
    policy_sha: Option<String>,
    finding: PolicyFinding,
) -> PrPolicyReport {
    PrPolicyReport {
        schema_version: 1,
        decision: PolicyDecision::Unknown,
        ruleset_id,
        policy_sha256: policy_sha,
        findings: vec![finding],
    }
}

/// Evaluates active server rulesets against `plans/pr-merge-policy.toml`.
///
/// # Errors
///
/// Returns an error only on unrecoverable local invariant failures; API
/// errors and drift are reported in [`PrPolicyReport`].
#[allow(clippy::unnecessary_wraps, clippy::too_many_lines)]
pub fn evaluate(root: &Path) -> Result<PrPolicyReport> {
    let policy_path = root.join(POLICY_PATH);
    let policy_bytes = match std::fs::read(&policy_path) {
        Ok(bytes) => bytes,
        Err(err) => {
            return Ok(unknown_report(
                None,
                None,
                PolicyFinding {
                    rule_id: None,
                    rule_type: "policy_file".to_owned(),
                    field: POLICY_PATH.to_owned(),
                    expected: "readable pr-merge-policy.toml".to_owned(),
                    actual: format!("read error: {err:#}"),
                    remediation: format!("Create or restore {POLICY_PATH}"),
                },
            ));
        }
    };

    let policy_sha256 = sha256_hex(&policy_bytes);
    let policy: PrMergePolicy = match toml::from_slice(&policy_bytes) {
        Ok(p) => p,
        Err(err) => {
            return Ok(unknown_report(
                None,
                Some(policy_sha256),
                PolicyFinding {
                    rule_id: None,
                    rule_type: "policy_file".to_owned(),
                    field: POLICY_PATH.to_owned(),
                    expected: "valid pr-merge-policy.toml".to_owned(),
                    actual: format!("parse error: {err:#}"),
                    remediation: format!("Correct syntax in {POLICY_PATH}"),
                },
            ));
        }
    };

    let rulesets: Vec<RulesetSummary> = match crate::gh::json(
        root,
        &["api", "repos/{owner}/{repo}/rulesets"],
        "api rulesets",
    ) {
        Ok(list) => list,
        Err(err) => {
            return Ok(unknown_report(
                Some(policy.ruleset_id),
                Some(policy_sha256),
                PolicyFinding {
                    rule_id: Some(policy.ruleset_id),
                    rule_type: "rulesets".to_owned(),
                    field: "api".to_owned(),
                    expected: "accessible repository rulesets".to_owned(),
                    actual: format!("{err:#}"),
                    remediation: "Verify gh auth and ruleset read permissions".to_owned(),
                },
            ));
        }
    };

    if !rulesets.iter().any(|r| r.id == policy.ruleset_id) {
        return Ok(PrPolicyReport {
            schema_version: 1,
            decision: PolicyDecision::Drift,
            ruleset_id: Some(policy.ruleset_id),
            policy_sha256: Some(policy_sha256),
            findings: vec![PolicyFinding {
                rule_id: Some(policy.ruleset_id),
                rule_type: "ruleset".to_owned(),
                field: "id".to_owned(),
                expected: format!("{}", policy.ruleset_id),
                actual: "ruleset not found in repository rulesets".to_owned(),
                remediation: format!("Restore or configure ruleset {}", policy.ruleset_id),
            }],
        });
    }

    let detail_endpoint = format!("repos/{{owner}}/{{repo}}/rulesets/{}", policy.ruleset_id);
    let detail: RulesetDetail =
        match crate::gh::json(root, &["api", &detail_endpoint], "api ruleset detail") {
            Ok(d) => d,
            Err(err) => {
                return Ok(unknown_report(
                    Some(policy.ruleset_id),
                    Some(policy_sha256),
                    PolicyFinding {
                        rule_id: Some(policy.ruleset_id),
                        rule_type: "ruleset_detail".to_owned(),
                        field: "api".to_owned(),
                        expected: format!("accessible ruleset {}", policy.ruleset_id),
                        actual: format!("{err:#}"),
                        remediation: "Check permissions for ruleset inspection".to_owned(),
                    },
                ));
            }
        };

    let branch_rules: Vec<BranchRule> = match crate::gh::json(
        root,
        &["api", "repos/{owner}/{repo}/rules/branches/main"],
        "api branch rules",
    ) {
        Ok(rules) => rules,
        Err(err) => {
            return Ok(unknown_report(
                Some(policy.ruleset_id),
                Some(policy_sha256),
                PolicyFinding {
                    rule_id: Some(policy.ruleset_id),
                    rule_type: "branch_rules".to_owned(),
                    field: "api".to_owned(),
                    expected: "accessible branch rules for main".to_owned(),
                    actual: format!("{err:#}"),
                    remediation: "Verify branch protection and ruleset query access".to_owned(),
                },
            ));
        }
    };

    let mut findings = Vec::new();
    check_metadata(&policy, &detail, &mut findings);
    check_conditions(&policy, &detail, &mut findings);
    check_bypass(&policy, &detail, &mut findings);
    check_rules(&policy, &detail, &mut findings);
    check_branch_rules(&policy, &branch_rules, &mut findings);

    let decision = if findings.is_empty() {
        PolicyDecision::Pass
    } else {
        PolicyDecision::Drift
    };

    Ok(PrPolicyReport {
        schema_version: 1,
        decision,
        ruleset_id: Some(policy.ruleset_id),
        policy_sha256: Some(policy_sha256),
        findings,
    })
}

fn render_text(report: &PrPolicyReport) {
    println!("PR Merge Policy Report");
    println!("======================");
    println!("Decision: {:?}", report.decision);
    if let Some(id) = report.ruleset_id {
        println!("Ruleset ID: {id}");
    }
    if let Some(sha) = &report.policy_sha256 {
        println!("Policy SHA-256: {sha}");
    }
    if report.findings.is_empty() {
        println!("\nAll PR merge policy rules match active server rulesets.");
    } else {
        println!("\nFindings ({}):", report.findings.len());
        for f in &report.findings {
            println!(
                "  - [{}] {}: expected '{}', got '{}'",
                f.rule_type, f.field, f.expected, f.actual
            );
            println!("    Remediation: {}", f.remediation);
        }
    }
}

/// Executes `doctor pr-policy` check and emits reports.
///
/// # Errors
///
/// Returns `CliError::Verify` on drift (exit 1), or `CliError::Usage` on
/// indeterminate/unknown status (exit 2).
pub fn run(root: &Path, format: Format) -> Result<(), CliError> {
    let report = match evaluate(root) {
        Ok(r) => r,
        Err(err) => {
            let unknown = unknown_report(
                None,
                None,
                PolicyFinding {
                    rule_id: None,
                    rule_type: "evaluation".to_owned(),
                    field: "evaluate".to_owned(),
                    expected: "successful policy evaluation".to_owned(),
                    actual: format!("{err:#}"),
                    remediation: "Investigate policy file or gh CLI configuration".to_owned(),
                },
            );
            if format == Format::Json {
                let _ = serde_json::to_string_pretty(&unknown).map(|json| println!("{json}"));
            } else {
                render_text(&unknown);
            }
            return Err(CliError::Usage(err));
        }
    };

    if format == Format::Json {
        let json =
            serde_json::to_string_pretty(&report).map_err(|e| CliError::Usage(anyhow!("{e}")))?;
        println!("{json}");
    } else {
        render_text(&report);
    }

    match report.decision {
        PolicyDecision::Pass => Ok(()),
        PolicyDecision::Drift => Err(CliError::Verify(anyhow!(
            "PR merge policy drift detected ({} finding(s))",
            report.findings.len()
        ))),
        PolicyDecision::Unknown => Err(CliError::Usage(anyhow!(
            "PR merge policy evaluation is indeterminate or unavailable"
        ))),
    }
}

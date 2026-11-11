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

/// Explains what the absent overlay means, for the derived-mode warning.
pub const POLICY_ABSENT_HINT: &str =
    "add plans/pr-merge-policy.toml to pin these rules and turn the derived mode strict";

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

/// Reports the server-derived merge rules for a repository with no overlay.
///
/// # Errors
///
/// Returns [`CliError::Usage`] (exit 2) when the ruleset cannot be read, which
/// is the same fail-closed verdict the merge path takes.
fn run_derived(root: &Path, format: Format) -> Result<(), CliError> {
    let rules = super::rules::MergeRules::load(root, "main").map_err(|err| {
        CliError::Usage(anyhow!(
            "the server merge rules could not be derived: {err:#}"
        ))
    })?;
    if format == Format::Json {
        let json =
            serde_json::to_string_pretty(&rules).map_err(|e| CliError::Usage(anyhow!("{e}")))?;
        println!("{json}");
        return Ok(());
    }
    println!("PR Merge Rules (derived from the server)");
    println!("=========================================");
    println!("Source: server ruleset (no {POLICY_PATH} present)");
    println!("Rules SHA-256: {}", rules.digest);
    println!(
        "Strict up-to-date: {}",
        if rules.strict_up_to_date { "yes" } else { "no" }
    );
    println!(
        "Linear history: {}",
        if rules.linear_history { "yes" } else { "no" }
    );
    println!(
        "Review threads must resolve: {}",
        if rules.thread_resolution { "yes" } else { "no" }
    );
    let methods: Vec<&str> = rules.usable_methods().iter().map(|m| m.as_str()).collect();
    println!("Usable merge methods: {}", methods.join(", "));
    if rules.has_no_required_checks() {
        println!("\nWARNING: the ruleset requires no status checks.");
        println!(
            "An empty required set is not proof that verification ran; add {POLICY_PATH} to tighten."
        );
    } else {
        println!("\nRequired status checks ({}):", rules.required.len());
        for required in &rules.required {
            match required.integration_id {
                Some(id) => println!("  - {} (app {id})", required.context),
                None => println!("  - {} (any app)", required.context),
            }
        }
    }
    if rules.merge_method_unavailable() {
        return Err(CliError::Verify(anyhow!(
            "the ruleset permits only merge methods the repository disables"
        )));
    }
    Ok(())
}

/// Executes `doctor pr-policy` check and emits reports.
///
/// # Errors
///
/// Returns `CliError::Verify` on drift (exit 1), or `CliError::Usage` on
/// indeterminate/unknown status (exit 2).
pub fn run(root: &Path, format: Format) -> Result<(), CliError> {
    // An adopting repository has no overlay and never will. Rather than
    // reporting an indeterminate policy, describe the rules the server actually
    // enforces, read-only: this is a report command, so it neither gates a merge
    // nor writes anything.
    if !root.join(POLICY_PATH).exists() {
        return run_derived(root, format);
    }
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

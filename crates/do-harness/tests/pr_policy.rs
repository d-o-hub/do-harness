//! Integration tests for `do-harness doctor pr-policy`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::unreadable_literal)]

#[cfg(unix)]
use serde_json::{Value, json};
#[cfg(unix)]
use std::path::Path;
#[cfg(unix)]
use std::process::Command;

#[cfg(unix)]
mod support;
#[cfg(unix)]
use support::pr_readiness::fake_gh_router;

#[cfg(unix)]
const VALID_POLICY_TOML: &str = r#"
schema_version = 1
ruleset_id = 22012548
name = "main"
target = "branch"
source_type = "Repository"
source = "acme/widget"
enforcement = "active"
include_refs = ["~DEFAULT_BRANCH"]
exclude_refs = []
bypass_actors = []
current_user_can_bypass = "never"

[required_status_checks]
strict_required_status_checks_policy = true
do_not_enforce_on_create = true
required = [
  { context = "pr-required", integration_id = 15368 },
  { context = "semver", integration_id = 15368 },
]

[pull_request]
required_approving_review_count = 0
required_review_thread_resolution = true
dismiss_stale_reviews_on_push = false
required_reviewers = []
require_code_owner_review = false
require_last_push_approval = false
require_extra_approval_for_unattributed_changes = false
allowed_merge_methods = ["merge", "squash", "rebase"]

[[code_scanning_tools]]
tool = "CodeQL"
security_alerts_threshold = "high_or_higher"
alerts_threshold = "errors_and_warnings"

[code_quality]
severity = "errors"

[other_rules]
deletion = true
"#;

#[cfg(unix)]
fn write_policy(root: &Path, content: &str) {
    let plans = root.join("plans");
    std::fs::create_dir_all(&plans).unwrap();
    std::fs::write(plans.join("pr-merge-policy.toml"), content).unwrap();
}

#[cfg(unix)]
fn standard_matching_detail() -> Value {
    json!({
        "id": 22012548,
        "name": "main",
        "target": "branch",
        "source_type": "Repository",
        "source": "acme/widget",
        "enforcement": "active",
        "conditions": {
            "ref_name": {
                "include": ["~DEFAULT_BRANCH"],
                "exclude": []
            }
        },
        "bypass_actors": [],
        "current_user_can_bypass": "never",
        "rules": [
            {
                "type": "deletion"
            },
            {
                "type": "code_scanning",
                "parameters": {
                    "code_scanning_tools": [
                        {
                            "tool": "CodeQL",
                            "security_alerts_threshold": "high_or_higher",
                            "alerts_threshold": "errors_and_warnings"
                        }
                    ]
                }
            },
            {
                "type": "code_quality",
                "parameters": {
                    "severity": "errors"
                }
            },
            {
                "type": "pull_request",
                "parameters": {
                    "required_approving_review_count": 0,
                    "dismiss_stale_reviews_on_push": false,
                    "required_reviewers": [],
                    "require_code_owner_review": false,
                    "require_last_push_approval": false,
                    "required_review_thread_resolution": true,
                    "require_extra_approval_for_unattributed_changes": false,
                    "allowed_merge_methods": ["merge", "squash", "rebase"]
                }
            },
            {
                "type": "required_status_checks",
                "parameters": {
                    "strict_required_status_checks_policy": true,
                    "do_not_enforce_on_create": true,
                    "required_status_checks": [
                        { "context": "pr-required", "integration_id": 15368 },
                        { "context": "semver", "integration_id": 15368 }
                    ]
                }
            }
        ]
    })
}

#[cfg(unix)]
fn standard_matching_branch_rules() -> Value {
    json!([
        {
            "type": "deletion",
            "ruleset_id": 22012548,
            "ruleset_source": "acme/widget",
            "ruleset_source_type": "Repository"
        },
        {
            "type": "pull_request",
            "ruleset_id": 22012548,
            "ruleset_source": "acme/widget",
            "ruleset_source_type": "Repository"
        },
        {
            "type": "required_status_checks",
            "ruleset_id": 22012548,
            "ruleset_source": "acme/widget",
            "ruleset_source_type": "Repository"
        }
    ])
}

#[cfg(unix)]
fn run_doctor_pr_policy(root: &Path, bin: &Path) -> (Option<i32>, String, String) {
    let path = std::env::var("PATH").unwrap_or_default();
    let output = Command::new(env!("CARGO_BIN_EXE_do-harness"))
        .arg("--root")
        .arg(root)
        .arg("doctor")
        .arg("pr-policy")
        .arg("--format")
        .arg("json")
        .env("PATH", format!("{}:{path}", bin.display()))
        .output()
        .expect("spawn do-harness");

    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[cfg(unix)]
#[test]
fn passing_policy_matches_and_exits_0() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    write_policy(root, VALID_POLICY_TOML);
    std::fs::write(
        root.join("rulesets.json"),
        json!([{"id": 22012548}]).to_string(),
    )
    .unwrap();
    std::fs::write(
        root.join("ruleset_detail.json"),
        standard_matching_detail().to_string(),
    )
    .unwrap();
    std::fs::write(
        root.join("branch_rules.json"),
        standard_matching_branch_rules().to_string(),
    )
    .unwrap();

    let (code, stdout, stderr) = run_doctor_pr_policy(root, &bin);
    assert_eq!(code, Some(0), "expected exit 0 on pass: {stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["decision"], "PASS");
    assert_eq!(report["ruleset_id"], 22012548);
    assert!(report["policy_sha256"].as_str().is_some());
    assert_eq!(report["findings"].as_array().unwrap().len(), 0);
}

#[cfg(unix)]
#[test]
fn drift_in_bypass_and_status_checks_exits_1() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    write_policy(root, VALID_POLICY_TOML);
    std::fs::write(
        root.join("rulesets.json"),
        json!([{"id": 22012548}]).to_string(),
    )
    .unwrap();

    let mut detail = standard_matching_detail();
    // Simulate current server drift: actor 5 has bypass_mode: always, current_user_can_bypass: always,
    // and required_status_checks has only 'verify' (missing 'pr-required' and 'semver').
    detail["bypass_actors"] = json!([
        {
            "actor_id": 5,
            "actor_type": "RepositoryRole",
            "bypass_mode": "always"
        }
    ]);
    detail["current_user_can_bypass"] = json!("always");
    detail["rules"][4]["parameters"]["required_status_checks"] = json!([
        { "context": "verify", "integration_id": 15368 }
    ]);
    std::fs::write(root.join("ruleset_detail.json"), detail.to_string()).unwrap();
    std::fs::write(
        root.join("branch_rules.json"),
        standard_matching_branch_rules().to_string(),
    )
    .unwrap();

    let (code, stdout, _stderr) = run_doctor_pr_policy(root, &bin);
    assert_eq!(code, Some(1), "expected exit 1 on drift");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["decision"], "DRIFT");
    let findings = report["findings"].as_array().unwrap();
    assert!(findings.iter().any(|f| f["rule_type"] == "bypass_actors"));
    assert!(findings.iter().any(|f| f["rule_type"] == "bypass"));
    assert!(
        findings
            .iter()
            .any(|f| f["field"] == "required[pr-required]")
    );
    assert!(findings.iter().any(|f| f["field"] == "required[semver]"));
    assert!(
        findings
            .iter()
            .any(|f| f["field"] == "unexpected_check[verify]")
    );
}

#[cfg(unix)]
#[test]
fn drift_in_thread_resolution_and_deletion_exits_1() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    write_policy(root, VALID_POLICY_TOML);
    std::fs::write(
        root.join("rulesets.json"),
        json!([{"id": 22012548}]).to_string(),
    )
    .unwrap();

    let mut detail = standard_matching_detail();
    detail["rules"][3]["parameters"]["required_review_thread_resolution"] = json!(false);
    // Remove deletion rule
    detail["rules"]
        .as_array_mut()
        .unwrap()
        .retain(|r| r["type"] != "deletion");
    std::fs::write(root.join("ruleset_detail.json"), detail.to_string()).unwrap();
    std::fs::write(
        root.join("branch_rules.json"),
        standard_matching_branch_rules().to_string(),
    )
    .unwrap();

    let (code, stdout, _stderr) = run_doctor_pr_policy(root, &bin);
    assert_eq!(code, Some(1), "expected exit 1 on drift");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["decision"], "DRIFT");
    let findings = report["findings"].as_array().unwrap();
    assert!(
        findings
            .iter()
            .any(|f| f["field"] == "required_review_thread_resolution")
    );
    assert!(findings.iter().any(|f| f["rule_type"] == "deletion"));
}

#[cfg(unix)]
#[test]
fn unexpected_inherited_branch_ruleset_is_drift() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    write_policy(root, VALID_POLICY_TOML);
    std::fs::write(
        root.join("rulesets.json"),
        json!([{"id": 22012548}]).to_string(),
    )
    .unwrap();
    std::fs::write(
        root.join("ruleset_detail.json"),
        standard_matching_detail().to_string(),
    )
    .unwrap();

    let mut branch_rules = standard_matching_branch_rules();
    // Simulate an unexpected org-level ruleset rule (id 999999)
    branch_rules.as_array_mut().unwrap().push(json!({
        "type": "pull_request",
        "ruleset_id": 999999,
        "ruleset_source": "acme-corp",
        "ruleset_source_type": "Organization"
    }));
    std::fs::write(root.join("branch_rules.json"), branch_rules.to_string()).unwrap();

    let (code, stdout, _stderr) = run_doctor_pr_policy(root, &bin);
    assert_eq!(
        code,
        Some(1),
        "expected exit 1 on unexpected branch ruleset"
    );
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["decision"], "DRIFT");
    let findings = report["findings"].as_array().unwrap();
    assert!(
        findings
            .iter()
            .any(|f| f["field"] == "applicable_ruleset" && f["rule_id"] == 999999)
    );
}

#[cfg(unix)]
#[test]
fn a_malformed_policy_file_is_unknown_and_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    // A file that exists but cannot be parsed is still an error: the operator
    // wrote an overlay and it is wrong, which is not the same as never having
    // written one (#515 made only *absence* non-fatal).
    write_policy(root, "schema_version = 'not a number'");
    let (code, stdout, _stderr) = run_doctor_pr_policy(root, &bin);
    assert_eq!(code, Some(2), "malformed policy file must exit 2");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["decision"], "UNKNOWN");
    assert_eq!(report["findings"][0]["rule_type"], "policy_file");
}

#[cfg(unix)]
#[test]
fn a_missing_policy_file_falls_back_to_the_server_rules() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    // No overlay, but the ruleset fixtures are present, so the derived mode has
    // something to read. `run_pr_without_policy` seeds exactly those.
    support::pr_readiness::ensure_ruleset_fixtures(root);

    let (code, stdout, stderr) = run_doctor_pr_policy(root, &bin);
    assert_eq!(
        code,
        Some(0),
        "an absent overlay must report the derived rules instead of UNKNOWN: {stderr}\n{stdout}"
    );
    let report: Value = serde_json::from_str(&stdout).expect("JSON rules report");
    assert_eq!(
        report["required"][0]["context"], "pr-required",
        "the derived report names the ruleset's required contexts"
    );
    assert!(
        report["digest"]
            .as_str()
            .unwrap_or_default()
            .starts_with("sha256:"),
        "the derived rules carry a digest"
    );
}

#[cfg(unix)]
#[test]
fn api_failures_permission_denial_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    write_policy(root, VALID_POLICY_TOML);

    // 1. Fail rulesets listing (e.g. 403)
    std::fs::write(root.join("fail_rulesets"), "").unwrap();
    let (code, stdout, _stderr) = run_doctor_pr_policy(root, &bin);
    assert_eq!(code, Some(2), "403 on rulesets list must exit 2");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["decision"], "UNKNOWN");

    std::fs::remove_file(root.join("fail_rulesets")).unwrap();
    std::fs::write(
        root.join("rulesets.json"),
        json!([{"id": 22012548}]).to_string(),
    )
    .unwrap();

    // 2. Fail ruleset detail (403)
    std::fs::write(root.join("fail_ruleset_detail"), "").unwrap();
    let (code2, stdout2, _stderr2) = run_doctor_pr_policy(root, &bin);
    assert_eq!(code2, Some(2), "403 on detail must exit 2");
    let report2: Value = serde_json::from_str(&stdout2).expect("JSON report");
    assert_eq!(report2["decision"], "UNKNOWN");

    std::fs::remove_file(root.join("fail_ruleset_detail")).unwrap();
    std::fs::write(
        root.join("ruleset_detail.json"),
        standard_matching_detail().to_string(),
    )
    .unwrap();

    // 3. Fail branch rules (403)
    std::fs::write(root.join("fail_branch_rules"), "").unwrap();
    let (code3, stdout3, _stderr3) = run_doctor_pr_policy(root, &bin);
    assert_eq!(code3, Some(2), "403 on branch rules must exit 2");
    let report3: Value = serde_json::from_str(&stdout3).expect("JSON report");
    assert_eq!(report3["decision"], "UNKNOWN");
}

//! Mock `gh` payload fixtures for the `pr` acceptance tests.
//!
//! Split out of `pr_readiness` to keep each support module under the 500-line
//! ceiling. These write the JSON a fake `gh` serves; nothing here runs a
//! command.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::{Value, json};
use std::path::Path;

/// Base SHA the fixtures report; 40 hex characters, as the real API always does.
pub const BASE: &str = "1111111111111111111111111111111111111111";
/// Head SHA the fixtures report.
pub const HEAD: &str = "2222222222222222222222222222222222222222";

/// Default merge-policy and ruleset fixtures.
///
/// `pr ready` refuses to authorize without a readable `plans/pr-merge-policy.toml`
/// and a matching server ruleset (that strictness is proven by `pr_policy`),
/// so every readiness fixture seeds a coherent pair once. A test that wants to
/// prove the missing-policy or drift behaviour overwrites these files or calls
/// the binary through its own runner instead of this helper.
pub fn ensure_authorization_fixtures(root: &Path) {
    let plans = root.join("plans");
    std::fs::create_dir_all(&plans).unwrap();
    let policy = plans.join("pr-merge-policy.toml");
    if !policy.exists() {
        std::fs::write(
            &policy,
            r#"schema_version = 1
ruleset_id = 1
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
"#,
        )
        .unwrap();
    }

    let rulesets = root.join("rulesets.json");
    if !rulesets.exists() {
        std::fs::write(&rulesets, r#"[{"id":1}]"#).unwrap();
    }
    let detail = root.join("ruleset_detail.json");
    if !detail.exists() {
        std::fs::write(
            &detail,
            r#"{
  "id": 1,
  "name": "main",
  "target": "branch",
  "enforcement": "active",
  "conditions": {"ref_name": {"include": ["~DEFAULT_BRANCH"], "exclude": []}},
  "bypass_actors": [],
  "current_user_can_bypass": "never",
  "rules": [
    {"type": "deletion"},
    {"type": "code_scanning", "parameters": {"code_scanning_tools": [{"tool": "CodeQL", "security_alerts_threshold": "high_or_higher", "alerts_threshold": "errors_and_warnings"}]}},
    {"type": "code_quality", "parameters": {"severity": "errors"}},
    {"type": "pull_request", "parameters": {"required_approving_review_count": 0, "dismiss_stale_reviews_on_push": false, "required_reviewers": [], "require_code_owner_review": false, "require_last_push_approval": false, "required_review_thread_resolution": true, "require_extra_approval_for_unattributed_changes": false, "allowed_merge_methods": ["merge", "squash", "rebase"]}},
    {"type": "required_status_checks", "parameters": {"strict_required_status_checks_policy": true, "do_not_enforce_on_create": true, "required_status_checks": [{"context": "pr-required", "integration_id": 15368}, {"context": "semver", "integration_id": 15368}]}}
  ]
}"#,
        )
        .unwrap();
    }
    ensure_ruleset_fixtures(root);
}

/// Seeds the server-side fixtures the derived mode reads (`rules/branches/main`
/// plus the repository settings), without writing a local overlay.
///
/// The required contexts use the `github-actions` app id so a fixture that
/// passes `pr-required` and `semver` from that app satisfies the derived rules;
/// the impersonation tests narrow this deliberately.
pub fn ensure_ruleset_fixtures(root: &Path) {
    let branch_rules = root.join("branch_rules.json");
    if !branch_rules.exists() {
        std::fs::write(
            &branch_rules,
            r#"[{"type":"deletion","ruleset_id":1,"ruleset_source":"acme/widget","ruleset_source_type":"Repository"},
                {"type":"pull_request","ruleset_id":1,"ruleset_source":"acme/widget","ruleset_source_type":"Repository","parameters":{"allowed_merge_methods":["merge","squash","rebase"],"required_review_thread_resolution":true}},
                {"type":"required_status_checks","ruleset_id":1,"ruleset_source":"acme/widget","ruleset_source_type":"Repository","parameters":{"strict_required_status_checks_policy":true,"required_status_checks":[{"context":"pr-required","integration_id":15368},{"context":"semver","integration_id":15368}]}}]"#,
        )
        .unwrap();
    }
    let repo = root.join("repo_settings.json");
    if !repo.exists() {
        std::fs::write(
            &repo,
            r#"{"allow_merge_commit":true,"allow_squash_merge":true,"allow_rebase_merge":true}"#,
        )
        .unwrap();
    }
}

pub fn write_view(root: &Path, pr: u64, overrides: &Value) {
    let mut view = json!({
        "number": pr,
        "baseRefName": "main",
        "baseRefOid": BASE,
        "headRefName": "topic",
        "headRefOid": HEAD,
        "mergeStateStatus": "CLEAN",
        "mergeable": "MERGEABLE",
        "autoMergeRequest": null,
        "reviewDecision": "",
        "state": "OPEN",
        "isDraft": false,
        "labels": [],
        "headRepository": { "nameWithOwner": "acme/widget" }
    });
    if let (Some(target), Some(source)) = (view.as_object_mut(), overrides.as_object()) {
        for (key, value) in source {
            target.insert(key.clone(), value.clone());
        }
    }
    std::fs::write(root.join("view.json"), view.to_string()).unwrap();
}

/// A green head that satisfies the default overlay's required contexts.
///
/// `ensure_authorization_fixtures` declares `pr-required` and `semver` as
/// required, and a readiness snapshot now resolves the required set from the
/// merge policy — so a fixture named `verify` alone would be a *missing
/// required check*, not a green suite. Both contexts are reported from the
/// `github-actions` app id the ruleset names.
pub fn green_checks(root: &Path) {
    std::fs::write(
        root.join("check_runs.json"),
        json!({
            "total_count": 3,
            "check_runs": [
                {
                    "id": 1,
                    "name": "verify",
                    "status": "completed",
                    "conclusion": "success",
                    "app": { "id": 15368, "slug": "github-actions" }
                },
                {
                    "id": 2,
                    "name": "pr-required",
                    "status": "completed",
                    "conclusion": "success",
                    "app": { "id": 15368, "slug": "github-actions" }
                },
                {
                    "id": 3,
                    "name": "semver",
                    "status": "completed",
                    "conclusion": "success",
                    "app": { "id": 15368, "slug": "github-actions" }
                }
            ]
        })
        .to_string(),
    )
    .unwrap();
}

/// The two contexts the fixture ruleset requires, both from the app it names.
pub fn ruleset_checks(root: &Path, app_id: u64) {
    std::fs::write(
        root.join("check_runs.json"),
        json!({
            "total_count": 2,
            "check_runs": [
                {
                    "id": 1,
                    "name": "pr-required",
                    "status": "completed",
                    "conclusion": "success",
                    "app": { "id": app_id, "slug": "github-actions" }
                },
                {
                    "id": 2,
                    "name": "semver",
                    "status": "completed",
                    "conclusion": "success",
                    "app": { "id": app_id, "slug": "github-actions" }
                }
            ]
        })
        .to_string(),
    )
    .unwrap();
}

pub fn reason_codes(stdout: &str) -> Vec<String> {
    let report: Value = serde_json::from_str(stdout).unwrap_or(Value::Null);
    report["authorization"]["reasons"]
        .as_array()
        .map(|reasons| {
            reasons
                .iter()
                .filter_map(|r| r["code"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

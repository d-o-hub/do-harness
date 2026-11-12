//! Tests for server-derived merge rules.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::rules::*;

fn repo_all_methods() -> RawRepo {
    RawRepo {
        allow_merge_commit: Some(true),
        allow_squash_merge: Some(true),
        allow_rebase_merge: Some(true),
    }
}

fn payload(json: serde_json::Value) -> Vec<RawRule> {
    serde_json::from_value(json).unwrap()
}

fn required_rules() -> Vec<RawRule> {
    payload(serde_json::json!([
        {
            "type": "required_status_checks",
            "parameters": {
                "strict_required_status_checks_policy": true,
                "required_status_checks": [
                    {"context": "CI / Required", "integration_id": 15368},
                    {"context": "Codacy Static Code Analysis", "integration_id": 56611}
                ]
            }
        },
        {
            "type": "pull_request",
            "parameters": {
                "allowed_merge_methods": ["merge", "squash", "rebase"],
                "required_review_thread_resolution": true
            }
        },
        {"type": "required_linear_history"},
        {"type": "deletion"}
    ]))
}

#[test]
fn derives_required_tuples_and_flags() {
    let rules = MergeRules::from_payloads(&required_rules(), &repo_all_methods());
    assert_eq!(rules.required.len(), 2);
    assert_eq!(
        rules.required[0],
        RequiredContext {
            context: "CI / Required".to_owned(),
            integration_id: Some(15368),
        }
    );
    assert!(rules.strict_up_to_date);
    assert!(rules.linear_history);
    assert!(rules.thread_resolution);
    assert!(!rules.merge_queue);
    assert_eq!(
        rules.allowed_methods,
        vec![MergeMethod::Merge, MergeMethod::Squash, MergeMethod::Rebase]
    );
}

#[test]
fn required_checks_without_an_app_id_stay_distinguishable() {
    let rules = MergeRules::from_payloads(
        &payload(serde_json::json!([{
            "type": "required_status_checks",
            "parameters": {
                "required_status_checks": [{"context": "build"}]
            }
        }])),
        &repo_all_methods(),
    );
    assert_eq!(rules.required[0].integration_id, None);
}

#[test]
fn digest_is_stable_regardless_of_payload_order() {
    let forward = MergeRules::from_payloads(&required_rules(), &repo_all_methods());
    let mut reversed = required_rules();
    reversed.reverse();
    let backward = MergeRules::from_payloads(&reversed, &repo_all_methods());
    assert_eq!(forward.digest, backward.digest);
}

#[test]
fn digest_changes_when_the_app_identity_changes() {
    let base = MergeRules::from_payloads(&required_rules(), &repo_all_methods());
    let mut altered_rules = required_rules();
    altered_rules[0] = payload(serde_json::json!([{
        "type": "required_status_checks",
        "parameters": {
            "required_status_checks": [
                {"context": "CI / Required", "integration_id": 99999}
            ]
        }
    }]))
    .remove(0);
    let altered = MergeRules::from_payloads(&altered_rules, &repo_all_methods());
    assert_ne!(base.digest, altered.digest);
}

#[test]
fn a_ruleset_without_methods_inherits_the_repository_settings() {
    let rules = MergeRules::from_payloads(
        &payload(serde_json::json!([{"type": "required_linear_history"}])),
        &RawRepo {
            allow_merge_commit: Some(false),
            allow_squash_merge: Some(true),
            allow_rebase_merge: Some(false),
        },
    );
    assert_eq!(rules.allowed_methods, vec![MergeMethod::Squash]);
    assert!(!rules.merge_method_unavailable());
}

#[test]
fn a_ruleset_permitting_only_a_disabled_method_is_a_contradiction() {
    let rules = MergeRules::from_payloads(
        &payload(serde_json::json!([{
            "type": "pull_request",
            "parameters": {"allowed_merge_methods": ["rebase"]}
        }])),
        &RawRepo {
            allow_merge_commit: Some(true),
            allow_squash_merge: Some(true),
            allow_rebase_merge: Some(false),
        },
    );
    assert!(rules.merge_method_unavailable());
    assert_eq!(rules.usable_methods(), Vec::new());
}

#[test]
fn a_ruleset_with_no_required_checks_says_so() {
    let rules = MergeRules::from_payloads(
        &payload(serde_json::json!([{"type": "deletion"}])),
        &repo_all_methods(),
    );
    assert!(rules.has_no_required_checks());
}

fn entry(name: &str, app_id: Option<u64>) -> crate::pr::checks::CheckEntry {
    crate::pr::checks::CheckEntry {
        name: name.to_owned(),
        source: "github-actions".to_owned(),
        app_id,
        kind: "check".to_owned(),
        state: "completed".to_owned(),
        conclusion: Some("success".to_owned()),
        verdict: crate::pr::checks::Verdict::Pass,
        reason: None,
        rerun_command: None,
        url: None,
    }
}

#[test]
fn a_required_context_from_the_named_app_is_satisfied() {
    let rules = MergeRules::from_payloads(&required_rules(), &repo_all_methods());
    let entries = vec![
        entry("CI / Required", Some(15368)),
        entry("Codacy Static Code Analysis", Some(56611)),
    ];
    assert_eq!(rules.gate(&entries), Vec::new());
}

#[test]
fn a_same_named_check_from_another_app_is_a_mismatch() {
    let rules = MergeRules::from_payloads(&required_rules(), &repo_all_methods());
    let entries = vec![
        entry("CI / Required", Some(99999)),
        entry("Codacy Static Code Analysis", Some(56611)),
    ];
    let violations = rules.gate(&entries);
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].context, "CI / Required");
    assert_eq!(violations[0].code, codes::REQUIRED_CHECK_APP_MISMATCH);
    assert!(violations[0].detail.contains("99999"));
    assert!(violations[0].detail.contains("15368"));
}

#[test]
fn a_passing_check_alongside_an_impersonated_one_satisfies_the_context() {
    let rules = MergeRules::from_payloads(&required_rules(), &repo_all_methods());
    let entries = vec![
        entry("CI / Required", Some(99999)),
        entry("CI / Required", Some(15368)),
        entry("Codacy Static Code Analysis", Some(56611)),
    ];
    // One passing run from the right app is enough; the gate is not a
    // "no other app may publish this name" rule.
    assert_eq!(rules.gate(&entries), Vec::new());
}

#[test]
fn a_run_without_an_app_id_fails_closed() {
    let rules = MergeRules::from_payloads(&required_rules(), &repo_all_methods());
    let entries = vec![
        entry("CI / Required", None),
        entry("Codacy Static Code Analysis", Some(56611)),
    ];
    let violations = rules.gate(&entries);
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].code, codes::REQUIRED_CHECK_APP_MISMATCH);
    assert!(violations[0].detail.contains("no app id"));
}

#[test]
fn an_absent_required_context_is_a_failure_not_a_pass() {
    let rules = MergeRules::from_payloads(&required_rules(), &repo_all_methods());
    let entries = vec![entry("Codacy Static Code Analysis", Some(56611))];
    let violations = rules.gate(&entries);
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].context, "CI / Required");
    assert_eq!(violations[0].code, codes::REQUIRED_CHECK_MISSING);
}

#[test]
fn a_required_context_that_is_present_but_failing_is_reported() {
    let rules = MergeRules::from_payloads(&required_rules(), &repo_all_methods());
    let mut failing = entry("CI / Required", Some(15368));
    failing.verdict = crate::pr::checks::Verdict::Fail;
    let entries = vec![failing, entry("Codacy Static Code Analysis", Some(56611))];
    let violations = rules.gate(&entries);
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].code, codes::REQUIRED_CHECK_FAILING);
}

#[test]
fn a_ruleset_naming_no_app_matches_on_the_context_alone() {
    let rules = MergeRules::from_payloads(
        &payload(serde_json::json!([{
            "type": "required_status_checks",
            "parameters": {
                "required_status_checks": [{"context": "build"}]
            }
        }])),
        &repo_all_methods(),
    );
    let entries = vec![entry("build", Some(99999))];
    assert_eq!(rules.gate(&entries), Vec::new());
}

#[test]
fn an_unreported_repository_set_is_unknown_not_a_contradiction() {
    // GitHub only reports the merge settings to a caller with admin permission,
    // so an adopter's response can omit all three. Absence must not read as
    // "every method disabled": that turns an unreadable setting into a
    // confident `merge_method_unavailable` blocker.
    let rules = MergeRules::from_payloads(
        &payload(serde_json::json!([{
            "type": "pull_request",
            "parameters": {"allowed_merge_methods": ["merge", "squash", "rebase"]}
        }])),
        &RawRepo {
            allow_merge_commit: None,
            allow_squash_merge: None,
            allow_rebase_merge: None,
        },
    );
    assert_eq!(rules.repository_methods, None);
    assert!(!rules.merge_method_unavailable());
    assert_eq!(
        rules.usable_methods(),
        vec![MergeMethod::Merge, MergeMethod::Squash, MergeMethod::Rebase]
    );
}

#[test]
fn a_reported_but_empty_repository_set_is_still_a_contradiction() {
    // Here the server did answer, and the answer was "no method enabled".
    let rules = MergeRules::from_payloads(
        &payload(serde_json::json!([{
            "type": "pull_request",
            "parameters": {"allowed_merge_methods": ["rebase"]}
        }])),
        &RawRepo {
            allow_merge_commit: Some(false),
            allow_squash_merge: Some(false),
            allow_rebase_merge: Some(false),
        },
    );
    assert_eq!(rules.repository_methods, Some(Vec::new()));
    assert!(rules.merge_method_unavailable());
}

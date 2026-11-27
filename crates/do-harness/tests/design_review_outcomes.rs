//! Hermetic integration tests for the `design-review` outcome taxonomy:
//! every reviewer behavior — success with findings, zero findings,
//! failure (nonzero exit, invalid JSON, missing fields, oversized
//! output), timeout, `not_run`, marker leak, and `unverified` — is a
//! distinct recorded outcome, exercised through the real CLI binary
//! running its own bridge action with pre-placed findings files. No model
//! is ever launched.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "support/design_review.rs"]
#[allow(dead_code)]
mod support;

use support::*;

use serde_json::json;

#[test]
fn design_review_flawed_design_yields_findings_with_clean_marker_probe() {
    let handoff = temp_dir("flawed-handoff");
    // The reviewer response is pre-placed from the fixture expectations.
    place_fixture_findings(&handoff, "flawed-expected.json");
    let record = run_flawed_review(
        "flawed",
        &[
            "--reviewer-command-json".to_owned(),
            bridge_reviewer(&handoff, "30"),
            "--parent-marker".to_owned(),
            "DESIGN-REVIEW-UT-MARKER-1".to_owned(),
        ],
    );
    assert_eq!(record["outcome"], "success_with_findings");
    assert_eq!(record["findings_count"], 2);
    assert_eq!(record["findings_truncated"], false);
    assert_eq!(record["marker_leaked"], false);
    assert_eq!(record["reviewer_mode"], "external");
    assert_eq!(record["launch_evidence"]["environment_scrubbed"], true);
    assert_eq!(record["launch_evidence"]["fresh_process"], true);
    let inherited = record["launch_evidence"]["environment_keys_inherited"]
        .as_array()
        .expect("inherited keys");
    for key in inherited {
        let name = key.as_str().expect("key");
        assert!(
            [
                "PATH",
                "HOME",
                "LANG",
                "LC_ALL",
                "TMPDIR",
                "TEMP",
                "TMP",
                "SYSTEMROOT",
                "SystemRoot"
            ]
            .contains(&name),
            "unexpected inherited key {name}"
        );
    }
}

#[test]
fn design_review_sound_design_yields_zero_findings() {
    let handoff = temp_dir("sound-handoff");
    // The sound fixture's expectation is zero findings: the sound reviewer
    // response is an empty findings array (the committed fixture records the
    // expectation, not a response body).
    place_findings(&handoff, "{\"findings\":[]}");
    let record = run_flawed_review(
        "sound",
        &[
            "--reviewer-command-json".to_owned(),
            bridge_reviewer(&handoff, "30"),
            "--parent-marker".to_owned(),
            "DESIGN-REVIEW-UT-MARKER-2".to_owned(),
        ],
    );
    assert_eq!(record["outcome"], "success_zero_findings");
    assert_eq!(record["findings_count"], 0);
}

#[test]
fn design_review_success_without_marker_is_unverified() {
    let handoff = temp_dir("unverified-handoff");
    place_findings(&handoff, "{\"findings\":[]}");
    let record = run_flawed_review(
        "unverified",
        &[
            "--reviewer-command-json".to_owned(),
            bridge_reviewer(&handoff, "30"),
        ],
    );
    assert_eq!(record["outcome"], "unverified");
    assert_eq!(record["isolation"]["marker_probe_configured"], false);
}

#[test]
fn design_review_unavailable_reviewer_is_not_run() {
    let handoff = temp_dir("unavailable");
    let record = run_flawed_review(
        "unavailable",
        &[
            "--reviewer-command-json".to_owned(),
            bridge_reviewer(&handoff, "0.5"),
        ],
    );
    assert_eq!(record["outcome"], "not_run");
    assert!(
        record["reason"]
            .as_str()
            .expect("reason")
            .contains("capability unavailable")
    );
}

#[test]
fn design_review_timeout_is_a_distinct_outcome() {
    let handoff = temp_dir("timeout");
    let record = run_flawed_review(
        "timeout",
        &[
            "--reviewer-command-json".to_owned(),
            bridge_reviewer(&handoff, "30"),
            "--limit-timeout".to_owned(),
            "2".to_owned(),
        ],
    );
    assert_eq!(record["outcome"], "timeout");
    assert_eq!(
        record["reason"],
        "reviewer exceeded the predeclared timeout"
    );
    assert!(record["elapsed_seconds"].as_f64().expect("elapsed") >= 2.0);
}

#[test]
fn design_review_marker_leak_fails_isolation() {
    let handoff = temp_dir("leak-handoff");
    let marker = fixture("leak-marker.txt");
    let marker = marker.trim();
    place_findings(
        &handoff,
        &format!("{{\"findings\":[],\"echo\":\"{marker}\"}}"),
    );
    let record = run_flawed_review(
        "leak",
        &[
            "--reviewer-command-json".to_owned(),
            bridge_reviewer(&handoff, "30"),
            "--parent-marker".to_owned(),
            marker.to_owned(),
        ],
    );
    assert_eq!(record["outcome"], "failure");
    assert_eq!(record["error_kind"], "isolation_marker_leaked");
    assert_eq!(record["marker_leaked"], true);
}

#[test]
fn design_review_oversized_output_is_failure() {
    let handoff = temp_dir("huge-handoff");
    let pad = "x".repeat(512);
    place_findings(&handoff, &format!("{{\"findings\":[],\"pad\":\"{pad}\"}}"));
    let record = run_flawed_review(
        "huge",
        &[
            "--reviewer-command-json".to_owned(),
            bridge_reviewer(&handoff, "30"),
            "--limit-output".to_owned(),
            "128".to_owned(),
        ],
    );
    assert_eq!(record["outcome"], "failure");
    assert_eq!(
        record["reason"],
        "reviewer response exceeds the output limit"
    );
}

#[test]
fn design_review_invalid_reviewer_json_is_failure() {
    let handoff = temp_dir("invalid-json-handoff");
    place_findings(&handoff, "{\"findings\": [");
    let record = run_flawed_review(
        "invalid-json",
        &[
            "--reviewer-command-json".to_owned(),
            bridge_reviewer(&handoff, "0.6"),
        ],
    );
    assert_eq!(record["outcome"], "failure");
    assert_eq!(record["reason"], "reviewer stdout is not valid JSON");
}

#[test]
fn design_review_missing_findings_array_is_failure() {
    let handoff = temp_dir("no-findings-handoff");
    place_findings(&handoff, "{\"other\":1}");
    let record = run_flawed_review(
        "no-findings",
        &[
            "--reviewer-command-json".to_owned(),
            bridge_reviewer(&handoff, "30"),
        ],
    );
    assert_eq!(record["outcome"], "failure");
    assert_eq!(
        record["reason"],
        "reviewer response must be an object with a findings array"
    );
}

#[test]
fn design_review_invalid_finding_fields_are_failure() {
    let handoff = temp_dir("bad-finding-handoff");
    place_findings(&handoff, "{\"findings\":[{\"id\":\"F1\"}]}");
    let record = run_flawed_review(
        "bad-finding",
        &[
            "--reviewer-command-json".to_owned(),
            bridge_reviewer(&handoff, "30"),
        ],
    );
    assert_eq!(record["outcome"], "failure");
    assert!(
        record["reason"]
            .as_str()
            .expect("reason")
            .contains("finding 0 missing fields")
    );
}

#[test]
fn design_review_truncates_over_limit_findings() {
    let handoff = temp_dir("truncated-handoff");
    let finding = json!({
        "contract_or_path": "p", "trigger": "t", "consequence": "c",
        "evidence": "e", "smallest_correction": "s",
    });
    let findings: Vec<serde_json::Value> = (0..7)
        .map(|i| {
            let mut item = finding.clone();
            item["contract_or_path"] = json!(format!("p{i}"));
            item
        })
        .collect();
    let body = serde_json::to_string(&json!({"findings": findings})).expect("response");
    place_findings(&handoff, &body);
    let record = run_flawed_review(
        "truncated",
        &[
            "--reviewer-command-json".to_owned(),
            bridge_reviewer(&handoff, "30"),
            "--parent-marker".to_owned(),
            "DESIGN-REVIEW-UT-MARKER-3".to_owned(),
        ],
    );
    assert_eq!(record["outcome"], "success_with_findings");
    assert_eq!(record["findings_count"], 5);
    assert_eq!(record["findings_truncated"], true);
}

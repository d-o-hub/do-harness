//! `app-report` classification acceptance fixtures (#517).
//!
//! These drive the real binary: a pull request whose only conversations are
//! vendor status comments must authorize, and the same fixture with a failure in
//! the report must not. The command has to stay read-only throughout.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[cfg(unix)]
use serde_json::{Value, json};

mod support;

#[cfg(unix)]
use support::pr_readiness::{fake_gh_router, run_pr};
#[cfg(unix)]
use support::pr_readiness_fixtures::write_view;

/// The real Sonar body from `d-o-hub/rust-self-learning-memory` PR 1263.
#[cfg(unix)]
const SONAR_PASS: &str = "## [![Quality Gate Passed](https://example.invalid/qg.png 'Quality Gate Passed')](https://example.invalid) **Quality Gate passed**  \nIssues  \n[0 New issues](https://example.invalid)  \n[0 Accepted issues](https://example.invalid)  \nMeasures  \n[0 Security Hotspots](https://example.invalid)";

/// The real benchmark body from the same pull request.
#[cfg(unix)]
const BENCHMARK: &str = "## Performance Benchmark Results\n\n<details><summary>Click to expand</summary>\n\n```\ntest cache_basic_cache_hit ... bench: 130.79 ns/iter (+/- 6.17)\ntest cache_eviction ... bench: 339.77 ns/iter (+/- 25.67)\n```\n\n</details>";

/// Writes the issue-comment fixture with the given `(id, login, body)` rows.
#[cfg(unix)]
fn write_comments(root: &std::path::Path, rows: &[(u64, &str, &str)]) {
    let comments: Vec<Value> = rows
        .iter()
        .map(|(id, login, body)| {
            json!({
                "id": id,
                "user": { "login": login },
                "body": body,
                "created_at": "2026-11-09T10:00:00Z",
                "html_url": format!("https://example.invalid/comment/{id}")
            })
        })
        .collect();
    std::fs::write(root.join("comments.json"), json!(comments).to_string()).unwrap();
}

/// The head's check runs, naming the apps that may attribute a comment.
#[cfg(unix)]
fn write_checks(root: &std::path::Path) {
    std::fs::write(
        root.join("check_runs.json"),
        json!({
            "total_count": 4,
            "check_runs": [
                {"id": 1, "name": "pr-required", "status": "completed", "conclusion": "success", "app": {"id": 15368, "slug": "github-actions"}},
                {"id": 2, "name": "semver", "status": "completed", "conclusion": "success", "app": {"id": 15368, "slug": "github-actions"}},
                {"id": 3, "name": "Codacy Static Code Analysis", "status": "completed", "conclusion": "success", "app": {"id": 56611, "slug": "codacy-production"}},
                {"id": 4, "name": "SonarCloud", "status": "completed", "conclusion": "success", "app": {"id": 7078, "slug": "sonarqubecloud"}}
            ]
        })
        .to_string(),
    )
    .unwrap();
}

/// The issue's originating shape: both vendor reports are attributable and
/// carry no failure vocabulary, so neither blocks.
#[cfg(unix)]
#[test]
fn attributable_pass_reports_do_not_block_and_record_their_slug() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    // Any mutating `gh` call fails the fixture loudly instead of being recorded.
    std::fs::write(root.join("assert_get_only"), "").unwrap();
    write_view(root, 5001, &json!({}));
    write_checks(root);
    write_comments(
        root,
        &[
            (6_736_397_836, "sonarqubecloud[bot]", SONAR_PASS),
            (6_737_402_364, "github-actions[bot]", BENCHMARK),
        ],
    );

    let (code, stdout, stderr) = run_pr(root, &bin, &["pr", "ready", "5001", "--format", "json"]);
    assert_eq!(
        code,
        Some(0),
        "two vendor reports must not block a green pull request: {stderr}\n{stdout}"
    );
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    let triage = report["conversations"]["needs_triage"]
        .as_array()
        .expect("needs_triage is an array");
    assert!(
        triage.is_empty(),
        "an attributable pass report needs no triage: {triage:?}"
    );

    // The decision is auditable: each report carries its slug and evidence.
    let reports: Vec<&Value> = report["conversations"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["classification"] == "app-report")
        .collect();
    assert_eq!(reports.len(), 2, "both reports classify as app-report");
    let authors: Vec<&str> = reports
        .iter()
        .map(|item| item["author"].as_str().unwrap())
        .collect();
    assert!(authors.contains(&"sonarqubecloud[bot]"), "{authors:?}");
    assert!(authors.contains(&"github-actions[bot]"), "{authors:?}");
    for item in &reports {
        assert_eq!(item["actionable"], false);
        let evidence = item["evidence"].as_str().expect("evidence is recorded");
        assert!(
            evidence.contains("check-run author"),
            "evidence names the attribution: {evidence}"
        );
        assert!(
            evidence.contains("no failure vocabulary"),
            "evidence names the veto that passed: {evidence}"
        );
    }
    // The Sonar report carries its recognised pass phrase as evidence.
    let sonar = reports
        .iter()
        .find(|item| item["author"] == "sonarqubecloud[bot]")
        .unwrap();
    assert!(
        sonar["evidence"]
            .as_str()
            .unwrap()
            .contains("quality gate passed"),
        "{}",
        sonar["evidence"]
    );

    // Read-only: the fake `gh` refuses any mutating call outright, so reaching
    // this assertion proves no comment, reaction, or merge was issued.
    assert!(
        !root.join("mutations.txt").exists(),
        "classification must never mutate: {:?}",
        std::fs::read_to_string(root.join("mutations.txt")).ok()
    );
}

/// The same app reporting a failure is still a blocker.
#[cfg(unix)]
#[test]
fn a_failing_gate_from_the_same_app_still_blocks() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    write_view(root, 5002, &json!({}));
    write_checks(root);
    write_comments(
        root,
        &[(
            6001,
            "sonarqubecloud[bot]",
            "## ![Quality Gate Failed] **Quality Gate failed**\n\n1 New issue, 1 vulnerability",
        )],
    );

    let (code, stdout, _stderr) = run_pr(root, &bin, &["pr", "ready", "5002", "--format", "json"]);
    assert_eq!(code, Some(1), "a failing gate must block: {stdout}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["ready"], false);
    let triage = report["conversations"]["needs_triage"].as_array().unwrap();
    assert_eq!(triage.len(), 1, "{triage:?}");
    assert_eq!(triage[0]["classification"], "actionable");
    assert!(
        triage[0]["evidence"].as_str().unwrap().contains("failure"),
        "the veto names the failure: {}",
        triage[0]["evidence"]
    );
}

/// An unknown bot asking a question is not an app report.
#[cfg(unix)]
#[test]
fn an_unattributable_bot_question_still_blocks() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    write_view(root, 5003, &json!({}));
    write_checks(root);
    write_comments(
        root,
        &[(
            6002,
            "mystery-bot[bot]",
            "Could you explain why this dependency was added?",
        )],
    );

    let (code, stdout, _stderr) = run_pr(root, &bin, &["pr", "ready", "5003", "--format", "json"]);
    assert_eq!(code, Some(1), "an unattributable bot must block: {stdout}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    let triage = report["conversations"]["needs_triage"].as_array().unwrap();
    assert_eq!(triage.len(), 1);
    assert_eq!(triage[0]["classification"], "actionable");
    assert!(
        triage[0]["evidence"]
            .as_str()
            .unwrap()
            .contains("no check run"),
        "{}",
        triage[0]["evidence"]
    );
}

/// A human review question is unchanged, and so is an unresolved thread.
#[cfg(unix)]
#[test]
fn human_questions_and_unresolved_threads_still_block() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    write_view(root, 5004, &json!({}));
    write_checks(root);
    write_comments(
        root,
        &[(6003, "octocat", "Why is this behind a feature flag?")],
    );
    let graphql = json!({
        "data": { "repository": { "pullRequest": { "reviewThreads": { "nodes": [
            {
                "id": "PRRT_app_report_1",
                "isResolved": false,
                "isOutdated": false,
                "path": "src/lib.rs",
                "line": 12,
                "comments": { "nodes": [ { "author": { "login": "reviewer" }, "body": "Please rename this." } ] }
            }
        ] } } } }
    });
    std::fs::write(root.join("graphql.json"), graphql.to_string()).unwrap();

    let (code, stdout, _stderr) = run_pr(root, &bin, &["pr", "ready", "5004", "--format", "json"]);
    assert_eq!(code, Some(1), "human input must still block: {stdout}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    let triage = report["conversations"]["needs_triage"].as_array().unwrap();
    assert_eq!(triage.len(), 1, "{triage:?}");
    assert_eq!(triage[0]["author"], "octocat");
    assert_eq!(triage[0]["classification"], "actionable");
    assert_eq!(
        report["conversations"]["unresolved_threads"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

/// An app that publishes no check on this head but is allowlisted qualifies.
#[cfg(unix)]
#[test]
fn the_policy_allowlist_attributes_an_app_with_no_check_run_here() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    write_view(root, 5005, &json!({}));
    write_checks(root);
    let policy = root.join(".git/pr-triage");
    std::fs::create_dir_all(&policy).unwrap();
    std::fs::write(policy.join("app-report-allow.txt"), "# vendor\nbenchbot\n").unwrap();
    write_comments(
        root,
        &[(
            6004,
            "benchbot[bot]",
            "## Benchmark summary\nall within budget",
        )],
    );

    let (code, stdout, stderr) = run_pr(root, &bin, &["pr", "ready", "5005", "--format", "json"]);
    assert_eq!(
        code,
        Some(0),
        "an allowlisted app report must not block: {stderr}\n{stdout}"
    );
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    let item = report["conversations"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["author"] == "benchbot[bot]")
        .expect("the comment is in the ledger");
    assert_eq!(item["classification"], "app-report");
    assert!(
        item["evidence"]
            .as_str()
            .unwrap()
            .contains("allowlisted app"),
        "{}",
        item["evidence"]
    );
}

/// Codacy keeps its existing classification: the legacy phrasing path and the
/// new structural path must not disagree.
#[cfg(unix)]
#[test]
fn the_codacy_report_keeps_its_classification() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    write_view(root, 5006, &json!({}));
    write_checks(root);
    write_comments(
        root,
        &[(
            6005,
            "codacy-production[bot]",
            "## Up to standards :white_check_mark:\n<details><summary><strong>Issues</strong> <code>0 issues</code></summary></details>",
        )],
    );

    let (code, stdout, stderr) = run_pr(root, &bin, &["pr", "ready", "5006", "--format", "json"]);
    assert_eq!(code, Some(0), "the Codacy pass must not block: {stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    let item = report["conversations"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["author"] == "codacy-production[bot]")
        .expect("the comment is in the ledger");
    assert_eq!(item["classification"], "app-report");
}

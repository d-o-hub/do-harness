//! Conversation fixtures for `pr ready`: unresolved threads and Codecov gaps.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::Value;

mod support;

use support::pr_readiness::{fake_gh_router, run_pr};

#[cfg(unix)]
#[test]
fn unanswered_codecov_comment_is_actionable_blocker() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    std::fs::write(
        root.join("view.json"),
        r#"{
            "number": 1042,
            "baseRefName": "main",
            "headRefOid": "abcd1234abcd1234abcd1234abcd1234abcd1234",
            "mergeStateStatus": "CLEAN",
            "mergeable": "MERGEABLE",
            "autoMergeRequest": null
        }"#,
    )
    .unwrap();

    std::fs::write(
        root.join("comments.json"),
        r###"[
            {
                "id": 101,
                "user": { "login": "codecov[bot]" },
                "body": "## [Codecov](https://app.codecov.io) Report\nAttention: Patch coverage is 0.00% with 2 lines in your changes missing coverage.",
                "created_at": "2026-09-25T10:00:00Z",
                "html_url": "https://github.com/d-o-hub/repo/pull/1042#issuecomment-1"
            }
        ]"###,
    )
    .unwrap();

    // With no subsequent waiver comment, Codecov gap is actionable and blocks merge:
    let (code, stdout, stderr) = run_pr(root, &bin, &["pr", "1042", "--format", "json"]);
    assert_eq!(code, Some(1));
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(report["ready"], serde_json::json!(false));
    assert_eq!(report["codecov"]["is_actionable"], serde_json::json!(true));
    assert_eq!(report["codecov"]["status"], serde_json::json!("actionable"));
    assert_eq!(report["codecov"]["patch_coverage"], serde_json::json!(0.0));
    assert_eq!(report["codecov"]["missing_lines"], serde_json::json!(2));
    assert_eq!(
        report["conversations"]["actionable_comments"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(stderr.contains("not ready to merge"));

    // Now developer replies with an explicit head-scoped waiver comment:
    std::fs::write(
        root.join("comments.json"),
        r###"[
            {
                "id": 101,
                "user": { "login": "codecov[bot]" },
                "body": "## [Codecov](https://app.codecov.io) Report\nAttention: Patch coverage is 0.00% with 2 lines in your changes missing coverage.",
                "created_at": "2026-09-25T10:00:00Z",
                "html_url": "https://github.com/d-o-hub/repo/pull/1042#issuecomment-1"
            },
            {
                "id": 102,
                "user": { "login": "developer" },
                "body": "Waiver for abcd1234abcd1234abcd1234abcd1234abcd1234 (codecov): CLI entry point not covered by unit suite.",
                "created_at": "2026-09-25T10:05:00Z",
                "html_url": "https://github.com/d-o-hub/repo/pull/1042#issuecomment-2"
            }
        ]"###,
    )
    .unwrap();

    let (code, stdout, _) = run_pr(root, &bin, &["pr", "1042", "--format", "json"]);
    assert_eq!(
        code,
        Some(0),
        "explicitly waived Codecov comment must not block merge:\n{stdout}"
    );
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(report["ready"], serde_json::json!(true));
    assert_eq!(report["codecov"]["status"], serde_json::json!("waived"));
    assert_eq!(report["codecov"]["waived"], serde_json::json!(true));
    assert_eq!(report["codecov"]["is_actionable"], serde_json::json!(false));
    assert_eq!(
        report["codecov"]["waiver_author"],
        serde_json::json!("developer")
    );
    assert_eq!(
        report["codecov"]["waiver_head_sha"],
        serde_json::json!("abcd1234abcd1234abcd1234abcd1234abcd1234")
    );
    assert_eq!(
        report["codecov"]["waiver_finding_ref"],
        serde_json::json!("codecov")
    );
    assert_eq!(
        report["codecov"]["waiver_reason"],
        serde_json::json!("CLI entry point not covered by unit suite.")
    );
    assert_eq!(
        report["conversations"]["actionable_comments"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(report["blockers"].as_array().unwrap().len(), 0);
}

#[cfg(unix)]
#[test]
fn unrelated_human_comment_leaves_concern_actionable() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    std::fs::write(
        root.join("view.json"),
        r#"{
            "number": 1042,
            "baseRefName": "main",
            "headRefOid": "abcd1234abcd1234abcd1234abcd1234abcd1234",
            "mergeStateStatus": "CLEAN",
            "mergeable": "MERGEABLE",
            "autoMergeRequest": null
        }"#,
    )
    .unwrap();

    std::fs::write(
        root.join("comments.json"),
        r###"[
            {
                "id": 101,
                "user": { "login": "codecov[bot]" },
                "body": "## [Codecov](https://app.codecov.io) Report\nAttention: Patch coverage is 0.00% with 2 lines in your changes missing coverage.",
                "created_at": "2026-09-25T10:00:00Z",
                "html_url": "https://github.com/d-o-hub/repo/pull/1042#issuecomment-1"
            },
            {
                "id": 102,
                "user": { "login": "developer" },
                "body": "updated README",
                "created_at": "2026-09-25T10:05:00Z",
                "html_url": "https://github.com/d-o-hub/repo/pull/1042#issuecomment-2"
            }
        ]"###,
    )
    .unwrap();

    let (code, stdout, _) = run_pr(root, &bin, &["pr", "1042", "--format", "json"]);
    assert_eq!(code, Some(1), "unrelated human comment must leave concern actionable");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(report["ready"], serde_json::json!(false));
    assert_eq!(report["codecov"]["is_actionable"], serde_json::json!(true));
    assert_eq!(report["codecov"]["status"], serde_json::json!("actionable"));
}

#[cfg(unix)]
#[test]
fn missing_author_or_unauthorized_waiver_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    std::fs::write(
        root.join("view.json"),
        r#"{
            "number": 1042,
            "baseRefName": "main",
            "headRefOid": "abcd1234abcd1234abcd1234abcd1234abcd1234",
            "mergeStateStatus": "CLEAN",
            "mergeable": "MERGEABLE",
            "autoMergeRequest": null
        }"#,
    )
    .unwrap();

    // Read-only commenter permission
    std::fs::write(
        root.join("permission.json"),
        r#"{"permission": "read", "role_name": "read"}"#,
    )
    .unwrap();

    std::fs::write(
        root.join("comments.json"),
        r###"[
            {
                "id": 101,
                "user": { "login": "codecov[bot]" },
                "body": "## [Codecov](https://app.codecov.io) Report\nAttention: Patch coverage is 0.00% with 2 lines in your changes missing coverage.",
                "created_at": "2026-09-25T10:00:00Z",
                "html_url": "https://github.com/d-o-hub/repo/pull/1042#issuecomment-1"
            },
            {
                "id": 102,
                "user": null,
                "body": "Waiver for abcd1234abcd1234abcd1234abcd1234abcd1234 (codecov): missing author.",
                "created_at": "2026-09-25T10:02:00Z",
                "html_url": "https://github.com/d-o-hub/repo/pull/1042#issuecomment-2"
            },
            {
                "id": 103,
                "user": { "login": "external-reader" },
                "body": "Waiver for abcd1234abcd1234abcd1234abcd1234abcd1234 (codecov): read-only user.",
                "created_at": "2026-09-25T10:05:00Z",
                "html_url": "https://github.com/d-o-hub/repo/pull/1042#issuecomment-3"
            }
        ]"###,
    )
    .unwrap();

    let (code, stdout, _) = run_pr(root, &bin, &["pr", "1042", "--format", "json"]);
    assert_eq!(code, Some(1), "unauthorized commenter cannot waive policy");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(report["ready"], serde_json::json!(false));
    assert_eq!(report["codecov"]["is_actionable"], serde_json::json!(true));
    let audit = &report["codecov"]["audit_waivers"];
    assert_eq!(audit[0]["authorized"], serde_json::json!(false));
}

#[cfg(unix)]
#[test]
fn stale_waiver_for_different_head_or_finding_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    std::fs::write(
        root.join("view.json"),
        r#"{
            "number": 1042,
            "baseRefName": "main",
            "headRefOid": "abcd1234abcd1234abcd1234abcd1234abcd1234",
            "mergeStateStatus": "CLEAN",
            "mergeable": "MERGEABLE",
            "autoMergeRequest": null
        }"#,
    )
    .unwrap();

    std::fs::write(
        root.join("comments.json"),
        r###"[
            {
                "id": 101,
                "user": { "login": "codecov[bot]" },
                "body": "## [Codecov](https://app.codecov.io) Report\nAttention: Patch coverage is 0.00% with 2 lines in your changes missing coverage.",
                "created_at": "2026-09-25T10:00:00Z",
                "html_url": "https://github.com/d-o-hub/repo/pull/1042#issuecomment-1"
            },
            {
                "id": 102,
                "user": { "login": "developer" },
                "body": "Waiver for 1111222233334444555566667777888899990000 (codecov): waiver for old head.",
                "created_at": "2026-09-25T10:05:00Z",
                "html_url": "https://github.com/d-o-hub/repo/pull/1042#issuecomment-2"
            }
        ]"###,
    )
    .unwrap();

    let (code, stdout, _) = run_pr(root, &bin, &["pr", "1042", "--format", "json"]);
    assert_eq!(code, Some(1), "stale waiver for previous head must be rejected");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(report["ready"], serde_json::json!(false));
    let audit = &report["codecov"]["audit_waivers"];
    assert_eq!(audit[0]["is_stale"], serde_json::json!(true));
}

#[cfg(unix)]
#[test]
fn permission_lookup_failure_retains_blocker() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    std::fs::write(
        root.join("view.json"),
        r#"{
            "number": 1042,
            "baseRefName": "main",
            "headRefOid": "abcd1234abcd1234abcd1234abcd1234abcd1234",
            "mergeStateStatus": "CLEAN",
            "mergeable": "MERGEABLE",
            "autoMergeRequest": null
        }"#,
    )
    .unwrap();

    std::fs::write(root.join("permission_fail"), "1").unwrap();

    std::fs::write(
        root.join("comments.json"),
        r###"[
            {
                "id": 101,
                "user": { "login": "codecov[bot]" },
                "body": "## [Codecov](https://app.codecov.io) Report\nAttention: Patch coverage is 0.00% with 2 lines in your changes missing coverage.",
                "created_at": "2026-09-25T10:00:00Z",
                "html_url": "https://github.com/d-o-hub/repo/pull/1042#issuecomment-1"
            },
            {
                "id": 102,
                "user": { "login": "developer" },
                "body": "Waiver for abcd1234abcd1234abcd1234abcd1234abcd1234 (codecov): CLI entry point not covered.",
                "created_at": "2026-09-25T10:05:00Z",
                "html_url": "https://github.com/d-o-hub/repo/pull/1042#issuecomment-2"
            }
        ]"###,
    )
    .unwrap();

    let (code, stdout, _) = run_pr(root, &bin, &["pr", "1042", "--format", "json"]);
    assert_eq!(code, Some(1), "permission lookup failure fails closed");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(report["ready"], serde_json::json!(false));
    assert_eq!(report["codecov"]["is_actionable"], serde_json::json!(true));
}

#[cfg(unix)]
#[test]
fn newer_clean_codecov_report_supersedes_older_gap() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    std::fs::write(
        root.join("view.json"),
        r#"{
            "number": 1042,
            "baseRefName": "main",
            "headRefOid": "abcd1234abcd1234abcd1234abcd1234abcd1234",
            "mergeStateStatus": "CLEAN",
            "mergeable": "MERGEABLE",
            "autoMergeRequest": null
        }"#,
    )
    .unwrap();

    std::fs::write(
        root.join("comments.json"),
        r###"[
            {
                "id": 101,
                "user": { "login": "codecov[bot]" },
                "body": "## [Codecov](https://app.codecov.io) Report\nAttention: Patch coverage is 0.00% with 2 lines in your changes missing coverage.",
                "created_at": "2026-09-25T10:00:00Z",
                "html_url": "https://github.com/d-o-hub/repo/pull/1042#issuecomment-1"
            },
            {
                "id": 102,
                "user": { "login": "codecov[bot]" },
                "body": "## [Codecov](https://app.codecov.io) Report\nAll changed lines are covered. Patch coverage is 100.00%.",
                "created_at": "2026-09-25T10:10:00Z",
                "html_url": "https://github.com/d-o-hub/repo/pull/1042#issuecomment-2"
            }
        ]"###,
    )
    .unwrap();

    let (code, stdout, _) = run_pr(root, &bin, &["pr", "1042", "--format", "json"]);
    assert_eq!(code, Some(0), "newer clean report supersedes older gap");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(report["ready"], serde_json::json!(true));
    assert_eq!(report["codecov"]["status"], serde_json::json!("no_concern"));
    assert_eq!(report["codecov"]["is_actionable"], serde_json::json!(false));
}

#[cfg(unix)]
#[test]
fn required_check_failures_remain_blockers_independently() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    std::fs::write(
        root.join("view.json"),
        r#"{
            "number": 1042,
            "baseRefName": "main",
            "headRefOid": "abcd1234abcd1234abcd1234abcd1234abcd1234",
            "mergeStateStatus": "CLEAN",
            "mergeable": "MERGEABLE",
            "autoMergeRequest": null
        }"#,
    )
    .unwrap();

    std::fs::write(
        root.join("check_runs.json"),
        r#"{"id": 1, "name": "build", "status": "completed", "conclusion": "failure", "html_url": "https://github.com/d-o-hub/repo/actions/runs/1"}"#,
    )
    .unwrap();

    std::fs::write(
        root.join("comments.json"),
        r###"[
            {
                "id": 101,
                "user": { "login": "codecov[bot]" },
                "body": "## [Codecov](https://app.codecov.io) Report\nAttention: Patch coverage is 0.00% with 2 lines in your changes missing coverage.",
                "created_at": "2026-09-25T10:00:00Z",
                "html_url": "https://github.com/d-o-hub/repo/pull/1042#issuecomment-1"
            },
            {
                "id": 102,
                "user": { "login": "developer" },
                "body": "Waiver for abcd1234abcd1234abcd1234abcd1234abcd1234 (codecov): CLI entry point not covered.",
                "created_at": "2026-09-25T10:05:00Z",
                "html_url": "https://github.com/d-o-hub/repo/pull/1042#issuecomment-2"
            }
        ]"###,
    )
    .unwrap();

    let (code, stdout, _) = run_pr(root, &bin, &["pr", "1042", "--format", "json"]);
    assert_eq!(code, Some(1), "check failure blocks merge independently of waiver");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(report["ready"], serde_json::json!(false));
    assert_eq!(report["checks"]["failed"].as_array().unwrap().len(), 1);
    assert_eq!(report["codecov"]["status"], serde_json::json!("waived"));
}

#[cfg(unix)]
#[test]
fn unresolved_thread_blocks_readiness() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    std::fs::write(
        root.join("view.json"),
        r#"{
            "number": 1043,
            "baseRefName": "main",
            "headRefOid": "abcd1234abcd1234abcd1234abcd1234abcd1234",
            "mergeStateStatus": "CLEAN",
            "mergeable": "MERGEABLE",
            "autoMergeRequest": null
        }"#,
    )
    .unwrap();

    let long_body = format!("{}€ tail of the comment body", "a".repeat(79));
    let graphql = serde_json::json!({
        "data": { "repository": { "pullRequest": { "reviewThreads": { "nodes": [
            {
                "id": "PRRT_thread_1",
                "isResolved": false,
                "isOutdated": false,
                "path": "src/lib.rs",
                "line": 42,
                "comments": { "nodes": [ { "author": { "login": "reviewer" }, "body": long_body } ] }
            },
            {
                "id": "PRRT_thread_2",
                "isResolved": true,
                "isOutdated": false,
                "path": "src/other.rs",
                "line": 7,
                "comments": { "nodes": [ { "author": { "login": "reviewer" }, "body": "Resolved already." } ] }
            }
        ] } } } }
    });
    std::fs::write(
        root.join("graphql.json"),
        serde_json::to_string_pretty(&graphql).unwrap(),
    )
    .unwrap();

    let (code, stdout, stderr) = run_pr(root, &bin, &["pr", "ready", "1043", "--format", "json"]);
    assert_eq!(code, Some(1), "open thread must block readiness:\n{stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(report["ready"], serde_json::json!(false));
    let threads = report["conversations"]["unresolved_threads"]
        .as_array()
        .expect("unresolved threads");
    assert_eq!(threads.len(), 1, "only the unresolved thread counts");
    assert_eq!(threads[0]["path"], "src/lib.rs");
    assert_eq!(threads[0]["line"], serde_json::json!(42));
    assert_eq!(threads[0]["author"], "reviewer");
    let excerpt = threads[0]["excerpt"].as_str().unwrap();
    assert!(excerpt.starts_with(&"a".repeat(79)), "excerpt: {excerpt}");
    assert!(excerpt.ends_with("€..."), "excerpt: {excerpt}");
    assert_eq!(excerpt.chars().count(), 83, "80 chars plus the ellipsis");
}

#[cfg(unix)]
#[test]
fn codecov_numbers_survive_case_folding_length_changes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    std::fs::write(
        root.join("view.json"),
        r#"{
            "number": 1045,
            "baseRefName": "main",
            "headRefOid": "abcd1234abcd1234abcd1234abcd1234abcd1234",
            "mergeStateStatus": "CLEAN",
            "mergeable": "MERGEABLE",
            "autoMergeRequest": null
        }"#,
    )
    .unwrap();

    std::fs::write(
        root.join("comments.json"),
        r###"[
            {
                "id": 201,
                "user": { "login": "codecov[bot]" },
                "body": "## [Codecov](https://app.codecov.io) Report\nİstanbul locale note: Patch coverage is 0.00% with 2 lines in your changes missing coverage.",
                "created_at": "2026-09-25T10:00:00Z",
                "html_url": "https://github.com/d-o-hub/repo/pull/1045#issuecomment-201"
            }
        ]"###,
    )
    .unwrap();

    let (code, stdout, stderr) = run_pr(root, &bin, &["pr", "ready", "1045", "--format", "json"]);
    assert_eq!(code, Some(1), "the gap still blocks readiness:\n{stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(report["codecov"]["patch_coverage"], serde_json::json!(0.0));
    assert_eq!(report["codecov"]["missing_lines"], serde_json::json!(2));
}

#[cfg(unix)]
#[test]
fn only_codecov_logins_report_and_bots_do_not_answer() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    std::fs::write(
        root.join("view.json"),
        r#"{
            "number": 1046,
            "baseRefName": "main",
            "headRefOid": "abcd1234abcd1234abcd1234abcd1234abcd1234",
            "mergeStateStatus": "CLEAN",
            "mergeable": "MERGEABLE",
            "autoMergeRequest": null
        }"#,
    )
    .unwrap();

    let report_comment = r###"{
        "id": 301,
        "user": { "login": "codecov[bot]" },
        "body": "## [Codecov](https://app.codecov.io) Report\nAttention: Patch coverage is 0.00% with 2 lines in your changes missing coverage.",
        "created_at": "2026-09-25T10:00:00Z",
        "html_url": "https://github.com/d-o-hub/repo/pull/1046#issuecomment-301"
    }"###;

    // A bot notice after the report is not an answer: the gap still blocks.
    std::fs::write(
        root.join("comments.json"),
        format!(
            r#"[{report_comment},
                {{
                    "id": 302,
                    "user": {{ "login": "renovate[bot]" }},
                    "body": "Dependency dashboard updated.",
                    "created_at": "2026-09-25T10:01:00Z",
                    "html_url": "https://github.com/d-o-hub/repo/pull/1046#issuecomment-302"
                }}]"#
        ),
    )
    .unwrap();
    let (code, stdout, stderr) = run_pr(root, &bin, &["pr", "ready", "1046", "--format", "json"]);
    assert_eq!(
        code,
        Some(1),
        "a bot notice must not waive the gap:\n{stderr}"
    );
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(report["codecov"]["is_actionable"], serde_json::json!(true));
    assert_eq!(report["codecov"]["patch_coverage"], serde_json::json!(0.0));
    assert_eq!(report["codecov"]["missing_lines"], serde_json::json!(2));
    assert_eq!(
        report["conversations"]["actionable_comments"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // An authorized human comment with explicit head SHA and finding waives the gap
    std::fs::write(
        root.join("comments.json"),
        format!(
            r#"[{report_comment},
                {{
                    "id": 303,
                    "user": {{ "login": "developer" }},
                    "body": "Waiver for abcd1234abcd1234abcd1234abcd1234abcd1234 (codecov): the Codecov numbers are stale for a docs-only change.",
                    "created_at": "2026-09-25T10:05:00Z",
                    "html_url": "https://github.com/d-o-hub/repo/pull/1046#issuecomment-303"
                }}]"#
        ),
    )
    .unwrap();
    let (code, stdout, stderr) = run_pr(root, &bin, &["pr", "ready", "1046", "--format", "json"]);
    assert_eq!(code, Some(0), "a human answer waives the gap:\n{stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(report["codecov"]["is_actionable"], serde_json::json!(false));
    assert_eq!(report["codecov"]["status"], serde_json::json!("waived"));

    // Codecov's comment-bot account carries no `[bot]` suffix and must still
    // be recognized as the report author.
    std::fs::write(
        root.join("comments.json"),
        format!(
            "[{}]",
            report_comment.replace("codecov[bot]", "codecov-commenter")
        ),
    )
    .unwrap();
    let (code, stdout, stderr) = run_pr(root, &bin, &["pr", "ready", "1046", "--format", "json"]);
    assert_eq!(code, Some(1), "the comment bot's gap blocks:\n{stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(report["codecov"]["is_actionable"], serde_json::json!(true));
}

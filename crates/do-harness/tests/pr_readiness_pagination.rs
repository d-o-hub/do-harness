//! Acceptance tests for PR readiness pagination and head SHA rechecks.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[cfg(unix)]
use serde_json::Value;

mod support;

#[cfg(unix)]
use support::pr_readiness::{fake_gh_router, run_pr};

#[cfg(unix)]
#[test]
fn paginated_threads_101st_unresolved_blocks_readiness() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    std::fs::write(
        root.join("view.json"),
        r#"{"number": 2001, "baseRefName": "main", "headRefOid": "head101", "mergeStateStatus": "CLEAN", "mergeable": "MERGEABLE"}"#,
    ).unwrap();

    let p1_nodes: Vec<Value> = (1..=100)
        .map(|i| {
            serde_json::json!({
                "id": format!("thread_{i}"),
                "isResolved": true,
                "isOutdated": false,
                "comments": { "nodes": [{ "author": { "login": "rev" }, "body": "ok" }] }
            })
        })
        .collect();

    let g1 = serde_json::json!({
        "data": { "repository": { "pullRequest": { "reviewThreads": {
            "pageInfo": { "hasNextPage": true, "endCursor": "cursor_p2" },
            "nodes": p1_nodes
        } } } }
    });
    std::fs::write(
        root.join("graphql.json"),
        serde_json::to_string(&g1).unwrap(),
    )
    .unwrap();

    let g2 = serde_json::json!({
        "data": { "repository": { "pullRequest": { "reviewThreads": {
            "pageInfo": { "hasNextPage": false, "endCursor": null },
            "nodes": [{
                "id": "thread_101",
                "isResolved": false,
                "isOutdated": false,
                "path": "src/late.rs",
                "line": 10,
                "comments": { "nodes": [{ "author": { "login": "rev" }, "body": "101st issue" }] }
            }]
        } } } }
    });
    std::fs::write(
        root.join("graphql_cursor_p2.json"),
        serde_json::to_string(&g2).unwrap(),
    )
    .unwrap();

    let (code, stdout, stderr) = run_pr(root, &bin, &["pr", "ready", "2001", "--format", "json"]);
    assert_eq!(
        code,
        Some(1),
        "unresolved 101st thread must block readiness:\n{stderr}"
    );
    let report: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["ready"], false);
    assert_eq!(
        report["conversations"]["unresolved_threads"][0]["id"],
        "thread_101"
    );
}

#[cfg(unix)]
#[test]
fn paginated_threads_all_resolved_passes_readiness() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    std::fs::write(
        root.join("view.json"),
        r#"{"number": 2002, "baseRefName": "main", "headRefOid": "headall", "mergeStateStatus": "CLEAN", "mergeable": "MERGEABLE"}"#,
    ).unwrap();

    let p1_nodes: Vec<Value> = (1..=100)
        .map(|i| {
            serde_json::json!({
                "id": format!("thread_{i}"),
                "isResolved": true,
                "isOutdated": false,
                "comments": { "nodes": [{ "author": { "login": "rev" }, "body": "ok" }] }
            })
        })
        .collect();

    let g1 = serde_json::json!({
        "data": { "repository": { "pullRequest": { "reviewThreads": {
            "pageInfo": { "hasNextPage": true, "endCursor": "c2" },
            "nodes": p1_nodes
        } } } }
    });
    std::fs::write(
        root.join("graphql.json"),
        serde_json::to_string(&g1).unwrap(),
    )
    .unwrap();

    let p2_nodes: Vec<Value> = (101..=110)
        .map(|i| {
            serde_json::json!({
                "id": format!("thread_{i}"),
                "isResolved": true,
                "isOutdated": false,
                "comments": { "nodes": [{ "author": { "login": "rev" }, "body": "ok" }] }
            })
        })
        .collect();

    let g2 = serde_json::json!({
        "data": { "repository": { "pullRequest": { "reviewThreads": {
            "pageInfo": { "hasNextPage": false, "endCursor": null },
            "nodes": p2_nodes
        } } } }
    });
    std::fs::write(
        root.join("graphql_c2.json"),
        serde_json::to_string(&g2).unwrap(),
    )
    .unwrap();

    let (code, stdout, stderr) = run_pr(root, &bin, &["pr", "ready", "2002", "--format", "json"]);
    assert_eq!(
        code,
        Some(0),
        "all resolved across pages must pass:\n{stderr}"
    );
    let report: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["ready"], true);
}

#[cfg(unix)]
#[test]
fn page_two_graphql_failure_fails_closed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    std::fs::write(
        root.join("view.json"),
        r#"{"number": 2003, "baseRefName": "main", "headRefOid": "headfail", "mergeStateStatus": "CLEAN", "mergeable": "MERGEABLE"}"#,
    ).unwrap();

    let g1 = serde_json::json!({
        "data": { "repository": { "pullRequest": { "reviewThreads": {
            "pageInfo": { "hasNextPage": true, "endCursor": "c2" },
            "nodes": [{ "id": "t1", "isResolved": true, "comments": { "nodes": [] } }]
        } } } }
    });
    std::fs::write(
        root.join("graphql.json"),
        serde_json::to_string(&g1).unwrap(),
    )
    .unwrap();
    std::fs::write(root.join("fail_graphql_page2"), "true").unwrap();

    let (code, stdout, stderr) = run_pr(root, &bin, &["pr", "ready", "2003", "--format", "json"]);
    assert_ne!(code, Some(0), "page two failure must fail closed");
    assert!(!stdout.contains(r#""ready": true"#));
    assert!(stderr.contains("reviewThreads failed"));
}

#[cfg(unix)]
#[test]
fn has_next_page_without_usable_cursor_errors() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    std::fs::write(
        root.join("view.json"),
        r#"{"number": 2004, "baseRefName": "main", "headRefOid": "headnocursor", "mergeStateStatus": "CLEAN", "mergeable": "MERGEABLE"}"#,
    ).unwrap();

    let g1 = serde_json::json!({
        "data": { "repository": { "pullRequest": { "reviewThreads": {
            "pageInfo": { "hasNextPage": true, "endCursor": null },
            "nodes": [{ "id": "t1", "isResolved": true, "comments": { "nodes": [] } }]
        } } } }
    });
    std::fs::write(
        root.join("graphql.json"),
        serde_json::to_string(&g1).unwrap(),
    )
    .unwrap();

    let (code, stdout, stderr) = run_pr(root, &bin, &["pr", "ready", "2004", "--format", "json"]);
    assert_ne!(code, Some(0), "missing cursor with hasNextPage must fail");
    assert!(!stdout.contains(r#""ready": true"#));
    assert!(stderr.contains("no usable advancing cursor"));
}

#[cfg(unix)]
#[test]
fn head_moving_during_collection_yields_no_ready_result() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    std::fs::write(root.join("view_seq.txt"), "0").unwrap();
    for i in 1..=6 {
        let oid = format!("head_v{i}");
        let view = serde_json::json!({
            "number": 2006,
            "baseRefName": "main",
            "headRefOid": oid,
            "mergeStateStatus": "CLEAN",
            "mergeable": "MERGEABLE"
        });
        std::fs::write(root.join(format!("view_{i}.json")), view.to_string()).unwrap();
    }

    let (code, stdout, stderr) = run_pr(root, &bin, &["pr", "ready", "2006", "--format", "json"]);
    assert_ne!(code, Some(0), "unstable head SHA must fail closed");
    assert!(!stdout.contains(r#""ready": true"#));
    assert!(
        stderr.contains("PR state moved during inspection"),
        "stderr: {stderr}"
    );
}

#[cfg(unix)]
#[test]
fn paginated_issue_comments_101st_comment_is_inventoried() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    std::fs::write(
        root.join("view.json"),
        r#"{"number": 2007, "baseRefName": "main", "headRefOid": "head101c", "mergeStateStatus": "CLEAN", "mergeable": "MERGEABLE"}"#,
    ).unwrap();

    // Generate 101 comments: first 100 courtesy, 101st is an actionable finding
    let mut comments: Vec<Value> = (1..=100)
        .map(|i| {
            serde_json::json!({
                "id": i,
                "user": { "login": "dev" },
                "body": "looks good",
                "created_at": "2026-11-01T00:00:00Z"
            })
        })
        .collect();

    comments.push(serde_json::json!({
        "id": 101,
        "user": { "login": "security-bot[bot]" },
        "body": "Warning: 101st issue comment finding",
        "created_at": "2026-11-01T01:00:00Z"
    }));

    std::fs::write(
        root.join("comments.json"),
        serde_json::to_string(&comments).unwrap(),
    )
    .unwrap();

    let (code, stdout, _stderr) = run_pr(root, &bin, &["pr", "ready", "2007", "--format", "json"]);
    assert_eq!(code, Some(1), "101st comment finding must block");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(report["ready"], false);
    let triage = report["conversations"]["needs_triage"].as_array().unwrap();
    assert!(triage.iter().any(|t| t["item_ref"] == "issue-comment:101"));
}

#[cfg(unix)]
#[test]
fn reviews_api_failure_fails_closed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    std::fs::write(
        root.join("view.json"),
        r#"{"number": 2008, "baseRefName": "main", "headRefOid": "head_err", "mergeStateStatus": "CLEAN", "mergeable": "MERGEABLE"}"#,
    ).unwrap();

    // Fail reviews API call (HTTP 403)
    std::fs::write(root.join("fail_reviews"), "").unwrap();

    let (code, stdout, stderr) = run_pr(root, &bin, &["pr", "ready", "2008", "--format", "json"]);
    assert_ne!(code, Some(0), "reviews failure must fail closed");
    assert!(stderr.contains("could not read reviews") || stderr.contains("permission denied"));
    assert!(!stdout.contains(r#""ready": true"#));
}

//! Acceptance fixtures for the deterministic PR sweep (#471).
//!
//! Each case drives the real binary against a fake `gh`, so the state
//! machine, the mutation boundary, and the report schema are exercised
//! without network access.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[cfg(unix)]
use serde_json::{Value, json};

#[cfg(unix)]
mod support;
#[cfg(unix)]
use support::pr_sweep::{
    BASE, HEAD, MERGE_SHA, UPDATED_HEAD, failing_checks, mutations, one_pr_list, run_sweep, setup,
    write_pr_list, write_view,
};

#[cfg(unix)]
#[test]
#[allow(clippy::too_many_lines)]
fn an_eligible_own_pr_merges_only_after_final_proof() {
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &one_pr_list(2));
    write_view(root, 2, &json!({}));

    eprintln!(
        "ENV-DEBUG bin={:?} gh_extra={:?} permission={:?}",
        bin,
        std::fs::read_to_string(root.join("gh_extra.sh"))
            .ok()
            .map(|s| s.len()),
        std::fs::read_to_string(root.join("permission.json")).ok()
    );
    let (code, stdout, stderr) = run_sweep(
        root,
        &bin,
        &[
            "pr",
            "sweep",
            "--pr",
            "2",
            "--execute",
            "--merge",
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(0), "eligible PR must merge: {stderr} {stdout}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["zero_model"], true);
    assert_eq!(report["target"]["pr"], 2);
    let pr = &report["prs"][0];
    assert_eq!(pr["outcome"], "merged", "report={report}");
    assert_eq!(pr["authorized"], true);
    assert_eq!(pr["base_sha"], BASE);
    assert_eq!(pr["head_sha"], HEAD);
    assert_eq!(pr["merge_sha"], MERGE_SHA);
    assert_eq!(pr["post_merge"], "pass");

    let cmds = mutations(root);
    assert_eq!(
        cmds,
        vec![format!("pr merge 2 --squash --match-head-commit {HEAD}")]
    );
    assert!(
        cmds.iter().all(|cmd| !cmd.contains("--auto")),
        "never auto-merge"
    );
}

#[cfg(unix)]
#[test]
#[allow(clippy::too_many_lines)]
fn a_second_pr_is_updated_and_revalidated_after_the_first_merge() {
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(
        root,
        &json!([
            {
                "number": 5,
                "title": "pr 5",
                "url": "https://github.com/acme/widget/pull/5",
                "author": { "login": "d-o-hub" },
                "createdAt": "2026-10-01T00:00:00Z",
                "updatedAt": "2026-10-02T00:00:00Z",
                "state": "OPEN",
                "isDraft": false,
                "baseRefName": "main",
                "baseRefOid": BASE,
                "headRefName": "topic-5",
                "headRefOid": HEAD,
                "mergeStateStatus": "CLEAN",
                "labels": [],
                "headRepository": { "nameWithOwner": "acme/widget" }
            },
            {
                "number": 6,
                "title": "pr 6",
                "url": "https://github.com/acme/widget/pull/6",
                "author": { "login": "d-o-hub" },
                "createdAt": "2026-10-03T00:00:00Z",
                "updatedAt": "2026-10-03T00:00:00Z",
                "state": "OPEN",
                "isDraft": false,
                "baseRefName": "topic-5",
                "baseRefOid": BASE,
                "headRefName": "topic-6",
                "headRefOid": "4444444444444444444444444444444444444444",
                "mergeStateStatus": "BEHIND",
                "labels": [],
                "headRepository": { "nameWithOwner": "acme/widget" }
            }
        ]),
    );
    write_view(root, 5, &json!({}));
    std::fs::write(
        root.join("files_5.json"),
        json!([{"filename": "src/a.rs"}]).to_string(),
    )
    .unwrap();
    std::fs::write(
        root.join("files_6.json"),
        json!([{"filename": "src/b.rs"}]).to_string(),
    )
    .unwrap();
    write_view(
        root,
        6,
        &json!({
            "baseRefName": "topic-5",
            "headRefName": "topic-6",
            "headRefOid": "4444444444444444444444444444444444444444",
            "mergeStateStatus": "BEHIND",
        }),
    );

    let (code, stdout, stderr) = run_sweep(
        root,
        &bin,
        &[
            "pr",
            "sweep",
            "--all",
            "--execute",
            "--merge",
            "--format",
            "json",
        ],
    );
    assert_eq!(
        code,
        Some(0),
        "stack must merge parent then child: {stderr} {stdout}"
    );
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    let outcomes: Vec<&str> = report["prs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|pr| pr["outcome"].as_str().unwrap())
        .collect();
    assert_eq!(outcomes, vec!["merged", "merged"]);

    let cmds = mutations(root);
    let merge5 = cmds
        .iter()
        .position(|cmd| cmd.contains("pr merge 5"))
        .expect("parent merge");
    let merge6 = cmds
        .iter()
        .position(|cmd| cmd.contains("pr merge 6"))
        .expect("child merge");
    assert!(merge5 < merge6, "parent merges before child: {cmds:?}");
    assert!(cmds.iter().any(|cmd| {
        cmd.contains("update-branch")
            && cmd.contains("expected_head_sha=4444444444444444444444444444444444444444")
    }));
    assert!(
        cmds.iter().any(|cmd| {
            *cmd == format!("pr merge 6 --squash --match-head-commit {UPDATED_HEAD}")
        })
    );
}

#[cfg(unix)]
#[test]
#[allow(clippy::too_many_lines)]
fn every_blocker_refuses_without_a_merge() {
    // Draft.
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &one_pr_list(2));
    write_view(root, 2, &json!({"isDraft": true}));
    let (code, stdout, _) = run_sweep(
        root,
        &bin,
        &[
            "pr",
            "sweep",
            "--pr",
            "2",
            "--execute",
            "--merge",
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(0));
    let report: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["prs"][0]["outcome"], "skipped");
    assert_eq!(report["prs"][0]["reasons"][0]["code"], "draft");
    assert_eq!(mutations(root), [] as [std::string::String; 0]);

    // Held label.
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &one_pr_list(2));
    write_view(root, 2, &json!({"labels": [{"name": "hold"}]}));
    let (code, stdout, _) = run_sweep(
        root,
        &bin,
        &[
            "pr",
            "sweep",
            "--pr",
            "2",
            "--execute",
            "--merge",
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(0));
    let report: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["prs"][0]["reasons"][0]["code"], "held");
    assert_eq!(mutations(root), [] as [std::string::String; 0]);

    // Failing checks.
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &one_pr_list(2));
    write_view(root, 2, &json!({}));
    std::fs::write(root.join("check_runs.json"), failing_checks().to_string()).unwrap();
    let (code, stdout, _) = run_sweep(
        root,
        &bin,
        &[
            "pr",
            "sweep",
            "--pr",
            "2",
            "--execute",
            "--merge",
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(0));
    let report: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["prs"][0]["outcome"], "blocked");
    assert_eq!(report["prs"][0]["reasons"][0]["code"], "checks_failed");
    assert!(
        report["prs"][0]["next_action"]
            .as_str()
            .unwrap()
            .contains("rerun")
    );
    assert_eq!(mutations(root), [] as [std::string::String; 0]);

    // Unresolved conversation thread.
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &one_pr_list(2));
    write_view(root, 2, &json!({}));
    std::fs::write(
        root.join("graphql.json"),
        json!({
            "data": { "repository": { "pullRequest": { "reviewThreads": {
                "nodes": [{
                    "id": "thread-1",
                    "isResolved": false,
                    "path": "src/lib.rs",
                    "line": 1,
                    "comments": { "nodes": [
                        { "author": { "login": "reviewer" }, "body": "Fix this." }
                    ] }
                }]
            } } } }
        })
        .to_string(),
    )
    .unwrap();
    let (code, stdout, _) = run_sweep(
        root,
        &bin,
        &[
            "pr",
            "sweep",
            "--pr",
            "2",
            "--execute",
            "--merge",
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(0));
    let report: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["prs"][0]["outcome"], "blocked");
    let codes: Vec<&str> = report["prs"][0]["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["code"].as_str().unwrap())
        .collect();
    assert!(codes.contains(&"conversation_unresolved"));
    assert_eq!(mutations(root), [] as [std::string::String; 0]);

    // Policy drift.
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &one_pr_list(2));
    write_view(root, 2, &json!({}));
    let detail = root.join("ruleset_detail.json");
    let mut value: Value =
        serde_json::from_str(&std::fs::read_to_string(&detail).unwrap()).unwrap();
    value["name"] = json!("drifted");
    std::fs::write(&detail, value.to_string()).unwrap();
    let (code, stdout, _) = run_sweep(
        root,
        &bin,
        &[
            "pr",
            "sweep",
            "--pr",
            "2",
            "--execute",
            "--merge",
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(0));
    let report: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["prs"][0]["outcome"], "blocked");
    let codes: Vec<&str> = report["prs"][0]["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["code"].as_str().unwrap())
        .collect();
    assert!(codes.contains(&"policy_drift"));
    assert_eq!(mutations(root), [] as [std::string::String; 0]);

    // External author: never touched.
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &one_pr_list(2));
    write_view(root, 2, &json!({}));
    std::fs::write(
        root.join("permission.json"),
        json!({"permission": "read", "role_name": "read"}).to_string(),
    )
    .unwrap();
    let (code, stdout, _) = run_sweep(
        root,
        &bin,
        &[
            "pr",
            "sweep",
            "--pr",
            "2",
            "--execute",
            "--merge",
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(0));
    let report: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["prs"][0]["outcome"], "skipped");
    assert_eq!(report["prs"][0]["reasons"][0]["code"], "external");
    assert_eq!(mutations(root), [] as [std::string::String; 0]);

    // Conflict.
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &one_pr_list(2));
    write_view(root, 2, &json!({"mergeable": "CONFLICTING"}));
    let (code, stdout, _) = run_sweep(
        root,
        &bin,
        &[
            "pr",
            "sweep",
            "--pr",
            "2",
            "--execute",
            "--merge",
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(0));
    let report: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["prs"][0]["outcome"], "skipped");
    assert_eq!(report["prs"][0]["reasons"][0]["code"], "conflict");
    assert_eq!(mutations(root), [] as [std::string::String; 0]);
}

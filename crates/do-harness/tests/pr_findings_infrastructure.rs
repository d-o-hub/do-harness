//! Findings fixtures for workflow-level infrastructure notices (#466).

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
fn write_view(root: &Path, pr: u64, head: &str, base: &str) {
    std::fs::write(
        root.join("view.json"),
        json!({
            "number": pr,
            "baseRefName": "main",
            "baseRefOid": base,
            "headRefOid": head,
            "mergeStateStatus": "CLEAN",
            "mergeable": "MERGEABLE",
            "autoMergeRequest": null
        })
        .to_string(),
    )
    .unwrap();
}

#[cfg(unix)]
fn run_findings(root: &Path, bin: &Path, args: &[&str]) -> (Option<i32>, String, String) {
    let path = std::env::var("PATH").unwrap_or_default();
    let output = Command::new(env!("CARGO_BIN_EXE_do-harness"))
        .arg("--root")
        .arg(root)
        .arg("pr")
        .arg("findings")
        .args(args)
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
fn workflow_level_notices_never_gate() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    write_view(root, 3007, "headnotice", "basenotice");

    std::fs::write(
        root.join("check_runs.json"),
        json!({
            "total_count": 1,
            "check_runs": [{
                "id": 107,
                "name": "verify",
                "status": "completed",
                "conclusion": "success",
                "app": { "slug": "github-actions" }
            }]
        })
        .to_string(),
    )
    .unwrap();

    // GitHub reports a deprecated action runtime against the workflow
    // directory, not a source file: it must not be collected as a finding.
    std::fs::write(
        root.join("annotations.json"),
        json!([{
            "path": ".github",
            "start_line": 3,
            "end_line": 3,
            "annotation_level": "warning",
            "message": "Node.js 20 is deprecated for this action"
        }])
        .to_string(),
    )
    .unwrap();

    let (code, stdout, stderr) = run_findings(root, &bin, &["3007", "--format", "json"]);
    assert_eq!(code, Some(0), "plan must succeed: {stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["findings"].as_array().unwrap().len(), 0);
    assert_eq!(report["warnings"], 0);
    assert_eq!(report["untracked_findings"], 0);
}

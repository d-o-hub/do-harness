//! Second half of the PR sweep acceptance fixtures (#471).

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[cfg(unix)]
use serde_json::{Value, json};

#[cfg(unix)]
mod support;
#[cfg(unix)]
use support::pr_sweep::{
    default_view, failing_checks, mutations, one_pr_list, run_sweep, setup, write_pr_list,
    write_view,
};

#[cfg(unix)]
#[test]
fn auto_merge_disarm_happens_only_in_execute_mode() {
    // Dry-run: armed auto-merge blocks without any mutation.
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &one_pr_list(2));
    write_view(root, 2, &json!({"autoMergeRequest": {"enabled": true}}));
    let (code, stdout, _) = run_sweep(
        root,
        &bin,
        &["pr", "sweep", "--pr", "2", "--format", "json"],
    );
    assert_eq!(code, Some(0));
    let report: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["prs"][0]["outcome"], "blocked");
    assert_eq!(report["prs"][0]["reasons"][0]["code"], "auto_merge_armed");
    assert_eq!(
        mutations(root),
        [] as [std::string::String; 0],
        "dry-run never mutates"
    );

    // Execute without --merge: disarm happens, no merge.
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &one_pr_list(2));
    write_view(root, 2, &json!({"autoMergeRequest": {"enabled": true}}));
    let (code, stdout, _) = run_sweep(
        root,
        &bin,
        &["pr", "sweep", "--pr", "2", "--execute", "--format", "json"],
    );
    assert_eq!(code, Some(0));
    let report: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["prs"][0]["outcome"], "deferred");
    let cmds = mutations(root);
    assert!(
        cmds.iter()
            .any(|cmd| cmd.contains("auto-merge") && cmd.contains("DELETE")),
        "disarm must issue DELETE: {cmds:?}"
    );
    assert!(
        cmds.iter().all(|cmd| !cmd.contains("pr merge")),
        "no merge without --merge"
    );

    // Failed disarm blocks.
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &one_pr_list(2));
    write_view(root, 2, &json!({"autoMergeRequest": {"enabled": true}}));
    std::fs::write(root.join("fail_disarm"), "x").unwrap();
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
    assert_eq!(report["prs"][0]["reasons"][0]["code"], "disarm_failed");
}

#[cfg(unix)]
#[test]
fn repeat_and_resume_runs_do_no_duplicate_work_and_no_stale_skip() {
    // Run 1 merges PR 2; run 2 sees the PR closed and does nothing — exactly
    // one merge total across both runs.
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &one_pr_list(2));
    write_view(root, 2, &json!({}));
    let (code, _, stderr) = run_sweep(
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
    assert_eq!(code, Some(0), "{stderr}");
    write_pr_list(root, &json!([]));
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
    assert_eq!(code, Some(0), "{stderr}");
    let report: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["prs"].as_array().unwrap().len(), 1);
    assert_eq!(report["prs"][0]["outcome"], "skipped");
    assert_eq!(report["prs"][0]["reasons"][0]["code"], "closed");
    let cmds = mutations(root);
    assert_eq!(
        cmds.iter().filter(|cmd| cmd.contains("pr merge")).count(),
        1
    );

    // A blocked run does not stick: after fixing the failing check, the next
    // run merges (no stale skip).
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
    assert_eq!(mutations(root), [] as [std::string::String; 0]);
    std::fs::remove_file(root.join("check_runs.json")).unwrap();
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
    assert_eq!(report["prs"][0]["outcome"], "merged");
}

#[cfg(unix)]
#[test]
#[allow(clippy::too_many_lines)]
fn races_are_unknown_or_blocked_and_post_merge_failure_halts() {
    eprintln!("SUBCASE-moving-start");
    // A moving head is UNKNOWN, never merged.
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &one_pr_list(2));
    write_view(root, 2, &json!({}));
    std::fs::write(root.join("always_moving"), "x").unwrap();
    let mut moving = default_view(2);
    moving["headRefOid"] = json!("9999999999999999999999999999999999999999");
    std::fs::write(root.join("view_move_1.json"), moving.to_string()).unwrap();
    std::fs::write(root.join("view_move_2.json"), default_view(2).to_string()).unwrap();
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
    assert_eq!(
        code,
        Some(0),
        "per-PR unknown does not fail the sweep: code={code:?} stderr={stderr}"
    );
    let report: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        report["prs"][0]["outcome"],
        "unknown",
        "report={} seq={:?} cmds={:?}",
        report,
        std::fs::read_to_string(root.join("view_seq.txt")),
        mutations(root)
    );
    assert!(mutations(root).iter().all(|cmd| !cmd.contains("pr merge")));
    let _ = std::fs::remove_file(root.join("always_moving"));
    let _ = std::fs::remove_file(root.join("move_toggle.txt"));

    eprintln!("SUBCASE-regression-start");
    // Post-merge regression halts the whole sweep.
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &one_pr_list(2));
    write_view(root, 2, &json!({}));
    std::fs::write(
        root.join("check_runs_merge.json"),
        json!([{
            "total_count": 1,
            "check_runs": [{
                "id": 7, "name": "verify", "status": "completed",
                "conclusion": "failure", "app": { "slug": "github-actions" }
            }]
        }])
        .to_string(),
    )
    .unwrap();
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
    assert_eq!(code, Some(1), "regression must halt with exit 1: {stderr}");
    let report: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["prs"][0]["post_merge"], "regression");
    assert_eq!(report["halted"]["code"], "regression");

    eprintln!("SUBCASE-unknown-start");
    // Unknown post-merge deadline (empty runs) halts with exit 2.
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &one_pr_list(2));
    write_view(root, 2, &json!({}));
    std::fs::write(
        root.join("check_runs_merge.json"),
        json!([{
            "total_count": 0,
            "check_runs": []
        }])
        .to_string(),
    )
    .unwrap();
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
            "--wait",
            "2",
            "--format",
            "json",
        ],
    );
    assert_eq!(
        code,
        Some(2),
        "unknown deadline must halt with exit 2: {stderr}"
    );
    let report: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["prs"][0]["post_merge"], "unknown");
    assert_eq!(report["halted"]["code"], "unknown");
}

#[cfg(unix)]
#[test]
fn dry_run_reports_and_never_mutates() {
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &one_pr_list(2));
    write_view(root, 2, &json!({}));
    let (code, stdout, stderr) =
        run_sweep(root, &bin, &["pr", "sweep", "--all", "--format", "json"]);
    assert_eq!(code, Some(0), "{stderr}");
    let report: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["mode"]["dry_run"], true);
    assert_eq!(report["mode"]["execute"], false);
    assert_eq!(report["mode"]["merge"], false);
    assert_eq!(report["target"]["all"], true);
    assert_eq!(report["prs"][0]["outcome"], "deferred");
    assert!(
        mutations(root).is_empty(),
        "dry run must make zero mutations"
    );

    // The subcommand `--dry-run` overrides mutating flags.
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &one_pr_list(2));
    write_view(root, 2, &json!({}));
    let (code, stdout, stderr) = run_sweep(
        root,
        &bin,
        &[
            "pr",
            "sweep",
            "--all",
            "--execute",
            "--merge",
            "--dry-run",
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    let report: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["mode"]["dry_run"], true);
    assert_eq!(report["mode"]["execute"], false);
    assert_eq!(report["mode"]["merge"], false);
    assert!(
        mutations(root).is_empty(),
        "subcommand --dry-run must make zero mutations"
    );

    // The global `--dry-run` also overrides mutating flags.
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &one_pr_list(2));
    write_view(root, 2, &json!({}));
    let (code, stdout, stderr) = run_sweep(
        root,
        &bin,
        &[
            "--dry-run",
            "pr",
            "sweep",
            "--all",
            "--execute",
            "--merge",
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    let report: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["mode"]["dry_run"], true);
    assert_eq!(report["mode"]["execute"], false);
    assert_eq!(report["mode"]["merge"], false);
    assert!(
        mutations(root).is_empty(),
        "global --dry-run must make zero mutations"
    );
}

#[cfg(unix)]
#[test]
fn usage_errors_are_rejected() {
    let (dir, bin) = setup();
    let root = dir.path();
    let (code, _, stderr) = run_sweep(root, &bin, &["pr", "sweep", "--format", "json"]);
    assert_eq!(code, Some(2), "no target must be a usage error: {stderr}");
    let (code, _, stderr) = run_sweep(
        root,
        &bin,
        &["pr", "sweep", "--all", "--pr", "2", "--format", "json"],
    );
    assert_eq!(
        code,
        Some(2),
        "conflicting targets must be a usage error: {stderr}"
    );
    let (code, _, stderr) = run_sweep(
        root,
        &bin,
        &["pr", "sweep", "--pr", "2", "--merge", "--format", "json"],
    );
    assert_eq!(
        code,
        Some(2),
        "--merge without --execute must be rejected: {stderr}"
    );
}

//! Author-classification acceptance fixtures for the deterministic PR sweep
//! (#514).
//!
//! Each case drives the real binary against a fake `gh`: `GitHub`'s `is_bot`
//! flag is authoritative, an App-authored `app/<slug>` login costs no
//! collaborator-permission call, and the two permission failures stay
//! distinguishable (a `404` is a third party, a `403` is an unknown identity).

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[cfg(unix)]
use serde_json::{Value, json};

#[cfg(unix)]
mod support;
#[cfg(unix)]
use support::pr_sweep::{
    HEAD, app_bot_author, app_bot_pr_list, human_author, human_pr_list, mutations,
    permission_calls, run_sweep, setup, write_pr_list, write_view,
};

#[cfg(unix)]
#[test]
fn an_app_authored_bot_pr_merges_end_to_end_without_a_permission_read() {
    // #514: `gh` reports the author as `app/dependabot` with `is_bot: true`,
    // and the collaborator endpoint answers 404 for that login. The sweep must
    // classify from the server flag, never consult permission, and merge.
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &app_bot_pr_list(2));
    write_view(root, 2, &json!({ "author": app_bot_author() }));
    std::fs::write(root.join("permission_404"), "x").unwrap();

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
    assert_eq!(code, Some(0), "bot PR must merge: {stderr} {stdout}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    let pr = &report["prs"][0];
    assert_eq!(pr["outcome"], "merged", "report={report}");
    assert_eq!(pr["author"], "app/dependabot");
    assert_eq!(pr["authorized"], true);
    assert_eq!(report["summary"]["merged"], 1);
    assert_eq!(report["summary"]["skipped"], 0);
    assert_eq!(report["summary"]["unknown"], 0);

    assert_eq!(
        permission_calls(root),
        [] as [std::string::String; 0],
        "an App author must cost zero permission calls"
    );
    let cmds = mutations(root);
    assert_eq!(cmds.len(), 1, "exactly one mutation: {cmds:?}");
    assert_eq!(
        cmds,
        vec![format!("pr merge 2 --squash --match-head-commit {HEAD}")]
    );
}

#[cfg(unix)]
#[test]
fn a_human_author_is_external_on_404_and_unknown_on_403() {
    // 404 is an answer (not a collaborator); 403 is unreadable. Treating them
    // alike would either trust an unreadable author or refuse a real third
    // party (#514).
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &human_pr_list(2));
    write_view(root, 2, &json!({ "author": human_author() }));
    std::fs::write(root.join("permission_404"), "x").unwrap();
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
    assert_eq!(report["prs"][0]["outcome"], "skipped");
    assert_eq!(report["prs"][0]["reasons"][0]["code"], "external");
    assert_eq!(mutations(root), [] as [std::string::String; 0]);

    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &human_pr_list(2));
    write_view(root, 2, &json!({ "author": human_author() }));
    std::fs::write(root.join("permission_fail"), "x").unwrap();
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
    assert_eq!(report["prs"][0]["outcome"], "skipped");
    assert_eq!(report["prs"][0]["reasons"][0]["code"], "unknown_identity");
    assert_eq!(
        report["prs"][0]["authorized"], false,
        "an unknown identity is never owned"
    );
    assert_eq!(mutations(root), [] as [std::string::String; 0]);
}

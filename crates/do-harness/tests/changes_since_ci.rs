//! Acceptance fixtures for opt-in CI base resolution (`--since @ci`).
//!
//! `@ci` must never turn a missing environment or an unfetched base branch
//! into a green run: the whole point of the explicit `--since` spelling (#551)
//! was that a green result cannot come from an empty diff. These cases prove
//! the sentinel fails closed (exit 2) and otherwise selects exactly what the
//! explicit ref would.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use serde_json::Value;

mod support;

use support::range_selection::{committed_rust_change, fixture_repo, rev, sensor_names};
use support::{harness, run};

/// CI variables the resolver knows about; every case starts from none of them
/// so an inherited developer shell cannot make a test pass by accident.
const CI_BASE_VARIABLES: [&str; 7] = [
    "GITHUB_BASE_REF",
    "CI_MERGE_REQUEST_TARGET_BRANCH_NAME",
    "SYSTEM_PULLREQUEST_TARGETBRANCH",
    "BITBUCKET_PR_DESTINATION_BRANCH",
    "BUILDKITE_PULL_REQUEST_BASE_BRANCH",
    "CHANGE_TARGET",
    "CI_COMMIT_TARGET_BRANCH",
];

/// Runs `verify --set all --since @ci --format json` with only `variable` set.
fn verify_ci(root: &Path, variable: Option<(&str, &str)>) -> (Option<i32>, String, String) {
    let mut command = harness(root);
    for name in CI_BASE_VARIABLES {
        command.env_remove(name);
    }
    if let Some((name, value)) = variable {
        command.env(name, value);
    }
    run(command
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--since")
        .arg("@ci")
        .arg("--format")
        .arg("json"))
}

/// Publishes `sha` as `origin/<branch>` so provider branch names resolve.
fn publish_origin(root: &Path, branch: &str, sha: &str) {
    support::git(
        root,
        &["update-ref", &format!("refs/remotes/origin/{branch}"), sha],
    );
}

#[test]
fn ci_sentinel_without_a_known_variable_exits_2() {
    let (_dir, root) = fixture_repo();
    let (code, stdout, stderr) = verify_ci(&root, None);
    assert_eq!(code, Some(2), "no CI variable must be a usage error");
    assert!(stdout.is_empty(), "usage errors print no report: {stdout}");
    assert!(
        stderr.contains("GITHUB_BASE_REF"),
        "the diagnostic must list the variables it looked for: {stderr}"
    );
}

#[test]
fn ci_sentinel_with_an_unfetched_base_exits_2() {
    let (_dir, root) = fixture_repo();
    let (code, _stdout, stderr) = verify_ci(&root, Some(("GITHUB_BASE_REF", "main")));
    assert_eq!(
        code,
        Some(2),
        "an unfetched base must be a usage error, not a full run: {stderr}"
    );
    assert!(
        stderr.contains("git fetch") || stderr.contains("fetch-depth"),
        "the diagnostic must say how to fetch the base: {stderr}"
    );
}

#[test]
fn ci_sentinel_selects_like_the_explicit_ref() {
    let (_dir, root) = fixture_repo();
    let base = committed_rust_change(&root);
    // The provider reports the base branch; the checkout must already have it.
    publish_origin(&root, "main", &base);

    let (code, stdout, stderr) = verify_ci(&root, Some(("GITHUB_BASE_REF", "main")));
    assert_eq!(code, Some(0), "@ci run must exit 0:\n{stderr}\n{stdout}");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(sensor_names(&report), vec!["rs-check".to_owned()]);
    // The requested spelling is recorded, and the revisions it resolved to are
    // the same ones the explicit ref produces.
    assert_eq!(report["selection"]["since"], serde_json::json!("@ci"));
    assert_eq!(report["selection"]["base_sha"], serde_json::json!(base));
    assert_eq!(
        report["selection"]["merge_base_sha"],
        serde_json::json!(base)
    );
    assert_eq!(
        report["selection"]["head_sha"],
        serde_json::json!(rev(&root, "HEAD"))
    );
}

#[test]
fn a_refs_heads_prefixed_provider_value_resolves() {
    let (_dir, root) = fixture_repo();
    let base = committed_rust_change(&root);
    publish_origin(&root, "main", &base);

    let (code, stdout, stderr) = verify_ci(
        &root,
        Some(("SYSTEM_PULLREQUEST_TARGETBRANCH", "refs/heads/main")),
    );
    assert_eq!(code, Some(0), "prefixed ref must resolve:\n{stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(sensor_names(&report), vec!["rs-check".to_owned()]);
}

#[test]
fn an_explicit_since_keeps_failing_closed_as_selection() {
    // Documented behaviour for explicit refs is unchanged by the sentinel:
    // a bad explicit ref still selects every candidate sensor rather than
    // exiting 2.
    let (_dir, root) = fixture_repo();
    let (code, stdout, _stderr) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--since")
        .arg("origin/does-not-exist")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "explicit failure still fails closed");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(
        sensor_names(&report),
        vec!["rs-check".to_owned(), "docs".to_owned()]
    );
}

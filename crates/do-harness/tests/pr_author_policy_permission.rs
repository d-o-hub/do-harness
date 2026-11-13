//! Real `GitHub` permission payloads for the author-policy boundary (#519).
//!
//! These three cases came from running the released binary against a real
//! repository rather than from the design. The collaborator endpoint answers for
//! *any* resolvable login, so a third party receives a role of `read` and only an
//! unresolvable login gets a `404`; the fixtures elsewhere in this tree model the
//! `404` shape, so without these the relaxed policy would refuse exactly the
//! contributors it exists to serve. The pair is deliberate: one test pins that a
//! `read` role is external (mergeable under the relaxed policy), the other that
//! it is never *owned* (refused under the default).
//!
//! A `403` is the third shape — a token without collaborator scope — and must
//! stay an unreadable identity rather than silently becoming either.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[cfg(unix)]
use serde_json::{Value, json};

mod support;

#[cfg(unix)]
use support::pr_sweep::{
    human_author, human_pr_list, mutations, run_sweep, setup, write_pr_list, write_view,
};

/// The relaxed policy.
#[cfg(unix)]
const RELAXED: &str = "owned-or-external-green";

/// The production permission payload for a non-collaborator is
/// `permission: "read"`, not a `404`.
///
/// Found by running the released binary against a real repository: `GitHub`
/// answers the collaborator endpoint for any resolvable login, so a third party
/// receives a role of `read`. Only a login that does not resolve at all gets a
/// `404`. Both must classify as external, or the relaxed policy would refuse
/// exactly the contributors it exists to serve.
#[cfg(unix)]
#[test]
fn a_real_read_role_response_classifies_as_external() {
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &human_pr_list(30));
    write_view(root, 30, &json!({ "author": human_author() }));
    std::fs::write(
        root.join("permission.json"),
        json!({
            "permission": "read",
            "role_name": "read",
            "user": { "login": "octocat" },
            "permissions": { "admin": false, "maintain": false, "push": false, "pull": true }
        })
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
            "30",
            "--execute",
            "--merge",
            "--author-policy",
            RELAXED,
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(
        report["prs"][0]["outcome"], "merged",
        "a `read` role is an external contributor, not an unreadable identity: {stdout}"
    );
    assert_eq!(report["prs"][0]["authorized"], true, "{stdout}");
}

/// A `read` role under the *default* policy is still refused.
///
/// The production payload must not accidentally become owned on the way to
/// classifying as external.
#[cfg(unix)]
#[test]
fn a_real_read_role_is_not_owned() {
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &human_pr_list(31));
    write_view(root, 31, &json!({ "author": human_author() }));
    std::fs::write(
        root.join("permission.json"),
        json!({ "permission": "read", "role_name": "read" }).to_string(),
    )
    .unwrap();

    let (code, stdout, stderr) = run_sweep(
        root,
        &bin,
        &[
            "pr",
            "sweep",
            "--pr",
            "31",
            "--execute",
            "--merge",
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["prs"][0]["outcome"], "skipped", "{stdout}");
    assert_eq!(
        report["prs"][0]["reasons"][0]["code"], "external",
        "{stdout}"
    );
    assert_eq!(mutations(root), [] as [String; 0], "{stdout}");
}

/// A `403` from the permission endpoint is an unreadable identity, and the
/// report says so rather than treating the author as external.
///
/// This is the production shape when the token lacks the collaborators scope.
#[cfg(unix)]
#[test]
fn a_403_is_an_unreadable_identity_not_an_external_one() {
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &human_pr_list(32));
    write_view(root, 32, &json!({ "author": human_author() }));
    std::fs::write(root.join("permission_fail"), "x").unwrap();

    let (code, stdout, stderr) = run_sweep(
        root,
        &bin,
        &[
            "pr",
            "sweep",
            "--pr",
            "32",
            "--execute",
            "--merge",
            "--author-policy",
            RELAXED,
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(
        report["prs"][0]["reasons"][0]["code"], "unknown_identity",
        "an unreadable answer must never be read as a third party: {stdout}"
    );
    assert_eq!(mutations(root), [] as [String; 0], "{stdout}");
}

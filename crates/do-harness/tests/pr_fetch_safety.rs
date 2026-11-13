//! The prefetch *bounds* (#520): does it stay inside what it is allowed to do?
//!
//! This fetch runs inside a path that can close a pull request, so the feature
//! being useful is not enough — it must also be bounded, idempotent, escapable,
//! and must never write a branch or a tag. Each property here is proven against a
//! **real** remote, because these are exactly the properties a stub would fake:
//! a fake `git` cannot show that a real fetch left `refs/heads/*` alone.
//!
//! The feature tests are in `pr_fetch.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[cfg(unix)]
use serde_json::Value;

mod support;

#[cfg(unix)]
use support::harness;
#[cfg(unix)]
use support::pr_fetch::setup;
#[cfg(unix)]
use support::pr_fetch_run::{assert_refs_untouched, refs, rev_exists, run_no_effect, snapshot};

/// `--no-fetch` restores today's behaviour: `UNKNOWN` plus the explicit command
/// to run. That is what makes the flag an honest escape hatch rather than a
/// silent downgrade.
#[cfg(unix)]
#[test]
fn no_fetch_restores_unknown_and_names_the_command() {
    let fixture = setup(true);
    let before = snapshot(&fixture.clone);
    let refs_before = refs(&fixture.clone);

    let (code, report, stderr) = run_no_effect(&fixture, &["--no-fetch"]);
    assert_eq!(
        code,
        Some(0),
        "an UNKNOWN verdict is still an analysis: {stderr}"
    );
    assert_eq!(
        report["merge_impact"]["verdict"], "UNKNOWN",
        "without the fetch the head is absent: {report}"
    );
    assert_eq!(report["closure_recommendation"], false, "{report}");

    // The remediation is carried by `reason` here rather than by a prefetch
    // warning: no prefetch ran, so the proof reported its own next action.
    let reason = report["merge_impact"]["reason"]
        .as_str()
        .unwrap_or_default();
    assert!(
        reason.contains("git fetch"),
        "the reason must name the next action: {reason:?}"
    );

    // And it really did not fetch.
    assert_eq!(snapshot(&fixture.clone), before, "no worktree change");
    assert_refs_untouched(&fixture.clone, &refs_before);
    assert!(
        !rev_exists(&fixture.clone, &fixture.head),
        "--no-fetch must leave the clone without the head"
    );
}

/// A full clone is the common path and must issue no fetch at all.
///
/// Proven by removing `origin` entirely: with both commits present, a prefetch
/// would fail against a missing remote, so a green result is proof that nothing
/// was fetched.
#[cfg(unix)]
#[test]
fn a_full_clone_fetches_nothing() {
    let fixture = setup(true);
    let before = snapshot(&fixture.clone);
    let refs_before = refs(&fixture.clone);
    // Unshallow so both commits and their ancestry exist, as on a real full
    // clone; then fetch the pull ref so the head is present too.
    support::git(&fixture.clone, &["fetch", "-q", "--unshallow", "origin"]);
    support::git(&fixture.clone, &["fetch", "-q", "origin", "main"]);
    support::git(
        &fixture.clone,
        &[
            "fetch",
            "-q",
            "origin",
            "+refs/pull/1263/head:refs/remotes/pr-triage/1263",
        ],
    );
    // Both commits exist and are connected, so nothing needs fetching: point
    // origin at a missing directory, and any fetch attempt now fails loudly.
    assert!(
        rev_exists(&fixture.clone, &fixture.base),
        "premise: base local"
    );
    assert!(
        rev_exists(&fixture.clone, &fixture.head),
        "premise: head local"
    );
    support::git(
        &fixture.clone,
        &["remote", "set-url", "origin", "/nonexistent/full-clone.git"],
    );

    let (code, report, stderr) = run_no_effect(&fixture, &[]);
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(
        report["merge_impact"]["verdict"], "TREE_EFFECT",
        "both commits are present, so the local proof must run: {report}"
    );
    assert_eq!(
        report["merge_impact"]["warnings"],
        serde_json::json!([]),
        "a full clone produces no prefetch warning: {report}"
    );
    assert_eq!(snapshot(&fixture.clone), before, "no worktree change");
    assert_refs_untouched(&fixture.clone, &refs_before);
}

/// A base that moves while the fetch is in flight must not be proven against.
///
/// Concurrent pushes to `main` are ordinary, and a proof prepared against one
/// base says nothing about the next. The fixture reports the planned base on the
/// first `pr view` and the moved base on every read after it, which is exactly
/// what this command observes when a push lands mid-fetch.
#[cfg(unix)]
#[test]
fn a_base_that_moves_during_the_fetch_refuses_the_proof() {
    let mut fixture = setup(true);
    let planned = fixture.base.clone();
    let moved = fixture.advance_base();
    assert_ne!(planned, moved, "the fixture must actually move the base");
    fixture.report_moved_base(&planned, &moved);
    let before = snapshot(&fixture.clone);
    let refs_before = refs(&fixture.clone);

    let (code, report, stderr) = run_no_effect(&fixture, &[]);
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(
        report["merge_impact"]["verdict"], "UNKNOWN",
        "a base that moved is not the base the proof was prepared against: {report}"
    );
    assert_eq!(
        report["merge_impact"]["evidence"], "base-moved",
        "the report must say why: {report}"
    );
    assert_eq!(
        report["closure_recommendation"], false,
        "a moved base can never recommend closure: {report}"
    );
    assert_eq!(snapshot(&fixture.clone), before, "no worktree change");
    assert_refs_untouched(&fixture.clone, &refs_before);
}

/// An unfetchable base yields `UNKNOWN` with the exact command, and mutates
/// nothing.
///
/// The remote is made unreachable after the clone exists, which is what a
/// network failure, an expired credential, or a revoked ref looks like.
#[cfg(unix)]
#[test]
fn an_unreachable_remote_is_unknown_with_a_command_and_no_mutation() {
    let fixture = setup(true);
    let before = snapshot(&fixture.clone);
    let refs_before = refs(&fixture.clone);
    // Point origin at a directory that does not exist: every fetch fails.
    support::git(
        &fixture.clone,
        &[
            "remote",
            "set-url",
            "origin",
            "/nonexistent/pr-fetch-fixture.git",
        ],
    );

    let (code, report, stderr) = run_no_effect(&fixture, &[]);
    assert_eq!(
        code,
        Some(0),
        "an UNKNOWN verdict is still an analysis: {stderr}"
    );
    assert_eq!(
        report["merge_impact"]["verdict"], "UNKNOWN",
        "an unreachable remote cannot prove anything: {report}"
    );
    assert_eq!(report["closure_recommendation"], false, "{report}");

    let warnings = report["merge_impact"]["warnings"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    assert!(
        warnings.contains("git fetch"),
        "the warning must name the command to run: {warnings:?}"
    );
    assert!(
        warnings.contains("refs/pull/1263/head") || warnings.contains(&fixture.base),
        "the warning must name the ref that could not be fetched: {warnings:?}"
    );

    // Immutability holds on the failure path too — this is the property the
    // issue's risk section is about.
    assert_eq!(snapshot(&fixture.clone), before, "no worktree change");
    assert_refs_untouched(&fixture.clone, &refs_before);
}

/// The prefetch is idempotent: a second run finds the objects it fetched and
/// costs no network, proven by removing the remote between runs.
#[cfg(unix)]
#[test]
fn a_second_run_is_a_no_op() {
    let fixture = setup(true);

    let (code, first, stderr) = run_no_effect(&fixture, &[]);
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(first["merge_impact"]["verdict"], "TREE_EFFECT", "{first}");

    // With the objects now local, an unreachable origin must not matter.
    support::git(&fixture.clone, &["remote", "remove", "origin"]);
    let (code, second, stderr) = run_no_effect(&fixture, &[]);
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(
        second["merge_impact"]["verdict"], "TREE_EFFECT",
        "the fetched commits stay usable: {second}"
    );
    assert_eq!(
        second["merge_impact"]["warnings"],
        serde_json::json!([]),
        "a run that needs no fetch warns about nothing: {second}"
    );
}

/// A fetch that has to be abandoned must not hang the command.
///
/// The bound is exercised through its real control (the environment override)
/// against a remote that sleeps: a `git` wrapper on `PATH` blocks so the fetch
/// can never finish, and the run must still return promptly with `UNKNOWN`.
#[cfg(unix)]
#[test]
fn a_stalled_fetch_is_bounded_and_reports_unknown() {
    let fixture = setup(true);
    let before = snapshot(&fixture.clone);
    let refs_before = refs(&fixture.clone);

    // A `git` shim that hangs only for `fetch`, so clone-time git is untouched.
    // It must delegate to the *absolute* real git: the shim directory is first on
    // `PATH`, so a bare `exec git` re-executes the shim and recurses forever.
    let shim_dir = fixture.clone.join("slow-bin");
    std::fs::create_dir_all(&shim_dir).unwrap();
    let real_git = String::from_utf8(
        std::process::Command::new("sh")
            .args(["-c", "command -v git"])
            .output()
            .expect("locate git")
            .stdout,
    )
    .expect("utf-8 git path");
    let shim = shim_dir.join("git");
    std::fs::write(
        &shim,
        format!(
            "#!/bin/sh\nif [ \"$1\" = \"fetch\" ]; then\n  sleep 300\n  exit 0\nfi\nexec {} \"$@\"\n",
            real_git.trim()
        ),
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let path = std::env::var("PATH").unwrap_or_default();
    let started = std::time::Instant::now();
    let output = harness(&fixture.clone)
        .env(
            "PATH",
            format!("{}:{}:{path}", shim_dir.display(), fixture.bin().display()),
        )
        .env("DO_HARNESS_FETCH_TIMEOUT_SECS", "1")
        .args(["pr", "no-effect", "1263", "--format", "json"])
        .output()
        .expect("spawn do-harness");
    let elapsed = started.elapsed();

    let report: Value =
        serde_json::from_slice(&output.stdout).expect("the command must still print a report");
    assert_eq!(
        report["merge_impact"]["verdict"], "UNKNOWN",
        "an abandoned fetch proves nothing: {report}"
    );
    assert!(
        elapsed < std::time::Duration::from_secs(60),
        "a stalled fetch must be bounded, took {elapsed:?}"
    );
    assert_eq!(snapshot(&fixture.clone), before, "no worktree change");
    assert_refs_untouched(&fixture.clone, &refs_before);
}

//! Reference-form cases for the `action-pins` sensor.
//!
//! Each case runs the real checker against a temporary repository and
//! asserts the exit code and the reported finding, because the policy is
//! about what the workflow file says, not about a grammar in isolation.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::fixtures::{
    CHECKOUT_SHA, RUST_CACHE_SHA, TOOLCHAIN_SHA, run, workflow_with, workspace, write_workflow,
};

/// A pinned action passes, and the report says how much was checked.
#[test]
fn pinned_action_passes() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        &workflow_with(&format!("actions/checkout@{CHECKOUT_SHA}")),
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 0, "a pinned action must pass: {output}");
    assert!(
        output.contains("FINDINGS: 0"),
        "the report must state zero findings: {output}"
    );
    assert!(
        output.contains("1 workflow file(s) checked"),
        "the report must state the checked file count: {output}"
    );
}

/// A reusable workflow pinned to a full SHA passes like any other action.
#[test]
fn pinned_reusable_workflow_passes() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/reusable.yml",
        "name: caller\non: push\njobs:\n  call:\n    uses: d-o-hub/do-harness/.github/workflows/reusable.yml@3d3c42e5aac5ba805825da76410c181273ba90b1\n",
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 0, "a pinned reusable workflow must pass: {output}");
}

/// A repository-local action and workflow reference stay valid.
#[test]
fn local_references_pass() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/local.yml",
        "name: local\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: ./.github/actions/example\n      - uses: ./.github/workflows/reusable.yml\n",
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 0, "local references must pass: {output}");
}

/// A Docker action image pinned to a sha256 digest passes, with or without a
/// tag in front of the digest.
#[test]
fn docker_digest_passes() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/docker.yml",
        "name: docker\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: docker://alpine:3.20@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n      - uses: docker://alpine@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n",
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 0, "a digest-pinned docker image must pass: {output}");
}

/// A mutable tag fails with the file and the line of the offending `uses`.
#[test]
fn mutable_tag_fails_with_file_and_line() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        &workflow_with("actions/checkout@v4"),
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "a tag must fail: {output}");
    assert!(
        output.contains(".github/workflows/pins.yml:7:"),
        "the finding must name the file and the `uses` line: {output}"
    );
    assert!(
        output.contains("FINDINGS: 1"),
        "the report must count the finding: {output}"
    );
}

/// A branch reference fails like a tag.
#[test]
fn branch_reference_fails() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        &workflow_with("actions/checkout@main"),
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "a branch must fail: {output}");
    assert!(
        output.contains(".github/workflows/pins.yml:7:"),
        "the finding must name the file and line: {output}"
    );
}

/// A seven-character SHA is not a full commit SHA.
#[test]
fn short_sha_fails() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        &workflow_with("actions/checkout@3d3c42e"),
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "a short SHA must fail: {output}");
    assert!(
        output.contains("short SHA"),
        "the finding must name the short-SHA case: {output}"
    );
}

/// A reference without a version fails.
#[test]
fn missing_reference_fails() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        &workflow_with("actions/checkout"),
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "a missing reference must fail: {output}");
    assert!(
        output.contains("no version"),
        "the finding must name the missing version: {output}"
    );
}

/// A `${{ ... }}` expression is not an immutable reference.
#[test]
fn expression_reference_fails() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        &workflow_with("actions/checkout@${{ matrix.ref }}"),
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "an expression must fail: {output}");
    assert!(
        output.contains("expression"),
        "the finding must name the expression case: {output}"
    );
}

/// A Docker image pinned only by tag is mutable.
#[test]
fn mutable_docker_tag_fails() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/docker.yml",
        "name: docker\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: docker://alpine:3.20\n",
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "a docker tag must fail: {output}");
    assert!(
        output.contains("sha256"),
        "the finding must name the digest requirement: {output}"
    );
}

/// A `uses` under a job (a reusable workflow) is checked like a step.
#[test]
fn job_level_uses_is_checked() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/caller.yml",
        "name: caller\non: push\njobs:\n  call:\n    uses: d-o-hub/do-harness/.github/workflows/reusable.yml@v1\n",
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "a mutable reusable workflow must fail: {output}");
    assert!(
        output.contains(".github/workflows/caller.yml:5:"),
        "the finding must name the job-level line: {output}"
    );
}

/// Several violations in one file are all reported, each with its own line.
#[test]
fn multiple_findings_report_each_line() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        "name: pins\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n      - uses: actions/cache@v3\n      - uses: dtolnay/rust-toolchain@stable\n",
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "the file must fail: {output}");
    assert!(
        output.contains("FINDINGS: 3"),
        "all three references must be counted: {output}"
    );
    for line in [7, 8, 9] {
        assert!(
            output.contains(&format!(".github/workflows/pins.yml:{line}:")),
            "line {line} must be reported: {output}"
        );
    }
}

/// The shipped repository's own workflows pass the checker, and the pinned
/// SHAs used above are the ones the repository really references.
#[test]
fn repository_workflows_pass() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (code, output) = run(&root);
    assert_eq!(code, 0, "the repository workflows must pass: {output}");
    assert!(
        output.contains("FINDINGS: 0"),
        "the repository must have no findings: {output}"
    );

    let workflows = root.join(".github/workflows");
    let verify = std::fs::read_to_string(workflows.join("verify.yml")).expect("read verify.yml");
    assert!(
        verify.contains(&format!("actions/checkout@{CHECKOUT_SHA}")),
        "the fixture SHA must be the one this repository uses"
    );
    assert!(
        verify.contains(&format!("dtolnay/rust-toolchain@{TOOLCHAIN_SHA}")),
        "the fixture SHA must be the one this repository uses"
    );
    assert!(
        verify.contains(&format!("Swatinem/rust-cache@{RUST_CACHE_SHA}")),
        "the fixture SHA must be the one this repository uses"
    );
}

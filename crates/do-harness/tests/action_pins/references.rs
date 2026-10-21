//! Reference-form cases for the `action-pins` sensor.
//!
//! Each case runs the real checker against a temporary repository and
//! asserts the exit code and the reported finding, because the policy is
//! about what the workflow file says, not about a grammar in isolation.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::fixtures::{CHECKOUT_SHA, run, workflow_with, workspace, write_workflow};
const MULTIPLE_FINDING_LINES: [usize; 3] = [7, 8, 9];

/// A pinned action passes and leaves the findings count at zero.
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

/// An action pinned in a sub-directory of its repository passes: the
/// sub-path is part of the reference, and the commit is still the pin.
#[test]
fn pinned_action_subpath_passes() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/subpath.yml",
        &format!(
            "name: subpath\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: owner/repo/.github/actions/example@{CHECKOUT_SHA}\n"
        ),
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 0, "a pinned sub-path action must pass: {output}");
}

/// A job-level `uses` that is not a string is a finding, like a step's.
#[test]
fn non_string_job_level_uses_fails() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/caller.yml",
        "name: caller\non: push\njobs:\n  call:\n    uses:\n      repo: owner/name\n",
    );

    let (code, output) = run(dir.path());
    assert_eq!(
        code, 1,
        "a non-string job-level reference must fail: {output}"
    );
    assert!(
        output.contains("not a scalar string"),
        "the finding must name the non-scalar case: {output}"
    );
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

/// An uppercase hexadecimal SHA is still a full commit SHA: the policy is
/// about the length and the alphabet, not about the case.
#[test]
fn uppercase_sha_passes() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        &workflow_with(&format!("actions/checkout@{}", CHECKOUT_SHA.to_uppercase())),
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 0, "an uppercase full SHA must pass: {output}");
}

/// A reference with surrounding whitespace is malformed, not a valid SHA.
/// The value is quoted, because a plain YAML scalar strips the padding
/// before the checker ever sees it.
#[test]
fn whitespace_reference_fails() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        &format!(
            "name: pins\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: 'actions/checkout@{CHECKOUT_SHA} '\n"
        ),
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "a padded reference must fail: {output}");
    assert!(
        output.contains("whitespace"),
        "the finding must name the whitespace: {output}"
    );
}

/// A scheme other than `docker://` is rejected rather than guessed at.
#[test]
fn unsupported_scheme_fails() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        &workflow_with(
            "oci://alpine@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        ),
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "an unsupported scheme must fail: {output}");
    assert!(
        output.contains("scheme"),
        "the finding must name the scheme: {output}"
    );
}

/// A local reference with nothing after `./` names no action at all.
#[test]
fn empty_local_reference_fails() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        &workflow_with("./"),
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "an empty local reference must fail: {output}");
    assert!(
        output.contains("empty"),
        "the finding must name the empty reference: {output}"
    );
}

/// A reference with no repository part is not an action reference. The value
/// is quoted, because a plain YAML scalar cannot start with `@`.
#[test]
fn repository_less_reference_fails() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        &format!(
            "name: pins\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: '@{CHECKOUT_SHA}'\n"
        ),
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "a repository-less reference must fail: {output}");
    assert!(
        output.contains("unsupported action reference form"),
        "the finding must name the missing repository: {output}"
    );
}

/// Empty and traversal path components are not valid external references.
#[test]
fn malformed_repository_path_fails() {
    for repository in [
        "owner/repo/",
        "owner/repo//action",
        "owner/../repo",
        "owner/repo/../action",
    ] {
        let dir = workspace();
        let reference = format!("{repository}@{CHECKOUT_SHA}");
        write_workflow(
            dir.path(),
            ".github/workflows/pins.yml",
            &workflow_with(&reference),
        );

        let (code, output) = run(dir.path());
        assert_eq!(
            code, 1,
            "a malformed repository path must fail ({repository}): {output}"
        );
        assert!(
            output.contains("malformed action repository"),
            "the finding must name the malformed repository path: {output}"
        );
    }
}

/// An empty version (`owner/repo@`) names no commit at all.
#[test]
fn empty_version_fails() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        &workflow_with("actions/checkout@"),
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "an empty version must fail: {output}");
    assert!(
        output.contains("empty version"),
        "the finding must name the empty version: {output}"
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
    for line in MULTIPLE_FINDING_LINES {
        assert!(
            output.contains(&format!(".github/workflows/pins.yml:{line}:")),
            "line {line} must be reported: {output}"
        );
    }
}

/// The shipped workflow inventory passes the real checker.
#[test]
fn repository_workflows_pass() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (code, output) = run(&root);
    assert_eq!(code, 0, "the repository workflows must pass: {output}");
    assert!(
        output.contains("FINDINGS: 0"),
        "the repository must have no findings: {output}"
    );
}

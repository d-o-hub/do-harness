//! Structure and discovery cases for the `action-pins` sensor.
//!
//! These are the cases where a reference is hidden rather than mutable: a
//! quoted or flow-mapped value, an alias, a duplicate key, a merge key, a
//! cycle, and the discovery boundaries that decide which files are read at
//! all.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::fixtures::{CHECKOUT_SHA, git_workspace, run, workflow_with, workspace, write_workflow};

/// A non-string `uses` value is a finding, never a silent skip.
#[test]
fn non_string_reference_fails() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        "name: pins\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses:\n          key: value\n",
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "a non-string reference must fail: {output}");
    assert!(
        output.contains("not a scalar string"),
        "the finding must name the non-scalar case: {output}"
    );
}

/// Malformed YAML is a finding, not a crash and not a skip.
#[test]
fn malformed_yaml_fails() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        "name: pins\njobs:\n  build:\n   - uses: [\n",
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "malformed YAML must fail: {output}");
    assert!(
        output.contains("not valid YAML"),
        "the finding must name the parse failure: {output}"
    );
}

/// A quoted value and a flow mapping hide nothing.
#[test]
fn quoted_and_flow_forms_are_checked() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        "name: pins\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: 'actions/checkout@v4'\n      - { uses: \"actions/cache@v3\" }\n",
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "quoted and flow forms must fail: {output}");
    assert!(
        output.contains("FINDINGS: 2"),
        "both quoted references must be reported: {output}"
    );
}

/// A block scalar is a non-string reference, not a silently accepted one.
#[test]
fn block_scalar_reference_fails() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        "name: pins\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: |\n          actions/checkout@v4\n",
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "a block scalar must fail: {output}");
    assert!(
        output.contains("FINDINGS: 1"),
        "the block scalar must be reported: {output}"
    );
}

/// An alias that resolves to a pinned reference is accepted; an alias that
/// resolves to a tag is rejected, at the use site.
#[test]
fn aliases_resolve_to_the_anchored_value() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        &format!(
            "name: pins\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: &pinned actions/checkout@{CHECKOUT_SHA}\n      - uses: *pinned\n"
        ),
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 0, "a resolved pinned alias must pass: {output}");

    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        "name: pins\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: &tagged actions/checkout@v4\n      - uses: *tagged\n",
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "a resolved tagged alias must fail: {output}");
    assert!(
        output.contains(".github/workflows/pins.yml:8:"),
        "the alias finding must point at the use site: {output}"
    );
    assert!(
        output.contains("FINDINGS: 2"),
        "the anchor and the alias are both references to the same mutable tag: {output}"
    );
}

/// A duplicate mapping key is a finding: the last value wins silently
/// otherwise, which is exactly how a pinned value disappears.
#[test]
fn duplicate_keys_fail() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        &format!(
            "name: pins\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@{CHECKOUT_SHA}\n        uses: actions/checkout@v4\n"
        ),
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "a duplicate key must fail: {output}");
    assert!(
        output.contains("duplicate mapping key 'uses'"),
        "the duplicate key must be reported: {output}"
    );
    assert!(
        output.contains("FINDINGS: 2"),
        "the duplicate key and the shadowing tag are both findings: {output}"
    );
}

/// A merge key is a finding: the merged mapping can carry a reference the
/// checker would otherwise never see.
#[test]
fn merge_keys_fail() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        &format!(
            "x-anchor: &anchor\n  uses: actions/checkout@v4\nname: pins\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - <<: *anchor\n        uses: actions/checkout@{CHECKOUT_SHA}\n"
        ),
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "a merge key must fail: {output}");
    assert!(
        output.contains("merge key is not allowed"),
        "the merge key must be reported: {output}"
    );
}

/// A cyclic alias inside the inspected region is a finding: the reference
/// cannot be resolved, so the checker must not treat it as compliant.
#[test]
fn cyclic_alias_fails() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        "name: pins\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - &step\n        uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1\n        self: *step\n",
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "a cyclic alias must fail: {output}");
    assert!(
        output.contains("cyclic YAML anchor or alias"),
        "the finding must name the cycle: {output}"
    );
}

/// A `uses` value carrying a custom tag is not a plain string reference.
#[test]
fn unknown_tag_reference_fails() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        "name: pins\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: !shortcut actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1\n",
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "an unknown tag must fail: {output}");
    assert!(
        output.contains("unsupported tag"),
        "the finding must name the tag: {output}"
    );
}

/// A `uses:` inside a shell script or a comment is not an action reference
/// and must not be reported.
#[test]
fn script_text_and_comments_are_ignored() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        &format!(
            "name: pins\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - name: mention\n        run: |\n          echo 'uses: actions/checkout@v4'\n          # uses: owner/repo@v1\n      - uses: actions/checkout@{CHECKOUT_SHA}\n"
        ),
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 0, "script text must not be inspected: {output}");
    assert!(
        output.contains("FINDINGS: 0"),
        "no finding may come from script text: {output}"
    );
}

/// Nested workflow directories are discovered, not only the top level.
#[test]
fn nested_workflows_are_discovered() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/nested/pins.yaml",
        &workflow_with("actions/checkout@v4"),
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "a nested workflow must be checked: {output}");
    assert!(
        output.contains(".github/workflows/nested/pins.yaml:7:"),
        "the nested finding must name its file: {output}"
    );
}

/// A `.yaml` extension is discovered as well as `.yml`.
#[test]
fn yaml_extension_is_discovered() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yaml",
        &workflow_with("actions/checkout@v4"),
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "a .yaml workflow must be checked: {output}");
}

/// An unstaged new workflow is discovered: the file is not in the index yet.
#[test]
fn unstaged_workflow_is_discovered() {
    let dir = git_workspace();
    let root = dir.path();
    write_workflow(
        root,
        ".github/workflows/new.yml",
        &workflow_with("actions/checkout@v4"),
    );

    let (code, output) = run(root);
    assert_eq!(code, 1, "an unstaged workflow must be checked: {output}");
    assert!(
        output.contains(".github/workflows/new.yml:7:"),
        "the finding must name the new file: {output}"
    );
}

/// A deleted workflow is not checked: the file is gone.
#[test]
fn deleted_workflow_is_not_checked() {
    let dir = git_workspace();
    let root = dir.path();
    write_workflow(
        root,
        ".github/workflows/gone.yml",
        &workflow_with("actions/checkout@v4"),
    );
    std::fs::remove_file(root.join(".github/workflows/gone.yml")).expect("remove workflow");

    let (code, output) = run(root);
    assert_ne!(code, 0, "a repository with no workflow must fail: {output}");
    assert!(
        output.contains("no workflow files found"),
        "the failure must name the empty discovery: {output}"
    );
}

/// A discovery command that fails fails the check closed.
#[test]
fn discovery_failure_fails_closed() {
    let dir = workspace();
    let root = dir.path();
    // A `.git` directory that is not a repository: `git ls-files` fails, and a
    // checker that silently checked nothing would report a vacuous pass.
    std::fs::create_dir_all(root.join(".git")).expect("create .git");
    write_workflow(
        root,
        ".github/workflows/pins.yml",
        &workflow_with("actions/checkout@v4"),
    );

    let (code, output) = run(root);
    assert_eq!(code, 1, "a discovery failure must fail closed: {output}");
    assert!(
        output.contains("git discovery failed"),
        "the failure must name the discovery error: {output}"
    );
}

/// A repository without any workflow file fails: a gate that checks nothing
/// is not a pass.
#[test]
fn zero_workflow_files_fail() {
    let dir = workspace();

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "an empty inventory must fail: {output}");
    assert!(
        output.contains("no workflow files found"),
        "the failure must name the empty inventory: {output}"
    );
}

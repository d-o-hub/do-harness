//! Structure cases for the `action-pins` sensor.
//!
//! These are the cases where a reference is hidden rather than mutable: a
//! quoted or flow-mapped value, an alias, a duplicate key, a merge key, a
//! cycle, and the keys nested where they are inputs rather than references.
//! The workflow shapes that declare no reference live in `shape`, and the
//! discovery boundaries live in `discovery`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fmt::Write as _;

use crate::fixtures::{CHECKOUT_SHA, run, workspace, write_workflow};

/// A mapping-valued step reference is a finding, never a silent skip.
#[test]
fn mapping_step_reference_fails() {
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

/// A node aliased in two different places is shared, not cyclic: the checker
/// must not read a plain reuse as an unresolvable reference.
#[test]
fn shared_alias_is_not_a_cycle() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        &format!(
            "name: pins\non: push\njobs:\n  first:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: &pinned actions/checkout@{CHECKOUT_SHA}\n  second:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: *pinned\n"
        ),
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 0, "a shared alias must pass: {output}");
    assert!(
        !output.contains("cyclic"),
        "a shared alias must not be reported as a cycle: {output}"
    );
}

/// A repeated alias DAG is checked once per node, not expanded exponentially.
#[test]
fn shared_alias_dag_is_checked_once() {
    const ALIAS_LEVELS: usize = 24;

    let dir = workspace();
    let mut workflow = format!(
        "name: pins\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@{CHECKOUT_SHA}\n"
    );
    workflow.push_str("alias_0: &alias_0 [leaf]\n");
    for level in 1..ALIAS_LEVELS {
        let previous = level - 1;
        let _ = writeln!(
            workflow,
            "alias_{level}: &alias_{level} [*alias_{previous}, *alias_{previous}]"
        );
    }
    write_workflow(dir.path(), ".github/workflows/pins.yml", &workflow);

    let (code, output) = run(dir.path());
    assert_eq!(code, 0, "a shared alias DAG must pass: {output}");
    assert!(
        output.contains("FINDINGS: 0"),
        "a shared alias DAG must not be mistaken for a cycle: {output}"
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

/// A numeric `uses` value is a number, not a reference string.
#[test]
fn non_string_tag_reference_fails() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        "name: pins\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: 12345\n",
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "an int reference must fail: {output}");
    assert!(
        output.contains("not a scalar string"),
        "the finding must name the non-string case: {output}"
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

/// A `uses` key nested inside a step's `with:` payload is an input, not an
/// action reference: only the step's own `uses` is inspected.
#[test]
fn nested_uses_key_is_not_inspected() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        &format!(
            "name: pins\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@{CHECKOUT_SHA}\n        with:\n          uses: actions/checkout@v4\n"
        ),
    );

    let (code, output) = run(dir.path());
    assert_eq!(
        code, 0,
        "a nested `uses` key must not be inspected: {output}"
    );
    assert!(
        output.contains("FINDINGS: 0"),
        "no finding may come from a nested key: {output}"
    );
}

/// A `uses` key under a job's other fields is not an action reference either:
/// only `jobs.<id>.uses` and `jobs.<id>.steps[*].uses` are inspected.
#[test]
fn job_level_nested_uses_key_is_not_inspected() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        &format!(
            "name: pins\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    with:\n      uses: actions/checkout@v4\n    steps:\n      - uses: actions/checkout@{CHECKOUT_SHA}\n"
        ),
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 0, "a nested job key must not be inspected: {output}");
    assert!(
        output.contains("FINDINGS: 0"),
        "no finding may come from a nested job key: {output}"
    );
}

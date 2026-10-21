//! Workflow-shape cases for the `action-pins` sensor.
//!
//! A workflow that declares no action reference at all proves nothing: the
//! checker would have nothing to inspect and would report a clean file. The
//! cases here pin the shapes that declare nothing — a second document, a
//! missing `jobs`, an empty job — as findings rather than as a vacuous pass.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::fixtures::{CHECKOUT_SHA, run, workspace, write_workflow};

/// A file with two YAML documents is a finding: the checker must not read
/// only the first one and call the file clean.
#[test]
fn multiple_documents_fail() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        &format!(
            "name: pins\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@{CHECKOUT_SHA}\n---\nname: second\n"
        ),
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "a second document must fail: {output}");
    assert!(
        output.contains("exactly one YAML document"),
        "the finding must name the document count: {output}"
    );
}

/// A file whose single document is not a mapping is a finding.
#[test]
fn non_mapping_document_fails() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        "- not\n- a\n- mapping\n",
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "a non-mapping document must fail: {output}");
    assert!(
        output.contains("not a YAML mapping"),
        "the finding must name the document shape: {output}"
    );
}

/// An empty workflow file is a finding: it declares nothing to check.
#[test]
fn empty_file_fails() {
    let dir = workspace();
    write_workflow(dir.path(), ".github/workflows/pins.yml", "");

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "an empty workflow must fail: {output}");
    assert!(
        output.contains("exactly one YAML document"),
        "the finding must name the empty document: {output}"
    );
}

/// A workflow without a `jobs` mapping declares no action reference at all,
/// so the checker would have nothing to prove.
#[test]
fn workflow_without_jobs_fails() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        "name: pins\non: push\n",
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "a workflow without jobs must fail: {output}");
    assert!(
        output.contains("no `jobs` mapping"),
        "the finding must name the missing jobs: {output}"
    );
}

/// An empty `jobs` mapping is the same vacuity as a missing one.
#[test]
fn empty_jobs_fails() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        "name: pins\non: push\njobs: {}\n",
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "empty jobs must fail: {output}");
    assert!(
        output.contains("`jobs` is empty"),
        "the finding must name the empty jobs: {output}"
    );
}

/// A job with neither `uses` nor `steps` carries no action reference: the
/// checker has nothing to prove about it.
#[test]
fn job_without_steps_or_uses_fails() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        "name: pins\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n",
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "a job without steps must fail: {output}");
    assert!(
        output.contains("neither `uses` nor `steps`"),
        "the finding must name the vacuous job: {output}"
    );
}

/// An empty `steps` sequence is the same vacuity as a missing one.
#[test]
fn empty_steps_fails() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        "name: pins\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps: []\n",
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "empty steps must fail: {output}");
    assert!(
        output.contains("empty `steps`"),
        "the finding must name the empty steps: {output}"
    );
}

/// A `steps` field that is not a sequence hides whatever it holds.
#[test]
fn non_sequence_steps_fails() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        "name: pins\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      uses: actions/checkout@v4\n",
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "a non-sequence steps must fail: {output}");
    assert!(
        output.contains("non-sequence `steps`"),
        "the finding must name the steps shape: {output}"
    );
}

/// A `jobs` field that is not a mapping of job definitions is a finding.
#[test]
fn non_mapping_jobs_fails() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        "name: pins\non: push\njobs:\n  - uses: actions/checkout@v4\n",
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "a non-mapping jobs must fail: {output}");
    assert!(
        output.contains("`jobs` is not a mapping"),
        "the finding must name the jobs shape: {output}"
    );
}

/// A reference carrying a newline must not break the diagnostic format: the
/// finding stays on one line, with the newline escaped.
#[test]
fn newline_in_reference_stays_on_one_line() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        "name: pins\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: \"actions/checkout@v4\\nsecond\\u2028third\"\n",
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "a newline in a reference must fail: {output}");
    let finding = output
        .lines()
        .find(|line| line.contains("action reference"))
        .unwrap_or_else(|| panic!("the finding must be reported: {output}"));
    assert!(
        finding.starts_with(".github/workflows/pins.yml:7:"),
        "the finding must stay on one line: {finding}"
    );
    assert!(
        finding.contains("\\n"),
        "the newline must be escaped in the diagnostic: {finding}"
    );
    assert!(
        finding.contains("\\u2028"),
        "the Unicode line separator must be escaped: {finding}"
    );
}

/// A parse failure names the line the parser broke on, not always line 1:
/// the mark is where the file stops making sense.
#[test]
fn parse_error_names_the_broken_line() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yml",
        "name: pins\non: push\njobs:\n  build:\n   - uses: [\n",
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "malformed YAML must fail: {output}");
    assert!(
        output.contains("not valid YAML"),
        "the finding must name the parse failure: {output}"
    );
    // The unterminated flow sequence is reported at the end of the stream,
    // not at line 1: the diagnostic points at where the file broke.
    let line = output
        .lines()
        .find(|line| line.contains("not valid YAML"))
        .unwrap_or_else(|| panic!("the finding must be reported: {output}"));
    let reported: usize = line
        .split(':')
        .nth(1)
        .and_then(|number| number.parse().ok())
        .unwrap_or_else(|| panic!("the finding must carry a line number: {line}"));
    assert_eq!(
        reported, 6,
        "the parser must report the end-of-stream line, not line 1: {line}"
    );
}

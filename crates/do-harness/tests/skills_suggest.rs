//! Integration tests for `do-harness skills suggest`.
//!
//! Each test drives the real binary with `--root <tmp>` against a fixture
//! catalog it builds itself, so the command's JSON schema, exit codes, and
//! offline behaviour are exercised end to end without touching the repository's
//! own skill tree.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;

/// Runs `do-harness --root <root> skills suggest ...`.
fn suggest(root: &Path, args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    cmd.arg("--root").arg(root).arg("skills").arg("suggest");
    for arg in args {
        cmd.arg(arg);
    }
    // Inherited selector configuration would make the fixture nondeterministic.
    cmd.env_remove("DO_HARNESS_SKILL_SELECTOR");
    cmd.output().expect("spawn do-harness skills suggest")
}

/// Writes one fixture skill with `name`/`description` frontmatter.
fn write_skill(root: &Path, dir: &str, name: &str, description: &str) {
    let path = root.join(".agents/skills").join(dir);
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(
        path.join("SKILL.md"),
        format!(
            "---\nname: {name}\ndescription: {description}\nlicense: MIT\n---\n\n# {name}\n\nBody prose that must stay out of the suggestion payload.\n"
        ),
    )
    .unwrap();
}

/// Parses stdout as JSON, panicking with stderr on failure.
fn json(output: &Output) -> Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    serde_json::from_str(&stdout)
        .unwrap_or_else(|_| panic!("stdout is not JSON:\n{stdout}\nstderr:\n{stderr}"))
}

#[test]
fn suggest_reports_the_documented_schema_offline() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_skill(
        root,
        "pr-triage",
        "pr-triage",
        "Triage GitHub pull requests and resolve CI failures.",
    );
    write_skill(
        root,
        "wheel-builder",
        "wheel-builder",
        "Build binary wheels for python packaging.",
    );

    let output = suggest(
        root,
        &[
            "--query",
            "resolve CI failures on a pull request",
            "--limit",
            "3",
            "--format",
            "json",
        ],
    );
    assert_eq!(output.status.code(), Some(0));
    let report = json(&output);

    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["catalog_size"], 2);
    assert_eq!(report["query_terms"], 6);
    assert_eq!(report["warnings"].as_array().unwrap().len(), 0);

    let candidates = report["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0]["name"], "pr-triage");
    assert_eq!(candidates[0]["path"], ".agents/skills/pr-triage/SKILL.md");
    assert!(candidates[0]["score"].as_f64().unwrap() > 0.0);

    // Metadata only: no candidate may carry skill body prose.
    let raw = String::from_utf8_lossy(&output.stdout);
    assert!(!raw.contains("Body prose"), "{raw}");
}

#[test]
fn suggest_is_stable_across_repeated_runs() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_skill(root, "alpha", "alpha", "Alpha review workflow.");
    write_skill(root, "beta", "beta", "Beta review workflow.");
    let args = [
        "--query",
        "review workflow",
        "--limit",
        "5",
        "--format",
        "json",
    ];

    let first = suggest(root, &args).stdout;
    let second = suggest(root, &args).stdout;
    assert_eq!(
        String::from_utf8_lossy(&first),
        String::from_utf8_lossy(&second)
    );
}

#[test]
fn suggest_without_a_skill_root_exits_zero_with_no_candidates() {
    let temp = tempfile::tempdir().unwrap();
    let output = suggest(
        temp.path(),
        &["--query", "anything", "--limit", "3", "--format", "json"],
    );
    assert_eq!(output.status.code(), Some(0));
    let report = json(&output);
    assert_eq!(report["catalog_size"], 0);
    assert!(report["candidates"].as_array().unwrap().is_empty());
}

#[test]
fn suggest_rejects_a_zero_limit_as_a_usage_error() {
    let temp = tempfile::tempdir().unwrap();
    let output = suggest(temp.path(), &["--query", "anything", "--limit", "0"]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn suggest_reports_malformed_skills_as_warnings() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_skill(root, "good", "good", "A usable skill.");
    let broken = root.join(".agents/skills/broken");
    std::fs::create_dir_all(&broken).unwrap();
    std::fs::write(broken.join("SKILL.md"), "---\nname: [unclosed\n---\n").unwrap();

    let output = suggest(
        root,
        &["--query", "usable", "--limit", "5", "--format", "json"],
    );
    assert_eq!(output.status.code(), Some(0));
    let report = json(&output);
    assert_eq!(report["catalog_size"], 1);
    let warnings = report["warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].as_str().unwrap().contains("malformed"));
}

#[test]
fn suggest_text_output_names_one_candidate_per_line() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_skill(root, "solo", "solo", "The only skill in this catalog.");
    let output = suggest(root, &["--query", "only skill", "--limit", "5"]);
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.lines().count(), 1, "{stdout}");
    assert!(stdout.contains("solo"), "{stdout}");
}

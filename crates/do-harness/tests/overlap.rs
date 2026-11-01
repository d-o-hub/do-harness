//! End-to-end coverage for `do-harness overlap`.
//!
//! `overlap` is the Tier-2 distinctiveness advisory: it ranks every pair of
//! skills under `.agents/skills` by cosine similarity over `SKILL.md` plus
//! `references/**/*.md`, flags pairs at or above `--threshold`, and always
//! exits `0` (advisory) unless the threshold is invalid. No test file exercised
//! the command before; these drive the real binary against a scratch corpus.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use tempfile::{TempDir, tempdir};

mod support;

use support::harness;

/// Default `--threshold`, mirroring `overlap::DEFAULT_THRESHOLD`.
const DEFAULT_THRESHOLD: f64 = 0.45;

/// Number of unordered pairs a three-skill corpus produces (`3 choose 2`).
const THREE_SKILL_PAIRS: usize = 3;

/// The same count as it appears in the JSON report (`flagged`).
const THREE_SKILL_PAIRS_JSON: i64 = 3;
/// The advisory exits `0` whenever the corpus is readable and the threshold
/// is valid, whether or not any pair is flagged.
const OK_EXIT: i32 = 0;
/// An out-of-range `--threshold` is a usage error.
const USAGE_EXIT: i32 = 2;

/// Two skills sharing nearly every meaningful token.
const ALPHA: &str = "# Alpha\nSpike scratchpad hypothesis verification cleanup transition.";
const BETA: &str = "# Beta\nSpike scratchpad hypothesis verification cleanup transition extra.";
/// A third skill whose prose is disjoint from both above.
const GAMMA: &str = "# Gamma\nUnrelated haiku prose ocean waves.";

/// Runs a harness command, returning (exit code, `stdout`, `stderr`).
fn run(cmd: &mut Command) -> (Option<i32>, String, String) {
    let out = cmd.output().expect("spawn do-harness");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Writes `.agents/skills/<name>/SKILL.md` under `root`.
fn write_skill(root: &Path, name: &str, body: &str) {
    let dir = root.join(".agents/skills").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("SKILL.md"), body).unwrap();
}

/// A scratch corpus of three skills: one near-duplicate pair plus a distinct
/// one, which yields exactly three pairs.
fn corpus() -> (TempDir, PathBuf) {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    write_skill(&root, "alpha", ALPHA);
    write_skill(&root, "beta", BETA);
    write_skill(&root, "gamma", GAMMA);
    (dir, root)
}

fn overlap(root: &Path, extra: &[&str]) -> (Option<i32>, String, String) {
    run(harness(root).arg("overlap").args(extra))
}

/// Parses the single JSON object the `--format json` report prints.
fn report(stdout: &str) -> Value {
    serde_json::from_str(stdout).unwrap_or_else(|err| {
        panic!("overlap --format json must print one JSON object; {err}; stdout: {stdout}")
    })
}

#[test]
fn near_duplicate_pair_is_flagged_and_distinct_pairs_are_not() {
    let (_dir, root) = corpus();

    // Text: the near-duplicate ranks first and is the only flagged pair.
    let (code, stdout, stderr) = overlap(&root, &[]);
    assert_eq!(
        code,
        Some(OK_EXIT),
        "the advisory must exit 0:\n{stdout}\n{stderr}"
    );
    assert!(stdout.contains("alpha <-> beta"), "stdout: {stdout}");
    assert!(
        stdout.contains("WARN: 1 pair(s) at or above threshold 0.45"),
        "stdout: {stdout}"
    );

    // JSON: the report names the threshold and flagged count and carries each
    // pair's similarity and top shared terms.
    let (code, stdout, stderr) = overlap(&root, &["--format", "json"]);
    assert_eq!(code, Some(OK_EXIT), "stdout: {stdout}\nstderr: {stderr}");
    let parsed = report(&stdout);
    assert_eq!(parsed["threshold"], DEFAULT_THRESHOLD);
    assert_eq!(parsed["flagged"], 1);
    let pairs = parsed["pairs"].as_array().expect("pairs array");
    assert_eq!(
        pairs.len(),
        THREE_SKILL_PAIRS,
        "three skills give three pairs"
    );
    assert_eq!(pairs[0]["a"], "alpha");
    assert_eq!(pairs[0]["b"], "beta");
    assert!(
        pairs[0]["similarity"].as_f64().unwrap() >= DEFAULT_THRESHOLD,
        "the near-duplicate must clear the threshold: {}",
        pairs[0]["similarity"]
    );
    let shared = pairs[0]["shared"].as_array().expect("shared array");
    assert!(
        shared.iter().any(|term| term == "spike"),
        "shared terms must surface the common vocabulary: {shared:?}"
    );
    // The pairs involving the disjoint skill sit below the threshold.
    for pair in &pairs[1..] {
        assert!(
            pair["similarity"].as_f64().unwrap() < DEFAULT_THRESHOLD,
            "a distinct pair must not be flagged: {pair}"
        );
    }
}

#[test]
fn threshold_selects_which_pairs_are_flagged() {
    let (_dir, root) = corpus();

    // Above every similarity: nothing flagged, and the text says so.
    let (code, stdout, _) = overlap(&root, &["--threshold", "0.99"]);
    assert_eq!(code, Some(OK_EXIT));
    assert!(
        stdout.contains("OK: no pair at or above threshold 0.99"),
        "stdout: {stdout}"
    );

    // At the floor even disjoint pairs (similarity 0.0) count, so all three
    // pairs flag.
    let (code, stdout, _) = overlap(&root, &["--threshold", "0.0", "--format", "json"]);
    assert_eq!(code, Some(OK_EXIT));
    assert_eq!(report(&stdout)["flagged"], THREE_SKILL_PAIRS_JSON);
}

#[test]
fn invalid_threshold_is_a_usage_error() {
    let (_dir, root) = corpus();

    let (code, stdout, stderr) = overlap(&root, &["--threshold", "1.5"]);
    assert_eq!(
        code,
        Some(USAGE_EXIT),
        "an out-of-range threshold must exit 2:\n{stdout}\n{stderr}"
    );
    assert!(
        stderr.contains("invalid --threshold 1.5"),
        "stderr must name the rejected threshold: {stderr}"
    );
}

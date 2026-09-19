//! Regression gate for the semantic-routing benchmark.
//!
//! Runs `scripts/pr-routing-benchmark.sh --fixtures <corpus>` against both
//! checked-in seeded corpora and asserts the properties #114 makes
//! release-blocking: the benchmark completes, no high-impact class is
//! downgraded, the aggregate downgrade count is zero, and the adversarially
//! misleading provider still routes a contract change to `deep`.
//!
//! Two corpora, one safety contract:
//!   - `tests/fixtures/pr-routing` — 12 minimal cases, pinning the route
//!     *decision*.
//!   - `tests/fixtures/pr-routing-large` — the same 12 classes at realistic
//!     diff sizes (2.8–25 KB), pinning that the decision holds, and that the
//!     route-aware cost columns stay internally consistent, as diffs grow.
//!
//! The fixture provider is deterministic, so a failure here means the routing
//! policy changed, not that a model answered differently.

// POSIX-only. The benchmark materializes its corpus as git repositories and
// drives them through `scripts/pr-routing-benchmark.sh`, whose provider is a
// `#!` script the router wrapper spawns directly. Windows cannot execute that
// (the windows job reports `os error 193`, "%1 is not a valid Win32
// application"), and the script additionally requires `jq`. Gated at the file
// level rather than per test: every case here needs the same surface, and the
// unix CI jobs run all of them.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

/// Repository root, derived from this crate's manifest directory.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repository root")
}

/// Runs the benchmark against the minimal seeded corpus.
///
/// The hook-inherited git environment is scrubbed: git exports `GIT_DIR` to
/// hooks, and an inherited value would make the benchmark's fixture `git init`
/// target the outer repository.
fn run_benchmark() -> Output {
    run_benchmark_at(&repo_root().join("tests/fixtures/pr-routing"))
}

/// Runs the benchmark against an explicit corpus directory.
///
/// The provider is pinned to the corpus's own `fake-router.sh`, because the
/// benchmark resolves `PR_TRIAGE_ROUTER` relative to `--fixtures`; pinning the
/// sibling corpus's provider would silently measure the wrong router.
fn run_benchmark_at(fixtures: &Path) -> Output {
    let root = repo_root();
    let fixtures = fixtures
        .canonicalize()
        .expect("canonical fixture corpus directory");
    let mut cmd = Command::new("/bin/bash");
    cmd.arg(root.join("scripts/pr-routing-benchmark.sh"));
    cmd.arg("--fixtures").arg(&fixtures);
    cmd.env("DO_HARNESS_BIN", env!("CARGO_BIN_EXE_do-harness"));
    cmd.env("PR_TRIAGE_ROUTER", fixtures.join("fake-router.sh"));
    cmd.env("PR_TRIAGE_ROUTER_TIMEOUT", "5");
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CEILING_DIRECTORIES",
        "GIT_NAMESPACE",
        "GIT_PREFIX",
    ] {
        cmd.env_remove(key);
    }
    cmd.output().expect("spawn pr-routing-benchmark.sh")
}

/// Runs the benchmark against the realistic large-diff corpus.
fn run_large_benchmark() -> Output {
    run_benchmark_at(&repo_root().join("tests/fixtures/pr-routing-large"))
}

/// Parses the per-case JSON rows from benchmark stdout.
fn rows(stdout: &str) -> Vec<Value> {
    stdout
        .lines()
        .filter(|line| line.starts_with('{'))
        .map(|line| serde_json::from_str(line).expect("benchmark row is JSON"))
        .collect()
}

/// Reads `key=<value>` from the stderr summary, scanning every line.
fn summary_value(stderr: &str, key: &str) -> Option<String> {
    let needle = format!("{key}=");
    for line in stderr.lines() {
        // Summary lines pack several `key=value` pairs, so scan tokens rather
        // than assuming the key starts the line.
        for token in line.split_whitespace() {
            if let Some(rest) = token.strip_prefix(&needle) {
                return Some(rest.to_owned());
            }
        }
    }
    None
}

#[test]
fn benchmark_runs_and_never_downgrades_a_high_impact_class() {
    let output = run_benchmark();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(
        output.status.code(),
        Some(0),
        "benchmark must pass:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let rows = rows(&stdout);
    assert_eq!(
        rows.len(),
        12,
        "every corpus case must be measured:\n{stdout}"
    );

    // Explicit route ordering, checked per row so a policy change names the
    // case it broke rather than only moving an aggregate.
    let rank = |route: &str| match route {
        "cheap" => 0,
        "focused" => 1,
        "deep" => 2,
        other => panic!("unknown route {other}"),
    };
    for row in &rows {
        let case = row["case"].as_str().unwrap();
        let route = row["route"].as_str().unwrap();
        let expected = row["expected_min_route"].as_str().unwrap();
        assert!(
            rank(route) >= rank(expected),
            "{case}: routed {route} below the required {expected}"
        );
        assert_eq!(row["oracle_pass"], Value::Bool(true), "{case}");
        assert_eq!(row["schema_version"], 1, "{case}");
    }

    assert_eq!(summary_value(&stderr, "downgrades").as_deref(), Some("0"));
    assert_eq!(
        summary_value(&stderr, "seeded_oracle_failures").as_deref(),
        Some("0")
    );
    assert!(
        stderr.contains("FINDINGS: 0"),
        "benchmark must report no findings:\n{stderr}"
    );
}

#[test]
fn deep_classes_are_routed_deep_with_a_misleading_provider() {
    let output = run_benchmark();
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows = rows(&String::from_utf8_lossy(&output.stdout));

    for case in [
        "public-api-break",
        "security",
        "persistence-schema",
        "concurrency",
        "mixed-buried",
        "router-invalid",
    ] {
        let row = rows
            .iter()
            .find(|row| row["case"] == case)
            .unwrap_or_else(|| panic!("missing case {case}"));
        assert_eq!(
            row["route"], "deep",
            "{case} must stay deep; a weak or failed provider may not downgrade it"
        );
    }
}

#[test]
fn benchmark_output_is_stable_across_runs() {
    let first = run_benchmark();
    let second = run_benchmark();
    assert_eq!(first.status.code(), Some(0));
    assert_eq!(second.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&second.stdout),
        "the deterministic provider must make the benchmark byte-identical"
    );
}

#[test]
fn large_corpus_never_downgrades() {
    let output = run_large_benchmark();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Safety, not economy, is what gates here: the large corpus is ~2-25 KB per
    // case, so it is the realistic-diff arm of the same route oracle.
    assert_eq!(
        output.status.code(),
        Some(0),
        "large benchmark must pass:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let rows = rows(&stdout);
    assert_eq!(rows.len(), 12, "every corpus case must be measured");

    let rank = |route: &str| match route {
        "cheap" => 0,
        "focused" => 1,
        "deep" => 2,
        other => panic!("unknown route {other}"),
    };
    for row in &rows {
        let case = row["case"].as_str().unwrap();
        let route = row["route"].as_str().unwrap();
        let expected = row["expected_min_route"].as_str().unwrap();
        assert!(
            rank(route) >= rank(expected),
            "{case}: routed {route} below the required {expected}"
        );
        assert_eq!(row["oracle_pass"], Value::Bool(true), "{case}");
    }

    assert_eq!(summary_value(&stderr, "downgrades").as_deref(), Some("0"));
    assert_eq!(
        summary_value(&stderr, "seeded_oracle_failures").as_deref(),
        Some("0")
    );
    assert!(
        stderr.contains("FINDINGS: 0"),
        "large corpus must report no findings:\n{stderr}"
    );

    // The route-aware columns must exist and be internally consistent on every
    // row, so a corpus run is comparable to the recorded economy verdict.
    for row in &rows {
        let case = row["case"].as_str().unwrap();
        let route = row["route"].as_str().unwrap();
        let routed_review = row["routed_review_input_bytes"].as_u64().unwrap();
        let routed_total = row["routed_total_model_input_bytes"].as_u64().unwrap();
        let router_input = row["router_input_bytes"].as_u64().unwrap();
        let expected_review = match route {
            "cheap" => 0,
            "focused" => row["t_res_bytes"].as_u64().unwrap(),
            "deep" => row["t_raw_bytes"].as_u64().unwrap(),
            other => panic!("unknown route {other}"),
        };
        // A case with no effective change spends no review bytes on any route,
        // so only non-empty cases pin the route's payload exactly.
        if row["t_raw_bytes"].as_u64().unwrap() > 0 {
            assert_eq!(
                routed_review, expected_review,
                "{case}: the {route} route must read exactly {expected_review} review bytes"
            );
        }
        assert_eq!(
            routed_total,
            router_input + routed_review,
            "{case}: routed total must be the router envelope plus the selected payload"
        );
    }
}

#[test]
fn large_corpus_output_is_stable() {
    let first = run_large_benchmark();
    let second = run_large_benchmark();
    assert_eq!(first.status.code(), Some(0));
    assert_eq!(second.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&second.stdout),
        "the deterministic provider must make the large benchmark byte-identical"
    );
}

#[test]
fn benchmark_rejects_an_unusable_fixture_directory() {
    let temp = tempfile::tempdir().unwrap();
    let mut cmd = Command::new("/bin/bash");
    cmd.arg(repo_root().join("scripts/pr-routing-benchmark.sh"));
    cmd.arg("--fixtures").arg(temp.path());
    cmd.env("DO_HARNESS_BIN", env!("CARGO_BIN_EXE_do-harness"));
    let output = cmd.output().expect("spawn benchmark");
    // With no manifest the script falls back to historical replay, which finds
    // no commits in a temp directory; either way it must not silently succeed
    // while measuring nothing.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_ne!(
        output.status.code(),
        Some(0),
        "an empty fixture directory must not report success:\n{stderr}"
    );
    // The refusal must be the corpus guard, not an accidental failure: an
    // explicitly requested corpus may never degrade into history replay.
    assert!(
        stderr.contains("has no manifest.json"),
        "expected the explicit-corpus guard:\n{stderr}"
    );
}

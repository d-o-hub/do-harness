//! Dogfood for evidence-driven `do-harness init` (epic Issue 5).
//!
//! Each test drives the real binary against a real repository shape and
//! parses the machine-readable bootstrap report, proving that init detects
//! reality, includes only proven signals, and executes the generated
//! contract before claiming readiness.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;
mod support;

/// Builds a `do-harness --root <root>` command using the real binary.
fn harness(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    support::isolate_command(&mut cmd);
    cmd.arg("--root").arg(root);
    cmd
}

/// Runs `init --format json`, returning (exit code, parsed report, `stderr`).
fn init_json(root: &Path) -> (Option<i32>, Value, String) {
    let output = harness(root)
        .arg("init")
        .arg("--format")
        .arg("json")
        .output()
        .expect("spawn do-harness init");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let report = serde_json::from_str(&stdout)
        .unwrap_or_else(|_| panic!("init stdout is not JSON:\n{stdout}\n{stderr}"));
    (output.status.code(), report, stderr)
}

/// Writes a rustfmt-clean minimal crate into `root`.
///
/// `with_test` decides whether the crate ships a unit test. The two adoption
/// cases are deliberately separate fixtures: the pack's `test` sensor fails
/// closed when a run executes zero tests, so a test-less crate is not the same
/// contract as a crate that satisfies every generated signal.
fn write_clean_crate(root: &Path, with_test: bool) {
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    let lib = if with_test {
        "pub fn answer() -> u32 {\n    42\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn answer_is_stable() {\n        assert_eq!(super::answer(), 42);\n    }\n}\n"
    } else {
        "pub fn answer() -> u32 {\n    42\n}\n"
    };
    std::fs::write(root.join("src/lib.rs"), lib).unwrap();
}

/// An empty directory bootstraps Rust, runs the contract, and reports green.
#[test]
fn fresh_bootstrap_proves_generated_contract() {
    let dir = tempfile::tempdir().unwrap();
    let (code, report, stderr) = init_json(dir.path());
    assert_eq!(code, Some(0), "bootstrap must be green:\n{stderr}");
    assert_eq!(report["language"], serde_json::json!("rust"));
    assert_eq!(report["baseline"]["state"], serde_json::json!("green"));
    assert!(
        report["baseline"]["passed"].as_u64().unwrap_or(0) > 0,
        "baseline must actually run sensors: {report}"
    );
    // Every included candidate passed its probe and appears in the config.
    let config = std::fs::read_to_string(dir.path().join("do-harness.toml")).unwrap();
    for candidate in report["candidates"].as_array().expect("candidates") {
        if candidate["included"] == serde_json::json!(true) {
            let name = candidate["name"].as_str().expect("name");
            assert!(
                config.contains(&format!("name = \"{name}\"")),
                "included candidate {name} missing from config:\n{config}"
            );
        }
    }
}

/// An existing Rust repository is adopted without rewriting application source.
#[test]
fn existing_rust_repo_is_not_rewritten() {
    let dir = tempfile::tempdir().unwrap();
    write_clean_crate(dir.path(), true);
    let manifest_before = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    let lib_before = std::fs::read_to_string(dir.path().join("src/lib.rs")).unwrap();

    let (code, report, stderr) = init_json(dir.path());
    assert_eq!(
        code,
        Some(0),
        "existing clean crate must baseline green:\n{stderr}"
    );
    assert!(
        report["detected"]
            .as_array()
            .expect("detected")
            .iter()
            .any(|finding| finding == "Cargo.toml"),
        "detection must see the manifest: {report}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap(),
        manifest_before
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("src/lib.rs")).unwrap(),
        lib_before
    );
    assert!(dir.path().join("do-harness.toml").is_file());
}

/// An adopted repository with no tests gets a red baseline that names the empty
/// `test` gate, instead of a green bootstrap that verified nothing.
///
/// The pack runs `cargo nextest run --workspace` without `--no-tests=pass`
/// (AGENTS.md section 4), so a repository adopted before it has any test is
/// told to add one — or to drop the sensor from `do-harness.toml`, which the
/// run still writes. Sources stay untouched either way.
#[test]
fn existing_rust_repo_without_tests_reports_the_empty_test_gate() {
    let has_nextest = Command::new("cargo")
        .args(["nextest", "--version"])
        .output()
        .is_ok_and(|o| o.status.success());
    if !has_nextest {
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    write_clean_crate(dir.path(), false);
    let manifest_before = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    let lib_before = std::fs::read_to_string(dir.path().join("src/lib.rs")).unwrap();

    let (code, report, stderr) = init_json(dir.path());
    assert_eq!(
        code,
        Some(1),
        "an empty suite must report a red baseline:\n{stderr}\n{report}"
    );
    assert_eq!(report["baseline"]["state"], serde_json::json!("red"));
    let failed = report["baseline"]["failed"].as_array().expect("failed");
    assert!(
        failed.iter().any(|name| name == "test"),
        "the empty suite must be attributed to the test sensor: {failed:?}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap(),
        manifest_before
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("src/lib.rs")).unwrap(),
        lib_before
    );
    assert!(
        dir.path().join("do-harness.toml").is_file(),
        "adoption must leave the contract in place so the user can act on it"
    );
}

/// A failed candidate check is surfaced as a red baseline and non-zero exit.
#[test]
fn failing_repo_reports_red_and_exits_nonzero() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"broken\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(
        dir.path().join("src/lib.rs"),
        "pub fn broken() -> u32 {\n    \"not a number\"\n}\n",
    )
    .unwrap();

    let (code, report, _stderr) = init_json(dir.path());
    assert_eq!(code, Some(1), "red baseline must exit 1: {report}");
    assert_eq!(report["baseline"]["state"], serde_json::json!("red"));
    let failed = report["baseline"]["failed"].as_array().expect("failed");
    assert!(
        failed
            .iter()
            .any(|name| name == "check" || name == "clippy"),
        "compile failure must name the failing sensor: {failed:?}"
    );
}

/// A non-Rust project gets the generic pack and an explicitly vacuous baseline.
#[test]
fn non_rust_repo_is_explicitly_vacuous() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("package.json"), "{}\n").unwrap();

    let (code, report, stderr) = init_json(dir.path());
    assert_eq!(code, Some(0), "vacuous bootstrap succeeds:\n{stderr}");
    assert_eq!(report["language"], serde_json::json!("generic"));
    assert_eq!(report["baseline"]["state"], serde_json::json!("vacuous"));
    assert!(
        !dir.path().join("Cargo.toml").exists(),
        "generic adoption must not scaffold a crate"
    );
    let config = std::fs::read_to_string(dir.path().join("do-harness.toml")).unwrap();
    assert!(config.contains("language = \"generic\""));
}

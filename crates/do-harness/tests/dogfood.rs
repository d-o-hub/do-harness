//! CLI dogfood for greenfield adoption: the real binary must computationally
//! prove the `init` -> `verify` outcomes — never assume them.
//!
//! - rust: `init` scaffolds a minimal crate, so the full sensor suite runs and
//!   exits 0 on a truly empty tree.

#![allow(clippy::unwrap_used, clippy::expect_used)]
//! - red: removing the crate must flip `verify` to a non-zero exit with named
//!   failing sensors (proves the sensors actually execute).
//! - generic: zero sensors means `verify` exits 0 without running any command
//!   — a documented vacuous pass, asserted as such.

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn isolated_command(program: &str) -> Command {
    let mut command = Command::new(program);
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CEILING_DIRECTORIES",
        "GIT_NAMESPACE",
        "GIT_PREFIX",
        // Outer cargo and coverage-session state must not reach the sandbox.
        // cargo-llvm-cov instruments a build through an inherited
        // RUSTC_WRAPPER and target dir; left in place, the sandbox's own
        // `cargo llvm-cov nextest` re-enters the outer session and dies with
        // "Resource temporarily unavailable (os error 11)", so `verify
        // --strict` fails its coverage sensor and the dogfood assertions flip
        // only under `cargo llvm-cov` — never under `cargo test`.
        "CARGO_TARGET_DIR",
        "CARGO_INCREMENTAL",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_LLVM_COV",
        "CARGO_LLVM_COV_TARGET_DIR",
        "RUSTC_WRAPPER",
        "CARGO_BUILD_RUSTC_WRAPPER",
        "RUSTC",
        "RUSTDOC",
        "RUSTFLAGS",
        "RUSTDOCFLAGS",
    ] {
        command.env_remove(key);
    }
    // Removing the profile path outright would leave instrumented children
    // writing `default_*.profraw` into the test's working directory (the crate
    // root); sandbox builds are never part of the outer measurement, so the
    // profile goes to the null device instead of the tree.
    command.env("LLVM_PROFILE_FILE", "/dev/null");
    command
}

/// Builds a `do-harness --root <root>` command using the real binary.
fn harness(root: &Path) -> Command {
    let mut cmd = isolated_command(env!("CARGO_BIN_EXE_do-harness"));
    cmd.arg("--root").arg(root);
    cmd
}

/// Runs a harness command, returning (success, stdout).
fn run(cmd: &mut Command) -> (bool, String) {
    let output = cmd.output().expect("spawn do-harness");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

/// Runs `verify --format json` (which must exit 0) and parses the report.
fn verify_json(root: &Path) -> Value {
    let (ok, stdout) = run(harness(root).arg("verify").arg("--format").arg("json"));
    assert!(ok, "verify exited non-zero:\n{stdout}");
    serde_json::from_str(&stdout).expect("verify stdout is one JSON object")
}

#[test]
fn rust_init_on_empty_git_repo_is_green() {
    let dir = tempfile::tempdir().unwrap();
    let status = isolated_command("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status()
        .expect("git init");
    assert!(status.success(), "git init failed");
    assert!(dir.path().join(".git").is_dir());

    let (ok, out) = run(harness(dir.path()).arg("init"));
    assert!(
        ok,
        "init failed on fresh git repository with no commits:\n{out}"
    );

    let report = verify_json(dir.path());
    assert_eq!(
        report["ok"],
        serde_json::json!(true),
        "rust init + verify on repo with no commits must be green"
    );
    let commitlint = report["sensors"]
        .as_array()
        .expect("sensors array")
        .iter()
        .find(|s| s["name"] == "commitlint")
        .expect("commitlint sensor entry");
    assert_eq!(commitlint["ok"], serde_json::json!(true));
    assert_eq!(commitlint["exit_code"], serde_json::json!(0));
}

#[test]
fn rust_init_then_full_verify_is_green() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, out) = run(harness(dir.path()).arg("init"));
    assert!(ok, "init failed:\n{out}");

    let report = verify_json(dir.path());
    assert_eq!(
        report["ok"],
        serde_json::json!(true),
        "greenfield rust init + verify must be green, not assumed"
    );
    let sensors: Vec<&str> = report["sensors"]
        .as_array()
        .expect("sensors array")
        .iter()
        .map(|s| s["name"].as_str().expect("sensor name"))
        .collect();
    assert!(
        !sensors.is_empty(),
        "sensors must actually run; a green report with no sensors is vacuous"
    );
    for want in [
        "fmt",
        "check",
        "clippy",
        "test",
        "doctest",
        "loc",
        "commitlint",
        "release-preflight",
    ] {
        assert!(
            sensors.contains(&want),
            "missing sensor {want} in {sensors:?}"
        );
        let ran = report["sensors"]
            .as_array()
            .expect("sensors array")
            .iter()
            .find(|s| s["name"] == want)
            .expect("sensor entry");
        assert_eq!(ran["ok"], serde_json::json!(true), "sensor {want} failed");
        assert_eq!(
            ran["exit_code"],
            serde_json::json!(0),
            "sensor {want} exit code"
        );
    }
}

#[test]
fn rust_doctest_only_failure_is_caught_by_verification_set() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, out) = run(harness(dir.path()).arg("init"));
    assert!(ok, "init failed:\n{out}");

    // Add a broken doctest in src/lib.rs while regular unit tests pass.
    let broken_lib = r"
/// A function with a broken doctest.
/// ```
/// assert_eq!(1, 2);
/// ```
pub fn broken() {}

#[cfg(test)]
mod tests {
    #[test]
    fn unit_test_passes() {
        assert_eq!(1 + 1, 2);
    }
}
";
    std::fs::write(dir.path().join("src/lib.rs"), broken_lib).unwrap();

    let (ok, stdout) = run(harness(dir.path())
        .arg("verify")
        .arg("--set")
        .arg("verification")
        .arg("--format")
        .arg("json"));
    assert!(!ok, "verify --set verification must fail on broken doctest");

    let report: Value = serde_json::from_str(&stdout).expect("valid json report");
    assert_eq!(report["ok"], serde_json::json!(false));

    let failed = report["failed"].as_array().expect("failed array");
    assert!(
        failed.iter().any(|name| name == "doctest"),
        "doctest sensor must be in failed array: {failed:?}"
    );

    let nextest_test = report["sensors"]
        .as_array()
        .expect("sensors")
        .iter()
        .find(|s| s["name"] == "test")
        .expect("test sensor entry");
    assert_eq!(
        nextest_test["ok"],
        serde_json::json!(true),
        "nextest 'test' sensor should pass even when doctest fails"
    );

    let doctest = report["sensors"]
        .as_array()
        .expect("sensors")
        .iter()
        .find(|s| s["name"] == "doctest")
        .expect("doctest sensor entry");
    assert_eq!(
        doctest["ok"],
        serde_json::json!(false),
        "doctest sensor must fail on broken /// example"
    );
}

#[test]
fn rust_verify_fails_without_a_crate() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, out) = run(harness(dir.path()).arg("init"));
    assert!(ok, "init failed:\n{out}");
    std::fs::remove_file(dir.path().join("Cargo.toml")).unwrap();
    std::fs::remove_dir_all(dir.path().join("src")).unwrap();

    let (ok, stdout) = run(harness(dir.path())
        .arg("verify")
        .arg("--format")
        .arg("json"));
    assert!(!ok, "verify must exit non-zero without a crate");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(report["ok"], serde_json::json!(false));
    let failed = report["failed"].as_array().expect("failed array");
    assert!(
        !failed.is_empty(),
        "failed sensors must be listed, not assumed"
    );
    assert!(
        failed.iter().any(|name| name == "fmt"),
        "cargo sensor must fail without Cargo.toml: {failed:?}"
    );
}

#[test]
fn rust_verify_with_evidence_writes_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, out) = run(harness(dir.path()).arg("init"));
    assert!(ok, "init failed:\n{out}");

    let evidence_path = dir.path().join("evidence.json");
    let (ok, _out) = run(harness(dir.path())
        .arg("verify")
        .arg("--evidence")
        .arg(&evidence_path)
        .arg("--strict"));
    assert!(ok, "verify --evidence --strict failed:\n{out}");
    assert!(evidence_path.exists(), "evidence file was not created");

    let text = std::fs::read_to_string(&evidence_path).unwrap();
    let doc: Value = serde_json::from_str(&text).expect("valid evidence JSON");
    assert_eq!(doc["schema_version"], serde_json::json!(4));
    assert_eq!(doc["tool"], serde_json::json!("do-harness"));
    assert_eq!(doc["summary"]["verdict"], serde_json::json!("pass"));
    assert_eq!(doc["summary"]["skip"], serde_json::json!(0));
    // Every sensor records its argv and an output hash; the artifact is sealed.
    assert!(doc["sensors"][0]["argv"].is_array());
    assert!(doc["sensors"][0]["output_sha256"].is_string());
    assert!(doc["chain_hash"].as_str().is_some_and(|h| !h.is_empty()));
}

#[test]
fn rust_verify_strict_fails_on_failing_sensor_and_writes_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, out) = run(harness(dir.path()).arg("init"));
    assert!(ok, "init failed:\n{out}");
    std::fs::remove_file(dir.path().join("Cargo.toml")).unwrap();
    std::fs::remove_dir_all(dir.path().join("src")).unwrap();

    let evidence_path = dir.path().join("failing_evidence.json");
    let (ok, _out) = run(harness(dir.path())
        .arg("verify")
        .arg("--evidence")
        .arg(&evidence_path)
        .arg("--strict"));
    assert!(!ok, "verify --strict must fail on failing sensors");
    assert!(
        evidence_path.exists(),
        "failing run must still write evidence artifact"
    );

    let text = std::fs::read_to_string(&evidence_path).unwrap();
    let doc: Value = serde_json::from_str(&text).expect("valid evidence JSON");
    assert_eq!(doc["summary"]["verdict"], serde_json::json!("fail"));
}

#[test]
fn rust_verify_strict_default_path() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, out) = run(harness(dir.path()).arg("init"));
    assert!(ok, "init failed:\n{out}");

    let (ok, out) = run(harness(dir.path()).arg("verify").arg("--strict"));
    assert!(ok, "verify --strict failed:\n{out}");

    let default_path = dir.path().join(".do-harness/evidence.json");
    assert!(
        default_path.exists(),
        "default evidence file .do-harness/evidence.json was not created"
    );
}

#[test]
fn audit_chain_cli_reports_intact_and_detects_tampering() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, out) = run(harness(dir.path()).arg("task").arg("add").arg("test task"));
    assert!(ok, "task add failed:\n{out}");

    let (ok, stdout) = run(harness(dir.path()).arg("audit-chain"));
    assert!(ok, "audit-chain failed on intact log:\n{stdout}");
    assert!(stdout.contains("OK: Hash chain intact"));

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let db_path = dir.path().join(".do-harness/agent_state.db");
        let conn = do_harness_db::connect(&db_path).await.unwrap();
        conn.execute(
            "UPDATE workflow_events SET chain_hash = 'badhash' WHERE seq = 1",
            (),
        )
        .await
        .unwrap();
    });

    let (ok, out_tampered) = run(harness(dir.path()).arg("audit-chain"));
    assert!(!ok, "audit-chain must fail on tampered log");
    assert!(out_tampered.contains("tampered at event seq 1"));
}

#[test]
fn generic_init_verify_is_vacuously_green() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, out) = run(harness(dir.path())
        .arg("init")
        .arg("--language")
        .arg("generic"));
    assert!(ok, "generic init failed:\n{out}");
    assert!(
        !dir.path().join("Cargo.toml").exists(),
        "generic pack must not scaffold a crate"
    );

    let report = verify_json(dir.path());
    assert_eq!(report["ok"], serde_json::json!(true));
    assert!(
        report["sensors"].as_array().expect("sensors").is_empty(),
        "generic pack ships zero sensors; the pass is vacuous by design"
    );
}

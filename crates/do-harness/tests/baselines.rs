//! CLI dogfood for the findings ratchet: bless initializes and only lowers,
//! regressions fail the gate, and evidence records advisory warns.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

/// Builds a `do-harness --root <root>` command using the real binary.
fn harness(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    cmd.arg("--root").arg(root);
    cmd
}

/// Runs a harness command, returning (success, stdout, stderr).
fn run(cmd: &mut Command) -> (bool, String, String) {
    let output = cmd.output().expect("spawn do-harness");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Writes the fixture workspace: a warn-severity sensor that reports
/// `FINDINGS: <n>` from a script file.
fn fixture(dir: &Path) {
    std::fs::write(
        dir.join("do-harness.toml"),
        r#"
[signal-sets]
verification = ["noisy"]

[[sensors]]
name = "noisy"
argv = ["bash", "noisy.sh"]
severity = "warn"
"#,
    )
    .unwrap();
    write_findings(dir, 3);
}

/// Rewrites the sensor script to report `count` findings and exit non-zero.
fn write_findings(dir: &Path, count: u64) {
    std::fs::write(
        dir.join("noisy.sh"),
        format!("echo 'FINDINGS: {count}'\nexit 1\n"),
    )
    .unwrap();
}

/// Reads the blessed baseline for `noisy`, when the file exists.
fn baseline(dir: &Path) -> Option<u64> {
    let bytes = std::fs::read(dir.join("plans/baselines.json")).ok()?;
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    value["sensors"]["noisy"].as_u64()
}

/// Bless bootstraps a baseline; within-baseline findings warn without failing,
/// a regression fails, and a bless only lowers.
#[test]
fn ratchet_blesses_lowers_and_fails_regressions() {
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path());

    // First bless initializes the baseline from the observed count.
    let (ok, stdout, stderr) = run(harness(dir.path()).args([
        "verify",
        "--record",
        "--bless",
        "--approver",
        "tester",
        "--format",
        "json",
    ]));
    assert!(
        ok,
        "advisory findings must not fail the gate:\n{stdout}\n{stderr}"
    );
    assert_eq!(
        baseline(dir.path()),
        Some(3),
        "bless must initialize the baseline"
    );
    let report: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["sensors"][0]["findings"], 3);
    assert_eq!(report["sensors"][0]["allow_failure"], true);

    // Within the baseline stays advisory and reports the blessed ceiling.
    let (ok, stdout, _) = run(harness(dir.path()).args(["verify", "--format", "json"]));
    assert!(ok, "within-baseline findings must stay advisory:\n{stdout}");
    let report: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["sensors"][0]["baseline"], 3);

    // A regression above the baseline fails even for warn severity.
    write_findings(dir.path(), 5);
    let (ok, stdout, stderr) = run(harness(dir.path()).args(["verify", "--format", "json"]));
    assert!(!ok, "a regression must fail the gate:\n{stdout}\n{stderr}");
    let report: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["failed"], serde_json::json!(["noisy"]));
    assert_eq!(report["sensors"][0]["allow_failure"], false);

    // A bless refuses to raise: the baseline stays at 3.
    let (ok, _, stderr) = run(harness(dir.path()).args([
        "verify",
        "--record",
        "--bless",
        "--approver",
        "tester",
        "--format",
        "json",
    ]));
    assert!(!ok, "the regression still fails the gate");
    assert!(
        stderr.contains("never raises"),
        "expected refusal:\n{stderr}"
    );
    assert_eq!(baseline(dir.path()), Some(3), "a bless must never raise");

    // A bless lowers to the new observed count.
    write_findings(dir.path(), 1);
    let (ok, stdout, stderr) = run(harness(dir.path()).args([
        "verify",
        "--record",
        "--bless",
        "--approver",
        "tester",
        "--format",
        "json",
    ]));
    assert!(
        ok,
        "below-baseline findings stay advisory:\n{stdout}\n{stderr}"
    );
    assert_eq!(
        baseline(dir.path()),
        Some(1),
        "bless must lower the baseline"
    );
}

/// A non-strict run records advisory verdicts in evidence; `--strict`
/// promotes the same findings to failures.
#[test]
fn strict_promotes_warns_and_evidence_records_them() {
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path());
    let (ok, stdout, stderr) = run(harness(dir.path()).args([
        "verify",
        "--record",
        "--bless",
        "--approver",
        "tester",
        "--format",
        "json",
    ]));
    assert!(ok, "bootstrap bless must pass:\n{stdout}\n{stderr}");

    let evidence_path = dir.path().join(".do-harness/evidence.verification.json");

    let (ok, stdout, stderr) =
        run(harness(dir.path()).args(["verify", "--set", "verification", "--format", "json"]));
    assert!(
        ok,
        "advisory findings must not fail a non-strict set:\n{stdout}\n{stderr}"
    );
    let evidence: Value = serde_json::from_slice(&std::fs::read(&evidence_path).unwrap()).unwrap();
    assert_eq!(evidence["sensors"][0]["verdict"], "warn");
    assert_eq!(evidence["summary"]["verdict"], "fail");

    let (ok, stdout, stderr) = run(harness(dir.path()).args([
        "verify",
        "--set",
        "verification",
        "--strict",
        "--format",
        "json",
    ]));
    assert!(
        !ok,
        "--strict must fail on advisory findings:\n{stdout}\n{stderr}"
    );
    let evidence: Value = serde_json::from_slice(&std::fs::read(&evidence_path).unwrap()).unwrap();
    assert_eq!(
        evidence["sensors"][0]["verdict"], "fail",
        "strict promotes the advisory failure in evidence"
    );
}

/// `--bless` without `--record` is rejected by the CLI.
#[test]
fn bless_requires_record() {
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path());
    let (ok, _, stderr) = run(harness(dir.path()).args(["verify", "--bless"]));
    assert!(!ok);
    assert!(
        stderr.contains("--record") || stderr.contains("required"),
        "expected a requires error:\n{stderr}"
    );
}

/// The ratchet persists observed maxima and bless history in the state DB.
#[tokio::test(flavor = "current_thread")]
async fn ratchet_persists_findings_and_bless_history() {
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path());
    let (ok, _, stderr) = run(harness(dir.path()).args([
        "verify",
        "--record",
        "--bless",
        "--approver",
        "tester",
        "--format",
        "json",
    ]));
    assert!(ok, "bootstrap bless must pass:\n{stderr}");

    write_findings(dir.path(), 5);
    let (ok, _, _) = run(harness(dir.path()).args(["verify", "--record", "--format", "json"]));
    assert!(!ok, "the regression must fail");

    write_findings(dir.path(), 1);
    let (ok, _, stderr) = run(harness(dir.path()).args([
        "verify",
        "--record",
        "--bless",
        "--approver",
        "tester",
        "--format",
        "json",
    ]));
    assert!(ok, "the lowering bless must pass:\n{stderr}");

    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let findings = do_harness_db::list_sensor_findings(&conn).await.unwrap();
    let noisy = findings
        .iter()
        .find(|row| row.sensor_name == "noisy")
        .expect("findings row");
    assert_eq!(noisy.max_findings, 5, "observed maximum must persist");

    let blesses = do_harness_db::list_sensor_blesses(&conn).await.unwrap();
    assert!(
        blesses.iter().any(|bless| {
            bless.sensor_name == "noisy"
                && bless.max_findings == 1
                && bless.previous_max == Some(3)
                && bless.approver == "tester"
        }),
        "lowering bless must be audited: {blesses:?}"
    );
}

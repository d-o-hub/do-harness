//! Acceptance fixtures for the deterministic DORA collector.
//!
//! Every fixture pins commit timestamps and injects the clock with `--now`,
//! so no test reads a real clock and the asserted numbers are exact. The
//! point of these fixtures is the pair of contracts the design exists for:
//! a metric is only reported with its derivation, and identical input yields
//! byte-identical output.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

mod support;

use support::git_command;

/// First fixture commit timestamp.
const T0: i64 = 1_700_000_000;

/// Builds a `do-harness --root <root>` command using the real binary.
fn harness(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    cmd.arg("--root").arg(root);
    cmd
}

/// Runs a harness command, returning (exit code, `stdout`, `stderr`).
fn run(cmd: &mut Command) -> (Option<i32>, String, String) {
    let output = cmd.output().expect("spawn do-harness");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Runs a git command inside `root` with pinned identity, asserting success.
fn git(root: &Path, args: &[&str]) {
    let status = git_command(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .args(args)
        .status()
        .expect("spawn git");
    assert!(
        status.success(),
        "git {args:?} failed in {}",
        root.display()
    );
}

/// Commits an empty change at `ts` with `subject`, pinned to an exact epoch.
fn commit_at(root: &Path, ts: i64, subject: &str) {
    let stamp = ts.to_string();
    let status = git_command(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .env("GIT_AUTHOR_DATE", &stamp)
        .env("GIT_COMMITTER_DATE", &stamp)
        .args(["commit", "-q", "--allow-empty", "-m", subject])
        .status()
        .expect("spawn git commit");
    assert!(
        status.success(),
        "git commit at {ts} failed in {}",
        root.display()
    );
}

/// Creates a fixture repository with the pinned DORA policy installed.
fn fixture_repo() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join("plans")).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plans/dora.json"),
        root.join("plans/dora.json"),
    )
    .unwrap();
    git(&root, &["init", "-q"]);
    (dir, root)
}

/// Runs `dora --format json --now <now> --days 30`, asserting the exit code.
fn dora_json(root: &Path, now: i64, expect_code: i32) -> (String, String, Value) {
    let (code, stdout, stderr) = run(harness(root)
        .arg("dora")
        .arg("--format")
        .arg("json")
        .arg("--days")
        .arg("30")
        .arg("--now")
        .arg(now.to_string()));
    assert_eq!(
        code,
        Some(expect_code),
        "dora exited {code:?}:\n{stdout}\n{stderr}"
    );
    let value = serde_json::from_str(&stdout)
        .unwrap_or_else(|_| panic!("dora stdout is not JSON:\n{stdout}\n{stderr}"));
    (stdout, stderr, value)
}

/// Breach rule names in reported order.
fn breach_names(snapshot: &Value) -> Vec<String> {
    snapshot["breaches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|breach| breach["name"].as_str().unwrap().to_string())
        .collect()
}

/// The fully populated fixture: two deploys, one revert, no restore.
fn deployed_fixture() -> (tempfile::TempDir, PathBuf) {
    let (dir, root) = fixture_repo();
    commit_at(&root, T0, "chore: seed");
    git(&root, &["tag", "v0.1.0"]);
    commit_at(&root, T0 + 100, "feat: a");
    commit_at(&root, T0 + 200, "feat: b");
    git(&root, &["tag", "v0.1.1"]);
    commit_at(&root, T0 + 400, "revert(fixture): drop the broken change");
    (dir, root)
}

#[test]
fn deployed_fixture_derives_exact_metrics_and_breaches() {
    let (_dir, root) = deployed_fixture();
    let (stdout, stderr, snapshot) = dora_json(&root, T0 + 86_400, 1);

    assert_eq!(snapshot["deploy_count"], serde_json::json!(2));
    assert_eq!(
        snapshot["deploy_tags"],
        serde_json::json!(["v0.1.0", "v0.1.1"])
    );
    assert_eq!(snapshot["lead_samples"], serde_json::json!(3));
    assert_eq!(snapshot["lead_p50_seconds"], serde_json::json!(0));
    assert_eq!(snapshot["lead_p90_seconds"], serde_json::json!(100));
    assert_eq!(snapshot["deploys_failed"], serde_json::json!(1));
    assert_eq!(snapshot["mttr_restored"], serde_json::json!(0));
    assert_eq!(snapshot["mttr_unrestored"], serde_json::json!(1));
    assert_eq!(snapshot["mttr_seconds"], Value::Null);
    assert_eq!(
        breach_names(&snapshot),
        vec!["change_failure_rate", "unrestored_deploys"]
    );

    // The derivation manifest is the whole point: the number is only
    // reportable because its inputs are recorded with it.
    let ranges = snapshot["derivation"]["ranges"].as_array().unwrap();
    assert_eq!(ranges.len(), 2);
    assert_eq!(ranges[0]["tag"], serde_json::json!("v0.1.0"));
    assert_eq!(ranges[0]["range"], serde_json::json!("v0.1.0"));
    assert_eq!(ranges[1]["range"], serde_json::json!("v0.1.0..v0.1.1"));
    assert_eq!(
        ranges[1]["failure_range"],
        serde_json::json!("v0.1.1..HEAD")
    );
    let incidents = snapshot["derivation"]["incidents"].as_array().unwrap();
    assert_eq!(incidents.len(), 1);
    assert_eq!(incidents[0]["restore_ts"], Value::Null);
    assert_eq!(
        snapshot["derivation"]["clock_skew_commits"],
        serde_json::json!(0)
    );
    assert_eq!(
        snapshot["derivation"]["percentile_method"],
        serde_json::json!("nearest-rank")
    );

    assert!(stdout.contains("\"source_rev\""));
    assert!(stderr.contains("FINDINGS: 2"), "stderr: {stderr}");
    let coverage: Vec<&str> = stderr
        .lines()
        .filter_map(|line| line.strip_prefix("COVERAGE: "))
        .collect();
    assert_eq!(coverage.len(), 1, "exactly one COVERAGE line: {stderr}");
    let parsed: Value = serde_json::from_str(coverage[0]).expect("COVERAGE is JSON");
    assert_eq!(parsed["ranges"].as_array().unwrap().len(), 2);
}

#[test]
fn repeated_runs_are_byte_identical() {
    let (_dir, root) = deployed_fixture();
    let (first, _, _) = dora_json(&root, T0 + 86_400, 1);
    let (second, _, _) = dora_json(&root, T0 + 86_400, 1);
    assert_eq!(
        first, second,
        "a metric that is not provably re-derivable is an opinion with a decimal point"
    );
}

#[test]
fn clean_deploy_history_reports_no_breach() {
    let (_dir, root) = fixture_repo();
    commit_at(&root, T0, "chore: seed");
    git(&root, &["tag", "v0.1.0"]);
    commit_at(&root, T0 + 100, "feat: a");

    let (_stdout, stderr, snapshot) = dora_json(&root, T0 + 86_400, 0);
    assert_eq!(snapshot["deploy_count"], serde_json::json!(1));
    assert_eq!(snapshot["deploys_failed"], serde_json::json!(0));
    assert!(breach_names(&snapshot).is_empty());
    assert!(stderr.contains("FINDINGS: 0"), "stderr: {stderr}");
}

#[test]
fn tagless_history_reports_missing_measurements_as_unmeasured() {
    let (_dir, root) = fixture_repo();
    commit_at(&root, T0, "chore: seed");

    let (_stdout, _stderr, snapshot) = dora_json(&root, T0 + 86_400, 1);
    assert_eq!(snapshot["deploy_count"], serde_json::json!(0));
    assert_eq!(snapshot["lead_p50_seconds"], Value::Null);
    assert_eq!(snapshot["lead_p90_seconds"], Value::Null);
    assert_eq!(snapshot["mttr_seconds"], Value::Null);
    // Both rules are intended: with no deploys there is no evidence of health,
    // and an unmeasurable lead time is a breach rather than a pass.
    assert_eq!(
        breach_names(&snapshot),
        vec!["deploys_below_min", "lead_p90"]
    );

    let (code, stdout, _stderr) = run(harness(&root)
        .arg("dora")
        .arg("--days")
        .arg("30")
        .arg("--now")
        .arg((T0 + 86_400).to_string()));
    assert_eq!(code, Some(1));
    assert!(stdout.contains("  lead time          p50 -  p90 -  (n=0)\n"));
    assert!(stdout.contains("  change failure rate -\n"));
    assert!(stdout.contains("  time to restore    -  (0 restored, 0 unrestored)\n"));
}

#[test]
fn missing_policy_is_a_usage_error_not_a_defaulted_measurement() {
    let (_dir, root) = fixture_repo();
    commit_at(&root, T0, "chore: seed");
    std::fs::remove_file(root.join("plans/dora.json")).unwrap();

    let (code, _stdout, stderr) = run(harness(&root).arg("dora").arg("--now").arg(T0.to_string()));
    assert_eq!(code, Some(2), "stderr: {stderr}");
    assert!(stderr.contains("plans/dora.json"), "stderr: {stderr}");
}

#[test]
fn outside_a_work_tree_the_collector_refuses_to_answer() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join("plans")).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plans/dora.json"),
        root.join("plans/dora.json"),
    )
    .unwrap();

    let (code, stdout, stderr) = run(harness(&root).arg("dora").arg("--now").arg(T0.to_string()));
    assert_eq!(code, Some(2), "stdout: {stdout}\nstderr: {stderr}");
    assert!(
        stderr.contains("not inside a git working tree"),
        "stderr: {stderr}"
    );
}

/// Installs the repository's own `dora` sensor wiring into `root`.
///
/// The shim lives at `scripts/check-dora.sh` in the real repo and resolves
/// the binary through `DO_HARNESS_BIN`, so the fixture copies the committed
/// shim and points that variable at the test binary. The sensor spec mirrors
/// `do-harness.toml`, including `coverage-inputs`, because the policy
/// fingerprint is part of what this test exercises.
fn install_dora_sensor(root: &Path) {
    std::fs::create_dir_all(root.join("scripts")).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/check-dora.sh"),
        root.join("scripts/check-dora.sh"),
    )
    .unwrap();
    std::fs::write(
        root.join("do-harness.toml"),
        r#"
[signal-sets]
metrics = ["dora"]

[[sensors]]
name = "dora"
argv = ["bash", "scripts/check-dora.sh"]
severity = "warn"
coverage-inputs = ["plans/dora.json"]
"#,
    )
    .unwrap();
}

/// Runs `verify --only dora --record` repeatedly, returning the last report.
///
/// `DO_HARNESS_BIN` is required because the copied shim resolves the binary
/// through it, exactly as the managed hooks do.
fn verify_record_dora(root: &Path, runs: usize) -> Value {
    let mut report = Value::Null;
    for attempt in 0..runs {
        let (code, stdout, stderr) = run(harness(root)
            .args(["verify", "--only", "dora", "--record", "--format", "json"])
            .env("DO_HARNESS_BIN", env!("CARGO_BIN_EXE_do-harness")));
        assert_eq!(
            code,
            Some(0),
            "record run {attempt} must pass the sensor gate:\n{stdout}\n{stderr}"
        );
        report = serde_json::from_str(&stdout)
            .unwrap_or_else(|_| panic!("verify stdout is not JSON:\n{stdout}\n{stderr}"));
    }
    report
}

/// Error-signature keys recorded in `root`'s state database.
fn recorded_signatures(root: &Path) -> Vec<String> {
    let (_code, stdout, stderr) = run(harness(root).args(["errors", "list", "--format", "json"]));
    let parsed: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|_| panic!("errors list stdout is not JSON:\n{stdout}\n{stderr}"));
    parsed
        .as_array()
        .expect("errors list JSON is an array")
        .iter()
        .map(|row| row["signature"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// A breaching metric must keep reporting instead of being quarantined.
///
/// This is the load-bearing exit-code contract: `check-dora.sh` maps the
/// CLI's exit 1 (a threshold breach) to sensor success, because a warn sensor
/// that fails three consecutive `--record` runs is quarantined and **never
/// executed again** — the metric would silently vanish rather than fail. A
/// plausible future edit (`exit "$code"` in the shim) would end that, so the
/// count must survive three recorded runs and leave no strike behind.
#[test]
fn reaching_breach_count_never_quarantines_the_sensor() {
    let (_dir, root) = deployed_fixture();
    install_dora_sensor(&root);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "chore: wire the dora sensor"]);

    let report = verify_record_dora(&root, 3);
    assert_eq!(report["ok"], serde_json::json!(true));
    assert_eq!(
        report["sensors"][0]["findings"],
        serde_json::json!(2),
        "the breach count must reach the runner through FINDINGS:"
    );
    // Advisory, not blocking: the gate passes while the breach is reported.
    assert_eq!(
        report["sensors"][0]["allow_failure"],
        serde_json::json!(true)
    );
    assert_eq!(report["sensors"][0]["severity"], serde_json::json!("warn"));

    // Zero strikes is the actual anti-quarantine evidence: three of them
    // would make the next run skip the sensor entirely.
    assert!(
        !recorded_signatures(&root).contains(&"sensor:dora".to_string()),
        "a breaching metric must accrue no strikes: {:?}",
        recorded_signatures(&root)
    );

    // The fourth run still executes and still reports the real count.
    let last = verify_record_dora(&root, 1);
    assert_eq!(last["sensors"][0]["findings"], serde_json::json!(2));
}

/// The shim must prefer the repo build over a stale PATH-installed binary.
///
/// `~/.cargo/bin/do-harness` can predate the workspace, and a `PATH` lookup
/// that wins over the fresh repo build makes `verify` dogfood an old CLI —
/// observed as `unrecognized subcommand 'dora'` from a pre-dora binary, a
/// failure the working tree does not have. `hook_script.rs` guards this with
/// "repo build newer wins"; the shim must too. With no repo build present,
/// falling back to `PATH` is correct, so the fixture provides one.
#[test]
fn a_stale_path_binary_does_not_shadow_the_repo_build() {
    let (_dir, root) = deployed_fixture();
    install_dora_sensor(&root);

    // A repo-local release build, newer than the PATH stand-in below.
    let repo_bin = root.join("target/release");
    std::fs::create_dir_all(&repo_bin).unwrap();
    std::fs::copy(
        env!("CARGO_BIN_EXE_do-harness"),
        repo_bin.join("do-harness"),
    )
    .unwrap();

    // A fake `do-harness` on PATH that cannot run `dora`, standing in for a
    // cargo-installed binary that predates this workspace.
    let stale_dir = root.join("stale-bin");
    std::fs::create_dir_all(&stale_dir).unwrap();
    let stale = stale_dir.join("do-harness");
    std::fs::write(
        &stale,
        "#!/bin/sh\necho \"unrecognized subcommand\" >&2\nexit 2\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&stale, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    // Age the stand-in so the repo build is strictly newer.
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3_600);
    let handle = std::fs::File::options().write(true).open(&stale).unwrap();
    handle.set_modified(old).unwrap();
    drop(handle);

    let path = format!(
        "{}:{}",
        stale_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let (code, stdout, stderr) = run(harness(&root)
        .args(["verify", "--only", "dora", "--format", "json"])
        .env("DO_HARNESS_BIN", "")
        .env("PATH", &path));
    assert_eq!(
        code,
        Some(0),
        "the shim must not fail on a stale binary:\n{stderr}"
    );
    let report: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|_| panic!("verify stdout is not JSON:\n{stdout}\n{stderr}"));
    assert_eq!(
        report["sensors"][0]["findings"],
        serde_json::json!(2),
        "the fresh repo build must be the one executed: {report}"
    );
    assert!(
        !stderr.contains("unrecognized subcommand"),
        "the stale stand-in must never run:\n{stderr}"
    );
}

/// A collector that cannot answer must report failure rather than health.
///
/// The shim propagates the CLI's exit 2, so the sensor is `ok = false` with
/// `exit_code = 2` — visible in the report and in evidence. Because the
/// sensor is `warn` severity the overall gate stays advisory (that is the
/// report-only contract), but `--strict` promotes it to a hard failure, which
/// is what stops a broken collector from passing for a healthy metric.
#[test]
fn an_unresolvable_collector_reports_failure_and_fails_strict() {
    let (_dir, root) = fixture_repo();
    commit_at(&root, T0, "chore: seed");
    install_dora_sensor(&root);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "chore: wire the dora sensor"]);
    // Remove the pinned policy: the collector must refuse, not default.
    std::fs::remove_file(root.join("plans/dora.json")).unwrap();

    let (code, stdout, stderr) = run(harness(&root)
        .args(["verify", "--only", "dora", "--format", "json"])
        .env("DO_HARNESS_BIN", env!("CARGO_BIN_EXE_do-harness")));
    assert_eq!(code, Some(0), "warn severity stays advisory:\n{stderr}");
    let report: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|_| panic!("verify stdout is not JSON:\n{stdout}\n{stderr}"));
    assert_eq!(
        report["sensors"][0]["ok"],
        serde_json::json!(false),
        "a failed collector must not read as healthy: {report}"
    );
    assert_eq!(
        report["sensors"][0]["exit_code"],
        serde_json::json!(2),
        "the shim must propagate the collector's exit 2: {report}"
    );
    assert!(
        stderr.contains("plans/dora.json"),
        "the failure must name the missing policy:\n{stderr}\n{stdout}"
    );

    // `--strict` turns the advisory failure into a hard one.
    let (strict_code, _stdout, strict_stderr) = run(harness(&root)
        .args(["verify", "--only", "dora", "--strict", "--format", "json"])
        .env("DO_HARNESS_BIN", env!("CARGO_BIN_EXE_do-harness")));
    assert_eq!(
        strict_code,
        Some(1),
        "a broken collector must fail under --strict:\n{strict_stderr}"
    );
}

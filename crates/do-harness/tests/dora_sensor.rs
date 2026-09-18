//! Sensor-wiring acceptance fixtures for the DORA deployment metric.
//!
//! These exercise the shipped sensor (`scripts/check-dora.sh`) through
//! `verify`, which is where the report-only contract actually lives: a
//! breaching metric must keep being measured rather than be quarantined, a
//! broken collector must not read as healthy, and the shim must execute the
//! workspace build rather than a stale binary from `PATH`.
//!
//! Shared fixtures live in `support::dora`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use serde_json::Value;

use support::dora::{harness, installed_sensor_repo, run};

mod support;

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
    let (_dir, root) = installed_sensor_repo();

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
    let (_dir, root) = installed_sensor_repo();

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
    let (_dir, root) = installed_sensor_repo();
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

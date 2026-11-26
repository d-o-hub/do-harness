//! The `loc` sensor must be the ceiling *gate*, not the report-only query.
//!
//! `do-harness loc` always exits 0 by design: it reports and defers the verdict
//! to the sensor. Wiring it as the sensor therefore made the ceiling advisory
//! in every run, including the release preflight — `main` shipped a 535-line
//! `crates/do-harness/src/cli/actions.rs` while `loc` reported PASS, and the
//! tag preflight only failed on an unrelated sensor.
//!
//! Two independent facts are pinned here:
//! 1. behaviour — `scripts/check-loc.sh --root <fixture>` exits non-zero for an
//!    over-limit file and names it, and exits 0 for a small one;
//! 2. wiring — the `loc` sensor runs that script and is part of the
//!    `verification`, `main`, and `release` signal sets, so the gate runs where
//!    a regression could otherwise reach a release.

// A test asserts; a panic is the failure signal, and a fixture that cannot be
// created has no meaningful alternative result.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

/// A scratch scope inside the workspace, because `--root` resolves relative to
/// the repository root. `target/` is gitignored, so the fixture never becomes a
/// tracked file.
struct Fixture {
    relative: String,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let relative = format!("target/loc-gate-{name}-{}", std::process::id());
        let dir = workspace_root().join(&relative);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).expect("create fixture scope");
        Self { relative }
    }

    fn write(&self, file: &str, body: &str) {
        fs::write(workspace_root().join(&self.relative).join(file), body).expect("write fixture");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(workspace_root().join(&self.relative));
    }
}

fn run_gate(scope: &str) -> std::process::Output {
    Command::new("bash")
        .arg("scripts/check-loc.sh")
        .arg("--root")
        .arg(scope)
        .current_dir(workspace_root())
        .output()
        .expect("run scripts/check-loc.sh")
}

#[test]
fn the_loc_gate_fails_and_names_an_over_limit_file() {
    let fixture = Fixture::new("over");
    fixture.write(
        "src/big.rs",
        &(0..600)
            .map(|line| format!("pub fn f{line}() {{}}\n"))
            .collect::<String>(),
    );

    let output = run_gate(&fixture.relative);
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        !output.status.success(),
        "the loc gate accepted a 600-line file; stdout+stderr:\n{combined}"
    );
    assert!(
        combined.contains("src/big.rs") && combined.contains("600"),
        "the failure does not name the offending file and size:\n{combined}"
    );
    assert!(
        combined.contains("FINDINGS: 1"),
        "the failure does not report FINDINGS for the ratchet:\n{combined}"
    );
}

#[test]
fn the_loc_gate_accepts_a_file_under_the_ceiling() {
    let fixture = Fixture::new("under");
    fixture.write(
        "src/small.rs",
        &(0..10)
            .map(|line| format!("pub fn f{line}() {{}}\n"))
            .collect::<String>(),
    );

    let output = run_gate(&fixture.relative);
    assert!(
        output.status.success(),
        "the loc gate rejected a 10-line file; stdout+stderr:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn the_loc_sensor_runs_the_gate_in_every_gating_set() {
    let root = workspace_root();
    let content = fs::read_to_string(root.join("do-harness.toml")).expect("read do-harness.toml");
    let document: toml::Value = toml::from_str(&content).expect("parse do-harness.toml");

    let sensor = document
        .get("sensors")
        .and_then(toml::Value::as_array)
        .expect("[[sensors]] entries")
        .iter()
        .find(|entry| entry.get("name").and_then(toml::Value::as_str) == Some("loc"))
        .expect("a loc sensor is registered");

    let argv: Vec<&str> = sensor
        .get("argv")
        .and_then(toml::Value::as_array)
        .expect("loc argv")
        .iter()
        .filter_map(toml::Value::as_str)
        .collect();
    assert!(
        argv.contains(&"scripts/check-loc.sh"),
        "the loc sensor must run scripts/check-loc.sh (the gate), not a report-only \
         command; got {argv:?}"
    );

    let sets = document
        .get("signal-sets")
        .and_then(toml::Value::as_table)
        .expect("[signal-sets] table");
    for set in ["verification", "main", "release"] {
        let members: Vec<&str> = sets
            .get(set)
            .and_then(toml::Value::as_array)
            .unwrap_or_else(|| panic!("signal set {set} exists"))
            .iter()
            .filter_map(toml::Value::as_str)
            .collect();
        assert!(
            members.contains(&"loc"),
            "signal set {set} does not include the loc gate, so an over-limit file could \
             reach that gate's verdict"
        );
    }
}

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

/// A Rust source body with exactly `count` function lines.
fn source_lines(count: usize) -> String {
    use std::fmt::Write as _;

    let mut body = String::new();
    for line in 0..count {
        writeln!(body, "pub fn f{line}() {{}}").expect("write to a String cannot fail");
    }
    body
}

#[test]
fn the_loc_gate_fails_and_names_an_over_limit_file() {
    let fixture = Fixture::new("over");
    fixture.write("src/big.rs", &source_lines(600));

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
    fixture.write("src/small.rs", &source_lines(10));

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

#[test]
fn the_loc_baseline_cannot_grandfather_an_over_limit_file() {
    let root = workspace_root();
    let raw = fs::read_to_string(root.join("plans/baselines.json")).expect("read baselines");
    let document: serde_json::Value = serde_json::from_str(&raw).expect("parse baselines");

    let count = document
        .get("sensors")
        .and_then(serde_json::Value::as_object)
        .and_then(|sensors| sensors.get("loc"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);

    assert_eq!(
        count, 0,
        "plans/baselines.json grandfathers {count} over-limit file(s) for the loc sensor; \
         the ceiling is not a ratchet — a blessed count would let the release gate pass an \
         over-limit tree again"
    );
}

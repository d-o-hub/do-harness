//! The `loc` sensor must be the ceiling *gate*, not the report-only query.
//!
//! `do-harness loc` reports and exits 0 unless `--strict` is given — the query
//! behaviour that keeps agent loops usable. The `loc` sensor therefore ran a
//! command that could not fail: `main` shipped a 535-line
//! `crates/do-harness/src/cli/actions.rs` through every gate, and the v0.5.0
//! release preflight printed `PASS loc` over an over-limit tree.
//!
//! The gate is the native command with `--strict`, so it runs on every host
//! without a POSIX shell: a Windows adopter whose only `bash` is the WSL
//! launcher must still be able to run the harness's own sensors.
//!
//! Three facts are pinned here:
//! 1. behaviour — `loc --strict` exits 1 and names an over-limit file, exits 0
//!    under the ceiling, and the report-only default still exits 0;
//! 2. wiring — the `loc` sensor runs that command and is part of the
//!    `verification`, `main`, and `release` signal sets;
//! 3. no grandfathering — `plans/baselines.json` carries no `loc` count, so a
//!    blessed finding cannot let the gate pass an over-limit tree again.

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

/// A scratch workspace root for one case. `target/` is gitignored, so the
/// fixture never becomes a tracked file.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let root = workspace_root().join(format!("target/loc-gate-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).expect("create fixture scope");
        Self { root }
    }

    fn write(&self, file: &str, lines: usize) {
        use std::fmt::Write as _;

        let mut body = String::new();
        for line in 0..lines {
            writeln!(body, "pub fn f{line}() {{}}").expect("write to a String cannot fail");
        }
        fs::write(self.root.join(file), body).expect("write fixture");
    }

    fn run(&self, strict: bool) -> std::process::Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_do-harness"));
        command.arg("--root").arg(&self.root).arg("loc");
        if strict {
            command.arg("--strict");
        }
        command.output().expect("run do-harness loc")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn the_strict_gate_fails_and_names_an_over_limit_file() {
    let fixture = Fixture::new("over");
    fixture.write("src/big.rs", 600);

    let output = fixture.run(true);
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert_eq!(
        output.status.code(),
        Some(1),
        "loc --strict must exit 1 on an over-limit file; stdout+stderr:\n{combined}"
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
fn the_strict_gate_accepts_a_file_under_the_ceiling() {
    let fixture = Fixture::new("under");
    fixture.write("src/small.rs", 10);

    let output = fixture.run(true);
    assert!(
        output.status.success(),
        "loc --strict rejected a 10-line file; stdout+stderr:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn the_report_only_query_still_exits_zero() {
    let fixture = Fixture::new("query");
    fixture.write("src/big.rs", 600);

    let output = fixture.run(false);
    assert!(
        output.status.success(),
        "plain `loc` must stay a query and exit 0; stdout+stderr:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("FINDINGS: 1"),
        "the query must still report the finding for the ratchet"
    );
}

#[test]
fn the_loc_sensor_runs_the_native_gate_in_every_gating_set() {
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
        argv.contains(&"--strict"),
        "the loc sensor must run the failing gate (`--strict`), not the report-only \
         query; got {argv:?}"
    );
    assert!(
        !argv.iter().any(|arg| {
            *arg == "bash"
                || Path::new(arg)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("sh"))
        }),
        "the loc gate must not require a POSIX shell — it runs on every host; got {argv:?}"
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

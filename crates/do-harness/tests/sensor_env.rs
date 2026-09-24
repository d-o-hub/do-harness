//! Regression coverage for sensor environment isolation.
//!
//! A harness launched with cargo inherits the launcher crate's CARGO_* identity
//! variables. Sensors must not see those variables: cargo-derived tools use
//! them to distinguish Cargo external-subcommand dispatch from cargo run.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::process::Command;

#[test]
fn sensor_does_not_inherit_launcher_crate_identity() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    fs::write(
        root.join("do-harness.toml"),
        r#"language = "rust"

[[sensors]]
name = "env-probe"
argv = ["bash", "check-env.sh"]
"#,
    )
    .unwrap();
    fs::write(
        root.join("check-env.sh"),
        r#"#!/usr/bin/env bash
set -eu

if [[ -n "${CARGO_PKG_NAME+x}" ]]; then
    echo "CARGO_PKG_NAME leaked into sensor"
    exit 1
fi
if [[ -n "${CARGO_MANIFEST_DIR+x}" ]]; then
    echo "CARGO_MANIFEST_DIR leaked into sensor"
    exit 1
fi
if [[ "${DO_HARNESS_TEST_KEEP:-}" != "1" ]]; then
    echo "sensor contract variable was removed"
    exit 1
fi
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_do-harness"))
        .arg("--root")
        .arg(root)
        .args(["verify", "--only", "env-probe"])
        .env("CARGO_PKG_NAME", "do-harness")
        .env("CARGO_MANIFEST_DIR", "/launcher")
        .env("DO_HARNESS_TEST_KEEP", "1")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "verify failed:
{}
{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("PASS  env-probe"),
        "unexpected verify output: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

//! Integration tests for `do-harness setup web` (real binary).
//!
//! The detected package manager is `npm` in every fixture (package-lock.json);
//! versions are stub manifests under `node_modules/`, so no network access is
//! needed. Non-interactive runs use a non-TTY stdin, which exercises the
//! "print the install command and fail closed" path.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::process::{Command, Output};

fn harness(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_do-harness"))
        .args(args)
        .output()
        .expect("spawn do-harness binary")
}

fn web_config() -> &'static str {
    "language = \"web\"\n"
}

fn stub_dep(root: &std::path::Path, name: &str, version: &str) {
    let dir = root.join("node_modules").join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("package.json"),
        format!(r#"{{"name": "{name}", "version": "{version}"}}"#),
    )
    .unwrap();
}

fn stub_all_web_deps(root: &std::path::Path) {
    stub_dep(root, "playwright", "1.40.0");
    stub_dep(root, "@axe-core/playwright", "4.8.0");
    stub_dep(root, "lighthouse", "11.0.0");
}

#[test]
fn setup_web_patches_config_when_deps_present() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("do-harness.toml"), web_config()).unwrap();
    fs::write(root.join("package.json"), r#"{"name": "fixture"}"#).unwrap();
    fs::write(root.join("package-lock.json"), "").unwrap();
    stub_all_web_deps(root);

    let output = harness(&["--root", root.to_str().unwrap(), "setup", "web", "--yes"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "setup web failed: {stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("package manager: npm"));
    assert!(stdout.contains("all web audit peer dependencies are installed"));
    assert!(
        stdout.contains("verify --only viewport-ux,a11y,console,perf,visual,i18n,web-ui-tests")
    );

    let config = fs::read_to_string(root.join("do-harness.toml")).unwrap();
    assert!(config.contains("[env]"));
    assert!(config.contains("WEB_AUDIT_ROUTES = \"/\""));
    assert!(config.contains("WEB_AUDIT_BASE_URL = \"http://127.0.0.1:3000\""));
    let table: toml::Table = config.parse().unwrap();
    assert_eq!(table["env"]["WEB_AUDIT_ROUTES"].as_str(), Some("/"));
}

#[test]
fn setup_web_offers_install_without_yes_noninteractive() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("do-harness.toml"), web_config()).unwrap();
    fs::write(root.join("package.json"), r#"{"name": "fixture"}"#).unwrap();
    fs::write(root.join("package-lock.json"), "").unwrap();
    stub_all_web_deps(root);
    // lighthouse missing: the command must print the install command, patch
    // the config, and fail closed (exit 2 = usage error) without installing.
    fs::remove_dir_all(root.join("node_modules/lighthouse")).unwrap();

    let output = harness(&["--root", root.to_str().unwrap(), "setup", "web"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(2));
    assert!(stdout.contains("npm install -D lighthouse"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("setup web --yes"));

    // The config is patched even though the install was not run.
    let config = fs::read_to_string(root.join("do-harness.toml")).unwrap();
    assert!(config.contains("[env]"));
    assert!(config.contains("WEB_AUDIT_ROUTES"));
}

#[test]
fn setup_web_dry_run_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("do-harness.toml"), web_config()).unwrap();
    fs::write(root.join("package.json"), r#"{"name": "fixture"}"#).unwrap();
    fs::write(root.join("package-lock.json"), "").unwrap();
    stub_all_web_deps(root);

    let before = fs::read_to_string(root.join("do-harness.toml")).unwrap();
    let output = harness(&[
        "--root",
        root.to_str().unwrap(),
        "--dry-run",
        "setup",
        "web",
    ]);
    assert!(
        output.status.success(),
        "dry-run setup web failed: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let after = fs::read_to_string(root.join("do-harness.toml")).unwrap();
    assert_eq!(before, after);
}

#[test]
fn setup_web_requires_existing_config() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("package.json"), r#"{"name": "fixture"}"#).unwrap();
    let output = harness(&["--root", root.to_str().unwrap(), "setup", "web", "--yes"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr.contains("do-harness init --language web"));
}

#[test]
fn config_env_reaches_sensors() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(
        root.join("do-harness.toml"),
        r#"language = "web"

[env]
WEB_TEST_VAR = "from-config"

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
[[ "${WEB_TEST_VAR:-}" == "from-config" ]] || { echo "missing WEB_TEST_VAR"; exit 1; }
echo "OK: WEB_TEST_VAR from config"
"#,
    )
    .unwrap();

    let output = harness(&[
        "--root",
        root.to_str().unwrap(),
        "verify",
        "--only",
        "env-probe",
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "verify failed: {stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn setup_web_offers_install_without_yes_noninteractive_missing_all() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("do-harness.toml"), web_config()).unwrap();
    fs::write(root.join("package.json"), r#"{"name": "fixture"}"#).unwrap();
    fs::write(root.join("package-lock.json"), "").unwrap();

    let output = harness(&["--root", root.to_str().unwrap(), "setup", "web"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(2));
    assert!(stdout.contains("npm install -D playwright @axe-core/playwright lighthouse"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("setup web --yes"));
}

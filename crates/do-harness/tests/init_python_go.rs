//! Auto-selection of the Python and Go packs (issue #563).
//!
//! Python markers (`pyproject.toml`, `requirements.txt`, `setup.py`,
//! `setup.cfg`, `tox.ini`) and a `go.mod` select their packs from plain
//! `init`; explicit and declared language precedence is preserved; a healthy
//! fixture goes GREEN, a deliberately broken source goes RED, and an
//! unconfigured toolchain reports INCOMPLETE rather than a claimed green.
//! Every test is deterministic: it drives the real binary with stub
//! executables on a fake PATH so no ruff/go/pytest installation is required.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::Value;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

mod support;

use support::{harness, run};

/// Builds a private "core" bin directory of symlinks to the few host binaries
/// the harness and the test guard script need (`bash`, `dirname`, ...). Keeping
/// `/usr/bin` out of the fake PATH makes the test deterministic even on hosts
/// that have `ruff`/`pytest`/`go` installed system-wide.
fn make_core_bin(root: &Path) -> PathBuf {
    let core_dir = root.join("core-bin");
    fs::create_dir_all(&core_dir).unwrap();
    for name in [
        "bash", "sh", "dirname", "env", "printf", "grep", "cat", "tr", "head", "tail", "sort",
        "awk",
    ] {
        let src = PathBuf::from(format!("/usr/bin/{name}"));
        if src.exists() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(&src, core_dir.join(name)).unwrap();
        }
    }
    core_dir
}

/// `bin_dir` (test stubs) plus the private core bin, shadowing the host PATH.
fn path_with(bin_dir: &Path, core_dir: &Path) -> OsString {
    std::env::join_paths(
        std::iter::once(bin_dir.to_path_buf()).chain(std::iter::once(core_dir.to_path_buf())),
    )
    .expect("join_paths")
}

/// Writes an executable `sh` stub named `name` into `bin_dir` and marks it
/// executable. The stub consults its arguments so the version probe (used by
/// detection) always passes while the sensor invocation can be forced to fail.
fn write_stub(bin_dir: &Path, name: &str, body: &str) {
    let path = bin_dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).unwrap();
    }
}

fn write_ok_stub(bin_dir: &Path, name: &str) {
    write_stub(bin_dir, name, "exit 0");
}

fn write_ruff_stub(bin_dir: &Path) {
    write_stub(
        bin_dir,
        "ruff",
        "case \"$1\" in check|format) [ \"${RUFF_FAIL:-0}\" = \"1\" ] && exit 1;; esac; exit 0",
    );
}

fn write_pytest_stub(bin_dir: &Path) {
    write_stub(
        bin_dir,
        "pytest",
        "case \"$1\" in -q) [ \"${PYTEST_FAIL:-0}\" = \"1\" ] && exit 1;; esac; exit 0",
    );
}

fn write_go_stub(bin_dir: &Path) {
    write_stub(
        bin_dir,
        "go",
        "case \"$1\" in vet|test) [ \"${GO_FAIL:-0}\" = \"1\" ] && exit 1;; esac; exit 0",
    );
}

fn write_gofmt_stub(bin_dir: &Path) {
    write_stub(
        bin_dir,
        "gofmt",
        "case \"$1\" in -l) [ \"${GOFMT_FAIL:-0}\" = \"1\" ] && exit 1;; esac; exit 0",
    );
}

fn run_init(root: &Path, bin_dir: &Path) -> (Option<i32>, Value, String) {
    let core_dir = make_core_bin(root);
    let mut cmd = harness(root);
    cmd.env("PATH", path_with(bin_dir, &core_dir));
    cmd.args(["init", "--format", "json"]);
    let (code, stdout, stderr) = run(&mut cmd);
    let report = serde_json::from_str(&stdout).expect("parse init json report");
    (code, report, stderr)
}

fn run_init_with_env(
    root: &Path,
    bin_dir: &Path,
    key: &str,
    val: &str,
) -> (Option<i32>, Value, String) {
    let core_dir = make_core_bin(root);
    let mut cmd = harness(root);
    cmd.env("PATH", path_with(bin_dir, &core_dir));
    cmd.env(key, val);
    cmd.args(["init", "--format", "json"]);
    let (code, stdout, stderr) = run(&mut cmd);
    let report = serde_json::from_str(&stdout).expect("parse init json report");
    (code, report, stderr)
}

/// A minimal Python project: pyproject.toml with a configured mypy type checker.
fn write_python_fixture(root: &Path) {
    fs::write(
        root.join("pyproject.toml"),
        "[project]\nname = \"cli\"\nversion = \"0.1.0\"\n\n[tool.mypy]\nstrict = true\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/app.py"),
        "def main() -> None:\n    print(\"hi\")\n",
    )
    .unwrap();
}

/// A minimal Go module.
fn write_go_fixture(root: &Path) {
    fs::write(root.join("go.mod"), "module example.com/cli\n\ngo 1.22\n").unwrap();
    fs::write(root.join("main.go"), "package main\n\nfunc main() {}\n").unwrap();
}

#[test]
fn fresh_python_fixture_goes_green() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_python_fixture(root);
    let bin_dir = root.join("fake-bin");
    fs::create_dir_all(&bin_dir).unwrap();
    write_ruff_stub(&bin_dir);
    write_pytest_stub(&bin_dir);
    write_ok_stub(&bin_dir, "mypy");

    let (code, report, stderr) = run_init(root, &bin_dir);
    assert_eq!(code, Some(0), "init failed:\n{stderr}");
    assert_eq!(report["language"], serde_json::json!("python"));
    assert_eq!(report["baseline"]["state"], serde_json::json!("green"));

    let config = fs::read_to_string(root.join("do-harness.toml")).unwrap();
    assert!(config.contains("language = \"python\""));
    for name in ["lint", "format", "typecheck", "test"] {
        assert!(
            config.contains(&format!("name = \"{name}\"")),
            "missing {name} in: {config}"
        );
    }
    for candidate in report["candidates"].as_array().unwrap() {
        assert_eq!(
            candidate["included"],
            serde_json::json!(true),
            "{candidate:?}"
        );
    }
}

#[test]
fn broken_python_source_goes_red() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_python_fixture(root);
    let bin_dir = root.join("fake-bin");
    fs::create_dir_all(&bin_dir).unwrap();
    write_ruff_stub(&bin_dir);
    write_pytest_stub(&bin_dir);
    write_ok_stub(&bin_dir, "mypy");

    let (code, report, stderr) = run_init_with_env(root, &bin_dir, "RUFF_FAIL", "1");
    assert_eq!(code, Some(1), "init should exit 1 on RED:\n{stderr}");
    assert_eq!(report["baseline"]["state"], serde_json::json!("red"));
}

#[test]
fn python_unconfigured_toolchain_is_incomplete() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_python_fixture(root);
    // Only ruff is on the fake PATH; pytest and mypy are absent, so the test
    // and typecheck sensors are omitted and the baseline is INCOMPLETE.
    let bin_dir = root.join("fake-bin");
    fs::create_dir_all(&bin_dir).unwrap();
    write_ruff_stub(&bin_dir);

    let (code, report, stderr) = run_init(root, &bin_dir);
    assert_eq!(code, Some(0), "init failed:\n{stderr}");
    assert_eq!(report["language"], serde_json::json!("python"));
    assert_eq!(report["baseline"]["state"], serde_json::json!("incomplete"));

    let candidates = report["candidates"].as_array().unwrap();
    let test = candidates.iter().find(|c| c["name"] == "test").unwrap();
    assert_eq!(test["included"], serde_json::json!(false));
    let typecheck = candidates
        .iter()
        .find(|c| c["name"] == "typecheck")
        .unwrap();
    assert_eq!(typecheck["included"], serde_json::json!(false));

    let diagnostics = report["diagnostics"].as_array().unwrap();
    assert!(
        diagnostics
            .iter()
            .any(|d| d["code"] == "missing_python_tooling"),
        "expected missing_python_tooling diagnostic: {diagnostics:?}"
    );
}

#[test]
fn fresh_go_fixture_goes_green() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_go_fixture(root);
    let bin_dir = root.join("fake-bin");
    fs::create_dir_all(&bin_dir).unwrap();
    write_go_stub(&bin_dir);
    write_gofmt_stub(&bin_dir);
    write_ok_stub(&bin_dir, "staticcheck");

    let (code, report, stderr) = run_init(root, &bin_dir);
    assert_eq!(code, Some(0), "init failed:\n{stderr}");
    assert_eq!(report["language"], serde_json::json!("go"));
    assert_eq!(report["baseline"]["state"], serde_json::json!("green"));

    let config = fs::read_to_string(root.join("do-harness.toml")).unwrap();
    assert!(config.contains("language = \"go\""));
    for name in ["fmt", "vet", "test", "staticcheck"] {
        assert!(
            config.contains(&format!("name = \"{name}\"")),
            "missing {name} in: {config}"
        );
    }
}

#[test]
fn broken_go_source_goes_red() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_go_fixture(root);
    let bin_dir = root.join("fake-bin");
    fs::create_dir_all(&bin_dir).unwrap();
    write_go_stub(&bin_dir);
    write_gofmt_stub(&bin_dir);
    write_ok_stub(&bin_dir, "staticcheck");

    let (code, report, stderr) = run_init_with_env(root, &bin_dir, "GOFMT_FAIL", "1");
    assert_eq!(code, Some(1), "init should exit 1 on RED:\n{stderr}");
    assert_eq!(report["baseline"]["state"], serde_json::json!("red"));
}

#[test]
fn go_unconfigured_toolchain_is_incomplete() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_go_fixture(root);
    // Empty fake bin: the Go toolchain is absent, so every sensor is omitted
    // and the baseline is INCOMPLETE (core `test` omitted), not green.
    let bin_dir = root.join("fake-bin");
    fs::create_dir_all(&bin_dir).unwrap();

    let (code, report, stderr) = run_init(root, &bin_dir);
    assert_eq!(code, Some(0), "init failed:\n{stderr}");
    assert_eq!(report["language"], serde_json::json!("go"));
    assert_eq!(report["baseline"]["state"], serde_json::json!("incomplete"));

    for candidate in report["candidates"].as_array().unwrap() {
        assert_eq!(
            candidate["included"],
            serde_json::json!(false),
            "{candidate:?}"
        );
    }
    let diagnostics = report["diagnostics"].as_array().unwrap();
    assert!(
        diagnostics
            .iter()
            .any(|d| d["code"] == "missing_go_tooling"),
        "expected missing_go_tooling diagnostic: {diagnostics:?}"
    );
}

#[test]
fn explicit_language_go_wins_over_python_markers() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_python_fixture(root);
    let bin_dir = root.join("fake-bin");
    fs::create_dir_all(&bin_dir).unwrap();
    write_go_stub(&bin_dir);
    write_gofmt_stub(&bin_dir);

    let core_dir = make_core_bin(root);
    let mut cmd = harness(root);
    cmd.env("PATH", path_with(&bin_dir, &core_dir));
    cmd.args(["init", "--language", "go", "--format", "json"]);
    let (code, stdout, stderr) = run(&mut cmd);
    let report: Value = serde_json::from_str(&stdout).expect("parse init json report");
    assert_eq!(code, Some(0), "init failed:\n{stderr}");
    assert_eq!(report["language"], serde_json::json!("go"));
    assert_eq!(report["baseline"]["state"], serde_json::json!("green"));
}

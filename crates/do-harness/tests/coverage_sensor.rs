//! Integration tests for `scripts/check-coverage.sh`.
//!
//! Covers the sensor contract:
//! - `inventory` prints the deterministic layer table on a fresh `init`
//!   workspace without running tests, and exits 0.
//! - the llvm-cov path derives the line and branch percentages from the lcov
//!   report, prints the branch percentage when the report carries one, and
//!   always reports `FINDINGS: <line deficit>` (0 above target) for the blessed
//!   ratchet.
//! - a missing toolchain keeps the SKIP/FAIL contract (`CI=true` /
//!   `DO_HARNESS_REQUIRE_TOOLS=1`).
//! - the sensor measures the workspace that contains it, independent of cwd.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::doc_markdown)]

use std::path::Path;
use std::process::Command;

#[path = "../src/shell.rs"]
#[allow(dead_code)]
mod shell;

/// Fake `cargo-llvm-cov` whose lcov report yields 74.00% lines and 65.00%
/// branches, with nothing on stdout: only a report-derived parser can pass.
const LCOV_WITH_BRANCHES: &str = r#"std::fs::write("lcov.info", "SF:src/lib.rs\nLF:100\nLH:74\nBRF:20\nBRH:13\nend_of_record\n").unwrap();"#;

/// Fake `cargo-llvm-cov` whose report has lines only (65.00%), so the sensor
/// must report the deficit without inventing a branch number.
const LCOV_WITHOUT_BRANCHES: &str =
    r#"std::fs::write("lcov.info", "SF:src/lib.rs\nLF:100\nLH:65\nend_of_record\n").unwrap();"#;

/// Removes ambient git state so a spawned command cannot inherit this test
/// run's repository.
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
    ] {
        command.env_remove(key);
    }
    command
}

fn harness(root: &Path) -> Command {
    let mut cmd = isolated_command(env!("CARGO_BIN_EXE_do-harness"));
    cmd.arg("--root").arg(root);
    cmd
}

/// Compiles a fake `cargo-<name>` whose `main` executes `main_body`.
fn create_fake_tool(bin_dir: &Path, name: &str, main_body: &str) {
    let exe_name = format!("cargo-{name}{}", std::env::consts::EXE_SUFFIX);
    let target_path = bin_dir.join(exe_name);

    let src_path = bin_dir.join(format!("{name}.rs"));
    std::fs::write(&src_path, format!("fn main() {{ {main_body} }}\n")).unwrap();

    let status = Command::new("rustc")
        .arg(&src_path)
        .arg("-o")
        .arg(&target_path)
        .status()
        .expect("compile fake tool");
    assert!(status.success(), "rustc compilation of fake {name} failed");
}

/// Prepends `bin_dir` to the inherited PATH.
fn path_with(bin_dir: &Path) -> std::ffi::OsString {
    std::env::join_paths(
        std::iter::once(bin_dir.to_path_buf()).chain(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        )),
    )
    .unwrap()
}

/// Script path of the repository this test belongs to.
fn script_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/check-coverage.sh")
}

/// Inits a rust workspace at `root` and returns a fake-tool bin directory.
fn scaffold(root: &Path, main_body: &str) -> std::path::PathBuf {
    let init_status = harness(root).arg("init").status().expect("run init");
    assert!(init_status.success(), "do-harness init failed");

    let bin_dir = root.join("fake-bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    create_fake_tool(&bin_dir, "nextest", "");
    create_fake_tool(&bin_dir, "llvm-cov", main_body);
    bin_dir
}

#[test]
fn fresh_init_inventory_exits_zero_with_stable_table() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    // The inventory fast path needs no toolchain; the fakes are irrelevant here.
    let init_status = harness(root).arg("init").status().expect("run init");
    assert!(init_status.success(), "do-harness init failed");

    let mut cmd = shell::bash();
    for key in ["CI", "DO_HARNESS_REQUIRE_TOOLS"] {
        cmd.env_remove(key);
    }

    let output = cmd
        .arg(script_path())
        .arg("inventory")
        .arg(root)
        .output()
        .expect("run check-coverage.sh inventory");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "inventory on fresh init workspace should exit 0. Output:\nSTDOUT:\n{stdout}\nSTDERR:\n{stderr}"
    );
    assert!(
        stdout.contains("layer          | files | tests | unique(file, fn)"),
        "table header missing, got stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("crates/*/src   |     0 |     0 |                0"),
        "expected empty crates/*/src row, got:\n{stdout}"
    );
    assert!(
        stdout.contains("src/           |     1 |     1 |                1"),
        "expected src/ row with 1 file, 1 test, 1 unique, got:\n{stdout}"
    );
}

#[test]
fn llvm_cov_path_prints_branch_percentage_from_the_report() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    let bin_dir = scaffold(root, LCOV_WITH_BRANCHES);

    let output = shell::bash()
        .arg(script_path())
        .arg("llvm-cov")
        .arg(root)
        .env("PATH", path_with(&bin_dir))
        .output()
        .expect("run check-coverage.sh llvm-cov");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "sensor failed:\n{stdout}");
    assert!(
        stdout.contains(
            "check-coverage OK: Line coverage is 74.00% (>= 70%), Branch coverage is 65.00%."
        ),
        "expected line and branch coverage from the report, got:\n{stdout}"
    );
    assert!(
        stdout.contains("FINDINGS: 0"),
        "expected FINDINGS: 0, got:\n{stdout}"
    );
}

#[test]
fn llvm_cov_path_reports_the_deficit_without_branch_data() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    let bin_dir = scaffold(root, LCOV_WITHOUT_BRANCHES);

    let output = shell::bash()
        .arg(script_path())
        .arg("llvm-cov")
        .arg(root)
        .env("PATH", path_with(&bin_dir))
        .output()
        .expect("run check-coverage.sh llvm-cov");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "sensor failed:\n{stdout}");
    assert!(
        stdout.contains("WARN: Line coverage is 65.00% (target 70%, deficit 5%)."),
        "expected line deficit without branch data, got:\n{stdout}"
    );
    assert!(
        stdout.contains("FINDINGS: 5"),
        "expected FINDINGS: 5 (the ratchet number stays the line deficit), got:\n{stdout}"
    );
    assert!(
        !stdout.contains("Branch coverage"),
        "a report without branch records must not print a branch percentage:\n{stdout}"
    );
}

#[test]
fn missing_tools_skips_locally_and_fails_in_ci() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    let init_status = harness(root).arg("init").status().expect("run init");
    assert!(init_status.success(), "do-harness init failed");

    let mut cmd_skip = shell::bash();
    for key in ["CI", "DO_HARNESS_REQUIRE_TOOLS"] {
        cmd_skip.env_remove(key);
    }
    let output_skip = cmd_skip
        .arg(script_path())
        .arg("llvm-cov")
        .arg(root)
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("run check-coverage.sh llvm-cov");

    let stdout_skip = String::from_utf8_lossy(&output_skip.stdout);
    assert!(
        output_skip.status.success(),
        "should skip and exit 0 when tools are missing locally:\n{stdout_skip}"
    );
    assert!(
        stdout_skip.contains("SKIP: cargo-llvm-cov or cargo-nextest not installed"),
        "expected SKIP message, got:\n{stdout_skip}"
    );

    let output_fail = shell::bash()
        .arg(script_path())
        .arg("llvm-cov")
        .arg(root)
        .env("PATH", "/usr/bin:/bin")
        .env("CI", "true")
        .output()
        .expect("run check-coverage.sh llvm-cov");

    let stdout_fail = String::from_utf8_lossy(&output_fail.stdout);
    assert!(
        !output_fail.status.success(),
        "should fail and exit non-zero when tools are missing and CI=true:\n{stdout_fail}"
    );
    assert!(
        stdout_fail.contains("FAIL: cargo-llvm-cov and cargo-nextest are required"),
        "expected FAIL message, got:\n{stdout_fail}"
    );
}

#[test]
fn the_sensor_measures_the_workspace_that_contains_it() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    let bin_dir = scaffold(&workspace, LCOV_WITH_BRANCHES);

    // Run the scaffolded copy from an unrelated cwd, with no target argument:
    // the sensor must measure the workspace that owns the script.
    let elsewhere = temp_dir.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();

    let output = shell::bash()
        .arg(workspace.join("scripts/check-coverage.sh"))
        .arg("llvm-cov")
        .current_dir(&elsewhere)
        .env("PATH", path_with(&bin_dir))
        .output()
        .expect("run the scaffolded check-coverage.sh");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "sensor failed:\n{stdout}");
    assert!(
        workspace.join("lcov.info").is_file(),
        "the report must land in the workspace that owns the script:\n{stdout}"
    );
    assert!(
        !elsewhere.join("lcov.info").exists(),
        "the sensor must not measure the caller's working directory"
    );
}

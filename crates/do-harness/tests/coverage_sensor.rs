//! Integration test suite for `scripts/check-coverage.sh` (coverage sensor).
//!
//! Verifies acceptance criteria:
//! - `inventory` on a fresh init rust workspace exits 0 with a stable layer table and no test execution.
//! - `llvm-cov` path emits `FINDINGS: <n>` (deficit) and `FINDINGS: 0` on green; branch % printed when parseable, never breaks ratchet when unparseable.
//! - Missing cargo-llvm-cov/cargo-nextest keeps current SKIP/FAIL contract (`CI=true` / `DO_HARNESS_REQUIRE_TOOLS=1`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::doc_markdown)]

use std::path::Path;
use std::process::Command;

#[path = "../src/shell.rs"]
#[allow(dead_code)]
mod shell;

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

fn create_fake_tool(bin_dir: &Path, name: &str, output_str: &str) {
    let exe_name = format!("cargo-{name}{}", std::env::consts::EXE_SUFFIX);
    let target_path = bin_dir.join(exe_name);

    let src = format!(r"fn main() {{ print!({output_str:?}); }}");
    let src_path = bin_dir.join(format!("{name}.rs"));
    std::fs::write(&src_path, src).unwrap();

    let status = Command::new("rustc")
        .arg(&src_path)
        .arg("-o")
        .arg(&target_path)
        .status()
        .expect("compile fake tool");
    assert!(status.success(), "rustc compilation of fake {name} failed");
}

#[test]
fn fresh_init_inventory_exits_zero_with_stable_table() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let script_path = repo_root.join("scripts/check-coverage.sh");

    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();

    let init_status = harness(root).arg("init").status().expect("run init");
    assert!(init_status.success(), "do-harness init failed");

    let mut cmd = shell::bash();
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CEILING_DIRECTORIES",
        "GIT_NAMESPACE",
        "GIT_PREFIX",
        "CI",
        "DO_HARNESS_REQUIRE_TOOLS",
    ] {
        cmd.env_remove(key);
    }

    let output = cmd
        .arg(&script_path)
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
fn missing_tools_skips_locally_and_fails_in_ci() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let script_path = repo_root.join("scripts/check-coverage.sh");

    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();

    let mut cmd_skip = shell::bash();
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CEILING_DIRECTORIES",
        "GIT_NAMESPACE",
        "GIT_PREFIX",
        "CI",
        "DO_HARNESS_REQUIRE_TOOLS",
    ] {
        cmd_skip.env_remove(key);
    }
    cmd_skip.env("PATH", "/usr/bin:/bin");

    let output_skip = cmd_skip
        .arg(&script_path)
        .arg("llvm-cov")
        .arg(root)
        .output()
        .expect("run check-coverage.sh llvm-cov");

    let stdout_skip = String::from_utf8_lossy(&output_skip.stdout);
    assert!(
        output_skip.status.success(),
        "should skip and exit 0 when tools missing locally"
    );
    assert!(
        stdout_skip.contains("SKIP: cargo-llvm-cov or cargo-nextest not installed"),
        "expected SKIP message, got:\n{stdout_skip}"
    );

    let mut cmd_fail = shell::bash();
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
        cmd_fail.env_remove(key);
    }
    cmd_fail.env("PATH", "/usr/bin:/bin");
    cmd_fail.env("CI", "true");

    let output_fail = cmd_fail
        .arg(&script_path)
        .arg("llvm-cov")
        .arg(root)
        .output()
        .expect("run check-coverage.sh llvm-cov");

    let stdout_fail = String::from_utf8_lossy(&output_fail.stdout);
    assert!(
        !output_fail.status.success(),
        "should fail and exit non-zero when tools missing and CI=true"
    );
    assert!(
        stdout_fail.contains("FAIL: cargo-llvm-cov and cargo-nextest are required"),
        "expected FAIL message, got:\n{stdout_fail}"
    );
}

#[test]
fn llvm_cov_parsing_with_and_without_branch_column() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let script_path = repo_root.join("scripts/check-coverage.sh");

    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();

    let init_status = harness(root).arg("init").status().expect("run init");
    assert!(init_status.success(), "do-harness init failed");

    let bin_dir = temp_dir.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();

    create_fake_tool(&bin_dir, "nextest", "");

    let cov_output_with_branch = r"Filename                      Regions    Missed Regions     Cover   Functions  Missed Functions  Executed       Lines      Missed Lines     Cover    Branches   Missed Branches     Cover
-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
src/lib.rs                         10                 2    80.00%           3                 0   100.00%          25                 5    80.00%           4                 1    75.00%
TOTAL                              50                10    80.00%          15                 0   100.00%         100                26    74.00%          20                 7    65.00%
";

    create_fake_tool(&bin_dir, "llvm-cov", cov_output_with_branch);

    let mut cmd = shell::bash();
    let path_env = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    cmd.env("PATH", &path_env);

    let output = cmd
        .arg(&script_path)
        .arg("llvm-cov")
        .arg(root)
        .output()
        .expect("run check-coverage.sh with branch output");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(
            "check-coverage OK: Line coverage is 74.00% (>= 70%), Branch coverage is 65.00%."
        ),
        "expected line and branch coverage in stdout, got:\n{stdout}"
    );
    assert!(
        stdout.contains("FINDINGS: 0"),
        "expected FINDINGS: 0, got:\n{stdout}"
    );

    let cov_output_no_branch = r"Filename                      Regions    Missed Regions     Cover   Functions  Missed Functions  Executed       Lines      Missed Lines     Cover
-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
src/lib.rs                         10                 2    80.00%           3                 0   100.00%          25                 5    80.00%
TOTAL                              50                10    80.00%          15                 0   100.00%         100                35    65.00%
";

    create_fake_tool(&bin_dir, "llvm-cov", cov_output_no_branch);

    let mut cmd2 = shell::bash();
    cmd2.env("PATH", &path_env);

    let output2 = cmd2
        .arg(&script_path)
        .arg("llvm-cov")
        .arg(root)
        .output()
        .expect("run check-coverage.sh without branch output");

    let stdout2 = String::from_utf8_lossy(&output2.stdout);
    assert!(
        stdout2.contains("WARN: Line coverage is 65.00% (target 70%, deficit 5%)."),
        "expected line coverage warn without branch info, got:\n{stdout2}"
    );
    assert!(
        stdout2.contains("FINDINGS: 5"),
        "expected FINDINGS: 5 (deficit), got:\n{stdout2}"
    );
}

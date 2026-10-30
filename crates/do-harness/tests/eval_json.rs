//! Integration tests verifying that `do-harness eval --format json` emits
//! parseable JSON on stdout across all outcomes (WF-02).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

#[cfg(unix)]
const EXEC_PERMS: u32 = 0o755;

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
}

fn gate_script_path() -> PathBuf {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    repo_root.join(".agents/skills/skill-creator/scripts/quick_validate.py")
}

fn fixture(name: &str, assertions: &[&str]) -> Fixture {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("repo");
    let skills_root = root.join(".agents/skills");
    let scripts = skills_root.join("skill-creator/scripts");
    fs::create_dir_all(&scripts).expect("create scripts dir");
    fs::copy(gate_script_path(), scripts.join("quick_validate.py"))
        .expect("copy quick_validate.py");

    let skill_dir = skills_root.join(name);
    let evals_dir = skill_dir.join("evals");
    fs::create_dir_all(&evals_dir).expect("create evals dir");

    let skill_md = format!(
        "---\nname: {name}\ndescription: A valid skill description that is comfortably longer than forty characters.\nlicense: MIT\n---\n\n# {name}\n"
    );
    fs::write(skill_dir.join("SKILL.md"), skill_md).expect("write SKILL.md");

    let assertions_json: Vec<String> = assertions.iter().map(|a| format!("\"{a}\"")).collect();
    let evals_json = format!(
        r#"{{
  "skill_name": "{name}",
  "evals": [
    {{
      "id": 1,
      "prompt": "run test",
      "expected_output": "output",
      "files": [],
      "assertions": [{}]
    }}
  ]
}}"#,
        assertions_json.join(", ")
    );
    fs::write(evals_dir.join("evals.json"), evals_json).expect("write evals.json");

    let walkthrough = r#"#!/bin/sh
if [ "$FLAG_FAIL" = "1" ]; then
  exit 0
fi
echo ok > result_ok.txt
exit 0
"#;
    let wt_path = evals_dir.join("walkthrough.sh");
    fs::write(&wt_path, walkthrough).expect("write walkthrough.sh");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&wt_path).expect("metadata").permissions();
        perms.set_mode(EXEC_PERMS);
        fs::set_permissions(&wt_path, perms).expect("chmod +x");
    }

    Fixture { _temp: temp, root }
}

fn harness(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    cmd.arg("--root").arg(root);
    cmd
}

fn run_cmd(cmd: &mut Command) -> (Option<i32>, String, String) {
    let output = cmd.output().expect("spawn do-harness");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn eval_json_warning_routes_diagnostics_to_stderr_and_stdout_is_valid_json() {
    let fx = fixture("warn-skill", &["exists:result_ok.txt"]);
    let (code, stdout, stderr) = run_cmd(harness(&fx.root).args([
        "eval",
        "--format",
        "json",
        "--no-lift",
        "--skill",
        "warn-skill",
    ]));

    assert_eq!(
        code,
        Some(0),
        "expected success: stdout={stdout}\nstderr={stderr}"
    );
    assert!(
        stderr.contains("fixture-WARN"),
        "stderr should contain warning: {stderr}"
    );
    assert!(
        !stdout.contains("fixture-WARN"),
        "stdout should not contain diagnostic: {stdout}"
    );

    let parsed: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|err| panic!("stdout must parse as JSON: {err}\nstdout: {stdout}"));
    let array = parsed.as_array().expect("stdout must be a JSON array");
    assert_eq!(array.len(), 1);
    assert_eq!(array[0]["name"], "warn-skill");
    assert_eq!(array[0]["pass_rate"], 1.0);
}

#[test]
fn eval_json_strict_fixtures_routes_miss_to_stderr_and_stdout_is_valid_json() {
    let fx = fixture("strict-skill", &["exists:result_ok.txt"]);
    let (code, stdout, stderr) = run_cmd(harness(&fx.root).args([
        "eval",
        "--format",
        "json",
        "--no-lift",
        "--strict-fixtures",
        "--skill",
        "strict-skill",
    ]));

    assert_eq!(code, Some(1), "expected failure exit code 1");
    assert!(
        stderr.contains("FIXTURE-MISS"),
        "stderr should contain miss diagnostic: {stderr}"
    );
    assert!(
        !stdout.contains("FIXTURE-MISS"),
        "stdout should not contain diagnostic: {stdout}"
    );

    let parsed: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|err| panic!("stdout must parse as JSON: {err}\nstdout: {stdout}"));
    let array = parsed.as_array().expect("stdout must be a JSON array");
    assert_eq!(array.len(), 1);
    assert_eq!(array[0]["name"], "strict-skill");
}

#[test]
fn eval_json_bless_routes_diagnostics_to_stderr_and_stdout_is_valid_json() {
    let fx = fixture("bless-skill", &["exists:result_ok.txt"]);
    let (code, stdout, stderr) = run_cmd(harness(&fx.root).args([
        "eval",
        "--format",
        "json",
        "--no-lift",
        "--bless",
        "--approver",
        "test-approver",
        "--skill",
        "bless-skill",
    ]));

    assert_eq!(
        code,
        Some(0),
        "expected success exit code: {stdout}\n{stderr}"
    );
    assert!(
        stderr.contains("blessed;"),
        "stderr should contain bless diagnostic: {stderr}"
    );
    assert!(
        !stdout.contains("blessed;"),
        "stdout should not contain diagnostic: {stdout}"
    );

    let parsed: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|err| panic!("stdout must parse as JSON: {err}\nstdout: {stdout}"));
    let array = parsed.as_array().expect("stdout must be a JSON array");
    assert_eq!(array.len(), 1);
    assert_eq!(array[0]["name"], "bless-skill");
}

#[test]
fn eval_json_grader_drift_routes_to_stderr_and_stdout_is_valid_json() {
    let fx = fixture("drift-skill", &["exists:result_ok.txt"]);
    // First bless baseline
    let (b_code, _, b_err) = run_cmd(harness(&fx.root).args([
        "eval",
        "--no-lift",
        "--bless",
        "--approver",
        "test-approver",
        "--skill",
        "drift-skill",
    ]));
    assert_eq!(b_code, Some(0), "bless failed: {b_err}");

    // Induce drift by modifying walkthrough.sh
    let wt = fx
        .root
        .join(".agents/skills/drift-skill/evals/walkthrough.sh");
    fs::write(&wt, "#!/bin/sh\n# modified for drift\nexit 0\n").expect("write drift");

    let (code, stdout, stderr) = run_cmd(harness(&fx.root).args([
        "eval",
        "--format",
        "json",
        "--no-lift",
        "--skill",
        "drift-skill",
    ]));

    assert_eq!(code, Some(1), "expected failure for grader drift");
    assert!(
        stderr.contains("grader-DRIFT"),
        "stderr should contain drift diagnostic: {stderr}"
    );
    assert!(
        !stdout.contains("grader-DRIFT"),
        "stdout should not contain diagnostic: {stdout}"
    );

    let parsed: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|err| panic!("stdout must parse as JSON: {err}\nstdout: {stdout}"));
    let array = parsed.as_array().expect("stdout must be a JSON array");
    // Since drift halted before report was produced, array is empty JSON array
    assert!(
        array.is_empty(),
        "expected empty JSON array for drifted skill"
    );
}

#[test]
fn eval_json_bar_miss_routes_to_stderr_and_stdout_is_valid_json() {
    let fx = fixture("bar-skill", &["exists:result_ok.txt"]);
    // First bless baseline
    let (b_code, _, b_err) = run_cmd(harness(&fx.root).args([
        "eval",
        "--no-lift",
        "--bless",
        "--approver",
        "test-approver",
        "--skill",
        "bar-skill",
    ]));
    assert_eq!(b_code, Some(0), "bless failed: {b_err}");

    // Induce assertion failure without grader drift via env var
    let (code, stdout, stderr) = run_cmd(harness(&fx.root).env("FLAG_FAIL", "1").args([
        "eval",
        "--format",
        "json",
        "--no-lift",
        "--skill",
        "bar-skill",
    ]));

    assert_eq!(code, Some(1), "expected failure for bar miss");
    assert!(
        stderr.contains("BAR-MISS"),
        "stderr should contain BAR-MISS diagnostic: {stderr}"
    );
    assert!(
        !stdout.contains("BAR-MISS"),
        "stdout should not contain diagnostic: {stdout}"
    );

    let parsed: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|err| panic!("stdout must parse as JSON: {err}\nstdout: {stdout}"));
    let array = parsed.as_array().expect("stdout must be a JSON array");
    assert_eq!(array.len(), 1);
    assert_eq!(array[0]["name"], "bar-skill");
    assert_eq!(array[0]["pass_rate"], 0.0);
}

#[test]
fn eval_json_dry_run_routes_diagnostic_to_stderr_and_stdout_is_valid_json() {
    let fx = fixture("dry-skill", &["exists:result_ok.txt"]);
    let (code, stdout, stderr) = run_cmd(harness(&fx.root).args([
        "eval",
        "--format",
        "json",
        "--dry-run",
        "--no-lift",
        "--skill",
        "dry-skill",
    ]));

    assert_eq!(code, Some(0), "expected success for dry run");
    assert!(
        stderr.contains("dry run"),
        "stderr should contain dry run diagnostic: {stderr}"
    );
    assert!(
        !stdout.contains("dry run"),
        "stdout should not contain diagnostic: {stdout}"
    );

    let parsed: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|err| panic!("stdout must parse as JSON: {err}\nstdout: {stdout}"));
    let array = parsed.as_array().expect("stdout must be a JSON array");
    assert!(array.is_empty(), "expected empty JSON array for dry run");
}

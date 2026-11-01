//! Integration tests for opt-in repeated agent-eval trials (WF-12).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

mod support;

use support::harness;

const TRIALS_COUNT: usize = 3;
const EXPECTED_PASS_RATE: f64 = 0.6667;
const RATE_TOLERANCE: f64 = 0.01;

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
      "prompt": "run task",
      "expected_output": "output",
      "files": [],
      "assertions": [{}]
    }}
  ]
}}"#,
        assertions_json.join(", ")
    );
    fs::write(evals_dir.join("evals.json"), evals_json).expect("write evals.json");

    Fixture { _temp: temp, root }
}

#[test]
fn eval_trials_records_controlled_pass_fail_pass_outcomes() {
    let fx = fixture("trial-skill", &["exists:answer.txt"]);
    let state_dir = tempfile::tempdir().unwrap();
    let counter_file = state_dir.path().join("attempt_counter.txt");
    fs::write(&counter_file, "1").unwrap();

    // The agent command reads the shared counter:
    // Attempt 1: writes answer.txt, writes trial_residue.txt, exits 0 (PASS)
    // Attempt 2: does NOT write answer.txt, exits 1 (FAIL)
    // Attempt 3: checks trial_residue.txt must NOT exist (fresh sandbox), writes answer.txt, exits 0 (PASS)
    let agent_cmd = format!(
        r#"
COUNT_FILE="{}"
ATTEMPT=$(cat "$COUNT_FILE")
NEXT=$((ATTEMPT + 1))
echo "$NEXT" > "$COUNT_FILE"

if [ -f trial_residue.txt ]; then
    echo "ERROR: residue crossed sandbox boundary!" >&2
    exit 99
fi
echo residue > trial_residue.txt

if [ "$ATTEMPT" = "2" ]; then
    echo "deliberate failure on attempt 2" >&2
    exit 1
fi

echo "success" > answer.txt
exit 0
"#,
        counter_file.display()
    );

    let output = harness(&fx.root)
        .args([
            "eval",
            "--skill",
            "trial-skill",
            "--no-lift",
            "--format",
            "json",
            "--trials",
            "3",
            "--agent-cmd",
            &agent_cmd,
        ])
        .output()
        .expect("run eval --trials 3");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "eval --trials 3 should succeed even with non-1.0 pass rate: stdout={stdout}\nstderr={stderr}"
    );

    let parsed: Value = serde_json::from_str(&stdout).unwrap();
    let report = &parsed[0];

    assert_eq!(report["name"], "trial-skill");
    assert_eq!(report["mode"], "agent");
    assert_eq!(report["trials"], TRIALS_COUNT);
    assert_eq!(
        report["graded"], TRIALS_COUNT,
        "must grade 3 total assertions across 3 trials"
    );
    assert_eq!(
        report["passed"], 2,
        "must report 2 passed assertions out of 3 trials"
    );
    // pass_rate must be 2/3 ≈ 0.6667
    let pass_rate = report["pass_rate"].as_f64().unwrap();
    assert!(
        (pass_rate - EXPECTED_PASS_RATE).abs() < RATE_TOLERANCE,
        "pass rate must be ~0.67, got {pass_rate}"
    );

    // missing baseline remains unmeasured
    assert!(report["lift"].is_null());
    assert!(report["without_pass_rate"].is_null());

    let cases = report["cases"].as_array().unwrap();
    assert_eq!(
        cases.len(),
        TRIALS_COUNT,
        "must record 3 case trial outcomes"
    );
    assert_eq!(cases[0]["trial"], 1);
    assert_eq!(cases[0]["passed"], 1);
    assert_eq!(cases[1]["trial"], 2);
    assert_eq!(cases[1]["passed"], 0);
    assert_eq!(cases[2]["trial"], TRIALS_COUNT);
    assert_eq!(cases[2]["passed"], 1);
}

#[test]
fn eval_default_trials_is_one() {
    let fx = fixture("default-trial-skill", &["exists:answer.txt"]);
    let agent_cmd = "echo ok > answer.txt";

    let output = harness(&fx.root)
        .args([
            "eval",
            "--skill",
            "default-trial-skill",
            "--no-lift",
            "--format",
            "json",
            "--agent-cmd",
            agent_cmd,
        ])
        .output()
        .expect("run eval default");

    assert!(output.status.success());
    let parsed: Value = serde_json::from_slice(&output.stdout).unwrap();
    let report = &parsed[0];
    assert_eq!(report["graded"], 1);
    assert_eq!(report["passed"], 1);
    assert_eq!(report["trials"], 1);
}

#[test]
fn eval_trials_zero_is_rejected_as_usage_error() {
    let fx = fixture("zero-trial-skill", &["exists:answer.txt"]);
    let output = harness(&fx.root)
        .args([
            "eval",
            "--skill",
            "zero-trial-skill",
            "--no-lift",
            "--trials",
            "0",
            "--agent-cmd",
            "true",
        ])
        .output()
        .expect("run eval --trials 0");

    assert_eq!(output.status.code(), Some(2), "trials=0 must exit code 2");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("trials") && stderr.contains('1'));
}

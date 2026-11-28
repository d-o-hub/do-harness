//! Real CLI coverage for explicitly mapped verification scenarios.

#![allow(clippy::unwrap_used)]

use serde_json::Value;
use std::path::{Path, PathBuf};

mod support;

use support::{git, harness, run};

fn repo(test_id: &str, enforce: bool) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::create_dir(root.join("plans")).unwrap();
    std::fs::write(
        root.join("plans/contract.md"),
        "# Reject invalid attempts\n",
    )
    .unwrap();
    std::fs::write(root.join("plans/verification-scenarios.json"), format!(r#"{{
        "schema_version":1,"signal_set":"verification","enforce":{enforce},
        "requirements":[{{"id":"task.validate","source":"plans/contract.md","scenarios":[{{
            "id":"task.invalid","expected":"reject","sensor":"contract","test_id":"{test_id}"}}]}}]}}"#)).unwrap();
    std::fs::write(root.join("do-harness.toml"), r#"
        language = "generic"
        [signal-sets]
        verification = ["contract"]
        [[sensors]]
        name = "contract"
        argv = ["sh", "-c", "printf '%s\\n' 'COVERAGE: {\"scenario_schema_version\":1,\"tests\":[{\"id\":\"task::reject\",\"branch\":\"reject\",\"outcome\":\"passed\"}]}'"]
        when-changed = ["src/**/*.rs"]
    "#).unwrap();
    git(&root, &["init", "-q"]);
    git(&root, &["add", "--all"]);
    git(&root, &["commit", "-qm", "test: scenario contract"]);
    (dir, root)
}

fn status(root: &Path, expected: i32) -> Value {
    let (code, out, err) =
        run(harness(root).args(["status", "--set", "verification", "--format", "json"]));
    assert_eq!(code, Some(expected), "{out}\n{err}");
    serde_json::from_str(&out).unwrap()
}

#[test]
fn the_exact_executed_scenario_is_sealed_and_its_spec_is_a_freshness_input() {
    let (_dir, root) = repo("task::reject", true);
    let (code, out, err) = run(harness(&root).args(["verify", "--set", "verification"]));
    assert_eq!(code, Some(0), "{out}\n{err}");
    let report = status(&root, 0);
    let result = &report["evidence"]["scenario_results"][0];
    assert_eq!(result["scenario"], "task.invalid");
    assert_eq!(result["test_id"], "task::reject");
    assert_eq!(result["outcome"], "passed");

    std::fs::write(
        root.join("plans/contract.md"),
        "# Reject incompatible attempts\n",
    )
    .unwrap();
    assert_eq!(status(&root, 1)["state"], "stale");
    let (code, out, err) =
        run(harness(&root).args(["verify", "--set", "verification", "--changed"]));
    assert_eq!(code, Some(0), "{out}\n{err}");
    let (code, out, err) = run(harness(&root).args(["verify", "--set", "verification"]));
    assert_eq!(code, Some(0), "{out}\n{err}");
    assert_eq!(status(&root, 0)["state"], "green");
}

#[test]
fn a_passing_sensor_does_not_prove_a_test_that_never_ran() {
    for enforce in [false, true] {
        let (_dir, root) = repo("task::missing", enforce);
        let expected = i32::from(enforce);
        let (code, out, err) = run(harness(&root).args(["verify", "--set", "verification"]));
        assert_eq!(code, Some(expected), "{out}\n{err}");
        let result = status(&root, expected);
        assert_eq!(
            result["evidence"]["scenario_results"][0]["outcome"],
            "not_run"
        );
    }
}

//! Stack-neutral onboarding contract fixtures (issue #478).
//!
//! These drive the real CLI against compact real projects for the supported
//! stacks. The generic arm is explicitly a fake-command runner fixture; the
//! Rust, Node, Python and Go arms are real ecosystem smoke tests. Every arm
//! proves the same contract: a green run is evidence, a planted behavioral
//! defect fails the named sensor, and a missing tool is `unrunnable` rather
//! than a pass.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

mod support;

use support::harness;

fn write(root: &Path, relative: &str, body: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, body).unwrap();
}

fn git_commit(root: &Path) {
    support::git(root, &["init", "-q"]);
    support::git(root, &["add", "-A"]);
    support::git(root, &["commit", "-qm", "chore(fixture): initial"]);
}

fn run_json(root: &Path, args: &[&str]) -> (Option<i32>, Value, String) {
    let (code, stdout, stderr) = support::run(harness(root).args(args));
    let value = serde_json::from_str(&stdout).unwrap_or_else(|_| {
        panic!(
            "expected JSON from `do-harness {}`:\nstdout:\n{stdout}\nstderr:\n{stderr}",
            args.join(" ")
        )
    });
    (code, value, stderr)
}

fn init_generic(root: &Path) {
    let (code, _, stderr) = run_json(
        root,
        &[
            "init",
            "--language",
            "generic",
            "--no-seed",
            "--minimal",
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(0), "generic init failed:\n{stderr}");
}

fn verify(root: &Path) -> (Option<i32>, Value) {
    let (code, value, _) = run_json(
        root,
        &[
            "verify",
            "--set",
            "verification",
            "--strict",
            "--record",
            "--format",
            "json",
        ],
    );
    (code, value)
}

fn status(root: &Path) -> Value {
    let (code, value, stderr) = run_json(
        root,
        &["status", "--set", "verification", "--format", "json"],
    );
    assert!(
        matches!(code, Some(0 | 1)),
        "status must report a verdict, not fail:\n{stderr}"
    );
    value
}

fn assert_green_then_defect(root: &Path, defect: &dyn Fn()) {
    let (code, report) = verify(root);
    assert_eq!(code, Some(0), "green run failed: {report}");
    assert_eq!(status(root)["state"], serde_json::json!("green"));
    defect();
    assert_ne!(status(root)["state"], serde_json::json!("green"));
    let (code, report) = verify(root);
    assert_eq!(code, Some(1), "planted defect must fail: {report}");
    assert!(
        report["failed"]
            .as_array()
            .is_some_and(|failed| failed.iter().any(|name| name == "test")),
        "the test sensor must be named: {report}"
    );
}

fn write_script_config(root: &Path, script: &str, when_changed: &[&str]) {
    let globs = when_changed
        .iter()
        .map(|glob| format!("\"{glob}\""))
        .collect::<Vec<_>>()
        .join(", ");
    write(
        root,
        "do-harness.toml",
        &format!(
            "[signal-sets]\nverification = [\"test\"]\n\n[[sensors]]\nname = \"test\"\nargv = [\"bash\", \"{script}\"]\nwhen-changed = [{globs}]\n"
        ),
    );
}

#[test]
fn generic_fake_runner_contract_is_labeled_and_enforced() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
        root,
        "do-harness.toml",
        "[signal-sets]\nverification = [\"probe\"]\n\n[[sensors]]\nname = \"probe\"\nargv = [\"true\"]\n",
    );
    git_commit(root);
    let (code, report) = verify(root);
    assert_eq!(code, Some(0), "{report}");
    assert_ne!(report["vacuous"], serde_json::json!(true));
    assert_eq!(report["sensors"].as_array().unwrap().len(), 1);
    write(
        root,
        "do-harness.toml",
        "[signal-sets]\nverification = [\"probe\"]\n\n[[sensors]]\nname = \"probe\"\nargv = [\"bash\", \"-c\", \"exit 1\"]\n",
    );
    let (code, report) = verify(root);
    assert_eq!(code, Some(1));
    assert!(
        report["failed"]
            .as_array()
            .unwrap()
            .iter()
            .any(|name| name == "probe")
    );
}

#[test]
fn python_fixture_passes_then_fails_and_missing_tool_is_unrunnable() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "app.py", "def add(a, b):\n    return a + b\n");
    write(
        root,
        "tests/test_app.py",
        "import unittest\nfrom app import add\n\nclass AppTest(unittest.TestCase):\n    def test_add(self):\n        self.assertEqual(add(1, 2), 3)\n",
    );
    write(
        root,
        "scripts/check-test.sh",
        "#!/usr/bin/env bash\nset -euo pipefail\nout=\"$(python3 -m unittest discover -s tests -v 2>&1)\"\nprintf '%s\\n' \"$out\"\n! grep -q '^Ran 0 tests' <<<\"$out\"\n",
    );
    write_script_config(
        root,
        "scripts/check-test.sh",
        &["**/*.py", "pyproject.toml"],
    );
    write(
        root,
        ".gitignore",
        ".do-harness/\n.agents/events/\n__pycache__/\n*.pyc\n",
    );
    git_commit(root);
    init_generic(root);
    assert_green_then_defect(root, &|| {
        write(
            root,
            "app.py",
            "def add(a, b):\n    return a - b + 0  # planted defect\n",
        );
    });
    write_script_config(root, "scripts/check-test.sh", &["**/*.py"]);
    write(
        root,
        "do-harness.toml",
        "[signal-sets]\nverification = [\"test\"]\n\n[[sensors]]\nname = \"test\"\nargv = [\"definitely_missing_tool_xyz\"]\n",
    );
    let (code, report) = verify(root);
    assert_eq!(code, Some(2), "missing tool must be exit 2: {report}");
    assert!(
        report["unrunnable"]
            .as_array()
            .unwrap()
            .iter()
            .any(|name| name == "test")
    );
}

#[test]
fn go_fixture_passes_then_fails_and_zero_tests_fail() {
    if Command::new("go").arg("version").output().is_err() {
        eprintln!("SKIP: go is unavailable; the missing-tool path is covered by python_fixture");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "go.mod", "module example.com/fixture\n\ngo 1.21\n");
    write(
        root,
        "app.go",
        "package fixture\n\nfunc Add(a, b int) int {\n\treturn a + b\n}\n",
    );
    write(
        root,
        "app_test.go",
        "package fixture\n\nimport \"testing\"\n\nfunc TestAdd(t *testing.T) {\n\tif Add(1, 2) != 3 {\n\t\tt.Fatal(\"bad sum\")\n\t}\n}\n",
    );
    write(
        root,
        "scripts/check-test.sh",
        "#!/usr/bin/env bash\nset -euo pipefail\nout=\"$(go test ./... 2>&1)\"\nprintf '%s\\n' \"$out\"\n! grep -q '\\[no test files\\]' <<<\"$out\"\n",
    );
    write_script_config(
        root,
        "scripts/check-test.sh",
        &["**/*.go", "go.mod", "go.sum"],
    );
    write(root, ".gitignore", ".do-harness/\n.agents/events/\n");
    git_commit(root);
    init_generic(root);
    assert_green_then_defect(root, &|| {
        write(
            root,
            "app.go",
            "package fixture\n\nfunc Add(a, b int) int {\n\treturn a - b\n}\n",
        );
    });
    // A package with no tests must fail, not pass vacuously.
    write(
        root,
        "app.go",
        "package fixture\n\nfunc Add(a, b int) int {\n\treturn a + b\n}\n",
    );
    std::fs::remove_file(root.join("app_test.go")).unwrap();
    let (code, report) = verify(root);
    assert_eq!(code, Some(1), "zero tests must fail: {report}");
}

#[test]
fn node_fixture_passes_then_fails() {
    if Command::new("node").arg("--version").output().is_err() {
        eprintln!("SKIP: node is unavailable");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
        root,
        "package.json",
        "{\"name\":\"fixture\",\"version\":\"1.0.0\",\"scripts\":{\"test\":\"node --test\"}}\n",
    );
    write(
        root,
        "app.js",
        "export function add(a, b) { return a + b; }\n",
    );
    write(
        root,
        "test/app.test.js",
        "import test from 'node:test';\nimport assert from 'node:assert/strict';\nimport { add } from '../app.js';\ntest('add', () => { assert.equal(add(1, 2), 3); });\n",
    );
    write(
        root,
        ".gitignore",
        ".do-harness/\n.agents/events/\nnode_modules/\npackage-lock.json\n",
    );
    git_commit(root);
    let (code, report, stderr) = run_json(
        root,
        &["init", "--no-seed", "--minimal", "--format", "json"],
    );
    assert_eq!(code, Some(0), "node init failed:\n{stderr}");
    assert_eq!(report["language"], serde_json::json!("node"));
    assert_green_then_defect(root, &|| {
        write(
            root,
            "app.js",
            "export function add(a, b) { return a - b; }\n",
        );
    });
}

#[test]
fn rust_fixture_passes_then_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
        root,
        "Cargo.toml",
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    write(
        root,
        "src/lib.rs",
        "pub fn answer() -> u32 {\n    42\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn answer_is_stable() {\n        assert_eq!(super::answer(), 42);\n    }\n}\n",
    );
    git_commit(root);
    let (code, report, stderr) = run_json(
        root,
        &["init", "--no-seed", "--minimal", "--format", "json"],
    );
    assert_eq!(code, Some(0), "rust init failed:\n{stderr}");
    assert_eq!(report["language"], serde_json::json!("rust"));
    assert_green_then_defect(root, &|| {
        write(root, "src/lib.rs", "pub fn answer() -> u32 {\n    0\n}\n");
    });
}

#[test]
fn unknown_stack_init_is_vacuous_and_diagnosed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
        root,
        "pyproject.toml",
        "[project]\nname = \"fixture\"\nversion = \"1.0.0\"\n",
    );
    git_commit(root);
    let (code, report, stderr) = run_json(
        root,
        &["init", "--no-seed", "--minimal", "--format", "json"],
    );
    assert_eq!(code, Some(0), "unknown-stack init failed:\n{stderr}");
    assert_eq!(report["language"], serde_json::json!("generic"));
    assert_eq!(report["baseline"]["state"], serde_json::json!("vacuous"));
    assert!(
        report["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|diagnostic| diagnostic["code"] == "generic_stack_selected"),
        "unfamiliar stack must be diagnosed: {report}"
    );
    let text_dir = tempfile::tempdir().unwrap();
    let text_root = text_dir.path();
    write(
        text_root,
        "pyproject.toml",
        "[project]\nname = \"fixture\"\nversion = \"1.0.0\"\n",
    );
    git_commit(text_root);
    let (_, text, _) = support::run(harness(text_root).args([
        "init",
        "--no-seed",
        "--minimal",
        "--format",
        "text",
    ]));
    assert!(
        text.contains("generic_stack_selected"),
        "text init must print the structured diagnostic: {text}"
    );
    let (code, verify_report) = verify(root);
    assert_eq!(code, Some(0));
    assert_eq!(verify_report["vacuous"], serde_json::json!(true));
    let (_, text, _) = support::run(harness(root).args(["verify", "--set", "verification"]));
    assert!(
        text.contains("VACUOUS"),
        "text output must label the vacuous run: {text}"
    );
}

#[test]
fn existing_scripts_and_hooks_survive_init() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
        root,
        "scripts/check-test.sh",
        "#!/usr/bin/env bash\necho custom\n",
    );
    std::fs::set_permissions(
        root.join("scripts/check-test.sh"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    write(
        root,
        ".git/hooks/pre-commit",
        "#!/usr/bin/env bash\necho foreign-hook\n",
    );
    git_commit(root);
    let (code, _, stderr) = run_json(
        root,
        &["init", "--no-seed", "--minimal", "--format", "json"],
    );
    assert_eq!(code, Some(0), "init failed:\n{stderr}");
    assert_eq!(
        std::fs::read_to_string(root.join("scripts/check-test.sh")).unwrap(),
        "#!/usr/bin/env bash\necho custom\n"
    );
    assert_eq!(
        std::fs::metadata(root.join("scripts/check-test.sh"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o644,
        "init must not chmod a script it did not write"
    );
    assert!(root.join(".git/hooks/pre-commit").exists());
    assert_eq!(
        std::fs::read_to_string(root.join(".git/hooks/pre-commit")).unwrap(),
        "#!/usr/bin/env bash\necho foreign-hook\n"
    );
}

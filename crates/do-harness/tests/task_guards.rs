//! Real CLI coverage for optional HTN task guards.

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

mod support;

use support::{git, harness, run};

fn repo(mode: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::create_dir(root.join("plans")).unwrap();
    std::fs::write(
        root.join("plans/methods.json"),
        r#"{
        "methods":[{"name":"pilot","preconditions":[],"subtasks":[
            {"name":"review","sensor":null,"spike_candidate":false}]}]}
    "#,
    )
    .unwrap();
    std::fs::write(
        root.join("plans/task-guards.json"),
        format!(
            r#"{{
        "schema_version":1,"mode":"{mode}","bindings":[{{
            "method":"pilot","transition":"advance","subtask":"review",
            "all":[{{"predicate":"sensor.fresh_pass","sensor":"smoke"}}],
            "otherwise":"reject"}}]}}"#
        ),
    )
    .unwrap();
    std::fs::write(
        root.join("do-harness.toml"),
        r#"
        language = "generic"
        [[sensors]]
        name = "smoke"
        argv = ["sh", "-c", "test -f permit"]
    "#,
    )
    .unwrap();
    git(&root, &["init", "-q"]);
    git(&root, &["add", "--all"]);
    git(&root, &["commit", "-qm", "test: task guards"]);
    (dir, root)
}

fn command(root: &Path, args: &[&str], expected: i32) -> (String, String) {
    let (code, stdout, stderr) = run(harness(root).args(args));
    assert_eq!(code, Some(expected), "{args:?}\n{stdout}\n{stderr}");
    (stdout, stderr)
}

#[test]
fn enforced_guard_requires_fresh_evidence_for_the_current_task() {
    let (_dir, root) = repo("enforce");
    for title in ["first", "parallel"] {
        command(&root, &["task", "add", title, "--method", "pilot"], 0);
    }
    let before = command(&root, &["task", "show", "1", "--format", "json"], 0).0;
    let advance = ["task", "advance", "1"];
    let blocked = command(&root, &advance, 2).1;
    assert!(blocked.contains("unknown (evidence_missing)"), "{blocked}");

    command(&root, &["verify", "--record", "--task", "1"], 1);
    let blocked = command(&root, &advance, 2).1;
    assert!(blocked.contains("rejected (fresh_failure)"), "{blocked}");

    std::fs::write(root.join("permit"), "ready\n").unwrap();
    command(&root, &["verify", "--record", "--task", "2"], 0);
    let blocked = command(&root, &advance, 2).1;
    assert!(blocked.contains("unknown (evidence_stale)"), "{blocked}");

    command(&root, &["verify", "--record", "--task", "1"], 0);
    command(&root, &["task", "advance", "1", "--dry-run"], 0);
    assert_eq!(
        command(&root, &["task", "show", "1", "--format", "json"], 0).0,
        before
    );
    command(&root, &advance, 0);
}

#[test]
fn advisory_guard_reports_unknown_and_allows_the_existing_transition() {
    let (_dir, root) = repo("advisory");
    command(&root, &["task", "add", "review", "--method", "pilot"], 0);
    let advisory = command(&root, &["task", "advance", "1"], 0).1;
    assert!(
        advisory.contains("guard advisory sensor.fresh_pass smoke Unknown (evidence_missing)"),
        "{advisory}"
    );
}

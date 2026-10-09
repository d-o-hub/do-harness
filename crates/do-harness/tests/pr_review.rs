//! Integration coverage for `do-harness pr review` in local range mode.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

mod support;

fn git(dir: &Path, args: &[&str]) {
    let output = support::git_command(dir)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_repo(dir: &Path) {
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.email", "test@example.com"]);
    git(dir, &["config", "user.name", "Test"]);
}

fn commit_file(dir: &Path, name: &str, content: &str, message: &str) {
    if let Some(parent) = Path::new(name).parent() {
        let full = dir.join(parent);
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(full).unwrap();
        }
    }
    std::fs::write(dir.join(name), content).unwrap();
    git(dir, &["add", name]);
    git(dir, &["commit", "-q", "-m", message]);
}

fn harness(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    cmd.arg("--root").arg(root);
    cmd
}

fn review(root: &Path, extra: &[&str]) -> std::process::Output {
    let mut cmd = harness(root);
    cmd.args(["pr", "review", "--base", "main", "--head", "feature"]);
    cmd.args(extra);
    cmd.output().expect("spawn do-harness")
}

fn json(output: &std::process::Output) -> Value {
    assert!(
        output.status.success(),
        "exit must be 0: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("json report")
}

fn feature_with_two_files(dir: &Path) {
    init_repo(dir);
    commit_file(dir, "a.txt", "one\n", "base");
    git(dir, &["switch", "-q", "-c", "feature"]);
    commit_file(dir, "a.txt", "two\n", "change a");
    commit_file(dir, "b.txt", "new\n", "add b");
}

#[test]
fn range_review_lists_residual_units_and_serves_cache() {
    let dir = tempfile::tempdir().unwrap();
    feature_with_two_files(dir.path());

    let report = json(&review(dir.path(), &["--format", "json"]));
    assert_eq!(report["mode"], serde_json::json!("range"));
    assert_eq!(report["cached"], serde_json::json!(false));
    assert_eq!(report["policy"]["present"], serde_json::json!(false));
    assert_eq!(report["exempt"].as_array().unwrap().len(), 0);
    assert_eq!(report["skipped"].as_array().unwrap().len(), 0);
    assert_eq!(report["false_proven"].as_array().unwrap().len(), 0);
    assert_ne!(report["merge_base"].as_str().unwrap().len(), 0);
    let measurement = &report["measurement"];
    assert!(measurement["t_raw"].as_u64().unwrap() > 0);
    assert!(measurement["t_res"].as_u64().unwrap() > 0);
    assert!(measurement["ratio"].as_f64().unwrap() > 0.0);
    assert!(measurement["verdict"].is_string());
    let residual = report["residual"].as_array().unwrap();
    assert!(residual.len() >= 2, "residual: {residual:?}");
    assert!(residual.iter().all(|unit| unit["id"].is_string()));
    assert!(
        residual
            .iter()
            .any(|unit| unit["path"] == serde_json::json!("b.txt"))
    );

    let cached = json(&review(dir.path(), &["--format", "json"]));
    assert_eq!(cached["cached"], serde_json::json!(true));

    let fresh = json(&review(dir.path(), &["--format", "json", "--recompute"]));
    assert_eq!(fresh["cached"], serde_json::json!(false));
}

#[test]
fn no_effect_range_reports_empty_residual() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "a.txt", "one\n", "base");
    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    git(
        dir.path(),
        &["commit", "-q", "--allow-empty", "-m", "empty"],
    );

    let report = json(&review(dir.path(), &["--format", "json"]));
    assert_eq!(report["residual"].as_array().unwrap().len(), 0);
    assert_eq!(report["warnings"].as_array().unwrap().len(), 0);
}

#[test]
fn policy_is_read_from_the_merge_base_not_the_head() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(
        dir.path(),
        ".github/pr-gate.toml",
        "[proof]\nmechanical = [\"**/Cargo.lock\"]\n",
        "policy base",
    );
    let base_sha = String::from_utf8(
        support::git_command(dir.path())
            .args(["rev-parse", "HEAD"])
            .output()
            .expect("rev-parse")
            .stdout,
    )
    .unwrap()
    .trim()
    .to_owned();
    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    commit_file(
        dir.path(),
        ".github/pr-gate.toml",
        "[[[ malformed head policy\n",
        "policy head",
    );

    let report = json(&review(dir.path(), &["--format", "json"]));
    assert_eq!(report["policy"]["present"], serde_json::json!(true));
    assert_eq!(report["policy"]["source_rev"], serde_json::json!(base_sha));
    assert!(
        report["warnings"].as_array().unwrap().is_empty(),
        "head policy must not be parsed: {}",
        report["warnings"]
    );
}

#[test]
fn malformed_base_policy_warns_but_keeps_reviewing() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(
        dir.path(),
        ".github/pr-gate.toml",
        "[[[ malformed\n",
        "policy base",
    );
    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    commit_file(dir.path(), "a.txt", "one\n", "change");

    let report = json(&review(dir.path(), &["--format", "json"]));
    assert_eq!(report["policy"]["present"], serde_json::json!(true));
    assert_ne!(report["warnings"].as_array().unwrap().len(), 0);
    assert_eq!(report["skipped"].as_array().unwrap().len(), 0);
}

#[test]
fn policy_exempts_mechanical_units_and_audits_them() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "src/lib.rs", "fn a() {}\n", "src base");
    commit_file(dir.path(), "Cargo.lock", "# lock\n", "lock base");
    commit_file(
        dir.path(),
        ".github/pr-gate.toml",
        "[proof]\nmechanical = [\"**/Cargo.lock\"]\n",
        "policy",
    );
    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    commit_file(dir.path(), "Cargo.lock", "# lock\n# dep\n", "lock change");
    commit_file(dir.path(), "src/lib.rs", "fn a() { b(); }\n", "src change");

    let report = json(&review(dir.path(), &["--format", "json"]));
    assert_eq!(report["policy"]["present"], serde_json::json!(true));
    let paths = |key: &str| -> Vec<String> {
        report[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|unit| unit["path"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(paths("exempt"), vec!["Cargo.lock"]);
    assert_eq!(paths("skipped"), Vec::<String>::new());
    assert_eq!(paths("residual"), vec!["src/lib.rs"]);
    assert_eq!(report["false_proven"].as_array().unwrap().len(), 0);
    assert_eq!(report["warnings"].as_array().unwrap().len(), 0);
    assert_eq!(
        report["measurement"]["verdict"],
        serde_json::json!("reduced")
    );
    let t_raw = report["measurement"]["t_raw"].as_u64().unwrap();
    let t_res = report["measurement"]["t_res"].as_u64().unwrap();
    assert!(t_res < t_raw, "t_res={t_res} t_raw={t_raw}");
}

#[test]
fn seeded_defect_is_never_skipped() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(
        dir.path(),
        "crates/core/src/lib.rs",
        "pub fn a() {}\n",
        "base",
    );
    commit_file(
        dir.path(),
        ".github/pr-gate.toml",
        "[proof]\nmechanical = [\"**/*.rs\"]\nbehavioral = [\"crates/**\"]\n",
        "policy",
    );
    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    commit_file(
        dir.path(),
        "crates/core/src/lib.rs",
        "pub fn a() { panic!(); }\n",
        "defect",
    );

    let report = json(&review(dir.path(), &["--format", "json"]));
    assert_eq!(report["skipped"].as_array().unwrap().len(), 0);
    assert_eq!(report["residual"].as_array().unwrap().len(), 1);
    assert_ne!(report["false_proven"].as_array().unwrap().len(), 0);
}

#[test]
fn invalid_glob_proves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "Cargo.lock", "# lock\n", "lock base");
    commit_file(
        dir.path(),
        ".github/pr-gate.toml",
        "[proof]\nmechanical = [\"**/Cargo.lock\"]\nbehavioral = [\"[\"]\n",
        "policy",
    );
    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    commit_file(dir.path(), "Cargo.lock", "# lock\n# dep\n", "lock change");

    let report = json(&review(dir.path(), &["--format", "json"]));
    assert_eq!(report["skipped"].as_array().unwrap().len(), 0);
    assert_ne!(report["warnings"].as_array().unwrap().len(), 0);
}

#[test]
fn structural_rename_and_mode_change_remain_residual_with_empty_policy() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "old.txt", "content\n", "base");
    commit_file(
        dir.path(),
        "script.sh",
        "#!/bin/sh\necho hi\n",
        "add script",
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            dir.path().join("script.sh"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        git(dir.path(), &["add", "script.sh"]);
        git(dir.path(), &["commit", "-q", "--amend", "--no-edit"]);
    }
    commit_file(dir.path(), ".github/pr-gate.toml", "[proof]\n", "policy");

    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    std::fs::rename(dir.path().join("old.txt"), dir.path().join("new.txt")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            dir.path().join("script.sh"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        git(dir.path(), &["add", "script.sh"]);
    }
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "structural changes"]);

    let report = json(&review(dir.path(), &["--format", "json"]));
    assert_eq!(report["exempt"].as_array().unwrap().len(), 0);
    assert_eq!(report["skipped"].as_array().unwrap().len(), 0);
    let residual = report["residual"].as_array().unwrap();
    assert!(
        residual
            .iter()
            .any(|u| u["path"] == serde_json::json!("new.txt")
                && u["old_path"] == serde_json::json!("old.txt")),
        "rename-only unit must remain residual: {residual:?}"
    );
    #[cfg(unix)]
    assert!(
        residual
            .iter()
            .any(|u| u["path"] == serde_json::json!("script.sh")
                && u["header"] == serde_json::json!("mode change")),
        "mode-change unit must remain residual: {residual:?}"
    );
}

#[test]
fn relocation_module_rename_and_mode_change_fixtures_remain_visible() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(
        dir.path(),
        ".github/workflows/ci.yml",
        "name: CI\n",
        "workflow",
    );
    commit_file(dir.path(), "src/mod_a.rs", "pub fn a() {}\n", "mod_a");
    commit_file(
        dir.path(),
        ".github/pr-gate.toml",
        "[proof]\nmechanical = [\"docs/**\"]\n",
        "policy",
    );

    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    std::fs::rename(
        dir.path().join(".github/workflows/ci.yml"),
        dir.path().join("ci.yml"),
    )
    .unwrap();
    std::fs::rename(
        dir.path().join("src/mod_a.rs"),
        dir.path().join("src/mod_b.rs"),
    )
    .unwrap();
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "relocate and rename"]);

    let report = json(&review(dir.path(), &["--format", "json"]));
    let residual = report["residual"].as_array().unwrap();
    assert!(
        residual
            .iter()
            .any(|u| u["path"] == serde_json::json!("ci.yml")
                && u["old_path"] == serde_json::json!(".github/workflows/ci.yml")),
        "workflow relocation must remain visible in residual: {residual:?}"
    );
    assert!(
        residual
            .iter()
            .any(|u| u["path"] == serde_json::json!("src/mod_b.rs")
                && u["old_path"] == serde_json::json!("src/mod_a.rs")),
        "module rename must remain visible in residual: {residual:?}"
    );
}

#[test]
fn both_rename_endpoints_participate_in_matching() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(
        dir.path(),
        "crates/core/lib.rs",
        "pub fn core() {}\n",
        "crates base",
    );
    commit_file(dir.path(), "docs/old.md", "# Docs\n", "docs base");
    commit_file(
        dir.path(),
        ".github/pr-gate.toml",
        "[proof]\nmechanical = [\"docs/**\"]\nbehavioral = [\"crates/**\"]\n",
        "policy",
    );

    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    // Move out of behavioral area into mechanical area
    std::fs::rename(
        dir.path().join("crates/core/lib.rs"),
        dir.path().join("docs/lib.rs"),
    )
    .unwrap();
    // Move within mechanical area
    std::fs::rename(
        dir.path().join("docs/old.md"),
        dir.path().join("docs/new.md"),
    )
    .unwrap();
    // Move protected policy file into mechanical area
    std::fs::rename(
        dir.path().join(".github/pr-gate.toml"),
        dir.path().join("docs/gate.toml"),
    )
    .unwrap();
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "moves"]);

    let report = json(&review(dir.path(), &["--format", "json"]));
    let exempt_paths: Vec<String> = report["exempt"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["path"].as_str().unwrap().to_owned())
        .collect();
    let residual_paths: Vec<String> = report["residual"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["path"].as_str().unwrap().to_owned())
        .collect();

    assert_eq!(exempt_paths, vec!["docs/new.md"]);
    assert!(residual_paths.contains(&"docs/lib.rs".to_owned()));
    assert!(residual_paths.contains(&"docs/gate.toml".to_owned()));
    assert_ne!(report["false_proven"].as_array().unwrap().len(), 0);
}

#[test]
fn missing_revision_is_an_analysis_error() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "a.txt", "one\n", "base");

    let output = harness(dir.path())
        .args(["pr", "review", "--base", "missing", "--head", "main"])
        .output()
        .expect("spawn do-harness");
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn mixed_modes_and_missing_target_are_usage_errors() {
    let dir = tempfile::tempdir().unwrap();
    let mixed = harness(dir.path())
        .args(["pr", "review", "42", "--base", "main", "--head", "HEAD"])
        .output()
        .expect("spawn do-harness");
    assert_eq!(mixed.status.code(), Some(2));

    let missing = harness(dir.path())
        .args(["pr", "review"])
        .output()
        .expect("spawn do-harness");
    assert_eq!(missing.status.code(), Some(2));
}

#[test]
fn head_change_invalidates_the_cache() {
    let dir = tempfile::tempdir().unwrap();
    feature_with_two_files(dir.path());

    let first = json(&review(dir.path(), &["--format", "json"]));
    let count = first["residual"].as_array().unwrap().len();

    commit_file(dir.path(), "c.txt", "third\n", "add c");
    let second = json(&review(dir.path(), &["--format", "json"]));
    assert_eq!(second["cached"], serde_json::json!(false));
    assert!(second["residual"].as_array().unwrap().len() > count);
}

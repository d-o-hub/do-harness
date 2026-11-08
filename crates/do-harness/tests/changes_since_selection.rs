//! Acceptance fixtures for committed-range selection (`--since`) that extend
//! `changes_since.rs`: bare-origin refs, staged/untracked unions, rename and
//! deletion ranges, missing merge base / shallow history, a poisoned git
//! environment, policy invalidation, range-keyed beat reuse, and reason
//! carrying into the verify report and sealed evidence.
//!
//! Split from `changes_since.rs` to keep both files under the 500-LOC
//! ceiling; the macOS `selection-portability` job runs both targets.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use serde_json::Value;

mod support;

use support::range_selection::{committed_rust_change, fixture_repo, rev, sensor_names};
use support::{git, harness, run};

/// A local bare `origin/main` selects the committed range a branch introduces,
/// with the requested ref resolved through the remote-tracking ref.
#[test]
fn bare_origin_main_selects_committed_range() {
    let (dir, root) = fixture_repo();
    let bare = dir.path().join("origin.git");
    git(dir.path(), &["init", "--bare", "-q", "origin.git"]);
    git(&root, &["remote", "add", "origin", bare.to_str().unwrap()]);
    git(
        &root,
        &["push", "-q", "-u", "origin", "HEAD:refs/heads/main"],
    );
    let origin_main = rev(&root, "origin/main");

    committed_rust_change(&root);
    assert_ne!(
        rev(&root, "HEAD"),
        origin_main,
        "the committed change must advance HEAD"
    );

    let (code, stdout, _) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--since")
        .arg("origin/main")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "bare-origin range must exit 0:\n{stdout}");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(sensor_names(&report), vec!["rs-check".to_owned()]);
    assert_eq!(report["selection"]["mode"], serde_json::json!("range"));
    assert_eq!(
        report["selection"]["since"],
        serde_json::json!("origin/main")
    );
    assert_eq!(
        report["selection"]["base_sha"],
        serde_json::json!(origin_main)
    );
    assert_eq!(
        report["selection"]["merge_base_sha"],
        serde_json::json!(origin_main)
    );
}

/// `--since` + `--changed` unions committed, staged, and untracked paths.
#[test]
fn staged_and_untracked_union_with_range_selects_both() {
    let (_dir, root) = fixture_repo();
    let base = committed_rust_change(&root);
    std::fs::write(root.join("lib.rs"), "pub fn f() {}\n").unwrap();
    git(&root, &["add", "lib.rs"]);
    std::fs::create_dir_all(root.join("docs")).unwrap();
    std::fs::write(root.join("docs/new.md"), "# new\n").unwrap();

    let (code, stdout, _) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--since")
        .arg(&base)
        .arg("--changed")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "union run must exit 0:\n{stdout}");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(
        sensor_names(&report),
        vec!["rs-check".to_owned(), "docs".to_owned()]
    );
    assert_eq!(
        report["selection"]["mode"],
        serde_json::json!("working-tree+range")
    );
    let files: Vec<&str> = report["selection"]["changed_files"]
        .as_array()
        .expect("changed_files")
        .iter()
        .map(|f| f.as_str().expect("path"))
        .collect();
    for path in ["main.rs", "lib.rs", "docs/new.md"] {
        assert!(
            files.contains(&path),
            "changed_files must include {path}: {files:?}"
        );
    }
}

/// A committed rename surfaces both ends and a deletion still triggers its
/// sensor.
#[test]
fn rename_both_ends_and_deletion_in_range_select_both_sensors() {
    let (_dir, root) = fixture_repo();
    std::fs::write(root.join("old.rs"), "fn old() {}\n").unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "test: base with old.rs"]);
    let base = rev(&root, "HEAD");
    git(&root, &["mv", "old.rs", "new.rs"]);
    std::fs::remove_file(root.join("README.md")).unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "test: rename and delete"]);

    let (code, stdout, _) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--since")
        .arg(&base)
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "rename/delete range must exit 0:\n{stdout}");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(
        sensor_names(&report),
        vec!["rs-check".to_owned(), "docs".to_owned()]
    );
    let files: Vec<&str> = report["selection"]["changed_files"]
        .as_array()
        .expect("changed_files")
        .iter()
        .map(|f| f.as_str().expect("path"))
        .collect();
    for path in ["old.rs", "new.rs", "README.md"] {
        assert!(
            files.contains(&path),
            "changed_files must include {path}: {files:?}"
        );
    }
}

/// Unrelated histories cannot produce a merge base: selection fails closed
/// instead of reporting an empty change set.
#[test]
fn unrelated_histories_missing_merge_base_fails_closed() {
    let (_dir, root) = fixture_repo();
    let base = rev(&root, "HEAD");
    git(&root, &["checkout", "--orphan", "other"]);
    std::fs::write(root.join("other.txt"), "unrelated\n").unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "test: unrelated root"]);

    let (code, stdout, _) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--since")
        .arg(&base)
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "fail-closed run must exit 0:\n{stdout}");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(
        sensor_names(&report),
        vec!["rs-check".to_owned(), "docs".to_owned()]
    );
    let error = report["selection"]["discovery_error"]
        .as_str()
        .expect("discovery error");
    assert!(
        error.contains("merge-base"),
        "diagnostic must name the merge-base failure: {error}"
    );
}

/// A ref pointing at a commit outside a shallow clone's object store cannot
/// be resolved: selection fails closed with a diagnostic.
#[test]
fn shallow_history_missing_object_fails_closed() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    let root = dir.path().join("shallow");
    std::fs::create_dir_all(&source).unwrap();
    let config = r#"
[signal-sets]
all = ["rs-check", "docs"]

[[sensors]]
name = "rs-check"
argv = ["true"]
when-changed = ["**/*.rs"]

[[sensors]]
name = "docs"
argv = ["true"]
when-changed = ["**/*.md"]
"#;
    std::fs::write(source.join("do-harness.toml"), config).unwrap();
    std::fs::write(source.join("main.rs"), "fn main() {}\n").unwrap();
    git(&source, &["init", "-q"]);
    git(&source, &["add", "-A"]);
    git(&source, &["commit", "-qm", "test: base"]);
    let base = rev(&source, "HEAD");
    std::fs::write(source.join("main.rs"), "fn main() { let x = 1; }\n").unwrap();
    git(&source, &["commit", "-qam", "test: second"]);

    git(
        dir.path(),
        &[
            "clone",
            "-q",
            "--depth",
            "1",
            &format!("file://{}", source.display()),
            root.to_str().unwrap(),
        ],
    );
    // Point the remote-tracking ref at a commit that the shallow clone did
    // not fetch: resolving it must fail closed, never select nothing. A
    // ref file is used because `git update-ref` refuses to point at an
    // object missing from the shallow store.
    std::fs::create_dir_all(root.join(".git/refs/remotes/origin")).unwrap();
    std::fs::write(
        root.join(".git/refs/remotes/origin/main"),
        format!(
            "{base}
"
        ),
    )
    .unwrap();

    let (code, stdout, _) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--since")
        .arg("origin/main")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "fail-closed run must exit 0:\n{stdout}");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(
        sensor_names(&report),
        vec!["rs-check".to_owned(), "docs".to_owned()]
    );
    let error = report["selection"]["discovery_error"]
        .as_str()
        .expect("discovery error");
    assert!(
        error.contains("origin/main"),
        "diagnostic must name the unresolvable ref: {error}"
    );
}

/// A poisoned `GIT_DIR` must not redirect selection to another repository:
/// the harness clears the inherited git view, so `--since` still resolves
/// inside the requested root.
#[test]
fn poisoned_git_dir_env_does_not_redirect_selection() {
    let (_dir, root) = fixture_repo();
    let (_other_dir, other) = fixture_repo();
    let base = committed_rust_change(&root);

    let mut cmd = harness(&root);
    cmd.arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--since")
        .arg(&base)
        .arg("--format")
        .arg("json")
        .env("GIT_DIR", other.to_str().unwrap());
    let (code, stdout, _) = run(&mut cmd);
    assert_eq!(
        code,
        Some(0),
        "poisoned GIT_DIR must not break selection:\n{stdout}"
    );
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(sensor_names(&report), vec!["rs-check".to_owned()]);
    assert_eq!(report["selection"]["base_sha"], serde_json::json!(base));
}

/// Editing `do-harness.toml` invalidates range evidence and prevents reuse of
/// a recorded beat under the same range.
#[test]
fn policy_change_invalidates_range_evidence_and_reuse() {
    let (_dir, root) = fixture_repo();
    let base = committed_rust_change(&root);

    let (code, _, stderr) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--since")
        .arg(&base)
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "range verify must exit 0:\n{stderr}");

    let (code, stdout, _) = run(harness(&root)
        .arg("status")
        .arg("--set")
        .arg("all")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "range evidence must be green:\n{stdout}");
    let doc: Value = serde_json::from_str(&stdout).expect("json status");
    assert_eq!(doc["state"], serde_json::json!("green"));

    let config = std::fs::read_to_string(root.join("do-harness.toml")).unwrap();
    let changed = config.replace("docs/**", "doc/**");
    assert_ne!(changed, config, "the policy edit must change bytes");
    std::fs::write(root.join("do-harness.toml"), changed).unwrap();

    let (code, stdout, _) = run(harness(&root)
        .arg("status")
        .arg("--set")
        .arg("all")
        .arg("--format")
        .arg("json"));
    assert_ne!(
        code,
        Some(0),
        "policy change must not stay green:\n{stdout}"
    );
    let doc: Value = serde_json::from_str(&stdout).expect("json status");
    assert_eq!(doc["state"], serde_json::json!("stale"));
    assert_eq!(doc["reason"], serde_json::json!("policy_changed"));
}

/// A recording passing beat is not reused when the resolved range changes,
/// while the unchanged range still reuses it.
#[test]
fn range_change_prevents_beat_reuse() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let config = r#"
[signal-sets]
all = ["rs-check"]

[[sensors]]
name = "rs-check"
argv = ["sh", "-c", "printf hit >> runs.log"]
inputs = ["main.rs"]
when-changed = ["**/*.rs"]
"#;
    std::fs::write(root.join("do-harness.toml"), config).unwrap();
    std::fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
    git(&root, &["init", "-q"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "test: parent"]);
    let parent = rev(&root, "HEAD");
    std::fs::write(root.join("main.rs"), "fn main() { let x = 1; }\n").unwrap();
    git(&root, &["commit", "-qam", "test: base"]);
    let base = rev(&root, "HEAD");
    std::fs::write(root.join("main.rs"), "fn main() { let x = 2; }\n").unwrap();
    git(&root, &["commit", "-qam", "test: head"]);

    let runs = |root: &Path| -> usize {
        std::fs::read_to_string(root.join("runs.log")).map_or(0, |s| s.matches("hit").count())
    };
    let execution = |stdout: &str| -> String {
        let report: Value = serde_json::from_str(stdout).expect("json report");
        report["sensors"][0]["execution"]
            .as_str()
            .expect("execution")
            .to_owned()
    };

    // Record under range base..HEAD.
    let (code, stdout, _) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--since")
        .arg(&base)
        .arg("--record")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "record run must exit 0:\n{stdout}");
    assert_eq!(execution(&stdout), "ran");
    assert_eq!(runs(&root), 1);

    // Same range again: the recorded pass is reused, no fresh execution.
    let (code, stdout, _) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--since")
        .arg(&base)
        .arg("--record")
        .arg("--unchanged=skip")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "reuse run must exit 0:\n{stdout}");
    assert_eq!(execution(&stdout), "reused");
    assert_eq!(runs(&root), 1, "a reused beat must not execute again");

    // A different range (parent..HEAD) is a different input identity: the
    // old beat must not be reused.
    let (code, stdout, _) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--since")
        .arg(&parent)
        .arg("--record")
        .arg("--unchanged=skip")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "changed-range run must exit 0:\n{stdout}");
    assert_eq!(execution(&stdout), "ran");
    assert_eq!(runs(&root), 2, "a changed range must force a fresh run");
}

/// Sealed evidence records selected sensors with reasons and the v2 input
/// digest algorithm.
#[test]
fn evidence_records_selected_reasons_and_v2_algorithm() {
    let (_dir, root) = fixture_repo();
    let base = committed_rust_change(&root);
    let evidence = root.join("out-evidence.json");

    let (code, _, stderr) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--since")
        .arg(&base)
        .arg("--evidence")
        .arg(&evidence)
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "range verify must exit 0:\n{stderr}");
    let doc: Value = serde_json::from_slice(&std::fs::read(&evidence).expect("evidence file"))
        .expect("json evidence");
    assert_eq!(doc["selection"]["mode"], serde_json::json!("range"));
    assert_eq!(
        doc["selection"]["selected"][0]["name"],
        serde_json::json!("rs-check")
    );
    let reason = doc["selection"]["selected"][0]["reason"]
        .as_str()
        .expect("selected reason");
    assert!(
        !reason.is_empty(),
        "evidence must record the selection reason"
    );
    assert_eq!(
        doc["input_manifest"]["algorithm"],
        serde_json::json!("sha256-canonical-inputs-v2")
    );
}

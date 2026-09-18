//! CLI dogfood for greenfield adoption: the real binary must computationally
//! prove the `init` -> `verify` outcomes — never assume them.
//!
//! - rust: `init` scaffolds a minimal crate, so the full sensor suite runs and
//!   exits 0 on a truly empty tree.

#![allow(clippy::unwrap_used, clippy::expect_used)]
//! - red: removing the crate must flip `verify` to a non-zero exit with named
//!   failing sensors (proves the sensors actually execute).
//! - generic: zero sensors means `verify` exits 0 without running any command
//!   — a documented vacuous pass, asserted as such.

use std::path::Path;
use std::process::Command;

use serde_json::Value;

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

/// Builds a `do-harness --root <root>` command using the real binary.
fn harness(root: &Path) -> Command {
    let mut cmd = isolated_command(env!("CARGO_BIN_EXE_do-harness"));
    cmd.arg("--root").arg(root);
    cmd
}

/// Runs a harness command, returning (success, stdout).
fn run(cmd: &mut Command) -> (bool, String) {
    let output = cmd.output().expect("spawn do-harness");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

/// Runs `verify --format json` (which must exit 0) and parses the report.
fn verify_json(root: &Path) -> Value {
    let (ok, stdout) = run(harness(root).arg("verify").arg("--format").arg("json"));
    assert!(ok, "verify exited non-zero:\n{stdout}");
    serde_json::from_str(&stdout).expect("verify stdout is one JSON object")
}

#[test]
fn rust_init_on_empty_git_repo_is_green() {
    let dir = tempfile::tempdir().unwrap();
    let status = isolated_command("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status()
        .expect("git init");
    assert!(status.success(), "git init failed");
    assert!(dir.path().join(".git").is_dir());

    let (ok, out) = run(harness(dir.path()).arg("init"));
    assert!(
        ok,
        "init failed on fresh git repository with no commits:\n{out}"
    );

    let report = verify_json(dir.path());
    assert_eq!(
        report["ok"],
        serde_json::json!(true),
        "rust init + verify on repo with no commits must be green"
    );
    let commitlint = report["sensors"]
        .as_array()
        .expect("sensors array")
        .iter()
        .find(|s| s["name"] == "commitlint")
        .expect("commitlint sensor entry");
    assert_eq!(commitlint["ok"], serde_json::json!(true));
    assert_eq!(commitlint["exit_code"], serde_json::json!(0));
}

#[test]
fn commitlint_empty_history_validates_arguments_and_messages() {
    let source = include_str!("../../../scripts/check-commitlint.sh");
    assert_eq!(
        source,
        include_str!("../templates/scripts/check-commitlint.sh")
    );
    let dir = tempfile::tempdir().unwrap();
    assert!(
        isolated_command("git")
            .args(["init", "-q"])
            .current_dir(dir.path())
            .status()
            .unwrap()
            .success()
    );
    assert!(dir.path().join(".git").is_dir());
    let scripts = dir.path().join("scripts");
    std::fs::create_dir(&scripts).unwrap();
    let script = scripts.join("check-commitlint.sh");
    std::fs::write(&script, source).unwrap();
    std::fs::write(dir.path().join("valid-message"), "fix: initial commit\n").unwrap();
    std::fs::write(dir.path().join("invalid-message"), "invalid subject\n").unwrap();

    let cases: &[(&[&str], i32)] = &[
        (&[], 0),
        (&["--count", "2"], 0),
        (&["--count", "0"], 2),
        (&["--unknown"], 2),
        (&["--range", "missing..HEAD"], 2),
        (&["--range=missing..HEAD"], 2),
        (&["--range"], 2),
        (&["--range", ""], 2),
        (&["--range="], 2),
        (&["--range", "missing..HEAD", "--count", "1"], 2),
        (&["--message", "valid-message"], 0),
        (&["--message", "invalid-message"], 1),
    ];
    for (args, expected) in cases {
        let output = isolated_command("bash")
            .arg(&script)
            .args(*args)
            .env_remove("DO_HARNESS_COMMITLINT_COUNT")
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(*expected),
            "args {args:?}: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn rust_init_then_full_verify_is_green() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, out) = run(harness(dir.path()).arg("init"));
    assert!(ok, "init failed:\n{out}");

    let report = verify_json(dir.path());
    assert_eq!(
        report["ok"],
        serde_json::json!(true),
        "greenfield rust init + verify must be green, not assumed"
    );
    let sensors: Vec<&str> = report["sensors"]
        .as_array()
        .expect("sensors array")
        .iter()
        .map(|s| s["name"].as_str().expect("sensor name"))
        .collect();
    assert!(
        !sensors.is_empty(),
        "sensors must actually run; a green report with no sensors is vacuous"
    );
    for want in ["fmt", "check", "clippy", "test", "loc", "commitlint"] {
        assert!(
            sensors.contains(&want),
            "missing sensor {want} in {sensors:?}"
        );
        let ran = report["sensors"]
            .as_array()
            .expect("sensors array")
            .iter()
            .find(|s| s["name"] == want)
            .expect("sensor entry");
        assert_eq!(ran["ok"], serde_json::json!(true), "sensor {want} failed");
        assert_eq!(
            ran["exit_code"],
            serde_json::json!(0),
            "sensor {want} exit code"
        );
    }
}

#[test]
fn rust_verify_fails_without_a_crate() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, out) = run(harness(dir.path()).arg("init"));
    assert!(ok, "init failed:\n{out}");
    std::fs::remove_file(dir.path().join("Cargo.toml")).unwrap();
    std::fs::remove_dir_all(dir.path().join("src")).unwrap();

    let (ok, stdout) = run(harness(dir.path())
        .arg("verify")
        .arg("--format")
        .arg("json"));
    assert!(!ok, "verify must exit non-zero without a crate");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(report["ok"], serde_json::json!(false));
    let failed = report["failed"].as_array().expect("failed array");
    assert!(
        !failed.is_empty(),
        "failed sensors must be listed, not assumed"
    );
    assert!(
        failed.iter().any(|name| name == "fmt"),
        "cargo sensor must fail without Cargo.toml: {failed:?}"
    );
}

#[test]
fn rust_verify_with_evidence_writes_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, out) = run(harness(dir.path()).arg("init"));
    assert!(ok, "init failed:\n{out}");

    let evidence_path = dir.path().join("evidence.json");
    let (ok, _out) = run(harness(dir.path())
        .arg("verify")
        .arg("--evidence")
        .arg(&evidence_path)
        .arg("--strict"));
    assert!(ok, "verify --evidence --strict failed:\n{out}");
    assert!(evidence_path.exists(), "evidence file was not created");

    let text = std::fs::read_to_string(&evidence_path).unwrap();
    let doc: Value = serde_json::from_str(&text).expect("valid evidence JSON");
    assert_eq!(doc["schema_version"], serde_json::json!(4));
    assert_eq!(doc["tool"], serde_json::json!("do-harness"));
    assert_eq!(doc["summary"]["verdict"], serde_json::json!("pass"));
    assert_eq!(doc["summary"]["skip"], serde_json::json!(0));
    // Every sensor records its argv and an output hash; the artifact is sealed.
    assert!(doc["sensors"][0]["argv"].is_array());
    assert!(doc["sensors"][0]["output_sha256"].is_string());
    assert!(doc["chain_hash"].as_str().is_some_and(|h| !h.is_empty()));
}

#[test]
fn rust_verify_strict_fails_on_failing_sensor_and_writes_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, out) = run(harness(dir.path()).arg("init"));
    assert!(ok, "init failed:\n{out}");
    std::fs::remove_file(dir.path().join("Cargo.toml")).unwrap();
    std::fs::remove_dir_all(dir.path().join("src")).unwrap();

    let evidence_path = dir.path().join("failing_evidence.json");
    let (ok, _out) = run(harness(dir.path())
        .arg("verify")
        .arg("--evidence")
        .arg(&evidence_path)
        .arg("--strict"));
    assert!(!ok, "verify --strict must fail on failing sensors");
    assert!(
        evidence_path.exists(),
        "failing run must still write evidence artifact"
    );

    let text = std::fs::read_to_string(&evidence_path).unwrap();
    let doc: Value = serde_json::from_str(&text).expect("valid evidence JSON");
    assert_eq!(doc["summary"]["verdict"], serde_json::json!("fail"));
}

#[test]
fn rust_verify_strict_default_path() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, out) = run(harness(dir.path()).arg("init"));
    assert!(ok, "init failed:\n{out}");

    let (ok, out) = run(harness(dir.path()).arg("verify").arg("--strict"));
    assert!(ok, "verify --strict failed:\n{out}");

    let default_path = dir.path().join(".do-harness/evidence.json");
    assert!(
        default_path.exists(),
        "default evidence file .do-harness/evidence.json was not created"
    );
}

#[test]
fn audit_chain_cli_reports_intact_and_detects_tampering() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, out) = run(harness(dir.path()).arg("task").arg("add").arg("test task"));
    assert!(ok, "task add failed:\n{out}");

    let (ok, stdout) = run(harness(dir.path()).arg("audit-chain"));
    assert!(ok, "audit-chain failed on intact log:\n{stdout}");
    assert!(stdout.contains("OK: Hash chain intact"));

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let db_path = dir.path().join(".do-harness/agent_state.db");
        let conn = do_harness_db::connect(&db_path).await.unwrap();
        conn.execute(
            "UPDATE workflow_events SET chain_hash = 'badhash' WHERE seq = 1",
            (),
        )
        .await
        .unwrap();
    });

    let (ok, out_tampered) = run(harness(dir.path()).arg("audit-chain"));
    assert!(!ok, "audit-chain must fail on tampered log");
    assert!(out_tampered.contains("tampered at event seq 1"));
}

#[test]
fn generic_init_verify_is_vacuously_green() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, out) = run(harness(dir.path())
        .arg("init")
        .arg("--language")
        .arg("generic"));
    assert!(ok, "generic init failed:\n{out}");
    assert!(
        !dir.path().join("Cargo.toml").exists(),
        "generic pack must not scaffold a crate"
    );

    let report = verify_json(dir.path());
    assert_eq!(report["ok"], serde_json::json!(true));
    assert!(
        report["sensors"].as_array().expect("sensors").is_empty(),
        "generic pack ships zero sensors; the pass is vacuous by design"
    );
}

/// A merge-tip branch is judged on its own commits, not the history it merged.
///
/// Regression: the sensor's default window ran `git log --no-merges -n 1`.
/// When a branch tip is a merge, that command skips the merge and descends into
/// the merged-in branch; because `git log` orders by commit date, the window
/// then lands on whichever side is newest. In CI that reported the
/// non-conventional subject of a commit already on `main`. No change on the
/// branch could fix it, and merging `main` again could not clear it either:
/// `--no-merges` excludes the merge, so the offending commit stayed in range as
/// an ancestor. `--first-parent` keeps the window on the branch's own line.
///
/// The fixture pins commit dates so the merged-in commit is the newest, which
/// is what makes the window reach it.
#[test]
fn commitlint_ignores_history_a_merge_tip_brought_in() {
    let source = include_str!("../../../scripts/check-commitlint.sh");
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let script_dir = root.join("scripts");
    std::fs::create_dir(&script_dir).unwrap();
    let script = script_dir.join("check-commitlint.sh");
    std::fs::write(&script, source).unwrap();

    let run_git = |args: &[&str], date: Option<&str>| {
        let mut command = isolated_command("git");
        command
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .args(args)
            .current_dir(root);
        if let Some(date) = date {
            command
                .env("GIT_AUTHOR_DATE", date)
                .env("GIT_COMMITTER_DATE", date);
        }
        let output = command.output().expect("spawn git");
        assert!(
            output.status.success(),
            "git {args:?} failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    let commit = |subject: &str, date: &str| {
        run_git(
            &["commit", "-q", "--allow-empty", "-m", subject],
            Some(date),
        );
    };

    run_git(&["init", "-q", "-b", "main"], None);
    commit("chore: seed", "1700000000");
    // Diverge first, so the merge is a real merge rather than a fast-forward.
    run_git(&["checkout", "-q", "-b", "feature"], None);
    commit("fix(feature): a conventional change", "1700000100");
    run_git(&["checkout", "-q", "main"], None);
    // Non-conventional on main, mirroring the real #111, and newer than the
    // branch's own commit so `git log` visits it first.
    commit("Fix something without a type", "1700000200");
    run_git(&["checkout", "-q", "feature"], None);
    run_git(
        &[
            "merge",
            "-q",
            "--no-ff",
            "-m",
            "Merge branch 'main' into feature",
            "main",
        ],
        Some("1700000300"),
    );

    let lint = || {
        let output = isolated_command("bash")
            .arg(&script)
            .env_remove("DO_HARNESS_COMMITLINT_COUNT")
            .current_dir(root)
            .output()
            .unwrap();
        (
            output.status.success(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    };

    let (ok, stdout, stderr) = lint();
    assert!(
        ok,
        "a merge tip must not fail on the history it merged:\n{stdout}\n{stderr}"
    );
    assert!(
        !stdout.contains("Fix something without a type"),
        "the merged-in non-conventional subject must not be linted:\n{stdout}"
    );
    assert!(
        stdout.contains("1 subject(s)"),
        "the window must land on the branch's own commit:\n{stdout}"
    );

    // The branch's own bad commit must still fail, so the window is not
    // vacuously empty.
    commit("Bad subject on the branch", "1700000400");
    let (ok, stdout, stderr) = lint();
    assert!(
        !ok,
        "the branch's own non-conventional subject must still fail:\n{stdout}\n{stderr}"
    );
}

//! Acceptance fixture for `do-harness timeline` (issue #647).
//!
//! The pilot's promise is a *reading* surface over evidence that already
//! exists, so the fixture records two real verify runs — a failing attempt, a
//! source edit that makes it stale, and a rerun that passes — and then asserts
//! the rendered timeline keeps them distinguishable, keeps the recorded timing,
//! and opens offline with local links only.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use serde_json::Value;

mod support;

use support::{git, harness, run};

/// The fixture's source before the edit: no marker, so the probe fails.
const SOURCE: &str = "fn plain() {}\n";

/// The edit that flips the sensor and makes the first attempt stale.
const EDITED: &str = "const MARKER: u8 = 1;\nfn plain() {}\n";

/// A committed workspace plus a directory that holds the saved artifacts.
///
/// The fixture is a git repository so a run records an established input
/// manifest, and the artifacts live outside it so saving an artifact never
/// changes the workspace the run was measured against.
struct Fixture {
    _repo: tempfile::TempDir,
    _artifacts: tempfile::TempDir,
    root: PathBuf,
    artifacts: PathBuf,
}

impl Fixture {
    fn evidence(&self, name: &str) -> PathBuf {
        self.artifacts.join(name)
    }
}

fn arg(path: &Path) -> &str {
    path.to_str().expect("utf-8 fixture path")
}

/// A workspace whose single sensor passes only once `allow` exists.
fn fixture() -> Fixture {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path().to_path_buf();
    let artifacts_dir = tempfile::tempdir().unwrap();
    // The probe is a `git` lookup, not a shell script: a POSIX shell is not part
    // of the contract a fixture may assume, and `git` is required by every
    // fixture here anyway. It passes only once the source carries the marker.
    let config = "[signal-sets]\ngate = [\"probe\"]\n\n[[sensors]]\nname = \"probe\"\n\
                  argv = [\"git\", \"grep\", \"--quiet\", \"-e\", \"MARKER\", \"--\", \"src\"]\n";
    std::fs::write(root.join("do-harness.toml"), config).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/lib.rs"), SOURCE).unwrap();
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "fixture"]);
    let artifacts = artifacts_dir.path().to_path_buf();
    Fixture {
        _repo: repo,
        _artifacts: artifacts_dir,
        root,
        artifacts,
    }
}

/// Runs `verify --record` for one attempt and returns its exit code.
fn record_attempt(root: &Path, evidence: &Path) -> Option<i32> {
    let (code, stdout, stderr) = run(harness(root)
        .arg("verify")
        .arg("--set")
        .arg("gate")
        .arg("--record")
        .arg("--global")
        .arg("--evidence")
        .arg(evidence));
    assert!(
        code == Some(0) || code == Some(1),
        "verify must decide pass/fail, not error:\n{stderr}\n{stdout}"
    );
    code
}

/// Captures `status --format json` against one artifact.
fn status(root: &Path, evidence: &Path) -> Value {
    let (code, stdout, stderr) = run(harness(root)
        .arg("status")
        .arg("--set")
        .arg("gate")
        .arg("--evidence")
        .arg(evidence)
        .arg("--format")
        .arg("json"));
    // Red and stale are verdicts (exit 1), not command failures: the snapshot
    // is exactly what the timeline must render.
    assert!(
        code == Some(0) || code == Some(1),
        "status must report freshness, not fail:\n{stderr}"
    );
    serde_json::from_str(&stdout).expect("status json")
}

#[test]
fn a_failed_attempt_a_stale_edit_and_a_green_rerun_render_as_one_timeline() {
    let fixture = fixture();
    let root = &fixture.root;
    let first = fixture.evidence("attempt1.json");
    let second = fixture.evidence("attempt2.json");
    let red_snapshot = fixture.evidence("status-red.json");
    let stale_snapshot = fixture.evidence("status-stale.json");
    let green_snapshot = fixture.evidence("status-green.json");

    // Attempt 1: the sensor fails, and the artifact is recorded anyway.
    assert_eq!(record_attempt(root, &first), Some(1));
    let red = status(root, &first);
    assert_eq!(red["state"], "red", "{red}");
    std::fs::write(&red_snapshot, red.to_string()).unwrap();

    // A source edit makes that evidence stale — the middle of the timeline.
    std::fs::write(root.join("src/lib.rs"), EDITED).unwrap();
    let stale = status(root, &first);
    assert_eq!(stale["state"], "stale", "{stale}");
    std::fs::write(&stale_snapshot, stale.to_string()).unwrap();

    // Attempt 2: the same sensor now passes, on a clean tree.
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "allow"]);
    assert_eq!(record_attempt(root, &second), Some(0));
    let green = status(root, &second);
    assert_eq!(green["state"], "green", "{green}");
    std::fs::write(&green_snapshot, green.to_string()).unwrap();

    // The JSON timeline keeps both attempts, their verdicts, and their timing.
    let (code, stdout, stderr) = run(harness(root)
        .arg("timeline")
        .arg("--evidence")
        .arg(arg(&first))
        .arg("--evidence")
        .arg(arg(&second))
        .arg("--status")
        .arg(arg(&red_snapshot))
        .arg("--status")
        .arg(arg(&stale_snapshot))
        .arg("--status")
        .arg(arg(&green_snapshot))
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "{stderr}");
    let document: Value = serde_json::from_str(&stdout).expect("timeline json");
    assert_eq!(document["order"], "supplied");
    let attempts = document["attempts"].as_array().expect("attempts");
    assert_eq!(attempts.len(), 2, "{document}");
    assert_eq!(attempts[0]["verdict"], "fail");
    assert_eq!(attempts[1]["verdict"], "pass");
    assert!(
        attempts[0]["sensors"][0]["duration_ms"].is_number(),
        "a real run records its measured duration: {}",
        attempts[0]
    );
    let first_freshness = attempts[0]["freshness"].as_array().expect("freshness");
    assert_eq!(
        first_freshness.len(),
        2,
        "the failed attempt keeps both observations taken against it: {}",
        attempts[0]
    );
    assert_eq!(first_freshness[0]["state"], "red");
    assert_eq!(first_freshness[1]["state"], "stale");
    assert!(
        first_freshness[0]["observed_at"].as_i64().unwrap_or(0) > 0,
        "an observation records when it was captured"
    );
    assert_eq!(attempts[1]["freshness"][0]["state"], "green");

    // The HTML page is self-contained and links each artifact locally.
    let page_path = fixture.evidence("timeline.html");
    let (code, stdout, stderr) = run(harness(root)
        .arg("timeline")
        .arg("--evidence")
        .arg(arg(&first))
        .arg("--evidence")
        .arg(arg(&second))
        .arg("--status")
        .arg(arg(&red_snapshot))
        .arg("--status")
        .arg(arg(&stale_snapshot))
        .arg("--status")
        .arg(arg(&green_snapshot))
        .arg("--out")
        .arg(arg(&page_path)));
    assert_eq!(code, Some(0), "{stderr}\n{stdout}");
    let page = std::fs::read_to_string(&page_path).expect("timeline.html");
    let first_link = first.to_string_lossy();
    let second_link = second.to_string_lossy();
    assert!(page.contains(first_link.as_ref()), "attempt 1 link");
    assert!(page.contains(second_link.as_ref()), "attempt 2 link");
    assert!(page.contains("Attempt 1 — fail"), "{page}");
    assert!(page.contains("Attempt 2 — pass"), "{page}");
    assert!(
        page.contains("freshness: stale"),
        "the stale window is kept: {page}"
    );
    assert!(
        !page.contains("http://") && !page.contains("https://"),
        "the page must open offline"
    );
}

#[test]
fn an_unrelated_status_snapshot_is_reported_as_a_gap_instead_of_bound() {
    let fixture = fixture();
    let root = &fixture.root;
    let first = fixture.evidence("attempt1.json");
    let other = fixture.evidence("other.json");
    assert_eq!(record_attempt(root, &first), Some(1));
    let unrelated = serde_json::json!({
        "state": "green",
        "reason": "current",
        "evidence": {
            "signal_set": "gate",
            "workspace_fingerprint": "sha256:elsewhere",
            "policy_fingerprint": "sha256:elsewhere",
        },
    });
    std::fs::write(&other, unrelated.to_string()).unwrap();

    let (code, stdout, stderr) = run(harness(root)
        .arg("timeline")
        .arg("--evidence")
        .arg(arg(&first))
        .arg("--status")
        .arg(arg(&other))
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "{stderr}");
    let document: Value = serde_json::from_str(&stdout).expect("timeline json");
    assert_eq!(
        document["attempts"][0]["freshness"]
            .as_array()
            .expect("freshness sequence")
            .len(),
        0,
        "an unbound snapshot leaves the attempt's freshness unknown: {document}"
    );
    let gaps = document["gaps"].as_array().expect("gaps");
    assert!(
        gaps.iter()
            .any(|gap| gap.as_str().unwrap_or_default().contains("unbound")),
        "{document}"
    );
}

#[test]
fn an_unsupported_evidence_schema_is_refused_with_an_actionable_error() {
    let fixture = fixture();
    let root = &fixture.root;
    let legacy = fixture.evidence("legacy.json");
    let document = serde_json::json!({
        "schema_version": 2,
        "tool": "do-harness",
        "harness_version": "0.2.0",
        "git_sha": null,
        "started_at": 1,
        "finished_at": 2,
        "root": root.to_string_lossy(),
        "task_id": null,
        "sensor_pack": "default",
        "workspace_fingerprint": "",
        "policy_fingerprint": "",
        "config_fingerprint": "",
        "changed": false,
        "sensors": [],
        "skipped": [],
        "summary": {"pass": 0, "fail": 0, "skip": 0, "verdict": "pass"},
    });
    std::fs::write(&legacy, document.to_string()).unwrap();

    let (code, _stdout, stderr) = run(harness(root)
        .arg("timeline")
        .arg("--evidence")
        .arg(arg(&legacy))
        .arg("--format")
        .arg("json"));
    assert_eq!(
        code,
        Some(2),
        "a refused artifact is a usage error: {stderr}"
    );
    assert!(stderr.contains("schema v2"), "{stderr}");
    assert!(stderr.contains("verify --record"), "{stderr}");
}

#[test]
fn attempts_from_different_workspaces_are_refused() {
    let fixture = fixture();
    let root = &fixture.root;
    let first = fixture.evidence("attempt1.json");
    let elsewhere = fixture.evidence("elsewhere.json");
    assert_eq!(record_attempt(root, &first), Some(1));
    let mut document: Value = serde_json::from_slice(&std::fs::read(&first).unwrap()).unwrap();
    document["root"] = Value::from("/somewhere/else");
    std::fs::write(&elsewhere, document.to_string()).unwrap();

    let (code, _stdout, stderr) = run(harness(root)
        .arg("timeline")
        .arg("--evidence")
        .arg(arg(&first))
        .arg("--evidence")
        .arg(arg(&elsewhere))
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stderr.contains("one workspace"), "{stderr}");
}

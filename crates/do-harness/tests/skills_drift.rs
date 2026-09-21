//! CLI contract for `do-harness skills drift`: exit codes, manifest trust, and
//! deterministic output.
//!
//! The checks drive the real binary, because the verdict is an exit code: `0`
//! clean, `1` drifted or missing, `2` unusable manifest.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use tempfile::{TempDir, tempdir};

/// Only the manifest's own pin decides what is managed; this tree is unmanaged
/// in every case unless a manifest names it.
const UNMANAGED: &str = "SKILL.md";

struct Sandbox {
    _dir: TempDir,
    root: PathBuf,
}

impl Sandbox {
    /// Builds a root with one managed tree at `.agents/skills/managed`.
    fn new(manifest: Option<&str>) -> Self {
        let dir = tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let managed = root.join(".agents/skills/managed");
        fs::create_dir_all(managed.join("references")).unwrap();
        fs::write(managed.join("SKILL.md"), "managed body\n").unwrap();
        fs::write(managed.join("references/notes.md"), "notes\n").unwrap();
        if let Some(text) = manifest {
            fs::write(root.join(".agents/skills-manifest.toml"), text).unwrap();
        }
        Self { _dir: dir, root }
    }

    fn write_manifest(&self, text: &str) {
        fs::write(self.root.join(".agents/skills-manifest.toml"), text).unwrap();
    }

    /// Runs `skills drift --format json`.
    fn drift(&self) -> Output {
        Command::new(env!("CARGO_BIN_EXE_do-harness"))
            .arg("--root")
            .arg(&self.root)
            .args(["skills", "drift", "--format", "json"])
            .output()
            .unwrap()
    }

    /// Runs `skills drift --manifest <path> --format json`.
    fn drift_with_manifest(&self, manifest: &std::path::Path) -> Output {
        Command::new(env!("CARGO_BIN_EXE_do-harness"))
            .arg("--root")
            .arg(&self.root)
            .args(["skills", "drift", "--manifest"])
            .arg(manifest)
            .args(["--format", "json"])
            .output()
            .unwrap()
    }

    /// Runs `skills drift` in its default text format.
    fn drift_text(&self) -> Output {
        Command::new(env!("CARGO_BIN_EXE_do-harness"))
            .arg("--root")
            .arg(&self.root)
            .args(["skills", "drift"])
            .output()
            .unwrap()
    }
}

/// Parses the JSON report a successful or drifted run prints.
fn report(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "skills drift --format json must print one JSON object; {err}; stdout: {}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

/// Manifest text for one managed skill at `path` pinned to `digest`.
fn manifest_text(path: &str, digest: &str) -> String {
    format!(
        "[[skills]]\nname = \"managed\"\npath = \"{path}\"\nupstream = \"d-o-hub/do-harness\"\ncommit = \"{}\"\ncontent_sha256 = \"{digest}\"\n",
        "a".repeat(40)
    )
}

#[test]
fn pinned_tree_is_clean_and_output_is_deterministic() {
    let sandbox = Sandbox::new(Some(&manifest_text(
        ".agents/skills/managed",
        &"0".repeat(64),
    )));

    // The placeholder pin cannot match, and the report carries the observed
    // digest — which is how a maintainer obtains the pin in the first place.
    let first = sandbox.drift();
    let drifted = report(&first);
    assert_eq!(first.status.code(), Some(1));
    assert_eq!(drifted["managed"], 1);
    assert_eq!(drifted["drifted"], 1);
    assert_eq!(drifted["skills"][0]["status"], "drift");
    assert_eq!(drifted["skills"][0]["path"], ".agents/skills/managed");
    let actual = drifted["skills"][0]["actual"].as_str().unwrap().to_owned();
    assert_eq!(actual.len(), 64);

    sandbox.write_manifest(&manifest_text(".agents/skills/managed", &actual));
    let first = sandbox.drift();
    let second = sandbox.drift();
    let clean = report(&first);
    assert_eq!(first.status.code(), Some(0));
    assert_eq!(clean["drifted"], 0);
    assert_eq!(clean["skills"][0]["status"], "ok");
    assert_eq!(
        first.stdout, second.stdout,
        "JSON output stays byte-identical across runs"
    );
}

#[test]
fn drifted_tree_fails_and_names_skill_and_path() {
    let sandbox = Sandbox::new(Some(&manifest_text(
        ".agents/skills/managed",
        &"0".repeat(64),
    )));
    let pinned = report(&sandbox.drift())["skills"][0]["actual"]
        .as_str()
        .unwrap()
        .to_owned();
    sandbox.write_manifest(&manifest_text(".agents/skills/managed", &pinned));

    fs::write(
        sandbox
            .root
            .join(".agents/skills/managed/references/notes.md"),
        "edited\n",
    )
    .unwrap();

    let output = sandbox.drift();
    let drifted = report(&output);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(drifted["skills"][0]["status"], "drift");
    assert_eq!(drifted["skills"][0]["name"], "managed");
    assert_eq!(drifted["skills"][0]["path"], ".agents/skills/managed");
    assert_eq!(drifted["skills"][0]["expected"], pinned);
    assert_ne!(drifted["skills"][0]["actual"], pinned);
}

#[test]
fn missing_managed_tree_fails_without_a_digest() {
    let sandbox = Sandbox::new(Some(&manifest_text(
        ".agents/skills/absent",
        &"0".repeat(64),
    )));

    let output = sandbox.drift();
    let missing = report(&output);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(missing["skills"][0]["status"], "missing");
    assert_eq!(missing["skills"][0]["path"], ".agents/skills/absent");
    assert!(
        missing["skills"][0].get("actual").is_none(),
        "a missing tree has no observed digest"
    );
}

#[test]
fn text_report_is_deterministic_and_names_the_skill() {
    let sandbox = Sandbox::new(Some(&manifest_text(
        ".agents/skills/managed",
        &"0".repeat(64),
    )));

    let first = sandbox.drift_text();
    let second = sandbox.drift_text();
    let text = String::from_utf8_lossy(&first.stdout);
    assert_eq!(first.status.code(), Some(1));
    assert_eq!(
        text,
        String::from_utf8_lossy(&second.stdout),
        "text output stays byte-identical across runs"
    );
    assert!(
        text.contains("DRIFT   managed .agents/skills/managed expected="),
        "the verdict names the skill, its path, and the pin: {text}"
    );
    assert!(
        text.contains("skills drift: 1 managed, 1 drifted"),
        "the summary counts what was checked: {text}"
    );
}

#[test]
fn absolute_override_manifest_is_reported_verbatim() {
    let sandbox = Sandbox::new(None);
    let elsewhere = tempdir().unwrap();
    let manifest = elsewhere.path().join("pins.toml");
    fs::write(
        &manifest,
        manifest_text(".agents/skills/managed", &"0".repeat(64)),
    )
    .unwrap();

    let output = sandbox.drift_with_manifest(&manifest);
    let report = report(&output);
    assert_eq!(
        output.status.code(),
        Some(1),
        "the override manifest is read, so the placeholder pin drifts"
    );
    assert_eq!(
        report["manifest"].as_str().unwrap(),
        manifest.to_string_lossy().replace('\\', "/"),
        "an absolute override outside the root is reported as given, with normalized separators"
    );
}

#[test]
fn absent_manifest_is_a_usage_error() {
    let sandbox = Sandbox::new(None);

    let output = sandbox.drift();
    assert_eq!(
        output.status.code(),
        Some(2),
        "an invoked check with nothing to check must not read as green"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(".agents/skills-manifest.toml"),
        "the usage error names the expected path: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty(), "a usage error prints no report");
}

#[test]
fn malformed_manifest_is_a_usage_error() {
    let sandbox = Sandbox::new(Some(&manifest_text(
        ".agents/skills/managed",
        "not-a-digest",
    )));

    let output = sandbox.drift();
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("content_sha256"),
        "usage errors name the offending field: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn unmanaged_skills_are_never_checked() {
    let sandbox = Sandbox::new(Some(&manifest_text(
        ".agents/skills/managed",
        &"0".repeat(64),
    )));
    let pinned = report(&sandbox.drift())["skills"][0]["actual"]
        .as_str()
        .unwrap()
        .to_owned();
    sandbox.write_manifest(&manifest_text(".agents/skills/managed", &pinned));

    // A second skill that the manifest does not name: it may exist, change, or
    // disappear without affecting the verdict.
    let unmanaged = sandbox.root.join(".agents/skills/unmanaged");
    fs::create_dir_all(&unmanaged).unwrap();
    fs::write(unmanaged.join(UNMANAGED), "unmanaged body\n").unwrap();

    let present = sandbox.drift();
    assert_eq!(present.status.code(), Some(0));
    assert_eq!(report(&present)["managed"], 1);

    fs::remove_dir_all(&unmanaged).unwrap();
    let absent = sandbox.drift();
    assert_eq!(absent.status.code(), Some(0));
}

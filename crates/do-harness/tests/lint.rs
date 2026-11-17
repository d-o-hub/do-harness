//! Acceptance fixtures for `do-harness lint`.
//!
//! The linter reads the guide layer only: a broken skill reference is an error
//! (exit 1), a skill no sensor names is a warning (exit 0), and a workspace
//! without `AGENTS.md` is a usage error rather than a green lint of nothing.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use serde_json::Value;

mod support;

use support::{harness, run};

const CONFIG: &str = r#"
[signal-sets]
all = ["harness-sensor"]

[[sensors]]
name = "harness-sensor"
argv = ["true"]
"#;

fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::write(root.join("do-harness.toml"), CONFIG).unwrap();
    (dir, root)
}

fn write_skill(root: &Path, name: &str) {
    let dir = root.join(".agents/skills").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("SKILL.md"), format!("# {name}\n")).unwrap();
}

#[test]
fn a_clean_guide_layer_passes() {
    let (_dir, root) = fixture();
    write_skill(&root, "harness-sensor");
    std::fs::write(
        root.join("AGENTS.md"),
        "# Contract\n\nSee `.agents/skills/harness-sensor/SKILL.md` for guidance.\n",
    )
    .unwrap();
    let (code, stdout, stderr) = run(harness(&root).arg("lint"));
    assert_eq!(code, Some(0), "{stderr}\n{stdout}");
    assert!(stdout.contains("0 error(s)"), "{stdout}");
}

#[test]
fn a_broken_skill_reference_is_an_error() {
    let (_dir, root) = fixture();
    std::fs::write(
        root.join("AGENTS.md"),
        "# Contract\n\nSee `.agents/skills/missing/SKILL.md`.\n",
    )
    .unwrap();
    let (code, stdout, _stderr) = run(harness(&root).arg("lint"));
    assert_eq!(code, Some(1), "a broken reference must fail the lint");
    assert!(stdout.contains("broken-skill-reference"), "{stdout}");
    assert!(stdout.contains("AGENTS.md:3"), "{stdout}");
}

#[test]
fn json_reports_structured_diagnostics() {
    let (_dir, root) = fixture();
    write_skill(&root, "orphan");
    std::fs::write(
        root.join("AGENTS.md"),
        "# Contract\n\nSee skill://ghost for details.\n",
    )
    .unwrap();
    let (code, stdout, stderr) = run(harness(&root).args(["lint", "--format", "json"]));
    assert_eq!(code, Some(1), "{stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("lint json");
    assert_eq!(report["ok"], serde_json::json!(false));
    let codes: Vec<&str> = report["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .map(|item| item["code"].as_str().expect("code"))
        .collect();
    assert!(codes.contains(&"broken-skill-reference"), "{codes:?}");
    assert!(codes.contains(&"skill-without-sensor"), "{codes:?}");
    assert_eq!(report["errors"], serde_json::json!(1));
    assert_eq!(report["warnings"], serde_json::json!(1));
}

#[test]
fn a_skill_no_sensor_names_is_a_warning_only() {
    let (_dir, root) = fixture();
    write_skill(&root, "harness-sensor");
    write_skill(&root, "advisory");
    std::fs::write(root.join("AGENTS.md"), "# Contract\n").unwrap();
    let (code, stdout, _stderr) = run(harness(&root).arg("lint"));
    assert_eq!(code, Some(0), "warnings must not fail the lint");
    assert!(stdout.contains("skill-without-sensor"), "{stdout}");
}

#[test]
fn a_duplicate_instruction_is_reported_once() {
    let (_dir, root) = fixture();
    write_skill(&root, "harness-sensor");
    let instruction = "Always run the verification set before calling the change done.";
    std::fs::write(
        root.join("AGENTS.md"),
        format!("# Contract\n\n{instruction}\n\n{instruction}\n"),
    )
    .unwrap();
    let (code, stdout, _stderr) = run(harness(&root).arg("lint"));
    assert_eq!(code, Some(0));
    assert!(stdout.contains("duplicate-instruction"), "{stdout}");
}

#[test]
fn a_missing_agents_file_is_a_usage_error() {
    let (_dir, root) = fixture();
    let (code, stdout, stderr) = run(harness(&root).arg("lint"));
    assert_eq!(code, Some(2), "no guide layer is not a green lint");
    assert!(stderr.contains("no AGENTS.md found"), "{stderr}");
    assert!(stdout.is_empty(), "{stdout}");
}

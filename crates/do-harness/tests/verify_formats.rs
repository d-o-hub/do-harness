//! Acceptance fixtures for `verify --simulate` and `verify --format junit`.
//!
//! Both surfaces are CI-facing contracts: the plan must not execute anything,
//! and the `JUnit` document must map every verdict (pass, failure, advisory,
//! unrunnable) onto the element a CI test tab understands.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use serde_json::Value;

mod support;

use support::{harness, run};

/// A config with three sensors and a signal set naming all of them.
const CONFIG: &str = r#"
[signal-sets]
all = ["ok", "bad", "soft"]

[[sensors]]
name = "ok"
argv = ["true"]
timeout = 30

[[sensors]]
name = "bad"
argv = ["sh", "-c", "echo boom; exit 1"]

[[sensors]]
name = "soft"
argv = ["sh", "-c", "exit 1"]
severity = "warn"
"#;

fn fixture(config: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::write(root.join("do-harness.toml"), config).unwrap();
    (dir, root)
}

#[test]
fn simulate_prints_a_plan_and_executes_nothing() {
    let (_dir, root) = fixture(
        r#"
[signal-sets]
all = ["probe"]

[[sensors]]
name = "probe"
argv = ["sh", "-c", "touch ran.marker"]
timeout = 12
"#,
    );
    let (code, stdout, stderr) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--simulate")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "simulate must exit 0:\n{stderr}");
    let plan: Value = serde_json::from_str(&stdout).expect("plan json");
    assert_eq!(plan["schema_version"], serde_json::json!(1));
    assert_eq!(plan["set"], serde_json::json!("all"));
    assert_eq!(plan["estimated_order"], serde_json::json!(["probe"]));
    assert_eq!(plan["sensors"][0]["name"], serde_json::json!("probe"));
    assert_eq!(
        plan["sensors"][0]["argv"],
        serde_json::json!(["sh", "-c", "touch ran.marker"])
    );
    assert_eq!(plan["sensors"][0]["timeout_secs"], serde_json::json!(12));
    assert_eq!(plan["sensors"][0]["sets"], serde_json::json!(["all"]));
    assert!(
        plan["notes"]
            .as_array()
            .expect("notes")
            .iter()
            .any(|note| note
                .as_str()
                .unwrap_or_default()
                .contains("no sensor was executed")),
        "{plan}"
    );
    // The whole contract: nothing ran and no evidence was written.
    assert!(
        !root.join("ran.marker").exists(),
        "simulate executed a sensor"
    );
    assert!(
        !root.join(".do-harness/evidence.json").exists(),
        "simulate wrote evidence"
    );
}

#[test]
fn simulate_cannot_write_state() {
    let (_dir, root) = fixture(CONFIG);
    let (code, _stdout, stderr) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--simulate")
        .arg("--record"));
    assert_eq!(code, Some(2), "simulate + record must be a usage error");
    assert!(
        stderr.contains("simulate") && stderr.contains("record"),
        "{stderr}"
    );
}

#[test]
fn simulate_rejects_junit() {
    let (_dir, root) = fixture(CONFIG);
    let (code, _stdout, stderr) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--simulate")
        .arg("--format")
        .arg("junit"));
    assert_eq!(code, Some(2));
    assert!(stderr.contains("--simulate"), "{stderr}");
}

#[test]
fn junit_maps_every_verdict() {
    let (_dir, root) = fixture(CONFIG);
    let (code, xml, _stderr) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--format")
        .arg("junit"));
    assert_eq!(code, Some(1), "a failing sensor must exit 1:\n{xml}");
    assert!(
        xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuites"),
        "{xml}"
    );
    assert!(xml.contains("tests=\"3\""), "{xml}");
    assert!(xml.contains("failures=\"1\""), "{xml}");
    assert!(
        xml.contains("<testcase classname=\"do-harness.verify\" name=\"ok\""),
        "{xml}"
    );
    assert!(
        xml.contains("<failure message=\"sensor failed with exit code 1\">boom"),
        "{xml}"
    );
    // An advisory failure is skipped, not failed: the process exits 1 only for
    // `bad`, and a red test for `soft` would contradict the exit code.
    assert!(
        xml.contains("<skipped message=\"advisory failure (exit code 1, severity warn)\"/>"),
        "{xml}"
    );
}

#[test]
fn junit_marks_an_unrunnable_sensor_as_an_error() {
    let (_dir, root) = fixture(
        r#"
[signal-sets]
all = ["missing"]

[[sensors]]
name = "missing"
argv = ["/nonexistent-do-harness-sensor"]
"#,
    );
    let (code, xml, _stderr) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--format")
        .arg("junit"));
    assert_eq!(code, Some(2), "an unrunnable run exits 2:\n{xml}");
    assert!(
        xml.contains("<error message=\"sensor could not run\">"),
        "{xml}"
    );
    // The OS error text differs by platform ("No such file or directory" vs
    // "The system cannot find the file specified."), so assert the shared
    // prefix and that the unspawnable program is named.
    assert!(
        xml.contains("failed to spawn /nonexistent-do-harness-sensor"),
        "{xml}"
    );
    assert!(xml.contains("os error 2"), "{xml}");
    assert!(xml.contains("errors=\"1\""), "{xml}");
}

#[test]
fn junit_never_carries_a_redacted_secret() {
    let (_dir, root) = fixture(
        r#"
[signal-sets]
all = ["leaky"]

[[sensors]]
name = "leaky"
argv = ["sh", "-c", "echo $LEAKY_SECRET; exit 1"]
redact-env = ["LEAKY_SECRET"]
"#,
    );
    let (code, xml, _stderr) = run(harness(&root)
        .env("LEAKY_SECRET", "s3cr3t-value")
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--format")
        .arg("junit"));
    assert_eq!(code, Some(1), "{xml}");
    assert!(!xml.contains("s3cr3t-value"), "secret reached JUnit: {xml}");
    assert!(xml.contains("<failure"), "{xml}");
}

#[test]
fn junit_escapes_markup_in_output() {
    let (_dir, root) = fixture(
        r#"
[signal-sets]
all = ["noisy"]

[[sensors]]
name = "noisy"
argv = ["sh", "-c", "printf '<tag> & \"quoted\"'; exit 1"]
"#,
    );
    let (code, xml, _stderr) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--format")
        .arg("junit"));
    assert_eq!(code, Some(1), "{xml}");
    assert!(
        xml.contains("&lt;tag&gt; &amp; &quot;quoted&quot;"),
        "{xml}"
    );
    assert!(
        !xml.contains("<tag>"),
        "raw markup leaked into the document"
    );
}

#[test]
fn text_and_json_formats_still_work() {
    let (_dir, root) = fixture(CONFIG);
    let (code, text, _stderr) = run(harness(&root).arg("verify").arg("--set").arg("all"));
    assert_eq!(code, Some(1));
    assert!(text.contains("PASS  ok"), "{text}");
    assert!(text.contains("FAIL  bad"), "{text}");

    let (code, json, _stderr) = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(1));
    let report: Value = serde_json::from_str(&json).expect("json report");
    assert_eq!(report["ok"], serde_json::json!(false));
}

/// The plan and the run must agree on selection for the same inputs.
#[test]
fn simulate_and_a_real_run_share_selection() {
    let (_dir, root) = fixture(CONFIG);
    let plan = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--simulate")
        .arg("--only")
        .arg("ok")
        .arg("--format")
        .arg("json"));
    assert_eq!(plan.0, Some(0), "{}", plan.2);
    let plan: Value = serde_json::from_str(&plan.1).expect("plan json");
    assert_eq!(plan["estimated_order"], serde_json::json!(["ok"]));

    let real = run(harness(&root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--only")
        .arg("ok")
        .arg("--format")
        .arg("json"));
    assert_eq!(real.0, Some(0));
    let real: Value = serde_json::from_str(&real.1).expect("json report");
    let names: Vec<&Path> = real["sensors"]
        .as_array()
        .expect("sensors")
        .iter()
        .map(|sensor| Path::new(sensor["name"].as_str().expect("name")))
        .collect();
    let planned: Vec<&Path> = plan["sensors"]
        .as_array()
        .expect("sensors")
        .iter()
        .map(|sensor| Path::new(sensor["name"].as_str().expect("name")))
        .collect();
    assert_eq!(names, planned);
}

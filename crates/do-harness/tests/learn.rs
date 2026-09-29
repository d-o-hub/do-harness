//! `do-harness learn --draft` acceptance: recurring sensor fires become a
//! concrete steering draft (guide row, skill name, commands), and the command
//! never writes.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

mod support;

use support::reuse::{fixture, harness, probe_config, run, verify_code};

/// Today's UTC date as `(year, month, day)` strings, so a synthetic event log
/// lands inside the default window regardless of when the suite runs.
fn today() -> (String, String, String) {
    let output = Command::new("date")
        .args(["-u", "+%Y %m %d"])
        .output()
        .expect("run date");
    assert!(output.status.success(), "date failed");
    let text = String::from_utf8_lossy(&output.stdout);
    let mut parts = text.split_whitespace();
    (
        parts.next().expect("year").to_owned(),
        parts.next().expect("month").to_owned(),
        parts.next().expect("day").to_owned(),
    )
}

/// Writes `count` `sensor-fire` events for `sensor` dated today.
fn write_fires(root: &Path, sensor: &str, count: usize) {
    let (year, month, day) = today();
    let dir = root.join(format!(".agents/events/{year}/{month}/{day}"));
    std::fs::create_dir_all(&dir).unwrap();
    for index in 0..count {
        let body = serde_json::json!({
            "date": format!("{year}-{month}-{day}"),
            "event": "sensor-fire",
            "guides": [{"kind": "reference", "path": ".agents/skills/harness/references/heuristics.md"}],
            "payload": {"sensor": sensor, "ok": false, "note": format!("fire {index}")},
        });
        std::fs::write(
            dir.join(format!("fire-{index}.json")),
            serde_json::to_string_pretty(&body).unwrap(),
        )
        .unwrap();
    }
}

/// Runs `learn --draft` and parses the JSON draft.
fn learn_json(root: &Path, args: &[&str]) -> (Value, String) {
    let mut cmd = harness(root);
    cmd.args(["learn", "--draft", "--format", "json"])
        .args(args);
    let (code, stdout, stderr) = run(&mut cmd);
    assert_eq!(code, Some(0), "learn exited {code:?}: {stdout}\n{stderr}");
    (serde_json::from_str(&stdout).expect("learn JSON"), stderr)
}

/// Three fires of one sensor produce a draft naming the sensor, the count, a
/// specification-valid skill name, and the guide row to add.
#[test]
fn three_event_fires_produce_a_steering_draft() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_fires(root, "Audit_Retry", 3);

    let (draft, _) = learn_json(root, &[]);
    let sensor = &draft["sensors"][0];
    assert_eq!(sensor["sensor"], "Audit_Retry");
    assert_eq!(sensor["fires"], 3);
    assert_eq!(sensor["event_fires"], 3);
    assert_eq!(sensor["beat_fires"], 0);
    assert_eq!(
        sensor["guide"],
        ".agents/skills/harness/references/heuristics.md"
    );

    let skill = sensor["skill_name"].as_str().expect("skill name");
    assert_eq!(skill, "audit-retry-steering");
    assert!(skill.len() <= 64, "{skill}");
    assert!(
        skill
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
        "{skill}"
    );
    assert!(
        !skill.starts_with('-') && !skill.ends_with('-') && !skill.contains("--"),
        "{skill}"
    );

    let row = sensor["guide_row"].as_str().expect("guide row");
    assert!(row.starts_with("- **"), "{row}");
    assert!(row.contains("Audit_Retry"), "{row}");
    assert!(row.contains("(from trace <N>)"), "{row}");

    // Nothing was written: the draft only proposes.
    assert!(
        !root.join(".do-harness").exists(),
        "learn must not create state"
    );
}

/// The text draft prints the sensor, its fire count, the guide row, and the
/// apply checklist.
#[test]
fn text_draft_prints_the_steering_actions() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_fires(root, "probe", 4);

    let mut cmd = harness(root);
    cmd.args(["learn", "--draft"]);
    let (code, stdout, stderr) = run(&mut cmd);
    assert_eq!(code, Some(0), "{stdout}\n{stderr}");
    assert!(
        stdout.contains("1 sensor(s) fired >= 3 time(s)"),
        "{stdout}"
    );
    assert!(stdout.contains("probe — 4 fire(s)"), "{stdout}");
    assert!(
        stdout.contains("guide row: - **<fix that stops probe recurring>**"),
        "{stdout}"
    );
    assert!(stdout.contains("init_skill.py probe-steering"), "{stdout}");
    assert!(
        stdout.contains("apply checklist (nothing was written)"),
        "{stdout}"
    );
}

/// Fires below the threshold are reported separately, and `--min-fires` lifts
/// them into the draft.
#[test]
fn threshold_controls_what_is_drafted() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_fires(root, "probe", 2);

    let (draft, _) = learn_json(root, &[]);
    assert_eq!(draft["sensors"].as_array().unwrap().len(), 0, "{draft}");
    assert_eq!(draft["below_threshold"][0]["sensor"], "probe");
    assert_eq!(draft["below_threshold"][0]["fires"], 2);

    let (draft, _) = learn_json(root, &["--min-fires", "1"]);
    assert_eq!(draft["sensors"][0]["sensor"], "probe", "{draft}");
    assert_eq!(draft["sensors"][0]["fires"], 2);
}

/// A fire outside the window is not counted.
#[test]
fn window_excludes_old_fires() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let old = root.join(".agents/events/2001/01/01");
    std::fs::create_dir_all(&old).unwrap();
    std::fs::write(
        old.join("fire.json"),
        r#"{"date":"2001-01-01","event":"sensor-fire","payload":{"sensor":"probe"}}"#,
    )
    .unwrap();

    let (draft, _) = learn_json(root, &[]);
    assert_eq!(draft["sensors"].as_array().unwrap().len(), 0, "{draft}");
    assert_eq!(
        draft["below_threshold"].as_array().unwrap().len(),
        0,
        "{draft}"
    );
}

/// Recorded failing beats are a fire source too.
#[test]
fn recorded_failures_are_counted_from_beats() {
    let (_dir, root) = fixture(&probe_config(
        "name = \"probe\"\nargv = [\"sh\", \"-c\", \"exit 1\"]\n",
    ));
    for _ in 0..3 {
        verify_code(&root, &["--record"], 1);
    }

    let (draft, _) = learn_json(&root, &[]);
    let sensor = &draft["sensors"][0];
    assert_eq!(sensor["sensor"], "probe", "{draft}");
    assert_eq!(sensor["beat_fires"], 3, "{draft}");
}

/// A symlink loop under the event tree cannot hang or crash the walk.
#[test]
fn symlink_loops_do_not_hang_the_walk() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_fires(root, "probe", 3);
    #[cfg(unix)]
    {
        let day = root.join(".agents/events");
        std::os::unix::fs::symlink(&day, day.join("loop")).unwrap();
    }
    let (draft, _) = learn_json(root, &[]);
    assert_eq!(draft["sensors"][0]["fires"], 3, "{draft}");
}

/// Invalid windows and thresholds are usage errors, not silent defaults.
#[test]
fn invalid_window_and_threshold_are_usage_errors() {
    let dir = tempfile::tempdir().unwrap();
    for args in [
        ["learn", "--draft", "--days", "0"],
        ["learn", "--draft", "--min-fires", "0"],
    ] {
        let mut cmd = harness(dir.path());
        cmd.args(args);
        let (code, stdout, stderr) = run(&mut cmd);
        assert_eq!(code, Some(2), "{args:?}: {stdout}\n{stderr}");
    }
}

/// `--draft` is required: the command never applies changes implicitly.
#[test]
fn learn_without_draft_is_a_usage_error() {
    let dir = tempfile::tempdir().unwrap();
    let mut cmd = harness(dir.path());
    cmd.arg("learn");
    let (code, stdout, stderr) = run(&mut cmd);
    assert_eq!(code, Some(2), "{stdout}\n{stderr}");
    assert!(stderr.contains("--draft"), "{stderr}");
}

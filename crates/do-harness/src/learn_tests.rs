#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::sources::*;
use super::*;
use crate::events::days_from_civil;

/// Proposed names obey the published Agent Skills specification.
#[test]
fn skill_name_normalizes_to_the_published_spec() {
    assert_eq!(skill_name("Audit_Retry"), "audit-retry-steering");
    assert_eq!(skill_name("--weird--name--"), "weird-name-steering");
    assert_eq!(skill_name("___"), "sensor-steering");
    assert_eq!(skill_name("check"), "check-steering");

    let long = skill_name(&"very-long-sensor-name-".repeat(8));
    assert!(long.len() <= MAX_SKILL_NAME, "{long}");
    assert!(!long.ends_with('-'), "{long}");
    assert!(!long.contains("--"), "{long}");
    assert!(long.starts_with("very-long-sensor-name"), "{long}");

    for name in [skill_name("Audit_Retry"), long] {
        assert!(
            name.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "{name}"
        );
        assert!(!name.starts_with('-') && !name.ends_with('-'), "{name}");
        assert!(!name.contains("--"), "{name}");
    }
}

/// Civil-day arithmetic matches the epoch and round-trips with `events`.
#[test]
fn days_from_civil_matches_the_epoch_and_round_trips() {
    assert_eq!(days_from_civil(1970, 1, 1), 0);
    assert_eq!(days_from_civil(2000, 3, 1), 11_017);
    for now in [0_i64, 1_700_000_000, 1_790_000_000] {
        let (year, month, day) = crate::events::civil_from_unix(now);
        assert_eq!(
            days_from_civil(year, i64::from(month), i64::from(day)),
            now.div_euclid(86_400),
            "round trip at {now}"
        );
    }
}

/// The threshold splits drafts from near misses, ordered by fires.
#[test]
fn draft_splits_at_the_threshold() {
    let mut fires = BTreeMap::new();
    fires.insert(
        "a".to_owned(),
        Fires {
            beats: 3,
            events: 0,
        },
    );
    fires.insert(
        "b".to_owned(),
        Fires {
            beats: 2,
            events: 0,
        },
    );
    fires.insert(
        "c".to_owned(),
        Fires {
            beats: 1,
            events: 2,
        },
    );
    fires.insert("quiet".to_owned(), Fires::default());

    let report = draft(fires, 30, 3);
    let names: Vec<&str> = report.sensors.iter().map(|d| d.sensor.as_str()).collect();
    assert_eq!(
        names,
        ["a", "c"],
        "both at or above the threshold, most fires first"
    );
    assert_eq!(report.sensors[1].fires, 3);
    assert_eq!(report.sensors[1].beat_fires, 1);
    assert_eq!(report.sensors[1].event_fires, 2);
    let below: Vec<(&str, u32)> = report
        .below_threshold
        .iter()
        .map(|miss| (miss.sensor.as_str(), miss.fires))
        .collect();
    assert_eq!(
        below,
        [("b", 2)],
        "a zero-fire sensor is not reported at all"
    );
}

/// A draft names the sensor in every artifact it proposes.
#[test]
fn draft_names_the_sensor_in_each_artifact() {
    let draft = draft_for(
        "Audit_Retry",
        Fires {
            beats: 2,
            events: 1,
        },
    );
    assert_eq!(draft.fires, 3);
    assert_eq!(draft.skill_name, "audit-retry-steering");
    assert_eq!(draft.guide, DEFAULT_GUIDE);
    assert!(
        draft.guide_row.contains("Audit_Retry"),
        "{}",
        draft.guide_row
    );
    assert!(
        draft.guide_row.contains("(from trace <N>)"),
        "{}",
        draft.guide_row
    );
    assert!(
        draft.trace_command.starts_with("do-harness trace add"),
        "{}",
        draft.trace_command
    );
    assert!(
        draft.distill_command.contains("audit-retry-steering")
            || draft.distill_command.contains(DEFAULT_SKILL),
        "{}",
        draft.distill_command
    );
    assert!(
        draft
            .scaffold_command
            .contains("init_skill.py audit-retry-steering"),
        "{}",
        draft.scaffold_command
    );
    assert!(
        draft.changelog.contains("Audit_Retry"),
        "{}",
        draft.changelog
    );
}

/// Event records count as fires by name, window, and explicit status.
#[test]
fn event_fires_counts_only_in_window_fires() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let day = root.join(".agents/events/2026/09/29");
    std::fs::create_dir_all(&day).unwrap();
    let write = |name: &str, body: &str| std::fs::write(day.join(name), body).unwrap();

    write(
        "fire.json",
        r#"{"date":"2026-09-29","event":"sensor-fire","payload":{"sensor":"probe","ok":false}}"#,
    );
    write(
        "fire-without-status.json",
        r#"{"date":"2026-09-29","event":"sensor-fire","payload":{"sensor":"probe"}}"#,
    );
    write(
        "passed.json",
        r#"{"date":"2026-09-29","event":"sensor-fire","payload":{"sensor":"probe","ok":true}}"#,
    );
    write(
        "warned.json",
        r#"{"date":"2026-09-29","event":"sensor-fire","payload":{"sensor":"other","status":"failed"}}"#,
    );
    write(
        "ok-status.json",
        r#"{"date":"2026-09-29","event":"sensor-fire","payload":{"sensor":"other","status":"ok"}}"#,
    );
    write(
        "unrelated.json",
        r#"{"date":"2026-09-29","event":"distill","payload":{"sensor":"ignored"}}"#,
    );
    write(
        "old.json",
        r#"{"date":"2001-01-01","event":"sensor-fire","payload":{"sensor":"probe"}}"#,
    );
    write("garbage.json", "not json");

    // 2026-09-29 is inside a 30-day window that starts 29 days earlier.
    let start = days_from_civil(2026, 9, 29) - 29;
    let fires = event_fires(root, start).unwrap();
    assert_eq!(
        fires.get("probe"),
        Some(&2),
        "passed and old records excluded"
    );
    assert_eq!(
        fires.get("other"),
        Some(&1),
        "ok status excluded, failed counted"
    );

    // The same window excludes everything older than its first day.
    let fires = event_fires(root, days_from_civil(2026, 10, 1)).unwrap();
    assert!(fires.is_empty(), "{fires:?}");
}

/// A date outside the window is skipped; a missing date is kept (fail-open for
/// the draft, which is advisory).
#[test]
fn window_filter_uses_field_then_path() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let start = days_from_civil(2026, 9, 1);
    let recent = serde_json::json!({ "date": "2026-09-29", "event": "sensor-fire" });
    let old = serde_json::json!({ "date": "2026-08-01", "event": "sensor-fire" });
    let undated = serde_json::json!({ "event": "sensor-fire" });
    let path = root.join(".agents/events/2026/09/29/fire.json");
    assert!(in_window(root, &path, &recent, start));
    assert!(!in_window(root, &path, &old, start));
    assert!(
        in_window(root, Path::new("missing.json"), &undated, start),
        "an undated record has no evidence against it"
    );
}

/// The window's first day is inclusive: a 30-day window spans 30 days.
#[test]
fn window_start_is_inclusive() {
    let now = 1_790_000_000;
    let (year, month, day) = crate::events::civil_from_unix(now);
    let today = days_from_civil(year, i64::from(month), i64::from(day));
    assert_eq!(window_start(now, 1), today);
    assert_eq!(window_start(now, 30), today - 29);
}

/// The window start handed to SQL is Unix seconds, not the civil-day ordinal.
#[test]
fn window_start_seconds_are_unix_seconds_in_the_window() {
    let now = 1_790_000_000;
    let day = window_start(now, 30);
    let seconds = day * SECONDS_PER_DAY;
    assert_eq!(seconds % SECONDS_PER_DAY, 0, "midnight-aligned: {seconds}");
    assert!(seconds <= now, "not in the future: {seconds}");
    assert!(
        now - seconds < 31 * SECONDS_PER_DAY,
        "inside the window: {seconds}"
    );
    assert!(
        day < seconds,
        "the ordinal is a day count, not a timestamp: {day}"
    );
}

/// A hostile sensor name cannot close a quoted fragment in a drafted command.
///
/// The drafted line is meant to be pasted into a shell, so a name carrying a
/// quote or a command substitution must arrive as one literal argument.
#[test]
fn drafted_commands_quote_a_hostile_sensor_name() {
    let hostile = "bad'name $(touch pwned)";
    let draft = draft_for(
        hostile,
        Fires {
            beats: 3,
            events: 0,
        },
    );
    for command in [&draft.trace_command, &draft.distill_command] {
        assert!(
            command.contains(r"'\''"),
            "the quote must be escaped for the shell: {command}"
        );
        assert!(!command.contains('\n'), "newline leaked: {command}");
    }
    assert!(!draft.guide_row.contains('\n'), "{}", draft.guide_row);
    assert!(!draft.changelog.contains('\n'), "{}", draft.changelog);
    assert_eq!(draft.skill_name, "bad-name-touch-pwned-steering");

    // Behavioural proof: run the drafted line with a stand-in binary that
    // prints its arguments, and require the hostile text to survive as one
    // argument (no word splitting, no command substitution).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("do-harness");
        std::fs::write(
            &bin,
            "#!/bin/sh\nfor arg in \"$@\"; do printf '[%s]\\n' \"$arg\"; done\n",
        )
        .unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = format!(
            "{}:{}",
            dir.path().display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(&draft.trace_command)
            .env("PATH", path)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("[<bad'name $(touch pwned) failure output>]"),
            "the hostile name must arrive as one literal argument:\n{stdout}"
        );
        assert!(!dir.path().join("pwned").exists(), "substitution ran");
    }
}

/// Out-of-range date components are rejected instead of overflowing.
#[test]
fn out_of_range_dates_are_rejected() {
    assert_eq!(parse_date("2026-09-29"), Some(days_from_civil(2026, 9, 29)));
    assert_eq!(parse_date("99999999999999999-01-01"), None);
    assert_eq!(parse_date("2026-13-01"), None);
    assert_eq!(parse_date("2026-09-32"), None);
    assert_eq!(parse_date("2026-00-10"), None);
    assert_eq!(parse_date("0-01-01"), None);
    assert_eq!(parse_date("2026-09"), None);
}

/// An extreme date in an event path or field cannot panic the walk.
#[test]
fn extreme_dates_do_not_panic_the_walk() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let day = root.join(".agents/events/2026/09/29");
    std::fs::create_dir_all(&day).unwrap();
    std::fs::write(
        day.join("extreme.json"),
        r#"{"date":"99999999999999999-01-01","event":"sensor-fire","payload":{"sensor":"probe"}}"#,
    )
    .unwrap();
    let far = root.join(".agents/events/99999999999999999/01/01");
    std::fs::create_dir_all(&far).unwrap();
    std::fs::write(
        far.join("fire.json"),
        r#"{"date":"99999999999999999-01-01","event":"sensor-fire","payload":{"sensor":"probe"}}"#,
    )
    .unwrap();

    // The date field is unusable, so the record falls back to its own path
    // (today) and still counts; the far-future path resolves to no day and is
    // kept, because an undated record is not evidence against itself.
    let start = days_from_civil(2026, 9, 29) - 29;
    let fires = event_fires(root, start).unwrap();
    assert_eq!(fires.get("probe"), Some(&2), "no panic, no loss");
}

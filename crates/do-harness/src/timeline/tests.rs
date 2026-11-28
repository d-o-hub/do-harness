//! Unit tests for the verification timeline: schema window, chain gaps,
//! fingerprint binding, and escaping.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use serde_json::json;

use super::{Attempt, Snapshot, Timeline, bind, chain_gaps, observations_for, render};

/// Builds an attempt from the fields the timeline reads.
fn attempt(path: &str, overrides: &serde_json::Value) -> Attempt {
    let mut document = json!({
        "schema_version": crate::evidence::EVIDENCE_SCHEMA_VERSION,
        "tool": "do-harness",
        "harness_version": "0.6.0",
        "git_sha": "aaaa1111",
        "started_at": 10,
        "finished_at": 12,
        "root": "/work/repo",
        "task_id": serde_json::Value::Null,
        "sensor_pack": "default",
        "signal_set": "verification",
        "workspace_fingerprint": "sha256:ws-1",
        "policy_fingerprint": "sha256:policy-1",
        "config_fingerprint": "sha256:config-1",
        "changed": false,
        "sensors": [{"name": "test", "argv": ["cargo", "test"], "verdict": "fail",
                     "exit_code": 1, "duration_ms": 1500, "output_sha256": "deadbeef",
                     "recorded": true}],
        "skipped": [],
        "summary": {"pass": 0, "fail": 1, "skip": 0, "verdict": "fail"},
        "chain_hash": "chain-1",
    });
    if let (Some(base), Some(extra)) = (document.as_object_mut(), overrides.as_object()) {
        for (key, value) in extra {
            base.insert(key.clone(), value.clone());
        }
    }
    Attempt {
        session: None,
        path: PathBuf::from(path),
        sha256: format!("sha256:{}", path.replace('.', "-")),
        document: serde_json::from_value(document).expect("valid evidence document"),
    }
}

/// Builds a captured status snapshot.
fn snapshot(state: &str, reason: &str, workspace: &str, policy: &str) -> Snapshot {
    Snapshot {
        path: PathBuf::from(format!("{state}.json")),
        observed_at: 1_760_000_000,
        state: state.to_owned(),
        reason: reason.to_owned(),
        set: Some("verification".to_owned()),
        workspace_fingerprint: Some(workspace.to_owned()),
        policy_fingerprint: Some(policy.to_owned()),
        artifact_sha256: None,
    }
}

/// A snapshot captured after the artifact digest contract landed.
fn snapshot_of(state: &str, reason: &str, digest: &str) -> Snapshot {
    Snapshot {
        artifact_sha256: Some(digest.to_owned()),
        ..snapshot(state, reason, "sha256:ws-1", "sha256:policy-1")
    }
}

#[test]
fn chain_gaps_report_a_broken_link_between_supplied_attempts() {
    let first = attempt("a.json", &json!({"chain_hash": "chain-1"}));
    let second = attempt(
        "b.json",
        &json!({"prev_hash": "chain-OTHER", "chain_hash": "chain-2"}),
    );
    let gaps = chain_gaps(&[first, second]);
    assert_eq!(gaps.len(), 1, "one gap: {gaps:?}");
    assert!(gaps[0].contains("does not follow"), "{gaps:?}");
}

#[test]
fn chain_gaps_are_silent_when_the_hashes_link() {
    let first = attempt("a.json", &json!({"chain_hash": "chain-1"}));
    let second = attempt(
        "b.json",
        &json!({"prev_hash": "chain-1", "chain_hash": "chain-2"}),
    );
    assert_eq!(
        chain_gaps(&[first, second]).len(),
        0,
        "a linked chain has no gaps"
    );
}

#[test]
fn an_invalidated_run_is_reported_as_a_gap() {
    let invalidated = attempt(
        "a.json",
        &json!({"invalidated_reason": "inputs_changed_during_run"}),
    );
    let gaps = chain_gaps(&[invalidated]);
    assert_eq!(gaps.len(), 1);
    assert!(gaps[0].contains("inputs_changed_during_run"), "{gaps:?}");
}

#[test]
fn a_snapshot_binds_to_the_attempt_it_observed() {
    let failing = attempt("a.json", &json!({}));
    let passing = attempt(
        "b.json",
        &json!({"workspace_fingerprint": "sha256:ws-2", "summary": {"pass": 1, "fail": 0, "skip": 0, "verdict": "pass"}}),
    );
    let bindings = bind(
        &[snapshot(
            "stale",
            "workspace_changed",
            "sha256:ws-2",
            "sha256:policy-1",
        )],
        &[failing, passing],
    );
    assert_eq!(bindings[0].attempt, Some(1), "binds to the second attempt");
    assert!(
        bindings[0]
            .note
            .as_deref()
            .unwrap_or_default()
            .contains("recorded fingerprints"),
        "a snapshot without a digest says how it was bound: {:?}",
        bindings[0].note
    );
}

#[test]
fn the_artifact_digest_binds_even_when_fingerprints_collide() {
    let first = attempt("a.json", &json!({"chain_hash": "chain-1"}));
    let second = attempt("b.json", &json!({"prev_hash": "chain-1"}));
    let digest = second.sha256.clone();
    let bindings = bind(
        &[snapshot_of("green", "current", &digest)],
        &[first, second],
    );
    assert_eq!(
        bindings[0].attempt,
        Some(1),
        "the digest identifies the attempt even though both share fingerprints"
    );
    assert!(bindings[0].note.is_none());
}

#[test]
fn a_snapshot_of_an_unsupplied_artifact_is_reported_unbound() {
    let only = attempt("a.json", &json!({}));
    let bindings = bind(
        &[snapshot_of("green", "current", "sha256:not-supplied")],
        &[only],
    );
    assert_eq!(bindings[0].attempt, None);
    assert!(
        bindings[0]
            .note
            .as_deref()
            .unwrap_or_default()
            .contains("none of the supplied"),
        "{:?}",
        bindings[0].note
    );
}

#[test]
fn identical_fingerprints_are_reported_as_ambiguous_instead_of_guessed() {
    let first = attempt("a.json", &json!({"chain_hash": "chain-1"}));
    let second = attempt("b.json", &json!({"prev_hash": "chain-1"}));
    let bindings = bind(
        &[snapshot(
            "red",
            "failures",
            "sha256:ws-1",
            "sha256:policy-1",
        )],
        &[first, second],
    );
    assert_eq!(bindings[0].attempt, None);
    assert!(
        bindings[0]
            .note
            .as_deref()
            .unwrap_or_default()
            .contains("ambiguous"),
        "{:?}",
        bindings[0].note
    );
}

#[test]
fn an_unrelated_snapshot_is_unbound_and_reported() {
    let only = attempt("a.json", &json!({}));
    let bindings = bind(
        &[snapshot("green", "current", "sha256:other", "sha256:other")],
        &[only],
    );
    assert_eq!(bindings[0].attempt, None);
    assert!(
        bindings[0]
            .note
            .as_deref()
            .unwrap_or_default()
            .contains("unbound"),
        "{:?}",
        bindings[0].note
    );
}

#[test]
fn html_escapes_every_recorded_string() {
    let hostile = attempt(
        "a.json",
        &json!({"sensors": [{"name": "<script>alert(1)</script>", "argv": [], "verdict": "fail",
                             "exit_code": 1, "duration_ms": 5, "output_sha256": "",
                             "recorded": true}]}),
    );
    let page = render::html(&Timeline {
        attempts: vec![hostile],
        bindings: vec![],
        gaps: vec!["<b>gap</b>".to_owned()],
    });
    assert!(!page.contains("<script>"), "raw script tag leaked");
    assert!(
        page.contains("&lt;script&gt;"),
        "escaped sensor name missing"
    );
    assert!(
        page.contains("&lt;b&gt;gap&lt;/b&gt;"),
        "escaped gap missing"
    );
}

#[test]
fn attempts_without_a_bound_snapshot_render_unknown_freshness() {
    let only = attempt("a.json", &json!({}));
    let timeline = Timeline {
        attempts: vec![only],
        bindings: vec![],
        gaps: vec![],
    };
    assert!(observations_for(&timeline, 0).is_empty());
    let page = render::html(&timeline);
    assert!(page.contains("freshness: unknown"), "{page}");
}

#[test]
fn the_json_document_carries_attempts_durations_and_gaps() {
    let only = attempt("a.json", &json!({}));
    let timeline = Timeline {
        attempts: vec![only],
        bindings: vec![],
        gaps: vec!["gap".to_owned()],
    };
    let document: serde_json::Value =
        serde_json::from_str(&render::json(&timeline).unwrap()).unwrap();
    assert_eq!(document["schema_version"], json!(super::TIMELINE_SCHEMA));
    assert_eq!(document["order"], json!("supplied"));
    assert_eq!(
        document["attempts"][0]["sensors"][0]["duration_ms"],
        json!(1500)
    );
    assert_eq!(document["gaps"][0], json!("gap"));
}

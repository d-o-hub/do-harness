//! Hermetic integration tests for the native `design-review` command:
//! packet assembly, provenance pinning, identity hashing, and the
//! disposition workflow. The reviewer process is the real CLI binary
//! running its own `design-review bridge` action; no model is ever
//! launched. Outcome-taxonomy tests live in `design_review_outcomes.rs`
//! and the bridge command surface in `design_review_bridge.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "support/design_review.rs"]
#[allow(dead_code)]
mod support;

use std::process::Command;
use support::*;

use serde_json::{Value, json};

#[test]
fn design_review_packet_assembles_not_run_and_pins_provenance() {
    let (_keep_dir, tmp) = temp_dir("packet");
    let packet_out = tmp.join("packet.json");
    let record_out = tmp.join("record.json");
    let args = vec![
        "design-review".to_owned(),
        "run".to_owned(),
        "--design-doc".to_owned(),
        format!("{FIXTURES}/sound-design.md"),
        "--record-out".to_owned(),
        path_str(&record_out),
        "--packet-out".to_owned(),
        path_str(&packet_out),
        "--objective".to_owned(),
        "Add a nullable retention column with a bounded backfill.".to_owned(),
        "--proposed-contract".to_owned(),
        "ADD COLUMN retention_hours INTEGER NULL".to_owned(),
        "--doc-path".to_owned(),
        format!("{FIXTURES}/sound-design.md"),
        "--meta".to_owned(),
        "{\"reviewer_provenance\":\"operator-supplied\"}".to_owned(),
    ];
    let (code, stdout, stderr) = run_cli(&args);
    assert_eq!(code, Some(0), "run failed: {stdout} {stderr}");
    let record = record_of(&record_out);
    assert_eq!(record["outcome"], "not_run");
    assert_eq!(record["reviewer_mode"], "none");
    assert_eq!(record["experiment"], "fresh-context-review-pilot");
    assert_eq!(record["dispositions"], json!([]));
    assert_eq!(record["meta"]["reviewer_provenance"], "operator-supplied");
    assert_eq!(record["isolation"]["marker_probe_configured"], false);

    let packet: Value =
        serde_json::from_str(&std::fs::read_to_string(&packet_out).unwrap()).unwrap();
    assert_eq!(packet["packet_hash"], record["packet_hash"]);
    assert_eq!(packet["schema_version"], 1);
    let commit = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(repo_root())
            .output()
            .expect("git rev-parse")
            .stdout,
    )
    .expect("git output is UTF-8");
    assert_eq!(packet["revisions"]["commit"], commit.trim());
    assert_eq!(
        packet["revisions"]["design_doc"]["path"],
        format!("{FIXTURES}/sound-design.md")
    );
    assert_eq!(
        packet["reviewer_note"],
        "The proposed contract and decision rationale live in the frozen design document itself. No author deliberation transcript, chat log, or summary telling the reviewer which conclusion to reach is included in this packet."
    );
}

#[test]
fn design_review_revision_pinning_fails_closed() {
    let (_keep_dir, tmp) = temp_dir("pinning");
    let record_out = tmp.join("record.json");
    let args = vec![
        "design-review".to_owned(),
        "run".to_owned(),
        "--design-doc".to_owned(),
        format!("{FIXTURES}/sound-design.md"),
        "--record-out".to_owned(),
        path_str(&record_out),
        "--objective".to_owned(),
        "x".to_owned(),
        "--proposed-contract".to_owned(),
        "y".to_owned(),
        "--doc-path".to_owned(),
        "does/not/exist.md".to_owned(),
    ];
    let (code, _stdout, stderr) = run_cli(&args);
    assert_eq!(
        code,
        Some(2),
        "missing pinned path must be a usage error: {stderr}"
    );
    assert!(stderr.contains("does not exist at"), "stderr: {stderr}");
}

#[test]
fn design_review_packet_hash_is_stable_and_sensitive() {
    let (_keep_dir, tmp) = temp_dir("hash");
    let review = |objective: &str, name: &str| {
        let record_out = tmp.join(name);
        let args = vec![
            "design-review".to_owned(),
            "run".to_owned(),
            "--design-doc".to_owned(),
            format!("{FIXTURES}/sound-design.md"),
            "--record-out".to_owned(),
            path_str(&record_out),
            "--review-id".to_owned(),
            "hash-stability".to_owned(),
            "--objective".to_owned(),
            objective.to_owned(),
            "--proposed-contract".to_owned(),
            "c".to_owned(),
        ];
        let (code, _out, err) = run_cli(&args);
        assert_eq!(code, Some(0), "{err}");
        record_of(&record_out)
    };
    let first = review("same objective", "a.json");
    let second = review("same objective", "b.json");
    let third = review("changed objective", "c.json");
    assert_eq!(first["packet_hash"], second["packet_hash"]);
    assert_ne!(first["packet_hash"], third["packet_hash"]);
}

#[test]
fn design_review_disposition_workflow() {
    let (_keep_dir, tmp) = temp_dir("dispose");
    let record_out = tmp.join("record.json");
    let (_keep_dir, handoff) = temp_dir("dispose-handoff");
    place_fixture_findings(&handoff, "flawed-expected.json");
    let mut args = run_args("dispose", &record_out);
    args.extend_from_slice(&[
        "--reviewer-command-json".to_owned(),
        bridge_reviewer(&handoff, "30"),
        "--parent-marker".to_owned(),
        "DESIGN-REVIEW-UT-MARKER-4".to_owned(),
    ]);
    let (code, _out, err) = run_cli(&args);
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(record_of(&record_out)["dispositions"], json!([]));

    let dispose = |finding: &str, status: &str, note: Option<&str>| {
        let mut args = vec![
            "design-review".to_owned(),
            "dispose".to_owned(),
            "--record".to_owned(),
            path_str(&record_out),
            "--finding-id".to_owned(),
            finding.to_owned(),
            "--status".to_owned(),
            status.to_owned(),
        ];
        if let Some(note) = note {
            args.push("--note".to_owned());
            args.push(note.to_owned());
        }
        run_cli(&args)
    };
    let (code, _out, err) = dispose("F1", "confirmed", Some("planted defect identified"));
    assert_eq!(code, Some(0), "{err}");
    let record = record_of(&record_out);
    assert_eq!(record["dispositions"][0]["status"], "confirmed");
    assert_eq!(
        record["dispositions"][0]["note"],
        "planted defect identified"
    );

    // Re-dispositioning the same finding replaces the entry.
    let (code, _out, err) = dispose("F1", "rejected", None);
    assert_eq!(code, Some(0), "{err}");
    let record = record_of(&record_out);
    let dispositions = record["dispositions"].as_array().expect("dispositions");
    assert_eq!(dispositions.len(), 1);
    assert_eq!(record["dispositions"][0]["status"], "rejected");
    assert_eq!(
        record["outcome"], "success_with_findings",
        "disposition preserves the outcome"
    );

    let (code, _out, err) = dispose("F9", "confirmed", None);
    assert_eq!(
        code,
        Some(2),
        "unknown finding id must be a usage error: {err}"
    );
    assert!(err.contains("finding 'F9' not present in record"));
}

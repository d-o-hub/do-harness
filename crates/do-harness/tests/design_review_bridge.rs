//! Hermetic integration tests for the `design-review bridge` action: the
//! reviewer process contract (packet on stdin, findings on stdout) that
//! host-side reviewers are wired through.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::Value;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(Path::to_path_buf)
        .expect("repository root")
}

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_do-harness")
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "do-harness-design-review-bridge-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn handoff_str(handoff: &Path) -> String {
    handoff.to_string_lossy().into_owned()
}

/// Spawns the bridge with a packet on stdin and returns (code, stdout, stderr).
fn run_bridge(handoff: &Path, max_wait_s: &str, packet: &str) -> (Option<i32>, String, String) {
    let mut child = Command::new(binary())
        .args([
            "design-review",
            "bridge",
            "--handoff-dir",
            &handoff_str(handoff),
            "--max-wait-s",
            max_wait_s,
        ])
        .current_dir(repo_root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn bridge");
    child
        .stdin
        .take()
        .expect("bridge stdin")
        .write_all(packet.as_bytes())
        .expect("write packet");
    let output = child.wait_with_output().expect("bridge output");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn design_review_bridge_relays_packet_and_findings() {
    let handoff = temp_dir("relay");
    let findings = "{\"findings\":[]}\n";
    std::fs::write(handoff.join("findings.json"), findings.as_bytes()).expect("place findings");
    let packet = "{\"review_id\":\"ut-bridge\",\"limits\":{\"timeout_s\":600}}";
    let (code, stdout, stderr) = run_bridge(&handoff, "30", packet);
    assert_eq!(code, Some(0), "bridge failed: {stderr}");
    // Findings are relayed verbatim; nothing else is printed.
    assert_eq!(stdout, findings);
    // The packet is written to the handoff verbatim.
    let written = std::fs::read_to_string(handoff.join("packet.json")).expect("packet.json");
    assert_eq!(written, packet);
}

#[test]
fn design_review_bridge_waits_for_a_growing_findings_file() {
    let handoff = temp_dir("grow");
    std::fs::write(handoff.join("findings.json"), b"{\"findings\": [").expect("partial");
    let findings = "{\"findings\":[{\"contract_or_path\":\"p\",\"trigger\":\"t\",\"consequence\":\"c\",\"evidence\":\"e\",\"smallest_correction\":\"s\"}]}\n";
    let path = handoff.join("findings.json");
    let writer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(700));
        std::fs::write(path, findings.as_bytes()).expect("complete findings");
    });
    let (code, stdout, stderr) = run_bridge(&handoff, "30", "{\"review_id\":\"ut-grow\"}");
    writer.join().expect("writer");
    assert_eq!(code, Some(0), "bridge failed: {stderr}");
    let payload: Value =
        serde_json::from_str(&stdout).expect("complete findings relayed, not the partial write");
    assert_eq!(payload["findings"].as_array().expect("findings").len(), 1);
}

#[test]
fn design_review_bridge_relays_unparsable_response_at_the_deadline() {
    let handoff = temp_dir("unparsable");
    std::fs::write(handoff.join("findings.json"), b"not json").expect("place invalid response");
    let (code, stdout, stderr) = run_bridge(&handoff, "0.6", "{\"review_id\":\"ut-raw\"}");
    assert_eq!(code, Some(0), "bridge failed: {stderr}");
    assert_eq!(
        stdout, "not json",
        "relayed verbatim so the adapter classifies it"
    );
}

#[test]
fn design_review_bridge_unavailable_is_exit_nine() {
    let handoff = temp_dir("none");
    let (code, _stdout, stderr) = run_bridge(&handoff, "0.5", "{\"review_id\":\"ut-none\"}");
    assert_eq!(code, Some(9));
    assert!(stderr.contains("capability unavailable"));
}

#[test]
fn design_review_bridge_rejects_a_non_object_packet() {
    let handoff = temp_dir("bad");
    let (code, _stdout, stderr) = run_bridge(&handoff, "30", "[]");
    assert_eq!(code, Some(2));
    assert!(stderr.contains("packet must be a JSON object"));
    // The handoff directory is created but no packet file is written.
    assert!(!handoff.join("packet.json").exists());
}

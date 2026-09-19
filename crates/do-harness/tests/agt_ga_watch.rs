//! The AGT GA watch must query the live crate name.
//!
//! `scripts/check-agt-ga.sh` gates the `chore-agt-promotion` decision in
//! `plans/agt-governance-epic.md`. The Rust SDK was renamed
//! `agent-governance` -> `agentmesh` upstream, and the retired name is frozen
//! at 3.2.2, so the predecessor of this script watched a crate that can never
//! move: every run reported `NOT_GA` and the promotion gate could never fire,
//! which is indistinguishable from a genuine hold in the CI log.
//!
//! These fixtures are hermetic — no network, no registry, no clock — so the
//! verdict logic is pinned rather than observed from a live service that is
//! expected to change under us.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::process::{Command, Output};

fn script() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/check-agt-ga.sh")
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/agt-ga")
        .join(name)
}

/// Runs the watch in fixture mode, optionally adding the retired-crate input.
fn run(crate_json: &str, release_json: &str, legacy: Option<&str>) -> Output {
    let mut cmd = Command::new("bash");
    cmd.arg(script())
        .arg("--crate-json")
        .arg(fixture(crate_json))
        .arg("--release-json")
        .arg(fixture(release_json));
    if let Some(legacy) = legacy {
        cmd.arg("--legacy-crate-json").arg(fixture(legacy));
    }
    cmd.output().expect("spawn check-agt-ga.sh")
}

fn verdict(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("VERDICT="))
        .unwrap_or_default()
        .to_owned()
}

/// The regression: the watched name must be the one upstream publishes.
///
/// A watch pointed at the retired `agent-governance` name reports `NOT_GA`
/// forever. Pin the live crate name in the non-fixture invocation, so a future
/// edit that reintroduces the dead URL fails here instead of silently
/// disabling the promotion gate.
#[test]
fn watch_source_is_the_live_crate_path() {
    // `--help` prints the header comment; the URLs live in the body, so read
    // the script itself.
    let source = std::fs::read_to_string(script()).expect("read check-agt-ga.sh");

    assert!(
        source.contains("crates.io/api/v1/crates/$CRATE"),
        "the crate URL must be built from the CRATE_NAME variable"
    );
    assert!(
        source.contains(r#"CRATE="agentmesh""#),
        "the watched crate must be `agentmesh`, the live Rust SDK name"
    );
    // The retired name may only appear as evidence, never as the gate.
    for line in source.lines() {
        if line.contains("CRATE_URL=") {
            assert!(
                !line.contains("agent-governance"),
                "the gating crate URL must not point at the retired name: {line}"
            );
        }
    }
}

/// The watch prints which sources it queried, so blind watching is visible.
#[test]
fn watch_reports_its_sources() {
    let out = run(
        "crate-preview.json",
        "release-no-ga.json",
        Some("crate-legacy.json"),
    );
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(out.status.success(), "{stdout}");
    assert!(stdout.contains("source: crate=https://crates.io/api/v1/crates/agentmesh"));
    assert!(stdout.contains("source: legacy=https://crates.io/api/v1/crates/agent-governance"));
    assert!(stdout.contains("source: release="));
}

/// A preview self-description keeps the gate closed at any version.
#[test]
fn preview_description_is_not_ga_even_when_the_release_is_loud() {
    // Release notes that do declare GA must still not promote a preview SDK.
    let out = run(
        "crate-preview.json",
        "release-ga.json",
        Some("crate-legacy.json"),
    );
    assert_eq!(verdict(&out), "NOT_GA");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("crate: name=agentmesh version=4.0.0"));
    // The retired crate is reported as evidence, and is not the gate.
    assert!(stdout.contains("legacy: name=agent-governance version=3.2.2"));
}

/// GA requires both signals: preview banner dropped and release notes say GA.
#[test]
fn ga_requires_both_the_banner_drop_and_the_release_declaration() {
    // Banner dropped but the release says nothing about GA.
    let out = run(
        "crate-ga.json",
        "release-no-ga.json",
        Some("crate-legacy.json"),
    );
    assert_eq!(
        verdict(&out),
        "NOT_GA",
        "a silent release must not be read as GA:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );

    // Both signals present.
    let out = run(
        "crate-ga.json",
        "release-ga.json",
        Some("crate-legacy.json"),
    );
    assert_eq!(
        verdict(&out),
        "GA",
        "dropped banner plus a GA declaration must promote:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// A prerelease never promotes, even with a GA-shaped description and body.
#[test]
fn prerelease_never_promotes() {
    let temp = tempfile::tempdir().unwrap();
    let release = temp.path().join("release.json");
    std::fs::write(
        &release,
        r#"{"tag_name":"v9.0.0-rc.1","name":"v9.0.0-rc.1","prerelease":true,
            "body":"generally available"}"#,
    )
    .unwrap();

    let out = Command::new("bash")
        .arg(script())
        .arg("--crate-json")
        .arg(fixture("crate-ga.json"))
        .arg("--release-json")
        .arg(&release)
        .output()
        .expect("spawn check-agt-ga.sh");
    assert_eq!(verdict(&out), "NOT_GA");
}

/// The retired crate is supplementary: omitting it must not change the verdict.
#[test]
fn the_retired_crate_cannot_swing_the_verdict() {
    let with_legacy = run(
        "crate-ga.json",
        "release-no-ga.json",
        Some("crate-legacy.json"),
    );
    let without_legacy = run("crate-ga.json", "release-no-ga.json", None);

    assert_eq!(verdict(&with_legacy), verdict(&without_legacy));
    let stdout = String::from_utf8_lossy(&without_legacy.stdout);
    assert!(
        !stdout.contains("legacy:"),
        "no legacy input means no legacy line:\n{stdout}"
    );
}

/// `--help` prints the header comment and never a line of code.
///
/// The header documents the crate rename and the fixture interface, so it grows
/// over time. A hard-coded line range in the predecessor leaked `set -euo
/// pipefail` into the help output the moment the header got longer; the range
/// must therefore be derived, not counted.
#[test]
fn help_prints_the_header_and_never_code() {
    let out = Command::new("bash")
        .arg(script())
        .arg("--help")
        .output()
        .expect("spawn check-agt-ga.sh --help");

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);

    // Documents the rename, so an operator learns which crate is watched.
    assert!(stdout.contains("agentmesh"), "{stdout}");
    assert!(stdout.contains("--legacy-crate-json"), "{stdout}");
    // Every printed line is a comment; no code may leak.
    for line in stdout.lines() {
        assert!(
            line.is_empty() || line.starts_with('#'),
            "help leaked a non-comment line: {line:?}"
        );
    }
}

/// Fixture mode requires both gating inputs; a partial invocation is a usage error.
#[test]
fn fixture_mode_rejects_a_missing_release_input() {
    let out = Command::new("bash")
        .arg(script())
        .arg("--crate-json")
        .arg(fixture("crate-preview.json"))
        .output()
        .expect("spawn check-agt-ga.sh");
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("must be passed together"));
}

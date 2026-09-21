//! Integration tests for the Rust binary provenance sensor contract.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use tempfile::{TempDir, tempdir};

/// Stands in for a cargo-auditable binary: the probe reads the link-time
/// section *name*, which section tables store verbatim in the file, so a
/// fixture only has to carry the marker to exercise the presence branch.
const AUDITABLE: &[u8] = b"\x7fELF\x02\x01\x01\x00mock\n.dep-v0\nmock binary body";
const PLAIN: &[u8] = b"compiled without cargo auditable; no embedded dependency data";

struct Sandbox {
    _dir: TempDir,
    root: PathBuf,
}

/// Writes the sensor, the artifacts, and a one-sensor `do-harness.toml`.
fn sandbox(artifacts: &[(&str, &[u8])], args: &[&str]) -> Sandbox {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    for (name, bytes) in artifacts {
        fs::write(root.join(name), bytes).unwrap();
    }
    let script = root.join("check-rust-binary-provenance.sh");
    fs::write(
        &script,
        include_str!("../../../scripts/check-rust-binary-provenance.sh"),
    )
    .unwrap();
    make_executable(&script);
    let argv_list = args
        .iter()
        .map(|arg| format!("\"{arg}\""))
        .collect::<Vec<_>>()
        .join(", ");
    fs::write(
        root.join("do-harness.toml"),
        format!(
            "language = \"rust\"\n\n[[sensors]]\nname = \"rust-binary-provenance\"\nargv = [\"bash\", \"check-rust-binary-provenance.sh\", {argv_list}]\n"
        ),
    )
    .unwrap();
    Sandbox { _dir: dir, root }
}

fn make_executable(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }
}

/// Mock cargo-audit. It accepts only the invocation cargo itself performs
/// (`<cargo-audit> audit bin …`), so a regression to `cargo-audit bin …` fails
/// every pass-path test instead of hiding behind the fixture.
fn mock_audit(sandbox: &Sandbox, behavior: &str) -> PathBuf {
    let path = sandbox.root.join("mock-cargo-audit.sh");
    fs::write(
        &path,
        format!(
            r#"#!/usr/bin/env bash
if [[ "${{1:-}}" == "--version" ]]; then
    echo "cargo-audit 0.22.2"
    exit 0
fi
if [[ "${{1:-}}" != "audit" || "${{2:-}}" != "bin" ]]; then
    echo "unexpected argv: $*" >&2
    exit 90
fi
{behavior}
"#
        ),
    )
    .unwrap();
    make_executable(&path);
    path
}

/// Runs the harness in the sandbox and returns the process output plus the
/// recorded coverage record for the sensor.
fn verify(sandbox: &Sandbox, strict: bool, envs: &[(&str, &str)]) -> (Output, serde_json::Value) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    cmd.arg("--root").arg(&sandbox.root).arg("verify");
    if strict {
        cmd.arg("--strict");
    } else {
        // Evidence is written for `--evidence` or `--strict`, and the skip path
        // is non-strict, so ask for the artifact explicitly.
        cmd.arg("--evidence")
            .arg(sandbox.root.join(".do-harness/evidence.json"));
    }
    for (key, value) in envs {
        cmd.env(key, value);
    }
    let output = cmd.output().unwrap();
    let evidence = fs::read_to_string(sandbox.root.join(".do-harness/evidence.json"))
        .expect("verify should record evidence");
    let doc: serde_json::Value = serde_json::from_str(&evidence).unwrap();
    (output, doc["coverage"]["rust-binary-provenance"].clone())
}

fn tool_env(mock: &std::path::Path) -> [(&str, &str); 1] {
    [("CARGO_AUDIT_BIN", mock.to_str().unwrap())]
}

#[test]
fn test_rust_binary_provenance_pass() {
    let sandbox = sandbox(&[("app.bin", AUDITABLE)], &["--artifact", "app.bin"]);
    let mock = mock_audit(&sandbox, "echo '0 vulnerabilities found'\nexit 0");

    let (output, coverage) = verify(&sandbox, true, &tool_env(&mock));

    assert!(
        output.status.success(),
        "auditable artifact without advisories should pass; stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(coverage["status"], "pass");
    assert_eq!(coverage["auditable_metadata"], true);
    assert_eq!(coverage["findings"], 0);
    assert_eq!(coverage["tool"], "cargo-audit 0.22.2");
    assert!(coverage["reason"].is_null());
    assert_eq!(coverage["artifact"], "app.bin");
    assert_eq!(coverage["artifacts"][0]["path"], "app.bin");
    assert_eq!(
        coverage["digest"].as_str().unwrap().len(),
        64,
        "evidence should carry the artifact sha256"
    );
}

#[test]
fn test_rust_binary_provenance_reports_every_artifact() {
    let sandbox = sandbox(
        &[
            ("first.bin", AUDITABLE),
            ("second.bin", b".dep-v0 other body"),
        ],
        &["--artifact", "first.bin second.bin"],
    );
    let mock = mock_audit(&sandbox, "echo '0 vulnerabilities found'\nexit 0");

    let (output, coverage) = verify(&sandbox, true, &tool_env(&mock));

    assert!(output.status.success());
    let artifacts = coverage["artifacts"].as_array().unwrap();
    assert_eq!(artifacts.len(), 2, "both artifacts must be reported");
    assert_eq!(artifacts[0]["path"], "first.bin");
    assert_eq!(artifacts[1]["path"], "second.bin");
    assert_eq!(artifacts[0]["auditable_metadata"], true);
    assert_eq!(artifacts[1]["auditable_metadata"], true);
    assert_ne!(
        artifacts[0]["digest"], artifacts[1]["digest"],
        "each artifact keeps its own digest"
    );
}

#[test]
fn test_rust_binary_provenance_vulnerability_finding() {
    let sandbox = sandbox(&[("app.bin", AUDITABLE)], &["--artifact", "app.bin"]);
    let mock = mock_audit(
        &sandbox,
        "echo 'Crate: bad-crate'\necho 'Version: 1.0.0'\necho 'Advisory: RUSTSEC-2024-0001'\nexit 1",
    );

    let (output, coverage) = verify(&sandbox, true, &tool_env(&mock));

    assert!(!output.status.success(), "an advisory must fail the gate");
    assert_eq!(coverage["status"], "fail");
    assert_eq!(coverage["findings"], 1);
    assert!(
        coverage["reason"]
            .as_str()
            .unwrap()
            .contains("security vulnerabilities"),
        "reason should name the finding class, got: {}",
        coverage["reason"]
    );
}

#[test]
fn test_rust_binary_provenance_distinguishes_tool_failure() {
    let sandbox = sandbox(&[("app.bin", AUDITABLE)], &["--artifact", "app.bin"]);
    let mock = mock_audit(
        &sandbox,
        "echo 'error: failed to fetch advisory database' >&2\nexit 2",
    );

    let (output, coverage) = verify(&sandbox, true, &tool_env(&mock));

    assert!(
        !output.status.success(),
        "a tool failure must fail the gate"
    );
    assert_eq!(coverage["status"], "fail");
    assert_eq!(
        coverage["findings"], 0,
        "a tool failure is not a vulnerability finding"
    );
    assert!(
        coverage["reason"]
            .as_str()
            .unwrap()
            .contains("cargo audit bin failed"),
        "reason should name the tool failure, got: {}",
        coverage["reason"]
    );
}

#[test]
fn test_rust_binary_provenance_missing_metadata() {
    let sandbox = sandbox(&[("plain.bin", PLAIN)], &["--artifact", "plain.bin"]);
    let mock = mock_audit(&sandbox, "echo '0 vulnerabilities found'\nexit 0");

    let (output, coverage) = verify(&sandbox, true, &tool_env(&mock));

    assert!(
        !output.status.success(),
        "a binary without auditable metadata must fail closed"
    );
    assert_eq!(coverage["status"], "fail");
    assert_eq!(coverage["auditable_metadata"], false);
    assert!(
        coverage["reason"]
            .as_str()
            .unwrap()
            .contains("auditable dependency metadata missing"),
        "reason should name the missing metadata, got: {}",
        coverage["reason"]
    );
}

#[test]
fn test_rust_binary_provenance_missing_binary() {
    let sandbox = sandbox(&[], &["--artifact", "nonexistent.bin"]);
    let mock = mock_audit(&sandbox, "echo '0 vulnerabilities found'\nexit 0");

    let (output, coverage) = verify(&sandbox, true, &tool_env(&mock));

    assert!(!output.status.success());
    assert_eq!(coverage["status"], "fail");
    assert_eq!(coverage["artifact"], "nonexistent.bin");
    assert!(
        coverage["reason"]
            .as_str()
            .unwrap()
            .contains("artifact file not found")
    );
}

#[test]
fn test_rust_binary_provenance_strict_missing_tool() {
    let sandbox = sandbox(
        &[("app.bin", AUDITABLE)],
        &["--artifact", "app.bin", "--strict"],
    );

    let (output, coverage) = verify(
        &sandbox,
        true,
        &[("CARGO_AUDIT_BIN", "/nonexistent/path/to/cargo-audit")],
    );

    assert!(
        !output.status.success(),
        "strict verification must fail closed when cargo-audit is unavailable"
    );
    assert_eq!(coverage["status"], "fail");
    assert!(
        coverage["reason"].as_str().unwrap().contains("cargo-audit"),
        "reason should name the missing tool, got: {}",
        coverage["reason"]
    );
}

#[test]
fn test_rust_binary_provenance_skips_without_strict() {
    let sandbox = sandbox(&[("app.bin", AUDITABLE)], &["--artifact", "app.bin"]);

    let (output, coverage) = verify(
        &sandbox,
        false,
        &[
            ("CARGO_AUDIT_BIN", "/nonexistent/path/to/cargo-audit"),
            ("CI", ""),
            ("DO_HARNESS_REQUIRE_TOOLS", ""),
        ],
    );

    assert!(
        output.status.success(),
        "an unavailable tool is a skip, not a failure, outside strict verification"
    );
    assert_eq!(
        coverage["status"], "warn",
        "a skip is recorded as a non-pass, never as a green"
    );
    assert!(
        coverage["reason"].as_str().unwrap().contains("cargo-audit"),
        "reason should name the missing tool, got: {}",
        coverage["reason"]
    );
}

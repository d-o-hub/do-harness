//! Integration tests for the generic artifact-provenance sensor contract.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_artifact_provenance_pass() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let artifact_path = root.join("artifact.bin");
    fs::write(&artifact_path, b"sample binary artifact content").unwrap();

    let script_path = root.join("check-provenance.sh");
    let script_content = include_str!("../../../scripts/check-artifact-provenance.sh");
    fs::write(&script_path, script_content).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).unwrap();
    }

    let config_path = root.join("do-harness.toml");
    let config_content = r#"
language = "generic"

[[sensors]]
name = "artifact-provenance"
argv = ["bash", "check-provenance.sh", "--artifact", "artifact.bin", "--mode", "digest-only"]
"#;
    fs::write(&config_path, config_content).unwrap();

    let bin_path = env!("CARGO_BIN_EXE_do-harness");
    let output = Command::new(bin_path)
        .arg("--root")
        .arg(root)
        .arg("verify")
        .arg("--strict")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "verify --strict should succeed on valid artifact provenance; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let evidence_path = root.join(".do-harness/evidence.json");
    assert!(evidence_path.exists());
    let evidence_text = fs::read_to_string(evidence_path).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&evidence_text).unwrap();

    let coverage = &doc["coverage"]["artifact-provenance"];
    assert_eq!(coverage["artifact"], "artifact.bin");
    assert_eq!(coverage["status"], "pass");
    assert!(coverage["reason"].is_null());
}

#[test]
fn test_artifact_provenance_fail_mismatched_digest() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let artifact_path = root.join("artifact.bin");
    fs::write(&artifact_path, b"sample binary artifact content").unwrap();

    let script_path = root.join("check-provenance.sh");
    let script_content = include_str!("../../../scripts/check-artifact-provenance.sh");
    fs::write(&script_path, script_content).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).unwrap();
    }

    let config_path = root.join("do-harness.toml");
    let config_content = r#"
language = "generic"

[[sensors]]
name = "artifact-provenance"
argv = ["bash", "check-provenance.sh", "--artifact", "artifact.bin", "--digest", "0000000000000000000000000000000000000000000000000000000000000000"]
"#;
    fs::write(&config_path, config_content).unwrap();

    let bin_path = env!("CARGO_BIN_EXE_do-harness");
    let output = Command::new(bin_path)
        .arg("--root")
        .arg(root)
        .arg("verify")
        .arg("--strict")
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "verify --strict should fail on digest mismatch"
    );

    let evidence_path = root.join(".do-harness/evidence.json");
    assert!(evidence_path.exists());
    let evidence_text = fs::read_to_string(evidence_path).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&evidence_text).unwrap();

    let coverage = &doc["coverage"]["artifact-provenance"];
    assert_eq!(coverage["artifact"], "artifact.bin");
    assert_eq!(coverage["status"], "fail");
    assert!(
        coverage["reason"]
            .as_str()
            .unwrap()
            .contains("digest mismatch")
    );
}

#[test]
fn test_artifact_provenance_missing_artifact() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let script_path = root.join("check-provenance.sh");
    let script_content = include_str!("../../../scripts/check-artifact-provenance.sh");
    fs::write(&script_path, script_content).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).unwrap();
    }

    let config_path = root.join("do-harness.toml");
    let config_content = r#"
language = "generic"

[[sensors]]
name = "artifact-provenance"
argv = ["bash", "check-provenance.sh", "--artifact", "missing.bin"]
"#;
    fs::write(&config_path, config_content).unwrap();

    let bin_path = env!("CARGO_BIN_EXE_do-harness");
    let output = Command::new(bin_path)
        .arg("--root")
        .arg(root)
        .arg("verify")
        .arg("--strict")
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "verify --strict should fail on missing artifact"
    );

    let evidence_path = root.join(".do-harness/evidence.json");
    assert!(evidence_path.exists());
    let evidence_text = fs::read_to_string(evidence_path).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&evidence_text).unwrap();

    let coverage = &doc["coverage"]["artifact-provenance"];
    assert_eq!(coverage["artifact"], "missing.bin");
    assert_eq!(coverage["status"], "fail");
    assert!(
        coverage["reason"]
            .as_str()
            .unwrap()
            .contains("artifact file not found")
    );
}

#[test]
fn test_artifact_provenance_reserved_mode_fails_closed() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let artifact_path = root.join("artifact.bin");
    fs::write(&artifact_path, b"sample binary artifact content").unwrap();

    let script_path = root.join("check-provenance.sh");
    let script_content = include_str!("../../../scripts/check-artifact-provenance.sh");
    fs::write(&script_path, script_content).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).unwrap();
    }

    let config_path = root.join("do-harness.toml");
    let config_content = r#"
language = "generic"

[[sensors]]
name = "artifact-provenance"
argv = ["bash", "check-provenance.sh", "--artifact", "artifact.bin", "--mode", "sbom"]
"#;
    fs::write(&config_path, config_content).unwrap();

    let bin_path = env!("CARGO_BIN_EXE_do-harness");
    let output = Command::new(bin_path)
        .arg("--root")
        .arg(root)
        .arg("verify")
        .arg("--strict")
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "a reserved mode must not pass on a digest check it never performed; stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );

    let evidence_path = root.join(".do-harness/evidence.json");
    assert!(evidence_path.exists());
    let evidence_text = fs::read_to_string(&evidence_path).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&evidence_text).unwrap();

    let coverage = &doc["coverage"]["artifact-provenance"];
    assert_eq!(coverage["artifact"], "artifact.bin");
    assert_eq!(coverage["status"], "fail");
    assert!(
        coverage["reason"]
            .as_str()
            .unwrap()
            .contains("reserved and not implemented"),
        "reason should name the reserved mode, got: {}",
        coverage["reason"]
    );
}

#[test]
fn test_artifact_provenance_malformed_evidence() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let config_path = root.join("do-harness.toml");
    let config_content = r#"
language = "generic"

[[sensors]]
name = "artifact-provenance"
argv = ["sh", "-c", "echo 'COVERAGE: {not_valid_json}'; exit 1"]
"#;
    fs::write(&config_path, config_content).unwrap();

    let bin_path = env!("CARGO_BIN_EXE_do-harness");
    let output = Command::new(bin_path)
        .arg("--root")
        .arg(root)
        .arg("verify")
        .arg("--strict")
        .output()
        .unwrap();
    assert!(!output.status.success());

    let evidence_path = root.join(".do-harness/evidence.json");
    if evidence_path.exists() {
        let evidence_text = fs::read_to_string(evidence_path).unwrap();
        let doc: serde_json::Value = serde_json::from_str(&evidence_text).unwrap();
        assert!(
            doc["coverage"].get("artifact-provenance").is_none(),
            "malformed COVERAGE output should not be recorded as valid coverage JSON"
        );
    }
}

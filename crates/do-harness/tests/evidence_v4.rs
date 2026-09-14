//! End-to-end evidence v4: declared artifact digests and coverage manifests.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

/// Builds a `do-harness --root <root>` command using the real binary.
fn harness(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    cmd.arg("--root").arg(root);
    cmd
}

/// Runs a harness command, returning (exit code, stdout, stderr).
fn run(cmd: &mut Command) -> (Option<i32>, String, String) {
    let output = cmd.output().expect("spawn do-harness");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Reads the per-set evidence artifact as JSON.
fn evidence(root: &Path) -> Value {
    let path = root.join(".do-harness/evidence.verification.json");
    serde_json::from_slice(
        &std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())),
    )
    .expect("evidence json")
}

/// A sensor that writes an artifact and reports a coverage manifest lands both
/// in the v4 evidence artifact.
#[test]
fn artifacts_and_coverage_land_in_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("emit.sh"),
        "mkdir -p out\nprintf '{\"ok\":true}' > out/report.json\n\
         echo 'COVERAGE: {\"routes\":2,\"viewports\":3}'\n",
    )
    .unwrap();
    std::fs::write(
        root.join("do-harness.toml"),
        "[signal-sets]\nverification = [\"web\"]\n\n[[sensors]]\nname = \"web\"\n\
         argv = [\"bash\", \"emit.sh\"]\nartifacts = [\"out/*.json\"]\n\
         coverage-inputs = [\"matrix.mjs\"]\n",
    )
    .unwrap();
    std::fs::write(root.join("matrix.mjs"), "export const m = [1];\n").unwrap();

    let (code, stdout, stderr) =
        run(harness(root).args(["verify", "--set", "verification", "--format", "json"]));
    assert_eq!(code, Some(0), "verify failed:\n{stdout}\n{stderr}");

    let doc = evidence(root);
    assert_eq!(doc["schema_version"], serde_json::json!(4));
    assert_eq!(
        doc["sensors"][0]["artifacts"][0]["path"],
        serde_json::json!("out/report.json")
    );
    let digest = doc["sensors"][0]["artifacts"][0]["sha256"]
        .as_str()
        .expect("artifact digest");
    assert_eq!(digest.len(), 64, "sha256 hex digest: {digest}");
    assert_eq!(doc["coverage"]["web"]["routes"], serde_json::json!(2));
    assert_eq!(doc["coverage"]["web"]["viewports"], serde_json::json!(3));
}

/// A declared artifact glob that matches nothing degrades the evidence and
/// fails `--strict` even though the sensor itself exits zero.
#[test]
fn missing_declared_artifact_fails_strict() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("do-harness.toml"),
        "[signal-sets]\nverification = [\"web\"]\n\n[[sensors]]\nname = \"web\"\n\
         argv = [\"true\"]\nartifacts = [\"out/*.png\"]\n",
    )
    .unwrap();

    let (code, stdout, stderr) = run(harness(root).args([
        "verify",
        "--set",
        "verification",
        "--strict",
        "--format",
        "json",
    ]));
    assert_eq!(
        code,
        Some(1),
        "strict must reject weak evidence:\n{stdout}\n{stderr}"
    );

    let doc = evidence(root);
    assert_eq!(doc["sensors"][0]["verdict"], serde_json::json!("warn"));
    assert_eq!(doc["summary"]["verdict"], serde_json::json!("fail"));
}

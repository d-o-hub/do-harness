//! Synthetic integration observations, never a claim about an actual coding client.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::process::Command;

const SCENARIO_COUNT: usize = 4;

fn script() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates/scripts/check-client-conformance.py")
}

fn run(adapter: &str, required: bool) -> (bool, serde_json::Value, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("adapter.py");
    std::fs::write(&path, adapter).unwrap();
    let mut command = Command::new("python3");
    command.arg(script());
    if required {
        command.arg("--required");
    }
    command.args(["--client", "fake-client", "--client-version", "1"]);
    command.args([
        "--integration",
        "local-test-harness",
        "--adapter",
        "python3",
    ]);
    let output = command.arg(path).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let report = serde_json::from_str(stdout.lines().next().unwrap()).unwrap();
    (output.status.success(), report, stdout)
}

fn adapter(supported: bool, instrumented: bool, refused: bool, outbound: &str) -> String {
    format!(
        "import json,sys\nr=json.load(sys.stdin)\nprint(json.dumps({{\
         'protocol_version':1,'supported':{},'instrumented':{},'refused':{},'outbound':{outbound}}}))\n",
        if supported { "True" } else { "False" },
        if instrumented { "True" } else { "False" },
        if refused { "True" } else { "False" },
    )
}

#[test]
fn transformed_and_refused_boundary_observations_pass() {
    for (refused, outbound) in [(false, "'synthetic context [PRIVATE]'"), (true, "''")] {
        let (ok, report, _) = run(&adapter(true, true, refused, outbound), true);
        assert!(ok, "{report}");
        assert_eq!(report["claim"], "client-boundary");
        assert_eq!(report["claims"]["sanitizer_fixture"], "unknown");
        assert_eq!(report["claims"]["guidance_installed"], "unknown");
        assert_eq!(report["claims"]["client_boundary"], "pass");
        assert_eq!(
            report["scenarios"].as_array().unwrap().len(),
            SCENARIO_COUNT
        );
    }
}

#[test]
fn local_fake_client_instruments_all_synthetic_routes_without_network() {
    let adapter = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/client-conformance/adapter.py");
    let (ok, report, _) = run(&std::fs::read_to_string(adapter).unwrap(), true);
    assert!(ok, "{report}");
    assert_eq!(report["verdict"], "pass");
}

#[test]
fn unknown_clients_and_uninstrumented_boundaries_fail_required_policy() {
    for (supported, instrumented) in [(false, true), (true, false)] {
        let code = adapter(supported, instrumented, false, "''");
        let (ok, report, stdout) = run(&code, false);
        assert!(ok);
        assert_eq!(report["verdict"], "unknown");
        assert!(stdout.contains("SKIP:"));
        assert!(!run(&code, true).0);
    }
    let output = Command::new("python3")
        .arg(script())
        .arg("--required")
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[test]
fn leakage_and_malformed_observations_are_safe_failures() {
    for code in [
        adapter(true, true, false, "r['private_markers'][0]"),
        "import json,sys\nr=json.load(sys.stdin)\nprint(r['private_markers'][0])".into(),
        "print('{\"claim\":\"sanitizer-fixture\",\"verdict\":\"pass\"}')".into(),
        adapter(true, true, true, "'unexpected outbound data'"),
    ] {
        let (ok, report, stdout) = run(&code, true);
        assert!(!ok);
        assert_eq!(report["verdict"], "fail");
        assert!(
            !stdout.contains("private_"),
            "raw markers must never be logged"
        );
        assert!(!stdout.contains("unexpected outbound data"));
    }
}

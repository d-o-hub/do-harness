//! Unit tests for the opt-in client capability profile.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::needless_pass_by_value
)]

use std::fs;
use std::path::{Path, PathBuf};

use super::profile::CLAIM_IDS;
use super::*;

const NOW: u64 = 1_700_000_000;
const ADAPTER_SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn profile(claims: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "client": { "id": "acme-client", "version": "1.4.0" },
        "configuration": "default-config",
        "adapter": { "argv": ["python3", "adapter.py"], "sha256": ADAPTER_SHA },
        "observations": "observations.json",
        "claims": claims,
    })
}

fn observation(claims: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "client": { "id": "acme-client", "version": "1.4.0" },
        "adapter": { "sha256": ADAPTER_SHA },
        "configuration": "default-config",
        "generated_at_unix_seconds": NOW,
        "max_age_seconds": 3600,
        "claims": claims,
    })
}

fn full_claims() -> serde_json::Value {
    serde_json::json!({
        "guidance_installed": { "required": true },
        "host_boundary": { "required": true, "route": "shell" },
        "mcp_mediation": { "required": true, "route": "mcp-tool-call" },
        "governance_enforced": { "required": true, "route": "mcp-tool-call" }
    })
}

fn full_observation(mode: &str) -> serde_json::Value {
    let mut value = observation(serde_json::json!({
        "guidance_installed": { "route": "guidance", "status": "pass", "reason": "observed" },
        "host_boundary": { "route": "shell", "status": "pass", "reason": "observed" },
        "mcp_mediation": { "route": "mcp-tool-call", "status": "pass", "reason": "observed" },
        "governance_enforced": { "route": "mcp-tool-call", "status": "pass", "reason": "observed" }
    }));
    value["governance"] = serde_json::json!({ "mode": mode, "route": "mcp-tool-call" });
    value
}

fn setup(
    profile: serde_json::Value,
    observation: serde_json::Value,
) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("observations.json"),
        observation.to_string(),
    )
    .unwrap();
    let path = dir.path().join("profile.json");
    fs::write(&path, profile.to_string()).unwrap();
    (dir, path)
}

fn status(report: &ClientReport, claim: &str) -> String {
    report.to_json()["claims"][claim]["status"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn reason(report: &ClientReport, claim: &str) -> String {
    report.to_json()["claims"][claim]["reason"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test(flavor = "current_thread")]
async fn no_profile_leaves_default_diagnostics_untouched() {
    assert!(evaluate_optional(None, false).await.unwrap().is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn required_claims_pass_only_with_bound_fresh_evidence() {
    let (_dir, path) = setup(profile(full_claims()), full_observation("enforced"));
    let report = evaluate_at(&path, false, NOW).await.unwrap();
    assert_eq!(report.verdict(), "pass");
    assert_eq!(report.required_failures().len(), 0);
    assert_eq!(status(&report, "guidance_installed"), "pass");
    assert_eq!(reason(&report, "guidance_installed"), "guidance-only");
    assert_eq!(status(&report, "host_boundary"), "pass");
}

#[tokio::test(flavor = "current_thread")]
async fn stub_proxy_build_cannot_satisfy_a_governance_claim() {
    let (_dir, path) = setup(profile(full_claims()), full_observation("stub"));
    let report = evaluate_at(&path, false, NOW).await.unwrap();
    assert_eq!(status(&report, "governance_enforced"), "stub");
    assert_eq!(reason(&report, "governance_enforced"), "stub-build");
    assert_eq!(report.verdict(), "fail");
    assert_eq!(report.required_failures().len(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn degraded_proxy_and_unsupported_client_are_explicit() {
    let (_dir, path) = setup(profile(full_claims()), full_observation("degraded"));
    let report = evaluate_at(&path, false, NOW).await.unwrap();
    assert_eq!(status(&report, "governance_enforced"), "degraded");
    assert_eq!(reason(&report, "governance_enforced"), "degraded-proxy");

    let mut observation = full_observation("enforced");
    observation["claims"]["host_boundary"] = serde_json::json!({ "route": "shell", "status": "unknown", "reason": "unsupported-client" });
    let (_dir, path) = setup(profile(full_claims()), observation);
    let report = evaluate_at(&path, false, NOW).await.unwrap();
    assert_eq!(status(&report, "host_boundary"), "unsupported");
    assert_eq!(reason(&report, "host_boundary"), "unsupported-client");
}

#[tokio::test(flavor = "current_thread")]
async fn an_mcp_route_never_implies_shell_or_file_mediation() {
    let claims = serde_json::json!({
        "mcp_mediation": { "required": true, "route": "mcp-tool-call" },
        "host_boundary": { "required": true, "route": "shell" }
    });
    let observation = observation(serde_json::json!({
        "mcp_mediation": { "route": "mcp-tool-call", "status": "pass", "reason": "observed" }
    }));
    let (_dir, path) = setup(profile(claims), observation);
    let report = evaluate_at(&path, false, NOW).await.unwrap();
    assert_eq!(status(&report, "mcp_mediation"), "pass");
    assert_eq!(status(&report, "host_boundary"), "unknown");
    assert_eq!(reason(&report, "host_boundary"), "no-observation");
    assert_eq!(report.verdict(), "fail");
}

#[tokio::test(flavor = "current_thread")]
async fn installed_guidance_cannot_stand_in_for_enforcement() {
    let observation = observation(serde_json::json!({
        "host_boundary": { "route": "shell", "status": "pass", "reason": "guidance-only" }
    }));
    let claims = serde_json::json!({ "host_boundary": { "required": true, "route": "shell" } });
    let (_dir, path) = setup(profile(claims), observation);
    let report = evaluate_at(&path, false, NOW).await.unwrap();
    assert_eq!(status(&report, "host_boundary"), "invalidated");
    assert_eq!(reason(&report, "host_boundary"), "malformed-observation");
}

#[tokio::test(flavor = "current_thread")]
async fn changed_pins_invalidate_prior_observations() {
    let mutations: [fn(&mut serde_json::Value); 4] = [
        |value: &mut serde_json::Value| value["client"]["version"] = serde_json::json!("1.3.0"),
        |value: &mut serde_json::Value| {
            value["adapter"]["sha256"] = serde_json::json!("b".repeat(64));
        },
        |value: &mut serde_json::Value| value["configuration"] = serde_json::json!("other-config"),
        |value: &mut serde_json::Value| {
            value["claims"]["host_boundary"]["route"] = serde_json::json!("other-route");
        },
    ];
    for mutate in mutations {
        let mut observation = full_observation("enforced");
        mutate(&mut observation);
        let (_dir, path) = setup(profile(full_claims()), observation);
        let report = evaluate_at(&path, false, NOW).await.unwrap();
        assert!(
            !report.required_failures().is_empty(),
            "a changed pin must not keep required claims passing"
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn stale_and_future_observations_are_not_passes() {
    let mut stale = full_observation("enforced");
    stale["generated_at_unix_seconds"] = serde_json::json!(NOW - 7200);
    let (_dir, path) = setup(profile(full_claims()), stale);
    let report = evaluate_at(&path, false, NOW).await.unwrap();
    assert_eq!(status(&report, "mcp_mediation"), "stale");
    assert_eq!(reason(&report, "mcp_mediation"), "stale-observation");

    let mut future = full_observation("enforced");
    future["generated_at_unix_seconds"] = serde_json::json!(NOW + 3600);
    let (_dir, path) = setup(profile(full_claims()), future);
    let report = evaluate_at(&path, false, NOW).await.unwrap();
    assert_eq!(status(&report, "mcp_mediation"), "invalidated");
    assert_eq!(reason(&report, "mcp_mediation"), "malformed-observation");
}

#[tokio::test(flavor = "current_thread")]
async fn malformed_and_oversized_documents_are_unusable_evidence() {
    let claims = serde_json::json!({ "host_boundary": { "required": true, "route": "shell" } });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("profile.json");
    fs::write(&path, profile(claims.clone()).to_string()).unwrap();

    fs::write(dir.path().join("observations.json"), "{not json").unwrap();
    let report = evaluate_at(&path, false, NOW).await.unwrap();
    assert_eq!(status(&report, "host_boundary"), "invalidated");
    assert_eq!(reason(&report, "host_boundary"), "malformed-observation");

    let oversized = "x".repeat(300 * 1024);
    fs::write(dir.path().join("observations.json"), oversized).unwrap();
    let report = evaluate_at(&path, false, NOW).await.unwrap();
    assert_eq!(reason(&report, "host_boundary"), "oversized-observation");
}

#[test]
fn a_profile_cannot_declare_unknown_claims_or_omit_a_route() {
    let (_dir, path) = setup(
        profile(serde_json::json!({ "made_up": { "required": true } })),
        observation(serde_json::json!({})),
    );
    assert!(Profile::load(&path).is_err());

    let (_dir, path) = setup(
        profile(serde_json::json!({ "host_boundary": { "required": true } })),
        observation(serde_json::json!({})),
    );
    assert!(Profile::load(&path).is_err());
}

#[tokio::test(flavor = "current_thread")]
async fn conformance_report_supplies_only_the_privacy_claims() {
    let claims = serde_json::json!({
        "context_sanitization": { "required": true },
        "host_boundary": { "required": true, "route": "shell" }
    });
    let mut value = profile(claims);
    value["conformance_report"] = serde_json::json!("conformance.json");
    let (_dir, path) = setup(
        value,
        observation(serde_json::json!({
            "host_boundary": { "route": "shell", "status": "pass", "reason": "observed" }
        })),
    );
    let report_doc = serde_json::json!({
        "schema_version": 1,
        "claim": "client-boundary",
        "client": { "id": "acme-client", "version": "1.4.0" },
        "integration_method": "local-test-harness",
        "test_suite_version": "1",
        "timestamp": "2026-11-03T00:00:00Z",
        "claims": {
            "guidance_installed": "unknown",
            "sanitizer_fixture": "pass",
            "client_boundary": "unknown"
        },
        "scenarios": [{}, {}, {}, {}],
        "verdict": "pass",
        "limitations": ["Synthetic observations only."]
    });
    fs::write(
        path.parent().unwrap().join("conformance.json"),
        report_doc.to_string(),
    )
    .unwrap();

    let report = evaluate_at(&path, false, NOW).await.unwrap();
    assert_eq!(status(&report, "context_sanitization"), "pass");
    assert_eq!(status(&report, "host_boundary"), "pass");
    assert_eq!(report.verdict(), "pass");
}

#[cfg(unix)]
fn adapter_script(dir: &Path, body: &str) -> (PathBuf, String) {
    use sha2::{Digest, Sha256};
    use std::os::unix::fs::PermissionsExt;

    let path = dir.join("adapter.py");
    fs::write(&path, format!("#!/usr/bin/env python3\n{body}")).unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).unwrap();
    let digest = hex::encode(Sha256::digest(fs::read(&path).unwrap()));
    (path, digest)
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn an_explicit_probe_runs_the_pinned_adapter_only() {
    let dir = tempfile::tempdir().unwrap();
    let sentinel = dir.path().join("ran");
    let body = format!(
        "import json,sys,os\njson.load(sys.stdin)\nopen({:?},'w').write(os.getcwd())\n\
         print(json.dumps({{'protocol_version':1,'governance':{{'mode':'enforced','route':'mcp-tool-call'}},\
         'observations':{{'host_boundary':{{'status':'pass','reason':'observed'}},\
         'mcp_mediation':{{'status':'pass','reason':'observed'}},\
         'governance_enforced':{{'status':'pass','reason':'observed'}}}}}}))\n",
        sentinel.display().to_string()
    );
    let (script, digest) = adapter_script(dir.path(), &body);
    let claims = serde_json::json!({
        "host_boundary": { "required": true, "route": "shell" },
        "mcp_mediation": { "required": true, "route": "mcp-tool-call" },
        "governance_enforced": { "required": true, "route": "mcp-tool-call" }
    });
    let mut value = profile(claims.clone());
    value["adapter"] = serde_json::json!({ "argv": [script], "sha256": digest });
    value.as_object_mut().unwrap().remove("observations");
    let path = dir.path().join("profile.json");
    fs::write(&path, value.to_string()).unwrap();

    let report = evaluate_at(&path, false, NOW).await.unwrap();
    assert_eq!(status(&report, "host_boundary"), "unknown");
    assert!(!sentinel.exists(), "a probe must never run implicitly");

    let report = evaluate_at(&path, true, NOW).await.unwrap();
    assert_eq!(report.verdict(), "pass");
    let cwd = fs::read_to_string(&sentinel).unwrap();
    assert!(!cwd.is_empty(), "the probe runs in a disposable workspace");
    assert_ne!(cwd, dir.path().to_string_lossy());
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn dishonest_or_oversized_probe_output_is_not_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let body = "import json,sys\nr=json.load(sys.stdin)\n\
                print(json.dumps({'protocol_version':1,'observations':{}}),r['synthetic_marker'])\n";
    let (script, digest) = adapter_script(dir.path(), body);
    let claims = serde_json::json!({ "host_boundary": { "required": true, "route": "shell" } });
    let mut value = profile(claims);
    value["adapter"] = serde_json::json!({ "argv": [script.clone()], "sha256": digest });
    value.as_object_mut().unwrap().remove("observations");
    let path = dir.path().join("profile.json");
    fs::write(&path, value.to_string()).unwrap();

    let report = evaluate_at(&path, true, NOW).await.unwrap();
    assert_eq!(status(&report, "host_boundary"), "invalidated");
    assert_eq!(reason(&report, "host_boundary"), "malformed-observation");
}

#[tokio::test(flavor = "current_thread")]
async fn reports_never_echo_adapter_or_document_content() {
    let claims = serde_json::json!({ "host_boundary": { "required": true, "route": "shell" } });
    let mut value = observation(serde_json::json!({
        "host_boundary": { "route": "shell", "status": "fail", "reason": "observed" }
    }));
    value["claims"]["host_boundary"]["note"] = serde_json::json!("token=secret-value");
    let (_dir, path) = setup(profile(claims), value);
    let report = evaluate_at(&path, false, NOW).await.unwrap();
    let rendered = serde_json::to_string(&report.to_json()).unwrap();
    assert!(!rendered.contains("secret-value"));
    assert!(!rendered.contains("token="));
    // Free text is rejected by the document contract, not rendered.
    assert_eq!(status(&report, "host_boundary"), "invalidated");
    assert_eq!(reason(&report, "host_boundary"), "malformed-observation");
}

#[test]
fn every_claim_has_a_fixed_limitation() {
    for id in CLAIM_IDS {
        assert!(!limitation(id).is_empty(), "{id} needs a limitation");
    }
}

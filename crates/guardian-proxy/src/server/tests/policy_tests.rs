//! Server tests for the declarative policy layer: caller-header routing and
//! audit digest binding over HTTP.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

fn test_mediator_with_policy(upstream: &str, policy_yaml: &str) -> Arc<ProxyMediator> {
    let dir = tempfile::tempdir().expect("tempdir");
    let policy_path = dir.path().join("policy.yaml");
    std::fs::write(&policy_path, policy_yaml).expect("write policy");
    let cfg = ProxyConfig {
        bind: "127.0.0.1:0".to_string(),
        upstream: upstream.to_string(),
        agent_id: "test-agent".to_string(),
        audit_log: None,
        upstream_allowlist: vec![],
        allow_private_upstreams: true,
        metrics_token: None,
        ingress_token: None,
        policy_file: Some(policy_path.display().to_string()),
        allowed_origins: vec![],
    };
    Arc::new(ProxyMediator::new(cfg).expect("mediator"))
}

#[tokio::test(flavor = "current_thread")]
async fn test_caller_header_feeds_policy_rules() {
    let policy = r#"
version: 1
rules:
  - action: deny
    tool: "data.read"
    caller: "ci"
  - action: allow
    tool: "data.read"
"#;
    let mediator = test_mediator_with_policy("http://127.0.0.1:9", policy);
    let router = create_router(mediator);

    // The caller header matches the deny rule -> 403 before upstream contact.
    let req = Request::builder()
        .uri("/mcp/tools/call")
        .method("POST")
        .header("content-type", "application/json")
        .header("x-do-harness-caller-id", "ci")
        .body(AxumBody::from(r#"{"tool":"data.read","params":{}}"#))
        .expect("request");
    let resp = router.clone().oneshot(req).await.expect("response");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // A different caller does not match the deny rule; the allow reaches the
    // upstream, which is unreachable -> 502 (proves the call was forwarded).
    let req2 = Request::builder()
        .uri("/mcp/tools/call")
        .method("POST")
        .header("content-type", "application/json")
        .header("x-do-harness-caller-id", "alice")
        .body(AxumBody::from(r#"{"tool":"data.read","params":{}}"#))
        .expect("request");
    let resp2 = router.oneshot(req2).await.expect("response");
    assert_eq!(resp2.status(), StatusCode::BAD_GATEWAY);
}

#[tokio::test(flavor = "current_thread")]
async fn test_audit_records_policy_digest_over_http() {
    use crate::AuditLog;

    let policy = r#"
version: 1
rules:
  - action: allow
    tool: "data.read"
"#;
    let dir = tempfile::tempdir().expect("tempdir");
    let audit_path = dir.path().join("audit.jsonl");
    let mediator = test_mediator_with_policy("http://127.0.0.1:9", policy);
    let audit = AuditLog::open(&audit_path).expect("open audit");
    let router = create_router_with_audit(mediator, audit);

    let body = serde_json::to_string(&json!({"tool":"data.read"})).expect("json");
    let req = Request::builder()
        .uri("/mcp/tools/call")
        .method("POST")
        .header("content-type", "application/json")
        .body(AxumBody::from(body))
        .expect("request");
    let resp = router.oneshot(req).await.expect("response");
    assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);

    AuditLog::verify(&audit_path).expect("verify chain");
    let content = std::fs::read_to_string(&audit_path).expect("read audit");
    let record: serde_json::Value =
        serde_json::from_str(content.lines().next().expect("line")).expect("parse record");
    let digest = record["policy_sha256"].as_str().expect("digest present");
    assert_eq!(digest.len(), 64);
}

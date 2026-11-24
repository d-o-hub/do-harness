//! Unit tests for the ssrf guard and mediator (`proxy.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use serde_json::json;

fn test_config() -> ProxyConfig {
    ProxyConfig {
        bind: "127.0.0.1:0".to_string(),
        upstream: "http://127.0.0.1:9000".to_string(),
        agent_id: "test-agent".to_string(),
        audit_log: None,
        upstream_allowlist: vec![],
        allow_private_upstreams: true,
        metrics_token: None,
        ingress_token: None,
        policy_file: None,
        allowed_origins: vec![],
    }
}

#[test]
fn test_config_deny_unknown_fields() {
    let bad = r#"{"bind":"a","upstream":"b","agent_id":"c","extra":1}"#;
    let err = serde_json::from_str::<ProxyConfig>(bad).unwrap_err();
    assert!(err.to_string().contains("unknown field"));
}

#[test]
fn test_ssrf_denies_metadata_upstream_always() {
    let mut cfg = test_config();
    cfg.upstream = "http://169.254.169.254/latest/meta-data".into();
    cfg.allow_private_upstreams = true;
    let err = validate_upstream(&cfg).unwrap_err().to_string();
    assert!(err.contains("link-local"), "{err}");
}

#[test]
fn test_ssrf_denies_ipv6_link_local_upstream_always() {
    let mut cfg = test_config();
    cfg.upstream = "http://[fe80::1]:8080".into();
    cfg.allow_private_upstreams = false;
    let err = match ProxyMediator::new(cfg) {
        Ok(_) => panic!("link-local upstream must be rejected"),
        Err(err) => err.to_string(),
    };
    assert!(err.contains("link-local"), "{err}");
}

#[test]
fn test_ssrf_denies_ipv4_mapped_ipv6_metadata() {
    // `::ffff:169.254.169.254` is the link-local metadata address in
    // IPv4-mapped form; it must be denied even with the private opt-in.
    let mut cfg = test_config();
    cfg.upstream = "http://[::ffff:169.254.169.254]/latest/meta-data".into();
    cfg.allow_private_upstreams = true;
    let err = validate_upstream(&cfg).unwrap_err().to_string();
    assert!(err.contains("link-local"), "{err}");
}

/// RFC 6598 shared-address space (`100.64.0.0/10`) is not publicly routable and
/// Alibaba Cloud serves instance metadata at `100.100.100.200` inside it, so it
/// must be denied unconditionally — not left to the private opt-in.
#[test]
fn test_ssrf_denies_shared_address_metadata_always() {
    let mut cfg = test_config();
    cfg.upstream = "http://100.100.100.200/latest/meta-data".into();
    // The strongest opt-in must not re-open it.
    cfg.allow_private_upstreams = true;
    cfg.upstream_allowlist = vec!["100.100.100.200".into()];
    let err = validate_upstream(&cfg).unwrap_err().to_string();
    assert!(err.contains("always denied"), "{err}");
}

/// The whole `100.64.0.0/10` block is denied, and its neighbours are not: the
/// check is the /10 boundary, not a `100.` prefix.
#[test]
fn test_ssrf_shared_address_range_boundaries() {
    for denied in ["100.64.0.1", "100.100.100.200", "100.127.255.254"] {
        let mut cfg = test_config();
        cfg.upstream = format!("http://{denied}/");
        assert!(
            validate_upstream(&cfg).is_err(),
            "{denied} is inside 100.64.0.0/10 and must be denied"
        );
    }
    for allowed in ["100.63.255.255", "100.128.0.1"] {
        let mut cfg = test_config();
        cfg.upstream = format!("http://{allowed}/");
        assert!(
            validate_upstream(&cfg).is_ok(),
            "{allowed} is outside 100.64.0.0/10 and must stay usable"
        );
    }
}

/// The WHATWG IPv4 spellings fold to a dotted quad before the range check, so a
/// hex-encoded shared address cannot slip past a textual test.
#[test]
fn test_ssrf_denies_hex_encoded_shared_address() {
    let mut cfg = test_config();
    cfg.upstream = "http://0x64400001/".into();
    let err = validate_upstream(&cfg).unwrap_err().to_string();
    assert!(err.contains("always denied"), "{err}");
}

#[test]
fn test_ssrf_requires_opt_in_for_ipv4_mapped_loopback() {
    let mut cfg = test_config();
    cfg.upstream = "http://[::ffff:127.0.0.1]:9000".into();
    cfg.allow_private_upstreams = false;
    let err = validate_upstream(&cfg).unwrap_err().to_string();
    assert!(err.contains("private/loopback"), "{err}");
}

#[test]
fn test_ssrf_requires_opt_in_for_unspecified_ipv6() {
    let mut cfg = test_config();
    cfg.upstream = "http://[::]:9000".into();
    cfg.allow_private_upstreams = false;
    let err = validate_upstream(&cfg).unwrap_err().to_string();
    assert!(err.contains("private/loopback"), "{err}");
}

#[test]
fn test_ssrf_denies_non_canonical_ipv6_spellings() {
    // The ranges are decided on the parsed address, so an uncompressed or
    // zero-padded spelling of the same address cannot read as a global one.
    let mut cfg = test_config();
    cfg.upstream = "http://[0:0:0:0:0:0:0:1]:9000".into();
    cfg.allow_private_upstreams = false;
    let err = validate_upstream(&cfg).unwrap_err().to_string();
    assert!(err.contains("private/loopback"), "{err}");

    let mut cfg = test_config();
    cfg.upstream = "http://[fc00:0:0:0:0:0:0:1]:9000".into();
    cfg.allow_private_upstreams = false;
    let err = validate_upstream(&cfg).unwrap_err().to_string();
    assert!(err.contains("private/loopback"), "{err}");

    let mut cfg = test_config();
    cfg.upstream = "http://[fe80:0:0:0:0:0:0:1]:9000".into();
    cfg.allow_private_upstreams = true;
    let err = validate_upstream(&cfg).unwrap_err().to_string();
    assert!(err.contains("link-local"), "{err}");
}

#[test]
fn test_ssrf_denies_trailing_dot_ipv4_literal() {
    // WHATWG drops one trailing dot before parsing, so `0x7f000001.` connects
    // to loopback while a guard that only read `0x7f000001` saw an odd hostname.
    let mut cfg = test_config();
    cfg.upstream = "http://0x7f000001.:9000".into();
    cfg.allow_private_upstreams = false;
    let err = validate_upstream(&cfg).unwrap_err().to_string();
    assert!(err.contains("private/loopback"), "{err}");
}

/// Loopback as four octets: the expected parse of every `127.0.0.1` spelling.
const LOOPBACK_OCTETS: [u8; 4] = [127, 0, 0, 1];

#[test]
fn parses_alternate_ipv4_literals() {
    assert_eq!(parse_ipv4_literal("127.0.0.1"), Some(LOOPBACK_OCTETS));
    assert_eq!(parse_ipv4_literal("0x7f000001"), Some(LOOPBACK_OCTETS));
    assert_eq!(parse_ipv4_literal("2130706433"), Some(LOOPBACK_OCTETS));
    assert_eq!(parse_ipv4_literal("017700000001"), Some(LOOPBACK_OCTETS));
    assert_eq!(parse_ipv4_literal("0xa9fea9fe"), Some([169, 254, 169, 254]));
    assert_eq!(parse_ipv4_literal("10"), Some([0, 0, 0, 10]));
    assert_eq!(parse_ipv4_literal("127.1"), Some(LOOPBACK_OCTETS));
    // One trailing dot is stripped by the URL stack before it splits.
    assert_eq!(parse_ipv4_literal("0x7f000001."), Some(LOOPBACK_OCTETS));
    assert_eq!(parse_ipv4_literal("127.0.0.1."), Some(LOOPBACK_OCTETS));
    // Interior empty parts and a bare dot are not literals.
    assert_eq!(parse_ipv4_literal("1..2"), None);
    assert_eq!(parse_ipv4_literal("."), None);
    assert_eq!(parse_ipv4_literal("localhost"), None);
    assert_eq!(parse_ipv4_literal("example.com"), None);
    assert_eq!(parse_ipv4_literal("1.2.3.4.5"), None);
    assert_eq!(parse_ipv4_literal("256.0.0.1"), None);
}

#[test]
fn test_ssrf_normalizes_alternate_ipv4_spellings() {
    for upstream in [
        "http://0x7f000001:9000",
        "http://2130706433:9000",
        "http://017700000001:9000",
    ] {
        let mut cfg = test_config();
        cfg.upstream = upstream.into();
        cfg.allow_private_upstreams = false;
        let err = validate_upstream(&cfg).unwrap_err().to_string();
        assert!(err.contains("private/loopback"), "{upstream}: {err}");
    }
    // Link-local metadata in hex is always denied, even with the opt-in.
    let mut cfg = test_config();
    cfg.upstream = "http://0xa9fea9fe/".into();
    cfg.allow_private_upstreams = true;
    let err = validate_upstream(&cfg).unwrap_err().to_string();
    assert!(err.contains("link-local"), "{err}");
}

#[test]
fn test_ssrf_requires_opt_in_for_loopback() {
    let mut cfg = test_config();
    cfg.allow_private_upstreams = false;
    let err = validate_upstream(&cfg).unwrap_err().to_string();
    assert!(err.contains("private/loopback"), "{err}");

    cfg.allow_private_upstreams = true;
    assert!(validate_upstream(&cfg).is_ok());
    // And the mediator itself enforces the same policy.
    cfg.allow_private_upstreams = false;
    assert!(ProxyMediator::new(cfg).is_err());
}

#[test]
fn test_ssrf_allowlist_is_exact_host_match() {
    let mut cfg = test_config();
    cfg.upstream = "http://api.example.com:9000".into();
    cfg.upstream_allowlist = vec!["other.example.com".into()];
    assert!(validate_upstream(&cfg).is_err());

    cfg.upstream_allowlist = vec!["API.example.com".into()];
    assert!(validate_upstream(&cfg).is_ok());
}

#[test]
fn test_empty_credential_is_rejected_at_construction() {
    let mut cfg = test_config();
    cfg.ingress_token = Some(String::new());
    let err = match ProxyMediator::new(cfg) {
        Ok(_) => panic!("empty ingress_token must be rejected"),
        Err(err) => err.to_string(),
    };
    assert!(err.contains("ingress_token must not be empty"), "{err}");

    let mut cfg = test_config();
    cfg.metrics_token = Some(String::new());
    let err = match ProxyMediator::new(cfg) {
        Ok(_) => panic!("empty metrics_token must be rejected"),
        Err(err) => err.to_string(),
    };
    assert!(err.contains("metrics_token must not be empty"), "{err}");

    let mut cfg = test_config();
    cfg.ingress_token = Some("s3cret".into());
    cfg.metrics_token = Some("mtok".into());
    assert!(ProxyMediator::new(cfg).is_ok());
}

#[test]
fn test_decide_allows_valid() {
    let mediator = ProxyMediator::new(test_config()).expect("create mediator");
    let call = McpLikeToolCall::new("data.read", Some(json!({"path":"/tmp/x"})));
    let decision = mediator.decide(&call).expect("decide");
    assert_eq!(decision, ForwardDecision::Allow);
}

#[test]
fn test_decide_deny_invalid_params() {
    let mediator = ProxyMediator::new(test_config()).expect("create mediator");
    let call = McpLikeToolCall::new("data.read", Some(json!("not a map")));
    let decision = mediator.decide(&call).expect("decide");
    assert!(matches!(decision, ForwardDecision::Deny { .. }));
}

#[test]
fn test_decide_allows_no_params() {
    let mediator = ProxyMediator::new(test_config()).expect("create mediator");
    let call = McpLikeToolCall::new("data.read", None);
    let decision = mediator.decide(&call).expect("decide");
    assert_eq!(decision, ForwardDecision::Allow);
}

#[test]
fn test_bind_upstream_accessors() {
    let mediator = ProxyMediator::new(test_config()).expect("create mediator");
    assert_eq!(mediator.bind(), "127.0.0.1:0");
    assert_eq!(mediator.upstream(), "http://127.0.0.1:9000");
}

/// A policy file that denies `fs.remove` under `/etc`, asks for `shell.exec`
/// from the `ci` caller, and allows everything else.
fn policy_yaml() -> &'static str {
    r#"
version: 1
rules:
  - action: deny
    tool: "fs.remove"
    params:
      path: "/etc/*"
    reason: "never remove system files"
  - action: ask
    tool: "shell.exec"
    caller: "ci"
  - action: allow
    tool: "data.read"
"#
}

fn mediator_with_policy(policy_yaml: &str) -> ProxyMediator {
    let dir = tempfile::tempdir().expect("tempdir");
    let policy_path = dir.path().join("policy.yaml");
    std::fs::write(&policy_path, policy_yaml).expect("write policy");
    let mut cfg = test_config();
    cfg.policy_file = Some(policy_path.display().to_string());
    ProxyMediator::new(cfg).expect("create mediator")
}

#[test]
fn test_mediator_no_policy_reports_no_digest() {
    let mediator = ProxyMediator::new(test_config()).expect("create mediator");
    assert_eq!(mediator.policy_sha256(), None);
}

#[test]
fn test_mediator_loads_policy_and_records_digest() {
    let mediator = mediator_with_policy(policy_yaml());
    let digest = mediator.policy_sha256().expect("digest");
    assert_eq!(digest.len(), 64);
    assert!(digest.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn test_mediator_fails_closed_on_missing_policy_file() {
    let mut cfg = test_config();
    cfg.policy_file = Some("/nonexistent/policy.yaml".to_string());
    match ProxyMediator::new(cfg) {
        Ok(_) => panic!("missing policy must fail closed"),
        Err(err) => assert!(err.to_string().contains("policy"), "{err}"),
    }
}

#[test]
fn test_mediator_fails_closed_on_invalid_policy() {
    let mut cfg = test_config();
    cfg.policy_file = Some("this is not yaml: [".to_string());
    match ProxyMediator::new(cfg) {
        Ok(_) => panic!("invalid policy must fail closed"),
        Err(err) => assert!(err.to_string().contains("policy"), "{err}"),
    }
}

#[test]
fn test_mediator_fails_closed_on_unknown_policy_field() {
    let dir = tempfile::tempdir().expect("tempdir");
    let policy_path = dir.path().join("policy.yaml");
    std::fs::write(&policy_path, "version: 1\nbogus: true\n").expect("write policy");
    let mut cfg = test_config();
    cfg.policy_file = Some(policy_path.display().to_string());
    match ProxyMediator::new(cfg) {
        Ok(_) => panic!("unknown field must fail closed"),
        Err(err) => assert!(err.to_string().contains("policy"), "{err}"),
    }
}

#[test]
fn test_decide_policy_denies_matching_rule() {
    let mediator = mediator_with_policy(policy_yaml());
    let call = McpLikeToolCall::new("fs.remove", Some(json!({"path": "/etc/passwd"})));
    let decision = mediator.decide(&call).expect("decide");
    assert_eq!(
        decision,
        ForwardDecision::Deny {
            reason: "never remove system files".to_string(),
        }
    );
}

#[test]
fn test_decide_policy_deny_rule_does_not_catch_other_params() {
    let mediator = mediator_with_policy(policy_yaml());
    let call = McpLikeToolCall::new("fs.remove", Some(json!({"path": "/home/me.txt"})));
    let decision = mediator.decide(&call).expect("decide");
    // No rule matches, so the fail-closed default denies; the deny rule is
    // scoped to its parameter matcher, not a blanket tool deny.
    match decision {
        ForwardDecision::Deny { reason } => {
            assert!(reason.contains("default deny"), "{reason}");
        }
        ForwardDecision::Allow => panic!("default deny expected"),
    }
}

#[test]
fn test_decide_policy_ask_maps_to_deny() {
    let mediator = mediator_with_policy(policy_yaml());
    let call = McpLikeToolCall::new("shell.exec", None).with_caller(Some("ci".to_string()));
    let decision = mediator.decide(&call).expect("decide");
    match decision {
        ForwardDecision::Deny { reason } => assert!(reason.starts_with("ask:"), "{reason}"),
        ForwardDecision::Allow => panic!("ask must not forward"),
    }
}

#[test]
fn test_decide_policy_caller_pattern_limits_ask() {
    let mediator = mediator_with_policy(policy_yaml());
    // Without the "ci" caller, the ask rule does not match and the default
    // deny applies (the policy has no blanket shell.exec allow).
    let call = McpLikeToolCall::new("shell.exec", None);
    let decision = mediator.decide(&call).expect("decide");
    match decision {
        ForwardDecision::Deny { reason } => {
            assert!(reason.contains("default deny"), "{reason}");
        }
        ForwardDecision::Allow => panic!("default deny expected"),
    }
}
#[test]
fn test_decide_policy_default_denies_when_no_rule_matches() {
    let mediator = mediator_with_policy(policy_yaml());
    let call = McpLikeToolCall::new("unlisted.tool", None);
    let decision = mediator.decide(&call).expect("decide");
    assert!(matches!(decision, ForwardDecision::Deny { .. }));
}

#[cfg(feature = "agt-governance")]
#[test]
fn test_decide_policy_allow_reaches_governance_gate() {
    let mediator = mediator_with_policy(policy_yaml());
    let call = McpLikeToolCall::new("data.read", None);
    let decision = mediator.decide(&call).expect("decide");
    assert_eq!(decision, ForwardDecision::Allow);
}

#[cfg(feature = "agt-governance")]
#[test]
fn test_decide_with_feature_allows() {
    let mediator = ProxyMediator::new(test_config()).expect("create mediator");
    let call = McpLikeToolCall::new("data.read", None);
    let decision = mediator.decide(&call).expect("decide");
    assert_eq!(decision, ForwardDecision::Allow);
}

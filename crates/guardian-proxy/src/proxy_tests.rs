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

#[cfg(feature = "agt-governance")]
#[test]
fn test_decide_with_feature_allows() {
    let mediator = ProxyMediator::new(test_config()).expect("create mediator");
    let call = McpLikeToolCall::new("data.read", None);
    let decision = mediator.decide(&call).expect("decide");
    assert_eq!(decision, ForwardDecision::Allow);
}

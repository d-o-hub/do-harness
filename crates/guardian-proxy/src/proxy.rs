//! Mediation logic: fail-closed gate for proxied tool calls.

use crate::error::Result;
use crate::{ForwardDecision, ProxyConfig};

use serde_json::Value;

/// MCP-like tool call intercepted by the proxy.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct McpLikeToolCall {
    /// Tool name.
    pub tool: String,
    /// Optional JSON params.
    pub params: Option<Value>,
}

impl McpLikeToolCall {
    /// Creates a new call.
    pub fn new(tool: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            tool: tool.into(),
            params,
        }
    }
}

/// Mediator that decides whether to forward a call.
pub struct ProxyMediator {
    config: ProxyConfig,
    #[cfg(feature = "agt-governance")]
    gate: AgtGateWrapper,
}

#[cfg(feature = "agt-governance")]
struct AgtGateWrapper {
    client: agent_governance::AgentMeshClient,
}

#[cfg(feature = "agt-governance")]
impl AgtGateWrapper {
    fn new(agent_id: &str) -> Result<Self> {
        let client = agent_governance::AgentMeshClient::new(agent_id)
            .map_err(|err| crate::error::GuardianError::GovernanceInit(err.to_string()))?;
        Ok(Self { client })
    }

    // The `Result` is the fail-closed error channel: an AGT error (or future
    // transport error) must reach the caller so it can deny. Invalid params
    // already deny here as `Ok(false)`.
    #[allow(clippy::unnecessary_wraps)]
    fn check(&self, tool: &str, params: Option<Value>) -> Result<bool> {
        use std::collections::HashMap;
        let context_map: Option<HashMap<String, serde_yaml::Value>> = match params {
            Some(v) => match serde_json::from_value(v) {
                Ok(map) => Some(map),
                Err(_) => return Ok(false),
            },
            None => None,
        };
        let result = self
            .client
            .execute_with_governance(tool, context_map.as_ref());
        Ok(result.allowed)
    }
}

impl ProxyMediator {
    /// Creates a mediator from the given config.
    ///
    /// # Errors
    ///
    /// Returns an error if the upstream is denied by the SSRF policy or the
    /// governance client cannot be initialized.
    pub fn new(config: ProxyConfig) -> Result<Self> {
        validate_upstream(&config)?;
        #[cfg(feature = "agt-governance")]
        {
            let gate = AgtGateWrapper::new(&config.agent_id)?;
            Ok(Self { config, gate })
        }
        #[cfg(not(feature = "agt-governance"))]
        {
            Ok(Self { config })
        }
    }

    /// Returns the bind address from config.
    #[must_use]
    pub fn bind(&self) -> &str {
        &self.config.bind
    }

    /// Returns the upstream from config.
    #[must_use]
    pub fn upstream(&self) -> &str {
        &self.config.upstream
    }

    /// Decides whether to allow the call (fail-closed).
    ///
    /// Decision mapping is explicit: only an explicit governance allow maps to
    /// [`ForwardDecision::Allow`]; denials, invalid params, and every error map
    /// to [`ForwardDecision::Deny`].
    ///
    /// # Errors
    ///
    /// Returns an error if the governance check itself fails; callers must map
    /// that to a deny.
    // The `Result` stays even in the stub build because the error channel is
    // part of the check contract: AGT errors and future transports surface
    // here, and callers already fail closed on `Err`.
    #[allow(clippy::unnecessary_wraps)]
    pub fn decide(&self, call: &McpLikeToolCall) -> Result<ForwardDecision> {
        #[cfg(feature = "agt-governance")]
        {
            let allowed = self.gate.check(&call.tool, call.params.clone())?;
            if allowed {
                Ok(ForwardDecision::Allow)
            } else {
                Ok(ForwardDecision::Deny {
                    reason: "denied by governance".to_string(),
                })
            }
        }
        #[cfg(not(feature = "agt-governance"))]
        {
            if let Some(ref v) = call.params {
                if !v.is_null() && !v.is_object() {
                    return Ok(ForwardDecision::Deny {
                        reason: "invalid params: expected object".to_string(),
                    });
                }
            }
            Ok(ForwardDecision::Allow)
        }
    }
}

/// SSRF guard: `Ok` only when the configured upstream is safe to call.
///
/// Rules (checked at construction, fail-closed):
/// - link-local/metadata addresses (`169.254.0.0/16`, `metadata.google.internal`)
///   are always rejected;
/// - loopback/private hosts require `allow_private_upstreams = true`;
/// - when `upstream_allowlist` is non-empty the host must match exactly.
///
/// # Errors
///
/// Returns `GuardianError::Config` when the upstream is missing, is not an
/// http(s) URL, or is denied by a rule.
pub fn validate_upstream(config: &ProxyConfig) -> Result<()> {
    let host = upstream_host(&config.upstream).ok_or_else(|| {
        crate::error::GuardianError::Config(format!(
            "upstream '{}' is not an http(s) URL",
            config.upstream
        ))
    })?;
    let host_lower = host.to_ascii_lowercase();
    if is_link_local(&host_lower) {
        return Err(crate::error::GuardianError::Config(format!(
            "upstream host '{host}' is link-local/metadata and always denied"
        )));
    }
    if !config.allow_private_upstreams && is_private_or_loopback(&host_lower) {
        return Err(crate::error::GuardianError::Config(format!(
            "upstream host '{host}' is private/loopback; set allow_private_upstreams=true to permit it"
        )));
    }
    if !config.upstream_allowlist.is_empty()
        && !config
            .upstream_allowlist
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(&host_lower))
    {
        return Err(crate::error::GuardianError::Config(format!(
            "upstream host '{host}' is not in upstream_allowlist"
        )));
    }
    Ok(())
}

/// Extracts the host from an http(s) URL without a URL-parsing dependency.
fn upstream_host(upstream: &str) -> Option<&str> {
    let rest = upstream
        .strip_prefix("http://")
        .or_else(|| upstream.strip_prefix("https://"))?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host_port = authority.rsplit('@').next()?;
    if let Some(rest) = host_port.strip_prefix('[') {
        return rest.split(']').next();
    }
    Some(host_port.split(':').next().unwrap_or(host_port))
}

/// Cloud metadata / link-local ranges that are never valid upstreams.
fn is_link_local(host: &str) -> bool {
    host.starts_with("169.254.") || host == "metadata.google.internal"
}

/// Loopback, private, and unique-local ranges; allowed only with an opt-in.
fn is_private_or_loopback(host: &str) -> bool {
    if host == "localhost" || host == "::1" || host == "0.0.0.0" {
        return true;
    }
    if host.starts_with("127.") || host.starts_with("10.") || host.starts_with("192.168.") {
        return true;
    }
    if let Some(second) = host
        .strip_prefix("172.")
        .and_then(|rest| rest.split('.').next())
        .and_then(|octet| octet.parse::<u8>().ok())
    {
        return (16..=31).contains(&second);
    }
    // IPv6 unique-local (fc00::/7).
    host.starts_with("fc") || host.starts_with("fd")
}

#[cfg(test)]
mod tests {
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
}

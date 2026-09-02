//! Mediation logic: fail-closed gate for proxied tool calls.

use anyhow::Result;
use serde_json::Value;

use crate::{ForwardDecision, ProxyConfig};

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
        use anyhow::Context as _;
        let client = agent_governance::AgentMeshClient::new(agent_id)
            .context("failed to init AgentMeshClient")?;
        Ok(Self { client })
    }

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
    /// Returns an error if the governance client cannot be initialized.
    pub fn new(config: ProxyConfig) -> Result<Self> {
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
    /// # Errors
    ///
    /// Returns an error if the governance check itself fails.
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_config() -> ProxyConfig {
        ProxyConfig {
            bind: "127.0.0.1:0".to_string(),
            upstream: "http://127.0.0.1:9000".to_string(),
            agent_id: "test-agent".to_string(),
        }
    }

    #[test]
    fn test_config_deny_unknown_fields() {
        let bad = r#"{"bind":"a","upstream":"b","agent_id":"c","extra":1}"#;
        let err = serde_json::from_str::<ProxyConfig>(bad).unwrap_err();
        assert!(err.to_string().contains("unknown field"));
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

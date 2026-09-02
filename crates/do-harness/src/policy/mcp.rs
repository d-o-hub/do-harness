//! MCP tool-call mediation surface.
//!
//! Wires `AgtGate::check` so the adapter is not dead code. The mediator is
//! always compiled; when the `agt-governance` feature is disabled it
//! becomes a permissive stub (no governance). When enabled it delegates to
//! the governance gate and stays fail-closed.

use anyhow::Result;
use serde_json::Value;

/// A tool invocation intercepted at the MCP boundary.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ToolCall {
    /// Tool name, e.g. `data.read` or `shell.exec`.
    pub tool: String,
    /// Optional JSON params for the tool.
    pub params: Option<Value>,
}

#[allow(dead_code)]
impl ToolCall {
    /// Creates a new tool call.
    pub fn new(tool: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            tool: tool.into(),
            params,
        }
    }
}

/// Mediator that gates tool calls through AGT when the feature is enabled.
#[allow(dead_code)]
pub struct McpMediator {
    #[cfg(feature = "agt-governance")]
    gate: crate::policy::agt::AgtGate,
}

#[allow(dead_code)]
impl McpMediator {
    /// Creates a mediator for the given logical agent id.
    ///
    /// With the feature off this is infallible and stores no state.
    /// With the feature on it constructs the underlying `AgtGate`.
    #[allow(clippy::unnecessary_wraps, clippy::unused_self)]
    pub fn new(agent_id: &str) -> Result<Self> {
        #[cfg(feature = "agt-governance")]
        {
            let gate = crate::policy::agt::AgtGate::new(agent_id)?;
            Ok(Self { gate })
        }
        #[cfg(not(feature = "agt-governance"))]
        {
            let _ = agent_id;
            Ok(Self {})
        }
    }

    /// Checks whether the given tool call is allowed.
    ///
    /// Fail-closed: invalid params deny, governance errors deny.
    #[allow(clippy::unnecessary_wraps, clippy::unused_self)]
    pub fn check(&self, call: &ToolCall) -> Result<bool> {
        #[cfg(feature = "agt-governance")]
        {
            self.gate.check(&call.tool, call.params.clone())
        }
        #[cfg(not(feature = "agt-governance"))]
        {
            // Permissive stub when governance is not compiled in.
            // Still fail-closed on non-object params to preserve the
            // contract surface for callers that rely on it.
            if let Some(ref v) = call.params {
                if !v.is_null() && !v.is_object() {
                    return Ok(false);
                }
            }
            Ok(true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_mediator_allows_valid_call_without_feature() {
        let mediator = McpMediator::new("test-agent").expect("create mediator");
        let call = ToolCall::new("data.read", Some(json!({"path": "/tmp/test"})));
        let allowed = mediator.check(&call).expect("check");
        assert!(allowed);
    }

    #[test]
    fn test_mediator_deny_invalid_params() {
        let mediator = McpMediator::new("test-agent").expect("create mediator");
        let call = ToolCall::new("data.read", Some(json!("not a map")));
        let allowed = mediator.check(&call).expect("check");
        // Fail-closed: non-map params must deny.
        assert!(!allowed);
    }

    #[test]
    fn test_mediator_allows_no_params() {
        let mediator = McpMediator::new("test-agent").expect("create mediator");
        let call = ToolCall::new("data.read", None);
        let allowed = mediator.check(&call).expect("check");
        assert!(allowed);
    }

    #[cfg(feature = "agt-governance")]
    #[test]
    fn test_mediator_with_feature_allows() {
        let mediator = McpMediator::new("test-agent").expect("create mediator");
        let call = ToolCall::new("data.read", None);
        let allowed = mediator.check(&call).expect("check");
        assert!(allowed);
    }
}

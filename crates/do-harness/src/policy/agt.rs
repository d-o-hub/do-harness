//! Optional adapter for microsoft/agent-governance-toolkit.
//! Compile-time gated via the `agt-governance` feature.

use std::collections::HashMap;

use anyhow::{Context, Result};

/// Governance gate delegating execution policy checks to AGT's [`agent_governance::AgentMeshClient`].
pub struct AgtGate {
    client: agent_governance::AgentMeshClient,
}

impl AgtGate {
    /// Creates a new AGT governance gate for the given `agent_id`.
    pub fn new(agent_id: &str) -> Result<Self> {
        Ok(Self {
            client: agent_governance::AgentMeshClient::new(agent_id)
                .context("failed to init AgentMeshClient")?,
        })
    }

    /// Evaluates an action against AGT governance policy.
    ///
    /// Fail-closed: a policy-runtime error or invalid parameters must deny, never allow (fail-closed doctrine).
    pub fn check(&self, action: &str, params: Option<serde_json::Value>) -> Result<bool> {
        let context_map: Option<HashMap<String, serde_yaml::Value>> = match params {
            Some(v) => match serde_json::from_value(v) {
                Ok(map) => Some(map),
                Err(_) => return Ok(false),
            },
            None => None,
        };

        let result = self
            .client
            .execute_with_governance(action, context_map.as_ref());
        Ok(result.allowed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_agt_gate_default_allow() {
        let gate = AgtGate::new("test-agent").expect("failed to create gate");
        let allowed = gate.check("data.read", None).expect("check failed");
        assert!(allowed);
    }

    #[test]
    fn test_agt_gate_invalid_params_fail_closed() {
        let gate = AgtGate::new("test-agent").expect("failed to create gate");
        let allowed = gate
            .check("data.read", Some(json!("not a map")))
            .expect("check failed");
        assert!(!allowed);
    }

    #[test]
    fn test_agt_gate_valid_params() {
        let gate = AgtGate::new("test-agent").expect("failed to create gate");
        let allowed = gate
            .check("data.read", Some(json!({"path": "/tmp/test"})))
            .expect("check failed");
        assert!(allowed);
    }
}

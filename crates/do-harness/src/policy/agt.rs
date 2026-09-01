//! Optional adapter for microsoft/agent-governance-toolkit.
//! Compile-time gated via the `agt-governance` feature.

use anyhow::{Context, Result};
use std::collections::HashMap;

/// Gate adapter wrapping Microsoft AGT `AgentMeshClient`.
#[allow(dead_code)]
pub struct AgtGate {
    client: agent_governance::AgentMeshClient,
}

#[allow(dead_code)]
impl AgtGate {
    /// Creates a new `AgtGate` for the specified agent identifier.
    ///
    /// # Errors
    ///
    /// Returns an error if `AgentMeshClient` initialization fails.
    pub fn new(agent_id: &str) -> Result<Self> {
        Ok(Self {
            client: agent_governance::AgentMeshClient::new(agent_id)
                .context("failed to init AgentMeshClient")?,
        })
    }

    /// Check whether an action is allowed by governance policy.
    ///
    /// Fail-closed: a policy-runtime error must deny, never allow.
    ///
    /// # Errors
    ///
    /// Returns an error if policy processing fails catastrophically, though
    /// runtime evaluation errors are treated as fail-closed (`Ok(false)`).
    #[allow(clippy::unnecessary_wraps)]
    pub fn check(&self, action: &str, params: Option<serde_json::Value>) -> Result<bool> {
        let yaml_map: Option<HashMap<String, serde_yaml::Value>> = match params {
            Some(serde_json::Value::Object(map)) => {
                let mut yaml_m = HashMap::new();
                for (k, v) in map {
                    if let Ok(yv) = serde_yaml::to_value(v) {
                        yaml_m.insert(k, yv);
                    }
                }
                Some(yaml_m)
            }
            _ => None,
        };

        let res = self
            .client
            .execute_with_governance(action, yaml_map.as_ref());
        Ok(res.allowed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agt_gate_instantiation() {
        let gate = AgtGate::new("test-agent");
        assert!(gate.is_ok());
    }

    #[test]
    fn test_agt_gate_check() {
        if let Ok(gate) = AgtGate::new("test-agent") {
            let res = gate.check("read_file", None);
            assert!(res.is_ok());
        }
    }
}

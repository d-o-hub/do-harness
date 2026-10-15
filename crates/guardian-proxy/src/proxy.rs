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
    /// Returns an error if the upstream is denied by the SSRF policy, a
    /// configured credential is empty, or the governance client cannot be
    /// initialized.
    pub fn new(config: ProxyConfig) -> Result<Self> {
        validate_upstream(&config)?;
        validate_credentials(&config)?;
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

    /// Returns the browser origins allowed on the MCP endpoint.
    #[must_use]
    pub fn allowed_origins(&self) -> &[String] {
        &self.config.allowed_origins
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

/// Rejects an empty bearer token.
///
/// An empty credential would authenticate a caller that sends `Bearer ` with
/// nothing after it, which is only ever a configuration typo.
fn validate_credentials(config: &ProxyConfig) -> Result<()> {
    for (field, value) in [
        ("ingress_token", &config.ingress_token),
        ("metrics_token", &config.metrics_token),
    ] {
        if value.as_deref().is_some_and(str::is_empty) {
            return Err(crate::error::GuardianError::Config(format!(
                "{field} must not be empty"
            )));
        }
    }
    Ok(())
}

/// SSRF guard: `Ok` only when the configured upstream is safe to call.
///
/// Rules (checked at construction, fail-closed):
/// - link-local/metadata addresses (`169.254.0.0/16`, `metadata.google.internal`,
///   `IPv6` `fe80::/10`) are always rejected;
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
    let host_lower = normalize_host_literal(&host.to_ascii_lowercase());
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

/// Rewrites a host literal to the address that will actually be dialled so the
/// range checks cannot be bypassed by alternate spellings: an IPv4-mapped IPv6
/// host (`::ffff:169.254.169.254`) becomes its embedded dotted IPv4 form, the
/// unspecified address (`::`) becomes `0.0.0.0`, and a WHATWG-style IPv4
/// literal (`0x7f000001`, `2130706433`, `017700000001`) becomes a dotted quad.
/// Non-literals pass through unchanged, so hostnames keep the textual checks.
fn normalize_host_literal(host: &str) -> String {
    if let Ok(v6) = host.parse::<std::net::Ipv6Addr>() {
        if let Some(v4) = v6.to_ipv4_mapped() {
            return v4.to_string();
        }
        if v6.is_unspecified() {
            return "0.0.0.0".to_string();
        }
        return host.to_string();
    }
    match parse_ipv4_literal(host) {
        Some(octets) => std::net::Ipv4Addr::from(octets).to_string(),
        None => host.to_string(),
    }
}

/// Parses a WHATWG-style IPv4 literal — decimal, octal (`0NNN`), or hex
/// (`0xNN`) with one to four dot-separated parts — into its four octets.
///
/// The URL stack folds these alternate spellings to a dotted quad at connect
/// time, so the range checks must see the same value; otherwise
/// `http://0x7f000001/` would reach loopback while the guard saw a hostname.
/// Returns `None` when the input is not an IPv4 literal.
fn parse_ipv4_literal(host: &str) -> Option<[u8; 4]> {
    if host.is_empty() {
        return None;
    }
    let mut numbers: Vec<u64> = Vec::new();
    for part in host.split('.') {
        let (radix, digits) =
            if part.len() > 2 && (part.starts_with("0x") || part.starts_with("0X")) {
                (16u32, &part[2..])
            } else if part.len() > 1 && part.starts_with('0') {
                (8u32, &part[1..])
            } else {
                (10u32, part)
            };
        if digits.is_empty() {
            return None;
        }
        numbers.push(u64::from_str_radix(digits, radix).ok()?);
    }
    if numbers.len() > 4 {
        return None;
    }
    let last = numbers.pop()?;
    // Every leading part is one octet; the final part fills the remainder.
    if numbers.iter().any(|&n| n > 255) || last >= 1u64 << (8 * (4 - numbers.len())) {
        return None;
    }
    let mut ipv4 = last;
    for (i, &n) in numbers.iter().enumerate() {
        ipv4 += n << (8 * (3 - i));
    }
    Some([
        ((ipv4 >> 24) & 0xff) as u8,
        ((ipv4 >> 16) & 0xff) as u8,
        ((ipv4 >> 8) & 0xff) as u8,
        (ipv4 & 0xff) as u8,
    ])
}

/// Cloud metadata / link-local ranges that are never valid upstreams.
fn is_link_local(host: &str) -> bool {
    // IPv4 link-local (169.254.0.0/16) and the cloud metadata aliases.
    if host.starts_with("169.254.") || host == "metadata.google.internal" {
        return true;
    }
    // IPv6 link-local (fe80::/10) and the deprecated site-local prefixes
    // (fec0:/fed0:). The host is lowercased before this check.
    host.starts_with("fe80:") || host.starts_with("fec0:") || host.starts_with("fed0:")
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
#[path = "proxy_tests.rs"]
mod tests;

//! Bounded client-profile parsing: expected claims and pinned identities.
//!
//! A profile declares what an adopter *expects*. It carries no observations, so
//! a config flag, an installed skill or a claimed header can never satisfy an
//! enforcement claim on its own.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::exit::UsageError;

/// Only accepted profile schema version.
pub(crate) const PROFILE_SCHEMA_VERSION: u32 = 1;

/// Largest profile document accepted, in bytes.
pub(crate) const MAX_PROFILE_BYTES: usize = 64 * 1024;

/// Longest accepted identifier (client id/version, configuration, route).
pub(crate) const MAX_IDENTIFIER_BYTES: usize = 100;

/// Maximum accepted adapter `argv` entries.
const MAX_ADAPTER_ARGS: usize = 16;

/// Longest accepted single adapter argument.
const MAX_ADAPTER_ARG_BYTES: usize = 200;

/// A SHA-256 digest rendered as lowercase hexadecimal.
pub(crate) const SHA256_HEX_LEN: usize = 64;

/// Claims a profile may declare; each is observed independently.
pub(crate) const CLAIM_IDS: [&str; 7] = [
    "guidance_installed",
    "host_boundary",
    "mcp_mediation",
    "governance_enforced",
    "completion_gated",
    "context_sanitization",
    "outbound_mediation",
];

/// Claims that are meaningless without the route they were observed on.
const ROUTE_BOUND_CLAIMS: [&str; 4] = [
    "host_boundary",
    "mcp_mediation",
    "governance_enforced",
    "completion_gated",
];

/// Reads at most `max` bytes from `path`, refusing anything larger.
///
/// # Errors
///
/// Returns an error when the file cannot be read, exceeds `max`, or is not
/// UTF-8: every one of those makes the document unusable as evidence.
pub(crate) fn read_bounded(path: &Path, max: usize) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    if bytes.len() > max {
        anyhow::bail!(
            "{} is {} bytes, over the {max}-byte limit",
            path.display(),
            bytes.len()
        );
    }
    String::from_utf8(bytes).with_context(|| format!("{} is not UTF-8", path.display()))
}

/// Whether `value` is a bounded, log-safe identifier.
pub(crate) fn is_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_IDENTIFIER_BYTES
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b':' | b'/' | b'-'))
}

/// Whether `value` is a lowercase hexadecimal SHA-256 digest.
pub(crate) fn is_sha256_hex(value: &str) -> bool {
    value.len() == SHA256_HEX_LEN
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Pinned client identity an observation must match.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ClientPin {
    pub(crate) id: String,
    pub(crate) version: String,
}

/// Trusted local observation adapter and its pinned digest.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Adapter {
    pub(crate) argv: Vec<String>,
    pub(crate) sha256: String,
}

/// What the profile expects of one claim.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ClaimRequirement {
    pub(crate) required: bool,
    #[serde(default)]
    pub(crate) route: Option<String>,
}

/// A parsed, validated client profile.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Profile {
    pub(crate) schema_version: u32,
    pub(crate) client: ClientPin,
    pub(crate) configuration: String,
    #[serde(default)]
    pub(crate) adapter: Option<Adapter>,
    #[serde(default)]
    pub(crate) observations: Option<String>,
    #[serde(default)]
    pub(crate) conformance_report: Option<String>,
    pub(crate) claims: BTreeMap<String, ClaimRequirement>,
}

impl Adapter {
    fn validate(&self) -> std::result::Result<(), String> {
        if self.argv.is_empty() || self.argv.len() > MAX_ADAPTER_ARGS {
            return Err(format!(
                "adapter.argv must have 1..={MAX_ADAPTER_ARGS} entries"
            ));
        }
        for arg in &self.argv {
            if arg.is_empty() || arg.len() > MAX_ADAPTER_ARG_BYTES {
                return Err(format!(
                    "adapter.argv entries must be 1..={MAX_ADAPTER_ARG_BYTES} bytes"
                ));
            }
        }
        if !is_sha256_hex(&self.sha256) {
            return Err("adapter.sha256 must be 64 lowercase hexadecimal characters".to_owned());
        }
        Ok(())
    }
}

impl Profile {
    /// Reads and validates a profile, classifying operator mistakes as usage.
    ///
    /// # Errors
    ///
    /// A missing, oversized, unparsable, or semantically invalid profile is an
    /// operator problem (exit 2): no claim was evaluated, so it cannot be a
    /// verification failure.
    pub(crate) fn load(path: &Path) -> Result<Self> {
        let raw = read_bounded(path, MAX_PROFILE_BYTES).map_err(UsageError::wrap)?;
        let profile: Self = serde_json::from_str(&raw).map_err(|err| {
            UsageError::msg(format!(
                "client profile {} is invalid: {err}",
                path.display()
            ))
        })?;
        profile.validate().map_err(|err| {
            UsageError::msg(format!(
                "client profile {} is invalid: {err}",
                path.display()
            ))
        })?;
        Ok(profile)
    }

    fn validate(&self) -> std::result::Result<(), String> {
        if self.schema_version != PROFILE_SCHEMA_VERSION {
            return Err(format!(
                "unsupported schema_version {} (expected {PROFILE_SCHEMA_VERSION})",
                self.schema_version
            ));
        }
        require_identifier("client.id", &self.client.id)?;
        require_identifier("client.version", &self.client.version)?;
        require_identifier("configuration", &self.configuration)?;
        if let Some(adapter) = &self.adapter {
            adapter.validate()?;
        }
        if self.claims.is_empty() {
            return Err("declares no claims".to_owned());
        }
        if self.claims.len() > CLAIM_IDS.len() {
            return Err("declares more claims than the protocol defines".to_owned());
        }
        for (id, requirement) in &self.claims {
            if !CLAIM_IDS.contains(&id.as_str()) {
                return Err(format!("declares unknown claim '{id}'"));
            }
            match (
                &requirement.route,
                ROUTE_BOUND_CLAIMS.contains(&id.as_str()),
            ) {
                (Some(route), _) => require_identifier(&format!("claims.{id}.route"), route)?,
                (None, true) => {
                    return Err(format!("claim '{id}' must name the route it was tested on"));
                }
                (None, false) => {}
            }
        }
        Ok(())
    }
}

fn require_identifier(field: &str, value: &str) -> std::result::Result<(), String> {
    if is_identifier(value) {
        Ok(())
    } else {
        Err(format!(
            "{field} must be 1..={MAX_IDENTIFIER_BYTES} characters of [A-Za-z0-9_.:/-]"
        ))
    }
}

//! Declarative policy engine for the guardian proxy.
//!
//! Policies are versioned YAML files evaluated by [`crate::ProxyMediator`]
//! at startup (fail-closed) or offline via `guardian-proxy policy test`.
//! The schema is deliberately small: allow/deny/ask rules matched against
//! tool name, caller identity, and parameter values. YAML was chosen over
//! Rego/OPA because the repository already parses YAML (`serde_yaml`), the
//! schema stays serde-native, and it adds no runtime or tooling dependency.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::McpLikeToolCall;
use crate::error::{GuardianError, Result};

/// Supported policy schema version.
pub const POLICY_FILE_VERSION: u32 = 1;

/// Reason recorded when no rule matches a call (default deny).
pub const NO_RULE_MATCH_REASON: &str = "no policy rule matched (default deny)";

/// Outcome of evaluating a call against a policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    /// A rule explicitly allowed the call.
    Allow,
    /// A rule denied the call; `reason` is the human-readable explanation.
    Deny {
        /// Human-readable explanation.
        reason: String,
    },
    /// The call requires human approval before it may proceed. The runtime
    /// mediator maps this to a deny (there is no interactive approval
    /// channel); `policy test` fixtures observe it as its own outcome.
    Ask {
        /// Human-readable explanation.
        reason: String,
    },
}

/// Action a matching rule prescribes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleAction {
    /// Forward the call.
    Allow,
    /// Deny without forwarding.
    Deny,
    /// Require human approval (maps to deny at runtime).
    Ask,
}

/// One declarative rule.
///
/// Rules are evaluated in file order; the first matching rule decides, so
/// precedence is expressed by ordering (list more specific rules first).
/// `tool` and `caller` are glob patterns (`*` matches any run of
/// characters). When `params` is present, every key must match for the rule
/// to apply: string expectations are globs, anything else compares for JSON
/// equality. An absent parameter never matches.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    /// Action to take when the rule matches.
    pub action: RuleAction,
    /// Tool-name pattern (`*` wildcards).
    pub tool: String,
    /// Caller-identity pattern (`*` wildcards); unnamed callers match `*`.
    #[serde(default = "default_wildcard")]
    pub caller: String,
    /// Parameter expectations; `None` matches any parameters.
    #[serde(default)]
    pub params: Option<BTreeMap<String, Value>>,
    /// Optional reason overrides the default deny/ask explanation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Versioned policy document.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    /// Schema version; must equal [`POLICY_FILE_VERSION`].
    pub version: u32,
    /// Rules in precedence order (first match wins).
    #[serde(default)]
    pub rules: Vec<Rule>,
}

/// A parsed policy plus the SHA-256 digest of its source bytes.
#[derive(Debug)]
pub struct LoadedPolicy {
    /// Parsed policy.
    pub policy: Policy,
    /// Lowercase hex SHA-256 of the policy file bytes.
    pub sha256: String,
}

/// Result of running a fixture directory.
#[derive(Debug, Default)]
pub struct FixtureSummary {
    /// Fixture files evaluated.
    pub total: usize,
    /// Fixtures whose decision matched the expectation.
    pub passed: usize,
    /// Human-readable mismatch/parse failures, one per fixture.
    pub failures: Vec<String>,
}

/// A single offline decision case.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fixture {
    /// Optional fixture name used in failure messages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The request to evaluate.
    pub request: McpLikeToolCall,
    /// Expected decision kind: `allow`, `deny`, or `ask`.
    pub expected: String,
}

impl Policy {
    /// Evaluates a call against this policy.
    ///
    /// The first rule matching the call decides; a call matching no rule is
    /// denied (default deny).
    #[must_use]
    pub fn evaluate(&self, call: &McpLikeToolCall) -> PolicyDecision {
        self.rules
            .iter()
            .find(|rule| rule.matches(call))
            .map_or_else(
                || PolicyDecision::Deny {
                    reason: NO_RULE_MATCH_REASON.to_string(),
                },
                Rule::decision,
            )
    }
}

impl Rule {
    fn matches(&self, call: &McpLikeToolCall) -> bool {
        wildcard_match(&self.tool, &call.tool)
            && wildcard_match(&self.caller, call.caller.as_deref().unwrap_or_default())
            && self
                .params
                .as_ref()
                .is_none_or(|expected| params_match(expected, call.params.as_ref()))
    }

    fn decision(&self) -> PolicyDecision {
        match self.action {
            RuleAction::Allow => PolicyDecision::Allow,
            RuleAction::Deny => PolicyDecision::Deny {
                reason: self
                    .reason
                    .clone()
                    .unwrap_or_else(|| "denied by policy".to_string()),
            },
            RuleAction::Ask => PolicyDecision::Ask {
                reason: self
                    .reason
                    .clone()
                    .unwrap_or_else(|| "requires human approval".to_string()),
            },
        }
    }
}

/// Loads and validates a policy file, returning it with the SHA-256 digest
/// of the source bytes.
///
/// # Errors
///
/// Returns an error when the file cannot be read, is not valid YAML, has
/// unknown fields, or declares an unsupported version (fail-closed).
pub fn load_policy(path: &Path) -> Result<LoadedPolicy> {
    let content = std::fs::read_to_string(path).map_err(|io| GuardianError::PolicyIo {
        path: path.display().to_string(),
        io,
    })?;
    let sha256 = hex::encode(Sha256::digest(content.as_bytes()));
    let policy: Policy =
        serde_yaml::from_str(&content).map_err(|err| GuardianError::PolicyParse {
            path: path.display().to_string(),
            message: err.to_string(),
        })?;
    if policy.version != POLICY_FILE_VERSION {
        return Err(GuardianError::PolicyUnsupportedVersion(policy.version));
    }
    Ok(LoadedPolicy { policy, sha256 })
}

/// Evaluates every fixture in `fixtures_dir` against the policy at
/// `policy_path`.
///
/// Hidden fixture files are ignored; JSON and YAML inputs are both accepted.
/// Failures are collected and reported, never short-circuiting the run.
///
/// # Errors
///
/// Returns an error when the policy file or the fixture directory cannot be
/// read.
pub fn run_fixtures(policy_path: &Path, fixtures_dir: &Path) -> Result<FixtureSummary> {
    let loaded = load_policy(policy_path)?;
    let mut summary = FixtureSummary::default();
    for path in read_fixture_entries(fixtures_dir)? {
        summary.total += 1;
        let label = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        match evaluate_fixture(&loaded.policy, &path) {
            Ok(()) => summary.passed += 1,
            Err(message) => summary.failures.push(format!("{label}: {message}")),
        }
    }
    Ok(summary)
}

/// Matches `pattern` (with `*` wildcards) against `text`.
pub(crate) fn wildcard_match(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut mark) = (None, 0usize);
    while ti < text.len() {
        if pi < pattern.len() && pattern[pi] == text[ti] {
            pi += 1;
            ti += 1;
        } else if pi < pattern.len() && pattern[pi] == '*' {
            star = Some(pi);
            pi += 1;
            mark = ti;
        } else if let Some(star) = star {
            pi = star + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < pattern.len() && pattern[pi] == '*' {
        pi += 1;
    }
    pi == pattern.len()
}

fn default_wildcard() -> String {
    "*".to_string()
}

fn params_match(expected: &BTreeMap<String, Value>, actual: Option<&Value>) -> bool {
    let Some(Value::Object(actual)) = actual else {
        return false;
    };
    expected
        .iter()
        .all(|(key, want)| actual.get(key).is_some_and(|got| value_matches(want, got)))
}

fn value_matches(expected: &Value, actual: &Value) -> bool {
    match (expected, actual) {
        (Value::String(pattern), Value::String(text)) => wildcard_match(pattern, text),
        _ => expected == actual,
    }
}

fn read_fixture_entries(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut entries = std::fs::read_dir(dir)
        .map_err(|io| GuardianError::PolicyIo {
            path: dir.display().to_string(),
            io,
        })?
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| matches!(ext, "json" | "yaml" | "yml"))
        })
        .collect::<Vec<_>>();
    entries.sort();
    Ok(entries)
}

fn evaluate_fixture(policy: &Policy, path: &Path) -> std::result::Result<(), String> {
    let content = std::fs::read_to_string(path).map_err(|err| format!("read failed: {err}"))?;
    let fixture: Fixture = if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
        serde_json::from_str(&content).map_err(|err| format!("parse failed: {err}"))?
    } else {
        serde_yaml::from_str(&content).map_err(|err| format!("parse failed: {err}"))?
    };
    let expected = parse_expected(&fixture.expected).map_err(ToOwned::to_owned)?;
    let decision = policy.evaluate(&fixture.request);
    if decision_kind(&decision) != decision_kind(&expected) {
        return Err(format!(
            "expected {}, got {}",
            fixture.expected,
            decision_kind(&decision)
        ));
    }
    Ok(())
}

fn decision_kind(decision: &PolicyDecision) -> &'static str {
    match decision {
        PolicyDecision::Allow => "allow",
        PolicyDecision::Deny { .. } => "deny",
        PolicyDecision::Ask { .. } => "ask",
    }
}

fn parse_expected(expected: &str) -> std::result::Result<PolicyDecision, &'static str> {
    match expected {
        "allow" => Ok(PolicyDecision::Allow),
        "deny" => Ok(PolicyDecision::Deny {
            reason: String::new(),
        }),
        "ask" => Ok(PolicyDecision::Ask {
            reason: String::new(),
        }),
        _ => Err("invalid expected decision: must be allow, deny, or ask"),
    }
}

#[cfg(test)]
#[path = "policy_tests.rs"]
mod tests;

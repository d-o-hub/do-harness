//! Sensor policy and severity types.

use serde::{Deserialize, Serialize};

/// Gate severity for a sensor.
///
/// `error` failures fail the verify gate; `warn` failures are advisory in
/// non-strict runs and only fail under `--strict` (except in `feedback`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SensorSeverity {
    /// A failure fails the gate.
    #[default]
    Error,
    /// A failure is advisory (warn-only).
    Warn,
}

/// The role a sensor plays in the gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SensorKind {
    /// A gate whose failure is judged by its severity.
    #[default]
    Command,
    /// A soft, project-specific facts check (status documents, tracker
    /// counts, published-claim drift). Warn-only unless `severity` says
    /// otherwise, and it carries a `fix` hint shown on failure.
    ProjectCheck,
}

/// A single computational sensor.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SensorSpec {
    /// Unique sensor name (e.g. "fmt").
    pub name: String,
    /// Sensor kind: `command` (the default gate) or `project-check` (a soft
    /// facts-drift check, warn-only unless `severity` overrides it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<SensorKind>,
    /// Command line, program first (e.g. `["cargo", "fmt", "--all", "--", "--check"]`).
    pub argv: Vec<String>,
    /// Optional number of retry attempts on failure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<u32>,
    /// Process timeout seconds (default one hour); 0 requires a justification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    /// Required reason for the explicit timeout=0 unlimited opt-out.
    #[serde(
        default,
        rename = "timeout-justification",
        skip_serializing_if = "Option::is_none"
    )]
    pub timeout_justification: Option<String>,
    /// Combined raw stdout/stderr budget; defaults to 1 `MiB`, at most 16 `MiB`.
    #[serde(
        default,
        rename = "max-output-bytes",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_output_bytes: Option<usize>,
    /// None inherits the environment; Some explicitly allowlists inherited keys.
    #[serde(
        default,
        rename = "environment-allowlist",
        skip_serializing_if = "Option::is_none"
    )]
    pub environment_allowlist: Option<Vec<String>>,
    /// Additional environment keys whose nonempty values must be redacted.
    #[serde(default, rename = "redact-env", skip_serializing_if = "Vec::is_empty")]
    pub redact_env: Vec<String>,
    /// Gate severity (`error` default, `warn` advisory).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<SensorSeverity>,
    /// Remediation hint printed with a failing sensor (e.g. how to refresh a
    /// drifted status document).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
    /// Deprecated alias for `severity = "warn"`.
    #[serde(
        default,
        rename = "allow_failure",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub allow_failure: bool,
    /// Reserved exit codes that signal transient failures eligible for retry/warn handling.
    #[serde(
        default,
        rename = "transient_exit_codes",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub transient_exit_codes: Vec<i32>,
    /// Repository-relative glob patterns selecting when this sensor is
    /// applicable to the current change (e.g. `["**/*.rs", "Cargo.toml"]`).
    /// Empty means the sensor is always applicable.
    #[serde(
        default,
        rename = "when-changed",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub when_changed: Vec<String>,
    /// Repository-relative glob patterns of artifacts this sensor produces
    /// (screenshots, reports, JSON findings). Matches are SHA-256 digested
    /// into the evidence artifact; a declared glob that matches nothing makes
    /// the run non-strict.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<String>,
    /// Repository-relative glob patterns of files that define what this
    /// sensor covers (e.g. a viewport/locale matrix module). Their content is
    /// part of the policy fingerprint, so changing the definition makes
    /// evidence stale without re-running sensors.
    #[serde(
        default,
        rename = "coverage-inputs",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub coverage_inputs: Vec<String>,
    /// Repository-relative glob patterns of every file whose content can
    /// change this sensor's outcome (sources, configs, fixtures, lockfiles).
    /// A nonempty declaration opts the sensor into unchanged-input reuse:
    /// `verify --unchanged` may reuse a recorded passing beat when every
    /// matched path and its content is identical. Empty (the default) means
    /// the sensor always runs. The declaration must be complete — commands
    /// that also read the environment, network, generated outputs, or
    /// history cannot safely reuse and must stay undeclared.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<String>,
    /// Environment variables whose values affect verification (only digests are stored).
    #[serde(
        default,
        rename = "environment-inputs",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub environment_inputs: Vec<String>,
}

impl SensorSpec {
    /// Effective severity after folding in the deprecated `allow_failure`
    /// alias and the `project-check` kind: warn when either source says warn
    /// or when a soft check leaves the severity unset; an explicit `severity`
    /// always wins.
    #[must_use]
    pub fn effective_severity(&self) -> SensorSeverity {
        match (self.severity, self.allow_failure) {
            (Some(SensorSeverity::Warn), _) | (None, true) => SensorSeverity::Warn,
            (None, false) if self.kind == Some(SensorKind::ProjectCheck) => SensorSeverity::Warn,
            _ => SensorSeverity::Error,
        }
    }
}

//! The `do-harness-sensor` plugin protocol.
//!
//! Sensors are trusted local commands declared in `do-harness.toml` as
//! `[[sensors]]` entries with a `name` and an `argv`. This crate is the
//! distribution protocol for reusable sensor packages: a *plugin* is a
//! versioned manifest (`sensor-plugin.toml`) that carries plugin metadata and
//! a full [`SensorSpec`] table. The harness discovers manifests under the
//! repository's `plugins/` directory and resolves `[[sensors]]` entries that
//! name a plugin (`plugin = "ruff"`) against them.
//!
//! Plugins are process-based: the manifest's `argv` is executed by the
//! harness exactly like an inline sensor's, with the same trust boundary and
//! resource policy as inline sensors. A plugin never loads Rust code into the
//! harness process; a third party may implement a plugin in any language that
//! ships an executable `argv`.
//!
//! # Manifest schema
//!
//! A manifest is TOML with `deny_unknown_fields` semantics; an unknown key is
//! a configuration error, never a silently ignored field. The `schema-version`
//! key must equal [`PLUGIN_SCHEMA_VERSION`]; a newer version fails closed so
//! the harness never guesses what a future manifest means.
//!
//! ```toml
//! schema-version = 1
//! version = "1.2.0"
//! description = "Ruff lint gate"
//!
//! [sensor]
//! name = "ruff"
//! argv = ["ruff", "check", "."]
//! when-changed = ["**/*.py"]
//! inputs = ["**/*.py", "pyproject.toml"]
//! severity = "error"
//! ```
//!
//! The `[sensor]` table is a full `[[sensors]]` entry ([`SensorSpec`]) with
//! the `plugin` field reserved for the harness's resolution bookkeeping; a
//! manifest must not set it. The harness merges the manifest's sensor
//! definition into the `[[sensors]]` entry that references the plugin, with
//! inline fields taking precedence over manifest defaults, and refuses name
//! collisions when resolving. See the harness's `plugins` module and
//! `docs/sensor-plugins.md` for the discovery and resolution contract.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use serde::{Deserialize, Serialize};

/// Current plugin manifest schema version. A manifest declaring any other
/// version is rejected; the harness fails closed rather than interpret a
/// future schema.
pub const PLUGIN_SCHEMA_VERSION: u32 = 1;

/// File name of a plugin manifest inside a plugin directory.
pub const PLUGIN_MANIFEST_FILE: &str = "sensor-plugin.toml";

/// Repository-relative directory the harness scans for plugin manifests.
pub const PLUGINS_DIR: &str = "plugins";

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
    /// Plugin reference: when set, this `[[sensors]]` entry resolves its
    /// definition from the installed plugin of the same name instead of
    /// declaring `argv` inline. The plugin's values are the defaults; fields
    /// set on this entry override them. Set by the harness during resolution;
    /// a plugin manifest's `[sensor]` table must not set it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
    /// Command line, program first (e.g.
    /// `["cargo", "fmt", "--all", "--", "--check"]`). Optional in a
    /// `[[sensors]]` entry that references a plugin (`plugin = "…"`): an
    /// omitted or empty `argv` inherits the plugin's. An inline sensor with
    /// an empty `argv` is unrunnable and fails at verify time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
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

/// A versioned sensor plugin manifest (`sensor-plugin.toml`).
///
/// The `[sensor]` table is the full [`SensorSpec`] the plugin provides; the
/// harness merges it into any `[[sensors]]` entry that references the plugin
/// by name. Parsing denies unknown fields at both levels, so a typo or a
/// future field fails closed instead of being ignored.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    /// Manifest schema version; must equal [`PLUGIN_SCHEMA_VERSION`].
    #[serde(rename = "schema-version")]
    pub schema_version: u32,
    /// Plugin package version (independent of the schema version).
    pub version: String,
    /// Human-readable description of what the sensor gate checks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The sensor definition the plugin provides.
    pub sensor: SensorSpec,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"
schema-version = 1
version = "1.2.0"
description = "Ruff lint gate"

[sensor]
name = "ruff"
argv = ["ruff", "check", "."]
when-changed = ["**/*.py"]
inputs = ["**/*.py", "pyproject.toml"]
severity = "error"
"#;

    fn parse(text: &str) -> Result<PluginManifest, toml::de::Error> {
        toml::from_str(text)
    }

    #[test]
    fn manifest_parses_with_nested_sensor_table() {
        let manifest = parse(MANIFEST).unwrap();
        assert_eq!(manifest.schema_version, PLUGIN_SCHEMA_VERSION);
        assert_eq!(manifest.version, "1.2.0");
        assert_eq!(manifest.sensor.name, "ruff");
        assert_eq!(manifest.sensor.argv, vec!["ruff", "check", "."]);
        assert_eq!(manifest.sensor.when_changed, vec!["**/*.py"]);
        assert_eq!(manifest.sensor.inputs, vec!["**/*.py", "pyproject.toml"]);
        assert_eq!(manifest.sensor.severity, Some(SensorSeverity::Error));
        assert_eq!(manifest.sensor.plugin, None);
    }

    #[test]
    fn deny_unknown_fields_fails_closed() {
        let bad = MANIFEST.replace(
            "severity = \"error\"",
            "typo-field = true\nseverity = \"error\"",
        );
        assert!(parse(&bad).is_err(), "unknown manifest key must fail");
        let bad_sensor = MANIFEST.replace(
            "severity = \"error\"",
            "severity = \"error\"\nunknown-key = 1",
        );
        assert!(
            parse(&bad_sensor).is_err(),
            "unknown [sensor] key must fail"
        );
    }

    #[test]
    fn manifest_plugin_reference_is_harness_rejected() {
        // `plugin` is a reserved field on `SensorSpec` for the harness's
        // resolution bookkeeping; parsing accepts it (it is a valid sensor
        // table), and discovery validation rejects it fail-closed.
        let bad = MANIFEST.replace("name = \"ruff\"", "name = \"ruff\"\nplugin = \"ruff\"");
        let manifest = parse(&bad).unwrap();
        assert_eq!(manifest.sensor.plugin.as_deref(), Some("ruff"));
    }

    #[test]
    fn kebab_case_keys_round_trip() {
        let manifest = parse(MANIFEST).unwrap();
        let rendered = toml::to_string(&manifest).unwrap();
        assert!(rendered.contains("schema-version = 1"));
        assert!(rendered.contains("[sensor]"));
        assert!(rendered.contains("when-changed ="));
        // Round-trip is stable: re-parsing the rendered text yields the same
        // definition.
        let reparsed: PluginManifest = toml::from_str(&rendered).unwrap();
        assert_eq!(reparsed.sensor.name, manifest.sensor.name);
        assert_eq!(reparsed.sensor.argv, manifest.sensor.argv);
        assert_eq!(reparsed.sensor.when_changed, manifest.sensor.when_changed);
        assert_eq!(reparsed.sensor.inputs, manifest.sensor.inputs);
        assert_eq!(reparsed.sensor.severity, manifest.sensor.severity);
        assert_eq!(reparsed.sensor.plugin, manifest.sensor.plugin);
    }
}

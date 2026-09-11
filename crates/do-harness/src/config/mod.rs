//! Parsed `do-harness.toml` configuration and the built-in sensor pack.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Parsed `do-harness.toml` configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Optional host language tag (informational; reserved for language packs).
    pub language: Option<String>,
    /// Hook sensor selection.
    #[serde(default)]
    pub hooks: HooksConfig,
    /// Named development signal sets: each maps a decision name (e.g.
    /// `feedback`, `verification`, `release`) to the ordered sensor names
    /// that provide evidence for it. Sets refer to sensors; they never
    /// contain command lines.
    #[serde(default, rename = "signal-sets")]
    pub signal_sets: BTreeMap<String, Vec<String>>,
    /// Ordered computational sensors; empty means the built-in Rust pack.
    #[serde(default)]
    pub sensors: Vec<SensorSpec>,
}

/// Which sensors each workflow gate runs.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HooksConfig {
    /// Sensors for pre-commit; empty = full suite.
    #[serde(default, rename = "pre-commit")]
    pub pre_commit: Vec<String>,
    /// Sensors for pre-push; empty = full suite.
    #[serde(default, rename = "pre-push")]
    pub pre_push: Vec<String>,
}

/// A single computational sensor.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SensorSpec {
    /// Unique sensor name (e.g. "fmt").
    pub name: String,
    /// Command line, program first (e.g. `["cargo", "fmt", "--all", "--", "--check"]`).
    pub argv: Vec<String>,
    /// Optional number of retry attempts on failure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<u32>,
    /// Optional process execution timeout budget in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    /// Whether failure of this sensor is advisory/warn-only and should not fail the verify gate.
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
}

/// Language pack identifiers accepted in `Config.language`.
pub const SUPPORTED_LANGUAGES: &[&str] = &["rust", "generic"];

/// The built-in Rust sensor pack, in canonical order.
static RUST_SENSORS: std::sync::LazyLock<Vec<SensorSpec>> = std::sync::LazyLock::new(rust_pack);

/// Builds a built-in sensor spec with the pack-default execution policy
/// (no retries, no timeout, mandatory pass).
fn spec(name: &str, argv: &[&str]) -> SensorSpec {
    SensorSpec {
        name: name.to_owned(),
        argv: argv.iter().map(|arg| (*arg).to_owned()).collect(),
        retry: None,
        timeout: None,
        allow_failure: false,
        transient_exit_codes: Vec::new(),
        when_changed: Vec::new(),
    }
}

/// Builds the built-in Rust sensor pack.
pub fn rust_pack() -> Vec<SensorSpec> {
    vec![
        spec("fmt", &["cargo", "fmt", "--all", "--", "--check"]),
        spec("check", &["cargo", "check", "--workspace"]),
        spec(
            "clippy",
            &["cargo", "clippy", "--workspace", "--", "-D", "warnings"],
        ),
        spec("test", &["cargo", "test", "--workspace"]),
        spec("loc", &["bash", "scripts/check-loc.sh"]),
        spec("deps", &["bash", "scripts/check-deps.sh"]),
        spec("audit", &["bash", "scripts/check-audit.sh"]),
        spec("commitlint", &["bash", "scripts/check-commitlint.sh"]),
    ]
}

/// Loads configuration from `explicit` or from `<root>/do-harness.toml`.
///
/// A missing default file yields the built-in Rust pack; a missing explicit
/// file is an error.
///
/// # Errors
///
/// Returns an error when the selected file cannot be read or parsed.
pub async fn load(root: &Path, explicit: Option<&Path>) -> Result<Config> {
    Ok(load_raw(root, explicit).await?.0)
}

/// Loads configuration plus the raw file bytes for policy fingerprinting.
///
/// The bytes are `None` when no file exists and the built-in Rust pack is
/// used; otherwise they are the exact bytes parsed into the config.
///
/// # Errors
///
/// Returns an error when the selected file cannot be read or parsed.
pub async fn load_raw(root: &Path, explicit: Option<&Path>) -> Result<(Config, Option<Vec<u8>>)> {
    let path = match explicit {
        Some(path) => path.to_path_buf(),
        None => root.join("do-harness.toml"),
    };
    if !path.exists() {
        if explicit.is_some() {
            anyhow::bail!("config file does not exist: {}", path.display());
        }
        return Ok((rust_default(), None));
    }
    let text = tokio::fs::read_to_string(&path)
        .await
        .with_context(|| format!("failed to read config file {}", path.display()))?;
    let cfg: Config =
        toml::from_str(&text).with_context(|| format!("invalid config file {}", path.display()))?;
    cfg.validate()?;
    Ok((cfg, Some(text.into_bytes())))
}

impl Config {
    /// Rejects unsupported language pack identifiers, malformed signal-set
    /// names, dangling sensor references, and duplicate entries in sets.
    fn validate(&self) -> Result<()> {
        if let Some(language) = &self.language {
            if !SUPPORTED_LANGUAGES.contains(&language.as_str()) {
                anyhow::bail!(
                    "unsupported language pack '{language}' (supported: {})",
                    SUPPORTED_LANGUAGES.join(", ")
                );
            }
        }
        let known: Vec<&str> = self
            .effective_sensors()
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        for sensor in &self.sensors {
            validate_globs(&sensor.name, &sensor.when_changed)?;
        }
        for (set, names) in &self.signal_sets {
            if !is_valid_set_name(set) {
                anyhow::bail!(
                    "invalid signal set name '{set}' (use letters, digits, '.', '_' or '-')"
                );
            }
            let mut seen = std::collections::HashSet::new();
            for name in names {
                if !seen.insert(name) {
                    anyhow::bail!("duplicate sensor '{name}' in signal set '{set}'");
                }
                if !known.contains(&name.as_str()) {
                    anyhow::bail!(
                        "signal set '{set}' references unknown sensor '{name}' (available: {})",
                        known.join(", ")
                    );
                }
            }
        }
        Ok(())
    }
}

/// Whether a `[signal-sets]` key is a safe set name and evidence-file stem.
fn is_valid_set_name(set: &str) -> bool {
    !set.is_empty()
        && set
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
}

/// Rejects `when-changed` patterns that cannot compile, so a typo fails at
/// config load instead of silently changing applicability at verify time.
fn validate_globs(sensor: &str, patterns: &[String]) -> Result<()> {
    if patterns.is_empty() {
        return Ok(());
    }
    let mut builder = globset::GlobSetBuilder::new();
    for pattern in patterns {
        let glob = globset::GlobBuilder::new(pattern)
            .literal_separator(true)
            .build()
            .with_context(|| {
                format!("sensor '{sensor}' has an invalid when-changed pattern '{pattern}'")
            })?;
        builder.add(glob);
    }
    builder
        .build()
        .with_context(|| format!("sensor '{sensor}' has unusable when-changed patterns"))?;
    Ok(())
}

/// Returns the built-in Rust configuration with the seven-pack of sensors.
pub fn rust_default() -> Config {
    let sensors = RUST_SENSORS.to_vec();
    let names: Vec<String> = sensors.iter().map(|s| s.name.clone()).collect();
    let mut signal_sets = BTreeMap::new();
    signal_sets.insert(
        "feedback".to_owned(),
        vec!["fmt".to_owned(), "check".to_owned(), "clippy".to_owned()],
    );
    signal_sets.insert("verification".to_owned(), names.clone());
    signal_sets.insert("release".to_owned(), names);
    Config {
        language: None,
        hooks: HooksConfig {
            pre_commit: vec!["fmt".to_owned(), "loc".to_owned()],
            pre_push: vec![],
        },
        signal_sets,
        sensors,
    }
}

impl Config {
    /// Returns the effective sensor list: configured, the generic pack's
    /// empty list, or the built-in Rust pack.
    pub fn effective_sensors(&self) -> &[SensorSpec] {
        if !self.sensors.is_empty() {
            return &self.sensors;
        }
        if self.language.as_deref() == Some("generic") {
            return &[];
        }
        &RUST_SENSORS
    }

    /// Names of the effective sensors, in order.
    pub fn sensor_names(&self) -> Vec<String> {
        self.effective_sensors()
            .iter()
            .map(|s| s.name.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests;

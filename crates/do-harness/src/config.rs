//! Parsed `do-harness.toml` configuration and the built-in sensor pack.

use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

/// Parsed `do-harness.toml` configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Optional host language tag (informational; reserved for language packs).
    #[allow(dead_code)]
    pub language: Option<String>,
    /// Hook sensor selection.
    #[serde(default)]
    pub hooks: HooksConfig,
    /// Ordered computational sensors; empty means the built-in Rust pack.
    #[serde(default)]
    pub sensors: Vec<SensorSpec>,
}

/// Which sensors each workflow gate runs.
#[derive(Debug, Clone, Default, Deserialize)]
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
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SensorSpec {
    /// Unique sensor name (e.g. "fmt").
    pub name: String,
    /// Command line, program first (e.g. `["cargo", "fmt", "--all", "--", "--check"]`).
    pub argv: Vec<String>,
    /// Optional number of retry attempts on failure.
    #[serde(default)]
    pub retry: Option<u32>,
    /// Optional process execution timeout budget in seconds.
    #[serde(default)]
    pub timeout: Option<u64>,
    /// Whether failure of this sensor is advisory/warn-only and should not fail the verify gate.
    #[serde(default, rename = "allow_failure")]
    pub allow_failure: bool,
    /// Reserved exit codes that signal transient failures eligible for retry/warn handling.
    #[serde(default, rename = "transient_exit_codes")]
    pub transient_exit_codes: Vec<i32>,
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
    }
}

/// Builds the built-in Rust sensor pack.
fn rust_pack() -> Vec<SensorSpec> {
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
pub fn load(root: &Path, explicit: Option<&Path>) -> Result<Config> {
    let path = match explicit {
        Some(path) => path.to_path_buf(),
        None => root.join("do-harness.toml"),
    };
    if !path.exists() {
        if explicit.is_some() {
            anyhow::bail!("config file does not exist: {}", path.display());
        }
        return Ok(rust_default());
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read config file {}", path.display()))?;
    let cfg: Config =
        toml::from_str(&text).with_context(|| format!("invalid config file {}", path.display()))?;
    cfg.validate()?;
    Ok(cfg)
}

impl Config {
    /// Rejects unsupported language pack identifiers and unknown sensor references in hooks.
    fn validate(&self) -> Result<()> {
        if let Some(language) = &self.language {
            if !SUPPORTED_LANGUAGES.contains(&language.as_str()) {
                anyhow::bail!(
                    "unsupported language pack '{language}' (supported: {})",
                    SUPPORTED_LANGUAGES.join(", ")
                );
            }
        }
        let available = self.sensor_names();
        for name in self.hooks.pre_commit.iter().chain(self.hooks.pre_push.iter()) {
            if !available.contains(name) {
                anyhow::bail!(
                    "do-harness.toml references unknown sensor '{name}'; fix the config or register the sensor (fail-closed)"
                );
            }
        }
        Ok(())
    }
}

/// Returns the built-in Rust configuration with the seven-pack of sensors.
pub fn rust_default() -> Config {
    Config {
        language: None,
        hooks: HooksConfig {
            pre_commit: vec!["fmt".to_owned(), "loc".to_owned()],
            pre_push: vec![],
        },
        sensors: RUST_SENSORS.to_vec(),
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
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// Writes a config file into a tempdir and loads it explicitly.
    #[test]
    fn parses_valid_config() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("do-harness.toml");
        let text = r#"
            language = "rust"
            [hooks]
            pre-commit = ["fmt", "check"]
            pre-push = []
            [[sensors]]
            name = "fmt"
            argv = ["cargo", "fmt", "--all", "--", "--check"]
            [[sensors]]
            name = "check"
            argv = ["cargo", "check", "--workspace"]
        "#;
        std::fs::write(&path, text).expect("write config");
        let cfg = load(dir.path(), Some(&path)).expect("load config");
        assert_eq!(cfg.language.as_deref(), Some("rust"));
        assert_eq!(
            cfg.hooks.pre_commit,
            vec!["fmt".to_owned(), "check".to_owned()]
        );
        assert!(cfg.hooks.pre_push.is_empty());
        assert_eq!(cfg.sensors.len(), 2);
        assert_eq!(
            cfg.sensors[0].argv,
            vec!["cargo", "fmt", "--all", "--", "--check"]
        );
    }

    /// Unknown top-level keys are rejected by `deny_unknown_fields`.
    #[test]
    fn rejects_unknown_fields() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("do-harness.toml");
        std::fs::write(&path, "bogus_key = 1\n").expect("write config");
        let err = load(dir.path(), Some(&path)).expect_err("load must fail");
        assert!(format!("{err:#}").contains("bogus_key"));
    }

    /// Parses new per-sensor fields: retry, timeout, `allow_failure`, `transient_exit_codes`.
    #[test]
    fn parses_transient_failure_sensor_options() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("do-harness.toml");
        let text = r#"
            [[sensors]]
            name = "links"
            argv = ["bash", "check-links.sh"]
            retry = 3
            timeout = 10
            allow_failure = true
            transient_exit_codes = [75, 429]
        "#;
        std::fs::write(&path, text).expect("write config");
        let cfg = load(dir.path(), Some(&path)).expect("load config");
        assert_eq!(cfg.sensors.len(), 1);
        let sensor = &cfg.sensors[0];
        assert_eq!(sensor.name, "links");
        assert_eq!(sensor.retry, Some(3));
        assert_eq!(sensor.timeout, Some(10));
        assert!(sensor.allow_failure);
        assert_eq!(sensor.transient_exit_codes, vec![75, 429]);
    }

    /// Unknown fields in [[sensors]] are rejected by `deny_unknown_fields`.
    #[test]
    fn rejects_unknown_sensor_fields() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("do-harness.toml");
        let text = r#"
            [[sensors]]
            name = "fmt"
            argv = ["cargo", "fmt"]
            unknown_sensor_option = true
        "#;
        std::fs::write(&path, text).expect("write config");
        let err = load(dir.path(), Some(&path)).expect_err("load must fail");
        assert!(format!("{err:#}").contains("unknown_sensor_option"));
    }

    /// A missing default config falls back to the built-in Rust pack.
    #[test]
    fn missing_file_returns_rust_default() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = load(dir.path(), None).expect("load default");
        assert_eq!(cfg.language, None);
        assert_eq!(
            cfg.hooks.pre_commit,
            vec!["fmt".to_owned(), "loc".to_owned()]
        );
        assert!(cfg.hooks.pre_push.is_empty());
        assert_eq!(cfg.sensors.len(), 7);
        assert_eq!(cfg.sensor_names().len(), 7);
        assert_eq!(cfg.effective_sensors().len(), 7);
    }

    /// An unknown language pack identifier is rejected at load time.
    #[test]
    fn rejects_unknown_language_pack() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("do-harness.toml");
        std::fs::write(&path, "language = \"python\"\n").expect("write config");
        let err = load(dir.path(), Some(&path)).expect_err("load must fail");
        assert!(format!("{err:#}").contains("unsupported language pack 'python'"));
    }

    /// The generic pack with no sensors yields an empty sensor list.
    #[test]
    fn generic_language_yields_no_effective_sensors() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("do-harness.toml");
        std::fs::write(&path, "language = \"generic\"\n").expect("write config");
        let cfg = load(dir.path(), Some(&path)).expect("load config");
        assert!(cfg.effective_sensors().is_empty());
        assert!(cfg.sensor_names().is_empty());
    }

    /// The rust pack with no sensors falls back to the built-in Rust sensors.
    #[test]
    fn rust_language_without_sensors_uses_builtin_pack() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("do-harness.toml");
        std::fs::write(&path, "language = \"rust\"\n").expect("write config");
        let cfg = load(dir.path(), Some(&path)).expect("load config");
        assert_eq!(cfg.effective_sensors().len(), 7);
        assert!(cfg.sensor_names().contains(&"clippy".to_owned()));
    }

    /// Sensor names referenced in hooks that are not in the effective sensor pack fail at load time.
    #[test]
    fn rejects_unknown_sensor_references_in_hooks() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("do-harness.toml");
        let text = r#"
            [hooks]
            pre-commit = ["fmt", "unknown_sensor"]
        "#;
        std::fs::write(&path, text).expect("write config");
        let err = load(dir.path(), Some(&path)).expect_err("load must fail");
        let msg = format!("{err:#}");
        assert!(msg.contains("references unknown sensor 'unknown_sensor'"));
        assert!(msg.contains("fail-closed"));
    }
}

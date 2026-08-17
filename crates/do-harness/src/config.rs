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
}

/// The built-in Rust sensor pack, in canonical order.
static RUST_SENSORS: std::sync::LazyLock<Vec<SensorSpec>> = std::sync::LazyLock::new(rust_pack);

/// Builds the built-in Rust sensor pack.
fn rust_pack() -> Vec<SensorSpec> {
    vec![
        SensorSpec {
            name: "fmt".to_owned(),
            argv: vec![
                "cargo".to_owned(),
                "fmt".to_owned(),
                "--all".to_owned(),
                "--".to_owned(),
                "--check".to_owned(),
            ],
        },
        SensorSpec {
            name: "check".to_owned(),
            argv: vec![
                "cargo".to_owned(),
                "check".to_owned(),
                "--workspace".to_owned(),
            ],
        },
        SensorSpec {
            name: "clippy".to_owned(),
            argv: vec![
                "cargo".to_owned(),
                "clippy".to_owned(),
                "--workspace".to_owned(),
                "--".to_owned(),
                "-D".to_owned(),
                "warnings".to_owned(),
            ],
        },
        SensorSpec {
            name: "test".to_owned(),
            argv: vec![
                "cargo".to_owned(),
                "test".to_owned(),
                "--workspace".to_owned(),
            ],
        },
        SensorSpec {
            name: "loc".to_owned(),
            argv: vec!["bash".to_owned(), "scripts/check-loc.sh".to_owned()],
        },
        SensorSpec {
            name: "deps".to_owned(),
            argv: vec!["bash".to_owned(), "scripts/check-deps.sh".to_owned()],
        },
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
    toml::from_str(&text).with_context(|| format!("invalid config file {}", path.display()))
}

/// Returns the built-in Rust configuration with the six-pack of sensors.
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
    /// Returns the effective sensor list: configured, or the built-in pack.
    pub fn effective_sensors(&self) -> &[SensorSpec] {
        if self.sensors.is_empty() {
            &RUST_SENSORS
        } else {
            &self.sensors
        }
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
    use super::*;

    /// Writes a config file into a tempdir and loads it explicitly.
    #[test]
    fn parses_valid_config() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("do-harness.toml");
        let text = r#"
            language = "rust"
            [hooks]
            pre-commit = ["fmt", "loc"]
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
            vec!["fmt".to_owned(), "loc".to_owned()]
        );
        assert!(cfg.hooks.pre_push.is_empty());
        assert_eq!(cfg.sensors.len(), 2);
        assert_eq!(
            cfg.sensors[0].argv,
            vec!["cargo", "fmt", "--all", "--", "--check"]
        );
    }

    /// Unknown top-level keys are rejected by deny_unknown_fields.
    #[test]
    fn rejects_unknown_fields() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("do-harness.toml");
        std::fs::write(&path, "bogus_key = 1\n").expect("write config");
        let err = load(dir.path(), Some(&path)).expect_err("load must fail");
        assert!(format!("{err:#}").contains("bogus_key"));
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
        assert_eq!(cfg.sensors.len(), 6);
        assert_eq!(cfg.sensor_names().len(), 6);
        assert_eq!(cfg.effective_sensors().len(), 6);
    }
}

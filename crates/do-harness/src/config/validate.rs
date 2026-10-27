//! Configuration validation and resource policy migration guards.
use super::{Config, SUPPORTED_LANGUAGES, SensorSeverity};
use anyhow::{Context, Result};

impl Config {
    /// Rejects unsupported language pack identifiers, malformed signal-set
    /// names, dangling sensor references, duplicate entries in sets, and a
    /// zero `jobs` worker bound.
    pub(crate) fn validate(&self) -> Result<()> {
        if let Some(jobs) = self.jobs {
            if jobs == 0 {
                anyhow::bail!("jobs must be at least 1 (got 0)");
            }
        }
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
            if sensor.timeout == Some(0)
                && sensor
                    .timeout_justification
                    .as_deref()
                    .is_none_or(|reason| reason.trim().is_empty())
            {
                anyhow::bail!(
                    "sensor '{}' timeout=0 requires timeout-justification",
                    sensor.name
                );
            }
            if let Some(bytes) = sensor.max_output_bytes {
                if !(super::MIN_OUTPUT_BYTES..=super::MAX_OUTPUT_BYTES).contains(&bytes) {
                    anyhow::bail!(
                        "sensor '{}' max-output-bytes must be between {} and {}",
                        sensor.name,
                        super::MIN_OUTPUT_BYTES,
                        super::MAX_OUTPUT_BYTES
                    );
                }
            }
            for key in sensor
                .redact_env
                .iter()
                .chain(sensor.environment_allowlist.iter().flatten())
                .chain(&sensor.environment_inputs)
            {
                if key.is_empty()
                    || !key.bytes().enumerate().all(|(i, b)| {
                        b == b'_' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit())
                    })
                {
                    anyhow::bail!(
                        "sensor '{}' has an invalid environment policy key",
                        sensor.name
                    );
                }
            }
            validate_globs(&sensor.name, "when-changed", &sensor.when_changed)?;
            validate_globs(&sensor.name, "artifacts", &sensor.artifacts)?;
            validate_globs(&sensor.name, "coverage-inputs", &sensor.coverage_inputs)?;
            validate_globs(&sensor.name, "inputs", &sensor.inputs)?;
            if sensor.allow_failure && sensor.severity == Some(SensorSeverity::Error) {
                anyhow::bail!(
                    "sensor '{}' sets both allow_failure = true and severity = \"error\"; \
                     use one severity source",
                    sensor.name
                );
            }
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
        validate_hook_references("hooks.pre-commit", &self.hooks.pre_commit, &known)?;
        validate_hook_references("hooks.pre-push", &self.hooks.pre_push, &known)?;
        Ok(())
    }
}

/// Validates that every non-empty hook sensor selection references a sensor
/// present in the effective sensor pack.
fn validate_hook_references(hook_key: &str, names: &[String], known: &[&str]) -> Result<()> {
    for name in names {
        if !known.contains(&name.as_str()) {
            anyhow::bail!(
                "{hook_key} references unknown sensor '{name}' (available: {})",
                known.join(", ")
            );
        }
    }
    Ok(())
}

/// Whether a `[signal-sets]` key is a safe set name and evidence-file stem.
fn is_valid_set_name(set: &str) -> bool {
    !set.is_empty()
        && set
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
}

/// Rejects glob patterns that cannot compile, so a typo fails at config load
/// instead of silently changing applicability or evidence at verify time.
fn validate_globs(sensor: &str, field: &str, patterns: &[String]) -> Result<()> {
    if patterns.is_empty() {
        return Ok(());
    }
    let mut builder = globset::GlobSetBuilder::new();
    for pattern in patterns {
        let path = std::path::Path::new(pattern);
        if path.is_absolute()
            || path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            anyhow::bail!("sensor '{sensor}' {field} patterns must stay workspace-relative");
        }
        let glob = globset::GlobBuilder::new(pattern)
            .literal_separator(true)
            .build()
            .with_context(|| {
                format!("sensor '{sensor}' has an invalid {field} pattern '{pattern}'")
            })?;
        builder.add(glob);
    }
    builder
        .build()
        .with_context(|| format!("sensor '{sensor}' has unusable {field} patterns"))?;
    Ok(())
}

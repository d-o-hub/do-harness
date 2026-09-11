//! Development signal sets: named sensor selections for a decision.
//!
//! A signal set answers "which group of commands provides evidence for this
//! development decision?" (`feedback`, `verification`, `release`). Sets refer
//! to sensors by name; sensor definitions stay the single source of truth in
//! `[[sensors]]`. Configs without `[signal-sets]` keep legacy behavior:
//! `verification` and `release` resolve to the effective sensor list, while
//! `feedback` has no implicit semantic guarantee and fails loudly.

use anyhow::{Result, anyhow};

use crate::config::{Config, SensorSpec};

/// A resolved signal set: the decision name and its sensor names in set order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSet {
    /// Signal-set name as requested (`--set`) or defaulted.
    pub name: String,
    /// Sensor names in set order (duplicates rejected at config load).
    pub sensors: Vec<String>,
}

/// Names usable without explicit `[signal-sets]` and what they resolve to.
const COMPAT_SETS: &[&str] = &["verification", "release"];

/// Names of the signal sets selectable for `cfg`.
///
/// Declared `[signal-sets]` keys when present; otherwise the compatibility
/// names `verification` and `release`.
#[must_use]
pub fn available(cfg: &Config) -> Vec<String> {
    if cfg.signal_sets.is_empty() {
        return COMPAT_SETS.iter().map(ToString::to_string).collect();
    }
    cfg.signal_sets.keys().cloned().collect()
}

/// Ordered sensor specs for `set` (`None` = the effective sensor list).
///
/// Shared by `verify`, `explain`, and `status` so every command resolves a
/// set identically.
///
/// # Errors
///
/// Returns an error when the set is unknown or references a missing sensor.
pub fn candidate_specs<'a>(cfg: &'a Config, set: Option<&str>) -> Result<Vec<&'a SensorSpec>> {
    let sensors = cfg.effective_sensors();
    let Some(name) = set else {
        return Ok(sensors.iter().collect());
    };
    let resolved = resolve(cfg, name)?;
    let mut specs = Vec::with_capacity(resolved.sensors.len());
    for sensor in &resolved.sensors {
        let Some(spec) = sensors.iter().find(|s| &s.name == sensor) else {
            return Err(anyhow!(
                "signal set '{name}' references unknown sensor '{sensor}'"
            ));
        };
        specs.push(spec);
    }
    Ok(specs)
}

/// Resolves `name` to its ordered sensor names.
///
/// Unknown sets fail loudly. `feedback` without explicit `[signal-sets]` has
/// no implicit semantic guarantee and is rejected; `verification` and
/// `release` without explicit sets resolve to the effective sensor list.
///
/// # Errors
///
/// Returns an error when the set is unknown, unconfigured, or (defensively)
/// references a sensor that no longer exists.
pub fn resolve(cfg: &Config, name: &str) -> Result<ResolvedSet> {
    if let Some(names) = cfg.signal_sets.get(name) {
        let known = cfg.sensor_names();
        for sensor in names {
            if !known.contains(sensor) {
                return Err(anyhow!(
                    "signal set '{name}' references unknown sensor '{sensor}' (available: {})",
                    known.join(", ")
                ));
            }
        }
        return Ok(ResolvedSet {
            name: name.to_owned(),
            sensors: names.clone(),
        });
    }
    if cfg.signal_sets.is_empty() && COMPAT_SETS.contains(&name) {
        return Ok(ResolvedSet {
            name: name.to_owned(),
            sensors: cfg.sensor_names(),
        });
    }
    if cfg.signal_sets.is_empty() && name == "feedback" {
        return Err(anyhow!(
            "signal set 'feedback' is not configured (no [signal-sets] in config); \
             define it explicitly or use one of: {}",
            available(cfg).join(", ")
        ));
    }
    Err(anyhow!(
        "unknown signal set '{name}' (available: {})",
        available(cfg).join(", ")
    ))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::config::SensorSpec;

    /// Config with two declared sets over two sensors.
    fn sets_config() -> Config {
        Config {
            language: None,
            hooks: crate::config::HooksConfig::default(),
            signal_sets: [
                ("feedback".to_owned(), vec!["a".to_owned()]),
                (
                    "verification".to_owned(),
                    vec!["a".to_owned(), "b".to_owned()],
                ),
            ]
            .into_iter()
            .collect(),
            sensors: ["a", "b"]
                .iter()
                .map(|name| SensorSpec {
                    name: (*name).to_owned(),
                    argv: vec!["true".to_owned()],
                    retry: None,
                    timeout: None,
                    allow_failure: false,
                    transient_exit_codes: Vec::new(),
                    when_changed: Vec::new(),
                })
                .collect(),
        }
    }

    /// Declared sets resolve in declaration order with their sensor order.
    #[test]
    fn resolves_declared_sets() {
        let cfg = sets_config();
        let set = resolve(&cfg, "feedback").unwrap();
        assert_eq!(set.sensors, vec!["a".to_owned()]);
        let set = resolve(&cfg, "verification").unwrap();
        assert_eq!(set.sensors, vec!["a".to_owned(), "b".to_owned()]);
        assert_eq!(available(&cfg), vec!["feedback", "verification"]);
    }

    /// Unknown set names fail loudly and list what exists.
    #[test]
    fn unknown_set_lists_available() {
        let cfg = sets_config();
        let err = resolve(&cfg, "release").unwrap_err();
        let text = format!("{err:#}");
        assert!(text.contains("release"), "{text}");
        assert!(text.contains("feedback"), "{text}");
    }

    /// Without `[signal-sets]`, verification/release fall back to the full list.
    #[test]
    fn compat_sets_fall_back_to_full_list() {
        let mut cfg = sets_config();
        cfg.signal_sets.clear();
        let set = resolve(&cfg, "verification").unwrap();
        assert_eq!(set.sensors, vec!["a".to_owned(), "b".to_owned()]);
        let set = resolve(&cfg, "release").unwrap();
        assert_eq!(set.sensors, vec!["a".to_owned(), "b".to_owned()]);
        assert_eq!(available(&cfg), vec!["verification", "release"]);
    }

    /// Without `[signal-sets]`, feedback has no implicit guarantee: it fails.
    #[test]
    fn feedback_without_sets_fails_loudly() {
        let mut cfg = sets_config();
        cfg.signal_sets.clear();
        let err = resolve(&cfg, "feedback").unwrap_err();
        assert!(format!("{err:#}").contains("not configured"));
    }
}

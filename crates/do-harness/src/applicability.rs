//! Change-aware sensor selection with explanations.
//!
//! The harness — never the model — decides which sensors the current change
//! needs. Sensors without `when-changed` are always applicable. Selection is
//! deterministic: same config plus same changed files yields the same
//! selected/skipped split with the same reasons.

use serde::Serialize;

use crate::changes::ChangedFiles;
use crate::config::SensorSpec;

/// A sensor the current change requires, with its selection reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Selected {
    /// Sensor name as configured.
    pub name: String,
    /// Why it was selected (matching pattern, no constraint, fail-closed).
    pub reason: String,
}

/// A sensor the current change does not require, with its skip reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Skipped {
    /// Sensor name as configured.
    pub name: String,
    /// Why it was skipped (no configured path matched).
    pub reason: String,
}

/// Deterministic applicability split for one candidate sensor list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    /// Applicable sensors, in candidate order.
    pub selected: Vec<Selected>,
    /// Inapplicable sensors, in candidate order.
    pub skipped: Vec<Skipped>,
}

impl Selection {
    /// Names of the selected sensors, in candidate order.
    #[must_use]
    pub fn selected_names(&self) -> Vec<String> {
        self.selected.iter().map(|s| s.name.clone()).collect()
    }
}

/// Reason recorded when change discovery failed and every sensor was kept.
pub const FAIL_CLOSED_REASON: &str = "change discovery failed; selected fail-closed";

/// Reason recorded when selection runs without `--changed`.
pub const UNFILTERED_REASON: &str = "change filtering disabled; always applicable";

/// Splits `candidates` into applicable and inapplicable sensors.
///
/// Deleted paths still match patterns (removing a source file must trigger
/// its checks). When `changed.discovery_failed`, every candidate is selected
/// with [`FAIL_CLOSED_REASON`] so a required signal can never silently
/// disappear because repository state could not be read.
#[must_use]
pub fn select(candidates: &[&SensorSpec], changed: &ChangedFiles) -> Selection {
    let mut selection = Selection {
        selected: Vec::new(),
        skipped: Vec::new(),
    };
    if changed.discovery_failed {
        for spec in candidates {
            selection.selected.push(Selected {
                name: spec.name.clone(),
                reason: FAIL_CLOSED_REASON.to_owned(),
            });
        }
        return selection;
    }
    let matchers: Vec<(&SensorSpec, Vec<(String, globset::GlobMatcher)>)> = candidates
        .iter()
        .map(|spec| {
            let compiled = spec
                .when_changed
                .iter()
                .filter_map(|pattern| {
                    globset::GlobBuilder::new(pattern)
                        .literal_separator(true)
                        .build()
                        .ok()
                        .map(|glob| (pattern.clone(), glob.compile_matcher()))
                })
                .collect();
            (*spec, compiled)
        })
        .collect();
    for (spec, compiled) in matchers {
        if spec.when_changed.is_empty() {
            selection.selected.push(Selected {
                name: spec.name.clone(),
                reason: "no applicability constraint (`when-changed` unset)".to_owned(),
            });
            continue;
        }
        if compiled.len() != spec.when_changed.len() {
            // A pattern that compiled at config load must compile here; if
            // it does not, keep the sensor rather than dropping it.
            selection.selected.push(Selected {
                name: spec.name.clone(),
                reason: FAIL_CLOSED_REASON.to_owned(),
            });
            continue;
        }
        let paths: Vec<&str> = changed.paths();
        let hit = compiled
            .iter()
            .find(|(_, matcher)| paths.iter().any(|path| matcher.is_match(path)));
        match hit {
            Some((pattern, _)) => selection.selected.push(Selected {
                name: spec.name.clone(),
                reason: format!("matched `{pattern}`"),
            }),
            None => selection.skipped.push(Skipped {
                name: spec.name.clone(),
                reason: "no configured path matched".to_owned(),
            }),
        }
    }
    selection
}

#[cfg(test)]
#[path = "applicability_tests.rs"]
mod tests;

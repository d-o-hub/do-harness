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
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::changes::{ChangeKind, ChangedFile};

    /// Builds a sensor spec with the given applicability patterns.
    fn spec(name: &str, when_changed: &[&str]) -> SensorSpec {
        SensorSpec {
            name: name.to_owned(),
            argv: vec!["true".to_owned()],
            retry: None,
            timeout: None,
            allow_failure: false,
            transient_exit_codes: Vec::new(),
            when_changed: when_changed.iter().map(ToString::to_string).collect(),
        }
    }

    /// Builds a changed-files set from bare paths (all modified).
    fn changed(paths: &[&str]) -> ChangedFiles {
        ChangedFiles {
            files: paths
                .iter()
                .map(|path| ChangedFile {
                    path: (*path).to_owned(),
                    kind: ChangeKind::Modified,
                })
                .collect(),
            discovery_failed: false,
        }
    }

    /// Matching patterns select with the pattern named; others are skipped.
    #[test]
    fn select_splits_by_first_matching_pattern() {
        let rs = spec("rs-check", &["**/*.rs", "Cargo.toml"]);
        let docs = spec("docs", &["**/*.md"]);
        let candidates = vec![&rs, &docs];

        let selection = select(&candidates, &changed(&["src/lib.rs"]));
        assert_eq!(selection.selected_names(), vec!["rs-check".to_owned()]);
        assert_eq!(selection.selected[0].reason, "matched `**/*.rs`");
        assert_eq!(selection.skipped.len(), 1);
        assert_eq!(selection.skipped[0].name, "docs");
        assert!(!selection.skipped[0].reason.is_empty());
    }

    /// Sensors without `when-changed` are always applicable.
    #[test]
    fn unconstrained_sensors_always_apply() {
        let audit = spec("audit", &[]);
        let candidates = vec![&audit];

        let selection = select(&candidates, &changed(&["README.md"]));
        assert_eq!(selection.selected_names(), vec!["audit".to_owned()]);

        let empty = ChangedFiles::default();
        let selection = select(&candidates, &empty);
        assert_eq!(selection.selected_names(), vec!["audit".to_owned()]);
    }

    /// Deleted paths still trigger their sensors.
    #[test]
    fn deleted_paths_still_match() {
        let rs = spec("rs-check", &["**/*.rs"]);
        let candidates = vec![&rs];
        let gone = ChangedFiles {
            files: vec![ChangedFile {
                path: "old.rs".to_owned(),
                kind: ChangeKind::Deleted,
            }],
            discovery_failed: false,
        };

        let selection = select(&candidates, &gone);
        assert_eq!(selection.selected_names(), vec!["rs-check".to_owned()]);
    }

    /// Discovery failure selects every sensor with the fail-closed reason.
    #[test]
    fn discovery_failure_selects_everything() {
        let rs = spec("rs-check", &["**/*.rs"]);
        let docs = spec("docs", &["**/*.md"]);
        let candidates = vec![&rs, &docs];
        let failed = ChangedFiles {
            files: Vec::new(),
            discovery_failed: true,
        };

        let selection = select(&candidates, &failed);
        assert_eq!(
            selection.selected_names(),
            vec!["rs-check".to_owned(), "docs".to_owned()]
        );
        assert!(
            selection
                .selected
                .iter()
                .all(|s| s.reason == FAIL_CLOSED_REASON)
        );
        assert!(selection.skipped.is_empty());
    }

    /// Separator semantics are platform-independent: `*` never crosses `/`
    /// while `**` does (discovery always normalizes to `/` first).
    #[test]
    fn separator_semantics_are_platform_independent() {
        let shallow = spec("shallow", &["*.rs"]);
        let deep = spec("deep", &["src/**"]);
        let candidates = vec![&shallow, &deep];

        let selection = select(&candidates, &changed(&["src/lib.rs"]));
        assert_eq!(selection.selected_names(), vec!["deep".to_owned()]);
        assert_eq!(selection.skipped[0].name, "shallow");

        let selection = select(&candidates, &changed(&["top.rs"]));
        assert_eq!(selection.selected_names(), vec!["shallow".to_owned()]);
    }

    /// Matching is case-sensitive on every platform for determinism.
    #[test]
    fn matching_is_case_sensitive() {
        let rs = spec("rs-check", &["**/*.RS"]);
        let candidates = vec![&rs];

        let selection = select(&candidates, &changed(&["src/lib.rs"]));
        assert!(selection.selected.is_empty());
        assert_eq!(selection.skipped.len(), 1);
    }
}

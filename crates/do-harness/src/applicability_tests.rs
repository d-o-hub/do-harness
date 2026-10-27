#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::changes::{ChangeKind, ChangedFile};
use crate::config::Config;
use std::path::Path;

/// Builds a sensor spec with the given applicability patterns.
fn spec(name: &str, when_changed: &[&str]) -> SensorSpec {
    SensorSpec {
        kind: None,
        fix: None,
        name: name.to_owned(),
        argv: vec!["true".to_owned()],
        retry: None,
        timeout_justification: None,
        max_output_bytes: None,
        environment_allowlist: None,
        redact_env: Vec::new(),
        timeout: None,
        severity: None,
        allow_failure: false,
        transient_exit_codes: Vec::new(),
        artifacts: Vec::new(),
        coverage_inputs: Vec::new(),
        inputs: Vec::new(),
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
    assert_ne!(selection.skipped[0].reason.len(), 0);
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
    assert_eq!(selection.skipped.len(), 0);
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
    assert_eq!(selection.selected.len(), 0);
    assert_eq!(selection.skipped.len(), 1);
}

/// Every sensor from the repository's own `do-harness.toml`, so the docs
/// selection proof is driven by the shipped applicability lists rather
/// than a copy that could drift from what CI actually runs.
fn repo_sensors() -> Vec<SensorSpec> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let text = std::fs::read_to_string(root.join("do-harness.toml"))
        .expect("the repository do-harness.toml must be readable");
    let config: Config = toml::from_str(&text).expect("do-harness.toml must parse");
    config.sensors
}

/// The docs contract sensors, in shipped configuration order.
fn docs_sensors() -> Vec<SensorSpec> {
    let sensors = repo_sensors();
    ["docs-sync", "docs-examples", "docs-links"]
        .iter()
        .map(|name| {
            sensors
                .iter()
                .find(|sensor| sensor.name == *name)
                .unwrap_or_else(|| panic!("do-harness.toml must define a `{name}` sensor"))
                .clone()
        })
        .collect()
}

/// Runs selection over the docs contract sensors for `changed`, returning
/// the selected and skipped names so a dropped `when-changed` glob cannot
/// pass silently.
fn select_docs(changed: &ChangedFiles) -> (Vec<String>, Vec<String>) {
    let sensors = docs_sensors();
    let candidates: Vec<&SensorSpec> = sensors.iter().collect();
    let selection = select(&candidates, changed);
    (
        selection.selected_names(),
        selection.skipped.iter().map(|s| s.name.clone()).collect(),
    )
}

/// A maintained Markdown edit is owned by both the generated-block
/// synchroniser and the link checker, so it must select `docs-sync` and
/// `docs-links`; dropping either Markdown glob from the shipped
/// applicability lists fails here.
#[test]
fn docs_only_change_selects_sync_and_links() {
    let (selected, skipped) = select_docs(&changed(&["README.md"]));
    assert!(
        selected.iter().any(|name| name == "docs-sync"),
        "a maintained Markdown edit must select docs-sync \
         (selected {selected:?}, skipped {skipped:?})"
    );
    assert!(
        selected.iter().any(|name| name == "docs-links"),
        "a maintained Markdown edit must select docs-links \
         (selected {selected:?}, skipped {skipped:?})"
    );
}

/// A CLI/source metadata edit regenerates the mechanical blocks, so it
/// must select `docs-sync` — the docs check that owns them — rather than
/// vacuously skipping the contract.
#[test]
fn source_metadata_change_selects_docs_sync() {
    let (selected, skipped) = select_docs(&changed(&["crates/do-harness/src/cli.rs"]));
    assert!(
        selected.iter().any(|name| name == "docs-sync"),
        "a CLI source edit must select docs-sync \
         (selected {selected:?}, skipped {skipped:?})"
    );
}

/// A renamed maintained document keeps its link obligations, so its new
/// path must select `docs-links`.
#[test]
fn renamed_maintained_doc_selects_docs_links() {
    let renamed = ChangedFiles {
        files: vec![ChangedFile {
            path: "docs/cli-reference.md".to_owned(),
            kind: ChangeKind::Renamed {
                from: "docs/cli.md".to_owned(),
            },
        }],
        discovery_failed: false,
    };

    let (selected, skipped) = select_docs(&renamed);
    assert!(
        selected.iter().any(|name| name == "docs-links"),
        "a renamed maintained document must select docs-links \
         (selected {selected:?}, skipped {skipped:?})"
    );
}

/// A deleted maintained document still owns its inbound links, so the
/// deletion must select `docs-links` — deletions are never skipped.
#[test]
fn deleted_maintained_doc_selects_docs_links() {
    let deleted = ChangedFiles {
        files: vec![ChangedFile {
            path: "docs/adoption.md".to_owned(),
            kind: ChangeKind::Deleted,
        }],
        discovery_failed: false,
    };

    let (selected, skipped) = select_docs(&deleted);
    assert!(
        selected.iter().any(|name| name == "docs-links"),
        "a deleted maintained document must select docs-links \
         (selected {selected:?}, skipped {skipped:?})"
    );
}

/// The three docs contracts replace the legacy shell link checker: exactly
/// one sensor owns Markdown links, so `markdown-links` must be gone.
#[test]
fn legacy_markdown_links_sensor_is_gone() {
    let legacy: Vec<String> = repo_sensors()
        .iter()
        .filter(|sensor| sensor.name == "markdown-links")
        .map(|sensor| sensor.name.clone())
        .collect();
    assert!(
        legacy.is_empty(),
        "docs-links supersedes the legacy link checker; remove `markdown-links`: {legacy:?}"
    );
}

/// The Rust sensors that compile, test, and lint the workspace.
const RUST_SENSORS: [&str; 6] = ["fmt", "check", "clippy", "test", "doctest", "coverage"];

/// The Rust sensors from the built-in pack, in pack order.
fn pack_rust_sensors() -> Vec<SensorSpec> {
    crate::config::rust_pack()
        .into_iter()
        .filter(|spec| RUST_SENSORS.contains(&spec.name.as_str()))
        .collect()
}

/// The Rust sensors from the repository's own `do-harness.toml`, in
/// shipped configuration order.
fn policy_rust_sensors() -> Vec<SensorSpec> {
    repo_sensors()
        .into_iter()
        .filter(|spec| RUST_SENSORS.contains(&spec.name.as_str()))
        .collect()
}

/// A changed-files set holding a single path, of the given kind.
fn single(path: &str, kind: ChangeKind) -> ChangedFiles {
    ChangedFiles {
        files: vec![ChangedFile {
            path: path.to_owned(),
            kind,
        }],
        discovery_failed: false,
    }
}

/// Asserts that every Rust sensor is selected for `changed`, naming the
/// source of the sensors in the failure so a broken declaration and a
/// broken selection are not confused.
fn assert_all_rust_sensors_selected(sensors: &[SensorSpec], changed: &ChangedFiles) {
    let candidates: Vec<&SensorSpec> = sensors.iter().collect();
    let selection = select(&candidates, changed);
    for name in RUST_SENSORS {
        assert!(
            selection.selected.iter().any(|s| s.name == name),
            "`{name}` must be selected for {:?} (selected {:?}, skipped {:?})",
            changed.paths(),
            selection.selected_names(),
            selection
                .skipped
                .iter()
                .map(|s| s.name.clone())
                .collect::<Vec<_>>(),
        );
    }
}

/// A member manifest is a compilation input: an edit to one must select
/// the Rust sensors that compile, test, and lint the workspace.
///
/// The cases cover the workspace root, a member under `crates/`, and a
/// member outside it, because a recursive glob that only reaches one of
/// those is the defect this guards.
#[test]
fn member_manifest_change_selects_the_rust_sensors() {
    let cases = [
        "Cargo.toml",
        "crates/db/Cargo.toml",
        "tools/tooling/Cargo.toml",
    ];
    for sensors in [pack_rust_sensors(), policy_rust_sensors()] {
        for path in cases {
            assert_all_rust_sensors_selected(&sensors, &single(path, ChangeKind::Modified));
        }
    }
}

/// A deleted member manifest still selects the Rust sensors: removing a
/// member is as much a compilation change as editing one.
#[test]
fn deleted_member_manifest_selects_the_rust_sensors() {
    for sensors in [pack_rust_sensors(), policy_rust_sensors()] {
        assert_all_rust_sensors_selected(
            &sensors,
            &single("crates/db/Cargo.toml", ChangeKind::Deleted),
        );
    }
}

/// A Markdown-only edit still skips the Rust sensors: the correction must
/// not make every change select the whole Rust toolchain.
#[test]
fn markdown_only_change_skips_the_rust_sensors() {
    for sensors in [pack_rust_sensors(), policy_rust_sensors()] {
        let candidates: Vec<&SensorSpec> = sensors.iter().collect();
        let selection = select(&candidates, &single("README.md", ChangeKind::Modified));
        assert!(
            selection.selected.is_empty(),
            "a Markdown-only edit must skip the Rust sensors (selected {:?})",
            selection.selected_names(),
        );
    }
}

/// A discovery failure keeps every candidate, with the fail-closed reason.
#[test]
fn discovery_failure_keeps_the_rust_sensors() {
    let failed = ChangedFiles {
        files: Vec::new(),
        discovery_failed: true,
    };
    for sensors in [pack_rust_sensors(), policy_rust_sensors()] {
        let candidates: Vec<&SensorSpec> = sensors.iter().collect();
        let selection = select(&candidates, &failed);
        assert_eq!(selection.selected_names().len(), RUST_SENSORS.len());
        assert!(
            selection
                .selected
                .iter()
                .all(|s| s.reason == FAIL_CLOSED_REASON),
            "a discovery failure must record the fail-closed reason"
        );
    }
}

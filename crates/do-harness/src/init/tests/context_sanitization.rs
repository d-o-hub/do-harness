//! Tests for the optional context-sanitization scaffolding.
//!
//! The helper is language-agnostic and independent of skill scaffolding, so
//! it ships in every pack and under `--minimal`. The sensor itself stays
//! opt-in: the script is inert until the adopter declares it and enables it.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;

use super::*;
/// The optional context-sanitization helper ships in every language pack,
/// including under `--minimal`, because it is independent of skill
/// scaffolding and inert until the adopter declares the sensor.
#[tokio::test(flavor = "current_thread")]
async fn init_scaffolds_the_optional_sanitization_helper_in_every_pack() {
    for lang in [
        Language::Rust,
        Language::Generic,
        Language::Web,
        Language::Node,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let report = init_workspace(dir.path(), &opts(Some(lang))).await.unwrap();

        assert!(
            report
                .written
                .contains(&"scripts/check-context-sanitization.py".to_owned()),
            "the helper must be written for language {lang:?}"
        );
        assert!(
            dir.path()
                .join("scripts/check-context-sanitization.py")
                .is_file(),
            "the helper must exist for language {lang:?}"
        );

        // The recipe is inert: it adds no sensor and names none in a set.
        let config = fs::read_to_string(dir.path().join("do-harness.toml")).unwrap();
        assert!(
            config.contains("# name = \"context-sanitization\""),
            "the opt-in recipe must be documented for language {lang:?}"
        );
        assert!(
            !config.contains("\nname = \"context-sanitization\""),
            "the opt-in recipe must stay commented out for language {lang:?}"
        );
        assert!(
            !config.contains("\"context-sanitization\","),
            "no signal set may name the optional sensor for language {lang:?}"
        );
    }
}

/// The helper is written under `--minimal` too: the check is independent of
/// skill scaffolding.
#[tokio::test(flavor = "current_thread")]
async fn init_minimal_still_scaffolds_the_sanitization_helper() {
    let dir = tempfile::tempdir().unwrap();
    let mut minimal = opts(Some(Language::Rust));
    minimal.minimal = true;

    init_workspace(dir.path(), &minimal).await.unwrap();

    assert!(
        dir.path()
            .join("scripts/check-context-sanitization.py")
            .is_file()
    );
}

/// A second init preserves the helper, and `--force` replaces it.
#[tokio::test(flavor = "current_thread")]
async fn init_preserves_the_sanitization_helper_and_force_replaces_it() {
    let dir = tempfile::tempdir().unwrap();
    init_workspace(dir.path(), &opts(Some(Language::Rust)))
        .await
        .unwrap();

    let path = dir.path().join("scripts/check-context-sanitization.py");
    let custom = "#!/usr/bin/env python3\n# adopter's own version\n";
    fs::write(&path, custom).unwrap();

    let report = init_workspace(dir.path(), &opts(Some(Language::Rust)))
        .await
        .unwrap();
    assert!(
        report
            .skipped
            .contains(&"scripts/check-context-sanitization.py".to_owned()),
        "a second init must skip the helper"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), custom);

    let mut force = opts(Some(Language::Rust));
    force.force = true;
    let report = init_workspace(dir.path(), &force).await.unwrap();
    assert!(
        report
            .written
            .contains(&"scripts/check-context-sanitization.py".to_owned()),
        "--force must replace the helper"
    );
    assert_ne!(fs::read_to_string(&path).unwrap(), custom);
}

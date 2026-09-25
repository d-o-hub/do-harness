//! `package.json` pin selection in the scaffolded release preflight.
//!
//! The preflight must read a manifest's *own* version. A `version` key inside a
//! dependency, override, or `publishConfig` sub-object is not a release pin, and
//! treating one as a pin fails a correct repository with a nonsense target.

use std::path::Path;

use super::super::*;
use super::release::{output_text, preflight, versioned_fixture};

/// Writes a tracked `package.json` into the fixture root.
fn manifest(root: &Path, relative: &str, body: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
}

#[cfg(unix)]
#[test]
fn nested_version_is_not_a_pin() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    versioned_fixture(root, "0.2.1");
    // No top-level version at all: the only `version` key belongs to a
    // dependency entry, so this manifest is not a pin.
    manifest(
        root,
        "web/package.json",
        "{\"name\":\"web\",\"dependencies\":{\"typescript\":{\"version\":\"^5.0.0\"}}}\n",
    );

    let out = preflight(root, &[], None, &[]);

    assert!(out.status.success(), "{}", output_text(&out));
    assert!(
        output_text(&out).contains("2 version pin(s) agree at 0.2.1"),
        "{}",
        output_text(&out)
    );
}

#[cfg(unix)]
#[test]
fn top_level_version_after_a_nested_object_is_a_pin() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    versioned_fixture(root, "0.2.1");
    manifest(
        root,
        "web/package.json",
        "{\"name\":\"web\",\"overrides\":{\"foo\":{\"version\":\"9.9.9\"}},\"version\":\"0.2.0\"}\n",
    );

    let out = preflight(root, &[], None, &[]);

    assert_eq!(out.status.code(), Some(1), "{}", output_text(&out));
    assert!(
        output_text(&out).contains("web/package.json declares 0.2.0"),
        "{}",
        output_text(&out)
    );
}

#[cfg(unix)]
#[test]
fn nested_private_does_not_hide_a_real_pin() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    versioned_fixture(root, "0.2.1");
    // `private` only counts at the top level (that is the npm field); a nested
    // one elsewhere in the manifest must not silently drop the pin.
    manifest(
        root,
        "web/package.json",
        "{\"name\":\"web\",\"config\":{\"private\":true},\"version\":\"0.2.0\"}\n",
    );

    let out = preflight(root, &[], None, &[]);

    assert_eq!(out.status.code(), Some(1), "{}", output_text(&out));
    assert!(
        output_text(&out).contains("web/package.json declares 0.2.0"),
        "{}",
        output_text(&out)
    );
}

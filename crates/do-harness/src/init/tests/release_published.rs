//! The scaffolded preflight's published-release comparison.
//!
//! `--release` is the only networked half: it fails when the target version
//! already shipped, and degrades loudly — or fails closed under `CI=true` /
//! `DO_HARNESS_REQUIRE_TOOLS=1` — when the comparison cannot run at all.

use super::release::{output_text, preflight, stub_gh, versioned_fixture};

#[cfg(unix)]
#[test]
fn preflight_fails_when_the_target_is_already_released() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    versioned_fixture(root, "0.2.0");
    let gh = stub_gh(
        root,
        r#"printf '[{"tagName":"v0.2.0"},{"tagName":"v0.1.0"}]'"#,
    );

    let out = preflight(root, &["--release"], Some(&gh), &[]);

    assert_eq!(out.status.code(), Some(1), "{}", output_text(&out));
    let shown = output_text(&out);
    assert!(
        shown.contains("already has a GitHub Release for 0.2.0 (v0.2.0)"),
        "{shown}"
    );
    assert!(shown.contains("FIX: bump every version pin"), "{shown}");
}

#[cfg(unix)]
#[test]
fn preflight_passes_for_an_unpublished_target() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    versioned_fixture(root, "0.2.1");
    let gh = stub_gh(root, r#"printf '[{"tagName":"v0.2.0"}]'"#);

    let out = preflight(root, &["--release"], Some(&gh), &[]);

    assert!(out.status.success(), "{}", output_text(&out));
    let shown = output_text(&out);
    assert!(
        shown.contains("0.2.1 is not published yet (latest example/repo release: v0.2.0)"),
        "{shown}"
    );
}

#[cfg(unix)]
#[test]
fn preflight_degrades_without_gh() {
    let dir = tempfile::tempdir().unwrap();
    versioned_fixture(dir.path(), "0.2.1");

    let out = preflight(dir.path(), &["--release"], None, &[]);

    assert!(out.status.success(), "{}", output_text(&out));
    let shown = output_text(&out);
    assert!(
        shown.contains("WARN: release comparison skipped:"),
        "{shown}"
    );
    assert!(shown.contains("not installed"), "{shown}");
}

#[cfg(unix)]
#[test]
fn preflight_degrades_when_gh_cannot_reach_github() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    versioned_fixture(root, "0.2.1");
    let gh = stub_gh(
        root,
        r"echo 'error connecting to api.github.com' >&2
exit 1",
    );

    let out = preflight(root, &["--release"], Some(&gh), &[]);

    assert!(out.status.success(), "{}", output_text(&out));
    let shown = output_text(&out);
    assert!(
        shown.contains("WARN: release comparison skipped:"),
        "{shown}"
    );
    assert!(
        shown.contains("could not list releases for example/repo"),
        "{shown}"
    );
}

#[cfg(unix)]
#[test]
fn preflight_fails_closed_in_ci_when_the_comparison_cannot_run() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    versioned_fixture(root, "0.2.1");
    let gh = stub_gh(root, "exit 1");

    let out = preflight(root, &["--release"], Some(&gh), &[("CI", "true")]);

    assert_eq!(out.status.code(), Some(1), "{}", output_text(&out));
    let shown = output_text(&out);
    assert!(
        shown.contains("FAIL: release preflight could not compare against published releases"),
        "{shown}"
    );
}

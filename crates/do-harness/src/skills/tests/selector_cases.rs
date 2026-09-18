//! Optional semantic selector cases.
//!
//! Fixtures (`write_named`, `rank`) come from the parent module.

use super::*;
use std::path::Path;

/// Writes an executable selector script and points the environment at it.
///
/// Unix-only: spawning a `#!` script directly is a POSIX behaviour, so the
/// spawn-based cases are gated and the trust logic is covered without it.
#[cfg(unix)]
fn write_selector(root: &Path, body: &str) -> std::path::PathBuf {
    let path = root.join("selector.sh");
    fs::write(&path, body).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    path
}

/// Builds a selector configuration pointing at `selector`, or a disabled one.
fn config(selector: Option<&Path>, timeout_secs: f64) -> suggest::SelectorConfig {
    suggest::SelectorConfig {
        bin: selector.map(|path| path.to_string_lossy().into_owned()),
        timeout: std::time::Duration::from_secs_f64(timeout_secs),
    }
}

/// Builds two candidates whose names are known to the fixtures.
fn two_candidates(root: &Path) -> Vec<suggest::Scored> {
    write_named(root, "pr-triage", "pr-triage", "Triage pull requests.");
    write_named(root, "harness", "harness", "Map sensors and guides.");
    rank(root, "sensors", 5)
}

#[test]
fn missing_selector_keeps_the_deterministic_order() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let candidates = two_candidates(root);
    let (selection, warnings) = suggest::select(root, "sensors", &candidates, &config(None, 10.0));
    assert_eq!(selection, suggest::Selection::Deterministic);
    assert!(warnings.is_empty());
}

#[cfg(unix)]
#[test]
fn selector_failures_all_fall_back_to_deterministic() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let candidates = two_candidates(root);

    let cases: [(&str, Option<&str>, &str); 6] = [
        ("#!/bin/sh\nexit 3\n", None, "exited"),
        ("#!/bin/sh\nsleep 5\n", Some("1"), "exceeded"),
        (
            "#!/bin/sh\ncat >/dev/null\nprintf 'not json'\n",
            None,
            "not JSON",
        ),
        (
            "#!/bin/sh\ncat >/dev/null\nprintf '{\"schema_version\":9,\"selected\":[],\"confidence\":0.9}'\n",
            None,
            "unsupported",
        ),
        (
            "#!/bin/sh\ncat >/dev/null\nprintf '{\"schema_version\":1,\"selected\":[],\"confidence\":1.5}'\n",
            None,
            "outside [0,1]",
        ),
        (
            "#!/bin/sh\ncat >/dev/null\nprintf '{\"schema_version\":1,\"selected\":[\"absent-skill\"],\"confidence\":0.9}'\n",
            None,
            "outside the candidate set",
        ),
    ];

    for (body, timeout, needle) in cases {
        let selector = write_selector(root, body);
        let seconds = timeout.map_or(10.0, |value| value.parse().unwrap());
        let (selection, warnings) = suggest::select(
            root,
            "sensors",
            &candidates,
            &config(Some(&selector), seconds),
        );
        assert_eq!(
            selection,
            suggest::Selection::Deterministic,
            "case {needle} must fall back"
        );
        assert!(
            warnings.iter().any(|warning| warning.contains(needle)),
            "case {needle}: {warnings:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn selector_sees_only_metadata_and_can_reorder() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let candidates = two_candidates(root);
    let capture = root.join("captured.json");

    // The selector records exactly what it received, then selects harness.
    let selector = write_selector(
        root,
        &format!(
            "#!/bin/sh\ncat > {}\nprintf '{{\"schema_version\":1,\"selected\":[\"harness\"],\"confidence\":0.94}}'\n",
            capture.display()
        ),
    );

    let (selection, warnings) =
        suggest::select(root, "sensors", &candidates, &config(Some(&selector), 10.0));
    assert_eq!(
        selection,
        suggest::Selection::Selected(vec!["harness".to_owned()])
    );
    assert!(warnings.is_empty(), "{warnings:?}");

    let seen = fs::read_to_string(&capture).unwrap();
    assert!(seen.contains("\"candidates\""));
    assert!(seen.contains("\"query\""));
    assert!(!seen.contains("secret body text"), "{seen}");
    let parsed: serde_json::Value = serde_json::from_str(&seen).unwrap();
    assert_eq!(parsed["schema_version"], 1);

    let reordered = suggest::reorder(candidates, &["harness".to_owned()]);
    assert_eq!(reordered[0].name, "harness");
    assert_eq!(reordered.len(), 2);
}

#[test]
fn reorder_keeps_unselected_candidates_in_deterministic_order() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_named(root, "a", "a", "alpha");
    write_named(root, "b", "b", "beta");
    write_named(root, "c", "c", "gamma");
    let candidates = rank(root, "alpha beta gamma", 5);

    let reordered = suggest::reorder(candidates, &["c".to_owned()]);
    assert_eq!(
        reordered
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        vec!["c", "a", "b"]
    );
}

#[test]
fn selector_output_validation_rejects_every_untrusted_shape() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let candidates = two_candidates(root);

    // The spawn path is platform-specific, but the trust boundary is not:
    // every rejection below is a fallback to the deterministic order.
    let rejections: [(&str, &str); 9] = [
        ("not json", "not JSON"),
        (
            "{\"schema_version\":9,\"selected\":[],\"confidence\":0.9}",
            "unsupported",
        ),
        (
            "{\"schema_version\":1,\"selected\":[],\"confidence\":1.5}",
            "outside [0,1]",
        ),
        (
            "{\"schema_version\":1,\"selected\":[],\"confidence\":-0.1}",
            "outside [0,1]",
        ),
        (
            "{\"schema_version\":1,\"selected\":[\"absent-skill\"],\"confidence\":0.9}",
            "outside the candidate set",
        ),
        (
            "{\"schema_version\":1,\"selected\":[\"harness\",\"harness\"],\"confidence\":0.9}",
            "repeated",
        ),
        (
            "{\"schema_version\":1,\"selected\":[\"harness\",\"pr-triage\",\"harness\"],\"confidence\":0.9}",
            "repeated",
        ),
        ("{\"schema_version\":1,\"confidence\":0.9}", "not JSON"),
        (
            "{\"schema_version\":1,\"selected\":[\"harness\"],\"confidence\":0.9,\"extra\":1}",
            "not JSON",
        ),
    ];
    for (stdout, needle) in rejections {
        let reason = suggest::validate(stdout, &candidates)
            .expect_err(&format!("{stdout} must be rejected"));
        assert!(reason.contains(needle), "case {stdout}: {reason}");
    }

    // An in-set, deduplicated selection is accepted unchanged.
    let accepted = suggest::validate(
        "{\"schema_version\":1,\"selected\":[\"harness\"],\"confidence\":0.9}",
        &candidates,
    )
    .unwrap();
    assert_eq!(accepted, vec!["harness".to_owned()]);

    // A selector may not choose a candidate it was not offered.
    let over = suggest::validate(
        "{\"schema_version\":1,\"selected\":[\"harness\",\"pr-triage\"],\"confidence\":0.9}",
        &candidates[..1],
    )
    .expect_err("an unoffered candidate must be rejected");
    assert!(over.contains("outside the candidate set"), "{over}");
}

#[test]
fn frontmatter_requires_matching_delimiters() {
    assert_eq!(
        catalog::frontmatter("---\nname: x\n---\nbody"),
        Some("name: x\n")
    );
    assert_eq!(catalog::frontmatter("name: x\n---\n"), None);
    assert_eq!(catalog::frontmatter("---\nname: x\n"), None);
    assert_eq!(
        catalog::frontmatter("---\r\nname: x\r\n---\r\n"),
        Some("name: x\r\n")
    );
}

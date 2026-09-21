//! Manifest, digest, and drift cases.
//!
//! Fixtures come from the parent module; every case builds its own tree.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use super::*;

/// Writes the manifest for `[("name", "path", digest)]` entries.
fn write_manifest(root: &Path, entries: &[(&str, &str, &str)]) {
    let commit = "a".repeat(40);
    let mut text = String::new();
    for (name, path, digest) in entries {
        writeln!(
            text,
            "[[skills]]\nname = \"{name}\"\npath = \"{path}\"\nupstream = \"d-o-hub/do-harness\"\ncommit = \"{commit}\"\ncontent_sha256 = \"{digest}\"\n"
        )
        .unwrap();
    }
    fs::write(root.join(drift::MANIFEST_PATH), text).unwrap();
}

/// Creates a managed skill directory `relative` with `files`.
fn write_tree(root: &Path, relative: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = root.join(relative);
    for (name, body) in files {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, body).unwrap();
    }
    dir
}

#[test]
fn tree_digest_ignores_write_order_and_tracks_content() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let first = write_tree(
        root,
        "first",
        &[("SKILL.md", "one"), ("references/notes.md", "two")],
    );
    let second = write_tree(root, "second", &[("references/notes.md", "two")]);
    fs::write(second.join("SKILL.md"), "one").unwrap();

    let digest = drift::tree_digest(&first).unwrap();
    assert_eq!(
        digest,
        drift::tree_digest(&second).unwrap(),
        "the digest must not depend on directory iteration order"
    );
    assert_eq!(digest.len(), 64);

    fs::write(first.join("SKILL.md"), "changed").unwrap();
    assert_ne!(digest, drift::tree_digest(&first).unwrap());

    fs::write(first.join("SKILL.md"), "one").unwrap();
    fs::write(first.join("extra.md"), "added").unwrap();
    assert_ne!(digest, drift::tree_digest(&first).unwrap());
}

#[cfg(unix)]
#[test]
fn tree_digest_rejects_a_symlinked_directory() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let dir = write_tree(root, "managed", &[("SKILL.md", "body")]);
    fs::create_dir_all(root.join("elsewhere")).unwrap();
    std::os::unix::fs::symlink(root.join("elsewhere"), dir.join("linked")).unwrap();

    let error = drift::tree_digest(&dir).unwrap_err().to_string();
    assert!(
        error.contains("symlinked directory"),
        "expected a symlinked-directory error, got: {error}"
    );
}

#[test]
fn parse_rejects_entries_that_cannot_be_checked() {
    let cases = [
        (
            "[[skills]]\nname = \"a\"\npath = \".agents/skills/a\"\nupstream = \"o/r\"\ncommit = \"{commit}\"\ncontent_sha256 = \"bb\"\nextra = true\n",
            "unknown field",
        ),
        (
            "[[skills]]\nname = \"\"\npath = \".agents/skills/a\"\nupstream = \"o/r\"\ncommit = \"{commit}\"\ncontent_sha256 = \"bb\"\n",
            "empty name",
        ),
        (
            "[[skills]]\nname = \"a\"\npath = \"/etc\"\nupstream = \"o/r\"\ncommit = \"{commit}\"\ncontent_sha256 = \"bb\"\n",
            "invalid path",
        ),
        (
            "[[skills]]\nname = \"a\"\npath = \"../outside\"\nupstream = \"o/r\"\ncommit = \"{commit}\"\ncontent_sha256 = \"bb\"\n",
            "invalid path",
        ),
        (
            "[[skills]]\nname = \"a\"\npath = \".agents/skills/a\"\nupstream = \"\"\ncommit = \"{commit}\"\ncontent_sha256 = \"bb\"\n",
            "empty upstream",
        ),
        (
            "[[skills]]\nname = \"a\"\npath = \".agents/skills/a\"\nupstream = \"o/r\"\ncommit = \"{short}\"\ncontent_sha256 = \"bb\"\n",
            "invalid commit pin",
        ),
        (
            "[[skills]]\nname = \"a\"\npath = \".agents/skills/a\"\nupstream = \"o/r\"\ncommit = \"{nonhex}\"\ncontent_sha256 = \"bb\"\n",
            "invalid commit pin",
        ),
        (
            "[[skills]]\nname = \"a\"\npath = \".agents/skills/a\"\nupstream = \"o/r\"\ncommit = \"{commit}\"\ncontent_sha256 = \"short\"\n",
            "invalid content_sha256",
        ),
        (
            "[[skills]]\nname = \"a\"\npath = \".agents/skills/a\"\nupstream = \"o/r\"\ncommit = \"{commit}\"\ncontent_sha256 = \"{digest}\"\n\n[[skills]]\nname = \"a\"\npath = \".agents/skills/b\"\nupstream = \"o/r\"\ncommit = \"{commit}\"\ncontent_sha256 = \"bb\"\n",
            "duplicate managed skill name",
        ),
        (
            "[[skills]]\nname = \"a\"\npath = \".agents/skills/a\"\nupstream = \"o/r\"\ncommit = \"{commit}\"\ncontent_sha256 = \"{digest}\"\n\n[[skills]]\nname = \"b\"\npath = \".agents/skills/a\"\nupstream = \"o/r\"\ncommit = \"{commit}\"\ncontent_sha256 = \"bb\"\n",
            "duplicate managed skill path",
        ),
    ];

    let digest = "b".repeat(64);
    let commit = "a".repeat(40);
    let short = "a".repeat(7);
    let nonhex = "z".repeat(40);
    for (text, needle) in cases {
        let text = text
            .replace("{digest}", &digest)
            .replace("{commit}", &commit)
            .replace("{short}", &short)
            .replace("{nonhex}", &nonhex);
        let error = format!("{:#}", drift::parse(&text).unwrap_err());
        assert!(
            error.contains(needle),
            "manifest case should report {needle:?}, got: {error}"
        );
    }
}

#[test]
fn evaluate_reports_ok_drift_and_missing_in_path_order() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let managed = write_tree(root, ".agents/skills/managed", &[("SKILL.md", "body")]);
    write_tree(root, ".agents/skills/other", &[("SKILL.md", "unmanaged")]);
    let digest = drift::tree_digest(&managed).unwrap();
    write_manifest(
        root,
        &[
            ("missing", ".agents/skills/absent", &"b".repeat(64)),
            ("managed", ".agents/skills/managed", &digest),
            ("drifted", ".agents/skills/other", &"c".repeat(64)),
        ],
    );

    let manifest =
        drift::parse(&fs::read_to_string(root.join(drift::MANIFEST_PATH)).unwrap()).unwrap();
    let report = drift::evaluate(root, &manifest).unwrap();

    let statuses: Vec<(&str, drift::Status)> = report
        .skills
        .iter()
        .map(|skill| (skill.name.as_str(), skill.status))
        .collect();
    assert_eq!(
        statuses,
        vec![
            ("missing", drift::Status::Missing),
            ("managed", drift::Status::Ok),
            ("drifted", drift::Status::Drift),
        ],
        "skills are reported in path order"
    );
    assert_eq!(report.drifted(), 2);
    assert!(
        report.skills[0].actual.is_none(),
        "a missing tree has no digest"
    );
    let observed = drift::tree_digest(&root.join(".agents/skills/other")).unwrap();
    assert_eq!(
        report.skills[2].actual.as_deref(),
        Some(observed.as_str()),
        "a drifted tree reports its observed digest"
    );
}

#[test]
fn parse_rejects_a_manifest_without_entries() {
    let error = format!("{:#}", drift::parse("").unwrap_err());
    assert!(
        error.contains("lists no managed skills"),
        "an empty manifest is a usage error, got: {error}"
    );
}

#[test]
fn manifest_path_defaults_and_honours_an_override() {
    assert_eq!(
        drift::manifest_path(Path::new("/root"), None),
        Path::new("/root").join(drift::MANIFEST_PATH)
    );
    assert_eq!(
        drift::manifest_path(Path::new("/root"), Some(Path::new("custom.toml"))),
        Path::new("/root/custom.toml")
    );
    assert_eq!(
        drift::manifest_path(Path::new("/root"), Some(Path::new("/abs/custom.toml"))),
        Path::new("/abs/custom.toml")
    );
}

/// The digest is a persisted cross-repo contract: this exact tree must keep
/// hashing to this exact value, or manifests in every adopting repository break
/// while relative-behaviour tests stay green.
#[test]
fn tree_digest_is_a_stable_golden_vector() {
    let temp = tempfile::tempdir().unwrap();
    let dir = write_tree(
        temp.path(),
        "managed",
        &[
            ("SKILL.md", "managed body\n"),
            ("references/notes.md", "notes\n"),
        ],
    );

    assert_eq!(
        drift::tree_digest(&dir).unwrap(),
        "1931948c9d191de667ee71a2a8691502442c1d85b715418433c7e7e84a9926ad",
        "framing changes invalidate every existing manifest; migrate pins deliberately"
    );
}

#[cfg(unix)]
#[test]
fn tree_digest_rejects_a_non_utf8_file_name() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let temp = tempfile::tempdir().unwrap();
    let dir = write_tree(temp.path(), "managed", &[("SKILL.md", "body")]);
    fs::write(dir.join(OsStr::from_bytes(b"bad-\xff-name.md")), "body").unwrap();

    let error = format!("{:#}", drift::tree_digest(&dir).unwrap_err());
    assert!(
        error.contains("non-UTF-8 file name"),
        "a lossy name would make distinct files hash alike, got: {error}"
    );
}

#[cfg(unix)]
#[test]
fn evaluate_rejects_a_managed_path_that_resolves_outside_the_root() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    let outside = write_tree(temp.path(), "elsewhere", &[("SKILL.md", "external")]);
    fs::create_dir_all(root.join(".agents/skills")).unwrap();
    std::os::unix::fs::symlink(&outside, root.join(".agents/skills/managed")).unwrap();
    let digest = drift::tree_digest(&outside).unwrap();
    write_manifest(&root, &[("managed", ".agents/skills/managed", &digest)]);

    let manifest =
        drift::parse(&fs::read_to_string(root.join(drift::MANIFEST_PATH)).unwrap()).unwrap();
    let error = format!("{:#}", drift::evaluate(&root, &manifest).unwrap_err());
    assert!(
        error.contains("resolves outside the repository root"),
        "a symlinked managed path must not hash content the repository does not own, got: {error}"
    );
}

//! Digest cases: framing stability and the entry types a pinned tree refuses.

use std::fs;

use super::*;

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

    let digest = tree::tree_digest(&first).unwrap();
    assert_eq!(
        digest,
        tree::tree_digest(&second).unwrap(),
        "the digest must not depend on directory iteration order"
    );
    assert_eq!(digest.len(), 64);

    fs::write(first.join("SKILL.md"), "changed").unwrap();
    assert_ne!(digest, tree::tree_digest(&first).unwrap());

    fs::write(first.join("SKILL.md"), "one").unwrap();
    fs::write(first.join("extra.md"), "added").unwrap();
    assert_ne!(digest, tree::tree_digest(&first).unwrap());
}

#[cfg(unix)]
#[test]
fn tree_digest_rejects_a_symlinked_directory() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let dir = write_tree(root, "managed", &[("SKILL.md", "body")]);
    fs::create_dir_all(root.join("elsewhere")).unwrap();
    std::os::unix::fs::symlink(root.join("elsewhere"), dir.join("linked")).unwrap();

    let error = tree::tree_digest(&dir).unwrap_err().to_string();
    assert!(
        error.contains("symlinked directory"),
        "expected a symlinked-directory error, got: {error}"
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
        tree::tree_digest(&dir).unwrap(),
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

    let error = format!("{:#}", tree::tree_digest(&dir).unwrap_err());
    assert!(
        error.contains("non-UTF-8 file name"),
        "a lossy name would make distinct files hash alike, got: {error}"
    );
}

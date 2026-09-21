//! Shared fixtures for the `skills` unit tests.
//!
//! Every fixture builds its own `.agents/skills` tree under a `TempDir`, so no
//! test reads the repository's real catalog or leaves residue behind.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;

use super::cache;
use super::catalog::{self, SKILL_ROOT};
use super::drift;
use super::suggest;

mod catalog_cases;
mod drift_cases;
mod selector_cases;

/// Writes a skill directory with the given frontmatter body.
pub(super) fn write_skill(root: &Path, dir: &str, frontmatter: &str) {
    let path = root.join(SKILL_ROOT).join(dir);
    fs::create_dir_all(&path).unwrap();
    fs::write(
        path.join("SKILL.md"),
        format!("---\n{frontmatter}\n---\n\n# Body\n\nsecret body text\n"),
    )
    .unwrap();
}

/// Writes a plausible skill with `name`/`description` frontmatter.
pub(super) fn write_named(root: &Path, dir: &str, name: &str, description: &str) {
    write_skill(
        root,
        dir,
        &format!(
            "name: {name}\ndescription: {description}\nlicense: MIT\nmetadata:\n  version: \"0.1.0\"\n  tags: a b\n"
        ),
    );
}

/// Ranks the catalog of `root` for `query`.
pub(super) fn rank(root: &Path, query: &str, limit: usize) -> Vec<suggest::Scored> {
    let catalog = catalog::scan(root).unwrap();
    suggest::rank(&catalog.skills, query, limit)
}

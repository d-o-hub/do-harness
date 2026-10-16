//! Nested-build-isolation Rule 3: an instrumented child that runs in the wrong
//! working directory writes `default_*.profraw` beside the sources it ran in.
//!
//! The skill documents this as a manual step after a coverage session
//! (`find . -name '*.profraw' | wc -l`). Nothing else notices a leak: the files
//! are gitignored, the plain suite passes, and the next instrumented run
//! silently inflates its report. This test runs the same check as part of every
//! suite run, next to the code whose spawn helpers cause the leak.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

/// Build output, local harness state, and dependency caches. The instrumentation
/// profile path lives under one of these, so a `.profraw` found here is
/// llvm-cov's own file rather than litter beside sources.
const BUILD_OR_STATE_DIRS: &[&str] = &["target", ".git", ".do-harness", "node_modules"];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repository root")
}

/// Collects `*.profraw` paths outside [`BUILD_OR_STATE_DIRS`].
///
/// Symlinked entries are skipped rather than followed, so a cyclic link cannot
/// turn the walk into a loop.
fn litter(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if file_type.is_dir() {
            let name = entry.file_name();
            if BUILD_OR_STATE_DIRS.contains(&name.to_string_lossy().as_ref()) {
                continue;
            }
            litter(&path, found);
        } else if file_type.is_file()
            && path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("profraw"))
        {
            found.push(path);
        }
    }
}

#[test]
fn no_profraw_litter_beside_sources() {
    let root = repo_root();
    let mut found = Vec::new();
    litter(&root, &mut found);
    found.sort();
    assert!(
        found.is_empty(),
        "instrumented runs leaked .profraw litter beside sources; redirect \
         LLVM_PROFILE_FILE at the spawn site instead of removing it \
         (.agents/skills/nested-build-isolation, rule 3): {found:?}"
    );
}

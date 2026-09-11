//! Shared helpers for integration fixtures.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;

/// Builds a `git` command rooted at `root` with hook-inherited repository
/// environment removed.
///
/// Git exports `GIT_DIR` (and related variables) to hooks. Tests that create
/// their own repositories must not inherit them, or `git init`/`git status`
/// silently target the parent repository instead of the fixture.
pub fn git_command(root: &Path) -> Command {
    let mut command = Command::new("git");
    command.current_dir(root);
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CEILING_DIRECTORIES",
        "GIT_NAMESPACE",
        "GIT_PREFIX",
    ] {
        command.env_remove(key);
    }
    command
}

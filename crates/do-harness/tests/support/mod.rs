//! Shared helpers for integration fixtures.
//!
//! Each test binary compiles its own copy of this module, so helpers used by
//! only some fixtures are legitimately dead in the others.
#![allow(clippy::unwrap_used, clippy::expect_used, dead_code)]

use std::path::Path;
use std::process::Command;

/// Environment variables through which `git` selects which repository and
/// history it reads.
///
/// Git exports `GIT_DIR` (and friends) to every hook, and a caller can set the
/// view overrides (`GIT_COMMON_DIR`, `GIT_SHALLOW_FILE`, graft/replace files).
/// Left in place, `git init`/`git status`/`git log` silently target another
/// repository, or report a rewritten history, instead of the fixture — which
/// is how a local `git push` once failed on a test unrelated to the diff. The
/// same set is cleared by `changes::git_command` and by the generated scripts
/// (`templates/scripts/check-*.sh`).
pub const GIT_VIEW_ENV: [&str; 12] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CEILING_DIRECTORIES",
    "GIT_NAMESPACE",
    "GIT_PREFIX",
    "GIT_COMMON_DIR",
    "GIT_SHALLOW_FILE",
    "GIT_GRAFT_FILE",
    "GIT_REPLACE_REF_BASE",
];

/// Removes [`GIT_VIEW_ENV`] from `command`.
pub fn clear_git_view(command: &mut Command) -> &mut Command {
    for key in GIT_VIEW_ENV {
        command.env_remove(key);
    }
    command
}

/// Removes inherited session state from a command a fixture spawns: the
/// [`GIT_VIEW_ENV`] view variables plus the outer cargo/coverage session.
///
/// cargo-llvm-cov instruments a build through an inherited `RUSTC_WRAPPER` and
/// target dir; left in place, a fixture's own `cargo llvm-cov nextest`
/// re-enters the outer session and dies with "Resource temporarily unavailable
/// (os error 11)", so `verify --strict` fails its coverage sensor under
/// `cargo llvm-cov` only. The profile path goes to the null device so
/// instrumented children cannot write `default_*.profraw` into the tree.
pub fn isolate_command(command: &mut Command) -> &mut Command {
    clear_git_view(command);
    for key in [
        "CARGO_TARGET_DIR",
        "CARGO_INCREMENTAL",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_LLVM_COV",
        "CARGO_LLVM_COV_TARGET_DIR",
        "RUSTC_WRAPPER",
        "CARGO_BUILD_RUSTC_WRAPPER",
        "RUSTC",
        "RUSTDOC",
        "RUSTFLAGS",
        "RUSTDOCFLAGS",
    ] {
        command.env_remove(key);
    }
    command.env("LLVM_PROFILE_FILE", "/dev/null");
    command
}

/// Builds a `git` command rooted at `root` with [`GIT_VIEW_ENV`] removed.
///
/// Tests that create their own repositories must not inherit the view
/// variables, or `git init`/`git status` silently target the parent repository
/// instead of the fixture.
pub fn git_command(root: &Path) -> Command {
    let mut command = Command::new("git");
    command.arg("-c").arg("core.hooksPath=.git/hooks");
    command.current_dir(root);
    clear_git_view(&mut command);
    command
}

pub mod dora;
pub mod reuse;

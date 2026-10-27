//! Harness workspace root discovery.

use std::path::{Path, PathBuf};

use crate::error::{DbError, Result};

/// Name of the harness config file that marks a workspace root.
pub const CONFIG_FILENAME: &str = "do-harness.toml";

/// Name of the agent contract file used as a secondary root marker.
pub const AGENTS_FILENAME: &str = "AGENTS.md";

/// Name of the local state directory inside a workspace root.
const STATE_DIR: &str = ".do-harness";

/// Relative path of the invariant seed file inside a workspace root.
const INVARIANTS_RELATIVE_PATH: &str = "plans/invariants.json";

/// Returns the do-harness workspace root containing `start`.
///
/// Walks up from `start` and treats a directory as a root when it contains
/// `do-harness.toml`, or contains `AGENTS.md` together with either
/// `.do-harness/` or `plans/invariants.json`.
///
/// # Errors
///
/// Returns an error when no root marker is found in `start` or any ancestor.
pub fn find_harness_root(start: &Path) -> Result<PathBuf> {
    for dir in start.ancestors() {
        if is_harness_root(dir) {
            return Ok(dir.to_path_buf());
        }
    }
    Err(DbError::RootNotFound(format!(
        "no {CONFIG_FILENAME} (or {AGENTS_FILENAME} with plans/invariants.json) under {}",
        start.display()
    )))
}

/// Whether `dir` carries a harness root marker.
fn is_harness_root(dir: &Path) -> bool {
    if dir.join(CONFIG_FILENAME).exists() {
        return true;
    }
    let agent_contract = dir.join(AGENTS_FILENAME);
    agent_contract.exists()
        && (dir.join(STATE_DIR).exists() || dir.join(INVARIANTS_RELATIVE_PATH).exists())
}

/// Default path of the agent-state database under `root`.
///
/// Resolved to `<root>/.do-harness/agent_state.db`.
#[must_use]
pub fn db_path(root: &Path) -> PathBuf {
    root.join(STATE_DIR).join("agent_state.db")
}

/// Check managed database/state paths before opening or writing any of them.
/// This requires a stable worktree; it does not isolate hostile concurrent renames.
///
/// # Errors
/// Rejects non-UTF-8 roots, state symlinks and unreadable metadata.
pub fn validate_state_paths(root: &Path) -> Result<()> {
    if root.to_str().is_none() {
        return Err(DbError::Invariant(
            "managed state root must be UTF-8".into(),
        ));
    }
    let paths = [
        root.join(STATE_DIR),
        db_path(root),
        root.join(STATE_DIR).join("agent_state.db-wal"),
        root.join(STATE_DIR).join("agent_state.db-shm"),
        root.join(STATE_DIR).join("agent_state.db.bak"),
    ];
    for path in paths {
        match std::fs::symlink_metadata(&path) {
            Ok(meta)
                if meta.file_type().is_symlink()
                    || (path == root.join(STATE_DIR) && !meta.is_dir())
                    || (path != root.join(STATE_DIR) && !meta.is_file()) =>
            {
                return Err(DbError::Invariant(
                    "managed .do-harness state path must be a regular file/directory without symlinks".into(),
                ));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => return Err(DbError::Io { path, source }),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::fs;

    #[test]
    fn find_harness_root_detects_config_marker() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("do-harness.toml"), "language = \"rust\"\n").expect("write");
        let nested = dir.path().join("a").join("b");
        fs::create_dir_all(&nested).expect("mkdir");
        let found = find_harness_root(&nested).expect("root");
        assert_eq!(found, dir.path());
    }

    #[test]
    fn find_harness_root_requires_agents_with_state_or_plans() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("AGENTS.md"), "# x\n").expect("write");
        let nested = dir.path().join("sub");
        fs::create_dir_all(&nested).expect("mkdir");
        assert!(find_harness_root(&nested).is_err());

        fs::create_dir_all(dir.path().join("plans")).expect("mkdir plans");
        fs::write(dir.path().join("plans/invariants.json"), "[]").expect("write plans");
        assert!(find_harness_root(&nested).is_ok());
    }

    #[test]
    fn find_harness_root_rejects_plain_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(find_harness_root(dir.path()).is_err());
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn state_symlinks_are_rejected_before_opening_the_database() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join(STATE_DIR)).unwrap();
        assert!(crate::connect_and_migrate(root.path()).await.is_err());
        assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
    }
}

//! Harness workspace root discovery.

use std::path::{Path, PathBuf};

use anyhow::Result;

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
    anyhow::bail!(
        "harness root not found: no {CONFIG_FILENAME} (or {AGENTS_FILENAME} with plans/invariants.json) \
         under {}",
        start.display()
    );
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

#[cfg(test)]
mod tests {
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
}

//! Optional `evals/walkthrough.sh` runner for `do-harness eval`.
//!
//! A skill may ship an executable `evals/walkthrough.sh` that produces real
//! artifacts (e.g. it writes a file that a later `exists:` assertion checks).
//! When present and executable it is run once per skill with `cwd` = the
//! workspace root and `DO_HARNESS_ROOT` set to the workspace root; a non-zero
//! exit fails every graded (prefixed) assertion for that skill.

use std::path::Path;
use std::process::Command;

/// Outcome of running a skill's walkthrough, or of deciding not to.
#[derive(Debug, Clone, Copy)]
pub struct WalkRun {
    /// Whether a walkthrough script was found and executed.
    pub present: bool,
    /// Whether the walkthrough exited 0. Trivially `true` when absent.
    pub success: bool,
}

/// Runs `skill_dir/evals/walkthrough.sh` when it exists and is executable.
///
/// A missing or non-executable script is skipped and counts as a success (the
/// walkthrough is purely optional). When present, the script runs with the
/// workspace root as its working directory and `DO_HARNESS_ROOT` set.
#[must_use]
pub fn run_walkthrough(skill_dir: &Path, root: &Path) -> WalkRun {
    let script = skill_dir.join("evals/walkthrough.sh");
    let present = script.is_file();
    if !present {
        return WalkRun {
            present,
            success: true,
        };
    }
    let status = Command::new(&script)
        .current_dir(root)
        .env("DO_HARNESS_ROOT", root)
        .env("PYTHONUNBUFFERED", "1")
        .status();
    match status {
        Ok(status) => WalkRun {
            present,
            success: status.success(),
        },
        // Executable bit missing or spawn error: treat as a skipped walkthrough.
        Err(_) => WalkRun {
            present: false,
            success: true,
        },
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use std::fs;

    #[test]
    fn absent_walkthrough_skips_and_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let run = run_walkthrough(dir.path(), dir.path());
        assert!(!run.present);
        assert!(run.success);
    }

    #[test]
    fn failing_walkthrough_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let evals = dir.path().join("evals");
        fs::create_dir_all(&evals).unwrap();
        let script = evals.join("walkthrough.sh");
        fs::write(&script, "#!/bin/sh\nexit 3\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let run = run_walkthrough(dir.path(), dir.path());
        assert!(run.present);
        assert!(!run.success);
    }

    #[test]
    fn successful_walkthrough_writes_artifact() {
        let dir = tempfile::tempdir().unwrap();
        let evals = dir.path().join("evals");
        fs::create_dir_all(&evals).unwrap();
        let script = evals.join("walkthrough.sh");
        fs::write(
            &script,
            "#!/bin/sh\necho generated > \"$DO_HARNESS_ROOT/artifact.txt\"\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let run = run_walkthrough(dir.path(), dir.path());
        assert!(run.present);
        assert!(run.success);
        assert!(dir.path().join("artifact.txt").is_file());
    }
}

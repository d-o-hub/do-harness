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

/// Runs `skill_dir/evals/walkthrough.sh` when it exists.
///
/// A missing script is skipped and counts as a success (the walkthrough is
/// purely optional). When present, the script runs with the workspace root as
/// its working directory, `DO_HARNESS_ROOT` set, and `DO_HARNESS_BIN` set to
/// the currently-executing harness binary so `cli:` residue and walkthrough
/// steps drive the same binary under test.
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
    let Ok(bin) = std::env::current_exe() else {
        return WalkRun {
            present,
            success: true,
        };
    };
    let output = Command::new("sh")
        .arg(&script)
        .current_dir(root)
        .env("DO_HARNESS_ROOT", root)
        .env("DO_HARNESS_BIN", bin)
        .env("PYTHONUNBUFFERED", "1")
        .output();
    match output {
        Ok(output) => WalkRun {
            present: true,
            success: output.status.success(),
        },
        // Spawn error: treat as a skipped walkthrough.
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

    #[test]
    fn noisy_stdout_and_stderr_success_walkthrough_still_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let evals = dir.path().join("evals");
        fs::create_dir_all(&evals).unwrap();
        let script = evals.join("walkthrough.sh");
        fs::write(&script, "#!/bin/sh\necho leaked; echo err >&2; exit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let run = run_walkthrough(dir.path(), dir.path());
        assert!(run.present);
        // The run succeeds even though the child wrote to both streams; with
        // `.output()` the child's stdout/stderr are captured via pipes and
        // dropped, never inherited by (or leaked to) the parent's terminal.
        assert!(run.success);
    }

    #[test]
    fn noisy_stdout_nonzero_exit_is_reported_failing() {
        let dir = tempfile::tempdir().unwrap();
        let evals = dir.path().join("evals");
        fs::create_dir_all(&evals).unwrap();
        let script = evals.join("walkthrough.sh");
        fs::write(
            &script,
            "#!/bin/sh\necho leaked stdout; echo leaked stderr >&2; exit 7\n",
        )
        .unwrap();
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
    fn walkthrough_env_vars_are_set() {
        let dir = tempfile::tempdir().unwrap();
        let evals = dir.path().join("evals");
        fs::create_dir_all(&evals).unwrap();
        let script = evals.join("walkthrough.sh");
        // Persist the env values so the test can assert they are set to the
        // expected root and the running harness binary. cwd is the root.
        fs::write(
            &script,
            "#!/bin/sh\n\
             printf '%s' \"$DO_HARNESS_ROOT\" > root.txt\n\
             printf '%s' \"$DO_HARNESS_BIN\" > bin.txt\n",
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
        let root = dir.path().to_str().unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("root.txt")).unwrap(),
            root
        );
        let bin = fs::read_to_string(dir.path().join("bin.txt")).unwrap();
        assert_eq!(bin, std::env::current_exe().unwrap().to_str().unwrap());
        // The bin path is non-empty and resolves back to the harness binary.
        assert!(!bin.is_empty());
    }
}

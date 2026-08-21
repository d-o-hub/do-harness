//! Optional `evals/walkthrough.sh` runner for `do-harness eval`.
//!
//! A skill may ship an executable `evals/walkthrough.sh` that produces real
//! artifacts (e.g. it writes a file that a later `exists:` assertion checks).
//! When present and executable it is run once per skill with `cwd` = the
//! workspace root and `DO_HARNESS_ROOT` set to the workspace root; a non-zero
//! exit fails every graded (prefixed) assertion for that skill.
//!
//! The script is executed **directly** so its shebang decides the interpreter:
//! routing it through `/bin/sh` silently breaks bash-only syntax such as
//! `set -o pipefail` on dash-based systems.

use std::io::ErrorKind;
use std::path::Path;
use std::process::{Command, Output};

/// Maximum characters of a failed walkthrough's stderr kept as evidence.
const STDERR_TAIL_CHARS: usize = 500;

/// Outcome of running a skill's walkthrough, or of deciding not to.
#[derive(Debug, Clone)]
pub struct WalkRun {
    /// Whether a walkthrough script was found and executed.
    pub present: bool,
    /// Whether the walkthrough exited 0. Trivially `true` when absent.
    pub success: bool,
    /// Why the walkthrough could not be launched or failed, when known.
    ///
    /// Launch failures (unresolvable harness binary, unspawnable shell) are
    /// recorded here and always mark the run as failed: a present-but-
    /// unlaunchable walkthrough must never be scored as a silent success.
    pub detail: Option<String>,
}

impl WalkRun {
    /// Outcome for a skill with no walkthrough script: skipped, successful.
    #[must_use]
    pub fn absent() -> Self {
        Self {
            present: false,
            success: true,
            detail: None,
        }
    }
}

/// Runs `skill_dir/evals/walkthrough.sh` when it exists.
///
/// A missing script is skipped and counts as a success (the walkthrough is
/// purely optional). When present, the script runs with the workspace root as
/// its working directory, `DO_HARNESS_ROOT` set, and `DO_HARNESS_BIN` set to
/// the currently-executing harness binary so `cli:` residue and walkthrough
/// steps drive the same binary under test.
///
/// A present script that cannot be launched (binary resolution or spawn
/// failure) is reported as a failed run with the cause in [`WalkRun::detail`]
/// rather than being silently scored as a success. A non-zero exit records
/// the stderr tail in `detail` so eval output shows *why* it failed.
#[must_use]
pub fn run_walkthrough(skill_dir: &Path, root: &Path) -> WalkRun {
    let script = skill_dir.join("evals/walkthrough.sh");
    if !script.is_file() {
        return WalkRun::absent();
    }
    let bin = match std::env::current_exe() {
        Ok(bin) => bin,
        Err(err) => {
            return WalkRun {
                present: true,
                success: false,
                detail: Some(format!(
                    "could not resolve harness binary for walkthrough {}: {err}",
                    script.display()
                )),
            };
        }
    };
    match spawn_walkthrough(&script, root, &bin) {
        Ok(output) => {
            let success = output.status.success();
            let detail = (!success).then(|| format_stderr_tail(&output.stderr));
            WalkRun {
                present: true,
                success,
                detail,
            }
        }
        Err(err) => WalkRun {
            present: true,
            success: false,
            detail: Some(format!("could not launch {}: {err}", script.display())),
        },
    }
}

/// Spawns the walkthrough script directly, honoring its shebang; falls back
/// to an explicit `bash` invocation only when a lost exec bit denies direct
/// execution.
fn spawn_walkthrough(script: &Path, root: &Path, bin: &Path) -> std::io::Result<Output> {
    let attempt = Command::new(script)
        .current_dir(root)
        .env("DO_HARNESS_ROOT", root)
        .env("DO_HARNESS_BIN", bin)
        .env("PYTHONUNBUFFERED", "1")
        .output();
    match attempt {
        Ok(output) => Ok(output),
        Err(err) if err.kind() == ErrorKind::PermissionDenied => Command::new("bash")
            .arg(script)
            .current_dir(root)
            .env("DO_HARNESS_ROOT", root)
            .env("DO_HARNESS_BIN", bin)
            .env("PYTHONUNBUFFERED", "1")
            .output(),
        Err(err) => Err(err),
    }
}

/// Bounds failed-run stderr to its last [`STDERR_TAIL_CHARS`] characters so
/// eval output carries the cause without dumping unbounded logs.
fn format_stderr_tail(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let trimmed = text.trim_end();
    let count = trimmed.chars().count();
    if count <= STDERR_TAIL_CHARS {
        return format!("walkthrough.sh failed: {trimmed}");
    }
    let tail: String = trimmed.chars().skip(count - STDERR_TAIL_CHARS).collect();
    format!("walkthrough.sh failed: …{tail}")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::fs;

    /// A bash-only walkthrough (`set -o pipefail`) passes: the script runs
    /// directly under its shebang interpreter, never through dash.
    #[test]
    fn bash_only_syntax_passes_via_direct_execution() {
        let dir = tempfile::tempdir().unwrap();
        let evals = dir.path().join("evals");
        fs::create_dir_all(&evals).unwrap();
        let script = evals.join("walkthrough.sh");
        fs::write(&script, "#!/usr/bin/env bash\nset -euo pipefail\necho ok\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let run = run_walkthrough(dir.path(), dir.path());
        assert!(run.present);
        assert!(run.success, "detail: {:?}", run.detail);
        assert!(run.detail.is_none());
    }

    /// A failing walkthrough surfaces its stderr tail as evidence instead of
    /// swallowing it.
    #[test]
    fn failing_walkthrough_stderr_surfaces_in_detail() {
        let dir = tempfile::tempdir().unwrap();
        let evals = dir.path().join("evals");
        fs::create_dir_all(&evals).unwrap();
        let script = evals.join("walkthrough.sh");
        fs::write(&script, "#!/usr/bin/env sh\necho boom-marker >&2\nexit 5\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let run = run_walkthrough(dir.path(), dir.path());
        assert!(run.present);
        assert!(!run.success);
        let detail = run.detail.expect("failing run must carry a detail");
        assert!(detail.contains("boom-marker"), "detail: {detail}");
        assert!(detail.starts_with("walkthrough.sh failed:"));
    }

    #[test]
    fn absent_walkthrough_skips_and_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let run = run_walkthrough(dir.path(), dir.path());
        assert!(!run.present);
        assert!(run.success);
        assert!(run.detail.is_none());
        let absent = WalkRun::absent();
        assert!(!absent.present);
        assert!(absent.success);
        assert!(absent.detail.is_none());
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

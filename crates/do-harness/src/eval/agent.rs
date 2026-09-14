//! Agent-run evaluation: real Skill Lift with an external agent in the loop.
//!
//! With `do-harness eval --agent-cmd <command>`, every eval case runs the
//! agent once in its own hermetic sandbox instead of the deterministic
//! walkthrough. The command runs with `cwd` = sandbox root; the case prompt
//! is exported as `DO_HARNESS_PROMPT` and piped on stdin; stdout is saved to
//! `agent_stdout.txt` so fixtures can grade the final answer. The
//! without-skill baseline runs the same agent with the guidance payload
//! stripped, so the delta is real Skill Lift in points.
//!
//! The sandbox is filesystem isolation only (see [`crate::eval_sandbox`]):
//! the agent command runs with the caller's privileges, so only point it at
//! an agent you would run by hand.

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::eval_sandbox::Sandbox;
use crate::eval_walk::{WalkRun, stderr_tail};

use super::grading::{GradeOutcome, SkillEvals, grade_skill, single_case};

/// Poll interval for agent exit and timeout checks.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// External agent invocation settings from `eval --agent-cmd`.
#[derive(Debug, Clone)]
pub(super) struct AgentSpec {
    /// Shell command executed once per eval case.
    pub(super) command: String,
    /// Wall-clock bound; the run is killed when it expires.
    pub(super) timeout: Duration,
}

/// Runs every case of `evals` through the agent, one sandbox per case.
///
/// With `without_guidance`, each sandbox has `SKILL.md` + `references/`
/// stripped before the agent runs: the same task without the skill's
/// guidance, which is the baseline Skill Lift subtracts.
pub(super) async fn check_skill_agent(
    real_root: &Path,
    skill_dir: &Path,
    name: &str,
    evals: &SkillEvals,
    spec: &AgentSpec,
    without_guidance: bool,
) -> Result<GradeOutcome> {
    let mut total = GradeOutcome::empty();
    for case in &evals.evals {
        let sandbox = Sandbox::for_skill(real_root, skill_dir, name)?;
        if without_guidance {
            sandbox.strip_guidance(name)?;
        }
        let root = sandbox.root().to_path_buf();
        let spec = spec.clone();
        let prompt = case.prompt.clone();
        let started = Instant::now();
        let run_root = root.clone();
        let walk = tokio::task::spawn_blocking(move || run_agent(&spec, &prompt, &run_root))
            .await
            .with_context(|| format!("agent run task failed for '{name}' case {}", case.id))?;
        let elapsed = started.elapsed().as_secs_f64();

        let case_evals = single_case(evals, case);
        let mut case_outcome = grade_skill(&case_evals, &root, &walk).await?;
        case_outcome.walk_secs = elapsed;
        total.walk_secs += elapsed;
        total.merge_case(case_outcome);
        drop(sandbox);
    }
    Ok(total)
}

/// Runs the agent command once against `root` and returns its outcome.
///
/// The prompt is exported as `DO_HARNESS_PROMPT` and piped on stdin; stdout
/// is always persisted to `<root>/agent_stdout.txt` (even on failure) so a
/// fixture can assert on the final answer and failures stay debuggable.
fn run_agent(spec: &AgentSpec, prompt: &str, root: &Path) -> WalkRun {
    let bin = match std::env::current_exe() {
        Ok(bin) => bin,
        Err(err) => {
            return WalkRun {
                present: true,
                success: false,
                detail: Some(format!("could not resolve harness binary for agent: {err}")),
            };
        }
    };
    let mut child = match Command::new("bash")
        .arg("-c")
        .arg(&spec.command)
        .current_dir(root)
        .env("DO_HARNESS_ROOT", root)
        .env("DO_HARNESS_BIN", &bin)
        .env("DO_HARNESS_PROMPT", prompt)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            return WalkRun {
                present: true,
                success: false,
                detail: Some(format!("could not launch agent command: {err}")),
            };
        }
    };

    // Feed stdin from a helper thread so a child that ignores stdin cannot
    // deadlock the parent on a full pipe.
    if let Some(mut stdin) = child.stdin.take() {
        let prompt = prompt.to_owned();
        thread::spawn(move || {
            let _ = stdin.write_all(prompt.as_bytes());
        });
    }
    // Drain both pipes to EOF on helper threads while the poll loop watches
    // the clock: reading only after exit would deadlock on a full pipe.
    let stdout_handle = child.stdout.take().map(|mut out| {
        thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = out.read_to_end(&mut buf);
            buf
        })
    });
    let stderr_handle = child.stderr.take().map(|mut err| {
        thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = err.read_to_end(&mut buf);
            buf
        })
    });

    let started = Instant::now();
    let (success, detail) = loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break (true, None),
            Ok(Some(_)) => {
                let stderr = stderr_handle
                    .and_then(|handle| handle.join().ok())
                    .unwrap_or_default();
                break (
                    false,
                    Some(format!("agent command failed: {}", stderr_tail(&stderr))),
                );
            }
            Ok(None) => {
                if started.elapsed() >= spec.timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    break (
                        false,
                        Some(format!("agent timed out after {}s", spec.timeout.as_secs())),
                    );
                }
                thread::sleep(POLL_INTERVAL);
            }
            Err(err) => break (false, Some(format!("could not wait for agent: {err}"))),
        }
    };

    let stdout = stdout_handle
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    let _ = std::fs::write(root.join("agent_stdout.txt"), &stdout);
    WalkRun {
        present: true,
        success,
        detail,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn agent_failure_surfaces_stderr_tail() {
        let dir = tempfile::tempdir().unwrap();
        let spec = AgentSpec {
            command: "echo boom-marker >&2; exit 3".to_owned(),
            timeout: Duration::from_secs(5),
        };
        let run = run_agent(&spec, "prompt", dir.path());
        assert!(run.present);
        assert!(!run.success);
        let detail = run.detail.expect("failure detail");
        assert!(detail.contains("boom-marker"), "detail: {detail}");
        assert!(detail.starts_with("agent command failed:"));
    }

    #[test]
    fn agent_stdout_is_captured_for_assertions() {
        let dir = tempfile::tempdir().unwrap();
        let spec = AgentSpec {
            command: "printf 'final answer: 42\\n'".to_owned(),
            timeout: Duration::from_secs(5),
        };
        let run = run_agent(&spec, "prompt", dir.path());
        assert!(run.success, "detail: {:?}", run.detail);
        let saved = std::fs::read_to_string(dir.path().join("agent_stdout.txt")).unwrap();
        assert!(saved.contains("final answer: 42"));
    }

    #[test]
    fn agent_receives_prompt_via_env_and_stdin() {
        let dir = tempfile::tempdir().unwrap();
        let spec = AgentSpec {
            command: "printf '%s|' \"$DO_HARNESS_PROMPT\" > prompt.txt; cat >> prompt.txt"
                .to_owned(),
            timeout: Duration::from_secs(5),
        };
        let run = run_agent(&spec, "hello-spike", dir.path());
        assert!(run.success, "detail: {:?}", run.detail);
        let seen = std::fs::read_to_string(dir.path().join("prompt.txt")).unwrap();
        assert_eq!(seen, "hello-spike|hello-spike");
    }

    #[test]
    fn agent_timeout_kills_the_run() {
        let dir = tempfile::tempdir().unwrap();
        let spec = AgentSpec {
            command: "sleep 30".to_owned(),
            timeout: Duration::from_millis(200),
        };
        let run = run_agent(&spec, "prompt", dir.path());
        assert!(run.present);
        assert!(!run.success);
        let detail = run.detail.expect("timeout detail");
        assert!(detail.contains("timed out"), "detail: {detail}");
    }
}

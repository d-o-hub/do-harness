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
use std::process::{ExitStatus, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{self, Receiver},
};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::eval_sandbox::Sandbox;
use crate::eval_walk::{WalkRun, stderr_tail};

use super::grading::{GradeOutcome, SkillEvals, grade_skill, single_case};

/// Poll interval for agent exit and timeout checks.
const POLL_INTERVAL: Duration = Duration::from_millis(50);
const AGENT_OUTPUT_LIMIT_BYTES: usize = crate::config::DEFAULT_OUTPUT_BYTES;
const AGENT_CAPTURE_CHUNK_BYTES: usize = 8192;

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
    trials: usize,
) -> Result<GradeOutcome> {
    let mut total = GradeOutcome::empty();
    let num_trials = trials.max(1);
    for case in &evals.evals {
        for trial in 1..=num_trials {
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
                .with_context(|| {
                    format!(
                        "agent run task failed for '{name}' case {} trial {trial}",
                        case.id
                    )
                })?;
            let elapsed = started.elapsed().as_secs_f64();

            let case_evals = single_case(evals, case);
            let mut case_outcome = grade_skill(&case_evals, &root, &walk).await?;
            case_outcome.walk_secs = elapsed;
            for c in &mut case_outcome.cases {
                c.trial = Some(trial);
            }
            total.walk_secs += elapsed;
            total.merge_case(case_outcome);
            drop(sandbox);
        }
    }
    Ok(total)
}

/// Runs the agent command once against `root` and returns its outcome.
///
/// The prompt is exported as `DO_HARNESS_PROMPT` and piped on stdin; stdout
/// is always persisted to `<root>/agent_stdout.txt` (even on failure) so a
/// fixture can assert on the final answer and failures stay debuggable.
pub(super) fn run_agent(spec: &AgentSpec, prompt: &str, root: &Path) -> WalkRun {
    run_agent_with_redactor(
        spec,
        prompt,
        root,
        crate::redaction::Redactor::from_environment(&[]),
    )
}

#[derive(Default)]
struct AgentPipeOutput {
    bytes: Vec<u8>,
    read_error: bool,
}

fn append_bounded(output: &mut Vec<u8>, bytes: &[u8], limit: usize) {
    let remaining = limit.saturating_sub(output.len());
    output.extend_from_slice(&bytes[..bytes.len().min(remaining)]);
}

fn pump_agent_pipe(
    mut pipe: impl Read + Send + 'static,
    redactor: crate::redaction::Redactor,
    total: Arc<AtomicU64>,
    oversized: Arc<AtomicBool>,
) -> Receiver<AgentPipeOutput> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut stream = redactor.stream();
        let mut output = Vec::new();
        let mut chunk = [0; AGENT_CAPTURE_CHUNK_BYTES];
        let mut read_error = false;
        let limit = AGENT_OUTPUT_LIMIT_BYTES;
        let limit_u64 = u64::try_from(limit).unwrap_or(u64::MAX);
        loop {
            match pipe.read(&mut chunk) {
                Ok(0) => break,
                Ok(count) => {
                    let count_u64 = u64::try_from(count).unwrap_or(u64::MAX);
                    let observed = total
                        .fetch_add(count_u64, Ordering::SeqCst)
                        .saturating_add(count_u64);
                    if observed > limit_u64 {
                        oversized.store(true, Ordering::SeqCst);
                    }
                    append_bounded(&mut output, &stream.feed(&chunk[..count], false), limit);
                }
                Err(_) => {
                    read_error = true;
                    break;
                }
            }
        }
        append_bounded(&mut output, &stream.feed(&[], true), limit);
        let _ = sender.send(AgentPipeOutput {
            bytes: output,
            read_error,
        });
    });
    receiver
}

struct AgentPipes {
    stdout: Option<Receiver<AgentPipeOutput>>,
    stderr: Option<Receiver<AgentPipeOutput>>,
    total: Arc<AtomicU64>,
    oversized: Arc<AtomicBool>,
}

struct CapturedAgentProcess {
    status: Option<ExitStatus>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    read_error: bool,
    failure: Option<String>,
    cleanup_unverified: bool,
}

pub(super) fn run_agent_with_redactor(
    spec: &AgentSpec,
    prompt: &str,
    root: &Path,
    redactor: crate::redaction::Redactor,
) -> WalkRun {
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
    let mut command = crate::shell::bash();
    command
        .arg("-c")
        .arg(&spec.command)
        .current_dir(root)
        .env("DO_HARNESS_ROOT", root)
        .env("DO_HARNESS_BIN", &bin)
        .env("DO_HARNESS_PROMPT", prompt)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    crate::shell::configure_process_group(&mut command);
    let child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            return WalkRun {
                present: true,
                success: false,
                detail: Some(format!("could not launch agent command: {err}")),
            };
        }
    };

    let captured = capture_agent(child, spec, prompt, redactor);
    let _ = std::fs::write(root.join("agent_stdout.txt"), &captured.stdout);
    let mut failure = captured.failure;
    if captured.cleanup_unverified {
        let cleanup_detail = format!(
            "agent cleanup unverified after {}ms",
            crate::shell::PROCESS_CLEANUP_BUDGET.as_millis()
        );
        failure = Some(match failure {
            Some(primary) => format!("{primary}; {cleanup_detail}"),
            None => cleanup_detail,
        });
    }
    let detail = match failure {
        Some(detail) => Some(detail),
        None if captured.read_error => {
            Some("agent output capture failed while reading a pipe".to_owned())
        }
        None => match captured.status {
            Some(status) if status.success() => None,
            Some(_) => Some(format!(
                "agent command failed: {}",
                stderr_tail(&captured.stderr)
            )),
            None => Some("could not verify agent process exit".to_owned()),
        },
    };
    WalkRun {
        present: true,
        success: detail.is_none(),
        detail,
    }
}

fn capture_agent(
    mut child: std::process::Child,
    spec: &AgentSpec,
    prompt: &str,
    redactor: crate::redaction::Redactor,
) -> CapturedAgentProcess {
    if let Some(mut stdin) = child.stdin.take() {
        let prompt = prompt.to_owned();
        thread::spawn(move || {
            let _ = stdin.write_all(prompt.as_bytes());
        });
    }
    let total = Arc::new(AtomicU64::new(0));
    let oversized = Arc::new(AtomicBool::new(false));
    let stdout = child.stdout.take().map(|pipe| {
        pump_agent_pipe(
            pipe,
            redactor.clone(),
            Arc::clone(&total),
            Arc::clone(&oversized),
        )
    });
    let stderr = child
        .stderr
        .take()
        .map(|pipe| pump_agent_pipe(pipe, redactor, Arc::clone(&total), Arc::clone(&oversized)));
    wait_for_agent(
        child,
        spec,
        AgentPipes {
            stdout,
            stderr,
            total,
            oversized,
        },
    )
}

fn receive_agent_pipe(
    receiver: Option<&Receiver<AgentPipeOutput>>,
    output: &mut Option<AgentPipeOutput>,
) {
    if output.is_none() {
        *output = receiver.and_then(|receiver| receiver.try_recv().ok());
    }
}

fn wait_for_agent(
    mut child: std::process::Child,
    spec: &AgentSpec,
    pipes: AgentPipes,
) -> CapturedAgentProcess {
    let AgentPipes {
        stdout,
        stderr,
        total,
        oversized,
    } = pipes;
    let mut stdout_output = stdout.is_none().then(AgentPipeOutput::default);
    let mut stderr_output = stderr.is_none().then(AgentPipeOutput::default);
    let mut failure = (stdout.is_none() || stderr.is_none())
        .then(|| "could not capture agent stdout and stderr".to_owned());
    let started = Instant::now();
    let mut status: Option<ExitStatus> = None;
    let mut cleanup_started = None;
    let mut cleanup_unverified = false;
    loop {
        receive_agent_pipe(stdout.as_ref(), &mut stdout_output);
        receive_agent_pipe(stderr.as_ref(), &mut stderr_output);
        if status.is_none() && cleanup_started.is_none() {
            match child.try_wait() {
                Ok(found) => status = found,
                Err(err) => failure = Some(format!("could not wait for agent: {err}")),
            }
        }
        if failure.is_none() {
            if oversized.load(Ordering::SeqCst) {
                let output_bytes = total.load(Ordering::SeqCst);
                failure = Some(format!(
                    "agent output limit exceeded ({output_bytes} bytes, limit {AGENT_OUTPUT_LIMIT_BYTES})"
                ));
            } else if status.is_none() && started.elapsed() >= spec.timeout {
                failure = Some(format!("agent timed out after {}s", spec.timeout.as_secs()));
            }
        }
        if (status.is_some() || failure.is_some()) && cleanup_started.is_none() {
            crate::shell::kill_tree(&mut child);
            cleanup_started = Some(Instant::now());
        }
        if stdout_output.is_some() && stderr_output.is_some() && cleanup_started.is_some() {
            break;
        }
        if cleanup_started
            .is_some_and(|since: Instant| since.elapsed() >= crate::shell::PROCESS_CLEANUP_BUDGET)
        {
            cleanup_unverified = true;
            break;
        }
        thread::sleep(POLL_INTERVAL);
    }

    let stdout_output = stdout_output.unwrap_or_default();
    let stderr_output = stderr_output.unwrap_or_default();
    CapturedAgentProcess {
        status,
        stdout: stdout_output.bytes,
        stderr: stderr_output.bytes,
        read_error: stdout_output.read_error || stderr_output.read_error,
        failure,
        cleanup_unverified,
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

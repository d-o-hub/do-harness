//! The authoritative completion gate.
//!
//! An executor's exit status, result file, or “done” claim never proves
//! completion. The gate runs the repository's own strict, recorded
//! verification and then requires fresh green evidence from `status`. A
//! warning, a vacuous check, a stale receipt, or a malformed report is not
//! success.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::redaction::Redactor;

use super::executor::{self, BoundedOutput};
use super::receipt::VerificationRecord;

/// Result of one authoritative gate run.
pub struct GateOutcome {
    /// Fresh, strict, non-vacuous evidence proves completion.
    pub green: bool,
    /// Recorded observation; absent when neither command produced usable JSON.
    pub verification: Option<VerificationRecord>,
    /// Bounded reason the gate is not green.
    pub diagnostic: Option<String>,
}

/// Runs verification and freshness checks in `workspace` for `signal_set`.
#[must_use]
pub fn run(workspace: &Path, signal_set: &str, timeout: Duration) -> GateOutcome {
    let redactor = Redactor::from_environment(&[]);
    // Re-invokes this same binary as the authoritative gate: the path
    // identifies the running tool, not an untrusted input.
    // nosemgrep: rust.lang.security.current-exe.current-exe
    let Ok(executable) = std::env::current_exe() else {
        return GateOutcome {
            green: false,
            verification: None,
            diagnostic: Some("cannot locate the do-harness binary for the gate".to_owned()),
        };
    };
    let verify = run_harness(
        &executable,
        workspace,
        &[
            "verify",
            "--set",
            signal_set,
            "--strict",
            "--record",
            "--unchanged=run",
            "--format",
            "json",
        ],
        timeout,
        &redactor,
    );
    let status = run_harness(
        &executable,
        workspace,
        &["status", "--set", signal_set, "--format", "json"],
        timeout,
        &redactor,
    );

    let (failed, unrunnable) = parse_verify(&verify.stdout);
    let (state, reason, recovery) = parse_status(&status.stdout);
    let green = verify.exit_code == Some(0) && state.as_deref() == Some("green");
    let verification = state.map(|state| VerificationRecord {
        state,
        reason: reason.unwrap_or_else(|| "unknown".to_owned()),
        failed,
        unrunnable,
    });
    let diagnostic = if green {
        None
    } else {
        Some(describe(&verify, &status, recovery.as_deref()))
    };
    GateOutcome {
        green,
        verification,
        diagnostic,
    }
}

/// Runs one harness command with a bounded wall clock.
fn run_harness(
    executable: &Path,
    workspace: &Path,
    args: &[&str],
    timeout: Duration,
    redactor: &Redactor,
) -> BoundedOutput {
    let mut command = Command::new(executable);
    command.arg("--root").arg(workspace).args(args);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.env("DO_HARNESS_AUTO_REPORT", "off");
    crate::shell::configure_process_group(&mut command);
    executor::run_bounded(command, timeout, redactor)
}

/// Extracts `(failed, unrunnable)` sensor names from a verify report.
fn parse_verify(stdout: &str) -> (Vec<String>, Vec<String>) {
    let Ok(document) = serde_json::from_str::<serde_json::Value>(stdout) else {
        return (Vec::new(), Vec::new());
    };
    (names(&document, "failed"), names(&document, "unrunnable"))
}

/// Extracts `(state, reason, recovery)` from a status report.
fn parse_status(stdout: &str) -> (Option<String>, Option<String>, Option<String>) {
    let Ok(document) = serde_json::from_str::<serde_json::Value>(stdout) else {
        return (None, None, None);
    };
    (
        document
            .get("state")
            .and_then(|v| v.as_str())
            .map(ToOwned::to_owned),
        document
            .get("reason")
            .and_then(|v| v.as_str())
            .map(ToOwned::to_owned),
        document
            .get("recovery")
            .and_then(|v| v.as_str())
            .map(ToOwned::to_owned),
    )
}

/// Sorted string array at `key`.
fn names(document: &serde_json::Value, key: &str) -> Vec<String> {
    let mut names: Vec<String> = document
        .get(key)
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(ToOwned::to_owned))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// Builds a bounded, redacted diagnostic for a red gate.
fn describe(verify: &BoundedOutput, status: &BoundedOutput, recovery: Option<&str>) -> String {
    let mut parts = Vec::new();
    if let Some(error) = &verify.spawn_error {
        parts.push(format!("verify did not run: {error}"));
    } else if verify.timed_out {
        parts.push("verify exceeded the gate budget".to_owned());
    } else {
        parts.push(format!("verify exited {:?}", verify.exit_code));
    }
    if let Some(state) = parse_status(&status.stdout).0 {
        parts.push(format!("evidence is {state}"));
    }
    let detail = verify.stderr.trim();
    if !detail.is_empty() {
        parts.push(detail.to_owned());
    }
    if let Some(recovery) = recovery {
        parts.push(format!("recovery: {recovery}"));
    }
    super::receipt::bounded(&parts.join("; "))
}

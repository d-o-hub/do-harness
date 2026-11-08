//! Deterministic mechanical repairs declared by project policy.
//!
//! A repair runs only when its named sensor failed in the same attempt, only
//! inside the job workspace, and at most once per job: its identifier is
//! recorded in the receipt before the command runs, so an interrupted job that
//! resumes never applies a completed side effect twice.

use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;

use sha2::Digest;

use crate::config::RepairSpec;
use crate::redaction::Redactor;

use super::executor;
use super::receipt::{JobReceipt, VerificationRecord};

/// Applies every declared repair whose sensor failed, returning their keys.
pub(super) fn apply(
    repairs: &[RepairSpec],
    verification: Option<&VerificationRecord>,
    receipt: &mut JobReceipt,
    workspace: &Path,
    root: &Path,
    timeout: Duration,
) -> Vec<String> {
    let Some(verification) = verification else {
        return Vec::new();
    };
    let failed: BTreeSet<&str> = verification.failed.iter().map(String::as_str).collect();
    let redactor = Redactor::from_environment(&[]);
    let mut applied = Vec::new();
    for repair in repairs {
        if !failed.contains(repair.sensor.as_str()) {
            continue;
        }
        let key = key(repair);
        if receipt.applied_repairs.contains(&key) {
            continue;
        }
        // Recorded before it runs: a crash may leave the receipt claiming a
        // repair that did not happen, which is the safe direction for resume.
        receipt.applied_repairs.push(key.clone());
        let _ = receipt.save(root);
        let mut command = crate::shell::command(&repair.argv[0], &repair.argv[1..]);
        command.current_dir(workspace);
        command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        crate::shell::configure_process_group(&mut command);
        let _ = executor::run_bounded(command, timeout, &redactor);
        applied.push(key);
    }
    applied
}

/// Stable identifier for one declared repair.
fn key(repair: &RepairSpec) -> String {
    let material = format!("{}\u{0}{}", repair.sensor, repair.argv.join("\u{0}"));
    format!(
        "{}:{}",
        repair.sensor,
        &hex::encode(sha2::Sha256::digest(material.as_bytes()))[..16]
    )
}

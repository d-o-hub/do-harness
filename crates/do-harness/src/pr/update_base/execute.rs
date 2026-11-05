//! Independent-PR latest-base update and the bounded post-update poll (#468).
//!
//! The update is issued through `gh pr update-branch` pinned to the expected
//! head (`--expected-head-sha`), so a concurrent author push is rejected by the
//! server rather than overwritten. A `403`/`422` blocks without retry. After an
//! accepted update the new head is observed immediately, then after 3 s, 10 s,
//! and every 30 s up to the deadline; a moved base, a stray head, or a timeout
//! invalidates the evidence rather than claiming success.

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::Result;

use crate::pr::gh;

/// Result of an executed update.
#[derive(Debug, Clone)]
pub enum ExecuteOutcome {
    /// The head now contains the base at `head`/`base`.
    Updated { head: String, base: String },
    /// `GitHub` rejected the update with `HTTP 403` (permission).
    Rejected403,
    /// `GitHub` rejected the update with `HTTP 422` (expected-head mismatch).
    Rejected422,
    /// The base moved while the update was in flight.
    BaseMoved { base: String },
    /// No new head appeared before the deadline.
    Timeout,
}

/// Default total deadline for observing the updated head.
const POLL_DEADLINE_SECS: u64 = 1200;

/// Reads the poll deadline, allowing a bounded override for tests.
fn deadline_secs() -> u64 {
    std::env::var("DO_HARNESS_PR_UPDATE_BASE_TIMEOUT_SECS")
        .ok()
        .and_then(|raw| raw.parse().ok())
        .filter(|&value| value > 0 && value <= POLL_DEADLINE_SECS)
        .unwrap_or(POLL_DEADLINE_SECS)
}

/// Issues `gh pr update-branch` and polls until the head reflects the update.
pub fn update(root: &Path, number: u64, old_head: &str, old_base: &str) -> Result<ExecuteOutcome> {
    let pr = number.to_string();
    let (output, stderr) = crate::gh::try_output(
        root,
        &["pr", "update-branch", &pr, "--expected-head-sha", old_head],
        "pr update-branch",
    )?;
    if !output.status.success() {
        let text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            stderr.trim()
        );
        if text.contains("HTTP 403") || text.contains("Forbidden") {
            return Ok(ExecuteOutcome::Rejected403);
        }
        if text.contains("HTTP 422") || text.contains("422") {
            return Ok(ExecuteOutcome::Rejected422);
        }
        anyhow::bail!("gh pr update-branch failed: {}", text.trim());
    }

    let deadline = Instant::now() + Duration::from_secs(deadline_secs());
    poll(root, number, old_base, old_head, deadline)
}

/// Polls `pr` until its head changes, the base moves, or the deadline passes.
///
/// The first observation happens immediately, then after 3 s, 10 s, and every
/// 30 s up to `deadline`. A moved base, a stray head, or a timeout invalidates
/// the evidence rather than claiming success.
fn poll(
    root: &Path,
    number: u64,
    old_base: &str,
    old_head: &str,
    deadline: Instant,
) -> Result<ExecuteOutcome> {
    // Immediate probe.
    let first = gh::readiness_view(root, number)?;
    if first.base_ref_oid != old_base {
        return Ok(ExecuteOutcome::BaseMoved {
            base: first.base_ref_oid,
        });
    }
    if first.head_ref_oid != old_head {
        return Ok(ExecuteOutcome::Updated {
            head: first.head_ref_oid,
            base: first.base_ref_oid,
        });
    }

    let mut waits = [3u64, 10].iter().copied().chain(std::iter::repeat(30u64));
    loop {
        let wait = Duration::from_secs(waits.next().unwrap_or(30));
        if Instant::now()
            .checked_add(wait)
            .is_none_or(|next| next >= deadline)
        {
            // No room for another wait: one final probe, then give up.
            let last = gh::readiness_view(root, number)?;
            if last.base_ref_oid == old_base && last.head_ref_oid != old_head {
                return Ok(ExecuteOutcome::Updated {
                    head: last.head_ref_oid,
                    base: last.base_ref_oid,
                });
            }
            return Ok(ExecuteOutcome::Timeout);
        }
        std::thread::sleep(wait);
        let view = gh::readiness_view(root, number)?;
        if view.base_ref_oid != old_base {
            return Ok(ExecuteOutcome::BaseMoved {
                base: view.base_ref_oid,
            });
        }
        if view.head_ref_oid != old_head {
            return Ok(ExecuteOutcome::Updated {
                head: view.head_ref_oid,
                base: view.base_ref_oid,
            });
        }
        if Instant::now() >= deadline {
            return Ok(ExecuteOutcome::Timeout);
        }
    }
}

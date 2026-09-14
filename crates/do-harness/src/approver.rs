//! Shared bless-approver identity resolution.

use std::path::Path;

use anyhow::{Result, bail};

/// Resolves a bless approver: explicit flag, `DO_HARNESS_APPROVER`, then the
/// git user email. An anonymous bless is rejected (fail-closed).
///
/// # Errors
///
/// Returns an error when no non-empty approver identity can be resolved.
pub fn resolve(explicit: Option<&str>, root: &Path) -> Result<String> {
    if let Some(value) = explicit.filter(|value| !value.trim().is_empty()) {
        return Ok(value.trim().to_owned());
    }
    if let Ok(value) = std::env::var("DO_HARNESS_APPROVER") {
        if !value.trim().is_empty() {
            return Ok(value.trim().to_owned());
        }
    }
    if let Ok(output) = crate::changes::git_command(root)
        .args(["config", "user.email"])
        .output()
    {
        if output.status.success() {
            let email = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if !email.is_empty() {
                return Ok(email);
            }
        }
    }
    bail!("--bless requires an approver: pass --approver <name> or set DO_HARNESS_APPROVER")
}

//! Shared invocation of the `GitHub` CLI (`gh`).
//!
//! Four modules read `GitHub` metadata (`pr`, `dora`, `ci_explain`, and the
//! PR-loop measures). They differed only in the command they run, so the
//! process plumbing — working directory, spawn context, non-zero-exit error,
//! and JSON decoding — lives here once.

use std::path::Path;
use std::process::{Command, Output};

use anyhow::{Context, Result, bail};

/// Builds a `gh` command rooted at `root`.
#[must_use]
pub fn command(root: &Path) -> Command {
    let mut command = Command::new("gh");
    command.current_dir(root);
    command
}

/// Runs `gh` with `args`, returning its captured output.
///
/// `what` names the operation in both failure messages, so a missing or
/// unauthenticated CLI stays actionable.
///
/// # Errors
///
/// Returns an error when `gh` cannot be spawned or exits non-zero.
pub fn output(root: &Path, args: &[&str], what: &str) -> Result<Output> {
    let output = command(root)
        .args(args)
        .output()
        .with_context(|| format!("failed to run gh {what} (is the GitHub CLI installed?)"))?;
    if !output.status.success() {
        bail!(
            "gh {what} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output)
}

/// Runs `gh` with `args` and parses stdout as a single JSON document.
///
/// # Errors
///
/// Returns an error when `gh` fails or the payload does not match `T`.
pub fn json<T: serde::de::DeserializeOwned>(root: &Path, args: &[&str], what: &str) -> Result<T> {
    let output = output(root, args, what)?;
    serde_json::from_slice(&output.stdout).with_context(|| format!("unexpected gh {what} JSON"))
}

/// Parses newline-delimited JSON objects, the shape `gh api --jq` streams.
///
/// # Errors
///
/// Returns an error naming `what` when a line does not parse.
pub fn json_lines<T: serde::de::DeserializeOwned>(bytes: &[u8], what: &str) -> Result<Vec<T>> {
    let text = String::from_utf8_lossy(bytes);
    let mut items = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        items.push(serde_json::from_str(line).with_context(|| format!("unexpected {what} JSON"))?);
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn json_lines_skips_blank_lines_and_parses_each_object() {
        let items: Vec<u32> = json_lines(b"1\n\n2\n", "row").unwrap();
        assert_eq!(items, vec![1, 2]);
    }

    #[test]
    fn json_lines_names_the_payload_on_a_bad_line() {
        let err = json_lines::<u32>(b"1\nnope\n", "check run").unwrap_err();
        assert!(
            format!("{err:#}").contains("unexpected check run JSON"),
            "unexpected error: {err:#}"
        );
    }
}

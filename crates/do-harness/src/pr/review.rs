//! Review report assembly: merge-base policy probe, residual units, cache.

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::cache::{self, CacheKey};
use super::command::Target;
use super::diff::{self, Unit};
use super::gh;
use super::no_effect;
use crate::changes::git_command;

/// Stable review report schema version.
pub const SCHEMA_VERSION: u32 = 1;
/// Gate policy path, read from the merge-base revision only.
pub const POLICY_PATH: &str = ".github/pr-gate.toml";

/// Where the gate policy was read from and whether it parsed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    /// Repository-relative policy path.
    pub path: String,
    /// Whether the policy file exists at the merge base.
    pub present: bool,
    /// Revision the policy was read from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_rev: Option<String>,
    /// Hex sha256 of the policy bytes; `none` when absent.
    pub sha256: String,
}

/// Proof claim that a residual unit was skipped but looks behavioral.
///
/// Always empty until phase-3 proof mapping lands; the field is part of the
/// frozen schema so consumers can fail closed on it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FalseProven {
    /// Unit id the claim covers.
    pub unit_id: String,
    /// Why the skip is untrusted.
    pub reason: String,
}

/// Deterministic review report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewReport {
    /// Schema version.
    pub schema_version: u32,
    /// `range` or `pr`.
    pub mode: String,
    /// Pull request number in `pr` mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr: Option<u64>,
    /// Base revision or branch.
    pub base: String,
    /// Head revision.
    pub head: String,
    /// Merge-base commit, or the base ref when none could be resolved.
    pub merge_base: String,
    /// Whether the report came from the local cache.
    pub cached: bool,
    /// Gate policy probe result.
    pub policy: Policy,
    /// Changed units evidence could not prove.
    pub residual: Vec<Unit>,
    /// Proven units listed for audit; empty until proof mapping lands.
    pub skipped: Vec<Unit>,
    /// Untrusted skip claims; empty until proof mapping lands.
    pub false_proven: Vec<FalseProven>,
    /// Non-fatal diagnostics; affected units stay residual.
    pub warnings: Vec<String>,
}

/// Builds the review report for `target`.
///
/// # Errors
///
/// Returns an error when the target revisions or diff cannot be resolved; the
/// command treats that as "cannot review", never as "nothing to review".
pub fn analyze(root: &Path, target: &Target, recompute: bool) -> Result<ReviewReport> {
    match target {
        Target::Range { base, head } => {
            let merge_base = no_effect::merge_base(root, base, head)
                .with_context(|| format!("cannot compare {base}...{head}"))?;
            let head_sha = no_effect::commit_sha(root, head)?;
            let text = local_diff(root, &merge_base, head)?;
            let (policy, warning) = probe_policy(root, &merge_base);
            Ok(assemble(
                Inputs {
                    root,
                    mode: "range",
                    pr: None,
                    base,
                    head,
                    merge_base: &merge_base,
                    head_sha: &head_sha,
                    diff: text,
                    policy,
                    warnings: warning.into_iter().collect(),
                    cacheable: true,
                },
                recompute,
            ))
        }
        Target::Pr(number) => {
            let view = gh::view(root, *number)?;
            if let Some((merge_base, head_sha, text)) = local_pr_inputs(root, &view) {
                let (policy, warning) = probe_policy(root, &merge_base);
                return Ok(assemble(
                    Inputs {
                        root,
                        mode: "pr",
                        pr: Some(*number),
                        base: &view.base_ref_name,
                        head: &view.head_ref_oid,
                        merge_base: &merge_base,
                        head_sha: &head_sha,
                        diff: text,
                        policy,
                        warnings: warning.into_iter().collect(),
                        cacheable: true,
                    },
                    recompute,
                ));
            }
            let warning =
                "merge base unavailable locally; using gh pr diff (policy not read)".to_owned();
            let text = gh::diff(root, *number)?;
            Ok(assemble(
                Inputs {
                    root,
                    mode: "pr",
                    pr: Some(*number),
                    base: &view.base_ref_name,
                    head: &view.head_ref_oid,
                    merge_base: &view.base_ref_name,
                    head_sha: &view.head_ref_oid,
                    diff: text,
                    policy: absent_policy(None),
                    warnings: vec![warning],
                    cacheable: false,
                },
                recompute,
            ))
        }
    }
}

/// Bundled report inputs; keeps the assembly signature small.
struct Inputs<'a> {
    root: &'a Path,
    mode: &'static str,
    pr: Option<u64>,
    base: &'a str,
    head: &'a str,
    merge_base: &'a str,
    head_sha: &'a str,
    diff: String,
    policy: Policy,
    warnings: Vec<String>,
    /// Whether the merge base is a resolved commit and cacheable.
    cacheable: bool,
}

/// Serves a cache hit or parses the diff and stores a fresh report.
fn assemble(inputs: Inputs<'_>, recompute: bool) -> ReviewReport {
    let key = CacheKey {
        schema_version: SCHEMA_VERSION,
        merge_base: inputs.merge_base.to_owned(),
        head_sha: inputs.head_sha.to_owned(),
        policy_sha256: inputs.policy.sha256.clone(),
    };
    if inputs.cacheable && !recompute {
        if let Some(mut cached) = cache::load::<ReviewReport>(inputs.root, &key) {
            if cached.schema_version == SCHEMA_VERSION
                && cached.merge_base == inputs.merge_base
                && cached.policy.sha256 == inputs.policy.sha256
            {
                cached.cached = true;
                return cached;
            }
        }
    }
    let parsed = diff::parse(&inputs.diff);
    let mut warnings = inputs.warnings;
    warnings.extend(parsed.warnings);
    let mut report = ReviewReport {
        schema_version: SCHEMA_VERSION,
        mode: inputs.mode.to_owned(),
        pr: inputs.pr,
        base: inputs.base.to_owned(),
        head: inputs.head.to_owned(),
        merge_base: inputs.merge_base.to_owned(),
        cached: false,
        policy: inputs.policy,
        residual: parsed.units,
        skipped: Vec::new(),
        false_proven: Vec::new(),
        warnings,
    };
    if inputs.cacheable {
        if let Err(err) = cache::store(inputs.root, &key, &report) {
            report
                .warnings
                .push(format!("review cache not written: {err}"));
        }
    }
    report
}

/// Resolves merge base and head for a PR from the local clone, when present.
fn local_pr_inputs(root: &Path, view: &gh::PrView) -> Option<(String, String, String)> {
    if !no_effect::rev_exists(root, &view.head_ref_oid) {
        return None;
    }
    let candidates = [
        view.base_ref_name.clone(),
        format!("origin/{}", view.base_ref_name),
    ];
    for candidate in candidates {
        if !no_effect::rev_exists(root, &candidate) {
            continue;
        }
        if let Ok(merge_base) = no_effect::merge_base(root, &candidate, &view.head_ref_oid) {
            if let Ok(text) = local_diff(root, &merge_base, &view.head_ref_oid) {
                return Some((merge_base, view.head_ref_oid.clone(), text));
            }
        }
    }
    None
}

/// Runs `git diff` for the merge-base..head pair.
fn local_diff(root: &Path, base: &str, head: &str) -> Result<String> {
    let output = git_command(root)
        .args([
            "diff",
            "--no-color",
            "--find-renames",
            "--unified=3",
            base,
            head,
            "--",
        ])
        .output()
        .context("failed to run git diff")?;
    if !output.status.success() {
        bail!(
            "git diff {base} {head} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Reads and validates the policy from `rev`, never from the PR head.
fn probe_policy(root: &Path, rev: &str) -> (Policy, Option<String>) {
    let output = git_command(root)
        .args(["show", &format!("{rev}:{POLICY_PATH}")])
        .output();
    let Ok(output) = output else {
        return (
            absent_policy(Some(rev)),
            Some(format!("could not read {POLICY_PATH} at {rev}")),
        );
    };
    if !output.status.success() {
        return (absent_policy(Some(rev)), None);
    }
    let bytes = output.stdout;
    let sha256 = sha256_hex(&bytes);
    let policy = Policy {
        path: POLICY_PATH.to_owned(),
        present: true,
        source_rev: Some(rev.to_owned()),
        sha256,
    };
    match String::from_utf8(bytes) {
        Ok(text) => match toml::from_str::<toml::Value>(&text) {
            Ok(_) => (policy, None),
            Err(err) => (
                policy,
                Some(format!("malformed {POLICY_PATH} at {rev}: {err}")),
            ),
        },
        Err(_) => (policy, Some(format!("{POLICY_PATH} at {rev} is not UTF-8"))),
    }
}

/// Policy probe result when the file is absent or cannot be read.
fn absent_policy(source_rev: Option<&str>) -> Policy {
    Policy {
        path: POLICY_PATH.to_owned(),
        present: false,
        source_rev: source_rev.map(str::to_owned),
        sha256: "none".to_owned(),
    }
}

/// Hex sha256 of `bytes`.
fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

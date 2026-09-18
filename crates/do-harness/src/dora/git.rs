//! Deterministic git derivation of the four DORA metrics.
//!
//! Every call goes through [`crate::changes::git_command`], which clears the
//! hook-inherited `GIT_DIR` family so a hook-invoked run cannot silently
//! target the hook's repository. Every function returns a full result or an
//! error: there is no partial number and no zero-on-failure path, because a
//! deployment metric that silently under-counts is worse than one that
//! refuses to answer.

use std::path::Path;

use anyhow::{Context, Result, bail};

use super::policy::SUPPORTED_REVERT_PATTERN;

/// One deploy tag resolved to its deployed revision and deploy timestamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    /// Short tag name (e.g. `v0.1.1`).
    pub name: String,
    /// Deployed revision: the peeled object for annotated tags, the ref
    /// itself for lightweight tags.
    pub rev: String,
    /// Commit timestamp of [`Tag::rev`] in Unix seconds.
    pub deploy_ts: i64,
}

/// Whether `root` lies inside a git working tree.
#[must_use]
pub fn is_work_tree(root: &Path) -> bool {
    crate::changes::git_command(root)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .is_ok_and(|out| {
            out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "true"
        })
}

/// Returns the current `HEAD` revision.
///
/// # Errors
///
/// Returns an error when `git rev-parse HEAD` fails (no repository, unborn
/// `HEAD`, or a corrupt object store).
pub fn head_rev(root: &Path) -> Result<String> {
    let out = run(root, &["rev-parse", "HEAD"])?;
    let rev = out.trim().to_string();
    if rev.is_empty() {
        bail!("git rev-parse HEAD produced no revision");
    }
    Ok(rev)
}

/// Lists matching tags in ascending `creatordate` order with their deployed
/// revision and deploy timestamp.
///
/// `creatordate` orders the tag list used for predecessor resolution. The
/// deploy timestamp is read from the *peeled* revision (`%(*objectname)` for
/// annotated tags, `%(objectname)` for lightweight ones), so an annotated tag
/// added later measures the deployment it names rather than the tagging
/// moment.
///
/// # Errors
///
/// Returns an error when `for-each-ref` fails or a tag does not resolve to a
/// commit.
pub fn matching_tags(root: &Path, glob: &str) -> Result<Vec<Tag>> {
    let format = "%(creatordate:unix)%09%(refname:short)%09%(*objectname)%09%(objectname)";
    let out = run(
        root,
        &[
            "for-each-ref",
            "--sort=creatordate",
            "--format",
            format,
            glob,
        ],
    )?;

    let mut tags = Vec::new();
    for line in out.lines().filter(|line| !line.trim().is_empty()) {
        let fields: Vec<&str> = line.split('\t').collect();
        let [_, name, peeled, object] = fields.as_slice() else {
            bail!("unexpected git for-each-ref row: {line}");
        };
        let rev = if peeled.is_empty() { object } else { peeled };
        tags.push(Tag {
            name: (*name).to_string(),
            rev: (*rev).to_string(),
            deploy_ts: commit_time(root, rev)?,
        });
    }
    // `creatordate` only guarantees a stable read order; the derivation needs
    // deployment order. For an annotated tag `creatordate` is the tagger date —
    // when the tag object was written — so a tag back-filled after a later
    // release would otherwise order the earlier release last and invert the
    // predecessor relationship, turning `P..T` into `descendant..ancestor` (an
    // empty range that silently reports zero lead-time samples). The name is a
    // deterministic tiebreak for tags sharing a commit.
    tags.sort_by(|left, right| {
        left.deploy_ts
            .cmp(&right.deploy_ts)
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(tags)
}

/// Reads the commit timestamp of `rev` in Unix seconds.
///
/// # Errors
///
/// Returns an error when `git log` fails or the output is not an integer.
fn commit_time(root: &Path, rev: &str) -> Result<i64> {
    let out = run(root, &["log", "-1", "--format=%ct", rev])?;
    out.trim()
        .parse::<i64>()
        .with_context(|| format!("git log -1 %ct {rev} produced a non-numeric timestamp: {out:?}"))
}

/// Lead-time samples for one deploy range, plus data-quality counters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeadScan {
    /// `deploy_ts - commit_ts` per non-bot commit, clamped at zero.
    pub samples: Vec<i64>,
    /// Parsed rows whose commit timestamp is *after* the deploy timestamp.
    pub clock_skew_commits: i64,
    /// First-parent commits in the range, including bot-authored ones.
    pub commits: i64,
}

/// Lead-time samples for one deploy range, plus the clock-skew row count.
///
/// Rows are taken from `range` with `--first-parent` in ascending commit
/// order, so for a squash-only history the measurement is
/// **commit → deploy** rather than PR-open → merge (the merge commit carries
/// both timestamps identically). The repository's merge strategy is recorded
/// in the policy file rather than assumed.
///
/// One `git log` supplies every field: samples, the range's commit count, and
/// `clock_skew_commits` (rows timestamped after their deploy — data-quality
/// evidence, not a sample).
///
/// # Errors
///
/// Returns an error when the range cannot be resolved — a shallow clone with
/// an unresolvable range must never read as zero samples.
pub fn lead_scan(
    root: &Path,
    range: &str,
    deploy_ts: i64,
    bot_allowlist: &[String],
) -> Result<LeadScan> {
    let rows = first_parent_rows(root, range)?;
    let mut samples = Vec::with_capacity(rows.len());
    let mut clock_skew_commits = 0_i64;
    for (ts, email) in &rows {
        if *ts > deploy_ts {
            clock_skew_commits += 1;
        }
        if bot_allowlist.iter().any(|bot| bot == email) {
            continue;
        }
        samples.push((deploy_ts - ts).max(0));
    }
    Ok(LeadScan {
        samples,
        clock_skew_commits,
        commits: i64::try_from(rows.len()).context("commit count does not fit in i64")?,
    })
}

/// Revert commits in `range` as `(sha, commit timestamp)`.
///
/// The subject predicate is the anchored conventional-commit type
/// (`revert(` / `revert:`), which `scripts/check-commitlint.sh` already
/// guarantees. `git log --grep` is deliberately not used: it matches commit
/// *bodies*, so a substring search returns false positives for a revert that
/// merely mentions the word. `pattern` is the policy's `revert_pattern` and is
/// re-checked here so a caller that bypasses [`policy::load`] still cannot
/// silently ignore it.
///
/// # Errors
///
/// Returns an error when `pattern` is not the single supported form, or when
/// the range cannot be resolved.
pub fn reverts(root: &Path, range: &str, pattern: &str) -> Result<Vec<(String, i64)>> {
    if pattern != SUPPORTED_REVERT_PATTERN {
        bail!(
            "unsupported revert_pattern {pattern:?}: the only implemented predicate is {SUPPORTED_REVERT_PATTERN:?}"
        );
    }
    let out = run(
        root,
        &["log", "--first-parent", "--format=%H%x09%ct%x09%s", range],
    )?;
    let mut found = Vec::new();
    for line in out.lines().filter(|line| !line.trim().is_empty()) {
        let mut fields = line.splitn(3, '\t');
        let (Some(sha), Some(ts), Some(subject)) = (fields.next(), fields.next(), fields.next())
        else {
            bail!("unexpected git log row: {line}");
        };
        if !is_revert_subject(subject) {
            continue;
        }
        let ts = ts
            .parse::<i64>()
            .with_context(|| format!("git log %ct produced a non-numeric timestamp: {ts:?}"))?;
        found.push((sha.to_string(), ts));
    }
    Ok(found)
}

/// The anchored conventional-commit revert predicate.
fn is_revert_subject(subject: &str) -> bool {
    subject.starts_with("revert(") || subject.starts_with("revert:")
}

/// One parsed `<commit timestamp>\t<author email>` first-parent row.
fn first_parent_rows(root: &Path, range: &str) -> Result<Vec<(i64, String)>> {
    let out = run(
        root,
        &["log", "--first-parent", "--format=%ct%x09%ae", range],
    )?;
    let mut rows = Vec::new();
    for line in out.lines().filter(|line| !line.trim().is_empty()) {
        let mut fields = line.splitn(2, '\t');
        let (Some(ts), Some(email)) = (fields.next(), fields.next()) else {
            bail!("unexpected git log row: {line}");
        };
        let ts = ts
            .parse::<i64>()
            .with_context(|| format!("git log %ct produced a non-numeric timestamp: {ts:?}"))?;
        rows.push((ts, email.to_string()));
    }
    Ok(rows)
}

/// Runs a git command rooted at `root`, returning stdout on success.
///
/// # Errors
///
/// Returns an error naming the failing argv and the stderr tail when git
/// exits non-zero, or when the process cannot be spawned.
fn run(root: &Path, args: &[&str]) -> Result<String> {
    let output = crate::changes::git_command(root)
        .args(args)
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        bail!("git {} failed: {stderr}", args.join(" "));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn revert_subject_predicate_is_anchored_on_the_conventional_type() {
        assert!(is_revert_subject("revert(mcp): drop the epic edit"));
        assert!(is_revert_subject("revert: drop the change"));
        // Bodies and unrelated mentions must not match.
        assert!(!is_revert_subject("docs: explain the revert workflow"));
        assert!(!is_revert_subject(
            "fix(mcp): follow up on the earlier revert"
        ));
        assert!(!is_revert_subject("reverted: nope"));
    }

    #[test]
    fn unsupported_revert_pattern_is_rejected_at_the_derivation_boundary() {
        let err = reverts(Path::new("."), "HEAD", "^revert").unwrap_err();
        assert!(err.to_string().contains("unsupported revert_pattern"));
    }

    #[test]
    fn non_work_tree_is_not_a_work_tree() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_work_tree(dir.path()));
    }
}

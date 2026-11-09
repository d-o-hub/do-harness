//! Git plumbing for `do-harness sync`.
//!
//! Every call runs through [`crate::changes::git_command`], which clears the
//! repository view git exports to hooks, so a sync started from a hook cannot
//! silently target another checkout. Nothing here writes a ref except
//! [`fetch`], [`switch_to_main`], [`fast_forward`], [`create_backup`], and
//! [`reset_hard`]; the callers decide when those are safe.

use std::path::Path;
use std::process::Output;

use anyhow::{Context, Result, bail};

use super::{BRANCH, MAX_LOCAL_COMMITS, REMOTE, SHORT_SHA_LEN};

/// Fails unless `root` is inside a git work tree.
pub(super) fn ensure_work_tree(root: &Path) -> Result<()> {
    let inside = git_text(root, &["rev-parse", "--is-inside-work-tree"])?;
    if inside == "true" {
        Ok(())
    } else {
        bail!("not inside a git work tree: {}", root.display())
    }
}

/// Fails unless the remote is configured.
pub(super) fn ensure_remote(root: &Path) -> Result<()> {
    git_text(root, &["remote", "get-url", REMOTE])
        .with_context(|| format!("remote {REMOTE} is not configured"))?;
    Ok(())
}

/// Returns the current branch name, or `None` when detached.
pub(super) fn current_branch(root: &Path) -> Result<Option<String>> {
    let branch = git_text(root, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if branch == "HEAD" {
        Ok(None)
    } else {
        Ok(Some(branch))
    }
}

/// Returns the local `main` commit, or `None` when the branch does not exist.
pub(super) fn local_branch_sha(root: &Path) -> Result<Option<String>> {
    let reference = format!("refs/heads/{BRANCH}");
    let output = run_git(root, &["rev-parse", "--verify", "--quiet", &reference])?;
    if !output.status.success() {
        return Ok(None);
    }
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    validate_object_id(&sha)?;
    Ok(Some(sha))
}

/// Whether the worktree has staged, unstaged, or untracked changes.
pub(super) fn is_clean(root: &Path) -> Result<bool> {
    Ok(git_text(root, &["status", "--porcelain"])?.is_empty())
}

/// Resolves `reference` to a commit id.
pub(super) fn rev_parse(root: &Path, reference: &str) -> Result<String> {
    let sha = git_text(root, &["rev-parse", "--verify", reference])?;
    validate_object_id(&sha)?;
    Ok(sha)
}

/// Current `HEAD` commit id.
pub(super) fn head(root: &Path) -> Result<String> {
    rev_parse(root, "HEAD")
}

/// Whether `sha` resolves to a local commit object.
pub(super) fn object_exists(root: &Path, sha: &str) -> Result<bool> {
    Ok(
        run_git(root, &["cat-file", "-e", &format!("{sha}^{{commit}}")])?
            .status
            .success(),
    )
}

/// Fetches `origin/main` into its remote-tracking ref.
pub(super) fn fetch(root: &Path) -> Result<()> {
    let refspec = format!("+refs/heads/{BRANCH}:refs/remotes/{REMOTE}/{BRANCH}");
    git_checked(root, &["fetch", "--no-tags", REMOTE, &refspec])?;
    Ok(())
}

/// Queries `origin/main` with `ls-remote`, without writing refs.
pub(super) fn ls_remote(root: &Path) -> Result<String> {
    let output = run_git(
        root,
        &[
            "ls-remote",
            "--exit-code",
            "--refs",
            REMOTE,
            &format!("refs/heads/{BRANCH}"),
        ],
    )?;
    if !output.status.success() {
        bail!(
            "git ls-remote {REMOTE} refs/heads/{BRANCH} failed: {}",
            stderr(&output)
        );
    }
    parse_ls_remote(&String::from_utf8_lossy(&output.stdout))
}

/// Parses exactly one `ls-remote` line into a validated object id.
pub(super) fn parse_ls_remote(stdout: &str) -> Result<String> {
    let mut lines = stdout.lines().filter(|line| !line.trim().is_empty());
    let line = lines
        .next()
        .ok_or_else(|| anyhow::anyhow!("ls-remote returned no ref match"))?;
    if lines.next().is_some() {
        bail!("ls-remote returned more than one ref match");
    }
    let mut columns = line.split_whitespace();
    let sha = columns
        .next()
        .ok_or_else(|| anyhow::anyhow!("malformed ls-remote line: {line}"))?;
    let reference = columns
        .next()
        .ok_or_else(|| anyhow::anyhow!("malformed ls-remote line: {line}"))?;
    if columns.next().is_some() {
        bail!("malformed ls-remote line: {line}");
    }
    validate_object_id(sha)?;
    let expected = format!("refs/heads/{BRANCH}");
    if reference != expected {
        bail!("unexpected ref match: {reference}");
    }
    Ok(sha.to_owned())
}

/// Rejects anything that is not a 40- or 64-character hexadecimal object id.
pub(super) fn validate_object_id(sha: &str) -> Result<()> {
    let shaped = (sha.len() == 40 || sha.len() == 64) && sha.chars().all(|c| c.is_ascii_hexdigit());
    if shaped {
        Ok(())
    } else {
        bail!("malformed object id: {sha}")
    }
}

/// Counts commits ahead/behind between two revisions.
pub(super) fn counts(root: &Path, left: &str, right: &str) -> Result<(usize, usize)> {
    let range = format!("{left}...{right}");
    let text = git_text(root, &["rev-list", "--left-right", "--count", &range])?;
    let mut parts = text.split_whitespace();
    let ahead = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("rev-list returned no ahead count"))?
        .parse::<usize>()
        .context("rev-list ahead count is not a number")?;
    let behind = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("rev-list returned no behind count"))?
        .parse::<usize>()
        .context("rev-list behind count is not a number")?;
    Ok((ahead, behind))
}

/// Switches to `main`, creating it from `origin/main` when absent.
pub(super) fn switch_to_main(root: &Path, exists: bool) -> Result<()> {
    if exists {
        git_checked(root, &["switch", BRANCH])?;
    } else {
        git_checked(
            root,
            &["switch", "--create", BRANCH, "--track", "origin/main"],
        )?;
    }
    Ok(())
}

/// Fast-forwards `main` onto `origin/main`.
pub(super) fn fast_forward(root: &Path) -> Result<()> {
    git_checked(
        root,
        &[
            "merge",
            "--ff-only",
            &format!("refs/remotes/{REMOTE}/{BRANCH}"),
        ],
    )?;
    Ok(())
}

/// Creates a backup branch for the pre-reset `main` commit.
pub(super) fn create_backup(root: &Path, sha: &str) -> Result<String> {
    let name = format!("backup/main-pre-sync-{}", short(sha));
    let reference = format!("refs/heads/{name}");
    if run_git(root, &["rev-parse", "--verify", "--quiet", &reference])?
        .status
        .success()
    {
        bail!("backup branch {name} already exists; resolve it before retrying");
    }
    git_checked(root, &["branch", &name, sha])?;
    Ok(name)
}

/// Hard-resets the current branch onto `origin/main`.
pub(super) fn reset_hard(root: &Path) -> Result<()> {
    git_checked(
        root,
        &[
            "reset",
            "--hard",
            &format!("refs/remotes/{REMOTE}/{BRANCH}"),
        ],
    )?;
    Ok(())
}

/// Bounded list of local commit subjects not on `origin/main`.
pub(super) fn local_commits(root: &Path) -> Result<Vec<String>> {
    let range = format!("refs/remotes/{REMOTE}/{BRANCH}..refs/heads/{BRANCH}");
    let text = git_text(
        root,
        &[
            "log",
            "--format=%h %s",
            "-n",
            &MAX_LOCAL_COMMITS.to_string(),
            &range,
        ],
    )?;
    Ok(text.lines().map(ToOwned::to_owned).collect())
}

/// Shortens an object id for display and backup names.
pub(super) fn short(sha: &str) -> String {
    sha.chars().take(SHORT_SHA_LEN).collect()
}

/// Runs `git <args>` in `root` with the inherited repository view removed.
fn run_git(root: &Path, args: &[&str]) -> Result<Output> {
    crate::changes::git_command(root)
        .args(args)
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))
}

/// Runs `git <args>` and fails with its stderr on a non-zero exit.
fn git_checked(root: &Path, args: &[&str]) -> Result<String> {
    let output = run_git(root, args)?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        bail!("git {} failed: {}", args.join(" "), stderr(&output))
    }
}

/// Runs `git <args>` and returns trimmed stdout.
fn git_text(root: &Path, args: &[&str]) -> Result<String> {
    git_checked(root, args)
}

/// Renders a command's stderr for diagnostics.
fn stderr(output: &Output) -> String {
    let text = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if text.is_empty() {
        format!("exit status {}", output.status)
    } else {
        text
    }
}

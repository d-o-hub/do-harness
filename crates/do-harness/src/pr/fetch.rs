//! Fetching what a merge-impact or ancestry proof needs (#520).
//!
//! A proof compares two commits, so both must exist locally *and share
//! ancestry*. On a full clone they do. In a fresh or shallow clone neither side
//! is guaranteed: `git clone --depth 1` fetches one commit on one branch,
//! `refs/pull/*/head` is never fetched at all, and a pull request's baseline
//! commit is usually not the tip the clone took. The proof machinery is correct
//! in that situation — it returns `UNKNOWN` with a warning — but a warning that
//! says "run `git fetch` yourself" makes the no-effect closure path unusable
//! without a manual step.
//!
//! # The part that is not obvious: presence is not enough
//!
//! Fetching the two objects by object id into a **shallow** clone does *not*
//! make the pair provable. A shallow clone carries a graft in `.git/shallow`
//! marking its tip as a root, and an object fetched by id arrives as its own
//! root; git then sees two unrelated histories:
//!
//! ```text
//! $ git merge-tree --write-tree <base> <head>
//! fatal: refusing to merge unrelated histories
//! ```
//!
//! This is reproducible against a real remote, so the fix is not "fetch the
//! objects" — it is "make the two commits connected". [`prefetch`] therefore
//! fetches the objects and then checks ancestry; when the clone is shallow and
//! the commits are disconnected, it deepens the history of the base ref until a
//! merge base exists, with a finite number of bounded attempts.
//!
//! # What this never touches
//!
//! Only `refs/remotes/pr-triage/*` is written, plus the deepening of an existing
//! remote-tracking history. The worktree, the index, the checked-out branch, and
//! `HEAD` are never modified — no `checkout`, no `switch`, no branch or tag ref
//! is created or moved. That matters because this runs inside an authorization
//! path: a fetch that could move a ref a local branch points at would make the
//! proof describe something other than the reviewed commit.
//!
//! Deepening does grow the local object database, which is the price of
//! answering the question at all; it is bounded by [`MAX_DEPTHEN_STEPS`] and
//! never converts a shallow clone into a full one.
//!
//! A fetch failure is never a pass. Every failure mode returns
//! [`PrefetchOutcome::Unavailable`] carrying the exact command to run, so the
//! caller reports `UNKNOWN` with a usable next action.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::changes::git_command;

/// The only ref namespace this module writes.
///
/// Deliberately a remote-tracking namespace rather than a branch or tag: it can
/// never be the checked-out ref, so fetching into it cannot change what the
/// worktree shows.
pub const REF_PREFIX: &str = "refs/remotes/pr-triage";

/// Upper bound on one `git fetch`, so an unreachable remote cannot hang a run.
///
/// A proof step is not worth a long wait: if the objects are not available in
/// this window, the honest answer is `UNKNOWN` with the command to run.
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(60);

/// The environment variable that overrides [`FETCH_TIMEOUT`], in whole seconds.
///
/// A real control rather than a test hook: a slow link may deserve a longer
/// window, and a CI job that would rather fail fast deserves a shorter one. A
/// value that does not parse as a positive integer is ignored in favour of the
/// default, because a typo must not silently disable the bound that keeps a
/// stalled transport from blocking a merge decision.
pub const FETCH_TIMEOUT_ENV: &str = "DO_HARNESS_FETCH_TIMEOUT_SECS";

/// Commits of history requested per deepening attempt.
///
/// A history is deepened geometrically, so the *number of round trips* stays
/// small while a large gap is still crossed. `git-fetch(1)` defines
/// `--deepen=<depth>` as commits "from the current shallow boundary", so each
/// attempt moves `DEEPEN_INCREMENT * step` commits further and the reach
/// compounds rather than repeating: with this value, four attempts cover 400
/// commits and all eight cover 1,440 (40 + 80 + … + 320). A linear step of 40
/// would need ten times as many fetches for the same gap.
pub const DEEPEN_INCREMENT: u32 = 40;

/// Maximum deepening attempts per run.
///
/// Finite on purpose. A pull request forked from an unrelated history never
/// becomes connected no matter how far the history is walked, so an unbounded
/// loop would turn "cannot prove this" into "downloads the whole repository and
/// still cannot prove it".
pub const MAX_DEPTHEN_STEPS: u32 = 8;

/// How often the bounded wait checks whether the child has exited.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// The configured fetch bound: [`FETCH_TIMEOUT_ENV`] when it names a positive
/// number of seconds, otherwise [`FETCH_TIMEOUT`].
#[must_use]
pub fn fetch_timeout() -> Duration {
    parse_timeout(std::env::var(FETCH_TIMEOUT_ENV).ok().as_deref())
}

/// Interprets a raw [`FETCH_TIMEOUT_ENV`] value.
///
/// Split out as a pure function so the fallback rules are testable without
/// mutating the process environment, which no test may do soundly: it is shared
/// with every other thread in the binary.
#[must_use]
pub fn parse_timeout(raw: Option<&str>) -> Duration {
    raw.and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|secs| *secs > 0)
        .map_or(FETCH_TIMEOUT, Duration::from_secs)
}

/// One fetch this module may perform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// Human-readable description, used in warnings.
    pub what: String,
    /// The `git fetch` arguments, without the program name.
    pub args: Vec<String>,
}

impl Step {
    /// The exact command a reader should run to reproduce this fetch.
    #[must_use]
    pub fn command(&self) -> String {
        format!("git fetch {}", self.args.join(" "))
    }
}

/// Result of a prefetch attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrefetchOutcome {
    /// Nothing was needed: both commits exist and already share ancestry.
    AlreadyPresent,
    /// At least one fetch ran and the pair is now provable.
    Fetched,
    /// A fetch was needed and failed. Carries the command and the reason.
    Unavailable {
        /// The command that failed, for the next-action message.
        command: String,
        /// Why it failed: the exit status, a timeout, or a spawn error.
        reason: String,
    },
    /// The prefetch was explicitly disabled.
    Disabled,
}

impl PrefetchOutcome {
    /// A warning line for the report, when the outcome is worth reporting.
    ///
    /// A failure is never silent and never a pass: the caller goes on to attempt
    /// the proof, which will report `UNKNOWN` on its own, and this warning is
    /// what turns that `UNKNOWN` into a next action.
    #[must_use]
    pub fn warning(&self) -> Option<String> {
        match self {
            Self::Unavailable { command, reason } => Some(format!(
                "could not fetch the commits this proof needs ({reason}); run `{command}` and retry"
            )),
            _ => None,
        }
    }
}

/// Plans the fetches that make `base_oid` and `head_oid` locally available.
///
/// Returns an empty plan when both commits already exist, so an ordinary full
/// clone issues no fetch at all — this module must not cost network on the
/// common path. The head is keyed on the *commit*, not on the [`REF_PREFIX`]
/// ref: a full clone that fetched the branch normally has the commit without
/// ever creating our namespace, and re-fetching it would be pure waste.
///
/// Availability only. Whether the pair is *connected* is a separate question,
/// answered by [`connected`]; a shallow clone can satisfy this plan and still be
/// unprovable, which is why [`prefetch`] does not stop here.
#[must_use]
pub fn plan(root: &Path, number: u64, base_oid: &str, head_oid: &str) -> Vec<Step> {
    let mut steps = Vec::new();
    if !base_oid.is_empty() && !crate::pr::no_effect::rev_exists(root, base_oid) {
        steps.push(Step {
            what: format!("base commit {base_oid}"),
            args: vec![
                "--no-tags".to_owned(),
                "origin".to_owned(),
                base_oid.to_owned(),
            ],
        });
    }
    if !head_available(root, number, head_oid) {
        steps.push(Step {
            what: format!("pull request {number} head"),
            args: vec![
                "--no-tags".to_owned(),
                "origin".to_owned(),
                format!("+refs/pull/{number}/head:{REF_PREFIX}/{number}"),
            ],
        });
    }
    steps
}

/// Whether the pull request head is already resolvable locally.
///
/// Either the reported head commit exists, or the tracked ref does — a shallow
/// clone of the pull ref can carry the ref without the commit, so both are
/// checked.
#[must_use]
pub fn head_available(root: &Path, number: u64, head_oid: &str) -> bool {
    (!head_oid.is_empty() && crate::pr::no_effect::rev_exists(root, head_oid))
        || head_present(root, number)
}

/// Whether the pull request's head ref is already tracked locally.
#[must_use]
pub fn head_present(root: &Path, number: u64) -> bool {
    crate::pr::no_effect::rev_exists(root, &format!("{REF_PREFIX}/{number}"))
}

/// Whether two commits are connected, i.e. a merge tree could be modelled.
///
/// `git merge-tree` requires a common ancestor; two objects fetched into a
/// shallow clone are each their own root, so this is false even when both exist.
#[must_use]
pub fn connected(root: &Path, base_oid: &str, head_oid: &str) -> bool {
    if base_oid.is_empty() || head_oid.is_empty() {
        return false;
    }
    git_command(root)
        .args(["merge-base", base_oid, head_oid])
        .output()
        .is_ok_and(|out| out.status.success())
}

/// Whether the repository is shallow.
#[must_use]
pub fn is_shallow(root: &Path) -> bool {
    git_command(root)
        .args(["rev-parse", "--is-shallow-repository"])
        .output()
        .is_ok_and(|out| {
            out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "true"
        })
}

/// Makes the commits [`plan`] identified available and their history connected.
///
/// Stops at the first failure: a partial prefetch cannot make a proof reliable,
/// and continuing would spend network on a result that is `UNKNOWN` anyway.
#[must_use]
pub fn prefetch(
    root: &Path,
    number: u64,
    base_oid: &str,
    head_oid: &str,
    timeout: Duration,
) -> PrefetchOutcome {
    let steps = plan(root, number, base_oid, head_oid);
    for step in &steps {
        match run_bounded(root, &step.args, timeout) {
            Ok(()) => {}
            Err(reason) => {
                return PrefetchOutcome::Unavailable {
                    command: step.command(),
                    reason: format!("{}: {reason}", step.what),
                };
            }
        }
    }
    if connected(root, base_oid, head_oid) {
        return if steps.is_empty() {
            PrefetchOutcome::AlreadyPresent
        } else {
            PrefetchOutcome::Fetched
        };
    }
    // The objects may be present yet disconnected — the shallow-graft case. Only
    // a shallow clone can be repaired by deepening; in any other repository a
    // missing merge base means the histories genuinely do not meet.
    if is_shallow(root) {
        if let Err(failure) = deepen_until_connected(root, base_oid, head_oid, timeout) {
            return failure;
        }
    }
    if connected(root, base_oid, head_oid) {
        PrefetchOutcome::Fetched
    } else {
        PrefetchOutcome::Unavailable {
            command: "git fetch --unshallow origin".to_owned(),
            reason: format!(
                "commits {base_oid} and {head_oid} have no common ancestor in this clone after \
                 deepening; a full history is required"
            ),
        }
    }
}

/// Deepens the available history until the pair connects.
///
/// Each attempt deepens by a growing multiple of [`DEEPEN_INCREMENT`] so the
/// round-trip count stays small.
///
/// No refspec is passed, so this deepens **every** remote-tracking branch, not
/// one: `git-fetch(1)` defines `--deepen=<depth>` as moving the shallow boundary
/// for "each remote branch history". That is the intended trade — a single
/// bounded request instead of one per candidate branch — but it is measurably
/// larger than a scoped fetch: on a `--depth 1 --no-single-branch` clone of 38
/// shallow branches, one `--deepen=40` took the pack from 4,769 to 9,464
/// objects and cut the shallow boundary to 3 entries. [`MAX_DEPTHEN_STEPS`]
/// bounds the number of requests, not the bytes each one moves.
fn deepen_until_connected(
    root: &Path,
    base_oid: &str,
    head_oid: &str,
    timeout: Duration,
) -> Result<(), PrefetchOutcome> {
    let mut depth = 0u32;
    for _ in 0..MAX_DEPTHEN_STEPS {
        depth += DEEPEN_INCREMENT;
        let args = vec![
            "--no-tags".to_owned(),
            format!("--deepen={depth}"),
            "origin".to_owned(),
        ];
        run_bounded(root, &args, timeout).map_err(|reason| PrefetchOutcome::Unavailable {
            command: format!("git fetch {}", args.join(" ")),
            reason: format!("deepening history to find a common ancestor: {reason}"),
        })?;
        if connected(root, base_oid, head_oid) {
            return Ok(());
        }
    }
    Err(PrefetchOutcome::Unavailable {
        command: format!(
            "git fetch {} --deepen={DEEPEN_INCREMENT} origin",
            "--no-tags"
        ),
        reason: format!(
            "no common ancestor between {base_oid} and {head_oid} after {MAX_DEPTHEN_STEPS} \
             deepening steps of {DEEPEN_INCREMENT} commits"
        ),
    })
}

/// Runs one `git fetch` with a hard bound.
///
/// The child gets its own process group on POSIX so a timeout kills a stalled
/// transport rather than leaving it holding the repository lock.
fn run_bounded(root: &Path, args: &[String], timeout: Duration) -> Result<(), String> {
    let mut command: Command = git_command(root);
    command
        .arg("fetch")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|err| format!("spawn failed: {err}"))?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => {
                // Read the error before the child is reaped away by `kill_tree`.
                let stderr = read_stderr(&mut child);
                return Err(describe_failure(status, &stderr));
            }
            Ok(None) => {}
            Err(err) => return Err(format!("wait failed: {err}")),
        }
        if Instant::now() >= deadline {
            crate::shell::kill_tree(&mut child);
            return Err(format!("timed out after {}s", timeout.as_secs()));
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Reads a failed fetch's stderr, bounded, for the diagnostic.
fn read_stderr(child: &mut std::process::Child) -> String {
    use std::io::Read;
    let Some(stderr) = child.stderr.as_mut() else {
        return String::new();
    };
    let mut buf = String::new();
    // The child has exited, so the pipe is at EOF: this cannot block. Cap the
    // size anyway, since a transport error can be verbose.
    let _ = stderr.take(4096).read_to_string(&mut buf);
    let trimmed = buf.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    format!(": {}", trimmed.lines().next().unwrap_or(trimmed))
}

/// A one-line description of a non-zero fetch exit.
fn describe_failure(status: std::process::ExitStatus, stderr: &str) -> String {
    match status.code() {
        Some(code) => format!("git fetch exited {code}{stderr}"),
        None => format!("git fetch was terminated by a signal{stderr}"),
    }
}

#[cfg(test)]
#[path = "fetch_tests.rs"]
mod tests;

//! Self-reporting of `do-harness` CLI defects to the tool's own issue tracker.
//!
//! A failure in the CLI itself is a defect in this tool, not in the repository
//! it is running inside, so it is filed against the tool's own repository. The
//! target is the tool's identity surface: `DO_HARNESS_REPO` overrides the
//! default, and the default is allowlisted in `scripts/check-repo-links.sh`.
//!
//! Reporting is best-effort and never changes the process exit code. A missing
//! `gh`, an unauthenticated session, or a network failure is reported on stderr
//! and otherwise ignored.
//!
//! An automatic report carries the CLI's own failure diagnostics, captured by
//! [`crate::diag`], so the issue shows the failing sensor's output rather than
//! only the final error line.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

use super::submit::submit_draft;
use super::types::{FeedbackDraft, SubmissionResult};
use crate::diag;
use crate::redaction::Redactor;

/// Environment variable naming the tool's own repository.
pub const TOOL_REPO_ENV: &str = "DO_HARNESS_REPO";
/// Default tool repository. This is the tool's identity, not the caller's
/// repository; `scripts/check-repo-links.sh` allowlists this file for it.
pub const DEFAULT_TOOL_REPO: &str = "d-o-hub/do-harness";
/// Environment variable selecting the automatic reporting mode.
pub const AUTO_REPORT_ENV: &str = "DO_HARNESS_AUTO_REPORT";
/// Maximum bytes of a captured log tail.
pub const MAX_LOG_BYTES: usize = 8 * 1024;
/// Maximum bytes of the rendered error chain.
pub const MAX_ERROR_BYTES: usize = 4 * 1024;
/// Maximum characters in a generated issue title.
const MAX_TITLE_CHARS: usize = 100;
/// Fingerprint prefix length, matching `feedback::draft`.
const FINGERPRINT_LEN: usize = 18;
/// Exit code reported for a panic.
const PANIC_EXIT_CODE: u8 = 101;
/// Guards the one automatic report a process may file.
static REPORTED: AtomicBool = AtomicBool::new(false);

/// How much of the CLI's own failure surface is reported automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoReport {
    /// Never report automatically.
    Off,
    /// Report panics and internal failures, not usage or sensor failures.
    Internal,
    /// Report every failure exit, except the expected in-flight state.
    All,
}

/// The CLI failure class, used to decide whether a report is warranted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureClass {
    /// Operator or environment problem (exit 2).
    Usage,
    /// A sensor or check failed (exit 1).
    Verify,
    /// The CLI panicked or failed in a way it could not classify.
    Internal,
    /// A verify run owns the evidence (exit 3): an expected state, not a defect.
    Running,
}

fn parse_mode(value: Option<&str>) -> AutoReport {
    match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
        Some("all") => AutoReport::All,
        Some("off" | "0" | "false" | "no") => AutoReport::Off,
        // Unset, empty, or unrecognised: internal failures are always defects
        // in this tool, so they are reported unless explicitly disabled.
        _ => AutoReport::Internal,
    }
}

/// Reads the automatic reporting mode from the environment.
///
/// Defaults to [`AutoReport::Internal`]: a panic or an unjudgeable run is a
/// defect in this tool, while a usage or sensor failure is usually the
/// adopter's own code or invocation.
#[must_use]
pub fn auto_report_mode() -> AutoReport {
    parse_mode(std::env::var(AUTO_REPORT_ENV).ok().as_deref())
}

/// Whether `class` should be reported under `mode`.
#[must_use]
pub fn should_report(mode: AutoReport, class: FailureClass) -> bool {
    match mode {
        AutoReport::Off => false,
        AutoReport::Internal => class == FailureClass::Internal,
        // Exit 3 is not a failure: a verify run owns the evidence and the CLI
        // is not allowed to judge it, so reporting it would file a defect for
        // every in-flight poll.
        AutoReport::All => class != FailureClass::Running,
    }
}

/// The tool's own repository, overridable with `DO_HARNESS_REPO`.
#[must_use]
pub fn tool_repo() -> String {
    std::env::var(TOOL_REPO_ENV)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_TOOL_REPO.to_owned())
}

fn clip(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_owned();
    }
    let mut end = max;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

fn excerpt(text: &str, max: usize) -> String {
    const MARKER: &str = "... [truncated]";
    if text.len() <= max {
        return text.to_owned();
    }
    let keep = max.saturating_sub(MARKER.len());
    format!("{}{MARKER}", clip(text, keep))
}

/// Reads at most [`MAX_LOG_BYTES`] from the end of `path`.
///
/// The tail is decoded lossily so a log that is not valid UTF-8 still yields a
/// usable excerpt instead of failing the report.
///
/// # Errors
///
/// Returns an error when the file cannot be opened, inspected, or read.
pub fn read_log_tail(path: &Path) -> Result<String> {
    use std::io::{Read, Seek, SeekFrom};

    let mut file =
        std::fs::File::open(path).with_context(|| format!("open log file {}", path.display()))?;
    let len = file
        .metadata()
        .with_context(|| format!("inspect log file {}", path.display()))?
        .len();
    let start = len.saturating_sub(MAX_LOG_BYTES as u64);
    file.seek(SeekFrom::Start(start))
        .with_context(|| format!("seek log file {}", path.display()))?;
    let mut buffer = Vec::new();
    file.take(MAX_LOG_BYTES as u64)
        .read_to_end(&mut buffer)
        .with_context(|| format!("read log file {}", path.display()))?;
    Ok(String::from_utf8_lossy(&buffer).into_owned())
}

/// Shortest all-hexadecimal token treated as a commit hash or digest.
const MIN_HEX_TOKEN: usize = 7;

/// Replaces volatile tokens so one defect fingerprints identically every run.
///
/// Pids, timestamps, line numbers, counts, and commit hashes all differ between
/// runs of the same defect. Left in the fingerprint they file a new issue on
/// every occurrence, which is exactly the noise this path must not create.
/// Absolute paths are collapsed too: the same defect in two checkouts differs
/// only by where it was found.
fn normalize(text: &str) -> String {
    /// Collapses absolute paths, which vary per checkout and temp directory.
    ///
    /// A `/` only starts a path at a word boundary, so `and/or` is left alone.
    fn strip_paths(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut chars = text.chars().peekable();
        let mut previous_alphanumeric = false;
        while let Some(ch) = chars.next() {
            if ch == '/' && !previous_alphanumeric {
                while let Some(&next) = chars.peek() {
                    if next.is_ascii_alphanumeric() || matches!(next, '.' | '_' | '-' | '/') {
                        chars.next();
                    } else {
                        break;
                    }
                }
                out.push_str("<path>");
                previous_alphanumeric = false;
                continue;
            }
            out.push(ch);
            previous_alphanumeric = ch.is_ascii_alphanumeric();
        }
        out
    }

    fn flush(token: &mut String, out: &mut String) {
        if token.is_empty() {
            return;
        }
        let volatile = token.chars().all(|ch| ch.is_ascii_digit())
            || (token.len() >= MIN_HEX_TOKEN && token.chars().all(|ch| ch.is_ascii_hexdigit()));
        if volatile {
            out.push('#');
        } else {
            out.push_str(token);
        }
        token.clear();
    }

    let stripped = strip_paths(text);
    let mut out = String::with_capacity(stripped.len());
    let mut token = String::new();
    for ch in stripped.chars() {
        if ch.is_ascii_alphanumeric() {
            token.push(ch);
        } else {
            flush(&mut token, &mut out);
            out.push(ch);
        }
    }
    flush(&mut token, &mut out);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Fingerprints a defect by its normalised command and error text.
///
/// The version is deliberately excluded: the same defect must link to the same
/// issue across releases, and the version is recorded in the body instead.
fn fingerprint(command: &str, error_text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(normalize(command).as_bytes());
    hasher.update(b":");
    hasher.update(normalize(error_text).as_bytes());
    format!("fp_{:x}", hasher.finalize())[..FINGERPRINT_LEN].to_owned()
}

#[allow(clippy::too_many_arguments)]
fn build_draft(
    repo: &str,
    kind: &str,
    error_text: &str,
    argv: &[String],
    exit_code: Option<u8>,
    log_tail: &str,
    redactor: &Redactor,
    now: i64,
) -> FeedbackDraft {
    let error_text = redactor.text(error_text);
    let command = redactor.text(&argv.join(" "));
    let log_tail = redactor.text(log_tail);
    let version = env!("CARGO_PKG_VERSION");
    let platform = format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH);
    let fp = fingerprint(&command, &error_text);
    let first_line = error_text.lines().next().unwrap_or("unknown failure");
    let title = clip(
        &format!("[harness:cli] {kind}: {}", normalize(first_line)),
        MAX_TITLE_CHARS,
    );
    let exit = exit_code.map_or_else(|| "n/a (manual report)".to_owned(), |code| code.to_string());
    let error_excerpt = excerpt(&error_text, MAX_ERROR_BYTES);
    let log_excerpt = if log_tail.trim().is_empty() {
        "(no log captured)".to_owned()
    } else {
        excerpt(&log_tail, MAX_LOG_BYTES)
    };
    let impact = if kind == "warning" {
        "The CLI reported a warning; the run may be degraded but not fatal."
    } else {
        "The CLI failed; every adopter running this version is affected."
    };
    let article = if kind == "error" { "an" } else { "a" };
    let body = format!(
        "## Summary\n\
         `do-harness` reported {article} {kind} while running `{command}`.\n\n\
         ## Impact\n\
         {impact}\n\n\
         ## Environment\n\
         - Version: `{version}`\n\
         - Platform: `{platform}`\n\
         - Exit code: `{exit}`\n\n\
         ## Steps to reproduce\n\
         ```bash\n\
         {command}\n\
         ```\n\n\
         ## Expected behavior\n\
         The command completes without an internal failure.\n\n\
         ## Actual behavior\n\
         ```text\n\
         {error_excerpt}\n\
         ```\n\n\
         ## Diagnostics\n\
         ```text\n\
         {log_excerpt}\n\
         ```\n\n\
         ## Acceptance criteria\n\
         - [ ] The command above completes without the reported failure.\n\
         - [ ] A regression test covers the failing path.\n\
         - [ ] `do-harness verify` passes.\n\n\
         <!-- do-harness-feedback-fingerprint: {fp} -->\n"
    );
    FeedbackDraft {
        schema_version: 1,
        fingerprint: fp,
        target_repo: repo.to_owned(),
        title,
        body,
        impact: impact.to_owned(),
        build_identity: format!("{version} ({platform})"),
        source_ids: Vec::new(),
        observed_behavior: first_line.to_owned(),
        expected_behavior: "The command completes without an internal failure.".to_owned(),
        reproduction: command,
        diagnostic_excerpt: error_excerpt,
        suggested_verification: "do-harness verify".to_owned(),
        created_at: now,
    }
}

/// Files an explicit defect report against the tool's own repository.
///
/// # Errors
///
/// Returns an error when the draft cannot be submitted: an invalid repository,
/// a missing `gh`, or a failed `gh issue create`.
#[allow(clippy::too_many_arguments)]
pub fn file_explicit(
    root: &Path,
    repo: Option<&str>,
    kind: &str,
    error_text: &str,
    argv: &[String],
    log_tail: &str,
    skip_confirmation: bool,
    dry_run: bool,
) -> Result<SubmissionResult> {
    let repo = repo.map_or_else(tool_repo, str::to_owned);
    let redactor = Redactor::from_environment(&[]);
    let draft = build_draft(
        &repo,
        kind,
        error_text,
        argv,
        None,
        log_tail,
        &redactor,
        do_harness_db::unix_now(),
    );
    submit_draft(root, &draft, Some(&repo), skip_confirmation, dry_run)
}

fn is_feedback_command(argv: &[String]) -> bool {
    argv.iter().skip(1).any(|arg| arg == "feedback")
}

/// Files a CLI failure when the configured mode selects its class.
///
/// Best-effort: returns `None` when reporting is disabled, already attempted in
/// this process, or the submission failed. The process exit code is never
/// changed.
pub fn file_failure(
    root: Option<&Path>,
    class: FailureClass,
    error_text: &str,
    argv: &[String],
    exit_code: u8,
) -> Option<SubmissionResult> {
    if !should_report(auto_report_mode(), class) || is_feedback_command(argv) {
        return None;
    }
    if REPORTED.swap(true, Ordering::SeqCst) {
        return None;
    }
    let root = root.map_or_else(
        || std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        Path::to_path_buf,
    );
    let redactor = Redactor::from_environment(&[]);
    let log_tail = diag::tail();
    let draft = build_draft(
        &tool_repo(),
        "error",
        error_text,
        argv,
        Some(exit_code),
        &log_tail,
        &redactor,
        do_harness_db::unix_now(),
    );
    // A panic here would replace the command's exit code with 101, so the
    // submission is isolated from the caller's outcome.
    let submitted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        submit_draft(&root, &draft, None, true, false)
    }));
    match submitted {
        Ok(Ok(result)) => {
            if let SubmissionResult::Created { issue_url, .. }
            | SubmissionResult::LinkedExisting { issue_url, .. } = &result
            {
                eprintln!("note: filed do-harness defect report: {issue_url}");
            }
            Some(result)
        }
        Ok(Err(error)) => {
            eprintln!("note: could not file a do-harness defect report: {error:#}");
            None
        }
        Err(_) => {
            eprintln!("note: could not file a do-harness defect report: reporting panicked");
            None
        }
    }
}

/// Installs a panic hook that files the panic as a CLI defect when enabled.
///
/// The previous hook still runs, so the panic message and backtrace are printed
/// as before.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        previous(info);
        if !should_report(auto_report_mode(), FailureClass::Internal) {
            return;
        }
        let payload = info.payload();
        let message = payload
            .downcast_ref::<&str>()
            .map(|text| (*text).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panic with a non-string payload".to_owned());
        let location = info.location().map_or_else(
            || "unknown location".to_owned(),
            |loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()),
        );
        let backtrace = std::backtrace::Backtrace::force_capture();
        let error_text = format!("panic at {location}: {message}\n\n{backtrace}");
        let argv = diag::argv();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            file_failure(
                None,
                FailureClass::Internal,
                &error_text,
                &argv,
                PANIC_EXIT_CODE,
            )
        }));
    }));
}

#[cfg(test)]
#[path = "report_tests.rs"]
mod tests;

//! `do-harness helpers commitlint`: conventional-commit gate with lowercase
//! subjects, natively.
//!
//! This is the Rust implementation of `scripts/check-commitlint.sh`; the
//! script is now a thin wrapper that delegates here. The output contract and
//! exit codes are identical, and the two are covered by the parity fixtures
//! in `tests/native_helpers.rs`.

use std::path::Path;

use crate::changes::git_command;
use crate::helpers::HelperError;

/// Type (scope)!: lowercase subject — the allowed conventional-commit types.
const TYPES: [&str; 10] = [
    "feat", "fix", "docs", "chore", "refactor", "test", "build", "ci", "perf", "revert",
];

/// Scope characters allowed by the conventional-commits rule.
fn scope_char_ok(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '/' | '-')
}

/// Whether `subject` is a merge commit line (exempt from linting).
fn is_merge(subject: &str) -> bool {
    subject
        .get("Merge".len()..)
        .and_then(|rest| rest.chars().next())
        .is_some_and(char::is_whitespace)
}

/// Lints a single subject line. Returns an error containing the exact FAIL
/// line the shell script printed for the same verdict.
fn lint_subject(subject: &str) -> Result<(), String> {
    if is_merge(subject) {
        return Ok(());
    }
    if !conventional_subject(subject) {
        return Err(format!("FAIL: non-conventional commit subject: {subject}"));
    }
    // Drop the "type(scope)!: " prefix, then require the subject to be
    // lowercase. `${subject#*: }` removes the shortest prefix through the
    // first ": "; when there is none, the whole subject is compared.
    let body = subject
        .find(": ")
        .map_or(subject, |idx| &subject[idx + 2..]);
    if body != body.to_lowercase() {
        return Err(format!("FAIL: commit subject is not lowercase: {subject}"));
    }
    Ok(())
}

/// Whether `subject` matches `^(feat|...|revert)(\([a-z0-9._/-]+\))?!?:\ ?.+$`.
fn conventional_subject(subject: &str) -> bool {
    let bytes = subject.as_bytes();
    let mut cursor = 0usize;

    // Type token: ends at the first `(`, `!`, or `:`.
    let type_end = bytes[cursor..]
        .iter()
        .position(|b| matches!(b, b'(' | b'!' | b':'))
        .map(|idx| cursor + idx)
        .unwrap_or(bytes.len());
    if cursor == type_end || !TYPES.iter().any(|&t| t == &subject[cursor..type_end]) {
        return false;
    }
    cursor = type_end;
    if cursor >= bytes.len() {
        return false;
    }

    // Optional `(scope)`.
    if bytes[cursor] == b'(' {
        let Some(close) = bytes[cursor + 1..].iter().position(|b| *b == b')') else {
            return false;
        };
        let scope_end = cursor + 1 + close;
        if cursor + 1 == scope_end || !subject[cursor + 1..scope_end].chars().all(scope_char_ok) {
            return false;
        }
        cursor = scope_end + 1;
        if cursor > bytes.len() {
            return false;
        }
    }

    // Optional `!` breaking marker.
    if cursor < bytes.len() && bytes[cursor] == b'!' {
        cursor += 1;
    }

    // `:` then optional single space then at least one character.
    if cursor >= bytes.len() || bytes[cursor] != b':' {
        return false;
    }
    cursor += 1;
    if cursor < bytes.len() && bytes[cursor] == b' ' {
        cursor += 1;
    }
    cursor < bytes.len()
}

/// commit-msg hook mode: lint a single prepared message file.
fn lint_message(message_file: &Path) -> Result<(), HelperError> {
    let text = std::fs::read_to_string(message_file).map_err(|_| {
        HelperError::Usage(format!(
            "check-commitlint: no commit message found in {}",
            message_file.display()
        ))
    })?;
    // Commit-msg files may carry leading '#' comment lines (git's template);
    // take the first non-comment, non-empty line as the subject.
    let subject = text
        .lines()
        .find(|line| !line.trim().is_empty() && !line.starts_with('#'));
    let Some(subject) = subject else {
        return Err(HelperError::Usage(format!(
            "check-commitlint: no commit message found in {}",
            message_file.display()
        )));
    };
    if let Err(msg) = lint_subject(subject) {
        eprintln!("{msg}");
        eprintln!("Conventional-commit invariant violated.");
        eprintln!("Use: 'type(scope): lowercase subject' e.g. 'fix: typo'");
        return Err(HelperError::Violation(String::new()));
    }
    println!("check-commitlint OK: subject is conventional and lowercase.");
    Ok(())
}

/// Whether `root` is a git repository (a `.git` entry, or a resolvable
/// git-dir), matching the script's guard.
fn is_git_repo(root: &Path) -> bool {
    root.join(".git").exists()
        && git_command(root)
            .args(["rev-parse", "--git-dir"])
            .output()
            .is_ok_and(|out| out.status.success())
}

/// Lints the history-window modes (`--count` / `--range`).
fn lint_window(root: &Path, count: usize, range: Option<&str>) -> Result<(), HelperError> {
    let count_text = count.to_string();
    let (log_args, log_source) = if let Some(range) = range {
        (
            vec!["log", "--no-merges", "--pretty=format:%s", range],
            format!("range {range}"),
        )
    } else {
        // The window judges the commits a branch adds, so it must not wander
        // into history that already exists on the base branch. Excluding the
        // base branch by name expresses the intent directly and is stable
        // across both local merges and GitHub's test-merge shape (see
        // scripts/check-commitlint.sh for the reasoning).
        let head = git_command(root)
            .args(["rev-parse", "-q", "--verify", "HEAD"])
            .output();
        let commits = git_command(root)
            .args(["rev-list", "--count", "--all"])
            .output();
        let no_commits = head.as_ref().is_ok_and(|out| !out.status.success())
            && commits.as_ref().is_ok_and(|out| {
                out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "0"
            });
        if no_commits {
            println!("check-commitlint OK: no commits to lint");
            return Ok(());
        }
        let mut base_ref = None;
        for candidate in ["origin/main", "main"] {
            if git_command(root)
                .args(["rev-parse", "-q", "--verify", candidate])
                .output()
                .is_ok_and(|out| out.status.success())
            {
                base_ref = Some(candidate);
                break;
            }
        }
        // Only exclude a base that is genuinely elsewhere: when the base ref
        // IS the current tip, `HEAD --not <base>` is empty and the window
        // would silently lint nothing.
        let same_tip = match (base_ref, head.as_ref()) {
            (Some(base), Ok(base_out)) if base_out.status.success() => git_command(root)
                .args(["rev-parse", base])
                .output()
                .is_ok_and(|out| out.status.success() && out.stdout == base_out.stdout),
            _ => false,
        };
        if let (Some(base), false) = (base_ref, same_tip) {
            (
                vec![
                    "log",
                    "--no-merges",
                    "-n",
                    count_text.as_str(),
                    "--pretty=format:%s",
                    "HEAD",
                    "--not",
                    base,
                ],
                format!("last {count} commit(s) not on {base}"),
            )
        } else {
            (
                vec![
                    "log",
                    "--no-merges",
                    "-n",
                    count_text.as_str(),
                    "--pretty=format:%s",
                ],
                format!("last {count} commit(s)"),
            )
        }
    };

    let output = match git_command(root).args(&log_args).output() {
        Ok(out) => out,
        Err(_) => {
            return Err(HelperError::Usage(format!(
                "check-commitlint: git log failed for the {log_source}"
            )));
        }
    };
    if !output.status.success() {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        return Err(HelperError::Usage(format!(
            "check-commitlint: git log failed for the {log_source}"
        )));
    }

    let subjects = String::from_utf8_lossy(&output.stdout);
    let mut failures = 0usize;
    let mut linted = 0usize;
    for subject in subjects.lines() {
        if subject.is_empty() {
            continue;
        }
        linted += 1;
        if let Err(msg) = lint_subject(subject) {
            eprintln!("{msg}");
            failures += 1;
        }
    }
    if failures > 0 {
        eprintln!("Conventional-commit invariant violated in the {log_source}.");
        eprintln!(
            "Use: 'type(scope): lowercase subject' e.g. 'feat(workflow): gate advance on beats'"
        );
        return Err(HelperError::Violation(String::new()));
    }
    println!(
        "check-commitlint OK: {linted} subject(s) in the {log_source} use conventional lowercase subjects."
    );
    Ok(())
}

/// Runs `do-harness helpers commitlint`.
pub fn run(
    root: &Path,
    message: Option<&Path>,
    count: Option<usize>,
    range: Option<&str>,
) -> Result<(), HelperError> {
    if let Some(message) = message {
        return lint_message(message);
    }

    // A linked worktree carries a .git *file* pointing at the main
    // repository, so a directory test alone would skip — and silently pass —
    // inside `git worktree add`.
    if !is_git_repo(root) {
        println!("check-commitlint skipped: no git repository");
        return Ok(());
    }

    if let (Some(_), Some(_)) = (count, range) {
        return Err(HelperError::Usage(
            "check-commitlint: --range and --count are mutually exclusive".to_owned(),
        ));
    }

    // `--count <N>` overrides the env default of 1; an unparseable env value
    // is a usage error naming the raw value, exactly like the script.
    let count = match count {
        Some(n) => n,
        None => match std::env::var("DO_HARNESS_COMMITLINT_COUNT") {
            Ok(raw) => match raw.parse::<usize>() {
                Ok(n) => n,
                Err(_) => {
                    return Err(HelperError::Usage(format!(
                        "check-commitlint: count must be a positive integer, got: {raw}"
                    )));
                }
            },
            Err(_) => 1,
        },
    };
    if count == 0 {
        return Err(HelperError::Usage(format!(
            "check-commitlint: count must be a positive integer, got: {count}"
        )));
    }

    lint_window(root, count, range)
}

//! Deterministic assertion grader for `do-harness eval`.
//!
//! # Assertion DSL
//!
//! Inside each fixture case's `assertions` array, only strings with one of the
//! following prefixes are *graded*; every other string is documentation and is
//! excluded from `pass_rate`. The `:` prefix marker was chosen because the
//! fixture files are JSON and colons are unambiguous there; within fields that
//! carry free text (needs/values) a secondary separator is used.
//!
//! ```text
//! exists:PATH                    PATH exists, relative to the workspace root.
//! contains:PATH|NEEDLE           PATH exists and its text contains NEEDLE.
//!                                ('|' separates path/needle; paths cannot
//!                                contain '|' on POSIX.)
//! db:TABLE:COLUMN=VALUE:min=CNT  agent_state.db has >= CNT rows in TABLE where
//!                                COLUMN = VALUE.
//! cli:ARGV:contains:TEXT         `do-harness ARGV` (split on spaces) exits 0
//!                                and its stdout contains TEXT. ARGV must not
//!                                contain --root/--config: the sandbox root is
//!                                injected by the grader and cannot be moved.
//! walk:                          the skill's evals/walkthrough.sh (run once
//!                                per skill) passed; skipped when absent.
//! ```
//!
//! Unprefixed strings are treated as human-readable expectations/docs and never
//! counted in the numerator or denominator of `pass_rate`.

use std::path::Path;

use anyhow::Result;

use crate::eval_walk::WalkRun;

/// Result of grading a single prefixed assertion.
#[derive(Debug, Clone)]
pub struct AssertionGrade {
    /// Whether the assertion passed.
    pub passed: bool,
    /// Human-readable reason for pass/fail (read by tests and future callers).
    #[allow(dead_code)]
    pub reason: String,
}

/// Whether `spec` is a graded (prefixed) assertion rather than documentation.
///
/// Prefixed assertions start with one of the reserved `key:` markers.
#[must_use]
pub fn is_graded(spec: &str) -> bool {
    spec.starts_with("exists:")
        || spec.starts_with("contains:")
        || spec.starts_with("db:")
        || spec.starts_with("cli:")
        || spec.starts_with("walk:")
}

/// Grades a single prefixed assertion against the workspace root.
///
/// `root` is the eval sandbox root: `exists`/`contains`/`db` resolve there and
/// `cli:` runs the harness binary with `--root <root>` so residue and queries
/// stay inside the sandbox. `walk` is the (already-run) walkthrough outcome.
///
/// # Errors
///
/// Returns an error when a `db:` assertion cannot reach or query the local
/// agent-state database.
pub async fn grade(root: &Path, spec: &str, walk: &WalkRun) -> Result<AssertionGrade> {
    if let Some(path) = spec.strip_prefix("exists:") {
        return Ok(grade_exists(root, path));
    }
    if let Some(rest) = spec.strip_prefix("contains:") {
        return Ok(grade_contains(root, rest).await);
    }
    if let Some(rest) = spec.strip_prefix("db:") {
        return grade_db(root, rest).await;
    }
    if let Some(rest) = spec.strip_prefix("cli:") {
        return Ok(grade_cli(root, rest).await);
    }
    if spec.starts_with("walk:") {
        return Ok(walk_success(walk, spec));
    }
    Ok(fail(format!("unknown assertion prefix: '{spec}'")))
}

/// The `exists:PATH` grader.
fn grade_exists(root: &Path, path: &str) -> AssertionGrade {
    let path = root.join(path);
    if path.is_file() || path.is_dir() {
        pass(format!("exists: {} found", path.display()))
    } else {
        fail(format!("exists: missing at {}", path.display()))
    }
}

/// The `contains:PATH|NEEDLE` grader.
async fn grade_contains(root: &Path, rest: &str) -> AssertionGrade {
    let Some((path, needle)) = rest.split_once('|') else {
        return fail("contains: expected contains:PATH|NEEDLE".to_owned());
    };
    let abs = root.join(path);
    match tokio::fs::read_to_string(&abs).await {
        Ok(contents) => {
            if contents.contains(needle) {
                pass(format!("contains: {} has '{}'", abs.display(), needle))
            } else {
                fail(format!(
                    "contains: '{}' not found in {}",
                    needle,
                    abs.display()
                ))
            }
        }
        Err(err) => fail(format!("contains: cannot read {}: {err}", abs.display())),
    }
}

/// The `db:TABLE:COLUMN=VALUE:min=CNT` grader.
async fn grade_db(root: &Path, rest: &str) -> Result<AssertionGrade> {
    let (head, min_part) = rest.split_once(":min=").unwrap_or((rest, "1"));
    let min: i64 = min_part.parse().unwrap_or(1);
    let Some((table, cond)) = head.split_once(':') else {
        return Ok(fail(
            "db: expected db:TABLE:COLUMN=VALUE:min=COUNT".to_owned(),
        ));
    };
    let Some((column, value)) = cond.split_once('=') else {
        return Ok(fail(
            "db: expected db:TABLE:COLUMN=VALUE:min=COUNT".to_owned(),
        ));
    };
    if !is_identifier(table) || !is_identifier(column) {
        return Ok(fail(format!(
            "db: invalid table/column identifier: {table}.{column}"
        )));
    }

    let db = do_harness_db::connect_and_migrate(root).await?;
    let count = match do_harness_db::count_where(&db, table, column, value).await {
        Ok(count) => count,
        Err(err) => {
            return Ok(fail(format!("db: could not query {table}: {err}")));
        }
    };
    if count >= min {
        Ok(pass(format!(
            "db: {table}.{column}={value} has {count} row(s) >= {min}"
        )))
    } else {
        Ok(fail(format!(
            "db: {table}.{column}={value} has {count} < {min} required"
        )))
    }
}

/// Whether `ident` is a safe SQL identifier (allowlist) so `db:` assertion
/// table/column names cannot break out of the quoted identifier and inject SQL.
fn is_identifier(ident: &str) -> bool {
    let mut chars = ident.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && ident.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The `cli:ARGV:contains:TEXT` grader.
///
/// Runs the harness binary with `--root root` then ARGV, so the command
/// operates against the eval sandbox, and requires exit 0 plus stdout
/// containing TEXT.
///
/// Fixture-supplied `--root`/`--config` arguments are rejected: clap's global
/// args are last-wins, so letting a fixture move the root would let the
/// fixture under test redirect the grader at the caller's real workspace.
async fn grade_cli(root: &Path, rest: &str) -> AssertionGrade {
    let Some((argv, text)) = rest.split_once(":contains:") else {
        return fail("cli: expected cli:ARGV:contains:TEXT".to_owned());
    };
    let cmd_parts: Vec<&str> = argv.split_whitespace().collect();
    if cmd_parts.is_empty() {
        return fail("cli: ARGV is empty".to_owned());
    }
    if let Some(flag) = cmd_parts.iter().find(|part| is_reserved_flag(part)) {
        return fail(format!(
            "cli: fixture argv may not override {flag}; the eval sandbox root is fixed by the grader"
        ));
    }
    let bin = binary_for_eval();
    let mut cmd = tokio::process::Command::new(&bin);
    cmd.arg("--root")
        .arg(root)
        .args(&cmd_parts)
        .env("DO_HARNESS_ROOT", root);
    let output = match cmd.output().await {
        Ok(out) => out,
        Err(err) => {
            return fail(format!("cli: could not run harness binary {bin}: {err}"));
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.success() && stdout.contains(text) {
        pass(format!(
            "cli: '{bin} --root {} {argv}' exited 0 and printed '{text}'",
            root.display()
        ))
    } else if !output.status.success() {
        fail(format!(
            "cli: 'do-harness {argv}' exited {:?}",
            output.status.code()
        ))
    } else {
        fail(format!("cli: 'do-harness {argv}' stdout lacks '{text}'"))
    }
}

/// Whether `part` is a CLI flag that would move the grader's fixed sandbox
/// root or config path (both are clap global args where last occurrence wins).
fn is_reserved_flag(part: &str) -> bool {
    part == "--root"
        || part == "--config"
        || part.starts_with("--root=")
        || part.starts_with("--config=")
}

/// Resolves the harness binary to run for `cli:` assertions.
///
/// Prefers the currently-executing `do-harness` binary (so evals drive the
/// binary under test), falling back to `CARGO_BIN_EXE_do-harness` when set by
/// the build, then `do-harness` on `PATH`.
fn binary_for_eval() -> String {
    if let Ok(exe) = std::env::current_exe() {
        return exe.display().to_string();
    }
    if let Some(exe) = option_env!("CARGO_BIN_EXE_do-harness") {
        return exe.to_owned();
    }
    "do-harness".to_owned()
}

/// The `walk:` grader: consult the already-run skill walkthrough.
fn walk_success(walk: &WalkRun, spec: &str) -> AssertionGrade {
    if walk.success {
        pass(format!("{spec} walkthrough passed"))
    } else if let Some(detail) = &walk.detail {
        fail(format!("{spec} walkthrough failed: {detail}"))
    } else {
        fail("walk: walkthrough.sh exited non-zero".to_owned())
    }
}

/// Convenience constructor for a passing grade.
fn pass(reason: String) -> AssertionGrade {
    AssertionGrade {
        passed: true,
        reason,
    }
}

fn fail(reason: String) -> AssertionGrade {
    AssertionGrade {
        passed: false,
        reason,
    }
}

#[cfg(test)]
mod tests;

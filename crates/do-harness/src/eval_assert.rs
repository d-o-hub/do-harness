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
//!                                and its stdout contains TEXT.
//! walk:                          the skill's evals/walkthrough.sh (run once
//!                                per skill) passed; skipped when absent.
//! ```
//!
//! Unprefixed strings are treated as human-readable expectations/docs and never
//! counted in the numerator or denominator of `pass_rate`.

use std::path::Path;
use std::process::Command;

use anyhow::Result;
use libsql::params;

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
        return Ok(grade_contains(root, rest));
    }
    if let Some(rest) = spec.strip_prefix("db:") {
        return grade_db(root, rest).await;
    }
    if let Some(rest) = spec.strip_prefix("cli:") {
        return Ok(grade_cli(root, rest));
    }
    if spec.starts_with("walk:") {
        return Ok(walk_success(*walk, spec));
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
fn grade_contains(root: &Path, rest: &str) -> AssertionGrade {
    let Some((path, needle)) = rest.split_once('|') else {
        return fail("contains: expected contains:PATH|NEEDLE".to_owned());
    };
    let abs = root.join(path);
    match std::fs::read_to_string(&abs) {
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
    let sql = format!("SELECT COUNT(*) FROM \"{table}\" WHERE \"{column}\" = ?1");
    let count = match db.query(&sql, params![value]).await {
        Ok(mut rows) => match rows.next().await? {
            Some(row) => row.get::<i64>(0)?,
            None => 0,
        },
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
fn grade_cli(root: &Path, rest: &str) -> AssertionGrade {
    let Some((argv, text)) = rest.split_once(":contains:") else {
        return fail("cli: expected cli:ARGV:contains:TEXT".to_owned());
    };
    let cmd_parts: Vec<&str> = argv.split_whitespace().collect();
    if cmd_parts.is_empty() {
        return fail("cli: ARGV is empty".to_owned());
    }
    let bin = binary_for_eval();
    let mut cmd = Command::new(&bin);
    cmd.arg("--root")
        .arg(root)
        .args(&cmd_parts)
        .env("DO_HARNESS_ROOT", root);
    let output = match cmd.output() {
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
fn walk_success(walk: WalkRun, spec: &str) -> AssertionGrade {
    if walk.success {
        pass(format!("{spec} walkthrough passed"))
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
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn identifier_allowlist_accepts_valid_and_rejects_injection() {
        assert!(is_identifier("tasks"));
        assert!(is_identifier("_beats"));
        assert!(is_identifier("col_1"));
        assert!(!is_identifier(""));
        assert!(!is_identifier("1tasks"));
        assert!(!is_identifier("tasks\""));
        assert!(!is_identifier("tasks; DROP"));
        assert!(!is_identifier("a-b"));
    }

    /// A malicious table identifier closes the gate as a failed assertion, not
    /// by reaching the SQL string that would inject.
    #[tokio::test(flavor = "current_thread")]
    async fn db_assertion_rejects_sql_injection_identifier() {
        let dir = tempfile::tempdir().unwrap();
        let walk = WalkRun {
            present: false,
            success: true,
        };
        let grade = grade(
            dir.path(),
            "db:tasks\"; DROP TABLE beats; --:status=done:min=1",
            &walk,
        )
        .await
        .unwrap();
        assert!(!grade.passed);
        assert!(grade.reason.contains("invalid table/column identifier"));
    }

    /// A malicious column identifier closes the gate the same way the table
    /// identifier does, even when it embeds a quote, semicolon, and comment.
    #[tokio::test(flavor = "current_thread")]
    async fn db_assertion_rejects_column_injection_payload() {
        let dir = tempfile::tempdir().unwrap();
        let walk = WalkRun {
            present: false,
            success: true,
        };
        for spec in [
            r#"db:tasks:status"; DROP -- =done:min=1"#,
            "db:tasks:=done:min=1",
        ] {
            let grade = grade(dir.path(), spec, &walk).await.unwrap();
            assert!(!grade.passed, "unexpected pass for {spec}");
            assert!(
                grade.reason.contains("invalid table/column identifier"),
                "expected allowlist rejection for {spec}: {}",
                grade.reason
            );
        }
    }

    /// Empty table and empty column names are rejected by the allowlist as a
    /// clean failed grade, never a panic or a reach into the database.
    #[tokio::test(flavor = "current_thread")]
    async fn db_assertion_empty_identifiers_fail_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let walk = WalkRun {
            present: false,
            success: true,
        };
        for spec in ["db::status=done:min=1", "db:tasks:=done:min=1"] {
            let grade = grade(dir.path(), spec, &walk).await.unwrap();
            assert!(!grade.passed, "unexpected pass for {spec}");
            assert!(grade.reason.contains("invalid table/column identifier"));
        }
    }

    /// Valid, allowlisted `db:` identifiers grade against a real temp database,
    /// and a non-numeric `:min=` degrades gracefully to the default of 1.
    #[tokio::test(flavor = "current_thread")]
    async fn db_assertion_valid_identifiers_grade_against_real_db() {
        let dir = tempfile::tempdir().unwrap();
        let conn = do_harness_db::connect_and_migrate(dir.path())
            .await
            .unwrap();
        do_harness_db::insert_beat(
            &conn,
            &do_harness_db::NewBeat {
                task_id: None,
                beat_type: "sensor",
                status: "ok",
                sensor_exit_code: Some(0),
                sensor_name: Some("hardening-test"),
                started_at: 0,
                completed_at: Some(1),
            },
        )
        .await
        .unwrap();
        let walk = WalkRun {
            present: false,
            success: true,
        };

        let matched = grade(dir.path(), "db:beats:beat_type=sensor:min=1", &walk)
            .await
            .unwrap();
        assert!(matched.passed, "{}", matched.reason);

        let non_numeric_min = grade(dir.path(), "db:beats:beat_type=sensor:min=abc", &walk)
            .await
            .unwrap();
        assert!(
            non_numeric_min.passed,
            "non-numeric min must fall back to 1: {}",
            non_numeric_min.reason
        );

        let absent = grade(dir.path(), "db:beats:beat_type=missing:min=1", &walk)
            .await
            .unwrap();
        assert!(!absent.passed, "{}", absent.reason);
    }
}

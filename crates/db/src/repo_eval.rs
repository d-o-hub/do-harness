//! Repository layer for eval-run history and per-dimension rates.

use crate::error::{DbError, Result};
use crate::migrate::unix_now;
use do_harness_types::{EvalMode, SkillEvalDimRate, SkillEvalRun};
use libsql::{Connection, params};

mod bars;

pub use bars::{
    bless_grader_baseline, get_grader_baseline, get_lift_floor, get_skill_bar, list_grader_blesses,
    raise_lift_floor, raise_skill_bar,
};

/// Insert parameters for a new skill-eval run.
#[derive(Debug, Clone)]
pub struct NewSkillEvalRun<'a> {
    /// Skill the evaluation belongs to.
    pub skill_name: &'a str,
    /// How the run executed (deterministic walkthrough or agent command).
    pub mode: EvalMode,
    /// Number of graded assertions in the run.
    pub graded: i64,
    /// Number of graded assertions that passed.
    pub passed: i64,
    /// Fraction of graded assertions that passed; `None` when nothing was
    /// graded.
    pub pass_rate: Option<f64>,
    /// Without-skill baseline pass rate for Skill Lift; `None` when lift
    /// was not measured.
    pub without_pass_rate: Option<f64>,
    /// Context-cost proxy: words in `SKILL.md` plus `references/`.
    pub skill_words: Option<i64>,
    /// Execution-cost proxy: walkthrough or agent wall time in seconds.
    pub walk_secs: Option<f64>,
}

/// Insert parameters for one dimension row of a skill-eval run.
#[derive(Debug, Clone)]
pub struct NewSkillEvalDimRate<'a> {
    /// Dimension wire name (`correctness`, `discoverability`, ...).
    pub dim: &'a str,
    /// Graded assertions in this dimension.
    pub graded: i64,
    /// Passing assertions in this dimension.
    pub passed: i64,
    /// Passing assertions in the without-skill baseline, when measured.
    pub without_passed: Option<i64>,
}

/// Appends a skill-eval run to the history table and returns its id.
///
/// # Errors
///
/// Returns an error when the insert statement fails.
pub async fn insert_skill_eval_run(conn: &Connection, run: &NewSkillEvalRun<'_>) -> Result<i64> {
    let mut rows = conn
        .query(
            "INSERT INTO skill_eval_runs \
             (skill_name, mode, graded, passed, pass_rate, without_pass_rate, \
              skill_words, walk_secs, ran_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
             RETURNING id",
            params!(
                run.skill_name,
                run.mode.as_str(),
                run.graded,
                run.passed,
                run.pass_rate,
                run.without_pass_rate,
                run.skill_words,
                run.walk_secs,
                unix_now()
            ),
        )
        .await?;
    let row = rows
        .next()
        .await?
        .ok_or_else(|| DbError::NotFound("run id vanished after insert".to_string()))?;
    Ok(row.get(0)?)
}

/// Lists a skill's evaluation runs in insertion order (oldest first).
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn list_skill_eval_runs(
    conn: &Connection,
    skill_name: &str,
) -> Result<Vec<SkillEvalRun>> {
    list_skill_eval_runs_page(conn, skill_name, -1, 0).await
}

/// Lists a skill's evaluation runs with `LIMIT`/`OFFSET` paging (`limit = -1`
/// disables the limit).
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn list_skill_eval_runs_page(
    conn: &Connection,
    skill_name: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<SkillEvalRun>> {
    let mut rows = conn
        .query(
            "SELECT id, skill_name, mode, graded, passed, pass_rate, without_pass_rate, \
              skill_words, walk_secs, ran_at \
             FROM skill_eval_runs WHERE skill_name = ?1 ORDER BY id LIMIT ?2 OFFSET ?3",
            params!(skill_name, limit, offset),
        )
        .await?;
    let mut runs = Vec::new();
    while let Some(row) = rows.next().await? {
        runs.push(run_from_row(&row)?);
    }
    Ok(runs)
}

/// Maps one `skill_eval_runs` row (`id`, `skill_name`, `mode`, `graded`,
/// `passed`, `pass_rate`, `without_pass_rate`, `skill_words`, `walk_secs`,
/// `ran_at`) to a [`SkillEvalRun`]. An unknown mode string degrades to the
/// default rather than failing the read.
fn run_from_row(row: &libsql::Row) -> Result<SkillEvalRun> {
    let mode_raw: String = row.get(2)?;
    Ok(SkillEvalRun {
        id: row.get(0)?,
        skill_name: row.get(1)?,
        mode: mode_raw.parse().unwrap_or_default(),
        graded: row.get(3)?,
        passed: row.get(4)?,
        pass_rate: row.get(5)?,
        without_pass_rate: row.get(6)?,
        skill_words: row.get(7)?,
        walk_secs: row.get(8)?,
        ran_at: row.get(9)?,
    })
}

/// Returns a skill's most recent eval run, if any.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn latest_eval_run(conn: &Connection, skill_name: &str) -> Result<Option<SkillEvalRun>> {
    let mut rows = conn
        .query(
            "SELECT id, skill_name, mode, graded, passed, pass_rate, without_pass_rate, \
              skill_words, walk_secs, ran_at \
             FROM skill_eval_runs WHERE skill_name = ?1 ORDER BY id DESC LIMIT 1",
            params!(skill_name),
        )
        .await?;
    match rows.next().await? {
        Some(row) => Ok(Some(run_from_row(&row)?)),
        None => Ok(None),
    }
}

/// Appends per-dimension breakdown rows for one eval run.
///
/// # Errors
///
/// Returns an error when the insert statement fails.
pub async fn insert_dim_rates(
    conn: &Connection,
    run_id: i64,
    rates: &[NewSkillEvalDimRate<'_>],
) -> Result<()> {
    for rate in rates {
        conn.execute(
            "INSERT INTO skill_eval_dim_rates \
             (run_id, dim, graded, passed, without_passed) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params!(
                run_id,
                rate.dim,
                rate.graded,
                rate.passed,
                rate.without_passed
            ),
        )
        .await?;
    }
    Ok(())
}

/// Lists the per-dimension breakdown rows of one eval run, ordered by
/// dimension name.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn dim_rates_for_run(conn: &Connection, run_id: i64) -> Result<Vec<SkillEvalDimRate>> {
    let mut rows = conn
        .query(
            "SELECT run_id, dim, graded, passed, without_passed \
             FROM skill_eval_dim_rates WHERE run_id = ?1 ORDER BY dim",
            params!(run_id),
        )
        .await?;
    let mut rates = Vec::new();
    while let Some(row) = rows.next().await? {
        rates.push(SkillEvalDimRate {
            run_id: row.get(0)?,
            dim: row.get(1)?,
            graded: row.get(2)?,
            passed: row.get(3)?,
            without_passed: row.get(4)?,
        });
    }
    Ok(rates)
}

/// Per-skill aggregate over the append-only `skill_eval_runs` history,
/// computed in SQL so `metrics` does not issue one query per skill.
#[derive(Debug, Clone, PartialEq)]
pub struct SkillEvalSummary {
    /// Skill name.
    pub skill_name: String,
    /// Number of recorded runs.
    pub runs: i64,
    /// Best pass rate across the history.
    pub best_pass_rate: Option<f64>,
    /// Most recent pass rate in the history.
    pub latest_pass_rate: Option<f64>,
}

/// Aggregates run counts and pass rates per skill, optionally ignoring runs
/// older than `since` (Unix seconds; [`None`] means all history).
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn skill_eval_summary(
    conn: &Connection,
    since: Option<i64>,
) -> Result<Vec<SkillEvalSummary>> {
    let mut rows = conn
        .query(
            "SELECT r.skill_name, COUNT(*), MAX(r.pass_rate), \
               (SELECT latest.pass_rate FROM skill_eval_runs latest \
                WHERE latest.skill_name = r.skill_name ORDER BY latest.id DESC LIMIT 1) \
             FROM skill_eval_runs r \
             WHERE (?1 IS NULL OR r.ran_at >= ?1) \
             GROUP BY r.skill_name ORDER BY r.skill_name",
            params!(since),
        )
        .await?;
    let mut summaries = Vec::new();
    while let Some(row) = rows.next().await? {
        summaries.push(SkillEvalSummary {
            skill_name: row.get(0)?,
            runs: row.get(1)?,
            best_pass_rate: row.get(2)?,
            latest_pass_rate: row.get(3)?,
        });
    }
    Ok(summaries)
}

/// The highest recorded pass rate across a skill's history, if any.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn max_pass_rate(conn: &Connection, skill_name: &str) -> Result<Option<f64>> {
    let mut rows = conn
        .query(
            "SELECT MAX(pass_rate) FROM skill_eval_runs WHERE skill_name = ?1",
            params!(skill_name),
        )
        .await?;
    match rows.next().await? {
        Some(row) => Ok(row.get(0)?),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests;

//! Repository layer for eval-run history, skill bars, and grader baselines.

use crate::error::{DbError, Result};
use crate::migrate::unix_now;
use do_harness_types::{GraderBaseline, SkillEvalBless, SkillEvalDimRate, SkillEvalRun};
use libsql::{Connection, params};

/// Insert parameters for a new skill-eval run.
#[derive(Debug, Clone)]
pub struct NewSkillEvalRun<'a> {
    /// Skill the evaluation belongs to.
    pub skill_name: &'a str,
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
    /// Execution-cost proxy: walkthrough wall time in seconds.
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
             (skill_name, graded, passed, pass_rate, without_pass_rate, \
              skill_words, walk_secs, ran_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
             RETURNING id",
            params!(
                run.skill_name,
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
            "SELECT id, skill_name, graded, passed, pass_rate, without_pass_rate, \
              skill_words, walk_secs, ran_at \
             FROM skill_eval_runs WHERE skill_name = ?1 ORDER BY id LIMIT ?2 OFFSET ?3",
            params!(skill_name, limit, offset),
        )
        .await?;
    let mut runs = Vec::new();
    while let Some(row) = rows.next().await? {
        runs.push(SkillEvalRun {
            id: row.get(0)?,
            skill_name: row.get(1)?,
            graded: row.get(2)?,
            passed: row.get(3)?,
            pass_rate: row.get(4)?,
            without_pass_rate: row.get(5)?,
            skill_words: row.get(6)?,
            walk_secs: row.get(7)?,
            ran_at: row.get(8)?,
        });
    }
    Ok(runs)
}

/// Returns a skill's most recent eval run, if any.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn latest_eval_run(conn: &Connection, skill_name: &str) -> Result<Option<SkillEvalRun>> {
    let mut rows = conn
        .query(
            "SELECT id, skill_name, graded, passed, pass_rate, without_pass_rate, \
              skill_words, walk_secs, ran_at \
             FROM skill_eval_runs WHERE skill_name = ?1 ORDER BY id DESC LIMIT 1",
            params!(skill_name),
        )
        .await?;
    match rows.next().await? {
        Some(row) => Ok(Some(SkillEvalRun {
            id: row.get(0)?,
            skill_name: row.get(1)?,
            graded: row.get(2)?,
            passed: row.get(3)?,
            pass_rate: row.get(4)?,
            without_pass_rate: row.get(5)?,
            skill_words: row.get(6)?,
            walk_secs: row.get(7)?,
            ran_at: row.get(8)?,
        })),
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

/// Returns a skill's blessed pass-rate floor, if one has been set.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn get_skill_bar(conn: &Connection, skill_name: &str) -> Result<Option<f64>> {
    let mut rows = conn
        .query(
            "SELECT floor FROM skill_bars WHERE skill_name = ?1",
            params!(skill_name),
        )
        .await?;
    match rows.next().await? {
        Some(row) => Ok(Some(row.get(0)?)),
        None => Ok(None),
    }
}

/// Raises a skill's pass-rate floor to `floor`, never lowering it.
///
/// Returns whether the bar moved (`false` when the existing floor was already
/// at or above `floor`).
///
/// # Errors
///
/// Returns an error when the upsert statement fails.
pub async fn raise_skill_bar(conn: &Connection, skill_name: &str, floor: f64) -> Result<bool> {
    let updated = conn
        .execute(
            "INSERT INTO skill_bars (skill_name, floor, updated_at) VALUES (?1, ?2, ?3) \
             ON CONFLICT(skill_name) DO UPDATE SET \
               floor = excluded.floor, updated_at = excluded.updated_at \
             WHERE excluded.floor > skill_bars.floor",
            params!(skill_name, floor, unix_now()),
        )
        .await?;
    Ok(updated > 0)
}

/// Returns a skill's blessed Skill Lift floor, if one has been set.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn get_lift_floor(conn: &Connection, skill_name: &str) -> Result<Option<f64>> {
    let mut rows = conn
        .query(
            "SELECT floor FROM skill_lift_floors WHERE skill_name = ?1",
            params!(skill_name),
        )
        .await?;
    match rows.next().await? {
        Some(row) => Ok(Some(row.get(0)?)),
        None => Ok(None),
    }
}

/// Raises a skill's Skill Lift floor to `floor`, never lowering it.
///
/// Returns whether the floor moved (`false` when the existing floor was
/// already at or above `floor`).
///
/// # Errors
///
/// Returns an error when the upsert statement fails.
pub async fn raise_lift_floor(conn: &Connection, skill_name: &str, floor: f64) -> Result<bool> {
    let updated = conn
        .execute(
            "INSERT INTO skill_lift_floors (skill_name, floor, updated_at) VALUES (?1, ?2, ?3) \
             ON CONFLICT(skill_name) DO UPDATE SET \
               floor = excluded.floor, updated_at = excluded.updated_at \
             WHERE excluded.floor > skill_lift_floors.floor",
            params!(skill_name, floor, unix_now()),
        )
        .await?;
    Ok(updated > 0)
}

/// Returns a skill's grader baseline, if it has been blessed.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn get_grader_baseline(
    conn: &Connection,
    skill_name: &str,
) -> Result<Option<GraderBaseline>> {
    let mut rows = conn
        .query(
            "SELECT skill_name, walkthrough_sha, specs_sha, blessed_at \
             FROM grader_baselines WHERE skill_name = ?1",
            params!(skill_name),
        )
        .await?;
    match rows.next().await? {
        Some(row) => Ok(Some(GraderBaseline {
            skill_name: row.get(0)?,
            walkthrough_sha: row.get(1)?,
            specs_sha: row.get(2)?,
            blessed_at: row.get(3)?,
        })),
        None => Ok(None),
    }
}

/// Upserts a skill's grader baseline and appends an immutable bless-history
/// row recording `approver` and optional `reason`, in one transaction.
///
/// # Errors
///
/// Returns an error when the upsert, history append, or transaction fails.
pub async fn bless_grader_baseline(
    conn: &Connection,
    skill_name: &str,
    walkthrough_sha: &str,
    specs_sha: &str,
    approver: &str,
    reason: Option<&str>,
) -> Result<()> {
    let now = unix_now();
    let tx = conn.transaction().await?;
    tx.execute(
        "INSERT INTO grader_baselines (skill_name, walkthrough_sha, specs_sha, blessed_at) \
         VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT(skill_name) DO UPDATE SET \
           walkthrough_sha = excluded.walkthrough_sha, \
           specs_sha = excluded.specs_sha, \
           blessed_at = excluded.blessed_at",
        params!(skill_name, walkthrough_sha, specs_sha, now),
    )
    .await?;
    tx.execute(
        "INSERT INTO skill_eval_blesses \
         (skill_name, walkthrough_sha, specs_sha, approver, reason, blessed_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params!(
            skill_name,
            walkthrough_sha,
            specs_sha,
            approver,
            reason,
            now
        ),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Lists a skill's bless history in approval order (oldest first).
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn list_grader_blesses(
    conn: &Connection,
    skill_name: &str,
) -> Result<Vec<SkillEvalBless>> {
    let mut rows = conn
        .query(
            "SELECT id, skill_name, walkthrough_sha, specs_sha, approver, reason, blessed_at \
             FROM skill_eval_blesses WHERE skill_name = ?1 ORDER BY id",
            params!(skill_name),
        )
        .await?;
    let mut blesses = Vec::new();
    while let Some(row) = rows.next().await? {
        blesses.push(SkillEvalBless {
            id: row.get(0)?,
            skill_name: row.get(1)?,
            walkthrough_sha: row.get(2)?,
            specs_sha: row.get(3)?,
            approver: row.get(4)?,
            reason: row.get(5)?,
            blessed_at: row.get(6)?,
        });
    }
    Ok(blesses)
}

#[cfg(test)]
mod tests;

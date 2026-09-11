//! Repository layer for eval-run history, skill bars, and grader baselines.

use crate::error::{DbError, Result};
use crate::migrate::unix_now;
use do_harness_types::{GraderBaseline, SkillEvalBless, SkillEvalRun};
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
}

/// Appends a skill-eval run to the history table and returns its id.
///
/// # Errors
///
/// Returns an error when the insert statement fails.
pub async fn insert_skill_eval_run(conn: &Connection, run: &NewSkillEvalRun<'_>) -> Result<i64> {
    let mut rows = conn
        .query(
            "INSERT INTO skill_eval_runs (skill_name, graded, passed, pass_rate, ran_at) \
             VALUES (?1, ?2, ?3, ?4, ?5) \
             RETURNING id",
            params!(
                run.skill_name,
                run.graded,
                run.passed,
                run.pass_rate,
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
            "SELECT id, skill_name, graded, passed, pass_rate, ran_at \
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
            ran_at: row.get(5)?,
        });
    }
    Ok(runs)
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
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn connect() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(crate::root::db_path(dir.path()).parent().unwrap()).unwrap();
        dir
    }

    async fn open(dir: &tempfile::TempDir) -> Connection {
        crate::migrate::connect_and_migrate(dir.path())
            .await
            .unwrap()
    }

    #[tokio::test(flavor = "current_thread")]
    async fn eval_runs_append_and_list_in_order() {
        let dir = connect();
        let conn = open(&dir).await;
        insert_skill_eval_run(
            &conn,
            &NewSkillEvalRun {
                skill_name: "harness",
                graded: 4,
                passed: 3,
                pass_rate: Some(0.75),
            },
        )
        .await
        .unwrap();
        insert_skill_eval_run(
            &conn,
            &NewSkillEvalRun {
                skill_name: "harness",
                graded: 4,
                passed: 4,
                pass_rate: Some(1.0),
            },
        )
        .await
        .unwrap();

        let runs = list_skill_eval_runs(&conn, "harness").await.unwrap();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].passed, 3);
        assert_eq!(runs[1].passed, 4);
        assert!(
            list_skill_eval_runs(&conn, "other")
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn skill_bar_never_lowers() {
        let dir = connect();
        let conn = open(&dir).await;
        assert!(get_skill_bar(&conn, "harness").await.unwrap().is_none());
        assert!(raise_skill_bar(&conn, "harness", 0.9).await.unwrap());
        assert_eq!(get_skill_bar(&conn, "harness").await.unwrap(), Some(0.9));
        // A lower floor is refused; the stored bar stays at 0.95 after a raise.
        assert!(!raise_skill_bar(&conn, "harness", 0.5).await.unwrap());
        assert_eq!(get_skill_bar(&conn, "harness").await.unwrap(), Some(0.9));
        assert!(raise_skill_bar(&conn, "harness", 0.95).await.unwrap());
        assert_eq!(get_skill_bar(&conn, "harness").await.unwrap(), Some(0.95));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn grader_baseline_upserts_per_skill() {
        let dir = connect();
        let conn = open(&dir).await;
        assert!(
            get_grader_baseline(&conn, "harness")
                .await
                .unwrap()
                .is_none()
        );
        bless_grader_baseline(
            &conn,
            "harness",
            "aaa",
            "bbb",
            "approver@example.test",
            None,
        )
        .await
        .unwrap();
        bless_grader_baseline(
            &conn,
            "harness",
            "ccc",
            "ddd",
            "approver@example.test",
            None,
        )
        .await
        .unwrap();
        let baseline = get_grader_baseline(&conn, "harness")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(baseline.walkthrough_sha, "ccc");
        assert_eq!(baseline.specs_sha, "ddd");

        // Latest-wins baseline, append-only history: both blesses remain.
        let history = list_grader_blesses(&conn, "harness").await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].walkthrough_sha, "aaa");
        assert_eq!(history[1].walkthrough_sha, "ccc");
        assert_eq!(history[1].approver, "approver@example.test");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn max_pass_rate_tracks_history() {
        let dir = connect();
        let conn = open(&dir).await;
        assert_eq!(max_pass_rate(&conn, "none").await.unwrap(), None);
        insert_skill_eval_run(
            &conn,
            &NewSkillEvalRun {
                skill_name: "harness",
                graded: 2,
                passed: 1,
                pass_rate: Some(0.5),
            },
        )
        .await
        .unwrap();
        insert_skill_eval_run(
            &conn,
            &NewSkillEvalRun {
                skill_name: "harness",
                graded: 2,
                passed: 2,
                pass_rate: Some(1.0),
            },
        )
        .await
        .unwrap();
        assert_eq!(max_pass_rate(&conn, "harness").await.unwrap(), Some(1.0));
    }

    /// The SQL summary matches the per-run aggregation and honors `since`.
    #[tokio::test(flavor = "current_thread")]
    async fn skill_eval_summary_aggregates_history() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::migrate::connect_and_migrate(dir.path())
            .await
            .unwrap();
        for rate in [0.5, 1.0, 0.75] {
            insert_skill_eval_run(
                &conn,
                &NewSkillEvalRun {
                    skill_name: "harness",
                    graded: 4,
                    passed: 3,
                    pass_rate: Some(rate),
                },
            )
            .await
            .unwrap();
        }

        let summary = skill_eval_summary(&conn, None).await.unwrap();
        assert_eq!(summary.len(), 1);
        assert_eq!(summary[0].runs, 3);
        assert_eq!(summary[0].best_pass_rate, Some(1.0));
        assert_eq!(summary[0].latest_pass_rate, Some(0.75));

        // A future cutoff filters every run out.
        let empty = skill_eval_summary(&conn, Some(i64::MAX)).await.unwrap();
        assert!(empty.is_empty());
    }
}

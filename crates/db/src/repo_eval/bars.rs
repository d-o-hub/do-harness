//! Skill bars, lift floors, and grader baselines.

use crate::error::Result;
use crate::migrate::unix_now;
use do_harness_types::{EvalMode, GraderBaseline, SkillEvalBless};
use libsql::{Connection, params};

/// Returns a skill's blessed pass-rate floor for `mode`, if one is set.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn get_skill_bar(
    conn: &Connection,
    skill_name: &str,
    mode: EvalMode,
) -> Result<Option<f64>> {
    let mut rows = conn
        .query(
            "SELECT floor FROM skill_bars WHERE skill_name = ?1 AND mode = ?2",
            params!(skill_name, mode.as_str()),
        )
        .await?;
    match rows.next().await? {
        Some(row) => Ok(Some(row.get(0)?)),
        None => Ok(None),
    }
}

/// Raises a skill's pass-rate floor for `mode` to `floor`, never lowering it.
///
/// Returns whether the bar moved (`false` when the existing floor was already
/// at or above `floor`).
///
/// # Errors
///
/// Returns an error when the upsert statement fails.
pub async fn raise_skill_bar(
    conn: &Connection,
    skill_name: &str,
    mode: EvalMode,
    floor: f64,
) -> Result<bool> {
    let updated = conn
        .execute(
            "INSERT INTO skill_bars (skill_name, mode, floor, updated_at) \
             VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(skill_name, mode) DO UPDATE SET \
               floor = excluded.floor, updated_at = excluded.updated_at \
             WHERE excluded.floor > skill_bars.floor",
            params!(skill_name, mode.as_str(), floor, unix_now()),
        )
        .await?;
    Ok(updated > 0)
}

/// Returns a skill's blessed Skill Lift floor for `mode`, if one is set.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn get_lift_floor(
    conn: &Connection,
    skill_name: &str,
    mode: EvalMode,
) -> Result<Option<f64>> {
    let mut rows = conn
        .query(
            "SELECT floor FROM skill_lift_floors WHERE skill_name = ?1 AND mode = ?2",
            params!(skill_name, mode.as_str()),
        )
        .await?;
    match rows.next().await? {
        Some(row) => Ok(Some(row.get(0)?)),
        None => Ok(None),
    }
}

/// Raises a skill's Skill Lift floor for `mode` to `floor`, never lowering it.
///
/// Returns whether the floor moved (`false` when the existing floor was
/// already at or above `floor`).
///
/// # Errors
///
/// Returns an error when the upsert statement fails.
pub async fn raise_lift_floor(
    conn: &Connection,
    skill_name: &str,
    mode: EvalMode,
    floor: f64,
) -> Result<bool> {
    let updated = conn
        .execute(
            "INSERT INTO skill_lift_floors (skill_name, mode, floor, updated_at) \
             VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(skill_name, mode) DO UPDATE SET \
               floor = excluded.floor, updated_at = excluded.updated_at \
             WHERE excluded.floor > skill_lift_floors.floor",
            params!(skill_name, mode.as_str(), floor, unix_now()),
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

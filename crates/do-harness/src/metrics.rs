//! `do-harness metrics`: longitudinal harness trends.
//!
//! Recursive self-improvement requires measuring the curve, not just the
//! latest score: sensor failure rates over time, open fail-fast strikes, and
//! per-skill eval history with their blessed bars.

use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use crate::report::Format;

/// Per-skill evaluation trend.
#[derive(Debug, Clone, Serialize)]
pub struct SkillTrend {
    /// Skill name.
    pub name: String,
    /// Latest persisted pass rate (read model).
    pub latest_pass_rate: Option<f64>,
    /// Best pass rate across all recorded runs.
    pub best_pass_rate: Option<f64>,
    /// Number of recorded eval runs.
    pub runs: i64,
    /// Blessed bar floor, when set.
    pub bar_floor: Option<f64>,
}

/// The full metrics snapshot.
#[derive(Debug, Clone, Serialize)]
pub struct MetricsSnapshot {
    /// Per-sensor beat statistics.
    pub sensors: Vec<do_harness_db::SensorStat>,
    /// Open error signatures (fail-fast strikes), worst first.
    pub strikes: Vec<do_harness_types::ErrorSignature>,
    /// Per-skill eval trends.
    pub skills: Vec<SkillTrend>,
}

/// Collects and prints the harness metrics snapshot.
///
/// # Errors
///
/// Returns an error when the state database cannot be initialized or queried.
pub async fn run_metrics(root: &Path, format: Format) -> Result<()> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let sensors = do_harness_db::sensor_stats(&conn).await?;
    let strikes = do_harness_db::list_error_signatures(&conn, None).await?;
    let mut skills = Vec::new();
    for skill in do_harness_db::list_all_skill_evals(&conn).await? {
        let runs = do_harness_db::list_skill_eval_runs(&conn, &skill.skill_name).await?;
        skills.push(SkillTrend {
            name: skill.skill_name.clone(),
            latest_pass_rate: skill.pass_rate,
            best_pass_rate: runs
                .iter()
                .filter_map(|run| run.pass_rate)
                .fold(None::<f64>, |best, rate| {
                    best.map_or(Some(rate), |b| Some(b.max(rate)))
                }),
            runs: i64::try_from(runs.len()).unwrap_or(i64::MAX),
            bar_floor: do_harness_db::get_skill_bar(&conn, &skill.skill_name).await?,
        });
    }
    // Keep a stable order even when the DB returns rows arbitrarily.
    skills.sort_by(|a, b| a.name.cmp(&b.name));

    let snapshot = MetricsSnapshot {
        sensors,
        strikes,
        skills,
    };
    match format {
        Format::Text => print_text(&snapshot),
        Format::Json => println!("{}", serde_json::to_string_pretty(&snapshot)?),
    }
    Ok(())
}

fn print_text(snapshot: &MetricsSnapshot) {
    println!("sensors:");
    if snapshot.sensors.is_empty() {
        println!("  (no recorded beats; run verify --record)");
    }
    for stat in &snapshot.sensors {
        println!(
            "  {:<12} runs={} failures={}",
            stat.name, stat.runs, stat.failures
        );
    }
    println!("strikes:");
    if snapshot.strikes.is_empty() {
        println!("  (no open error signatures)");
    }
    for sig in &snapshot.strikes {
        let scope = sig
            .task_id
            .map_or_else(|| "global".to_owned(), |task_id| format!("task {task_id}"));
        println!("  {} [{}] x{}", sig.signature, scope, sig.attempt_count);
    }
    println!("skills:");
    if snapshot.skills.is_empty() {
        println!("  (no recorded skill evals; run do-harness eval)");
    }
    for trend in &snapshot.skills {
        let latest = trend
            .latest_pass_rate
            .map_or_else(|| "-".to_owned(), |rate| format!("{rate:.2}"));
        let best = trend
            .best_pass_rate
            .map_or_else(|| "-".to_owned(), |rate| format!("{rate:.2}"));
        let bar = trend
            .bar_floor
            .map_or_else(|| "unset".to_owned(), |floor| format!("{floor:.2}"));
        println!(
            "  {:<16} latest={latest} best={best} runs={} bar={bar}",
            trend.name, trend.runs
        );
    }
}

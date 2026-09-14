//! `do-harness metrics`: longitudinal harness trends.

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
    /// Latest Skill Lift in points (with minus without), when measured.
    pub lift: Option<f64>,
    /// Blessed lift floor, when set.
    pub lift_floor: Option<f64>,
    /// Context-cost proxy of the latest run: skill words loaded.
    pub skill_words: Option<i64>,
    /// Execution-cost proxy of the latest run: walkthrough seconds.
    pub walk_secs: Option<f64>,
    /// Per-dimension rates of the latest run.
    pub dims: Vec<DimTrend>,
}

/// Per-dimension rate within the latest run of a skill.
#[derive(Debug, Clone, Serialize)]
pub struct DimTrend {
    /// Dimension wire name.
    pub dim: String,
    /// With-skill pass rate, when the dimension graded anything.
    pub rate: Option<f64>,
    /// Without-skill pass rate, when the baseline graded anything.
    pub without_rate: Option<f64>,
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
/// Returns an error when `--since` is not a Unix timestamp, or the state
/// database cannot be read.
pub async fn run_metrics(
    root: &Path,
    format: Format,
    sensor_filter: Option<&str>,
    skill_filter: Option<&str>,
    since_filter: Option<&str>,
) -> Result<()> {
    let since = match since_filter {
        Some(raw) => Some(raw.parse::<i64>().map_err(|_| {
            anyhow::anyhow!("invalid --since '{raw}': expected a Unix timestamp in seconds")
        })?),
        None => None,
    };
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let mut sensors = do_harness_db::sensor_stats(&conn, since).await?;
    if let Some(s) = sensor_filter {
        sensors.retain(|st| st.name == s);
    }

    let strikes = do_harness_db::list_error_signatures(&conn, None).await?;
    let latest_by_skill: std::collections::HashMap<String, Option<f64>> =
        do_harness_db::list_all_skill_evals(&conn)
            .await?
            .into_iter()
            .map(|eval| (eval.skill_name, eval.pass_rate))
            .collect();
    let mut skills = Vec::new();
    for summary in do_harness_db::skill_eval_summary(&conn, since).await? {
        if let Some(sk) = skill_filter {
            if summary.skill_name != sk {
                continue;
            }
        }
        let latest_run = do_harness_db::latest_eval_run(&conn, &summary.skill_name).await?;
        let (lift, skill_words, walk_secs, dims) = match latest_run {
            Some(run) => {
                let lift = match (run.pass_rate, run.without_pass_rate) {
                    (Some(with), Some(without)) => Some(with - without),
                    _ => None,
                };
                // The baseline grades the same assertions, so the with-run
                // denominator serves both rates.
                let mut dims = Vec::new();
                for rate in do_harness_db::dim_rates_for_run(&conn, run.id).await? {
                    dims.push(DimTrend {
                        rate: dim_rate(rate.passed, rate.graded),
                        without_rate: rate.without_passed.and_then(|passed| {
                            (rate.graded > 0)
                                .then(|| dim_rate(passed, rate.graded))
                                .flatten()
                        }),
                        dim: rate.dim,
                    });
                }
                (lift, run.skill_words, run.walk_secs, dims)
            }
            None => (None, None, None, Vec::new()),
        };
        skills.push(SkillTrend {
            latest_pass_rate: latest_by_skill.get(&summary.skill_name).copied().flatten(),
            bar_floor: do_harness_db::get_skill_bar(&conn, &summary.skill_name).await?,
            name: summary.skill_name.clone(),
            best_pass_rate: summary.best_pass_rate,
            runs: summary.runs,
            lift,
            lift_floor: do_harness_db::get_lift_floor(&conn, &summary.skill_name).await?,
            skill_words,
            walk_secs,
            dims,
        });
    }
    skills.sort_by(|a, b| a.name.cmp(&b.name));

    let snapshot = MetricsSnapshot {
        sensors,
        strikes,
        skills,
    };
    match format {
        Format::Text => print_text(&snapshot),
        Format::Json => println!("{}", serde_json::to_string(&snapshot)?),
    }
    Ok(())
}

/// Pass rate for small assertion counts; `None` when nothing was graded.
#[allow(clippy::cast_precision_loss)]
fn dim_rate(passed: i64, graded: i64) -> Option<f64> {
    (graded > 0).then(|| passed as f64 / graded as f64)
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
        let lift = trend
            .lift
            .map_or_else(|| "n/a".to_owned(), |lift| format!("{lift:+.2}"));
        let lift_floor = trend
            .lift_floor
            .map_or_else(|| "unset".to_owned(), |floor| format!("{floor:+.2}"));
        let words = trend
            .skill_words
            .map_or_else(|| "-".to_owned(), |words| format!("{words}"));
        let walk = trend
            .walk_secs
            .map_or_else(|| "-".to_owned(), |secs| format!("{secs:.1}s"));
        println!(
            "  {:<16} latest={latest} best={best} runs={} bar={bar} lift={lift} lift_floor={lift_floor} words={words} walk={walk}",
            trend.name, trend.runs
        );
        for dim in &trend.dims {
            let rate = dim
                .rate
                .map_or_else(|| "-".to_owned(), |rate| format!("{rate:.2}"));
            let without = dim
                .without_rate
                .map_or_else(|| "-".to_owned(), |rate| format!("{rate:.2}"));
            println!("    {:<16} with={rate} without={without}", dim.dim);
        }
    }
}

//! `do-harness learn --draft`: turn recurring sensor fires into a steering draft.
//!
//! AGENTS.md §6 asks for the steering loop to run when a sensor fires more than
//! twice in a sprint (update the feedforward guide so the sensor fires less).
//! That loop was manual; this command finds the sensors (recorded beats plus
//! `.agents/events/**` `sensor-fire` records) and drafts the artifacts that
//! close it: the guide row, the `trace add` + `distill` pair that produces it,
//! a skill scaffold command, and the CHANGELOG line.
//!
//! Nothing is written. The draft is a proposal the operator applies, because
//! only a human can classify *why* a sensor fired and what the guide should
//! say. Proposed skill names follow the published Agent Skills specification
//! (<https://agentskills.io/specification>): lowercase alphanumerics and
//! hyphens, no leading/trailing or consecutive hyphens, at most 64 characters.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::report::Format;

mod sources;

use sources::{Fires, MAX_WINDOW_DAYS, SECONDS_PER_DAY, beat_fires, event_fires, window_start};

/// Default steering window.
pub const DEFAULT_WINDOW_DAYS: i64 = 30;
/// Default fire threshold (AGENTS.md §6: "more than 2 times in one sprint").
pub const DEFAULT_MIN_FIRES: u32 = 3;
/// Tracked guide that receives steering rows by default.
const DEFAULT_GUIDE: &str = ".agents/skills/harness/references/heuristics.md";
/// Skill the drafted `distill` invocation writes into.
const DEFAULT_SKILL: &str = "harness";
/// Skill directory the scaffold command would create.
const SKILL_DIR: &str = ".agents/skills";
/// `name` cap from the Agent Skills specification.
const MAX_SKILL_NAME: usize = 64;
/// Suffix appended to the sensor slug for the proposed skill.
const SKILL_SUFFIX: &str = "-steering";

/// A drafted steering action for one recurring sensor.
#[derive(Debug, Clone, Serialize)]
pub struct Draft {
    /// Sensor that keeps firing.
    pub sensor: String,
    /// Total fires in the window (beats + events).
    pub fires: u32,
    /// Fires read from recorded beats.
    pub beat_fires: u32,
    /// Fires read from `.agents/events/**`.
    pub event_fires: u32,
    /// Skill the drafted `distill` invocation targets.
    pub skill: String,
    /// Proposed skill name, valid per the Agent Skills specification.
    pub skill_name: String,
    /// Guide that receives the steering row.
    pub guide: String,
    /// Row template to add to the guide.
    pub guide_row: String,
    /// `trace add` invocation that records the fix.
    pub trace_command: String,
    /// `distill` invocation that appends the row.
    pub distill_command: String,
    /// Scaffold invocation for a dedicated skill, when a row is not enough.
    pub scaffold_command: String,
    /// CHANGELOG entry draft.
    pub changelog: String,
}
/// A sensor that fired but stayed under the threshold.
#[derive(Debug, Clone, Serialize)]
pub struct NearMiss {
    /// Sensor name.
    pub sensor: String,
    /// Fires in the window.
    pub fires: u32,
}
/// The whole draft, printable as text or JSON.
#[derive(Debug, Clone, Serialize)]
pub struct LearnReport {
    /// Window the fires were counted over, in days.
    pub window_days: i64,
    /// Fires a sensor needed to appear in `sensors`.
    pub min_fires: u32,
    /// Recurring sensors, most fires first.
    pub sensors: Vec<Draft>,
    /// Sensors that fired but stayed under the threshold, most fires first.
    pub below_threshold: Vec<NearMiss>,
}
/// Collects recurring sensor fires and prints the steering draft.
///
/// # Errors
///
/// Returns an error when the window or threshold is invalid, when the state
/// database or an event file cannot be read, or when output fails.
pub async fn run(
    root: &Path,
    days: Option<i64>,
    min_fires: Option<u32>,
    format: Format,
) -> Result<()> {
    let window_days = days.unwrap_or(DEFAULT_WINDOW_DAYS);
    if window_days <= 0 {
        bail!("--days must be a positive number of days");
    }
    if window_days > MAX_WINDOW_DAYS {
        bail!("--days must be at most {MAX_WINDOW_DAYS} days");
    }
    let min_fires = min_fires.unwrap_or(DEFAULT_MIN_FIRES);
    if min_fires == 0 {
        bail!("--min-fires must be at least 1");
    }

    let now = do_harness_db::unix_now();
    // Beats store Unix seconds; event files are dated in civil days. Keep the
    // two units apart: SQL compares seconds, `days_from_civil` yields the day.
    let start_day = window_start(now, window_days);
    let start_seconds = start_day * SECONDS_PER_DAY;
    let mut fires = beat_fires(root, start_seconds).await?;
    for (sensor, count) in event_fires(root, start_day)? {
        fires.entry(sensor).or_default().events += count;
    }

    let report = draft(fires, window_days, min_fires);
    match format {
        Format::Text => print_text(&report),
        Format::Json => {
            let text = serde_json::to_string_pretty(&report).context("serialize draft")?;
            println!("{text}");
        }
    }
    Ok(())
}
/// Splits the collected fires into drafts and near misses.
fn draft(fires: BTreeMap<String, Fires>, window_days: i64, min_fires: u32) -> LearnReport {
    let mut sensors = Vec::new();
    let mut below_threshold = Vec::new();
    for (sensor, counts) in fires {
        if counts.total() == 0 {
            continue;
        }
        if counts.total() >= min_fires {
            sensors.push(draft_for(&sensor, counts));
        } else {
            below_threshold.push(NearMiss {
                sensor,
                fires: counts.total(),
            });
        }
    }
    sensors.sort_by(|a, b| b.fires.cmp(&a.fires).then_with(|| a.sensor.cmp(&b.sensor)));
    below_threshold.sort_by(|a, b| b.fires.cmp(&a.fires).then_with(|| a.sensor.cmp(&b.sensor)));
    LearnReport {
        window_days,
        min_fires,
        sensors,
        below_threshold,
    }
}
/// Quotes `value` as one POSIX shell single-quoted fragment.
///
/// The drafted commands are meant to be pasted and run, and the sensor name
/// comes from event records and config, so an embedded quote must not close
/// the fragment.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}
/// Collapses control characters so a name cannot break a Markdown row.
fn display_name(sensor: &str) -> String {
    sensor
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}
/// Builds one sensor's draft.
fn draft_for(sensor: &str, counts: Fires) -> Draft {
    let name = display_name(sensor);
    let pattern = format!("<fix that stops {name} recurring>");
    let conditions =
        format!("<conditions where the fix applies, e.g. {name} failing on the same class>");
    let skill_name = skill_name(sensor);
    Draft {
        sensor: sensor.to_owned(),
        fires: counts.total(),
        beat_fires: counts.beats,
        event_fires: counts.events,
        skill: DEFAULT_SKILL.to_owned(),
        skill_name: skill_name.clone(),
        guide: DEFAULT_GUIDE.to_owned(),
        guide_row: format!("- **{pattern}**: {conditions} (from trace <N>)"),
        // Every placeholder is quoted: an unquoted `<session>` is a shell
        // redirection, so the drafted line would not even parse.
        trace_command: format!(
            "do-harness trace add --session {} --command {} \
             --error-diff {} --resolution-steps {}",
            shell_quote("<session>"),
            shell_quote("<command that fired>"),
            shell_quote(&format!("<{name} failure output>")),
            shell_quote("<fix>")
        ),
        distill_command: format!(
            "do-harness distill --skill {DEFAULT_SKILL} --from-trace {} \
             --pattern {} --description {}",
            shell_quote("<N>"),
            shell_quote(&pattern),
            shell_quote(&conditions)
        ),
        scaffold_command: format!(
            "python3 {SKILL_DIR}/skill-creator/scripts/init_skill.py {skill_name} --path {SKILL_DIR}"
        ),
        changelog: format!(
            "- docs({DEFAULT_SKILL}): add a {name} steering row after {} fires",
            counts.total()
        ),
    }
}
/// Normalizes a sensor name into a specification-valid skill name.
///
/// Per <https://agentskills.io/specification>: lowercase alphanumerics and
/// hyphens only, no leading or trailing hyphen, no consecutive hyphens, at
/// most 64 characters.
fn skill_name(sensor: &str) -> String {
    let mut slug = String::new();
    for ch in sensor.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            slug.push(lower);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let mut name = slug.trim_end_matches('-').to_owned();
    if name.is_empty() {
        name.push_str("sensor");
    }
    name.push_str(SKILL_SUFFIX);
    while name.ends_with('-') {
        name.pop();
    }
    name.truncate(MAX_SKILL_NAME);
    while name.ends_with('-') {
        name.pop();
    }
    name
}

/// Prints the draft for humans.
fn print_text(report: &LearnReport) {
    if report.sensors.is_empty() {
        println!(
            "learn --draft: no sensor fired >= {} time(s) in the last {} day(s)",
            report.min_fires, report.window_days
        );
    } else {
        println!(
            "learn --draft: {} sensor(s) fired >= {} time(s) in the last {} day(s)",
            report.sensors.len(),
            report.min_fires,
            report.window_days
        );
        for draft in &report.sensors {
            println!();
            println!(
                "{} — {} fire(s) ({} beat(s), {} event(s))",
                draft.sensor, draft.fires, draft.beat_fires, draft.event_fires
            );
            println!("  guide:     {}", draft.guide);
            println!("  guide row: {}", draft.guide_row);
            println!("  trace:     {}", draft.trace_command);
            println!("  distill:   {}", draft.distill_command);
            println!("  skill:     {}", draft.scaffold_command);
            println!("  changelog: {}", draft.changelog);
        }
        println!();
        println!("apply checklist (nothing was written):");
        println!("  1. classify why the sensor fired (read the recorded output tail)");
        println!("  2. pick the guide: add the row above, or extend a SKILL.md for a procedure");
        println!(
            "  3. record it: run `trace add`, then `distill`; the row is appended with its trace id"
        );
        println!("  4. verify: re-run the sensor and watch `do-harness metrics` trends");
    }
    if !report.below_threshold.is_empty() {
        let below: Vec<String> = report
            .below_threshold
            .iter()
            .map(|miss| format!("{} {}", miss.sensor, miss.fires))
            .collect();
        println!();
        println!("below threshold: {}", below.join(", "));
    }
}

#[cfg(test)]
#[path = "learn_tests.rs"]
mod learn_tests;

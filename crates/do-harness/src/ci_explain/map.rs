//! Mapping a failed CI job back to the local sensor that reproduces it.

use crate::config::SensorSpec;

use super::gh::FailedLogSignals;

/// Parses a run ID from a raw number string or a `GitHub` Actions URL.
///
/// # Errors
///
/// Returns an error if no numeric run ID can be extracted.
pub fn parse_run_id(input: &str) -> anyhow::Result<u64> {
    let trimmed = input.trim();
    if let Some(idx) = trimmed.find("/actions/runs/") {
        let rest = &trimmed[idx + "/actions/runs/".len()..];
        let id_part = rest.split(['/', '?', '#']).next().unwrap_or("");
        id_part.parse::<u64>().map_err(|_| {
            anyhow::anyhow!("invalid workflow run URL '{input}': could not parse run ID")
        })
    } else {
        trimmed.parse::<u64>().map_err(|_| {
            anyhow::anyhow!(
                "invalid run ID '{input}': expected an integer run ID or GitHub Actions URL"
            )
        })
    }
}

/// A tool signature that identifies the sensor covering a failing step.
struct ToolRule {
    /// Substrings of the job, step, or error text that select this rule.
    keywords: &'static [&'static str],
    /// Sensor name as configured in `do-harness.toml`.
    sensor: &'static str,
    /// Command used when the repository has no such sensor configured.
    fallback: &'static str,
}

/// Tool signatures, most specific first; `error[E...]` precedes `test` because
/// a rustc diagnostic identifies the build sensor even inside a test job.
const TOOL_RULES: &[ToolRule] = &[
    ToolRule {
        keywords: &[
            "error[e0",
            "could not compile",
            "cargo check",
            "cargo build",
        ],
        sensor: "check",
        fallback: "cargo check --workspace",
    },
    ToolRule {
        keywords: &["clippy"],
        sensor: "clippy",
        fallback: "cargo clippy --workspace --all-targets -- -D warnings",
    },
    ToolRule {
        keywords: &[
            "nextest",
            "cargo test",
            "test result: failed",
            "test failed",
            "panicked at",
            "failed to run test",
        ],
        sensor: "test",
        fallback: "cargo nextest run --workspace --no-tests=pass",
    },
    ToolRule {
        keywords: &["doctest", "doc test"],
        sensor: "doctest",
        fallback: "cargo test --doc --workspace",
    },
    ToolRule {
        keywords: &["fmt", "rustfmt"],
        sensor: "fmt",
        fallback: "cargo fmt --all -- --check",
    },
    ToolRule {
        keywords: &["check-loc", "loc ceiling", "500 loc"],
        sensor: "loc",
        fallback: "bash scripts/check-loc.sh",
    },
    ToolRule {
        keywords: &["markdownlint"],
        sensor: "markdownlint",
        fallback: "markdownlint-cli2",
    },
    ToolRule {
        keywords: &["shellcheck"],
        sensor: "shell",
        fallback: "shellcheck scripts/*.sh",
    },
    ToolRule {
        keywords: &["cargo deny"],
        sensor: "deps",
        fallback: "cargo deny check",
    },
    ToolRule {
        keywords: &["cargo audit"],
        sensor: "audit",
        fallback: "cargo audit",
    },
];

/// Matches a failed job to a local sensor and the command that reproduces it.
///
/// Precedence:
/// 1. A `FAIL <name>` marker the harness printed in the failed step's log: the
///    harness names the sensor itself, so this signal is authoritative.
/// 2. A configured sensor whose name appears in the job, step, or error text.
/// 3. A tool signature (`clippy`, `nextest`, a rustc `error[E....]`) mapped to
///    the sensor that runs it, preferring the configured argv.
pub fn map_sensor(
    job_name: &str,
    failed_step: Option<&str>,
    reason: Option<&str>,
    signals: &FailedLogSignals,
    sensors: &[SensorSpec],
) -> (Option<String>, Option<String>) {
    if let Some(name) = signals.failed_sensors.first() {
        return resolve(name, sensors);
    }

    let mut context = format!(
        "{} {} {}",
        job_name.to_lowercase(),
        failed_step.unwrap_or("").to_lowercase(),
        reason.unwrap_or("").to_lowercase()
    );
    for line in &signals.error_lines {
        context.push(' ');
        context.push_str(&line.to_lowercase());
    }

    if let Some(spec) = sensors
        .iter()
        .find(|spec| context.contains(&spec.name.to_lowercase()))
    {
        return (Some(spec.name.clone()), Some(spec.argv.join(" ")));
    }

    if let Some(rule) = TOOL_RULES
        .iter()
        .find(|rule| rule.keywords.iter().any(|kw| context.contains(kw)))
    {
        return resolve(rule.sensor, sensors);
    }

    (None, None)
}

/// Resolves a sensor name against the config, else the tool's canonical
/// command, else an opt-in re-run of that single sensor.
fn resolve(sensor: &str, sensors: &[SensorSpec]) -> (Option<String>, Option<String>) {
    let configured = sensors.iter().find(|spec| spec.name == sensor);
    let command = configured
        .map(|spec| spec.argv.join(" "))
        .or_else(|| {
            TOOL_RULES
                .iter()
                .find(|rule| rule.sensor == sensor)
                .map(|rule| rule.fallback.to_owned())
        })
        .or_else(|| Some(format!("do-harness verify --only {sensor}")));
    let name = configured.map_or_else(|| sensor.to_owned(), |spec| spec.name.clone());
    (Some(name), command)
}

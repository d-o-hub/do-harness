//! `JUnit` XML rendering for `verify --format junit`.
//!
//! Extracted from `report.rs` so the verify reporter and the CI-annotation
//! renderer stay under the 500-line ceiling; the split is move-only.

use crate::report::{Execution, FailureReason, SensorResult, VerifyReport, findings_suffix};

/// Prints a verify report as `JUnit` XML.
///
/// One `<testcase>` per sensor, so a CI test tab and its annotations show the
/// same set the text report does. The mapping is deliberate:
///
/// - a sensor that never started is `<error>` (a process problem, not an
///   assertion failure) and keeps the run's exit code 2;
/// - a failing `error`-severity sensor is `<failure>` with its already-redacted
///   output tail as the body;
/// - a reused, planned, or advisory (`warn` severity, quarantined, or
///   SKIP-marked) result is `<skipped>` with the reason in `message`, so a
///   green advisory run never shows a red test in CI;
/// - a vacuous run is an `<error>` testcase: `JUnit` has no way to say "zero
///   tests is not a pass", and a silent empty suite would read as green.
///
/// Durations are seconds with millisecond precision, as the schema expects.
///
/// A short document is one `write!` per element; a failure to format into a
/// `String` cannot happen, so the renderer has no error path.
pub fn print_junit(report: &VerifyReport) {
    use std::fmt::Write as _;

    let mut xml = String::new();
    let failures = report
        .sensors
        .iter()
        .filter(|sensor| {
            !sensor.ok
                && !sensor.allow_failure
                && sensor.details.reason != Some(FailureReason::Spawn)
        })
        .count();
    let errors = report.unrunnable.len() + usize::from(report.vacuous);
    let skipped = report
        .sensors
        .iter()
        .filter(|sensor| sensor_skipped(sensor))
        .count();
    let seconds: f64 = report
        .sensors
        .iter()
        .map(|sensor| std::time::Duration::from_millis(sensor.duration_ms).as_secs_f64())
        .sum();
    let _ = write!(
        xml,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <testsuites name=\"do-harness\" tests=\"{}\" failures=\"{failures}\" errors=\"{errors}\" skipped=\"{skipped}\" time=\"{seconds:.3}\">\n\
         \x20 <testsuite name=\"{}\" tests=\"{}\" failures=\"{failures}\" errors=\"{errors}\" skipped=\"{skipped}\" time=\"{seconds:.3}\">\n",
        report.sensors.len() + usize::from(report.vacuous),
        xml_escape(&report.root),
        report.sensors.len() + usize::from(report.vacuous),
    );
    for sensor in &report.sensors {
        let name = xml_escape(&sensor.name);
        let time = std::time::Duration::from_millis(sensor.duration_ms).as_secs_f64();
        if sensor.details.reason == Some(FailureReason::Spawn) {
            let _ = write!(
                xml,
                "    <testcase classname=\"do-harness.verify\" name=\"{name}\" time=\"{time:.3}\">\n\
                 \x20     <error message=\"{}\">{}</error>\n\
                 \x20   </testcase>\n",
                xml_escape(&unrunnable_message(sensor)),
                xml_escape(&sensor.output),
            );
        } else if !sensor.ok && !sensor.allow_failure {
            let _ = write!(
                xml,
                "    <testcase classname=\"do-harness.verify\" name=\"{name}\" time=\"{time:.3}\">\n\
                 \x20     <failure message=\"{}\">{}</failure>\n\
                 \x20   </testcase>\n",
                xml_escape(&failure_message(sensor)),
                xml_escape(&sensor.output),
            );
        } else if sensor_skipped(sensor) {
            let _ = write!(
                xml,
                "    <testcase classname=\"do-harness.verify\" name=\"{name}\" time=\"{time:.3}\">\n\
                 \x20     <skipped message=\"{}\"/>\n\
                 \x20   </testcase>\n",
                xml_escape(&skip_message(sensor)),
            );
        } else {
            let _ = writeln!(
                xml,
                "    <testcase classname=\"do-harness.verify\" name=\"{name}\" time=\"{time:.3}\"/>",
            );
        }
    }
    if report.vacuous {
        xml.push_str(
            "    <testcase classname=\"do-harness.verify\" name=\"vacuous\" time=\"0.000\">\n\
             \x20     <error message=\"VACUOUS: no sensors selected; this run is not verification evidence.\"/>\n\
             \x20   </testcase>\n",
        );
    }
    xml.push_str("  </testsuite>\n</testsuites>\n");
    print!("{xml}");
}

/// Whether a `JUnit` consumer should see this sensor as skipped.
fn sensor_skipped(sensor: &SensorResult) -> bool {
    match sensor.execution {
        Execution::Reused | Execution::Planned | Execution::NotRun => true,
        Execution::Ran => sensor.warned || (sensor.allow_failure && !sensor.ok),
    }
}

/// `<failure>` message: exit status plus any findings/baseline context.
fn failure_message(sensor: &SensorResult) -> String {
    let mut message = match sensor.exit_code {
        Some(code) => format!("sensor failed with exit code {code}"),
        None => "sensor failed".to_owned(),
    };
    let suffix = findings_suffix(sensor);
    if !suffix.is_empty() {
        message.push_str(&suffix);
    }
    if let Some(fix) = &sensor.fix {
        message.push_str("; fix: ");
        message.push_str(fix);
    }
    message
}

/// `<error>` message for a sensor whose program could not be started.
fn unrunnable_message(sensor: &SensorResult) -> String {
    match sensor.details.reason {
        Some(FailureReason::Timeout) => "sensor timed out".to_owned(),
        Some(FailureReason::Cancelled) => "sensor cancelled by fail-fast".to_owned(),
        Some(FailureReason::OutputLimit) => "sensor output exceeded its budget".to_owned(),
        _ => "sensor could not run".to_owned(),
    }
}

/// `<skipped>` message explaining why a sensor produced no observed verdict.
fn skip_message(sensor: &SensorResult) -> String {
    match sensor.execution {
        Execution::Reused => match sensor.reused_beat_id {
            Some(id) => format!("unchanged inputs; reused recorded passing beat {id}"),
            None => "unchanged inputs; reused recorded passing beat".to_owned(),
        },
        Execution::Planned => "planned only (dry run); not executed".to_owned(),
        Execution::NotRun => "not run: blocked or quarantined by policy".to_owned(),
        Execution::Ran => {
            if sensor.ok {
                "SKIP: tool or runtime unavailable".to_owned()
            } else {
                let mut message = format!("advisory failure (severity {})", severity_str(sensor));
                if let Some(code) = sensor.exit_code {
                    message = format!(
                        "advisory failure (exit code {code}, severity {})",
                        severity_str(sensor)
                    );
                }
                message
            }
        }
    }
}

fn severity_str(sensor: &SensorResult) -> &'static str {
    match sensor.severity {
        crate::config::SensorSeverity::Error => "error",
        crate::config::SensorSeverity::Warn => "warn",
    }
}

/// XML-escapes text for an attribute or element body.
///
/// `\r` is escaped numerically as well: `XML` parsers normalize a literal CR in
/// element content, so an output tail would otherwise change bytes on the way
/// into the `JUnit` tab.
fn xml_escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            '\r' => escaped.push_str("&#13;"),
            // Control characters other than tab/newline are not legal XML 1.0.
            c if (c as u32) < 0x20 && c != '\n' && c != '\t' => escaped.push('\u{FFFD}'),
            c => escaped.push(c),
        }
    }
    escaped
}

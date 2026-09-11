//! `do-harness verify`: sensor execution, beat recording, and evidence.

use std::path::{Path, PathBuf};

use crate::report::{self, Format};
use crate::{CliError, config, evidence, sensors, telemetry};

/// Runs the `verify` subcommand: sensors, optional beat recording, report.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run(
    root: &Path,
    config: Option<&Path>,
    fail_fast: bool,
    format: Format,
    only: Vec<String>,
    exclude: Vec<String>,
    record: bool,
    task: Option<i64>,
    evidence: Option<PathBuf>,
    strict: bool,
) -> std::result::Result<(), CliError> {
    let started_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    let cfg = config::load(root, config).map_err(CliError::Usage)?;
    let blocked = if record {
        telemetry::blocked_sensors(root, &cfg.sensor_names(), task)
            .await
            .map_err(CliError::Usage)?
    } else {
        Vec::new()
    };
    let opts = sensors::VerifyOpts {
        fail_fast,
        only,
        exclude,
        blocked,
    };
    match sensors::verify(&cfg, root, &opts) {
        Ok(report) => {
            if record {
                telemetry::record_verify(root, &report, &opts.blocked, task)
                    .await
                    .map_err(CliError::Usage)?;
            }
            report::print_report(&report, format);

            let evidence_path = evidence
                .or_else(|| strict.then(|| PathBuf::from(".do-harness/evidence.json")))
                .map(|p| if p.is_relative() { root.join(p) } else { p });

            if let Some(path) = evidence_path {
                let finished_at = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
                let doc = evidence::EvidenceDocument::from_run(
                    &cfg,
                    root,
                    &report,
                    &opts.only,
                    task,
                    started_at,
                    finished_at,
                );
                if let Some(parent) = path.parent() {
                    if !parent.as_os_str().is_empty() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                }
                let json =
                    serde_json::to_vec_pretty(&doc).map_err(|e| CliError::Verify(e.into()))?;
                std::fs::write(&path, json).map_err(|e| CliError::Verify(e.into()))?;

                if strict && !doc.is_strict_clean() {
                    eprintln!(
                        "strict evidence check failed; artifact at {}",
                        path.display()
                    );
                    return Err(CliError::Verify(anyhow::anyhow!("weak evidence")));
                }
            }

            if report.ok {
                Ok(())
            } else {
                Err(CliError::Verify(anyhow::anyhow!(
                    "{} sensor(s) failed",
                    report.failed.len()
                )))
            }
        }
        Err(err) => Err(CliError::Usage(err)),
    }
}

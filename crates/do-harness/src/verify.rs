//! `do-harness verify`: sensor execution, beat recording, and evidence.

use std::path::{Path, PathBuf};

use crate::sensors::VerifyOpts;
use crate::{CliError, config, evidence, report, sensors, telemetry};

/// Runs the `verify` subcommand: sensors, optional beat recording, report.
pub(crate) async fn run(root: &Path, mut opts: VerifyOpts) -> std::result::Result<(), CliError> {
    let started_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    let cfg = config::load(root, opts.config.as_deref())
        .await
        .map_err(CliError::Usage)?;
    if opts.record && opts.task.is_none() {
        eprintln!(
            "warning: verify --record without --task records beats in the global namespace; \
             pass --task <id> to scope them to a task"
        );
    }
    if opts.record {
        opts.blocked = telemetry::blocked_sensors(root, &cfg.sensor_names(), opts.task)
            .await
            .map_err(CliError::Usage)?;
    }
    match sensors::verify(&cfg, root, &opts) {
        Ok(report) => {
            if opts.record {
                telemetry::record_verify(root, &report, &opts.blocked, opts.task)
                    .await
                    .map_err(CliError::Usage)?;
            }
            report::print_report(&report, opts.format);

            let evidence_path = opts
                .evidence
                .clone()
                .or_else(|| {
                    opts.strict
                        .then(|| PathBuf::from(".do-harness/evidence.json"))
                })
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
                    opts.task,
                    started_at,
                    finished_at,
                );
                if let Some(parent) = path.parent() {
                    if !parent.as_os_str().is_empty() {
                        let _ = tokio::fs::create_dir_all(parent).await;
                    }
                }
                let json =
                    serde_json::to_vec_pretty(&doc).map_err(|e| CliError::Verify(e.into()))?;
                tokio::fs::write(&path, json)
                    .await
                    .map_err(|e| CliError::Verify(e.into()))?;

                if opts.strict && !doc.is_strict_clean() {
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

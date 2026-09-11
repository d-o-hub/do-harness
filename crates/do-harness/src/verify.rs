//! `do-harness verify`: sensor execution, beat recording, and evidence.

use std::path::Path;

use crate::evidence::EvidenceSkipped;
use crate::sensors::VerifyOpts;
use crate::{CliError, config, evidence, report, sensors, telemetry};

/// Runs the `verify` subcommand: sensors, optional beat recording, report.
pub(crate) async fn run(root: &Path, mut opts: VerifyOpts) -> std::result::Result<(), CliError> {
    let started_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    let (cfg, config_bytes) = config::load_raw(root, opts.config.as_deref())
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

            write_evidence(
                root,
                &cfg,
                config_bytes.as_deref(),
                &report,
                &opts,
                started_at,
            )
            .await?;

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

/// Writes the evidence artifact for runs that own one.
///
/// Signal-set runs own their evidence file so a feedback run can never
/// clobber verification evidence (or vice versa). Runs without `--set` keep
/// the legacy behavior: an artifact only for `--evidence` or `--strict`.
async fn write_evidence(
    root: &Path,
    cfg: &config::Config,
    config_bytes: Option<&[u8]>,
    report: &report::VerifyReport,
    opts: &VerifyOpts,
    started_at: i64,
) -> std::result::Result<(), CliError> {
    let evidence_path = opts
        .evidence
        .clone()
        .map(|p| if p.is_relative() { root.join(p) } else { p })
        .or_else(|| {
            opts.set
                .as_ref()
                .map(|set| crate::status::default_path_for_set(root, set))
        })
        .or_else(|| opts.strict.then(|| root.join(".do-harness/evidence.json")));
    let Some(path) = evidence_path else {
        return Ok(());
    };
    // Selection is resolved again for evidence metadata; the run above made
    // the same decision internally.
    let selection = sensors::resolve_selection(cfg, root, opts).map_err(CliError::Usage)?;
    let selected: Vec<String> = selection.specs.iter().map(|s| s.name.clone()).collect();
    let skipped: Vec<EvidenceSkipped> = selection
        .skipped
        .iter()
        .map(|s| EvidenceSkipped {
            name: s.name.clone(),
            reason: s.reason.clone(),
        })
        .collect();
    let candidates =
        crate::signals::candidate_specs(cfg, opts.set.as_deref()).map_err(CliError::Usage)?;
    // Fingerprints describe the post-execution workspace: sensors may create
    // files, and status never executes sensors, so the left-behind state is
    // the stable comparison point.
    let post = crate::changes::discover(root);
    let fingerprints = crate::fingerprint::for_run(
        root,
        cfg,
        config_bytes,
        opts.set.as_deref(),
        &candidates,
        &post,
    );
    let finished_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    let meta = evidence::RunMeta {
        cfg,
        root,
        set: opts.set.as_deref(),
        selected: &selected,
        fingerprints,
        changed: opts.changed,
        skipped,
        task: opts.task,
        started_at,
        finished_at,
    };
    let mut doc = evidence::EvidenceDocument::from_run(report, &meta);
    // Chain artifacts in the same workspace: reading an existing artifact's
    // chain hash makes tampering/reordering detectable.
    let prev_hash = tokio::fs::read(&path)
        .await
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|value| {
            value
                .get("chain_hash")
                .and_then(|hash| hash.as_str())
                .map(ToOwned::to_owned)
        })
        .filter(|hash| !hash.is_empty());
    doc.seal(prev_hash.clone())
        .map_err(|e| CliError::Verify(e.into()))?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
    }
    let json = serde_json::to_vec_pretty(&doc).map_err(|e| CliError::Verify(e.into()))?;
    tokio::fs::write(&path, json)
        .await
        .map_err(|e| CliError::Verify(e.into()))?;

    if opts.strict && !(doc.is_strict_clean() && doc.verify_chain(prev_hash.as_deref())) {
        eprintln!(
            "strict evidence check failed; artifact at {}",
            path.display()
        );
        return Err(CliError::Verify(anyhow::anyhow!("weak evidence")));
    }
    Ok(())
}

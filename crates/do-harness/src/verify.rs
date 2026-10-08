//! `do-harness verify`: sensor execution, beat recording, and evidence.

use std::path::Path;

use crate::baselines::{Baselines, BlessOutcome};
use crate::config::SensorSeverity;
use crate::evidence::EvidenceSkipped;
use crate::sensors::VerifyOpts;
use crate::{CliError, config, evidence, report, sensor_inputs, sensors, telemetry};

/// Marker exported by managed git hooks (`hook install`); see
/// `hook_script::EXEC_VERIFY`.
const HOOK_MARKER: &str = "DO_HARNESS_HOOK";

/// Whether the unscoped-record advisory should be printed.
///
/// A hook-driven `--record` has no task context — a git hook runs outside any
/// task — so its beats land in the global namespace by design and the
/// `--task` advice would be unactionable noise. Manual and agent runs keep the
/// advisory so the scoping hint is not silently lost.
fn warn_unscoped_record(
    record: bool,
    scope: &telemetry::BeatScope,
    explicit_global: bool,
    hook_marker: Option<&std::ffi::OsStr>,
) -> bool {
    record && scope == &telemetry::BeatScope::Global && !explicit_global && hook_marker.is_none()
}

/// Runs the `verify` subcommand: sensors, optional beat recording, report.
pub(crate) async fn run(root: &Path, mut opts: VerifyOpts) -> std::result::Result<(), CliError> {
    let started_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    let (cfg, config_bytes) = config::load_raw(root, opts.config.as_deref())
        .await
        .map_err(CliError::Usage)?;
    let beat_scope = telemetry::BeatScope::resolve(root, opts.raw_task.as_deref(), opts.global);
    if opts.task.is_none() {
        opts.task = beat_scope.task_id();
    }
    if warn_unscoped_record(
        opts.record,
        &beat_scope,
        opts.global,
        std::env::var_os(HOOK_MARKER).as_deref(),
    ) {
        eprintln!(
            "warning: verify --record without --task records beats in the global namespace; \
             pass --task <id> to scope them to a task"
        );
    }
    if opts.bless && !opts.record {
        return Err(CliError::Usage(anyhow::anyhow!(
            "--bless requires --record (a bless writes state)"
        )));
    }
    if opts.unchanged == sensors::UnchangedMode::Skip && !opts.record {
        return Err(CliError::Usage(anyhow::anyhow!(
            "--unchanged=skip requires --record (reuse reads recorded beats)"
        )));
    }
    opts.baselines = Baselines::load(root).await.map_err(CliError::Usage)?;
    let selection = sensors::resolve_selection(&cfg, root, &opts).map_err(CliError::Usage)?;

    // Compute pre-execution input digests for all selected sensors so mid-run
    // mutations can be detected for every run, including non---record runs.
    let mut pre_digests = std::collections::BTreeMap::new();
    for spec in &selection.specs {
        if let Some(digest) = sensor_inputs::effective_input_digest(root, spec, config_bytes.as_deref()) {
            pre_digests.insert(spec.name.clone(), digest);
        }
    }

    if opts.record {
        let struck = telemetry::struck_sensors(root, &cfg.sensor_names(), &beat_scope.key())
            .await
            .map_err(CliError::Usage)?;
        for name in struck {
            let severity = cfg
                .effective_sensors()
                .iter()
                .find(|spec| spec.name == name)
                .map(config::SensorSpec::effective_severity);
            if severity == Some(SensorSeverity::Warn) {
                opts.quarantined.push(name);
            } else {
                opts.blocked.push(name);
            }
        }
        prepare_reuse(root, &cfg, config_bytes.as_deref(), &beat_scope, &mut opts, &selection).await?;
    } else {
        opts.pre_digests = pre_digests.clone();
    }

    match sensors::verify_selection(&cfg, root, &opts, &selection) {
        Ok(report) => {
            let skipped: Vec<String> = opts
                .blocked
                .iter()
                .chain(&opts.quarantined)
                .cloned()
                .collect();
            if opts.record {
                telemetry::record_verify(
                    root,
                    &report,
                    &skipped,
                    &beat_scope,
                    &cfg,
                    config_bytes.as_deref(),
                    &opts.pre_digests,
                )
                .await
                .map_err(CliError::Usage)?;
            }
            if opts.bless {
                bless_baselines(root, &report, &opts).await?;
            }

            // Compare pre-execution and post-execution input identities.
            // If any selected sensor's input identity changed during execution
            // (e.g. source modified by a sensor or concurrent writer), mark the evidence
            // as invalidated.
            let mut invalidated_reason = None;
            for spec in &selection.specs {
                if opts.blocked.contains(&spec.name) || opts.quarantined.contains(&spec.name) {
                    continue;
                }
                if let Some(pre) = pre_digests.get(&spec.name) {
                    if let Some(post) = sensor_inputs::effective_input_digest(root, spec, config_bytes.as_deref()) {
                        if post != *pre {
                            invalidated_reason = Some("inputs_changed_during_run".to_string());
                            break;
                        }
                    } else {
                        invalidated_reason = Some("inputs_changed_during_run".to_string());
                        break;
                    }
                }
            }

            report::print_report(&report, opts.format, opts.quiet);

            write_evidence(
                root,
                &cfg,
                config_bytes.as_deref(),
                &report,
                &opts,
                started_at,
                &selection,
                invalidated_reason.as_deref(),
            )
            .await?;

            if let Some(reason) = invalidated_reason {
                return Err(CliError::Verify(anyhow::anyhow!(
                    "evidence invalidated during run ({reason})"
                )));
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

/// Resolves which selected sensors may reuse a recorded passing beat.
///
/// Only `--record` runs consult the cache. For every selected sensor that is
/// neither blocked nor quarantined, the declared-input identity is computed
/// *before* anything executes and stored in `pre_digests` so the recorder can
/// reject a digest whose inputs changed while the sensor ran. Unless `--bless`
/// (which needs current observations) or `--unchanged=run` (the forcing path)
/// is set, the latest beat in the same scope decides eligibility:
/// the newest beat must be `ok`, exit 0, with an identical stored digest — a
/// later failed or warned beat always shadows an earlier pass. Eligible beats
/// are reused under `--unchanged=skip` and reported as an advisory otherwise.
///
/// # Errors
///
/// Returns an error when selection resolution or the state database fails.
async fn prepare_reuse<'a>(
    root: &Path,
    _cfg: &config::Config,
    config_bytes: Option<&[u8]>,
    scope: &telemetry::BeatScope,
    opts: &mut VerifyOpts,
    selection: &sensors::ResolvedSelection<'a>,
) -> std::result::Result<(), CliError> {
    let lookup = !opts.bless && opts.unchanged != sensors::UnchangedMode::Run;
    // Opened on first need: a run whose sensors declare no `inputs` never
    // consults the cache and must not pay for a state-database connection.
    let mut conn: Option<do_harness_db::Connection> = None;
    for spec in &selection.specs {
        if opts.blocked.contains(&spec.name) || opts.quarantined.contains(&spec.name) {
            continue;
        }
        let Some(digest) = sensor_inputs::digest(root, spec, config_bytes) else {
            continue;
        };
        if lookup {
            let db = match conn.take() {
                Some(db) => db,
                None => do_harness_db::connect_and_migrate(root)
                    .await
                    .map_err(|e| CliError::Usage(e.into()))?,
            };
            let latest = do_harness_db::latest_sensor_beat(&db, &spec.name, &scope.key())
                .await
                .map_err(|e| CliError::Usage(e.into()))?;
            if let Some((beat_id, status, exit_code, stored)) = latest {
                let eligible = status == "ok"
                    && exit_code == Some(0)
                    && stored.as_deref() == Some(digest.as_str());
                if eligible {
                    if opts.unchanged == sensors::UnchangedMode::Skip {
                        opts.reused.insert(spec.name.clone(), beat_id);
                    } else {
                        eprintln!(
                            "warning: {}: unchanged inputs; would reuse beat {beat_id} with --unchanged=skip",
                            spec.name
                        );
                    }
                }
            }
            conn = Some(db);
        }
        opts.pre_digests.insert(spec.name.clone(), digest);
    }
    Ok(())
}

/// Applies `--bless` to the run's observed findings counts: initializes or
/// lowers blessed baselines (never raises), writes the committed file, and
/// appends an audit record to the state database.
async fn bless_baselines(
    root: &Path,
    report: &report::VerifyReport,
    opts: &VerifyOpts,
) -> std::result::Result<(), CliError> {
    let approver =
        crate::approver::resolve(opts.approver.as_deref(), root).map_err(CliError::Usage)?;
    let observed: Vec<(String, u64)> = report
        .sensors
        .iter()
        .filter_map(|sensor| {
            sensor
                .findings
                .map(|findings| (sensor.name.clone(), findings))
        })
        .collect();
    if observed.is_empty() {
        eprintln!("warning: --bless found no FINDINGS markers; baselines unchanged");
        return Ok(());
    }
    let mut baselines = opts.baselines.clone();
    let mut changed = false;
    let conn = do_harness_db::connect_and_migrate(root)
        .await
        .map_err(|e| CliError::Usage(e.into()))?;
    let now = do_harness_db::unix_now();
    for (name, count) in &observed {
        match baselines.bless(name, *count) {
            BlessOutcome::Initialized(to) => {
                changed = true;
                do_harness_db::insert_sensor_bless(&conn, name, to_i64(to), None, &approver, now)
                    .await
                    .map_err(|e| CliError::Usage(e.into()))?;
                eprintln!("{name}: baseline initialized at {to}");
            }
            BlessOutcome::Lowered { from, to } => {
                changed = true;
                do_harness_db::insert_sensor_bless(
                    &conn,
                    name,
                    to_i64(to),
                    Some(to_i64(from)),
                    &approver,
                    now,
                )
                .await
                .map_err(|e| CliError::Usage(e.into()))?;
                eprintln!("{name}: baseline lowered {from} -> {to}");
            }
            BlessOutcome::Unchanged(baseline) => {
                eprintln!("{name}: baseline unchanged at {baseline}");
            }
            BlessOutcome::Refused { baseline, observed } => {
                eprintln!(
                    "warning: {name}: observed {observed} exceeds baseline {baseline}; \
                     a bless never raises a baseline"
                );
            }
        }
    }
    if changed {
        baselines.save(root).await.map_err(CliError::Usage)?;
    }
    Ok(())
}

/// Converts a findings count to the database integer type.
fn to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// Writes the evidence artifact for runs that own one.
///
/// Signal-set runs own their evidence file so a feedback run can never
/// clobber verification evidence (or vice versa). Runs without `--set` keep
/// the legacy behavior: an artifact only for `--evidence` or `--strict`.
async fn write_evidence<'a>(
    root: &Path,
    cfg: &config::Config,
    config_bytes: Option<&[u8]>,
    report: &report::VerifyReport,
    opts: &VerifyOpts,
    started_at: i64,
    selection: &sensors::ResolvedSelection<'a>,
    invalidated_reason: Option<&str>,
) -> std::result::Result<(), CliError> {
    let evidence_path = opts
        .evidence
        .clone()
        .map(|p| if p.is_relative() { root.join(p) } else { p })
        .or_else(|| {
            opts.output
                .clone()
                .map(|p| if p.is_relative() { root.join(p) } else { p })
        })
        .or_else(|| {
            opts.set
                .as_ref()
                .map(|set| crate::status::default_path_for_set(root, set))
        })
        .or_else(|| opts.strict.then(|| root.join(".do-harness/evidence.json")));
    let Some(path) = evidence_path else {
        return Ok(());
    };
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
        record: opts.record,
        started_at,
        finished_at,
        invalidated_reason: invalidated_reason.map(ToOwned::to_owned),
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

#[cfg(test)]
mod tests {
    use super::warn_unscoped_record;
    use crate::telemetry::BeatScope;
    use std::ffi::OsStr;

    #[test]
    fn hook_runs_skip_the_unscoped_record_advisory() {
        let marker = Some(OsStr::new("1"));
        let global = BeatScope::Global;
        let branch = BeatScope::Branch("main".to_string());
        let task = BeatScope::Task(42);

        // Global fallback without explicit flag warns outside hooks:
        assert!(warn_unscoped_record(true, &global, false, None));
        // Suppressed under git hook:
        assert!(!warn_unscoped_record(true, &global, false, marker));
        // Explicit global flag suppresses warning:
        assert!(!warn_unscoped_record(true, &global, true, None));
        // Scoped runs (branch or task) never warn:
        assert!(!warn_unscoped_record(true, &branch, false, None));
        assert!(!warn_unscoped_record(true, &task, false, None));
        // Record disabled never warns:
        assert!(!warn_unscoped_record(false, &global, false, None));
    }
}

//! Bounded parallel sensor execution for `do-harness verify`.
//!
//! Sensors are independent `argv` commands with no shared-state contract, so
//! a run with `jobs > 1` executes each chunk of sensors on scoped worker
//! threads and reassembles results in config order (deterministic text and
//! JSON output). `--fail-fast` kills in-flight siblings through the shared
//! cancel flag and never starts a later chunk once a chunk hard-fails.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;

use super::exec::run_sensor;
use super::{VerifyOpts, sensor_blocked};
use crate::config::SensorSpec;
use crate::report::SensorResult;

/// Resolves the worker bound: explicit `--jobs` wins, then `jobs` from
/// `do-harness.toml`, then sequential.
///
/// # Errors
///
/// Returns an error when either source requests zero workers.
pub fn effective_jobs(config_jobs: Option<usize>, cli_jobs: Option<usize>) -> Result<usize> {
    let jobs = cli_jobs.or(config_jobs).unwrap_or(1);
    if jobs == 0 {
        anyhow::bail!("jobs must be at least 1 (got 0)");
    }
    Ok(jobs)
}

/// A spec paired with its config-order index so worker threads reassemble
/// results deterministically regardless of finish order.
#[derive(Clone, Copy)]
struct IndexedSpec<'a> {
    index: usize,
    spec: &'a SensorSpec,
}

/// Sorts indexed results back into config order, dropping the indices.
fn into_order(mut indexed: Vec<(usize, SensorResult)>) -> Vec<SensorResult> {
    indexed.sort_by_key(|(index, _)| *index);
    indexed.into_iter().map(|(_, result)| result).collect()
}

/// Runs one chunk of specs on scoped worker threads.
///
/// Each worker checks the shared cancel flag before spawning: with
/// `--fail-fast`, a sibling's hard failure stops later spawns quickly while
/// already-running sensors die through the same flag inside the executor.
fn run_chunk(
    chunk: &[IndexedSpec<'_>],
    root: &Path,
    opts: &VerifyOpts,
    cancel: &AtomicBool,
) -> Vec<(usize, SensorResult)> {
    std::thread::scope(|scope| {
        let handles: Vec<_> = chunk
            .iter()
            .map(|item| {
                let handle = scope.spawn(|| {
                    let result = if opts.fail_fast && cancel.load(Ordering::SeqCst) {
                        SensorResult {
                            name: item.spec.name.clone(),
                            ok: false,
                            exit_code: None,
                            duration_ms: 0,
                            severity: item.spec.effective_severity(),
                            allow_failure: item.spec.effective_severity()
                                == crate::config::SensorSeverity::Warn,
                            warned: false,
                            findings: None,
                            baseline: None,
                            output: format!(
                                "sensor '{}' cancelled before start: fail-fast stopped this run",
                                item.spec.name
                            ),
                        }
                    } else if opts.blocked.contains(&item.spec.name) {
                        sensor_blocked(item.spec)
                    } else if opts.quarantined.contains(&item.spec.name) {
                        super::sensor_quarantined(item.spec)
                    } else {
                        let result = run_sensor(item.spec, root, cancel, &opts.baselines);
                        if opts.fail_fast && !result.ok && !result.allow_failure {
                            cancel.store(true, Ordering::SeqCst);
                        }
                        result
                    };
                    (item.index, result)
                });
                (item, handle)
            })
            .collect();
        handles
            .into_iter()
            .map(|(item, handle)| match handle.join() {
                Ok(pair) => pair,
                Err(_) => (
                    item.index,
                    super::exec::sensor_result(
                        item.spec,
                        false,
                        None,
                        0,
                        format!("sensor '{}' worker panicked", item.spec.name),
                    ),
                ),
            })
            .collect()
    })
}

/// Runs every spec on the calling thread, exactly replicating the historical
/// sequential contract: a blocked sensor and a hard failure both halt the run
/// under `--fail-fast`, and sensors after the halt are not reported.
fn run_sequential(
    specs: Vec<&SensorSpec>,
    root: &Path,
    opts: &VerifyOpts,
    cancel: &AtomicBool,
) -> Vec<SensorResult> {
    let mut results = Vec::with_capacity(specs.len());
    for spec in specs {
        let result = if opts.blocked.contains(&spec.name) {
            sensor_blocked(spec)
        } else if opts.quarantined.contains(&spec.name) {
            super::sensor_quarantined(spec)
        } else {
            run_sensor(spec, root, cancel, &opts.baselines)
        };
        let hard_failed = !result.ok && !result.allow_failure;
        results.push(result);
        if hard_failed && opts.fail_fast {
            cancel.store(true, Ordering::SeqCst);
            break;
        }
    }
    results
}

/// Runs the selected specs with at most `jobs` sensors in flight.
///
/// `jobs == 1` keeps the sequential contract verbatim. With `jobs > 1` the
/// first chunk always runs; under `--fail-fast`, a hard failure in a chunk
/// (including a blocked sensor) cancels in-flight siblings and no later
/// chunk starts.
pub fn run_parallel(
    specs: Vec<&SensorSpec>,
    root: &Path,
    opts: &VerifyOpts,
    jobs: usize,
    cancel: &AtomicBool,
) -> Vec<SensorResult> {
    let bound = jobs.max(1);
    if bound == 1 {
        return run_sequential(specs, root, opts, cancel);
    }
    let indexed: Vec<IndexedSpec<'_>> = specs
        .into_iter()
        .enumerate()
        .map(|(index, spec)| IndexedSpec { index, spec })
        .collect();
    let mut results = Vec::with_capacity(indexed.len());
    for chunk in indexed.chunks(bound) {
        if opts.fail_fast && cancel.load(Ordering::SeqCst) {
            break;
        }
        let chunk_results = run_chunk(chunk, root, opts, cancel);
        let hard_failed = chunk_results
            .iter()
            .any(|(_, result)| !result.ok && !result.allow_failure);
        results.extend(chunk_results);
        if opts.fail_fast && hard_failed {
            cancel.store(true, Ordering::SeqCst);
            break;
        }
    }
    into_order(results)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::collections::BTreeMap;

    use super::*;
    use crate::config::{Config, HooksConfig};

    fn spec(name: &str, argv: &[&str]) -> SensorSpec {
        SensorSpec {
            name: name.to_owned(),
            argv: argv.iter().map(|a| (*a).to_owned()).collect(),
            retry: None,
            timeout: None,
            severity: None,
            allow_failure: false,
            transient_exit_codes: vec![],
            when_changed: vec![],
        }
    }

    fn config_with(specs: Vec<SensorSpec>) -> Config {
        Config {
            language: None,
            hooks: HooksConfig {
                pre_commit: vec![],
                pre_push: vec![],
            },
            signal_sets: BTreeMap::new(),
            sensors: specs,
            jobs: None,
        }
    }

    /// Slowest-first sensors still report in config order with jobs > 1.
    #[test]
    fn parallel_preserves_config_order() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = config_with(vec![
            spec("slow", &["sh", "-c", "sleep 0.4"]),
            spec("mid", &["sh", "-c", "sleep 0.2"]),
            spec("fast", &["true"]),
        ]);
        let specs: Vec<&SensorSpec> = cfg.sensors.iter().collect();
        let opts = VerifyOpts {
            ..Default::default()
        };
        let cancel = AtomicBool::new(false);
        let results = run_parallel(specs, dir.path(), &opts, 3, &cancel);
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["slow", "mid", "fast"]);
        assert!(results.iter().all(|r| r.ok));
    }

    /// A hard failure with --fail-fast kills an in-flight sibling quickly.
    #[test]
    fn fail_fast_cancels_inflight_sibling() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = config_with(vec![
            spec("fail", &["false"]),
            spec("sleeper", &["sh", "-c", "sleep 30"]),
        ]);
        let specs: Vec<&SensorSpec> = cfg.sensors.iter().collect();
        let opts = VerifyOpts {
            fail_fast: true,
            ..Default::default()
        };
        let cancel = AtomicBool::new(false);
        let results = run_parallel(specs, dir.path(), &opts, 2, &cancel);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "fail");
        assert!(!results[0].ok);
        let sibling = &results[1];
        assert!(!sibling.ok, "cancelled sibling must not pass");
        assert!(
            sibling.duration_ms < 10_000,
            "sibling must die fast, took {}ms",
            sibling.duration_ms
        );
        assert!(
            sibling.output.contains("cancelled"),
            "unexpected output: {}",
            sibling.output
        );
    }

    /// Sensors after a failed chunk never start under --fail-fast.
    #[test]
    fn fail_fast_skips_later_chunks() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = config_with(vec![
            spec("fail", &["false"]),
            spec("pass", &["true"]),
            spec("never", &["true"]),
        ]);
        let specs: Vec<&SensorSpec> = cfg.sensors.iter().collect();
        let opts = VerifyOpts {
            fail_fast: true,
            ..Default::default()
        };
        let cancel = AtomicBool::new(false);
        let results = run_parallel(specs, dir.path(), &opts, 2, &cancel);
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        // The failed chunk always reports; the later chunk never starts.
        // Whether `pass` ran or lost the spawn race to the cancel flag is
        // timing-dependent and deliberately unasserted.
        assert_eq!(names, vec!["fail", "pass"]);
        assert_eq!(results[0].name, "fail");
        assert!(!results[0].ok);
    }

    /// jobs resolution: CLI wins, then config, then sequential default; zero fails.
    #[test]
    fn jobs_resolution_order() {
        assert_eq!(effective_jobs(None, None).expect("default"), 1);
        assert_eq!(effective_jobs(Some(4), None).expect("config"), 4);
        assert_eq!(effective_jobs(Some(4), Some(2)).expect("cli wins"), 2);
        assert!(effective_jobs(Some(0), None).is_err());
        assert!(effective_jobs(None, Some(0)).is_err());
    }
}

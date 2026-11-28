//! Versioned, repository-authored task guard bindings.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result, ensure};
use do_harness_db::{TaskGuardDecision, TaskGuardInput};
use do_harness_types::{Method, TaskRecord};
use serde::Deserialize;

const POLICY: &str = "plans/task-guards.json";
const VERSION: u32 = 1;
const MAX_BINDINGS: usize = 128;
const MAX_PREDICATES: usize = 16;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    schema_version: u32,
    mode: Mode,
    bindings: Vec<Binding>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Mode {
    Advisory,
    Enforce,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    method: String,
    transition: Transition,
    subtask: Option<String>,
    all: Vec<Predicate>,
    #[serde(rename = "otherwise")]
    _otherwise: Otherwise,
}

#[derive(Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
enum Transition {
    Advance,
    Complete,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Otherwise {
    Reject,
}

#[derive(Deserialize)]
#[serde(tag = "predicate", deny_unknown_fields)]
enum Predicate {
    #[serde(rename = "sensor.fresh_pass")]
    FreshPass { sensor: String },
}

pub(super) fn snapshot(
    root: &Path,
    task: &TaskRecord,
    complete: bool,
    methods: &[Method],
    cfg: &crate::config::Config,
    config_bytes: Option<&[u8]>,
) -> Result<(Option<String>, Vec<TaskGuardInput>)> {
    let bytes = match std::fs::read(root.join(POLICY)) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok((None, Vec::new())),
        Err(err) => return Err(err).context(POLICY),
    };
    let policy: Policy = serde_json::from_slice(&bytes).context(POLICY)?;
    ensure!(policy.schema_version == VERSION, "unsupported guard schema");
    ensure!(
        !policy.bindings.is_empty() && policy.bindings.len() <= MAX_BINDINGS,
        "guard policy needs 1 to {MAX_BINDINGS} bindings"
    );

    let sensors = cfg.effective_sensors();
    let transition = if complete {
        Transition::Complete
    } else {
        Transition::Advance
    };
    let mut keys = BTreeSet::new();
    let mut inputs = Vec::new();
    for binding in &policy.bindings {
        let method = crate::methods::find_method(methods, &binding.method)
            .with_context(|| format!("unknown guard method '{}'", binding.method))?;
        let subtask = match (binding.transition, &binding.subtask) {
            (Transition::Complete, None) => None,
            (Transition::Advance, Some(name)) => Some(
                method
                    .subtasks
                    .iter()
                    .position(|s| &s.name == name)
                    .with_context(|| {
                        format!("unknown guard subtask '{name}' on '{}'", binding.method)
                    })?,
            ),
            _ => anyhow::bail!(
                "advance guards require a subtask; completion guards apply to the method"
            ),
        };
        ensure!(
            keys.insert((&binding.method, binding.transition, subtask)),
            "duplicate guard binding"
        );
        ensure!(
            !binding.all.is_empty() && binding.all.len() <= MAX_PREDICATES,
            "guard needs 1 to {MAX_PREDICATES} predicates"
        );

        let applies = task.method.as_ref() == Some(&binding.method)
            && transition == binding.transition
            && subtask.is_none_or(|index| usize::try_from(task.subtask_index) == Ok(index));
        let mut seen = BTreeSet::new();
        for Predicate::FreshPass { sensor } in &binding.all {
            ensure!(seen.insert(sensor), "duplicate guard sensor '{sensor}'");
            let spec = sensors
                .iter()
                .find(|spec| &spec.name == sensor)
                .with_context(|| format!("unknown guard sensor '{sensor}'"))?;
            if applies {
                inputs.push(TaskGuardInput {
                    sensor: sensor.clone(),
                    digest: crate::provenance::sensor_digest(root, spec, config_bytes, None),
                    enforce: matches!(policy.mode, Mode::Enforce),
                });
            }
        }
    }
    Ok((
        Some(crate::fingerprint::config_digest(Some(&bytes))),
        inputs,
    ))
}

pub(super) fn print_advisory(decisions: &[TaskGuardDecision]) {
    for decision in decisions.iter().filter(|decision| !decision.enforce) {
        eprintln!(
            "guard advisory sensor.fresh_pass {} {:?} ({})",
            decision.sensor, decision.verdict, decision.reason
        );
    }
}

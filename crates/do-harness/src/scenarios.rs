//! Optional executable scenario bindings over existing verification evidence.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::evidence::EvidenceDocument;

const MAPPING_PATH: &str = "plans/verification-scenarios.json";
const VERSION: u32 = 1;
const MAX_SCENARIOS: usize = 256;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Mapping {
    schema_version: u32,
    signal_set: String,
    #[serde(default)]
    enforce: bool,
    requirements: Vec<Requirement>,
    #[serde(skip)]
    input_digest: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Requirement {
    id: String,
    source: String,
    scenarios: Vec<Scenario>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Scenario {
    id: String,
    expected: Branch,
    sensor: String,
    test_id: String,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Branch {
    Accept,
    Reject,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TestResults {
    scenario_schema_version: u32,
    tests: Vec<TestResult>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TestResult {
    id: String,
    branch: Branch,
    outcome: TestOutcome,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum TestOutcome {
    Passed,
    Failed,
    Skipped,
}

/// Disposition of one explicitly mapped scenario in a recorded run.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioOutcome {
    /// The identified test executed and observed the expected branch.
    Passed,
    /// The test failed or observed a different branch.
    Failed,
    /// The runner declared the identified test skipped.
    Skipped,
    /// The sensor did not execute or the result omitted the identified test.
    NotRun,
    /// The runner did not report an unambiguous versioned result.
    Unknown,
}

/// Stable, bounded diagnostics retained with the verification artifact.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScenarioResult {
    /// Declared requirement identifier.
    pub requirement: String,
    /// Declared scenario identifier.
    pub scenario: String,
    /// Registered sensor responsible for executing the scenario.
    pub sensor: String,
    /// Exact test identifier reported by the runner.
    pub test_id: String,
    /// Observed execution outcome.
    pub outcome: ScenarioOutcome,
}

/// Loads the optional contract and includes its source files in the existing
/// evidence provenance, for the one signal set explicitly covered by it.
pub(crate) fn configure(
    root: &Path,
    cfg: &mut Config,
    set: Option<&str>,
) -> Result<Option<Mapping>> {
    let Some(set) = set else { return Ok(None) };
    let path = root.join(MAPPING_PATH);
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) => ensure!(metadata.is_file(), "{MAPPING_PATH} must be a regular file"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context(MAPPING_PATH),
    }
    validate_path(&root.canonicalize()?, MAPPING_PATH)?;
    let bytes = std::fs::read(&path).context(MAPPING_PATH)?;
    let mut mapping: Mapping = serde_json::from_slice(&bytes).context(MAPPING_PATH)?;
    if mapping.signal_set != set {
        return Ok(None);
    }
    let inputs = mapping.validate(root, cfg)?;
    if cfg.sensors.is_empty() {
        cfg.sensors = cfg.effective_sensors().to_vec();
    }
    for sensor in &mut cfg.sensors {
        if let Some(paths) = inputs.get(&sensor.name) {
            for path in paths.iter().copied().chain([MAPPING_PATH]) {
                insert_path(&mut sensor.coverage_inputs, path);
                if !sensor.when_changed.is_empty() {
                    insert_path(&mut sensor.when_changed, path);
                }
            }
        }
    }
    mapping.input_digest = crate::fingerprint::config_digest(Some(&bytes));
    Ok(Some(mapping))
}

fn insert_path(paths: &mut Vec<String>, path: &str) {
    if !paths.iter().any(|existing| existing == path) {
        paths.push(path.to_owned());
    }
}

impl Mapping {
    fn validate(&self, root: &Path, cfg: &Config) -> Result<BTreeMap<String, BTreeSet<&str>>> {
        ensure!(
            self.schema_version == VERSION,
            "unsupported scenario mapping schema"
        );
        let set = cfg
            .signal_sets
            .get(&self.signal_set)
            .context("unknown scenario signal set")?;
        let count: usize = self.requirements.iter().map(|r| r.scenarios.len()).sum();
        ensure!(
            (1..=MAX_SCENARIOS).contains(&count),
            "mapping needs 1 to {MAX_SCENARIOS} scenarios"
        );
        let root = root.canonicalize()?;
        let mut requirement_ids = BTreeSet::new();
        let mut scenario_ids = BTreeSet::new();
        let mut bindings = BTreeSet::new();
        let mut inputs: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
        for requirement in &self.requirements {
            ensure!(
                valid_id(&requirement.id) && requirement_ids.insert(&requirement.id),
                "invalid or duplicate requirement id '{}'",
                requirement.id
            );
            ensure!(
                !requirement.scenarios.is_empty(),
                "requirement '{}' has no scenarios",
                requirement.id
            );
            validate_path(&root, &requirement.source)?;
            for scenario in &requirement.scenarios {
                ensure!(
                    valid_id(&scenario.id) && scenario_ids.insert(&scenario.id),
                    "invalid or duplicate scenario id '{}'",
                    scenario.id
                );
                ensure!(
                    set.contains(&scenario.sensor),
                    "scenario '{}' references a sensor outside its signal set",
                    scenario.id
                );
                ensure!(
                    valid_id(&scenario.test_id)
                        && bindings.insert((&scenario.sensor, &scenario.test_id)),
                    "invalid or ambiguous test identity for scenario '{}'",
                    scenario.id
                );
                inputs
                    .entry(scenario.sensor.clone())
                    .or_default()
                    .insert(&requirement.source);
            }
        }
        Ok(inputs)
    }

    pub(crate) fn enforces(&self) -> bool {
        self.enforce
    }

    pub(crate) fn record(&self, root: &Path, document: &mut EvidenceDocument) {
        let current = std::fs::read(root.join(MAPPING_PATH)).ok();
        if crate::fingerprint::config_digest(current.as_deref()) != self.input_digest {
            document.invalidated_reason = Some("scenario_mapping_changed_during_run".to_owned());
        }
        let parsed: BTreeMap<_, _> = document
            .coverage
            .iter()
            .filter_map(|(name, coverage)| {
                let results = TestResults::deserialize(coverage).ok()?;
                (results.scenario_schema_version == VERSION).then_some((name, results.tests))
            })
            .collect();
        let mut results = Vec::new();
        for requirement in &self.requirements {
            for scenario in &requirement.scenarios {
                let outcome = match document
                    .sensors
                    .iter()
                    .find(|sensor| sensor.name == scenario.sensor)
                {
                    Some(sensor) if sensor.exit_code.is_some() && sensor.verdict != "skip" => {
                        outcome(
                            scenario,
                            parsed.get(&scenario.sensor).map(Vec::as_slice),
                            sensor.verdict == "pass",
                        )
                    }
                    _ => ScenarioOutcome::NotRun,
                };
                results.push(ScenarioResult {
                    requirement: requirement.id.clone(),
                    scenario: scenario.id.clone(),
                    sensor: scenario.sensor.clone(),
                    test_id: scenario.test_id.clone(),
                    outcome,
                });
            }
        }
        if self.enforce
            && results
                .iter()
                .any(|result| result.outcome != ScenarioOutcome::Passed)
        {
            "fail".clone_into(&mut document.summary.verdict);
        }
        document.scenario_results = results;
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn validate_path(root: &Path, value: &str) -> Result<()> {
    let path = Path::new(value);
    ensure!(
        !value.is_empty()
            && !value.contains(['*', '?', '[', ']', '{', '}', '\\'])
            && path.components().all(|c| matches!(c, Component::Normal(_))),
        "scenario source path must be a literal repository-relative file: {value}"
    );
    let resolved = root
        .join(path)
        .canonicalize()
        .with_context(|| format!("missing scenario source {value}"))?;
    ensure!(
        resolved.starts_with(root) && resolved.is_file(),
        "invalid scenario source {value}"
    );
    Ok(())
}

fn outcome(
    scenario: &Scenario,
    tests: Option<&[TestResult]>,
    sensor_passed: bool,
) -> ScenarioOutcome {
    let Some(tests) = tests else {
        return ScenarioOutcome::Unknown;
    };
    let mut matches = tests.iter().filter(|test| test.id == scenario.test_id);
    let Some(test) = matches.next() else {
        return ScenarioOutcome::NotRun;
    };
    if matches.next().is_some() {
        return ScenarioOutcome::Unknown;
    }
    match test.outcome {
        TestOutcome::Passed if sensor_passed && test.branch == scenario.expected => {
            ScenarioOutcome::Passed
        }
        TestOutcome::Passed | TestOutcome::Failed => ScenarioOutcome::Failed,
        TestOutcome::Skipped => ScenarioOutcome::Skipped,
    }
}

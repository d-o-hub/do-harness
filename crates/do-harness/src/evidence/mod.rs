//! Evidence artifact writer for `do-harness verify --evidence`.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::report::VerifyReport;

pub const EVIDENCE_SCHEMA_VERSION: u32 = 3;

/// A sensor skipped by change-aware selection, with its reason.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EvidenceSkipped {
    /// Sensor name as configured.
    pub name: String,
    /// Why the sensor was not applicable (mirrors the explain reason).
    pub reason: String,
}

/// Single sensor result in the evidence artifact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EvidenceSensor {
    /// Sensor name as configured.
    pub name: String,
    /// Exact argv executed (including the program).
    pub argv: Vec<String>,
    /// `"pass"` | `"fail"` | `"skip"`.
    pub verdict: String,
    /// Process exit code, when the sensor ran.
    pub exit_code: Option<i32>,
    /// Wall-clock duration in milliseconds, when the sensor ran.
    pub duration_ms: Option<u64>,
    /// SHA-256 (hex) of the captured sensor output; empty when not run.
    pub output_sha256: String,
    /// Whether a beat was persisted for this sensor.
    pub recorded: bool,
}

/// Aggregated summary of sensor verdicts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EvidenceSummary {
    pub pass: usize,
    pub fail: usize,
    pub skip: usize,
    pub verdict: String, // "pass" | "fail"
}

/// Schema-versioned evidence document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EvidenceDocument {
    pub schema_version: u32,
    pub tool: String,
    pub harness_version: String,
    pub git_sha: Option<String>,
    pub started_at: i64,
    pub finished_at: i64,
    pub root: String,
    pub task_id: Option<i64>,
    pub sensor_pack: String,
    /// Development signal set selected via `--set`; absent for full runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal_set: Option<String>,
    /// Workspace fingerprint (`sha256:…`) of the post-run working tree.
    pub workspace_fingerprint: String,
    /// Effective run-policy fingerprint (`sha256:…`).
    pub policy_fingerprint: String,
    /// Policy fingerprint without the signal-set name (`sha256:…`).
    pub config_fingerprint: String,
    /// Whether the run used `--changed` selection.
    pub changed: bool,
    pub sensors: Vec<EvidenceSensor>,
    /// Sensors skipped by change-aware selection, with reasons.
    pub skipped: Vec<EvidenceSkipped>,
    pub summary: EvidenceSummary,
    /// Previous artifact's chain hash, when one existed in the same workspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_hash: Option<String>,
    /// SHA-256 hash chaining this artifact to the previous one.
    #[serde(default)]
    pub chain_hash: String,
}

impl EvidenceDocument {
    /// `--strict` rejects anything weak: skips, missing exit codes or
    /// durations, or an unsealed document without a chain hash.
    #[must_use]
    pub fn is_strict_clean(&self) -> bool {
        self.summary.verdict == "pass"
            && self.summary.skip == 0
            && !self.chain_hash.is_empty()
            && self
                .sensors
                .iter()
                .all(|s| s.verdict != "skip" && s.exit_code.is_some() && s.duration_ms.is_some())
    }

    /// Seals the document into the hash chain, linking it to `prev`.
    ///
    /// The payload excludes `prev_hash`/`chain_hash`, so `verify_chain` can
    /// recompute it from the artifact alone.
    ///
    /// # Errors
    ///
    /// Returns an error when the canonical payload cannot be serialized.
    pub fn seal(&mut self, prev: Option<String>) -> Result<(), serde_json::Error> {
        let payload = serde_json::json!({
            "schema_version": self.schema_version,
            "tool": self.tool,
            "harness_version": self.harness_version,
            "git_sha": self.git_sha,
            "started_at": self.started_at,
            "finished_at": self.finished_at,
            "root": self.root,
            "task_id": self.task_id,
            "sensor_pack": self.sensor_pack,
            "signal_set": self.signal_set,
            "workspace_fingerprint": self.workspace_fingerprint,
            "policy_fingerprint": self.policy_fingerprint,
            "config_fingerprint": self.config_fingerprint,
            "changed": self.changed,
            "sensors": self.sensors,
            "skipped": self.skipped,
            "summary": self.summary,
        });
        let canonical = do_harness_types::canonical_value(&payload)?;
        self.chain_hash = do_harness_types::chain_hash(prev.as_deref(), &canonical);
        self.prev_hash = prev;
        Ok(())
    }

    /// Recomputes the chain hash and compares it to the stored value.
    #[must_use]
    pub fn verify_chain(&self, prev: Option<&str>) -> bool {
        let mut probe = self.clone();
        let stored = self.chain_hash.clone();
        if probe.seal(prev.map(ToString::to_string)).is_err() {
            return false;
        }
        probe.chain_hash == stored
    }

    /// Creates an evidence document from a completed verify run.
    pub fn from_run(report: &VerifyReport, meta: &RunMeta<'_>) -> Self {
        let git_sha = resolve_git_sha(meta.root);
        let sensor_pack = meta
            .cfg
            .language
            .clone()
            .unwrap_or_else(|| "rust".to_string());

        let active_specs = meta.cfg.effective_sensors();
        let selected_specs: Vec<_> = active_specs
            .iter()
            .filter(|spec| meta.selected.contains(&spec.name))
            .collect();

        let mut sensors = Vec::new();
        let mut pass_count = 0;
        let mut fail_count = 0;
        let mut skip_count = 0;

        for spec in selected_specs {
            if let Some(res) = report.sensors.iter().find(|r| r.name == spec.name) {
                let verdict = if res.ok || res.allow_failure {
                    if res.ok {
                        pass_count += 1;
                        "pass"
                    } else {
                        // allow_failure soft failure is still a non-pass in strict terms if failed
                        fail_count += 1;
                        "fail"
                    }
                } else {
                    fail_count += 1;
                    "fail"
                };

                sensors.push(EvidenceSensor {
                    name: spec.name.clone(),
                    argv: spec.argv.clone(),
                    verdict: verdict.to_string(),
                    exit_code: res.exit_code,
                    duration_ms: Some(res.duration_ms),
                    output_sha256: output_sha256(&res.output),
                    recorded: true,
                });
            } else {
                skip_count += 1;
                sensors.push(EvidenceSensor {
                    name: spec.name.clone(),
                    argv: spec.argv.clone(),
                    verdict: "skip".to_string(),
                    exit_code: None,
                    duration_ms: None,
                    output_sha256: String::new(),
                    recorded: false,
                });
            }
        }

        let summary_verdict = if fail_count == 0 { "pass" } else { "fail" };

        EvidenceDocument {
            schema_version: EVIDENCE_SCHEMA_VERSION,
            tool: "do-harness".to_owned(),
            harness_version: env!("CARGO_PKG_VERSION").to_owned(),
            git_sha,
            started_at: meta.started_at,
            finished_at: meta.finished_at,
            root: meta.root.display().to_string(),
            task_id: meta.task,
            sensor_pack,
            signal_set: meta.set.map(ToOwned::to_owned),
            workspace_fingerprint: meta.fingerprints.workspace.clone(),
            policy_fingerprint: meta.fingerprints.policy.clone(),
            config_fingerprint: meta.fingerprints.config.clone(),
            changed: meta.changed,
            sensors,
            skipped: meta.skipped.clone(),
            summary: EvidenceSummary {
                pass: pass_count,
                fail: fail_count,
                skip: skip_count,
                verdict: summary_verdict.to_string(),
            },
            prev_hash: None,
            chain_hash: String::new(),
        }
    }
}

/// Run metadata for [`EvidenceDocument::from_run`] beyond the report itself.
#[derive(Debug, Clone)]
pub struct RunMeta<'a> {
    /// Configuration the run executed under.
    pub cfg: &'a Config,
    /// Workspace root the run executed from.
    pub root: &'a Path,
    /// Selected signal-set name; `None` for full runs.
    pub set: Option<&'a str>,
    /// Ordered sensor names the run targeted (after `--set`/`--only`/
    /// `--exclude` filtering); sensors without a report entry are recorded
    /// as `skip`.
    pub selected: &'a [String],
    /// Post-execution workspace/policy fingerprints.
    pub fingerprints: crate::fingerprint::Fingerprints,
    /// Whether the run used `--changed` selection.
    pub changed: bool,
    /// Sensors skipped by change-aware selection, with reasons.
    pub skipped: Vec<EvidenceSkipped>,
    /// Task id scoping persisted beats, when recording.
    pub task: Option<i64>,
    /// Unix timestamp when the run started.
    pub started_at: i64,
    /// Unix timestamp when the run finished.
    pub finished_at: i64,
}

/// Lowercase hex SHA-256 of a sensor's captured output.
fn output_sha256(output: &str) -> String {
    use sha2::{Digest, Sha256};

    hex::encode(Sha256::digest(output.as_bytes()))
}

/// Resolves git commit SHA at runtime or falls back to compile-time env var.
fn resolve_git_sha(root: &Path) -> Option<String> {
    if let Ok(output) = crate::changes::git_command(root)
        .args(["rev-parse", "HEAD"])
        .output()
    {
        if output.status.success() {
            let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !sha.is_empty() {
                return Some(sha);
            }
        }
    }
    option_env!("DO_HARNESS_GIT_SHA").map(ToString::to_string)
}

#[cfg(test)]
mod tests;

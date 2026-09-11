//! Evidence artifact writer for `do-harness verify --evidence`.

use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::report::VerifyReport;

pub const EVIDENCE_SCHEMA_VERSION: u32 = 2;

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
    pub tool: &'static str,
    pub harness_version: &'static str,
    pub git_sha: Option<String>,
    pub started_at: i64,
    pub finished_at: i64,
    pub root: String,
    pub task_id: Option<i64>,
    pub sensor_pack: String,
    pub sensors: Vec<EvidenceSensor>,
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
            "sensors": self.sensors,
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
    pub fn from_run(
        cfg: &Config,
        root: &Path,
        report: &VerifyReport,
        only: &[String],
        task: Option<i64>,
        started_at: i64,
        finished_at: i64,
    ) -> Self {
        let git_sha = resolve_git_sha(root);
        let sensor_pack = cfg.language.clone().unwrap_or_else(|| "rust".to_string());

        let active_specs = cfg.effective_sensors();
        let selected_specs: Vec<_> = active_specs
            .iter()
            .filter(|spec| only.is_empty() || only.contains(&spec.name))
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
            tool: "do-harness",
            harness_version: env!("CARGO_PKG_VERSION"),
            git_sha,
            started_at,
            finished_at,
            root: root.display().to_string(),
            task_id: task,
            sensor_pack,
            sensors,
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

/// Lowercase hex SHA-256 of a sensor's captured output.
fn output_sha256(output: &str) -> String {
    use sha2::{Digest, Sha256};

    hex::encode(Sha256::digest(output.as_bytes()))
}

/// Resolves git commit SHA at runtime or falls back to compile-time env var.
fn resolve_git_sha(root: &Path) -> Option<String> {
    if let Ok(output) = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
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
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn strict_clean_checks() {
        let doc = EvidenceDocument {
            schema_version: 1,
            tool: "do-harness",
            harness_version: "0.1.0",
            git_sha: Some("46463ef".into()),
            started_at: 100,
            finished_at: 200,
            root: "/tmp".into(),
            task_id: None,
            sensor_pack: "rust".into(),
            sensors: vec![EvidenceSensor {
                name: "check".into(),
                argv: vec!["cargo".into(), "check".into()],
                verdict: "pass".into(),
                exit_code: Some(0),
                duration_ms: Some(10),
                output_sha256: "abc123".into(),
                recorded: true,
            }],
            summary: EvidenceSummary {
                pass: 1,
                fail: 0,
                skip: 0,
                verdict: "pass".into(),
            },
            prev_hash: None,
            chain_hash: "sealed-hash".into(),
        };
        assert!(doc.is_strict_clean());

        let mut doc_skip = doc.clone();
        doc_skip.summary.skip = 1;
        doc_skip.sensors.push(EvidenceSensor {
            name: "test".into(),
            argv: vec!["cargo".into(), "test".into()],
            verdict: "skip".into(),
            exit_code: None,
            duration_ms: None,
            output_sha256: String::new(),
            recorded: false,
        });
        assert!(!doc_skip.is_strict_clean());

        let mut doc_no_exit = doc.clone();
        doc_no_exit.sensors[0].exit_code = None;
        assert!(!doc_no_exit.is_strict_clean());
    }

    /// `allow_failure` softens the local gate only: evidence must still record
    /// the sensor as failed so `--strict` cannot bless a weak run.
    #[test]
    fn soft_failure_is_recorded_as_fail_not_pass() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = crate::config::rust_default();
        cfg.sensors = vec![crate::config::SensorSpec {
            name: "links".into(),
            argv: vec!["true".into()],
            retry: None,
            timeout: None,
            allow_failure: true,
            transient_exit_codes: vec![],
        }];
        let report = VerifyReport {
            ok: true,
            root: dir.path().display().to_string(),
            failed: vec![],
            sensors: vec![crate::report::SensorResult {
                name: "links".into(),
                ok: false,
                exit_code: Some(1),
                duration_ms: 5,
                allow_failure: true,
                output: "boom".into(),
            }],
        };
        let doc = EvidenceDocument::from_run(&cfg, dir.path(), &report, &[], None, 0, 1);
        assert_eq!(doc.sensors[0].verdict, "fail");
        assert_eq!(doc.summary.fail, 1);
        assert_eq!(doc.summary.pass, 0);
        assert!(!doc.is_strict_clean());
    }

    #[test]
    fn serialization_matches_schema() {
        let doc = EvidenceDocument {
            schema_version: 1,
            tool: "do-harness",
            harness_version: "0.1.0",
            git_sha: Some("46463ef".into()),
            started_at: 1_755_852_762,
            finished_at: 1_755_852_810,
            root: "/abs/workspace".into(),
            task_id: None,
            sensor_pack: "rust".into(),
            sensors: vec![EvidenceSensor {
                name: "check".into(),
                argv: vec!["cargo".into(), "check".into()],
                verdict: "pass".into(),
                exit_code: Some(0),
                duration_ms: Some(4200),
                output_sha256: "abc123".into(),
                recorded: true,
            }],
            summary: EvidenceSummary {
                pass: 1,
                fail: 0,
                skip: 0,
                verdict: "pass".into(),
            },
            prev_hash: None,
            chain_hash: "sealed-hash".into(),
        };

        let json = serde_json::to_string_pretty(&doc).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["tool"], "do-harness");
        assert_eq!(value["sensors"][0]["verdict"], "pass");
    }

    /// Evidence types are a stability contract: stale payloads carrying
    /// unknown fields are rejected at the deserialization boundary.
    #[test]
    fn unknown_fields_are_rejected() {
        let sensor = r#"{"name":"check","verdict":"pass","exit_code":0,
             "duration_ms":1,"recorded":false,"bogus":true}"#;
        assert!(serde_json::from_str::<EvidenceSensor>(sensor).is_err());

        let summary = r#"{"pass":1,"fail":0,"skip":0,"verdict":"pass","bogus":true}"#;
        assert!(serde_json::from_str::<EvidenceSummary>(summary).is_err());

        let document = r#"{"schema_version":1,"tool":"do-harness",
             "harness_version":"0.1.0","git_sha":null,"started_at":0,
             "finished_at":0,"root":"/","task_id":null,"sensor_pack":"rust",
             "sensors":[],"summary":{"pass":0,"fail":0,"skip":0,"verdict":"pass"},
             "bogus":true}"#;
        assert!(serde_json::from_str::<EvidenceDocument>(document).is_err());
    }

    /// Sealing links documents and `verify_chain` detects tampering.
    #[test]
    fn seal_and_verify_chain() {
        let mut doc = EvidenceDocument {
            schema_version: 2,
            tool: "do-harness",
            harness_version: "0.1.0",
            git_sha: Some("abc".into()),
            started_at: 1,
            finished_at: 2,
            root: "/tmp".into(),
            task_id: None,
            sensor_pack: "rust".into(),
            sensors: vec![],
            summary: EvidenceSummary {
                pass: 0,
                fail: 0,
                skip: 0,
                verdict: "pass".into(),
            },
            prev_hash: None,
            chain_hash: String::new(),
        };
        doc.seal(None).unwrap();
        assert!(doc.verify_chain(None));
        let genesis = doc.chain_hash.clone();

        let mut next = doc.clone();
        next.seal(Some(genesis.clone())).unwrap();
        assert_eq!(next.prev_hash.as_deref(), Some(genesis.as_str()));
        assert!(next.verify_chain(Some(&genesis)));

        // Tampering with a hashed field invalidates the chain.
        next.git_sha = Some("tampered".into());
        assert!(!next.verify_chain(Some(&genesis)));
    }
}

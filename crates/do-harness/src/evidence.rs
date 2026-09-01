//! Evidence artifact writer for `do-harness verify --evidence`.

use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::report::VerifyReport;

pub const EVIDENCE_SCHEMA_VERSION: u32 = 1;

/// Single sensor result in the evidence artifact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceSensor {
    pub name: String,
    pub verdict: String, // "pass" | "fail" | "skip"
    pub exit_code: Option<i32>,
    pub duration_ms: Option<u64>,
    pub recorded: bool,
}

/// Aggregated summary of sensor verdicts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceSummary {
    pub pass: usize,
    pub fail: usize,
    pub skip: usize,
    pub verdict: String, // "pass" | "fail"
}

/// Schema-versioned evidence document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
}

impl EvidenceDocument {
    /// `--strict` rejects anything weak: skips, missing exit codes or durations.
    pub fn is_strict_clean(&self) -> bool {
        self.summary.verdict == "pass"
            && self.summary.skip == 0
            && self
                .sensors
                .iter()
                .all(|s| s.verdict != "skip" && s.exit_code.is_some() && s.duration_ms.is_some())
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
                    verdict: verdict.to_string(),
                    exit_code: res.exit_code,
                    duration_ms: Some(res.duration_ms),
                    recorded: true,
                });
            } else {
                skip_count += 1;
                sensors.push(EvidenceSensor {
                    name: spec.name.clone(),
                    verdict: "skip".to_string(),
                    exit_code: None,
                    duration_ms: None,
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
        }
    }
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
                verdict: "pass".into(),
                exit_code: Some(0),
                duration_ms: Some(10),
                recorded: true,
            }],
            summary: EvidenceSummary {
                pass: 1,
                fail: 0,
                skip: 0,
                verdict: "pass".into(),
            },
        };
        assert!(doc.is_strict_clean());

        let mut doc_skip = doc.clone();
        doc_skip.summary.skip = 1;
        doc_skip.sensors.push(EvidenceSensor {
            name: "test".into(),
            verdict: "skip".into(),
            exit_code: None,
            duration_ms: None,
            recorded: false,
        });
        assert!(!doc_skip.is_strict_clean());

        let mut doc_no_exit = doc.clone();
        doc_no_exit.sensors[0].exit_code = None;
        assert!(!doc_no_exit.is_strict_clean());
    }

    #[test]
    fn serialization_matches_schema() {
        let doc = EvidenceDocument {
            schema_version: 1,
            tool: "do-harness",
            harness_version: "0.1.0",
            git_sha: Some("46463ef".into()),
            started_at: 1755852762,
            finished_at: 1755852810,
            root: "/abs/workspace".into(),
            task_id: None,
            sensor_pack: "rust".into(),
            sensors: vec![EvidenceSensor {
                name: "check".into(),
                verdict: "pass".into(),
                exit_code: Some(0),
                duration_ms: Some(4200),
                recorded: true,
            }],
            summary: EvidenceSummary {
                pass: 1,
                fail: 0,
                skip: 0,
                verdict: "pass".into(),
            },
        };

        let json = serde_json::to_string_pretty(&doc).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["tool"], "do-harness");
        assert_eq!(value["sensors"][0]["verdict"], "pass");
    }
}

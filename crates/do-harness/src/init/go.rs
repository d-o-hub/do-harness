//! Go language pack candidate detection and config generation.
//!
//! The pack is evidence-driven like the Node/Python packs: probes are gated
//! on the tool's presence, and a sensor is included only when its tooling
//! probe succeeds. `gofmt` gates the format sensor; the `go` toolchain gates
//! vet and test; `staticcheck` is optional and included only when installed.

use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use super::{Candidate, CandidateStatus};
use crate::config::{Config, HooksConfig, SensorSpec};

/// One Go probe run: probed candidates plus the specs of the included ones.
#[derive(Debug, Clone)]
pub struct GoProbe {
    /// Probed candidate signals for the Go pack.
    pub candidates: Vec<Candidate>,
    /// Sensor specs for the included candidates.
    pub specs: Vec<SensorSpec>,
    /// Required tools (`go`, `gofmt`) absent from PATH.
    pub missing_tools: Vec<&'static str>,
}

/// `when-changed` globs shared by every Go sensor.
fn go_globs() -> Vec<String> {
    ["**/*.go", "go.mod", "go.sum"]
        .iter()
        .map(|glob| (*glob).to_owned())
        .collect()
}

/// Full Go probe: candidate statuses plus the included sensor specs.
#[must_use]
pub fn probe_go_full(root: &Path) -> GoProbe {
    let _analysis = super::go_manifest::analyze_go_manifest(root);
    let go_ok = probe("go", &["version"]);
    let gofmt_ok = probe("gofmt", &[]);
    let staticcheck_ok = probe("staticcheck", &["-version"]);

    let mut missing_tools = Vec::new();
    if !go_ok {
        missing_tools.push("go");
    }
    if !gofmt_ok {
        missing_tools.push("gofmt");
    }

    let globs = go_globs();
    let mut candidates = Vec::new();
    let mut specs = Vec::new();

    // 1. fmt — `gofmt -l .` (fails on output)
    let (status, include, detail) = tool_status(gofmt_ok, "gofmt was not found on PATH");
    candidates.push(Candidate {
        name: "fmt".to_owned(),
        status,
        included: include,
        detail,
    });
    if include {
        specs.push(go_spec(
            "fmt",
            vec!["gofmt".to_owned(), "-l".to_owned(), ".".to_owned()],
            &globs,
        ));
    }

    // 2. vet — `go vet ./...`
    let (status, include, detail) = tool_status(go_ok, "go was not found on PATH");
    candidates.push(Candidate {
        name: "vet".to_owned(),
        status,
        included: include,
        detail: detail.clone(),
    });
    if include {
        specs.push(go_spec(
            "vet",
            vec!["bash".to_owned(), "scripts/check-go-vet.sh".to_owned()],
            &globs,
        ));
    }

    // 3. test — `go test ./...`
    let (status, include, detail) = tool_status(go_ok, "go was not found on PATH");
    candidates.push(Candidate {
        name: "test".to_owned(),
        status,
        included: include,
        detail,
    });
    if include {
        specs.push(go_spec(
            "test",
            vec!["bash".to_owned(), "scripts/check-go-test.sh".to_owned()],
            &globs,
        ));
    }

    // 4. staticcheck — optional, included only when installed
    let (status, include, detail) =
        tool_status(staticcheck_ok, "staticcheck was not found on PATH");
    candidates.push(Candidate {
        name: "staticcheck".to_owned(),
        status,
        included: include,
        detail,
    });
    if include {
        specs.push(go_spec(
            "staticcheck",
            vec![
                "bash".to_owned(),
                "scripts/check-go-staticcheck.sh".to_owned(),
            ],
            &globs,
        ));
    }

    GoProbe {
        candidates,
        specs,
        missing_tools,
    }
}

/// Renders the Go `do-harness.toml` for the included sensors.
///
/// # Errors
///
/// Returns an error if rendering to TOML fails.
pub fn generate_go_config(sensors: &[SensorSpec]) -> Result<String> {
    let names: Vec<&str> = sensors.iter().map(|spec| spec.name.as_str()).collect();
    let mut signal_sets: BTreeMap<String, Vec<String>> = BTreeMap::new();

    let feedback = ["fmt", "vet"]
        .iter()
        .filter(|name| names.contains(name))
        .map(ToString::to_string)
        .collect();

    let verification: Vec<String> = ["fmt", "vet", "test"]
        .iter()
        .filter(|name| names.contains(name))
        .map(ToString::to_string)
        .collect();

    let release = ["fmt", "vet", "test", "staticcheck"]
        .iter()
        .filter(|name| names.contains(name))
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    signal_sets.insert("feedback".to_owned(), feedback);
    signal_sets.insert("main".to_owned(), verification.clone());
    signal_sets.insert("verification".to_owned(), verification);
    signal_sets.insert("release".to_owned(), release);

    let pre_commit = ["fmt", "vet"]
        .iter()
        .filter(|name| names.contains(name))
        .map(ToString::to_string)
        .collect();

    let cfg = Config {
        schema_version: crate::config::CONFIG_SCHEMA_VERSION,
        language: Some("go".to_owned()),
        hooks: HooksConfig {
            pre_commit,
            pre_push: Vec::new(),
        },
        env: BTreeMap::new(),
        signal_sets,
        sensors: sensors.to_vec(),
        jobs: None,
        repairs: Vec::new(),
        pr: crate::pr::author_policy::PrConfig::default(),
    };
    let body = toml::to_string(&cfg).context("failed to render generated config")?;
    Ok(format!(
        "# do-harness.toml — generated by `do-harness init` from detected Go tooling.\n\
         # Edit freely; `do-harness explain --set verification --changed` shows what applies.\n{body}"
    ))
}

/// Builds a candidate status/inclusion triple for a required tool probe.
fn tool_status(ok: bool, missing_detail: &str) -> (CandidateStatus, bool, String) {
    if ok {
        (CandidateStatus::Pass, true, String::new())
    } else {
        (CandidateStatus::Missing, false, missing_detail.to_owned())
    }
}

/// Builds a Go sensor spec with the pack-default execution policy.
fn go_spec(name: &str, argv: Vec<String>, globs: &[String]) -> SensorSpec {
    SensorSpec {
        name: name.to_owned(),
        kind: None,
        argv,
        retry: None,
        timeout_justification: None,
        max_output_bytes: None,
        environment_allowlist: None,
        redact_env: Vec::new(),
        timeout: None,
        severity: None,
        fix: None,
        allow_failure: false,
        transient_exit_codes: Vec::new(),
        when_changed: globs.to_vec(),
        artifacts: Vec::new(),
        environment_inputs: Vec::new(),
        coverage_inputs: Vec::new(),
        inputs: Vec::new(),
    }
}

/// Runs a `<program> <args>`-style probe; never uses a shell.
fn probe(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .output()
        .is_ok_and(|output| output.status.success())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn generate_go_config_lists_only_included_sensors() {
        let sensors = vec![go_spec("fmt", vec![], &[]), go_spec("test", vec![], &[])];
        let text = generate_go_config(&sensors).unwrap();
        assert!(text.contains("language = \"go\""));
        assert!(text.contains("name = \"fmt\""));
        assert!(text.contains("name = \"test\""));
        assert!(!text.contains("name = \"vet\""));
        assert!(!text.contains("name = \"staticcheck\""));
    }
}

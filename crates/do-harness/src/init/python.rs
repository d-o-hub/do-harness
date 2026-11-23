//! Python language pack candidate detection and config generation.
//!
//! The pack is evidence-driven like the Node pack: probes are gated on the
//! tool's presence, and a sensor is included only when its tooling probe
//! succeeds. `ruff` gates both lint and format; a type checker is included
//! only when the repository configures one (`mypy`/`pyright`); `pytest` gates
//! test, whose argv runs a zero-tests-collected guard so a fresh project with
//! no tests yet is not a false red.

use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use super::{Candidate, CandidateStatus};
use crate::config::{Config, HooksConfig, SensorSpec};

/// One Python probe run: probed candidates plus the specs of the included ones.
#[derive(Debug, Clone)]
pub struct PythonProbe {
    /// Probed candidate signals for the Python pack.
    pub candidates: Vec<Candidate>,
    /// Sensor specs for the included candidates.
    pub specs: Vec<SensorSpec>,
    /// Required tools (`ruff`, `pytest`) absent from PATH.
    pub missing_tools: Vec<&'static str>,
}

/// `when-changed` globs shared by every Python sensor: sources plus the
/// manifest/config files that select tooling.
fn python_globs() -> Vec<String> {
    [
        "**/*.py",
        "pyproject.toml",
        "requirements.txt",
        "setup.py",
        "setup.cfg",
        "tox.ini",
        "ruff.toml",
    ]
    .iter()
    .map(|glob| (*glob).to_owned())
    .collect()
}

/// Full Python probe: candidate statuses plus the included sensor specs.
#[must_use]
pub fn probe_python_full(root: &Path) -> PythonProbe {
    let analysis = super::python_manifest::analyze_python_manifest(root);
    let ruff_ok = probe("ruff", &["--version"]);
    let pytest_ok = probe("pytest", &["--version"]);

    // The type checker is conditional: configured by the repository, then
    // present on PATH. Unconfigured projects never get a typecheck sensor.
    let typecheck_tool = analysis.type_checker_tool();
    let typecheck_ok = typecheck_tool.is_some_and(|tool| probe(tool, &["--version"]));

    let mut missing_tools = Vec::new();
    if !ruff_ok {
        missing_tools.push("ruff");
    }
    if !pytest_ok {
        missing_tools.push("pytest");
    }

    let globs = python_globs();
    let mut candidates = Vec::new();
    let mut specs = Vec::new();

    // 1. lint — `ruff check`
    let (status, include, detail) = tool_status(ruff_ok, "ruff was not found on PATH");
    candidates.push(Candidate {
        name: "lint".to_owned(),
        status,
        included: include,
        detail,
    });
    if include {
        specs.push(python_spec(
            "lint",
            vec!["ruff".to_owned(), "check".to_owned()],
            &globs,
        ));
    }

    // 2. format — `ruff format --check`
    let (status, include, detail) = tool_status(ruff_ok, "ruff was not found on PATH");
    candidates.push(Candidate {
        name: "format".to_owned(),
        status,
        included: include,
        detail,
    });
    if include {
        specs.push(python_spec(
            "format",
            vec!["ruff".to_owned(), "format".to_owned(), "--check".to_owned()],
            &globs,
        ));
    }

    // 3. typecheck — only when configured and the checker is present
    let (status, include, detail) = match typecheck_tool {
        Some(_tool) if typecheck_ok => (CandidateStatus::Pass, true, String::new()),
        Some(tool) => (
            CandidateStatus::Missing,
            false,
            format!("{tool} was not found on PATH"),
        ),
        None => (
            CandidateStatus::Missing,
            false,
            "no type checker configured (mypy or pyright)".to_owned(),
        ),
    };
    candidates.push(Candidate {
        name: "typecheck".to_owned(),
        status,
        included: include,
        detail,
    });
    if include {
        let tool = typecheck_tool.unwrap_or("mypy");
        let argv = if tool == "pyright" {
            vec!["pyright".to_owned()]
        } else {
            vec!["mypy".to_owned(), ".".to_owned()]
        };
        specs.push(python_spec("typecheck", argv, &globs));
    }

    // 4. test — `pytest` through a zero-tests-collected guard
    let (status, include, detail) = tool_status(pytest_ok, "pytest was not found on PATH");
    candidates.push(Candidate {
        name: "test".to_owned(),
        status,
        included: include,
        detail,
    });
    if include {
        specs.push(python_spec(
            "test",
            vec!["bash".to_owned(), "scripts/check-python-test.sh".to_owned()],
            &globs,
        ));
    }

    PythonProbe {
        candidates,
        specs,
        missing_tools,
    }
}

/// Renders the Python `do-harness.toml` for the included sensors.
///
/// # Errors
///
/// Returns an error if rendering to TOML fails.
pub fn generate_python_config(sensors: &[SensorSpec]) -> Result<String> {
    let names: Vec<&str> = sensors.iter().map(|spec| spec.name.as_str()).collect();
    let mut signal_sets: BTreeMap<String, Vec<String>> = BTreeMap::new();

    let feedback = ["format", "lint", "typecheck"]
        .iter()
        .filter(|name| names.contains(name))
        .map(ToString::to_string)
        .collect();

    let verification: Vec<String> = ["format", "lint", "typecheck", "test"]
        .iter()
        .filter(|name| names.contains(name))
        .map(ToString::to_string)
        .collect();

    signal_sets.insert("feedback".to_owned(), feedback);
    signal_sets.insert("main".to_owned(), verification.clone());
    signal_sets.insert("verification".to_owned(), verification.clone());
    signal_sets.insert("release".to_owned(), verification);

    let pre_commit = ["format", "lint"]
        .iter()
        .filter(|name| names.contains(name))
        .map(ToString::to_string)
        .collect();

    let cfg = Config {
        schema_version: crate::config::CONFIG_SCHEMA_VERSION,
        language: Some("python".to_owned()),
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
        "# do-harness.toml — generated by `do-harness init` from detected Python tooling.\n\
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

/// Builds a Python sensor spec with the pack-default execution policy.
fn python_spec(name: &str, argv: Vec<String>, globs: &[String]) -> SensorSpec {
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

/// Runs a `<program> --version`-style probe; never uses a shell.
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
    fn generate_python_config_lists_only_included_sensors() {
        let sensors = vec![
            python_spec("lint", vec![], &[]),
            python_spec("test", vec![], &[]),
        ];
        let text = generate_python_config(&sensors).unwrap();
        assert!(text.contains("language = \"python\""));
        assert!(text.contains("name = \"lint\""));
        assert!(text.contains("name = \"test\""));
        assert!(!text.contains("name = \"format\""));
        assert!(!text.contains("name = \"typecheck\""));
    }
}

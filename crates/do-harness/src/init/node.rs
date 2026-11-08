//! Node/Web JS/TS language pack candidate detection and config generation.

use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use super::{Candidate, CandidateStatus};
use crate::config::{Config, HooksConfig, SensorSpec};

/// One Node probe run: probed candidates plus the specs of the included ones.
#[derive(Debug, Clone)]
pub struct NodeProbe {
    /// Probed candidate signals for the Node pack.
    pub candidates: Vec<Candidate>,
    /// Sensor specs for the included candidates.
    pub specs: Vec<SensorSpec>,
    /// The selected package-manager name.
    pub package_manager: String,
    /// Whether the selected package manager binary is on PATH.
    pub package_manager_present: bool,
}

/// Full Node probe result with the package-manager availability facts.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn probe_node_full(root: &Path) -> NodeProbe {
    let analysis = super::node_manifest::analyze_node_manifest(root);
    let has_pnpm_lock = root.join("pnpm-lock.yaml").exists();
    let has_pnpm_ws = root.join("pnpm-workspace.yaml").exists();
    let has_yarn_lock = root.join("yarn.lock").exists();
    let has_package_lock = root.join("package-lock.json").exists();

    let pnpm_ok = probe("pnpm", &["--version"]);
    let yarn_ok = probe("yarn", &["--version"]);
    let npm_ok = probe("npm", &["--version"]);

    let pm = if pnpm_ok && (has_pnpm_lock || has_pnpm_ws) {
        "pnpm"
    } else if yarn_ok && has_yarn_lock {
        "yarn"
    } else if npm_ok && has_package_lock {
        "npm"
    } else if pnpm_ok {
        "pnpm"
    } else if yarn_ok {
        "yarn"
    } else {
        "npm"
    };
    let pm_present = match pm {
        "pnpm" => pnpm_ok,
        "yarn" => yarn_ok,
        _ => npm_ok,
    };

    let lockfile = match pm {
        "pnpm" => "pnpm-lock.yaml",
        "yarn" => "yarn.lock",
        _ => "package-lock.json",
    };

    let pm_exec: Vec<String> = match pm {
        "pnpm" => vec!["pnpm".into(), "exec".into()],
        "yarn" => vec!["yarn".into(), "exec".into()],
        _ => vec!["npx".into()],
    };

    let has_script = |key: &str| analysis.has_script(key);
    let get_script = |key: &str| analysis.get_script(key);
    let has_turbo_task = |key: &str| analysis.has_turbo_task(key);
    let has_dep = |dep_name: &str| analysis.has_dep(dep_name);

    let ts_globs = vec![
        "**/*.ts".to_string(),
        "**/*.tsx".to_string(),
        "tsconfig.json".to_string(),
        "**/tsconfig*.json".to_string(),
        "package.json".to_string(),
        lockfile.to_string(),
    ];

    let code_globs = vec![
        "**/*.ts".to_string(),
        "**/*.tsx".to_string(),
        "**/*.js".to_string(),
        "**/*.jsx".to_string(),
        "package.json".to_string(),
        lockfile.to_string(),
    ];

    let build_globs = vec![
        "**/*.ts".to_string(),
        "**/*.tsx".to_string(),
        "**/*.js".to_string(),
        "**/*.jsx".to_string(),
        "tsconfig.json".to_string(),
        "**/tsconfig*.json".to_string(),
        "package.json".to_string(),
        lockfile.to_string(),
    ];

    let supported = analysis.supported_commands();
    let tc_proven = supported.contains(&"typecheck");
    let lint_proven = supported.contains(&"lint");
    let test_proven = supported.contains(&"test");
    let build_proven = supported.contains(&"build");

    let mut candidates = Vec::new();
    let mut specs = Vec::new();

    // The package-manager binary gates every proven signal: without it the
    // contract is not runnable and the sensor is omitted with a `Missing`
    // candidate rather than a silently unverifiable pass.
    let include = pm_present;
    let status = if pm_present {
        CandidateStatus::Pass
    } else {
        CandidateStatus::Missing
    };
    let tool_detail = if pm_present {
        String::new()
    } else {
        format!("{pm} was not found on PATH")
    };

    // 1. typecheck
    if tc_proven {
        let argv = if has_script("typecheck") {
            vec![pm.to_string(), "run".to_string(), "typecheck".to_string()]
        } else if has_turbo_task("typecheck") {
            let mut a = pm_exec.clone();
            a.push("turbo".to_string());
            a.push("run".to_string());
            a.push("typecheck".to_string());
            a
        } else {
            let mut a = pm_exec.clone();
            a.push("tsc".to_string());
            a.push("--noEmit".to_string());
            a
        };

        candidates.push(Candidate {
            name: "typecheck".to_string(),
            status,
            included: include,
            detail: tool_detail.clone(),
        });
        if include {
            specs.push(SensorSpec {
                kind: None,
                fix: None,
                name: "typecheck".to_string(),
                argv,
                retry: None,
                timeout_justification: None,
                max_output_bytes: None,
                environment_allowlist: None,
                redact_env: Vec::new(),
                timeout: None,
                severity: None,
                allow_failure: false,
                transient_exit_codes: Vec::new(),
                when_changed: ts_globs,
                artifacts: Vec::new(),
                environment_inputs: Vec::new(),
                coverage_inputs: Vec::new(),
                inputs: Vec::new(),
            });
        }
    } else {
        candidates.push(Candidate {
            name: "typecheck".to_string(),
            status: CandidateStatus::Missing,
            included: false,
            detail:
                "no typecheck script, turbo task, tsconfig.json, or typescript dependency found"
                    .to_string(),
        });
    }

    // 2. lint
    if lint_proven {
        let argv = if has_script("lint") {
            vec![pm.to_string(), "run".to_string(), "lint".to_string()]
        } else if has_turbo_task("lint") {
            let mut a = pm_exec.clone();
            a.push("turbo".to_string());
            a.push("run".to_string());
            a.push("lint".to_string());
            a
        } else {
            let mut a = pm_exec.clone();
            a.push("eslint".to_string());
            a.push(".".to_string());
            a
        };

        candidates.push(Candidate {
            name: "lint".to_string(),
            status,
            included: include,
            detail: tool_detail.clone(),
        });
        if include {
            specs.push(SensorSpec {
                kind: None,
                fix: None,
                name: "lint".to_string(),
                argv,
                retry: None,
                timeout_justification: None,
                max_output_bytes: None,
                environment_allowlist: None,
                redact_env: Vec::new(),
                timeout: None,
                severity: None,
                allow_failure: false,
                transient_exit_codes: Vec::new(),
                when_changed: code_globs.clone(),
                artifacts: Vec::new(),
                environment_inputs: Vec::new(),
                coverage_inputs: Vec::new(),
                inputs: Vec::new(),
            });
        }
    } else {
        candidates.push(Candidate {
            name: "lint".to_string(),
            status: CandidateStatus::Missing,
            included: false,
            detail: "no lint script, turbo task, eslint config, or eslint dependency found"
                .to_string(),
        });
    }

    // 3. test
    let is_dummy_test = get_script("test").is_some_and(|s| s.contains("no test specified"));
    let has_real_test_script = has_script("test") && !is_dummy_test;

    if test_proven {
        let argv = if has_real_test_script {
            vec![pm.to_string(), "test".to_string()]
        } else if has_turbo_task("test") {
            let mut a = pm_exec.clone();
            a.push("turbo".to_string());
            a.push("run".to_string());
            a.push("test".to_string());
            a
        } else if has_dep("vitest") {
            let mut a = pm_exec.clone();
            a.push("vitest".to_string());
            a.push("run".to_string());
            a
        } else {
            let mut a = pm_exec.clone();
            a.push("jest".to_string());
            a
        };

        candidates.push(Candidate {
            name: "test".to_string(),
            status,
            included: include,
            detail: tool_detail.clone(),
        });
        if include {
            specs.push(SensorSpec {
                kind: None,
                fix: None,
                name: "test".to_string(),
                argv,
                retry: None,
                timeout_justification: None,
                max_output_bytes: None,
                environment_allowlist: None,
                redact_env: Vec::new(),
                timeout: None,
                severity: None,
                allow_failure: false,
                transient_exit_codes: Vec::new(),
                when_changed: code_globs,
                artifacts: Vec::new(),
                environment_inputs: Vec::new(),
                coverage_inputs: Vec::new(),
                inputs: Vec::new(),
            });
        }
    } else {
        candidates.push(Candidate {
            name: "test".to_string(),
            status: CandidateStatus::Missing,
            included: false,
            detail: "no test script, turbo task, vitest, or jest dependency found".to_string(),
        });
    }

    // 4. build
    if build_proven {
        let argv = if has_script("build") {
            vec![pm.to_string(), "run".to_string(), "build".to_string()]
        } else {
            let mut a = pm_exec.clone();
            a.push("turbo".to_string());
            a.push("run".to_string());
            a.push("build".to_string());
            a
        };

        candidates.push(Candidate {
            name: "build".to_string(),
            status,
            included: include,
            detail: tool_detail.clone(),
        });
        if include {
            specs.push(SensorSpec {
                kind: None,
                fix: None,
                name: "build".to_string(),
                argv,
                retry: None,
                timeout_justification: None,
                max_output_bytes: None,
                environment_allowlist: None,
                redact_env: Vec::new(),
                timeout: None,
                severity: None,
                allow_failure: false,
                transient_exit_codes: Vec::new(),
                when_changed: build_globs,
                artifacts: Vec::new(),
                environment_inputs: Vec::new(),
                coverage_inputs: Vec::new(),
                inputs: Vec::new(),
            });
        }
    } else {
        candidates.push(Candidate {
            name: "build".to_string(),
            status: CandidateStatus::Missing,
            included: false,
            detail: "no build script or turbo task found".to_string(),
        });
    }

    NodeProbe {
        candidates,
        specs,
        package_manager: pm.to_owned(),
        package_manager_present: pm_present,
    }
}

/// Renders the Node `do-harness.toml` for the included sensors.
///
/// # Errors
///
/// Returns an error if rendering to TOML fails.
pub fn generate_node_config(sensors: &[SensorSpec]) -> Result<String> {
    let names: Vec<&str> = sensors.iter().map(|spec| spec.name.as_str()).collect();
    let mut signal_sets: BTreeMap<String, Vec<String>> = BTreeMap::new();

    let feedback = ["typecheck", "lint"]
        .iter()
        .filter(|name| names.contains(name))
        .map(ToString::to_string)
        .collect();

    let verification: Vec<String> = ["typecheck", "lint", "test"]
        .iter()
        .filter(|name| names.contains(name))
        .map(ToString::to_string)
        .collect();

    let release = ["typecheck", "lint", "test", "build"]
        .iter()
        .filter(|name| names.contains(name))
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    signal_sets.insert("feedback".to_owned(), feedback);
    signal_sets.insert("main".to_owned(), verification.clone());
    signal_sets.insert("verification".to_owned(), verification);
    signal_sets.insert("release".to_owned(), release);

    let pre_commit = ["typecheck", "lint"]
        .iter()
        .filter(|name| names.contains(name))
        .map(ToString::to_string)
        .collect();

    let cfg = Config {
        language: Some("node".to_owned()),
        hooks: HooksConfig {
            pre_commit,
            pre_push: Vec::new(),
        },
        signal_sets,
        sensors: sensors.to_vec(),
        jobs: None,
        repairs: Vec::new(),
    };
    let body = toml::to_string(&cfg).context("failed to render generated config")?;
    Ok(format!(
        "# do-harness.toml — generated by `do-harness init` from detected tooling.\n\
         # Edit freely; `do-harness explain --set verification --changed` shows what applies.\n{body}"
    ))
}

fn probe(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .output()
        .is_ok_and(|output| output.status.success())
}

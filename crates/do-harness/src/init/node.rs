//! Node/Web JS/TS language pack candidate detection and config generation.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};

use super::{Candidate, CandidateStatus};
use crate::config::{Config, HooksConfig, SensorSpec};

/// Probes Node candidate sensors in `root` and returns candidates and candidate specs.
#[must_use]
pub fn probe_node(root: &Path) -> (Vec<Candidate>, Vec<SensorSpec>) {
    let has_pnpm_lock = root.join("pnpm-lock.yaml").exists();
    let has_pnpm_ws = root.join("pnpm-workspace.yaml").exists();
    let has_yarn_lock = root.join("yarn.lock").exists();
    let has_npm_lock = root.join("package-lock.json").exists();
    let has_turbo_json = root.join("turbo.json").exists();

    let pm = if has_pnpm_lock || has_pnpm_ws || probe("pnpm", &["--version"]) {
        "pnpm"
    } else if has_yarn_lock || probe("yarn", &["--version"]) {
        "yarn"
    } else if has_npm_lock || probe("npm", &["--version"]) {
        "npm"
    } else {
        "npm"
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

    let pkg_json = fs::read_to_string(root.join("package.json")).ok();
    let pkg_val: Option<serde_json::Value> =
        pkg_json.as_deref().and_then(|s| serde_json::from_str(s).ok());
    let scripts = pkg_val
        .as_ref()
        .and_then(|v| v.get("scripts"))
        .and_then(|s| s.as_object());
    let deps = pkg_val.as_ref();

    let turbo_json = fs::read_to_string(root.join("turbo.json")).ok();
    let turbo_val: Option<serde_json::Value> =
        turbo_json.as_deref().and_then(|s| serde_json::from_str(s).ok());
    let turbo_tasks = turbo_val.as_ref().and_then(|v| {
        v.get("tasks")
            .or_else(|| v.get("pipeline"))
            .and_then(|t| t.as_object())
    });

    let has_script =
        |key: &str| -> bool { scripts.map_or(false, |s| s.contains_key(key)) };

    let get_script = |key: &str| -> Option<&str> {
        scripts.and_then(|s| s.get(key)).and_then(|v| v.as_str())
    };

    let has_turbo_task = |key: &str| -> bool {
        has_turbo_json && turbo_tasks.map_or(false, |t| t.contains_key(key))
    };

    let has_dep = |dep_name: &str| -> bool {
        deps.map_or(false, |v| {
            ["dependencies", "devDependencies", "peerDependencies"]
                .iter()
                .any(|key| {
                    v.get(key)
                        .and_then(|d| d.as_object())
                        .map_or(false, |obj| obj.contains_key(dep_name))
                })
        })
    };

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

    let mut candidates = Vec::new();
    let mut specs = Vec::new();

    // 1. typecheck
    let tc_proven = has_script("typecheck")
        || has_turbo_task("typecheck")
        || root.join("tsconfig.json").exists()
        || root.join("tsconfig.base.json").exists()
        || has_dep("typescript");

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
            status: CandidateStatus::Pass,
            included: true,
            detail: String::new(),
        });
        specs.push(SensorSpec {
            name: "typecheck".to_string(),
            argv,
            retry: None,
            timeout: None,
            severity: None,
            allow_failure: false,
            transient_exit_codes: Vec::new(),
            when_changed: ts_globs,
            artifacts: Vec::new(),
            coverage_inputs: Vec::new(),
        });
    } else {
        candidates.push(Candidate {
            name: "typecheck".to_string(),
            status: CandidateStatus::Missing,
            included: false,
            detail: "no typecheck script, turbo task, tsconfig.json, or typescript dependency found".to_string(),
        });
    }

    // 2. lint
    let has_eslint_config = root.join(".eslintrc").exists()
        || root.join(".eslintrc.json").exists()
        || root.join(".eslintrc.js").exists()
        || root.join(".eslintrc.cjs").exists()
        || root.join("eslint.config.js").exists()
        || root.join("eslint.config.mjs").exists()
        || root.join("eslint.config.cjs").exists();

    let lint_proven = has_script("lint")
        || has_turbo_task("lint")
        || has_eslint_config
        || has_dep("eslint");

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
            status: CandidateStatus::Pass,
            included: true,
            detail: String::new(),
        });
        specs.push(SensorSpec {
            name: "lint".to_string(),
            argv,
            retry: None,
            timeout: None,
            severity: None,
            allow_failure: false,
            transient_exit_codes: Vec::new(),
            when_changed: code_globs.clone(),
            artifacts: Vec::new(),
            coverage_inputs: Vec::new(),
        });
    } else {
        candidates.push(Candidate {
            name: "lint".to_string(),
            status: CandidateStatus::Missing,
            included: false,
            detail: "no lint script, turbo task, eslint config, or eslint dependency found".to_string(),
        });
    }

    // 3. test
    let is_dummy_test =
        get_script("test").map_or(false, |s| s.contains("no test specified"));
    let has_real_test_script = has_script("test") && !is_dummy_test;
    let test_proven = has_real_test_script
        || has_turbo_task("test")
        || has_dep("vitest")
        || has_dep("jest");

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
            status: CandidateStatus::Pass,
            included: true,
            detail: String::new(),
        });
        specs.push(SensorSpec {
            name: "test".to_string(),
            argv,
            retry: None,
            timeout: None,
            severity: None,
            allow_failure: false,
            transient_exit_codes: Vec::new(),
            when_changed: code_globs,
            artifacts: Vec::new(),
            coverage_inputs: Vec::new(),
        });
    } else {
        candidates.push(Candidate {
            name: "test".to_string(),
            status: CandidateStatus::Missing,
            included: false,
            detail: "no test script, turbo task, vitest, or jest dependency found".to_string(),
        });
    }

    // 4. build
    let build_proven = has_script("build") || has_turbo_task("build");

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
            status: CandidateStatus::Pass,
            included: true,
            detail: String::new(),
        });
        specs.push(SensorSpec {
            name: "build".to_string(),
            argv,
            retry: None,
            timeout: None,
            severity: None,
            allow_failure: false,
            transient_exit_codes: Vec::new(),
            when_changed: build_globs,
            artifacts: Vec::new(),
            coverage_inputs: Vec::new(),
        });
    } else {
        candidates.push(Candidate {
            name: "build".to_string(),
            status: CandidateStatus::Missing,
            included: false,
            detail: "no build script or turbo task found".to_string(),
        });
    }

    (candidates, specs)
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

    let verification = ["typecheck", "lint", "test"]
        .iter()
        .filter(|name| names.contains(name))
        .map(ToString::to_string)
        .collect();

    let release = ["typecheck", "lint", "test", "build"]
        .iter()
        .filter(|name| names.contains(name))
        .map(ToString::to_string)
        .collect();

    signal_sets.insert("feedback".to_owned(), feedback);
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
    };
    let body = toml::to_string(&cfg).context("failed to render generated config")?;
    Ok(format!(
        "# do-harness.toml — generated by `do-harness init --language node` from detected tooling.\n\
         # Edit freely; `do-harness explain --set verification --changed` shows what applies.\n{body}"
    ))
}

fn probe(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .output()
        .is_ok_and(|output| output.status.success())
}

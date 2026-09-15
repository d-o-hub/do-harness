//! Node/Web JS/TS language pack candidate detection and config generation.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};

use super::{Candidate, CandidateStatus};
use crate::config::{Config, HooksConfig, SensorSpec};

const PM_PNPM: &str = "pnpm";
const PM_YARN: &str = "yarn";
const PM_NPM: &str = "npm";

fn detect_pm(path: &Path) -> &'static str {
    let pinned =
        if path.join("pnpm-lock.yaml").exists() || path.join("pnpm-workspace.yaml").exists() {
            Some(PM_PNPM)
        } else if path.join("yarn.lock").exists() {
            Some(PM_YARN)
        } else if path.join("package-lock.json").exists() {
            Some(PM_NPM)
        } else {
            None
        };
    // Prefer the pinned manager when its binary is installed; otherwise use
    // the first installed manager so generated sensors always execute instead
    // of failing on a missing runner.
    if pinned.is_some_and(probe) {
        pinned.unwrap_or(PM_NPM)
    } else if probe(PM_PNPM) {
        PM_PNPM
    } else if probe(PM_YARN) {
        PM_YARN
    } else {
        PM_NPM
    }
}

fn pm_lockfile(pm: &str) -> &'static str {
    match pm {
        PM_PNPM => "pnpm-lock.yaml",
        PM_YARN => "yarn.lock",
        _ => "package-lock.json",
    }
}

fn pm_exec(pm: &str) -> Vec<String> {
    match pm {
        PM_PNPM => vec!["pnpm".into(), "exec".into()],
        PM_YARN => vec!["yarn".into(), "exec".into()],
        _ => vec!["npx".into()],
    }
}

fn parse_pkg_json(root: &Path) -> Option<serde_json::Value> {
    let pkg_json = fs::read_to_string(root.join("package.json")).ok();
    pkg_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
}

fn parse_turbo_json(root: &Path) -> Option<serde_json::Value> {
    let turbo_json = fs::read_to_string(root.join("turbo.json")).ok();
    turbo_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
}

fn json_object<'a>(
    val: Option<&'a serde_json::Value>,
    key: &str,
) -> Option<&'a serde_json::Map<String, serde_json::Value>> {
    val?.get(key)?.as_object()
}

fn has_script(scripts: Option<&serde_json::Map<String, serde_json::Value>>, key: &str) -> bool {
    scripts.is_some_and(|s| s.contains_key(key))
}

fn get_script<'a>(
    scripts: Option<&'a serde_json::Map<String, serde_json::Value>>,
    key: &str,
) -> Option<&'a str> {
    scripts.and_then(|s| s.get(key)).and_then(|v| v.as_str())
}

fn is_dummy_test_script(scripts: Option<&serde_json::Map<String, serde_json::Value>>) -> bool {
    get_script(scripts, "test").is_some_and(|s| s.contains("no test specified"))
}

fn has_turbo_task(turbo_val: Option<&serde_json::Value>, key: &str) -> bool {
    let tasks = turbo_val.and_then(|v| {
        v.get("tasks")
            .or_else(|| v.get("pipeline"))
            .and_then(|t| t.as_object())
    });
    tasks.is_some_and(|t| t.contains_key(key))
}

fn has_dep(deps: Option<&serde_json::Value>, dep_name: &str) -> bool {
    deps.is_some_and(|v| {
        ["dependencies", "devDependencies", "peerDependencies"]
            .iter()
            .any(|key| {
                v.get(key)
                    .and_then(|d| d.as_object())
                    .is_some_and(|obj| obj.contains_key(dep_name))
            })
    })
}

fn ts_globs(lockfile: &str) -> Vec<String> {
    vec![
        "**/*.ts".to_string(),
        "**/*.tsx".to_string(),
        "tsconfig.json".to_string(),
        "**/tsconfig*.json".to_string(),
        "package.json".to_string(),
        lockfile.to_string(),
    ]
}

fn code_globs(lockfile: &str) -> Vec<String> {
    vec![
        "**/*.ts".to_string(),
        "**/*.tsx".to_string(),
        "**/*.js".to_string(),
        "**/*.jsx".to_string(),
        "package.json".to_string(),
        lockfile.to_string(),
    ]
}

fn build_globs(lockfile: &str) -> Vec<String> {
    vec![
        "**/*.ts".to_string(),
        "**/*.tsx".to_string(),
        "**/*.js".to_string(),
        "**/*.jsx".to_string(),
        "tsconfig.json".to_string(),
        "**/tsconfig*.json".to_string(),
        "package.json".to_string(),
        lockfile.to_string(),
    ]
}

fn push_candidate(candidates: &mut Vec<Candidate>, name: &str, included: bool, detail: String) {
    candidates.push(Candidate {
        name: name.to_string(),
        status: if included {
            CandidateStatus::Pass
        } else {
            CandidateStatus::Missing
        },
        included,
        detail,
    });
}

fn push_spec(
    specs: &mut Vec<SensorSpec>,
    name: &str,
    argv: Vec<String>,
    when_changed: Vec<String>,
) {
    specs.push(SensorSpec {
        name: name.to_string(),
        argv,
        retry: None,
        timeout: None,
        severity: None,
        allow_failure: false,
        transient_exit_codes: Vec::new(),
        when_changed,
        artifacts: Vec::new(),
        coverage_inputs: Vec::new(),
    });
}

/// Shared inputs for the per-sensor probe builders below.
struct ProbeCtx<'a> {
    root: &'a Path,
    pm: &'static str,
    exec: Vec<String>,
    scripts: Option<&'a serde_json::Map<String, serde_json::Value>>,
    turbo: Option<&'a serde_json::Value>,
    deps: Option<&'a serde_json::Value>,
}

fn probe_typecheck(
    ctx: &ProbeCtx<'_>,
    candidates: &mut Vec<Candidate>,
    specs: &mut Vec<SensorSpec>,
    globs: Vec<String>,
) {
    let proven = has_script(ctx.scripts, "typecheck")
        || has_turbo_task(ctx.turbo, "typecheck")
        || ctx.root.join("tsconfig.json").exists()
        || ctx.root.join("tsconfig.base.json").exists()
        || has_dep(ctx.deps, "typescript");

    if proven {
        let argv = if has_script(ctx.scripts, "typecheck") {
            vec![
                ctx.pm.to_string(),
                "run".to_string(),
                "typecheck".to_string(),
            ]
        } else if has_turbo_task(ctx.turbo, "typecheck") {
            let mut a = ctx.exec.clone();
            a.extend(
                ["turbo", "run", "typecheck"]
                    .iter()
                    .map(ToString::to_string),
            );
            a
        } else {
            let mut a = ctx.exec.clone();
            a.extend(["tsc", "--noEmit"].iter().map(ToString::to_string));
            a
        };
        push_candidate(candidates, "typecheck", true, String::new());
        push_spec(specs, "typecheck", argv, globs);
    } else {
        push_candidate(
            candidates,
            "typecheck",
            false,
            "no typecheck script, turbo task, tsconfig.json, or typescript dependency found"
                .to_string(),
        );
    }
}

fn probe_lint(
    ctx: &ProbeCtx<'_>,
    candidates: &mut Vec<Candidate>,
    specs: &mut Vec<SensorSpec>,
    globs: Vec<String>,
) {
    let has_eslint_config = ctx.root.join(".eslintrc").exists()
        || ctx.root.join(".eslintrc.json").exists()
        || ctx.root.join(".eslintrc.js").exists()
        || ctx.root.join(".eslintrc.cjs").exists()
        || ctx.root.join("eslint.config.js").exists()
        || ctx.root.join("eslint.config.mjs").exists()
        || ctx.root.join("eslint.config.cjs").exists();
    let proven = has_script(ctx.scripts, "lint")
        || has_turbo_task(ctx.turbo, "lint")
        || has_eslint_config
        || has_dep(ctx.deps, "eslint");

    if proven {
        let argv = if has_script(ctx.scripts, "lint") {
            vec![ctx.pm.to_string(), "run".to_string(), "lint".to_string()]
        } else if has_turbo_task(ctx.turbo, "lint") {
            let mut a = ctx.exec.clone();
            a.extend(["turbo", "run", "lint"].iter().map(ToString::to_string));
            a
        } else {
            let mut a = ctx.exec.clone();
            a.extend(["eslint", "."].iter().map(ToString::to_string));
            a
        };
        push_candidate(candidates, "lint", true, String::new());
        push_spec(specs, "lint", argv, globs);
    } else {
        push_candidate(
            candidates,
            "lint",
            false,
            "no lint script, turbo task, eslint config, or eslint dependency found".to_string(),
        );
    }
}

fn probe_test(
    ctx: &ProbeCtx<'_>,
    candidates: &mut Vec<Candidate>,
    specs: &mut Vec<SensorSpec>,
    globs: Vec<String>,
) {
    let dummy = is_dummy_test_script(ctx.scripts);
    let has_real_test = has_script(ctx.scripts, "test") && !dummy;
    let proven = has_real_test
        || has_turbo_task(ctx.turbo, "test")
        || has_dep(ctx.deps, "vitest")
        || has_dep(ctx.deps, "jest");

    if proven {
        let argv = if has_real_test {
            vec![ctx.pm.to_string(), "test".to_string()]
        } else if has_turbo_task(ctx.turbo, "test") {
            let mut a = ctx.exec.clone();
            a.extend(["turbo", "run", "test"].iter().map(ToString::to_string));
            a
        } else if has_dep(ctx.deps, "vitest") {
            let mut a = ctx.exec.clone();
            a.extend(["vitest", "run"].iter().map(ToString::to_string));
            a
        } else {
            let mut a = ctx.exec.clone();
            a.push("jest".to_string());
            a
        };
        push_candidate(candidates, "test", true, String::new());
        push_spec(specs, "test", argv, globs);
    } else {
        push_candidate(
            candidates,
            "test",
            false,
            "no test script, turbo task, vitest, or jest dependency found".to_string(),
        );
    }
}

fn probe_build(
    ctx: &ProbeCtx<'_>,
    candidates: &mut Vec<Candidate>,
    specs: &mut Vec<SensorSpec>,
    globs: Vec<String>,
) {
    let proven = has_script(ctx.scripts, "build") || has_turbo_task(ctx.turbo, "build");

    if proven {
        let argv = if has_script(ctx.scripts, "build") {
            vec![ctx.pm.to_string(), "run".to_string(), "build".to_string()]
        } else {
            let mut a = ctx.exec.clone();
            a.extend(["turbo", "run", "build"].iter().map(ToString::to_string));
            a
        };
        push_candidate(candidates, "build", true, String::new());
        push_spec(specs, "build", argv, globs);
    } else {
        push_candidate(
            candidates,
            "build",
            false,
            "no build script or turbo task found".to_string(),
        );
    }
}

/// Probes Node candidate sensors in `root` and returns candidates and candidate specs.
#[must_use]
pub fn probe_node(root: &Path) -> (Vec<Candidate>, Vec<SensorSpec>) {
    let pm = detect_pm(root);
    let lockfile = pm_lockfile(pm);

    let pkg_val = parse_pkg_json(root);
    let turbo_val = parse_turbo_json(root);
    let ctx = ProbeCtx {
        root,
        pm,
        exec: pm_exec(pm),
        scripts: json_object(pkg_val.as_ref(), "scripts"),
        turbo: turbo_val.as_ref(),
        deps: pkg_val.as_ref(),
    };

    let mut candidates = Vec::new();
    let mut specs = Vec::new();
    probe_typecheck(&ctx, &mut candidates, &mut specs, ts_globs(lockfile));
    probe_lint(&ctx, &mut candidates, &mut specs, code_globs(lockfile));
    probe_test(&ctx, &mut candidates, &mut specs, code_globs(lockfile));
    probe_build(&ctx, &mut candidates, &mut specs, build_globs(lockfile));

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

fn probe(program: &str) -> bool {
    Command::new(program)
        .args(["--version"])
        .output()
        .is_ok_and(|output| output.status.success())
}

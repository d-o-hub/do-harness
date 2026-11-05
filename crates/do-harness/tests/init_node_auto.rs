//! Auto-selection of the Node pack for supported `package.json` projects
//! (issue #477).
//!
//! A package.json that proves a supported Node command selects the Node pack
//! from plain `init`; malformed or unsupported manifests stay Generic with a
//! structured diagnostic; explicit and declared language precedence is
//! preserved; missing package-manager tooling yields an INCOMPLETE baseline,
//! never a claimed green; repeated init is byte-preserving; and generated
//! hooks/signal sets reference only configured sensor names.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;

mod support;

use support::{harness, run};

/// The generic template's config: a valid, zero-sensor declared-language
/// fixture.
const GENERIC_CONFIG: &str = r#"language = "generic"

[hooks]
pre-commit = []
pre-push = []
"#;

fn write_package_json(root: &Path, body: &str) {
    fs::write(root.join("package.json"), body).unwrap();
}

fn write_npm_project(root: &Path) {
    write_package_json(
        root,
        r#"{
  "name": "npm-app",
  "scripts": {
    "test": "node -e \"\"",
    "lint": "node -e \"\""
  }
}"#,
    );
    // A lockfile makes the probe select npm deterministically.
    fs::write(root.join("package-lock.json"), "{}\n").unwrap();
}

fn parse_run(cmd: &mut Command) -> (Option<i32>, Value, String) {
    let (code, stdout, stderr) = run(cmd);
    let report = serde_json::from_str(&stdout).expect("parse init json report");
    (code, report, stderr)
}

fn run_init(root: &Path) -> (Option<i32>, Value, String) {
    let mut cmd = harness(root);
    cmd.args(["init", "--format", "json"]);
    parse_run(&mut cmd)
}

fn assert_report(code: Option<i32>, report: &Value, stderr: &str) {
    assert_eq!(code, Some(0), "init failed:\n{stderr}");
    let _ = report;
}

fn sensor_names_from_config(config: &str) -> Vec<String> {
    let value: toml::Value = toml::from_str(config).expect("parse generated toml");
    value["sensors"]
        .as_array()
        .map(|sensors| {
            sensors
                .iter()
                .filter_map(|s| s["name"].as_str().map(ToOwned::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// Whether `tool` is an executable on PATH, mirroring the CLI's own probe
/// semantics (file exists, and executable on Unix). Used to gate strict
/// npm/pnpm-present assertions on hosts that actually have the manager, so
/// the same fixture asserts green/configured sensors where the tool exists
/// and the INCOMPLETE omitted-semantics path where it does not.
fn tool_on_path(tool: &str) -> bool {
    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };
    let names = [
        tool.to_string(),
        format!("{tool}.exe"),
        format!("{tool}.cmd"),
        format!("{tool}.bat"),
    ];
    std::env::split_paths(&path_var).any(|dir| {
        names.iter().any(|name| {
            let candidate = dir.join(name);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                candidate.is_file()
                    && candidate
                        .metadata()
                        .is_ok_and(|meta| meta.permissions().mode() & 0o111 != 0)
            }
            #[cfg(not(unix))]
            {
                candidate.is_file()
            }
        })
    })
}

/// A clean rustfmt-compliant crate with one passing unit test.
fn write_clean_crate(root: &Path) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(
        root.join("src/lib.rs"),
        "pub fn answer() -> u32 {\n    42\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn answer_is_42() {\n        assert_eq!(super::answer(), 42);\n    }\n}\n",
    )
    .unwrap();
}

#[test]
fn npm_project_selects_node_and_scaffolds_config() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_npm_project(root);

    let (code, report, stderr) = run_init(root);
    assert_report(code, &report, &stderr);
    assert_eq!(report["language"], serde_json::json!("node"));
    let candidates = report["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 4, "{candidates:?}");
    if tool_on_path("npm") {
        // A machine with npm must get the runnable contract: green baseline,
        // the proven sensors included, and the generated config configured.
        assert_eq!(report["baseline"]["state"], serde_json::json!("green"));
        assert!(
            candidates
                .iter()
                .any(|c| c["name"] == "test" && c["included"] == true)
        );
        assert!(
            candidates
                .iter()
                .any(|c| c["name"] == "lint" && c["included"] == true)
        );
        assert!(
            candidates
                .iter()
                .any(|c| c["name"] == "typecheck" && c["included"] == false)
        );
        let config = fs::read_to_string(root.join("do-harness.toml")).unwrap();
        assert!(config.contains("language = \"node\""));
        assert!(config.contains("name = \"lint\""));
        assert!(config.contains("name = \"test\""));
    } else {
        // Without npm the same manifest still selects Node, but the contract
        // is INCOMPLETE with all candidates omitted — never a claimed green.
        eprintln!("npm not on PATH; asserting the INCOMPLETE omitted-semantics path");
        assert_eq!(report["baseline"]["state"], serde_json::json!("incomplete"));
        for candidate in candidates {
            assert_eq!(
                candidate["included"],
                serde_json::json!(false),
                "{candidate:?}"
            );
        }
        let diagnostics = report["diagnostics"].as_array().unwrap();
        assert!(
            diagnostics
                .iter()
                .any(|d| d["code"] == "missing_package_manager"),
            "expected a missing_package_manager diagnostic: {diagnostics:?}"
        );
    }
}

#[test]
fn pnpm_project_selects_node() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_package_json(
        root,
        r#"{
  "name": "pnpm-app",
  "scripts": {
    "test": "node -e \"\"",
    "build": "node -e \"\""
  }
}"#,
    );
    fs::write(root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n").unwrap();

    let (code, report, stderr) = run_init(root);
    assert_report(code, &report, &stderr);
    assert_eq!(report["language"], serde_json::json!("node"));

    let config = fs::read_to_string(root.join("do-harness.toml")).unwrap();
    assert!(config.contains("language = \"node\""));
    if tool_on_path("pnpm") || tool_on_path("npm") {
        // Either manager makes the proven scripts runnable; the generated
        // config must then include the test/build sensors.
        assert!(config.contains("name = \"test\""));
        assert!(config.contains("name = \"build\""));
        assert_eq!(report["baseline"]["state"], serde_json::json!("green"));
    } else {
        eprintln!("pnpm/npm not on PATH; asserting the INCOMPLETE path");
        assert_eq!(report["baseline"]["state"], serde_json::json!("incomplete"));
    }
}

#[test]
fn missing_package_manager_selects_node_with_incomplete_baseline() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_package_json(
        root,
        r#"{"name": "app", "scripts": {"test": "node --test"}}"#,
    );

    // Strip PATH to nothing so no package-manager binary exists.
    let empty_bin = tempfile::tempdir().unwrap();
    let path_var =
        std::env::join_paths(std::iter::once(empty_bin.path().to_path_buf())).expect("join_paths");

    let mut cmd = harness(root);
    cmd.env("PATH", &path_var);
    cmd.args(["init", "--format", "json"]);
    let (code, report, stderr) = parse_run(&mut cmd);

    // The manifest proved a supported command, so Node is selected even
    // without the package manager; the contract is INCOMPLETE, not green.
    assert_report(code, &report, &stderr);
    assert_eq!(report["language"], serde_json::json!("node"));
    assert_eq!(report["baseline"]["state"], serde_json::json!("incomplete"));
    let diagnostics = report["diagnostics"].as_array().unwrap();
    assert!(
        diagnostics
            .iter()
            .any(|d| d["code"] == "missing_package_manager"),
        "expected a missing_package_manager diagnostic: {diagnostics:?}"
    );
    for candidate in report["candidates"].as_array().unwrap() {
        assert_eq!(
            candidate["included"],
            serde_json::json!(false),
            "{candidate:?}"
        );
    }
}

#[test]
fn unsupported_package_json_selects_generic_with_diagnostic() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_package_json(root, "{}\n");

    let (code, report, stderr) = run_init(root);
    assert_report(code, &report, &stderr);
    assert_eq!(report["language"], serde_json::json!("generic"));
    assert_eq!(report["baseline"]["state"], serde_json::json!("vacuous"));
    let diagnostics = report["diagnostics"].as_array().unwrap();
    assert!(
        diagnostics
            .iter()
            .any(|d| d["code"] == "no_supported_node_commands"),
        "expected a no_supported_node_commands diagnostic: {diagnostics:?}"
    );
}

#[test]
fn malformed_package_json_selects_generic_with_diagnostic() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_package_json(root, "{oops");

    let (code, report, stderr) = run_init(root);
    assert_report(code, &report, &stderr);
    assert_eq!(report["language"], serde_json::json!("generic"));
    assert_eq!(report["baseline"]["state"], serde_json::json!("vacuous"));
    let diagnostics = report["diagnostics"].as_array().unwrap();
    assert!(
        diagnostics
            .iter()
            .any(|d| d["code"] == "invalid_package_json"),
        "expected an invalid_package_json diagnostic: {diagnostics:?}"
    );
}

#[test]
fn explicit_language_wins_over_supported_package_json() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_npm_project(root);

    let mut cmd = harness(root);
    cmd.args(["init", "--language", "generic", "--format", "json"]);
    let (code, report, stderr) = parse_run(&mut cmd);
    assert_report(code, &report, &stderr);
    assert_eq!(report["language"], serde_json::json!("generic"));
    assert!(
        report["diagnostics"].as_array().unwrap().is_empty(),
        "an explicit override must not emit selection diagnostics"
    );
}

#[test]
fn existing_declared_language_wins_over_supported_package_json() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_npm_project(root);
    fs::write(root.join("do-harness.toml"), GENERIC_CONFIG).unwrap();

    let (code, report, stderr) = run_init(root);
    assert_report(code, &report, &stderr);
    assert_eq!(report["language"], serde_json::json!("generic"));
    assert_eq!(report["baseline"]["state"], serde_json::json!("vacuous"));
    // The declared config is left untouched.
    assert_eq!(
        fs::read_to_string(root.join("do-harness.toml")).unwrap(),
        GENERIC_CONFIG
    );
}

#[test]
fn cargo_plus_package_json_selects_rust() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_clean_crate(root);
    write_npm_project(root);

    let (code, report, stderr) = run_init(root);
    assert_report(code, &report, &stderr);
    assert_eq!(report["language"], serde_json::json!("rust"));
    assert_eq!(report["baseline"]["state"], serde_json::json!("green"));
    let config = fs::read_to_string(root.join("do-harness.toml")).unwrap();
    assert!(config.contains("language = \"rust\""));
}

#[test]
fn repeated_init_preserves_files() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_npm_project(root);
    fs::write(root.join("README.md"), "user file\n").unwrap();

    let (code, report, stderr) = run_init(root);
    assert_report(code, &report, &stderr);

    let snapshot = |root: &Path| {
        [
            "do-harness.toml",
            "AGENTS.md",
            "USE_RESTRICTIONS.md",
            "package.json",
            "README.md",
        ]
        .iter()
        .map(|name| {
            let path = root.join(name);
            (name.to_string(), fs::read(&path).unwrap())
        })
        .collect::<Vec<_>>()
    };
    let first = snapshot(root);

    let (code, second, stderr) = run_init(root);
    assert_report(code, &second, &stderr);
    assert!(
        second["skipped"].as_array().unwrap().len() >= 2,
        "re-init should skip existing files: {:?}",
        second["skipped"]
    );

    let second_bytes = snapshot(root);
    for ((name, a), (_, b)) in first.iter().zip(&second_bytes) {
        assert_eq!(a, b, "{name} changed across repeated init");
    }
}

#[test]
fn generated_hooks_and_signal_sets_reference_configured_sensors() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_npm_project(root);

    let (code, report, stderr) = run_init(root);
    assert_report(code, &report, &stderr);

    let config = fs::read_to_string(root.join("do-harness.toml")).unwrap();
    let value: toml::Value = toml::from_str(&config).expect("parse generated toml");
    let sensor_names = sensor_names_from_config(&config);
    if tool_on_path("npm") {
        // A machine with npm must get a configured sensor set.
        assert!(
            !sensor_names.is_empty(),
            "generated config must configure sensors: {sensor_names:?}"
        );
    } else {
        eprintln!(
            "npm not on PATH; hook/set references are vacuously valid for an empty sensor set"
        );
        assert!(
            sensor_names.is_empty(),
            "without npm no sensor may be configured: {sensor_names:?}"
        );
    }

    let hooks = value["hooks"].as_table().expect("hooks table");
    for (hook, names) in hooks {
        for name in names.as_array().expect("hook sensor list") {
            let name = name.as_str().unwrap();
            assert!(
                sensor_names.iter().any(|s| s == name),
                "hook {hook} references {name}, which is not a configured sensor: {sensor_names:?}"
            );
        }
    }
    let sets = value["signal-sets"].as_table().expect("signal-sets table");
    for (set, names) in sets {
        for name in names.as_array().expect("set sensor list") {
            let name = name.as_str().unwrap();
            assert!(
                sensor_names.iter().any(|s| s == name),
                "signal set {set} references {name}, which is not a configured sensor: {sensor_names:?}"
            );
        }
    }
}

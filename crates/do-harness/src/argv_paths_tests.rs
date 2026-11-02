#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use std::fs;

fn spec(argv: &[&str]) -> SensorSpec {
    SensorSpec {
        kind: None,
        fix: None,
        name: "probe".to_owned(),
        argv: argv.iter().map(|a| (*a).to_owned()).collect(),
        retry: None,
        timeout_justification: None,
        max_output_bytes: None,
        environment_allowlist: None,
        redact_env: Vec::new(),
        timeout: None,
        severity: None,
        allow_failure: false,
        transient_exit_codes: Vec::new(),
        artifacts: Vec::new(),
        environment_inputs: Vec::new(),
        coverage_inputs: Vec::new(),
        inputs: Vec::new(),
        when_changed: Vec::new(),
    }
}

#[test]
fn reports_a_missing_script_argument() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("scripts")).unwrap();
    let missing = missing_paths(dir.path(), &spec(&["bash", "scripts/does-not-exist.sh"]));
    assert_eq!(missing, ["scripts/does-not-exist.sh"]);
}

#[test]
fn accepts_an_existing_script_argument() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("scripts")).unwrap();
    fs::write(dir.path().join("scripts/check.sh"), "exit 0\n").unwrap();
    assert_eq!(
        missing_paths(dir.path(), &spec(&["bash", "scripts/check.sh"])),
        Vec::<String>::new()
    );
}

#[test]
fn ignores_flags_values_and_shell_expressions() {
    let dir = tempfile::tempdir().unwrap();
    let missing = missing_paths(
        dir.path(),
        &spec(&[
            "cargo",
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
            "-p",
            "guardian-proxy",
            "--features",
            "agt-governance",
            "bash",
            "-c",
            "echo scanning scripts/does-not-exist.sh; exit 0",
        ]),
    );
    assert_eq!(missing, Vec::<String>::new());
}

#[test]
fn ignores_git_refs_that_are_not_paths() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        missing_paths(dir.path(), &spec(&["git", "diff", "origin/main"])),
        Vec::<String>::new()
    );
}

#[test]
fn reports_a_missing_bare_file_with_a_known_extension() {
    let dir = tempfile::tempdir().unwrap();
    let missing = missing_paths(dir.path(), &spec(&["tool", "Cargo.toml"]));
    assert_eq!(missing, ["Cargo.toml"]);
}

#[test]
fn reports_a_missing_dotfile() {
    let dir = tempfile::tempdir().unwrap();
    let missing = missing_paths(dir.path(), &spec(&["tool", ".gitignore"]));
    assert_eq!(missing, [".gitignore"]);
}

#[test]
fn skips_the_program_argument() {
    let dir = tempfile::tempdir().unwrap();
    // A missing program already fails loudly at spawn; it is not an argv path.
    assert_eq!(
        missing_paths(dir.path(), &spec(&["scripts/missing-tool"])),
        Vec::<String>::new()
    );
}

#[test]
fn collects_missing_paths_per_sensor() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("scripts")).unwrap();
    let specs = vec![
        spec(&["bash", "scripts/missing-a.sh"]),
        spec(&["bash", "scripts/missing-b.sh"]),
    ];
    let missing = missing_paths_for(dir.path(), &specs);
    assert_eq!(missing.len(), 2);
    assert_eq!(missing[0].sensor, "probe");
    assert_eq!(missing[0].arg, "scripts/missing-a.sh");
    assert_eq!(missing[1].arg, "scripts/missing-b.sh");
}

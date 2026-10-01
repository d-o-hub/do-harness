//! Explicit-root policy selection boundaries.
//!
//! `--root DIR` without `DIR/do-harness.toml` (and without `--config FILE`)
//! must be a usage error before sensors, beat recording, or evidence writes:
//! the built-in pack must not be silently selected for an explicitly targeted
//! directory, and the invocation cwd's policy must not leak into it.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// One-line TOML sensor block for a fixture config.
fn sensor_config(name: &str, argv: &str) -> String {
    format!(
        "# test fixture policy\n[[sensors]]\nname = \"{name}\"\nargv = {argv}\nwhen-changed = [\"**/*\"]\n"
    )
}

/// Two directories: `a` is configured (`cwd-policy`), `b` is `a/nested` and
/// starts config-less. Invocations run from `a` targeting `b`.
struct Fixture {
    _temp: tempfile::TempDir,
    a: PathBuf,
    b: PathBuf,
}

fn fixture() -> Fixture {
    let temp = tempfile::tempdir().expect("tempdir");
    let a = temp.path().join("a");
    let b = a.join("nested");
    std::fs::create_dir_all(&b).expect("create fixture dirs");
    std::fs::write(
        a.join("do-harness.toml"),
        sensor_config("cwd-policy", r#"["true"]"#),
    )
    .expect("write A config");
    Fixture { _temp: temp, a, b }
}

/// Builds a `do-harness` command invoked from `cwd` (never a process-global
/// directory change, so tests stay parallel-safe).
fn harness(cwd: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    cmd.current_dir(cwd);
    cmd
}

/// Runs a harness command, returning (exit code, `stdout`, `stderr`).
fn run(cmd: &mut Command) -> (Option<i32>, String, String) {
    let output = cmd.output().expect("spawn do-harness");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Sensor names from an `explain --format json` report.
fn selected_names(stdout: &str) -> Vec<String> {
    let report: Value = serde_json::from_str(stdout).expect("explain JSON report");
    report["selected"]
        .as_array()
        .expect("selected array")
        .iter()
        .map(|sensor| sensor["name"].as_str().expect("sensor name").to_owned())
        .collect()
}

/// A config-consuming command against a config-less explicit root exits 2,
/// names the missing file, prints no report, and leaves no state behind.
#[test]
fn explicit_root_without_config_rejects_policy_consumers() {
    let fx = fixture();
    let cases: &[&[&str]] = &[
        &["list", "--format", "json"],
        &["explain", "--format", "json"],
        &["status", "--format", "json"],
        &["verify", "--record", "--format", "json"],
    ];
    let missing = fx.b.join("do-harness.toml");
    for args in cases {
        let (code, stdout, stderr) = run(harness(&fx.a).arg("--root").arg(&fx.b).args(*args));
        assert_eq!(
            code,
            Some(2),
            "`{args:?}` against a config-less explicit root must exit 2\nstdout: {stdout}\nstderr: {stderr}"
        );
        assert!(
            stderr.contains(&missing.display().to_string()),
            "stderr must name the missing explicit config {}\nstderr: {stderr}",
            missing.display()
        );
        assert!(
            stdout.trim().is_empty(),
            "`{args:?}` must print no report on rejection\nstdout: {stdout}"
        );
    }
    assert!(
        !fx.b.join(".do-harness").exists(),
        "a rejected explicit root must gain no state or evidence"
    );
}

/// An explicit root never falls back to an ancestor's config, including the
/// `--root .` spelling from inside the config-less directory.
#[test]
fn explicit_root_never_falls_back_to_ancestor_policy() {
    let fx = fixture();
    let (code, stdout, stderr) = run(harness(&fx.a)
        .arg("--root")
        .arg(&fx.b)
        .arg("explain")
        .arg("--format")
        .arg("json"));
    assert_eq!(
        code,
        Some(2),
        "config-less child of a configured ancestor must be rejected\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stderr.contains(&fx.b.join("do-harness.toml").display().to_string()),
        "stderr must name B's config path\nstderr: {stderr}"
    );
    assert!(
        !stdout.contains("cwd-policy"),
        "ancestor policy must not leak into an explicit root\nstdout: {stdout}"
    );

    let (code, stdout, stderr) = run(harness(&fx.b)
        .arg("--root")
        .arg(".")
        .arg("explain")
        .arg("--format")
        .arg("json"));
    assert_eq!(
        code,
        Some(2),
        "`--root .` inside a config-less dir must be rejected\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stderr.contains("do-harness.toml"), "stderr: {stderr}");
}

/// A configured explicit root selects its own policy, not the invocation
/// cwd's, for absolute and relative root arguments alike.
#[test]
fn explicit_root_selects_target_config_over_invocation_cwd() {
    let fx = fixture();
    std::fs::write(
        fx.b.join("do-harness.toml"),
        sensor_config("target-policy", r#"["true"]"#),
    )
    .unwrap();

    let (code, stdout, stderr) = run(harness(&fx.a)
        .arg("--root")
        .arg(&fx.b)
        .arg("explain")
        .arg("--format")
        .arg("json"));
    assert_eq!(
        code,
        Some(0),
        "configured explicit root must explain\nstderr: {stderr}"
    );
    assert_eq!(selected_names(&stdout), ["target-policy"]);

    let (code, stdout, stderr) = run(harness(&fx.a)
        .arg("--root")
        .arg("nested")
        .arg("explain")
        .arg("--format")
        .arg("json"));
    assert_eq!(
        code,
        Some(0),
        "relative explicit root must resolve against the invocation cwd\nstderr: {stderr}"
    );
    assert_eq!(selected_names(&stdout), ["target-policy"]);
}

/// An explicit external config selects exactly that policy, runs its sensors
/// in the target root, and writes evidence there even when the root has a
/// valid config of its own.
#[test]
fn explicit_config_selects_external_policy_and_target_dir() {
    let fx = fixture();
    let external = fx.a.join("external.toml");
    std::fs::write(
        &external,
        sensor_config(
            "external-policy",
            r#"["bash", "-c", "printf external > policy-ran"]"#,
        ),
    )
    .unwrap();

    let (code, stdout, stderr) = run(harness(&fx.a)
        .arg("--root")
        .arg(&fx.b)
        .arg("--config")
        .arg(&external)
        .arg("explain")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "external config must load\nstderr: {stderr}");
    assert_eq!(selected_names(&stdout), ["external-policy"]);

    let (code, stdout, stderr) = run(harness(&fx.a)
        .arg("--root")
        .arg(&fx.b)
        .arg("--config")
        .arg(&external)
        .arg("verify")
        .arg("--format")
        .arg("json")
        .arg("--evidence")
        .arg(".do-harness/evidence.json"));
    assert_eq!(
        code,
        Some(0),
        "external-policy run must pass\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("external-policy"),
        "report must name the selected policy\nstdout: {stdout}"
    );
    assert_eq!(
        std::fs::read_to_string(fx.b.join("policy-ran")).expect("marker in target root"),
        "external"
    );
    assert!(
        !fx.a.join("policy-ran").exists(),
        "sensor must not execute in the invocation cwd"
    );
    assert!(
        fx.b.join(".do-harness/evidence.json").is_file(),
        "evidence must land under the target root"
    );
    assert!(
        !fx.a.join(".do-harness").exists(),
        "invocation cwd must gain no state"
    );

    std::fs::write(
        fx.b.join("do-harness.toml"),
        sensor_config("target-policy", r#"["true"]"#),
    )
    .unwrap();
    let (code, stdout, stderr) = run(harness(&fx.a)
        .arg("--root")
        .arg(&fx.b)
        .arg("--config")
        .arg(&external)
        .arg("explain")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "stderr: {stderr}");
    assert_eq!(
        selected_names(&stdout),
        ["external-policy"],
        "explicit --config must win over the root's own config"
    );
}

/// A missing, malformed, or directory-valued explicit config exits 2 and never
/// silently selects the root's valid config instead.
#[test]
fn broken_explicit_config_fails_even_with_valid_root_config() {
    let fx = fixture();
    std::fs::write(
        fx.b.join("do-harness.toml"),
        sensor_config("target-policy", r#"["true"]"#),
    )
    .unwrap();
    let missing = fx.a.join("missing.toml");
    let malformed = fx.a.join("malformed.toml");
    std::fs::write(&malformed, "not = = toml\n").unwrap();

    let cases: &[(&Path, &[&str])] = &[
        (&missing, &["explain", "--format", "json"]),
        (&missing, &["verify", "--format", "json"]),
        (&malformed, &["explain", "--format", "json"]),
        (&fx.a, &["explain", "--format", "json"]),
    ];
    for (config, args) in cases {
        let (code, stdout, stderr) = run(harness(&fx.a)
            .arg("--root")
            .arg(&fx.b)
            .arg("--config")
            .arg(config)
            .args(*args));
        assert_eq!(
            code,
            Some(2),
            "`{args:?}` with config {} must exit 2\nstdout: {stdout}\nstderr: {stderr}",
            config.display()
        );
        assert!(
            stderr.contains(&config.display().to_string()),
            "stderr must name the broken explicit config\nstderr: {stderr}"
        );
        assert!(
            stdout.trim().is_empty(),
            "an explicit-policy failure must not fall back to the root config\nstdout: {stdout}"
        );
    }
    assert!(
        !fx.b.join(".do-harness").exists(),
        "failed explicit policy must write no state under the target root"
    );
}

/// Without `--root`, implicit upward discovery from a child directory still
/// selects the discovered workspace's policy.
#[test]
fn implicit_discovery_without_root_override_still_selects_cwd_policy() {
    let fx = fixture();
    let (code, stdout, stderr) = run(harness(&fx.b).arg("explain").arg("--format").arg("json"));
    assert_eq!(
        code,
        Some(0),
        "implicit discovery from a child dir must still work\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert_eq!(selected_names(&stdout), ["cwd-policy"]);
}

//! Release-scaffold tests: the runbook, the preflight script `init` writes,
//! and the `release-preflight` sensor generated with the Rust config.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use super::super::*;
use super::opts;

/// Writes the shipped preflight script into `<root>/scripts`.
fn write_preflight(root: &Path) -> PathBuf {
    fs::create_dir_all(root.join("scripts")).unwrap();
    let path = root.join("scripts/check-release-preflight.sh");
    fs::write(&path, CHECK_RELEASE_PREFLIGHT).unwrap();
    path
}

/// A fixture pinning `VERSION` and a workspace `Cargo.toml` at `version`.
fn versioned_fixture(root: &Path, version: &str) -> PathBuf {
    let script = write_preflight(root);
    fs::write(root.join("VERSION"), format!("{version}\n")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        format!("[workspace]\n\n[workspace.package]\nversion = \"{version}\"\n"),
    )
    .unwrap();
    script
}

/// Runs the shipped preflight from a fixture root.
///
/// `gh` is always injected — `Some(stub)` for the stubbed case, `None` for a
/// path that does not exist — so no test can reach the network or a real `gh`.
fn preflight(root: &Path, args: &[&str], gh: Option<&Path>, envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new("bash");
    cmd.arg(root.join("scripts/check-release-preflight.sh"))
        .args(args)
        .current_dir(root)
        .env("GH_REPO", "example/repo")
        .env_remove("CI")
        .env_remove("DO_HARNESS_REQUIRE_TOOLS")
        .env(
            "RELEASE_PREFLIGHT_GH",
            gh.map_or_else(|| root.join("no-such-gh"), Path::to_path_buf),
        );
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd.output().unwrap()
}

/// Writes an executable stub `gh` whose program body is `body`.
fn stub_gh(root: &Path, body: &str) -> PathBuf {
    let path = root.join("stub-gh");
    fs::write(&path, format!("#!/usr/bin/env bash\n{body}\n")).unwrap();
    crate::fs_perm::set_owner_exec(&path).unwrap();
    path
}

/// Both streams, so assertions do not depend on which one carries a message.
fn output_text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[cfg(unix)]
#[test]
fn preflight_passes_when_every_pin_agrees() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    versioned_fixture(root, "0.2.1");
    fs::create_dir_all(root.join("web")).unwrap();
    fs::write(
        root.join("web/package.json"),
        "{\"name\": \"web\", \"version\": \"0.2.1\"}\n",
    )
    .unwrap();

    let out = preflight(root, &[], None, &[]);

    assert!(out.status.success(), "{}", output_text(&out));
    let shown = output_text(&out);
    assert!(shown.contains("3 version pin(s) agree at 0.2.1"), "{shown}");
    // The release comparison is opt-in, so the offline run says so instead of
    // looking like the whole preflight ran.
    assert!(shown.contains("release comparison skipped"), "{shown}");
}

#[cfg(unix)]
#[test]
fn preflight_fails_on_pin_drift() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    versioned_fixture(root, "0.2.1");
    fs::create_dir_all(root.join("web")).unwrap();
    fs::write(
        root.join("web/package.json"),
        "{\"name\": \"web\", \"version\": \"0.2.0\"}\n",
    )
    .unwrap();

    let out = preflight(root, &[], None, &[]);

    assert_eq!(out.status.code(), Some(1), "{}", output_text(&out));
    let shown = output_text(&out);
    assert!(
        shown.contains("web/package.json declares 0.2.0, but the target version is 0.2.1"),
        "{shown}"
    );
    assert!(
        shown.contains("FIX: bump every pin in one release-prep PR"),
        "{shown}"
    );
}

#[cfg(unix)]
#[test]
fn preflight_fails_when_the_target_is_already_released() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    versioned_fixture(root, "0.2.0");
    let gh = stub_gh(
        root,
        r#"printf '[{"tagName":"v0.2.0"},{"tagName":"v0.1.0"}]'"#,
    );

    let out = preflight(root, &["--release"], Some(&gh), &[]);

    assert_eq!(out.status.code(), Some(1), "{}", output_text(&out));
    let shown = output_text(&out);
    assert!(
        shown.contains("already has a GitHub Release for 0.2.0 (v0.2.0)"),
        "{shown}"
    );
    assert!(shown.contains("FIX: bump every version pin"), "{shown}");
}

#[cfg(unix)]
#[test]
fn preflight_passes_for_an_unpublished_target() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    versioned_fixture(root, "0.2.1");
    let gh = stub_gh(root, r#"printf '[{"tagName":"v0.2.0"}]'"#);

    let out = preflight(root, &["--release"], Some(&gh), &[]);

    assert!(out.status.success(), "{}", output_text(&out));
    let shown = output_text(&out);
    assert!(
        shown.contains("0.2.1 is not published yet (latest example/repo release: v0.2.0)"),
        "{shown}"
    );
}

#[cfg(unix)]
#[test]
fn preflight_degrades_without_gh() {
    let dir = tempfile::tempdir().unwrap();
    versioned_fixture(dir.path(), "0.2.1");

    let out = preflight(dir.path(), &["--release"], None, &[]);

    assert!(out.status.success(), "{}", output_text(&out));
    let shown = output_text(&out);
    assert!(
        shown.contains("WARN: release comparison skipped:"),
        "{shown}"
    );
    assert!(shown.contains("not installed"), "{shown}");
}

#[cfg(unix)]
#[test]
fn preflight_degrades_when_gh_cannot_reach_github() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    versioned_fixture(root, "0.2.1");
    let gh = stub_gh(
        root,
        r"echo 'error connecting to api.github.com' >&2
exit 1",
    );

    let out = preflight(root, &["--release"], Some(&gh), &[]);

    assert!(out.status.success(), "{}", output_text(&out));
    let shown = output_text(&out);
    assert!(
        shown.contains("WARN: release comparison skipped:"),
        "{shown}"
    );
    assert!(
        shown.contains("could not list releases for example/repo"),
        "{shown}"
    );
}

#[cfg(unix)]
#[test]
fn preflight_fails_closed_in_ci_when_the_comparison_cannot_run() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    versioned_fixture(root, "0.2.1");
    let gh = stub_gh(root, "exit 1");

    let out = preflight(root, &["--release"], Some(&gh), &[("CI", "true")]);

    assert_eq!(out.status.code(), Some(1), "{}", output_text(&out));
    let shown = output_text(&out);
    assert!(
        shown.contains("FAIL: release preflight could not compare against published releases"),
        "{shown}"
    );
}

#[cfg(unix)]
#[test]
fn preflight_ignores_private_package_json() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    versioned_fixture(root, "0.2.1");
    fs::create_dir_all(root.join("web")).unwrap();
    // npm workspaces park private packages at a placeholder version; they are
    // not release pins.
    fs::write(
        root.join("web/package.json"),
        "{\"name\": \"web\", \"private\": true, \"version\": \"0.0.0\"}\n",
    )
    .unwrap();

    let out = preflight(root, &[], None, &[]);

    assert!(out.status.success(), "{}", output_text(&out));
    let shown = output_text(&out);
    assert!(shown.contains("2 version pin(s) agree at 0.2.1"), "{shown}");
}

#[cfg(unix)]
#[test]
fn preflight_can_ignore_manifest_pins() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    versioned_fixture(root, "0.2.1");
    fs::create_dir_all(root.join("web")).unwrap();
    fs::write(
        root.join("web/package.json"),
        "{\"name\": \"web\", \"version\": \"0.1.0\"}\n",
    )
    .unwrap();

    // Drift is reported by default...
    let drifted = preflight(root, &[], None, &[]);
    assert_eq!(drifted.status.code(), Some(1), "{}", output_text(&drifted));

    // ...and drops out for manifests that are stamped at publish time.
    let out = preflight(root, &["--no-package-json"], None, &[]);
    assert!(out.status.success(), "{}", output_text(&out));
    assert!(output_text(&out).contains("2 version pin(s) agree at 0.2.1"));
}

#[cfg(unix)]
#[test]
fn preflight_skips_when_no_pin_exists() {
    let dir = tempfile::tempdir().unwrap();
    write_preflight(dir.path());

    let out = preflight(dir.path(), &[], None, &[]);

    assert!(out.status.success(), "{}", output_text(&out));
    let shown = output_text(&out);
    assert!(shown.contains("no version pin found"), "{shown}");
}

#[tokio::test(flavor = "current_thread")]
async fn init_rust_writes_release_runbook_and_preflight() {
    let dir = tempfile::tempdir().unwrap();
    let options = opts(Some(Language::Rust));

    let report = init_workspace(dir.path(), &options).await.unwrap();

    assert!(report.written.contains(&"plans/RELEASING.md".to_owned()));
    assert!(
        report
            .written
            .contains(&"scripts/check-release-preflight.sh".to_owned())
    );
    let runbook = fs::read_to_string(dir.path().join("plans/RELEASING.md")).unwrap();
    assert!(runbook.contains("check-release-preflight.sh --release"));
    let script = fs::read_to_string(dir.path().join("scripts/check-release-preflight.sh")).unwrap();
    assert_eq!(script, CHECK_RELEASE_PREFLIGHT);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(dir.path().join("scripts/check-release-preflight.sh"))
            .unwrap()
            .permissions()
            .mode();
        assert!(
            mode & 0o100 != 0,
            "preflight must be executable (mode {mode:o})"
        );
    }

    // A second run skips both rather than clobbering local edits.
    let again = init_workspace(dir.path(), &options).await.unwrap();
    assert!(again.skipped.contains(&"plans/RELEASING.md".to_owned()));
    assert!(
        again
            .skipped
            .contains(&"scripts/check-release-preflight.sh".to_owned())
    );
}

#[tokio::test(flavor = "current_thread")]
async fn init_generic_has_no_release_scaffold() {
    let dir = tempfile::tempdir().unwrap();

    let report = init_workspace(dir.path(), &opts(Some(Language::Generic)))
        .await
        .unwrap();

    assert!(
        !report
            .written
            .iter()
            .any(|path| path == "plans/RELEASING.md")
    );
    assert!(
        !report
            .written
            .iter()
            .any(|path| path == "scripts/check-release-preflight.sh")
    );
    let config = fs::read_to_string(dir.path().join("do-harness.toml")).unwrap();
    assert!(!config.contains("release-preflight"), "{config}");
}

#[test]
fn generated_rust_config_scopes_release_preflight() {
    // `loc` stands in for the pack's shell-probed sensors: the preflight is
    // only generated where a shell was proven, because `init` writes its
    // script.
    let shell_proven: Vec<_> = crate::config::rust_pack()
        .into_iter()
        .filter(|spec| spec.name == "loc")
        .collect();
    let text = generate_rust_config(&shell_proven).unwrap();

    let cfg: crate::config::Config = toml::from_str(&text).unwrap();
    assert!(
        cfg.signal_sets["release"].contains(&"release-preflight".to_owned()),
        "{:?}",
        cfg.signal_sets
    );
    assert!(
        cfg.signal_sets["verification"].contains(&"release-preflight".to_owned()),
        "{:?}",
        cfg.signal_sets
    );
    assert!(
        !cfg.signal_sets["feedback"].contains(&"release-preflight".to_owned()),
        "{:?}",
        cfg.signal_sets
    );
    let sensor = cfg
        .sensors
        .iter()
        .find(|spec| spec.name == "release-preflight")
        .expect("generated release-preflight sensor");
    assert_eq!(
        sensor.argv,
        vec![
            "bash".to_owned(),
            "scripts/check-release-preflight.sh".to_owned()
        ]
    );
    assert_eq!(
        sensor.when_changed,
        RELEASE_PIN_GLOBS
            .iter()
            .map(|glob| (*glob).to_owned())
            .collect::<Vec<_>>()
    );
    assert!(matches!(
        sensor.effective_severity(),
        crate::config::SensorSeverity::Error
    ));

    let without_shell = generate_rust_config(&[]).unwrap();
    assert!(
        !without_shell.contains("release-preflight"),
        "{without_shell}"
    );
}

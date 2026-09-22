//! The audit sensor's recovery path: a failing `cargo audit` must be retried, the
//! retry budget must stay bounded, and the cleanup that precedes the last retry
//! must never abort the sensor.
//!
//! Measured twice in CI (PR #211 and PR #213): on the Windows runner the
//! advisory database under `$CARGO_HOME` is shared by every dogfood sandbox
//! running at that moment, so one sandbox saw
//! `rm: cannot remove '…/advisory-db/…': Permission denied` — and under
//! `set -euo pipefail` the failing `rm -rf` killed the sensor before the retry
//! its recovery branch exists to perform. The dogfood assertion then failed for
//! a reason unrelated to the code under test.
//!
//! `cargo` is faked on `PATH`, so the cases are deterministic and need no
//! network, no advisory database, and no real audit.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// More failures than any retry budget; must fit a shell integer comparison.
const ALWAYS_FAILS: usize = 1_000_000;

/// Every copy of the sensor: the repository's own and the one `init` ships into
/// scaffolded workspaces (the copy that failed in the dogfood sandboxes).
fn sensor_scripts() -> Vec<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    [
        "../../scripts/check-audit.sh",
        "templates/scripts/check-audit.sh",
    ]
    .iter()
    .map(|relative| manifest.join(relative))
    .collect()
}

struct Run {
    success: bool,
    stdout: String,
    stderr: String,
    calls: usize,
}

/// Runs `script` with a fake `cargo` on `PATH` that fails `fail_times` calls,
/// then succeeds.
///
/// `lock_cargo_home` makes the cleanup fail the way the Windows runner does:
/// the advisory-db directory cannot be removed because its parent denies writes.
fn run_sensor(script: &Path, fail_times: usize, lock_cargo_home: bool) -> Run {
    let sandbox = tempfile::tempdir().unwrap();
    let bin = sandbox.path().join("bin");
    let cargo_home = sandbox.path().join("cargo-home");
    let calls_log = sandbox.path().join("calls.log");
    fs::create_dir_all(&bin).unwrap();
    fs::create_dir_all(cargo_home.join("advisory-db/crates")).unwrap();
    fs::write(
        cargo_home.join("advisory-db/crates/example.md"),
        b"advisory",
    )
    .unwrap();
    fs::write(&calls_log, b"").unwrap();

    let fake_cargo = bin.join("cargo");
    fs::write(
        &fake_cargo,
        format!(
            "#!/bin/sh\n\
             # The sensor probes `cargo audit --version` first; answer it without\n\
             # consuming an attempt.\n\
             if [ \"$1\" = \"audit\" ] && [ \"$2\" = \"--version\" ]; then\n  echo 'cargo-audit 0.0.0'\n  exit 0\nfi\n\
             echo \"$@\" >> \"{log}\"\n\
             calls=$(wc -l < \"{log}\" | tr -d ' ')\n\
             if [ \"$calls\" -le {fail_times} ]; then\n  echo 'error: fake audit failure' >&2\n  exit 1\nfi\nexit 0\n",
            log = calls_log.display(),
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&fake_cargo, fs::Permissions::from_mode(0o755)).unwrap();
    }

    if lock_cargo_home {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // A read-only parent makes `rm -rf …/advisory-db` fail, which is the
            // collision the Windows runner produced with a sibling sandbox.
            fs::set_permissions(&cargo_home, fs::Permissions::from_mode(0o500)).unwrap();
        }
    }

    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let output = Command::new("bash")
        .arg(script)
        .env("PATH", path)
        .env("CARGO_HOME", &cargo_home)
        .env("AUDIT_RETRY_DELAY_SECONDS", "0")
        .env_remove("CI")
        .env_remove("DO_HARNESS_REQUIRE_TOOLS")
        .output()
        .expect("spawn the audit sensor");

    if lock_cargo_home {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&cargo_home, fs::Permissions::from_mode(0o700));
        }
    }

    Run {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        calls: fs::read_to_string(&calls_log).unwrap().lines().count(),
    }
}

#[test]
fn a_failing_audit_is_retried_and_then_reported_clean() {
    for script in sensor_scripts() {
        let run = run_sensor(&script, 1, false);
        assert!(
            run.success,
            "{}: one failed attempt must not fail the sensor (stderr: {})",
            script.display(),
            run.stderr
        );
        assert_eq!(
            run.calls,
            2,
            "{}: the sensor must retry a failed audit exactly once",
            script.display()
        );
    }
}

#[test]
fn a_persistently_failing_audit_stays_bounded() {
    for script in sensor_scripts() {
        let run = run_sensor(&script, ALWAYS_FAILS, false);
        assert!(
            !run.success,
            "{}: a persistent failure must surface as a sensor failure",
            script.display()
        );
        assert_eq!(
            run.calls,
            3,
            "{}: attempts must be bounded (first, retry, post-cleanup retry)",
            script.display()
        );
    }
}

#[cfg(unix)]
#[test]
fn a_failing_cleanup_does_not_abort_the_final_retry() {
    for script in sensor_scripts() {
        let run = run_sensor(&script, ALWAYS_FAILS, true);
        assert!(
            !run.success,
            "{}: a persistent failure must still fail the sensor",
            script.display()
        );
        assert_eq!(
            run.calls,
            3,
            "{}: an unremovable advisory-db must not skip the final retry",
            script.display()
        );
        assert!(
            run.stdout.contains("clearing advisory-db"),
            "{}: the cleanup must be attempted and reported (stdout: {})",
            script.display(),
            run.stdout
        );
    }
}

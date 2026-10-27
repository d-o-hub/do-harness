//! Adversarial execution and managed-path acceptance cases.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::path::Path;
use std::process::Command;

const MAX_DIAGNOSTIC: usize = 8192;
const STABILITY_WAIT: std::time::Duration = std::time::Duration::from_millis(100);

fn verify(root: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_do-harness"))
        .arg("--root")
        .arg(root)
        .args(["verify", "--format", "json", "--strict"])
        .output()
        .unwrap()
}

fn policy(root: &Path, script: &str, settings: &str) {
    std::fs::write(root.join("sensor.py"), script).unwrap();
    std::fs::write(root.join("do-harness.toml"), format!(
        "language='generic'\n[[sensors]]\nname='probe'\nargv=['python3','sensor.py']\n{settings}\n"
    )).unwrap();
}

#[test]
fn oversized_output_is_a_structured_bounded_failure() {
    let dir = tempfile::tempdir().unwrap();
    policy(dir.path(), "print('x' * 1000000)", "max-output-bytes=1024");
    let out = verify(dir.path());
    assert!(!out.status.success());
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["sensors"][0]["details"]["reason"], "output_limit");
    assert_eq!(report["sensors"][0]["details"]["output_truncated"], true);
    assert!(out.stderr.len() < MAX_DIAGNOSTIC);
}

#[test]
fn secret_values_never_enter_failure_tails_or_evidence() {
    let dir = tempfile::tempdir().unwrap();
    policy(
        dir.path(),
        "import os,sys\nvalue=os.environ['SENSOR_TEST_SECRET']\n"
            .to_owned()
            .as_str(),
        "timeout=5",
    );
    std::fs::write(
        dir.path().join("sensor.py"),
        "import os,sys\nv=os.environ['SENSOR_TEST_SECRET']\nprint(v,flush=True)\n".to_owned()
            + "sys.stderr.write(v)\nraise SystemExit(1)\n",
    )
    .unwrap();
    let secret = "synthetic-secret-never-store";
    let out = Command::new(env!("CARGO_BIN_EXE_do-harness"))
        .arg("--root")
        .arg(dir.path())
        .args(["verify", "--strict", "--record"])
        .env("SENSOR_TEST_SECRET", secret)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(!String::from_utf8_lossy(&out.stderr).contains(secret));
    let evidence = std::fs::read_to_string(dir.path().join(".do-harness/evidence.json")).unwrap();
    assert!(!evidence.contains(secret));
}

#[cfg(any(unix, windows))]
#[test]
fn timeout_stops_descendants_that_ignore_sigterm() {
    use std::thread::sleep;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("worker.py"),
        "import signal,time,os\nfrom pathlib import Path\n".to_owned()
            + "Path('pid').write_text(str(os.getpid()))\n"
            + "signal.signal(signal.SIGTERM,signal.SIG_IGN)\n"
            + "while True:\n Path('heartbeat').write_text(str(time.time_ns()))\n time.sleep(.01)\n",
    )
    .unwrap();
    policy(
        dir.path(),
        "import subprocess,sys,time\nsubprocess.Popen([sys.executable,'worker.py'])\ntime.sleep(20)",
        "timeout=1",
    );
    let out = verify(dir.path());
    let before = std::fs::read(dir.path().join("heartbeat")).unwrap();
    sleep(STABILITY_WAIT);
    let after = std::fs::read(dir.path().join("heartbeat")).unwrap();
    let pid = std::fs::read_to_string(dir.path().join("pid")).unwrap();
    // Clean up even when testing the old runner, which only kills the parent.
    if before != after {
        #[cfg(unix)]
        let _ = Command::new("kill").args(["-KILL", &pid]).status();
        #[cfg(windows)]
        let _ = Command::new("taskkill")
            .args(["/PID", &pid, "/T", "/F"])
            .status();
    }
    assert_eq!(
        before, after,
        "a descendant kept running after timeout returned"
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["sensors"][0]["details"]["reason"], "timeout");
}

#[cfg(unix)]
#[test]
fn default_evidence_directory_cannot_be_a_symlink_escape() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    policy(dir.path(), "print('ok')", "timeout=5");
    symlink(outside.path(), dir.path().join(".do-harness")).unwrap();
    let out = verify(dir.path());
    assert!(!out.status.success());
    assert!(!outside.path().join("evidence.json").exists());
}

#[test]
fn environment_policy_is_explicit_and_backward_compatible() {
    for allowlist in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let script = if allowlist {
            "import os\nassert 'SENSOR_MARKER' not in os.environ\nassert os.environ.get('PATH')\n"
        } else {
            "import os\nassert os.environ['SENSOR_MARKER']=='present'\n"
        };
        policy(
            dir.path(),
            script,
            if allowlist {
                "environment-allowlist=['PATH','PYTHONHOME','SystemRoot']"
            } else {
                ""
            },
        );
        let out = Command::new(env!("CARGO_BIN_EXE_do-harness"))
            .arg("--root")
            .arg(dir.path())
            .arg("verify")
            .env("SENSOR_MARKER", "present")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn unlimited_timeout_requires_a_reason_and_bad_budgets_are_usage_errors() {
    for settings in [
        "timeout=0",
        "max-output-bytes=0",
        "environment-allowlist=['BAD=KEY']",
        "artifacts=['../escape/*.json']",
    ] {
        let dir = tempfile::tempdir().unwrap();
        policy(dir.path(), "print('ok')", settings);
        assert_eq!(verify(dir.path()).status.code(), Some(2));
    }
    let dir = tempfile::tempdir().unwrap();
    policy(
        dir.path(),
        "print('ok')",
        "timeout=0\ntimeout-justification='explicit long-running local task'",
    );
    assert!(verify(dir.path()).status.success());
}

#[cfg(unix)]
#[test]
fn init_refuses_a_symlinked_script_directory_before_writing_outside_root() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("scripts")).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_do-harness"))
        .arg("--root")
        .arg(root.path())
        .args(["init", "--language", "generic", "--force"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
}

#[cfg(any(unix, windows))]
#[test]
fn fail_fast_cancels_the_whole_descendant_tree() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("worker.py"),
        "import signal,time,os\nfrom pathlib import Path\n".to_owned()
            + "Path('pid').write_text(str(os.getpid()))\n"
            + "signal.signal(signal.SIGTERM,signal.SIG_IGN)\n"
            + "while True:\n Path('heartbeat').write_text(str(time.time_ns()))\n time.sleep(.01)\n",
    )
    .unwrap();
    std::fs::write(root.path().join("bad.py"),
        "import time\nfrom pathlib import Path\nwhile not Path('heartbeat').exists(): time.sleep(.01)\nraise SystemExit(1)\n").unwrap();
    std::fs::write(root.path().join("long.py"),
        "import subprocess,sys,time\nsubprocess.Popen([sys.executable,'worker.py'])\ntime.sleep(20)\n").unwrap();
    std::fs::write(root.path().join("do-harness.toml"),
        "language='generic'\njobs=2\n[[sensors]]\nname='bad'\nargv=['python3','bad.py']\ntimeout=10\n\
         [[sensors]]\nname='long'\nargv=['python3','long.py']\ntimeout=10\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_do-harness"))
        .arg("--root")
        .arg(root.path())
        .args(["verify", "--fail-fast", "--format", "json"])
        .output()
        .unwrap();
    let before = std::fs::read(root.path().join("heartbeat")).unwrap();
    std::thread::sleep(STABILITY_WAIT);
    let after = std::fs::read(root.path().join("heartbeat")).unwrap();
    let pid = std::fs::read_to_string(root.path().join("pid")).unwrap();
    if before != after {
        #[cfg(unix)]
        let _ = Command::new("kill").args(["-KILL", &pid]).status();
        #[cfg(windows)]
        let _ = Command::new("taskkill")
            .args(["/PID", &pid, "/T", "/F"])
            .status();
    }
    assert_eq!(before, after);
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let long = report["sensors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "long")
        .unwrap();
    assert_eq!(long["details"]["reason"], "cancelled");
}

#[test]
fn known_secrets_in_argv_are_redacted_from_persisted_metadata() {
    let root = tempfile::tempdir().unwrap();
    let secret = "synthetic-sensitive-argument";
    let argv = serde_json::json!([
        "python3",
        "-c",
        format!("print('{secret}');raise SystemExit(1)")
    ]);
    std::fs::write(
        root.path().join("do-harness.toml"),
        format!("language='generic'\n[[sensors]]\nname='probe'\nargv={argv}\n"),
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_do-harness"))
        .arg("--root")
        .arg(root.path())
        .args(["verify", "--strict"])
        .env("SENSOR_TEST_SECRET", secret)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(!String::from_utf8_lossy(&out.stderr).contains(secret));
    let evidence = std::fs::read_to_string(root.path().join(".do-harness/evidence.json")).unwrap();
    assert!(!evidence.contains(secret));
    assert!(evidence.contains("[REDACTED]"));
}

#[cfg(unix)]
#[test]
fn status_rejects_a_fifo_instead_of_blocking_on_evidence() {
    use std::time::{Duration, Instant};
    const DEADLINE: Duration = Duration::from_secs(2);
    const POLL: Duration = Duration::from_millis(10);
    let root = tempfile::tempdir().unwrap();
    policy(root.path(), "print('ok')", "");
    let path = root.path().join("do-harness.toml");
    let text = std::fs::read_to_string(&path).unwrap().replace(
        "[[sensors]]",
        "[signal-sets]\nverification=['probe']\n[[sensors]]",
    );
    std::fs::write(path, text).unwrap();
    std::fs::create_dir(root.path().join(".do-harness")).unwrap();
    assert!(
        Command::new("mkfifo")
            .arg(root.path().join(".do-harness/evidence.verification.json"))
            .status()
            .unwrap()
            .success()
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_do-harness"))
        .arg("--root")
        .arg(root.path())
        .args(["status", "--set", "verification", "--format", "json"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let start = Instant::now();
    while child.try_wait().unwrap().is_none() {
        if start.elapsed() > DEADLINE {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("status blocked on a non-regular evidence file");
        }
        std::thread::sleep(POLL);
    }
    let out = child.wait_with_output().unwrap();
    assert!(!out.status.success());
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["reason"], "unreadable");
}

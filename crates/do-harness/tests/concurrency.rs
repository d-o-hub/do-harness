//! Concurrency: overlapping `verify --record` writers and task commands share
//! one libSQL file without `SQLITE_BUSY`, duplicate event seq, or a broken
//! hash chain.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::{Child, Command, Output};

fn harness() -> Command {
    Command::new(env!("CARGO_BIN_EXE_do-harness"))
}

fn run(dir: &Path, args: &[&str]) -> Output {
    let out = harness()
        .arg("--root")
        .arg(dir)
        .args(args)
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "do-harness {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

fn spawn(dir: &Path, args: &[&str]) -> Child {
    harness()
        .arg("--root")
        .arg(dir)
        .args(args)
        .spawn()
        .expect("spawn")
}

#[test]
fn concurrent_verify_record_and_advance_keep_chain_intact() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    run(root, &["init"]);
    run(
        root,
        &[
            "task",
            "add",
            "concurrent slice",
            "--method",
            "vertical-event-slice",
        ],
    );

    // Two concurrent verify --record writers plus a task advance.
    let mut children = vec![
        spawn(root, &["verify", "--record", "--only", "loc"]),
        spawn(root, &["verify", "--record", "--only", "loc"]),
        spawn(root, &["task", "advance", "1"]),
    ];
    for child in &mut children {
        let status = child.wait().expect("wait");
        assert!(status.success(), "concurrent writer failed: {status}");
    }

    // A second wave exercises the retry path on an already-chained log.
    let mut second = vec![
        spawn(root, &["verify", "--record", "--only", "loc"]),
        spawn(root, &["verify", "--record", "--only", "loc"]),
        spawn(root, &["verify", "--record", "--only", "loc"]),
    ];
    for child in &mut second {
        let status = child.wait().expect("wait");
        assert!(status.success(), "second-wave writer failed: {status}");
    }

    let out = run(root, &["audit-chain"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("OK"),
        "hash chain must stay intact: {stdout}"
    );
}

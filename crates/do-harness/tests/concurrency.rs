//! Concurrency: overlapping `verify --record` writers and task commands share
//! one libSQL file without `SQLITE_BUSY`, duplicate event seq, or a broken
//! hash chain.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::{Child, Command, Output};

mod support;

fn harness() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    // The spawned harness runs `init`, whose baseline verify builds the
    // scaffold. Under `cargo llvm-cov` an inherited `RUSTC_WRAPPER` and
    // `CARGO_TARGET_DIR` turn that nested build into a second instrumented
    // workspace; the concurrent writers then pile onto it and the run OOMs
    // (measured 2026-10-20: past 13 GB in the coverage gate). Clear the outer
    // coverage session exactly as the dogfood and init tests do.
    support::isolate_command(&mut command);
    command
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
    // Under coverage instrumentation every spawned harness runs an order of
    // magnitude slower, so full writer waves pile up without bound (measured
    // 2026-10-14: past 11GB anon, OOM-killing the coverage run). Shrink the
    // waves when coverage instrumentation is active; the retry path is still exercised.
    let under_coverage = std::env::var_os("LLVM_PROFILE_FILE").is_some()
        || std::env::var_os("CARGO_LLVM_COV").is_some();
    let (first_wave, second_wave) = if under_coverage { (1, 1) } else { (2, 3) };
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

    // Concurrent verify --record writers plus a task advance.
    let mut children: Vec<Child> = (0..first_wave)
        .map(|_| {
            spawn(
                root,
                &["verify", "--record", "--task", "1", "--only", "loc"],
            )
        })
        .collect();
    children.push(spawn(root, &["task", "advance", "1"]));
    for child in &mut children {
        let status = child.wait().expect("wait");
        assert!(status.success(), "concurrent writer failed: {status}");
    }

    // A second wave exercises the retry path on an already-chained log.
    let mut second: Vec<Child> = (0..second_wave)
        .map(|_| {
            spawn(
                root,
                &["verify", "--record", "--task", "1", "--only", "loc"],
            )
        })
        .collect();
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

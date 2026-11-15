#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration, Instant};

use super::{BoundedOutput, run_with_budget};

fn output(stdout: &str) -> BoundedOutput {
    BoundedOutput {
        exit_code: Some(0),
        timed_out: false,
        spawn_error: None,
        output_bytes: 0,
        output_truncated: false,
        stdout: stdout.to_owned(),
        stderr: String::new(),
    }
}

#[test]
fn verify_and_status_share_one_monotonic_budget() {
    let timeout = Duration::from_secs(2);
    let started = Instant::now()
        .checked_sub(Duration::from_secs(1))
        .expect("one second is representable");
    let mut commands = Vec::new();
    let mut budgets = Vec::new();
    let outcome = run_with_budget("verification", timeout, started, |args, budget| {
        commands.push(args[0].to_owned());
        budgets.push(budget);
        if args[0] == "verify" {
            std::thread::sleep(Duration::from_millis(600));
            output(r#"{"failed":[],"unrunnable":[]}"#)
        } else {
            std::thread::sleep(Duration::from_millis(600));
            output(r#"{"state":"green","reason":"current"}"#)
        }
    });

    assert_eq!(commands, ["verify", "status"]);
    assert!(budgets[0] < timeout);
    assert!(budgets[1] < budgets[0]);
    assert!(
        !outcome.green,
        "late green evidence must not complete the gate"
    );
    assert_eq!(outcome.verification.as_ref().unwrap().state, "green");
}

#[test]
fn exhausted_budget_skips_the_next_gate_command() {
    let timeout = Duration::from_millis(100);
    let started = Instant::now()
        .checked_sub(timeout)
        .expect("test timeout is representable");
    let mut calls = 0;
    let outcome = run_with_budget("verification", timeout, started, |_, _| {
        calls += 1;
        output(r#"{"failed":[],"unrunnable":[]}"#)
    });

    assert_eq!(calls, 0);
    assert!(!outcome.green);
}

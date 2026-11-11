//! A closed reader must end the process cleanly, not panic it (issue #512).
//!
//! `do-harness <cmd> | head` is an ordinary invocation: the reader stops asking
//! for output, so the write fails with `EPIPE`. Before this was handled, that
//! failure went through `println!`, which unwraps and aborts with exit 101 and a
//! backtrace — and, worse, the panic hook filed the abort as a *new* CLI defect
//! issue on every occurrence.
//!
//! The regression is proven here by closing the read end of a pipe and then
//! asking for materially more output than the pipe buffer holds, so the writer
//! is still producing when it discovers the reader is gone. Without the fix the
//! process exits 101; with it, it exits `141`.
//!
//! Unix only, because the status this asserts is the shell's convention for
//! `SIGPIPE` (`128 + 13`). The fix cannot deliver that signal itself — the crate
//! forbids `unsafe`, so the default disposition cannot be restored — so `main`
//! catches the panic instead and exits with the status the signal would have
//! produced. A platform that does deliver the signal is also accepted below.
//! Windows has no such signal; it takes the same caught-panic path and exits
//! `0`, which is covered by the unit tests on `is_broken_pipe_message`.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::{Command, Stdio};

/// Command that emits a lot of stdout and needs no repository, network, or
/// database: `completions` renders a full shell script from the Clap
/// definitions, which is tens of thousands of bytes at every supported shell.
const LARGE_OUTPUT_ARGS: [&str; 2] = ["completions", "bash"];

/// The panic exit code the standard library uses for an unwrapped `println!`.
const PANIC_EXIT_CODE: i32 = 101;

/// Shell status for a process killed by `SIGPIPE`, mirrored from the binary's
/// own constant so a change to one is a test failure in the other.
const SIGPIPE_EXIT_CODE: i32 = 141;

/// `SIGPIPE` itself, for the signal-death branch below. Linux and macOS agree on
/// 13; the constant keeps the arithmetic checkable rather than magic.
const SIGPIPE_SIGNAL: i32 = 13;

/// Run the binary with its stdout connected to a pipe whose read end is closed
/// before the first write completes, and return the exit status.
fn run_with_closed_stdout() -> std::process::ExitStatus {
    let mut child = Command::new(env!("CARGO_BIN_EXE_do-harness"))
        .args(LARGE_OUTPUT_ARGS)
        // Keep the run hermetic: no inherited repository, and no chance the
        // failure path reaches the network to file a report.
        .env("DO_HARNESS_AUTO_REPORT", "off")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn do-harness");

    // Take the read end and drop it immediately. The child keeps writing into a
    // pipe with no readers, which is the `| head` shape: `head` exits after a
    // few lines and the writer's next write sees `EPIPE`.
    drop(child.stdout.take());
    child.wait().expect("wait for do-harness")
}

#[test]
fn a_closed_stdout_does_not_panic_the_process() {
    let status = run_with_closed_stdout();

    assert_ne!(
        status.code(),
        Some(PANIC_EXIT_CODE),
        "a closed stdout must not abort the process as an internal panic; \
         status was {status:?}"
    );
}

#[test]
fn a_closed_stdout_ends_the_process_with_the_sigpipe_status() {
    let status = run_with_closed_stdout();

    // `141` is what a shell reports for a writer killed by `SIGPIPE`
    // (`128 + 13`). The fix cannot deliver the signal itself — the crate forbids
    // `unsafe`, so `SIG_DFL` cannot be restored — so the process exits with that
    // status instead. The observable pipeline contract is the same one `head`,
    // `grep`, and `cat` satisfy.
    let code = status.code().unwrap_or_else(|| {
        use std::os::unix::process::ExitStatusExt;
        // A genuine signal death is also correct; assert it is the right one.
        assert_eq!(
            status.signal(),
            Some(SIGPIPE_SIGNAL),
            "status was {status:?}"
        );
        SIGPIPE_EXIT_CODE
    });
    assert_eq!(
        code, SIGPIPE_EXIT_CODE,
        "a closed stdout must end with the documented pipe status, not {code}"
    );
}

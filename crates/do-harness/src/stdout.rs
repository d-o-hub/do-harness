//! Piped-stdout handling.
//!
//! `do-harness <cmd> | head` is an ordinary invocation — a pager, a `head`, or a
//! closed terminal is the reader's right — but the standard library treats a
//! broken pipe as a panic: `println!` unwraps the failed write and aborts with
//! exit 101 and a backtrace. That is not a defect in the command that was run,
//! it is the reader having had enough output, so a broken pipe must end the
//! process quietly and conventionally instead.
//!
//! Unix has said this for decades: `SIGPIPE` terminates a writer whose reader
//! went away, and a shell reports that death as `141` (`128 + 13`). Rust asks
//! for `SIG_IGN` on that signal before `main` runs, which is exactly why the
//! write surfaces as an `EPIPE` error instead of ending the process.
//!
//! This module reproduces the *outcome* without the signal, because the crate
//! forbids `unsafe` and restoring `SIG_DFL` needs a C `signal(2)` call. `main`
//! catches the panic at the process boundary and reports [`SIGPIPE_EXIT_CODE`]
//! instead, which is the status a shell would have reported for the signal. The
//! observable contract is the same — the writer stops and the pipeline reports a
//! pipe status — while the mechanism differs: the process exits normally rather
//! than dying from the signal.
//!
//! Windows has no `SIGPIPE`, so there is no matching status to mirror. It takes
//! the same caught-panic path and exits `0`: the reader stopped asking for
//! output, which is a finished job rather than a failed one.

/// Exit status a shell reports for a process killed by `SIGPIPE`.
pub const SIGPIPE_EXIT_CODE: u8 = 141;

/// Whether a panic message is a write failing because the reader went away.
///
/// Two shapes reach the hook. `std::io::stdio` reports `failed printing to
/// stdout: Broken pipe (os error 32)`; a writer that returns the error itself
/// reports it too — `clap_complete` panics with `failed to write completion
/// file: Os { code: 32, … }`, which is the same event one layer down. Both name
/// the pipe error and a failed write, and neither is a defect in this tool.
///
/// The match is deliberately positive on both parts: a panic that merely
/// mentions a pipe, or one that fails to write for another reason (a full disk,
/// say), keeps its normal abort behaviour and its report.
#[must_use]
pub fn is_broken_pipe_message(message: &str) -> bool {
    message.contains("Broken pipe")
        && (message.contains("failed printing to") || message.contains("failed to write"))
}

/// The exit status for a caught broken-pipe panic, or `None` when the panic is
/// something else that must keep its normal abort behaviour.
#[must_use]
pub fn broken_pipe_exit(payload: &(dyn std::any::Any + Send)) -> Option<u8> {
    let message = payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())?;
    is_broken_pipe_message(&message).then_some(SIGPIPE_EXIT_CODE)
}

/// Suppresses the default panic report for a broken-pipe panic only.
///
/// The panic is expected and harmless, so the standard backtrace (`thread 'main'
/// panicked at …, failed printing to stdout: Broken pipe`, plus the
/// `RUST_BACKTRACE` hint) is noise on a normal `| head`. Installing this before
/// the original hook keeps every other panic reporting exactly as it did,
/// because the guard delegates to the hook it replaced.
pub fn install_panic_hook_quieting_broken_pipes() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if panic_is_broken_pipe(info) {
            return;
        }
        previous(info);
    }));
}

/// Whether a panic is the expected broken-pipe write, read from its payload.
fn panic_is_broken_pipe(info: &std::panic::PanicHookInfo<'_>) -> bool {
    let payload = info.payload();
    let message = payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_default();
    is_broken_pipe_message(&message)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn recognizes_the_standard_library_broken_pipe_panic() {
        let message = "failed printing to stdout: Broken pipe (os error 32)";
        assert!(is_broken_pipe_message(message));
    }

    /// `clap_complete` returns the write error instead of unwrapping `println!`,
    /// so its panic text differs while the cause is identical.
    #[test]
    fn recognizes_a_writer_surface_that_returns_the_pipe_error() {
        let message = "failed to write completion file: Os { code: 32, kind: \
                       BrokenPipe, message: \"Broken pipe\" }";
        assert!(is_broken_pipe_message(message));
    }

    #[test]
    fn ignores_unrelated_panics() {
        assert!(!is_broken_pipe_message("index out of bounds"));
        // A write that failed for a reason other than a closed reader stays a
        // real failure and keeps its abort and its report.
        assert!(!is_broken_pipe_message(
            "failed printing to stdout: No space left on device (os error 28)"
        ));
        assert!(!is_broken_pipe_message(
            "failed to write completion file: Os { code: 28, kind: StorageFull }"
        ));
        // A pipe mentioned without a failed write is not this event.
        assert!(!is_broken_pipe_message("Broken pipe"));
    }

    #[test]
    fn a_broken_pipe_payload_maps_to_the_pipe_exit_status() {
        let payload: Box<dyn std::any::Any + Send> =
            Box::new("failed printing to stdout: Broken pipe (os error 32)".to_owned());
        assert_eq!(broken_pipe_exit(payload.as_ref()), Some(SIGPIPE_EXIT_CODE));
    }

    #[test]
    fn any_other_payload_keeps_its_own_failure_path() {
        let payload: Box<dyn std::any::Any + Send> = Box::new("boom");
        assert_eq!(broken_pipe_exit(payload.as_ref()), None);

        let not_a_string: Box<dyn std::any::Any + Send> = Box::new(7_u8);
        assert_eq!(broken_pipe_exit(not_a_string.as_ref()), None);
    }
}

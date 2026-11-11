//! Unified CLI for the do-harness agent execution harness.
//!
//! Entrypoints: `verify` (computational sensors), `list` (sensor names),
//! `init-db` (migrations), `seed` (architecture invariants from
//! `plans/invariants.json`), `init` (workspace scaffold), `task` (task
//! state), `trace` (interaction traces), `distill` (heuristic extraction),
//! `eval` (skill-eval runner), `hook` (git hook management), `pr`
//! (deterministic PR analysis), `sync` (reconcile `origin/main`), `dora`
//! (deterministic deployment metrics), and `version` (version information).

#![forbid(unsafe_code)]

use std::process::ExitCode;

use clap::Parser;

use crate::cli::Cli;

mod applicability;
mod approver;
mod argv_paths;
mod artifacts;
mod audit;
mod baselines;
mod binary;
mod changes;
mod ci_explain;
mod cli;
mod commands;
mod config;
mod dbcheck;
mod diag;
mod dispatch;
mod distill;
mod distill_strikes;
#[cfg(test)]
mod docs_generation;
mod doctor;
mod dora;
mod errors;
mod eval;
mod eval_assert;
mod eval_integrity;
mod eval_sandbox;
mod eval_walk;
mod events;
mod evidence;
mod exit;
mod explain;
mod feedback;
mod fingerprint;
mod fs_perm;
mod gh;
mod hook_script;
mod hooks;
mod init;
mod job;
mod learn;
mod loc;
mod man;
mod managed_path;
mod methods;
mod metrics;
mod overlap;
mod pr;
mod provenance;
mod redaction;
mod report;
mod sensor_inputs;
mod sensors;
mod shell;
mod signals;
mod skill_write;
mod skills;
mod split;
mod status;
mod stdout;
mod sync;
mod task;
mod telemetry;
mod trace;
mod util;
mod verify;
mod version;

/// Classified CLI failure carrying its process exit code.
pub enum CliError {
    /// Usage, config, or discovery problems: exit 2.
    Usage(anyhow::Error),
    /// Sensor verification failed: exit 1.
    Verify(anyhow::Error),
    /// A verify run owns the evidence and cannot be judged: exit 3.
    Running(anyhow::Error),
}

impl CliError {
    /// The process exit code for this error class.
    #[must_use]
    pub fn exit_code(&self) -> u8 {
        match self {
            CliError::Usage(_) => 2,
            CliError::Verify(_) => 1,
            CliError::Running(_) => 3,
        }
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliError::Usage(err) | CliError::Verify(err) | CliError::Running(err) => {
                write!(f, "{err:#}")
            }
        }
    }
}

// A plain `main` rather than `#[tokio::main]`: the dispatch future has to be
// polled *inside* `catch_unwind`, so the runtime is built here and the block-on
// call is what the guard wraps. `#[tokio::main]` would enter a runtime first and
// leave nothing to catch.
fn main() -> ExitCode {
    // Diagnostic context only: the arguments are reported, never used for
    // authorization.
    // nosemgrep: rust.lang.security.args.args
    diag::set_argv(std::env::args().collect());
    // Quieting the expected pipe panic happens before the reporting hook is
    // installed, so the two compose: this one recognises a broken pipe and stays
    // silent, and every other panic still reaches the reporting hook below.
    stdout::install_panic_hook_quieting_broken_pipes();
    feedback::report::install_panic_hook();

    // A broken-pipe panic must surface as `Err` here instead of aborting the
    // process: that is what turns `do-harness … | head` from exit 101 plus a
    // backtrace into the quiet pipe status a pipeline expects.
    // `AssertUnwindSafe` is sound because every panic that is *not* a broken
    // pipe is re-raised immediately below, so no invariant is ever read after
    // an unwind has been swallowed.
    let outcome = std::panic::catch_unwind(|| {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| CliError::Usage(anyhow::Error::from(err)))?;
        let cli = Cli::parse();
        let root = cli.root.clone();
        let result = runtime.block_on(dispatch::run(cli));
        Ok::<_, CliError>((result, root))
    });

    match outcome {
        Ok(Ok((Ok(()), _))) => ExitCode::SUCCESS,
        Ok(Ok((Err(err), root))) => report_and_exit(&err, root.as_deref()),
        // A runtime that cannot be built has no workspace root yet, because the
        // CLI has not been parsed; report it against the default target.
        Ok(Err(err)) => report_and_exit(&err, None),
        // The reader closed the pipe. Report the status a shell gives a writer
        // killed by `SIGPIPE` so `| head` looks like every other pipeline.
        Err(payload) => stdout::broken_pipe_exit(payload.as_ref()).map_or_else(
            // A panic that is not a broken pipe keeps its usual abort semantics;
            // re-raising it preserves the message, backtrace, and exit code.
            || std::panic::resume_unwind(payload),
            ExitCode::from,
        ),
    }
}

/// Records, reports, and classifies a `CliError` exactly once.
fn report_and_exit(err: &CliError, root: Option<&std::path::Path>) -> ExitCode {
    let code = err.exit_code();
    let class = match err {
        CliError::Usage(_) => feedback::report::FailureClass::Usage,
        CliError::Verify(_) => feedback::report::FailureClass::Verify,
        CliError::Running(_) => feedback::report::FailureClass::Running,
    };
    let message = format!("{err:#}");
    diag::record(&format!("error: {message}"));
    feedback::report::file_failure(root, class, &message, &diag::argv(), code);
    eprintln!("error: {message}");
    ExitCode::from(code)
}

//! Unified CLI for the do-harness agent execution harness.
//!
//! Entrypoints: `verify` (computational sensors), `list` (sensor names),
//! `init-db` (migrations), `seed` (architecture invariants from
//! `plans/invariants.json`), `init` (workspace scaffold), `task` (task
//! state), `trace` (interaction traces), `distill` (heuristic extraction),
//! `eval` (skill-eval runner), `hook` (git hook management), `pr`
//! (deterministic PR analysis), `dora` (deterministic deployment metrics),
//! and `version` (version information).

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

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    feedback::report::install_panic_hook();
    let cli = Cli::parse();
    let root = cli.root.clone();
    let argv: Vec<String> = std::env::args().collect();
    match dispatch::run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            let code = err.exit_code();
            let class = match &err {
                CliError::Usage(_) => feedback::report::FailureClass::Usage,
                CliError::Verify(_) => feedback::report::FailureClass::Verify,
                CliError::Running(_) => feedback::report::FailureClass::Running,
            };
            let message = format!("{err:#}");
            diag::record(&format!("error: {message}"));
            feedback::report::file_failure(root.as_deref(), class, &message, &argv, code);
            eprintln!("error: {message}");
            ExitCode::from(code)
        }
    }
}

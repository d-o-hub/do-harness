//! Native (non-shell) implementations of the shipped helper sensors.
//!
//! The rust pack's `commitlint`, `deps`, `audit`, and `loc` sensors
//! historically ran POSIX shell scripts (`scripts/check-*.sh`). The scripts
//! remain as thin delegating wrappers so existing adopter configs keep
//! working, but the canonical implementations are these subcommands: they run
//! natively on every platform (Windows included, without Git Bash or WSL)
//! and share the scripts' output contract and exit codes.
//!
//! The group's shape follows `do-harness loc`, the first helper that moved
//! native: a documented subcommand with the same output contract, `FINDINGS`
//! markers where the script had them, and exit codes 0 (pass), 1 (a check
//! failed), and 2 (usage/config errors).

pub mod audit;
pub mod commitlint;
pub mod deps;

use std::fmt;
/// Dispatches the native helper subcommands.
#[allow(clippy::needless_pass_by_value)]
pub fn run(root: &std::path::Path, action: crate::cli::HelpersAction) -> Result<(), HelperError> {
    use crate::cli::HelpersAction;
    match action {
        HelpersAction::Commitlint {
            message,
            count,
            range,
        } => commitlint::run(root, message.as_deref(), count, range.as_deref()),
        HelpersAction::Deps => deps::run(root),
        HelpersAction::Audit { args } => audit::run(root, &args),
    }
}

/// A helper run that must map to a specific process exit code.
#[derive(Debug)]
pub enum HelperError {
    /// A check failed (sensor verdict): exit 1.
    Violation(String),
    /// Usage or environment error: exit 2.
    Usage(String),
}

impl fmt::Display for HelperError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HelperError::Violation(msg) | HelperError::Usage(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for HelperError {}

/// Whether the caller demands real gates (CI or an explicit opt-in),
/// mirroring `scripts/lib.sh`'s `dh_require_tools`.
#[must_use]
pub fn require_tools() -> bool {
    std::env::var("CI").as_deref() == Ok("true")
        || std::env::var("DO_HARNESS_REQUIRE_TOOLS").as_deref() == Ok("1")
}

/// Runs `command` and returns (success, output with stderr merged).
///
/// The output preserves command order exactly as a shell would see it, so the
/// helper messages and the sensor's `SKIP:`/`FINDINGS:` markers stay
/// byte-compatible with the shell versions.
fn run_captured(mut command: std::process::Command) -> (bool, String) {
    use std::process::Stdio;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let Ok(output) = command.output() else {
        return (false, String::new());
    };
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    (output.status.success(), text)
}

/// Runs `cargo <args>` in `root`, returning (success, merged output).
#[must_use]
pub fn run_cargo(root: &std::path::Path, args: &[&str]) -> (bool, String) {
    let mut command = std::process::Command::new("cargo");
    command.current_dir(root).args(args);
    run_captured(command)
}

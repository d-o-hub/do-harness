//! Options bag for one `do-harness eval` invocation.

use crate::report::Format;

/// Options for one `do-harness eval` invocation.
///
/// Deliberately a flat options bag (like `VerifyOpts`); the boolean fields
/// are independent CLI switches, not a state machine.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub struct EvalOpts<'a> {
    /// Restrict evaluation to this skill directory name.
    pub skill: Option<&'a str>,
    /// Re-baseline graders and ratchet floors on a fully green run.
    pub bless: bool,
    /// List available skills and exit.
    pub list_skills: bool,
    /// Halt on the first failing skill.
    pub fail_fast: bool,
    /// Skip execution (report only).
    pub dry_run: bool,
    /// Output format.
    pub format: Format,
    /// Approver identity recorded with `bless`.
    pub approver: Option<&'a str>,
    /// Skip the without-skill baseline run.
    pub no_lift: bool,
    /// External agent command run once per case instead of the walkthrough.
    pub agent_cmd: Option<&'a str>,
    /// Kill an agent run after this many seconds.
    pub agent_timeout_secs: u64,
    /// Fail skills whose fixture has dataset-quality gaps.
    pub strict_fixtures: bool,
    /// Number of evaluation trials (default: 1).
    pub trials: usize,
}

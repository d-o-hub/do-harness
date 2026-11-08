//! Typed specification for one explicitly invoked unattended job.
//!
//! A spec is authorized operator input, never repository content: it names the
//! task, the pinned revision, the external executor, the declared isolation
//! mechanism, and the finite attempt/time limits. It never names a default
//! executor, so ordinary deterministic verification cannot start a paid model.

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::Digest;

/// Job-spec schema version understood by this binary.
pub const SPEC_SCHEMA_VERSION: u32 = 1;
/// Default executor attempts across the whole job, resumes included.
pub const DEFAULT_ATTEMPTS: u32 = 3;
/// Hard ceiling on declared attempts.
pub const MAX_ATTEMPTS: u32 = 20;
/// Default wall-clock budget for the whole job.
pub const DEFAULT_DEADLINE_SECONDS: u64 = 3600;
/// Hard ceiling on the whole-job budget.
pub const MAX_DEADLINE_SECONDS: u64 = 24 * 60 * 60;
/// Default wall-clock budget for one executor invocation.
pub const DEFAULT_EXECUTOR_TIMEOUT_SECONDS: u64 = 1800;
/// Hard ceiling on one executor invocation.
pub const MAX_EXECUTOR_TIMEOUT_SECONDS: u64 = 6 * 60 * 60;
/// Shortest accepted budget; zero is a usage error, never an unlimited run.
pub const MIN_TIMEOUT_SECONDS: u64 = 1;
/// Longest task description a spec may carry.
pub const MAX_TASK_BYTES: usize = 8 * 1024;
/// Hexadecimal characters kept from a spec digest for the job id.
const JOB_ID_HEX_CHARS: usize = 16;

/// Declared executor accounting availability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Accounting {
    /// The executor exposes no enforceable spend or token accounting.
    #[default]
    Unavailable,
    /// The executor reports enforceable spend or token accounting.
    Reported,
}

/// External executor identity and configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutorSpec {
    /// Command line, program first; the program is the executor identity.
    pub argv: Vec<String>,
    /// Environment variable names inherited from this process. Every other
    /// variable, including credentials, is removed from the child.
    #[serde(default, rename = "allowed-env")]
    pub allowed_env: Vec<String>,
    /// Whether spend/token accounting is enforceable.
    #[serde(default)]
    pub accounting: Accounting,
    /// Token budget; only accepted when accounting is enforceable.
    #[serde(default, rename = "token-budget")]
    pub token_budget: Option<u64>,
    /// Whether the executor must declare a result through
    /// `DO_HARNESS_JOB_RESULT`.
    #[serde(default = "default_result_required", rename = "result-required")]
    pub result_required: bool,
}

impl ExecutorSpec {
    /// The executor program (identity) declared by the spec.
    #[must_use]
    pub fn program(&self) -> &str {
        self.argv.first().map_or("", String::as_str)
    }
}

/// Declared external isolation mechanism.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IsolationSpec {
    /// Machine name of the mechanism that isolates the executor's process,
    /// network, and filesystem (for example `container`).
    pub mechanism: String,
}

/// One unattended job specification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct JobSpec {
    /// Schema version; must equal [`SPEC_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Authorized task description handed to the executor.
    pub task: String,
    /// Repository revision the job workspace is created from.
    pub revision: String,
    /// Signal set whose fresh passing evidence is the completion gate.
    pub signal_set: String,
    /// Executor attempts allowed across the whole job, resumes included.
    #[serde(default = "default_attempts")]
    pub attempts: u32,
    /// Whole-job wall-clock budget in seconds, measured from first start.
    #[serde(default = "default_deadline")]
    pub deadline_seconds: u64,
    /// Wall-clock budget for one executor invocation in seconds.
    #[serde(default = "default_executor_timeout")]
    pub executor_timeout_seconds: u64,
    /// External executor identity and configuration.
    pub executor: ExecutorSpec,
    /// Declared external isolation mechanism.
    pub isolation: IsolationSpec,
}

fn default_attempts() -> u32 {
    DEFAULT_ATTEMPTS
}

fn default_deadline() -> u64 {
    DEFAULT_DEADLINE_SECONDS
}

fn default_executor_timeout() -> u64 {
    DEFAULT_EXECUTOR_TIMEOUT_SECONDS
}

fn default_result_required() -> bool {
    true
}

impl JobSpec {
    /// Reads and validates a spec file, returning it with its raw bytes.
    ///
    /// # Errors
    ///
    /// Returns an error when the file cannot be read, is not valid TOML, or
    /// fails validation.
    pub fn load(path: &Path) -> Result<(Self, String)> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read job spec {}", path.display()))?;
        let spec: Self = toml::from_str(&text)
            .with_context(|| format!("invalid job spec {}", path.display()))?;
        spec.validate()?;
        Ok((spec, text))
    }

    /// Rejects a spec that cannot produce a bounded, honest job.
    ///
    /// # Errors
    ///
    /// Returns the first violated rule as a usage-correctable message.
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != SPEC_SCHEMA_VERSION {
            anyhow::bail!(
                "job spec schema-version {} is not supported (expected {})",
                self.schema_version,
                SPEC_SCHEMA_VERSION
            );
        }
        if self.task.trim().is_empty() {
            anyhow::bail!("job spec must declare a task");
        }
        if self.task.len() > MAX_TASK_BYTES {
            anyhow::bail!("job spec task exceeds {MAX_TASK_BYTES} bytes");
        }
        if self.revision.trim().is_empty() {
            anyhow::bail!("job spec must pin a repository revision");
        }
        if self.revision.starts_with('-') || self.revision.chars().any(char::is_whitespace) {
            anyhow::bail!(
                "job spec revision '{}' is not a plain revision",
                self.revision
            );
        }
        if self.signal_set.trim().is_empty() {
            anyhow::bail!("job spec must name a signal-set");
        }
        if !(1..=MAX_ATTEMPTS).contains(&self.attempts) {
            anyhow::bail!("job spec attempts must be between 1 and {MAX_ATTEMPTS}");
        }
        if !(MIN_TIMEOUT_SECONDS..=MAX_DEADLINE_SECONDS).contains(&self.deadline_seconds) {
            anyhow::bail!(
                "job spec deadline-seconds must be between {MIN_TIMEOUT_SECONDS} and {MAX_DEADLINE_SECONDS}"
            );
        }
        if !(MIN_TIMEOUT_SECONDS..=MAX_EXECUTOR_TIMEOUT_SECONDS)
            .contains(&self.executor_timeout_seconds)
        {
            anyhow::bail!(
                "job spec executor-timeout-seconds must be between {MIN_TIMEOUT_SECONDS} and {MAX_EXECUTOR_TIMEOUT_SECONDS}"
            );
        }
        if self.executor.argv.is_empty() || self.executor.program().trim().is_empty() {
            anyhow::bail!("job spec must declare executor argv, program first");
        }
        let mut seen = std::collections::BTreeSet::new();
        for key in &self.executor.allowed_env {
            if !is_env_key(key) {
                anyhow::bail!("job spec has an invalid allowed-env key '{key}'");
            }
            if !seen.insert(key) {
                anyhow::bail!("job spec repeats allowed-env key '{key}'");
            }
        }
        if self.executor.token_budget.is_some()
            && self.executor.accounting == Accounting::Unavailable
        {
            anyhow::bail!(
                "job spec declares token-budget but the executor exposes no enforceable accounting"
            );
        }
        if self.isolation.mechanism.trim().is_empty() {
            anyhow::bail!("job spec must declare an external isolation mechanism");
        }
        if matches!(
            self.isolation.mechanism.to_ascii_lowercase().as_str(),
            "worktree" | "git-worktree" | "none"
        ) {
            anyhow::bail!(
                "a git worktree is not a process/network/filesystem sandbox; \
                 job spec must name the external isolation mechanism"
            );
        }
        Ok(())
    }

    /// Stable digest of the spec's semantic content.
    ///
    /// # Errors
    ///
    /// Returns an error when the spec cannot be serialized.
    pub fn digest(&self) -> Result<String> {
        let bytes = serde_json::to_vec(self).context("serialize job spec for digest")?;
        Ok(hex::encode(sha2::Sha256::digest(bytes)))
    }

    /// Job id derived from the spec digest; changing the spec starts a new job.
    ///
    /// # Errors
    ///
    /// Returns an error when the spec cannot be serialized.
    pub fn job_id(&self) -> Result<String> {
        let digest = self.digest()?;
        Ok(format!("job-{}", &digest[..JOB_ID_HEX_CHARS]))
    }
}

/// Whether `key` is a conventional environment variable name.
fn is_env_key(key: &str) -> bool {
    !key.is_empty()
        && key.bytes().enumerate().all(|(index, byte)| {
            byte == b'_' || byte.is_ascii_alphabetic() || (index > 0 && byte.is_ascii_digit())
        })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn spec() -> JobSpec {
        JobSpec {
            schema_version: SPEC_SCHEMA_VERSION,
            task: "fix the failing sensor".to_owned(),
            revision: "HEAD".to_owned(),
            signal_set: "verification".to_owned(),
            attempts: DEFAULT_ATTEMPTS,
            deadline_seconds: DEFAULT_DEADLINE_SECONDS,
            executor_timeout_seconds: DEFAULT_EXECUTOR_TIMEOUT_SECONDS,
            executor: ExecutorSpec {
                argv: vec!["executor".to_owned()],
                allowed_env: vec!["EXECUTOR_TOKEN".to_owned()],
                accounting: Accounting::Unavailable,
                token_budget: None,
                result_required: true,
            },
            isolation: IsolationSpec {
                mechanism: "container".to_owned(),
            },
        }
    }

    #[test]
    fn a_valid_spec_passes() {
        spec().validate().unwrap();
    }

    #[test]
    fn a_worktree_is_not_accepted_as_isolation() {
        let mut spec = spec();
        spec.isolation.mechanism = "git-worktree".to_owned();
        let error = spec.validate().unwrap_err().to_string();
        assert!(
            error.contains("not a process/network/filesystem sandbox"),
            "{error}"
        );
    }

    #[test]
    fn a_token_budget_requires_enforceable_accounting() {
        let mut spec = spec();
        spec.executor.token_budget = Some(1);
        let error = spec.validate().unwrap_err().to_string();
        assert!(error.contains("no enforceable accounting"), "{error}");
        spec.executor.accounting = Accounting::Reported;
        spec.validate().unwrap();
    }

    #[test]
    fn zero_budgets_are_usage_errors() {
        let mut spec = spec();
        spec.attempts = 0;
        assert!(spec.validate().is_err());
        spec.attempts = 1;
        spec.deadline_seconds = 0;
        assert!(spec.validate().is_err());
    }

    #[test]
    fn a_changed_spec_changes_the_job_id() {
        let first = spec();
        let mut second = spec();
        assert_eq!(first.job_id().unwrap(), second.job_id().unwrap());
        second.task = "a different task".to_owned();
        assert_ne!(first.job_id().unwrap(), second.job_id().unwrap());
    }
}

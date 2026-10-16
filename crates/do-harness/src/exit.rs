//! Exit-code classification for CLI failures.
//!
//! One rule, stated once: an error the operator can fix by changing the
//! invocation or the environment *before any check runs* is a usage problem
//! (exit 2); anything else is a check that ran and failed (exit 1). Errors are
//! usage-classified by being wrapped in [`UsageError`], so an unwrapped error
//! defaults to the loud verification failure rather than silently exiting 2.

use anyhow::Error;

use crate::CliError;

/// An operator/environment problem rather than a failed check.
#[derive(Debug)]
pub struct UsageError(Error);

impl UsageError {
    /// Marks an error as a usage/discovery problem.
    pub fn wrap(err: impl Into<Error>) -> Self {
        Self(err.into())
    }

    /// Marks a message as a usage/discovery problem.
    pub fn msg(message: impl Into<String>) -> Self {
        Self(Error::msg(message.into()))
    }
}

impl std::fmt::Display for UsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for UsageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

/// Whether a chain entry is the usage marker.
///
/// A named predicate rather than an inline closure: `clippy` asks for the
/// method path form, and this reads better than the turbofish it suggests.
fn is_usage_marker(cause: &(dyn std::error::Error + 'static)) -> bool {
    cause.is::<UsageError>()
}

/// Classifies an error into the CLI's exit-code contract.
///
/// The marker is located anywhere in the chain and the error is passed through
/// whole, so a caller adding `context()` above a marked error changes neither
/// the classification nor the rendered message.
#[must_use]
pub fn classify(err: Error) -> CliError {
    if err.chain().any(is_usage_marker) {
        CliError::Usage(err)
    } else {
        CliError::Verify(err)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// Marked errors exit 2; everything else stays a verification failure.
    #[test]
    fn marked_errors_are_usage_and_the_default_is_verify() {
        let usage = classify(UsageError::msg("not a git repository").into());
        assert_eq!(usage.exit_code(), 2);

        let verify = classify(Error::msg("eval failed for skill(s): harness"));
        assert_eq!(verify.exit_code(), 1);
    }

    /// The marker survives an intervening `context()` layer, and the rendered
    /// chain keeps both messages.
    #[test]
    fn marker_survives_context_layers() {
        let err =
            Error::from(UsageError::msg("skill 'nope' not found")).context("while running eval");
        let classified = classify(err);
        assert_eq!(classified.exit_code(), 2);
        let rendered = classified.to_string();
        assert!(rendered.contains("while running eval"), "{rendered}");
        assert!(rendered.contains("skill 'nope' not found"), "{rendered}");
    }

    /// A wrapped source error keeps its own message.
    #[test]
    fn wrapped_source_errors_keep_their_message() {
        let source = std::io::Error::new(std::io::ErrorKind::NotFound, "no such file");
        let classified = classify(UsageError::wrap(source).into());
        assert_eq!(classified.exit_code(), 2);
        assert!(classified.to_string().contains("no such file"));
    }
}

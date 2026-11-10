//! Collaborator-permission reads through `gh api` (#514).
//!
//! Kept beside [`super::gh`] rather than inside it: the two failure modes are
//! load-bearing (a `404` is an answer, a `403` is not), so they carry their own
//! type and their own doc, and `gh.rs` stays under the LOC ceiling.

use std::path::Path;

use anyhow::{Result, bail};
use serde::Deserialize;

/// `gh`'s rendering of the collaborator endpoint's `404 Not Found`.
const HTTP_NOT_FOUND: &str = "HTTP 404";

/// `gh`'s rendering of a refused request: rate limit, or a token without the
/// collaborator scope.
const HTTP_FORBIDDEN: &str = "HTTP 403";

#[derive(Deserialize)]
struct PermissionResponse {
    permission: Option<String>,
    #[serde(rename = "role_name")]
    role_name: Option<String>,
}

/// What the collaborator-permission endpoint said about a login.
///
/// The two failures are not the same fact: a `404` proves the account holds no
/// collaboration relationship on this repository, while a `403` (rate limit, or
/// a token that may not read collaborators) proves nothing about the account. A
/// caller that treats them alike either trusts an unreadable author or refuses
/// a real third party.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionOutcome {
    /// The endpoint answered with a role (`admin`, `write`, `read`, …).
    Role(String),
    /// `HTTP 404`: the login is not a collaborator on this repository.
    NotCollaborator,
    /// The permission could not be determined, with the reason.
    Unknown(String),
}

/// Reads repository permission for a user via `gh api`.
///
/// # Errors
///
/// Returns an error when `gh api` fails or author permission cannot be determined.
pub fn user_permission(root: &Path, login: &str) -> Result<String> {
    match user_permission_outcome(root, login)? {
        PermissionOutcome::Role(role) => Ok(role),
        PermissionOutcome::NotCollaborator => Ok(String::new()),
        PermissionOutcome::Unknown(reason) => bail!("{reason}"),
    }
}

/// Reads repository permission, keeping the failure kind.
///
/// # Errors
///
/// Returns an error only when `gh` cannot be spawned.
pub fn user_permission_outcome(root: &Path, login: &str) -> Result<PermissionOutcome> {
    if login.trim().is_empty() {
        return Ok(PermissionOutcome::Unknown("empty login".to_owned()));
    }
    let endpoint = format!("repos/{{owner}}/{{repo}}/collaborators/{login}/permission");
    let (output, stderr) =
        crate::gh::try_output(root, &["api", &endpoint], "api collaborator permission")?;
    Ok(classify_permission_response(
        output.status.success(),
        &output.stdout,
        &stderr,
    ))
}

/// Turns one `gh api` result into a [`PermissionOutcome`].
///
/// The classification is the whole contract, so it is a pure function: the
/// difference between "no collaboration relationship" and "no answer" must not
/// depend on how the transport happened to report the failure.
fn classify_permission_response(success: bool, stdout: &[u8], stderr: &str) -> PermissionOutcome {
    if !success {
        let text = format!("{}\n{}", String::from_utf8_lossy(stdout), stderr.trim());
        // A `404` is an answer: no collaboration relationship. Everything else
        // (a `403` for rate limit or scope, a `5xx`, a network error) is the
        // absence of an answer and must never be read as "not a collaborator".
        if text.contains(HTTP_NOT_FOUND) {
            return PermissionOutcome::NotCollaborator;
        }
        let reason = if text.contains(HTTP_FORBIDDEN) {
            format!(
                "author permission could not be determined: {text}",
                text = text.trim()
            )
        } else {
            format!(
                "collaborator permission read failed: {text}",
                text = text.trim()
            )
        };
        return PermissionOutcome::Unknown(reason);
    }
    match serde_json::from_slice::<PermissionResponse>(stdout) {
        Ok(res) => PermissionOutcome::Role(res.role_name.or(res.permission).unwrap_or_default()),
        Err(error) => PermissionOutcome::Unknown(format!(
            "unexpected gh api collaborator permission JSON: {error}"
        )),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn failed(stderr: &str) -> PermissionOutcome {
        classify_permission_response(false, b"", stderr)
    }

    #[test]
    fn a_not_found_answer_is_a_real_third_party() {
        assert_eq!(
            failed("HTTP 404: Not Found"),
            PermissionOutcome::NotCollaborator
        );
    }

    #[test]
    fn a_forbidden_answer_is_unknown_and_keeps_its_reason() {
        let unknown = failed("HTTP 403: rate limit exceeded");
        assert!(
            matches!(&unknown, PermissionOutcome::Unknown(reason)
                if reason.contains("403") && reason.contains("could not be determined")),
            "a 403 proves nothing about the account: {unknown:?}"
        );
    }

    #[test]
    fn an_odd_failure_is_unknown_rather_than_a_third_party() {
        for stderr in ["HTTP 500: server error", "dial tcp: no such host", ""] {
            assert!(
                matches!(failed(stderr), PermissionOutcome::Unknown(_)),
                "{stderr:?} must not be read as no relationship"
            );
        }
    }

    #[test]
    fn an_answer_carries_its_role() {
        assert_eq!(
            classify_permission_response(true, br#"{"permission":"write"}"#, ""),
            PermissionOutcome::Role("write".to_owned())
        );
        // The endpoint also reports `role_name`; `read` is a real role and must
        // not collapse into "no relationship" — the caller decides that.
        assert_eq!(
            classify_permission_response(true, br#"{"permission":"read","role_name":"read"}"#, ""),
            PermissionOutcome::Role("read".to_owned())
        );
        assert_eq!(
            classify_permission_response(true, b"{}", ""),
            PermissionOutcome::Role(String::new())
        );
    }

    #[test]
    fn an_unparsable_body_is_unknown_not_a_role() {
        assert!(matches!(
            classify_permission_response(true, b"not json", ""),
            PermissionOutcome::Unknown(_)
        ));
    }
}

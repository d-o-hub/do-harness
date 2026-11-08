//! `GitHub` reads specific to `pr update-base` (#468).
//!
//! Authentication, repository identity, the PR author, and the open-PR list
//! are all read-only and scoped to eligibility and stack detection. They live
//! here (not in [`super::super::gh`]) so the shared PR transport stays small.

use std::path::Path;

use anyhow::Result;
use serde::Deserialize;

/// Authenticated user identity.
#[derive(Deserialize)]
struct CurrentUser {
    login: String,
}

/// Current repository identity and its default branch.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct RepoIdentity {
    #[serde(rename = "nameWithOwner")]
    pub name_with_owner: String,
    #[serde(rename = "defaultBranchRef", default)]
    pub default_branch_ref: Option<DefaultBranchRef>,
}

/// Default branch reference of a repository.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct DefaultBranchRef {
    #[serde(default)]
    pub name: String,
}

/// One open pull request in the list endpoint.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct OpenPr {
    pub number: u64,
    #[serde(rename = "headRefName", default)]
    pub head_ref_name: String,
}

#[derive(Deserialize)]
struct AuthorView {
    author: Option<super::super::gh::CommentAuthor>,
}

/// Reads the authenticated user's login.
///
/// # Errors
///
/// Returns an error when `gh` fails or the payload omits `login`.
pub fn current_user(root: &Path) -> Result<String> {
    let user: CurrentUser = crate::gh::json(root, &["api", "user"], "api user")?;
    Ok(user.login)
}

/// Reads the current repository identity and its default branch.
///
/// # Errors
///
/// Returns an error when `gh` fails or the payload cannot be parsed.
pub fn repo_identity(root: &Path) -> Result<RepoIdentity> {
    crate::gh::json(
        root,
        &["repo", "view", "--json", "nameWithOwner,defaultBranchRef"],
        "repo view",
    )
}

/// Reads the PR author's login (empty when the API omits it).
///
/// # Errors
///
/// Returns an error when `gh` fails.
pub fn pr_author(root: &Path, number: u64) -> Result<String> {
    let number_arg = number.to_string();
    let view: AuthorView = crate::gh::json(
        root,
        &["pr", "view", &number_arg, "--json", "author"],
        &format!("pr view {number}"),
    )?;
    Ok(view.author.map(|author| author.login).unwrap_or_default())
}

/// Lists open pull requests with their head branches.
///
/// # Errors
///
/// Returns an error when `gh` fails or the payload cannot be parsed.
pub fn open_prs(root: &Path) -> Result<Vec<OpenPr>> {
    crate::gh::json(
        root,
        &[
            "pr",
            "list",
            "--state",
            "open",
            "--json",
            "number,headRefName",
        ],
        "pr list",
    )
}

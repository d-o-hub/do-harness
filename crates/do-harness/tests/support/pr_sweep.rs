//! Shared fixtures for the `pr sweep` acceptance tests.
//!
//! Drives the real `do-harness` binary against a fake `gh` (see
//! `pr_readiness::fake_gh_router`), extended through `gh_extra.sh` with the
//! sweep-specific subcommands: `pr list`, `pr merge`, `pr close`, `run list`,
//! and the `update-branch`/`auto-merge`/`files` API endpoints.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use serde_json::{Value, json};

/// Standard head SHA.
pub const HEAD: &str = "2222222222222222222222222222222222222222";
/// Standard base SHA.
pub const BASE: &str = "1111111111111111111111111111111111111111";
/// Standard merge SHA produced by the fake `pr merge`.
pub const MERGE_SHA: &str = "3333333333333333333333333333333333333333";
/// Head SHA the fake update-branch handler writes for a `BEHIND` PR.
pub const UPDATED_HEAD: &str = "5555555555555555555555555555555555555555";

/// Writes the open-PR inventory.
pub fn write_pr_list(root: &Path, entries: &Value) {
    std::fs::write(root.join("pr_list.json"), entries.to_string()).unwrap();
}

/// The canonical full readiness view for one PR.
pub fn default_view(pr: u64) -> Value {
    json!({
        "number": pr,
        "title": format!("pr {pr}"),
        "url": format!("https://github.com/acme/widget/pull/{pr}"),
        "author": { "login": "d-o-hub" },
        "createdAt": "2026-10-01T00:00:00Z",
        "updatedAt": "2026-10-02T00:00:00Z",
        "baseRefName": "main",
        "baseRefOid": BASE,
        "headRefName": format!("topic-{pr}"),
        "headRefOid": HEAD,
        "mergeStateStatus": "CLEAN",
        "mergeable": "MERGEABLE",
        "autoMergeRequest": null,
        "reviewDecision": "",
        "state": "OPEN",
        "isDraft": false,
        "labels": [],
        "headRepository": { "nameWithOwner": "acme/widget" },
        "mergeCommit": null,
    })
}

/// Writes `view_<pr>.json`.
pub fn write_view(root: &Path, pr: u64, overrides: &Value) {
    let mut view = default_view(pr);
    if let (Some(target), Some(source)) = (view.as_object_mut(), overrides.as_object()) {
        for (key, value) in source {
            target.insert(key.clone(), value.clone());
        }
    }
    std::fs::write(root.join(format!("view_{pr}.json")), view.to_string()).unwrap();
}

/// Writes the optional `gh_extra.sh` router extension.
pub fn write_gh_extra(root: &Path, body: &str) {
    std::fs::write(root.join("gh_extra.sh"), body).unwrap();
}

/// The standard sweep router extension.
///
/// - `pr list` serves `pr_list.json` (empty by default).
/// - `pr view N` prefers `view_N.json`.
/// - `pr merge N --squash --match-head-commit SHA` records the exact argv and
///   flips `view_N.json` to MERGED with the standard merge SHA (unless
///   `fail_merge` exists).
/// - `pr close N` records the exact argv.
/// - `api … update-branch` records and rewrites `view_N.json` to the updated
///   head unless `fail_update` exists.
/// - `api … auto-merge` records and disarms unless `fail_disarm` exists.
/// - `api … files` serves `files_N.json` (one file by default).
/// - `run list` serves `runs.json` (empty by default).
pub fn standard_gh_extra() -> &'static str {
    r#"printf '%s\n' "$*" >> "$MOCK_DIR/gh_calls.txt"
if [ "$1" = "pr" ] && [ "$2" = "list" ]; then
  if [ -f "$MOCK_DIR/pr_list.json" ]; then cat "$MOCK_DIR/pr_list.json"; else printf '%s' '[]'; fi
  exit 0
fi
if [ "$1" = "pr" ] && [ "$2" = "view" ]; then
  if [ -f "$MOCK_DIR/view_$3.json" ]; then cat "$MOCK_DIR/view_$3.json"; exit 0; fi
fi
if [ "$1" = "pr" ] && [ "$2" = "merge" ]; then
  printf '%s\n' "$*" >> "$MOCK_DIR/mutations.txt"
  PR="$3"
  if [ -f "$MOCK_DIR/fail_merge" ]; then echo "HTTP 422: merge refused" >&2; exit 1; fi
  if [ -f "$MOCK_DIR/view_$PR.json" ]; then
    sed -E -e 's/"state":[[:space:]]*"OPEN"/"state": "MERGED"/' \
        -e 's/"mergeCommit":[[:space:]]*null/"mergeCommit": {"oid":"3333333333333333333333333333333333333333"}/' \
        "$MOCK_DIR/view_$PR.json" > "$MOCK_DIR/view_merged_$PR"
    mv "$MOCK_DIR/view_merged_$PR" "$MOCK_DIR/view_$PR.json"
  fi
  echo "✓ Merged pull request #$PR (3333333333333333333333333333333333333333)."
  exit 0
fi
if [ "$1" = "pr" ] && [ "$2" = "close" ]; then
  printf '%s\n' "$*" >> "$MOCK_DIR/mutations.txt"
  echo "Closed pull request #$3."
  exit 0
fi
if [ "$1" = "api" ]; then
  endpoint=""
  for arg in "$@"; do case "$arg" in repos/*|graphql) endpoint="$arg" ;; esac; done
  case "$endpoint" in
    *update-branch*)
      PR=$(printf '%s' "$endpoint" | sed -n 's|.*/pulls/\([0-9]*\)/update-branch.*|\1|p')
      if [ -f "$MOCK_DIR/fail_update" ]; then echo "HTTP 422: head is not up to date" >&2; exit 1; fi
      if [ -f "$MOCK_DIR/view_$PR.json" ]; then
        sed -E -e 's/"headRefOid":[[:space:]]*"4444444444444444444444444444444444444444"/"headRefOid": "5555555555555555555555555555555555555555"/' \
            -e 's/"mergeStateStatus":[[:space:]]*"BEHIND"/"mergeStateStatus": "CLEAN"/' \
            "$MOCK_DIR/view_$PR.json" > "$MOCK_DIR/view_updated_$PR"
        mv "$MOCK_DIR/view_updated_$PR" "$MOCK_DIR/view_$PR.json"
      fi
      echo '{"status":"in_progress","message":"Updating pull request branch"}'
      exit 0
      ;;
    *auto-merge*)
      if [ -f "$MOCK_DIR/fail_disarm" ]; then echo "HTTP 403: cannot disable" >&2; exit 1; fi
      PR=$(printf '%s' "$endpoint" | sed -n 's|.*/pulls/\([0-9]*\)/auto-merge.*|\1|p')
      if [ -f "$MOCK_DIR/view_$PR.json" ]; then
        sed -E -e 's/"autoMergeRequest":[[:space:]]*\{[^}]*\}/"autoMergeRequest": null/' \
            "$MOCK_DIR/view_$PR.json" > "$MOCK_DIR/view_disarmed_$PR"
        mv "$MOCK_DIR/view_disarmed_$PR" "$MOCK_DIR/view_$PR.json"
      fi
      echo '{}'
      exit 0
      ;;
    *files*)
      PR=$(printf '%s' "$endpoint" | sed -n 's|.*/pulls/\([0-9]*\)/files.*|\1|p')
      if [ -f "$MOCK_DIR/files_$PR.json" ]; then cat "$MOCK_DIR/files_$PR.json"; else printf '%s' '[{"filename":"src/lib.rs"}]'; fi
      exit 0
      ;;
  esac
fi
if [ "$1" = "run" ] && [ "$2" = "list" ]; then
  if [ -f "$MOCK_DIR/runs.json" ]; then cat "$MOCK_DIR/runs.json"; else printf '%s' '[]'; fi
  exit 0
fi
"#
}

/// Runs a sweep command against the real binary with the fake `gh` on PATH.
pub fn run_sweep(root: &Path, bin: &Path, args: &[&str]) -> (Option<i32>, String, String) {
    let path = std::env::var("PATH").unwrap_or_default();
    let mut cmd = super::harness(root);
    cmd.args(args);
    cmd.env("PATH", format!("{}:{path}", bin.display()));
    super::run(&mut cmd)
}

/// Standard fixture setup: git repo, policy fixtures, fake gh router.
pub fn setup() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    super::git(root, &["init", "-q"]);
    super::pr_readiness::ensure_authorization_fixtures(root);
    let bin = super::pr_readiness::fake_gh_router(root);
    write_gh_extra(root, standard_gh_extra());
    (dir, bin)
}

/// The standard one-PR open inventory.
pub fn one_pr_list(pr: u64) -> Value {
    json!([{
        "number": pr,
        "title": format!("pr {pr}"),
        "url": format!("https://github.com/acme/widget/pull/{pr}"),
        "author": { "login": "d-o-hub" },
        "createdAt": "2026-10-01T00:00:00Z",
        "updatedAt": "2026-10-02T00:00:00Z",
        "state": "OPEN",
        "isDraft": false,
        "baseRefName": "main",
        "baseRefOid": BASE,
        "headRefName": format!("topic-{pr}"),
        "headRefOid": HEAD,
        "mergeStateStatus": "CLEAN",
        "labels": [],
        "headRepository": { "nameWithOwner": "acme/widget" }
    }])
}

/// A failing check fixture.
pub fn failing_checks() -> Value {
    json!({
        "total_count": 1,
        "check_runs": [{
            "id": 1,
            "name": "verify",
            "status": "completed",
            "conclusion": "failure",
            "app": { "slug": "github-actions" }
        }]
    })
}

/// Reads the mutation ledger (one command per line).
pub fn mutations(root: &Path) -> Vec<String> {
    let path = root.join("mutations.txt");
    std::fs::read_to_string(path)
        .map(|text| {
            text.lines()
                .map(str::to_owned)
                .filter(|line| !line.trim().is_empty())
                .collect()
        })
        .unwrap_or_default()
}

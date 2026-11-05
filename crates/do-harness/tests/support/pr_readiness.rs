//! Shared fixtures for the `pr ready` acceptance tests.
//!
//! Each fixture drives the real `do-harness` binary against a fake `gh` on
//! `PATH`, so readiness evaluation, exit codes, and report rendering are all
//! exercised without network access.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn harness(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    cmd.arg("--root").arg(root);
    cmd
}

/// Creates a fake `gh` script that dispatches API and view queries based on
/// mock JSON data written in `mock_dir`.
#[allow(clippy::too_many_lines)]
pub fn fake_gh_router(mock_dir: &Path) -> PathBuf {
    let bin = mock_dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let script = bin.join("gh");
    let script_content = format!(
        r#"#!/bin/sh
MOCK_DIR="{}"
# gh pr view ...
if [ "$1" = "pr" ] && [ "$2" = "view" ]; then
    if [ -f "$MOCK_DIR/view_seq.txt" ]; then
        count=$(cat "$MOCK_DIR/view_seq.txt")
        count=$((count + 1))
        echo "$count" > "$MOCK_DIR/view_seq.txt"
        if [ -f "$MOCK_DIR/view_$count.json" ]; then
            cat "$MOCK_DIR/view_$count.json"
            exit 0
        fi
    fi
    cat "$MOCK_DIR/view.json"
    exit 0
fi
# gh api ... — the endpoint is an argument, not `$2`: `--paginate` and
# `--jq` flags may precede it.
if [ "$1" = "api" ]; then
    is_mutation=0
    for arg in "$@"; do
        case "$arg" in
            -XPOST|-XPUT|-XPATCH|-XDELETE|POST|PUT|PATCH|DELETE) is_mutation=1 ;;
        esac
    done
    if [ "$is_mutation" -eq 1 ]; then
        if [ -f "$MOCK_DIR/assert_get_only" ]; then
            echo "mutation forbidden in GET-only api mock: $@" >&2
            exit 2
        fi
        printf '%s\n' "$*" >> "$MOCK_DIR/mutations.txt"
    fi
    endpoint=""
    for arg in "$@"; do
        case "$arg" in
            repos/*|graphql) endpoint="$arg" ;;
        esac
    done
    case "$endpoint" in
        *annotations*)
            if [ -f "$MOCK_DIR/fail_annotations" ]; then
                echo "HTTP 403: permission denied" >&2
                exit 1
            fi
            if [ -f "$MOCK_DIR/annotations.json" ]; then
                cat "$MOCK_DIR/annotations.json"
            else
                printf '%s' '[]'
            fi
            exit 0
            ;;
        *commits/*base*/check-runs*)
            if [ -f "$MOCK_DIR/check_runs_base.json" ]; then
                printf '['
                cat "$MOCK_DIR/check_runs_base.json"
                printf ']'
            else
                printf '%s' '[{{"total_count":0,"check_runs":[]}}]'
            fi
            exit 0
            ;;
        *check-runs*)
            if [ -f "$MOCK_DIR/fail_api" ]; then
                echo "HTTP 403: rate limit exceeded" >&2
                exit 1
            fi
            # Record the pagination flags so a test can prove the CLI asks for
            # every page instead of trusting a single response.
            for arg in "$@"; do
                case "$arg" in
                    --paginate|--slurp) printf '%s\n' "$arg" >> "$MOCK_DIR/check_runs_flags" ;;
                esac
            done
            if [ -f "$MOCK_DIR/empty_checks" ]; then
                printf '%s' '[{{"total_count":0,"check_runs":[]}}]'
            elif [ -f "$MOCK_DIR/check_runs_pages.json" ]; then
                # Already the full `--paginate --slurp` array of page documents.
                cat "$MOCK_DIR/check_runs_pages.json"
            elif [ -f "$MOCK_DIR/check_runs.json" ]; then
                printf '['
                cat "$MOCK_DIR/check_runs.json"
                printf ']'
            else
                # Default fixture: a green suite, so tests about conversations
                # or waivers do not have to restate CI.
                printf '%s' '[{{"total_count":1,"check_runs":[{{"id":1,"name":"verify","status":"completed","conclusion":"success","app":{{"slug":"github-actions"}}}}]}}]'
            fi
            exit 0
            ;;
        *collaborators*|*permission*)
            if [ -f "$MOCK_DIR/permission_fail" ]; then
                echo "HTTP 403: rate limit exceeded" >&2
                exit 1
            fi
            if [ -f "$MOCK_DIR/permission.json" ]; then
                cat "$MOCK_DIR/permission.json"
            else
                printf '%s' '{{"permission":"write","role_name":"write"}}'
            fi
            exit 0
            ;;
        *status*)
            if [ -f "$MOCK_DIR/status.json" ]; then
                printf '['
                cat "$MOCK_DIR/status.json"
                printf ']'
            else
                printf '%s' '[{{"total_count":0,"statuses":[]}}]'
            fi
            exit 0
            ;;
        *comments*)
            if [ -f "$MOCK_DIR/comments.json" ]; then
                cat "$MOCK_DIR/comments.json"
            else
                printf '%s' '[]'
            fi
            exit 0
            ;;
        *code-scanning/alerts*)
            if [ -f "$MOCK_DIR/code_scanning_alerts.json" ]; then
                cat "$MOCK_DIR/code_scanning_alerts.json"
            else
                printf '%s' '[]'
            fi
            exit 0
            ;;
        *compare*)
            if [ -f "$MOCK_DIR/compare_override.json" ]; then
                cat "$MOCK_DIR/compare_override.json"
                exit 0
            fi
            base=$(printf '%s' "$endpoint" | sed -n 's|.*/compare/\([^.]*\)\.\.\..*|\1|p')
            printf '{{"status":"ahead","base_commit":{{"sha":"%s"}},"merge_base_commit":{{"sha":"%s"}}}}' "$base" "$base"
            exit 0
            ;;
        *issues*)
            if [ "$is_mutation" -eq 1 ]; then
                count=101
                if [ -f "$MOCK_DIR/created_issues_count.txt" ]; then
                    count=$(cat "$MOCK_DIR/created_issues_count.txt")
                    count=$((count + 1))
                fi
                echo "$count" > "$MOCK_DIR/created_issues_count.txt"
                printf '{{"number": %s, "html_url": "https://github.com/acme/widget/issues/%s"}}' "$count" "$count"
                exit 0
            elif [ -f "$MOCK_DIR/issues_all.json" ]; then
                cat "$MOCK_DIR/issues_all.json"
            else
                printf '%s' '[]'
            fi
            exit 0
            ;;
        *reviews*)
            if [ -f "$MOCK_DIR/fail_reviews" ]; then
                echo "HTTP 403: permission denied" >&2
                exit 1
            fi
            if [ -f "$MOCK_DIR/reviews.json" ]; then
                cat "$MOCK_DIR/reviews.json"
            else
                printf '%s' '[]'
            fi
            exit 0
            ;;
        *rulesets/*)
            if [ -f "$MOCK_DIR/fail_ruleset_detail" ]; then
                echo "HTTP 403: permission denied" >&2
                exit 1
            fi
            if [ -f "$MOCK_DIR/ruleset_detail.json" ]; then
                cat "$MOCK_DIR/ruleset_detail.json"
            else
                echo "HTTP 404: Not Found" >&2
                exit 1
            fi
            exit 0
            ;;
        *rulesets*)
            if [ -f "$MOCK_DIR/fail_rulesets" ]; then
                echo "HTTP 403: permission denied" >&2
                exit 1
            fi
            if [ -f "$MOCK_DIR/rulesets.json" ]; then
                cat "$MOCK_DIR/rulesets.json"
            else
                printf '%s' '[]'
            fi
            exit 0
            ;;
        *rules/branches/*)
            if [ -f "$MOCK_DIR/fail_branch_rules" ]; then
                echo "HTTP 403: permission denied" >&2
                exit 1
            fi
            if [ -f "$MOCK_DIR/branch_rules.json" ]; then
                cat "$MOCK_DIR/branch_rules.json"
            else
                printf '%s' '[]'
            fi
            exit 0
            ;;
        graphql)
            # `gh` substitutes {{owner}}/{{repo}} only for `-F` fields; a raw `-f`
            # query ships the placeholders verbatim and resolves nothing, so
            # emulate that failure and let the flag regress loudly.
            after_val=""
            for arg in "$@"; do
                if [ "$arg" = "-f" ]; then
                    printf '%s' '{{"data":{{"repository":null}},"errors":[{{"message":"Could not resolve to a Repository with the name {{owner}}/{{repo}}."}}]}}'
                    exit 0
                fi
                case "$arg" in
                    after=*) after_val="${{arg#after=}}" ;;
                esac
            done
            if [ -n "$after_val" ] && [ -f "$MOCK_DIR/graphql_$after_val.json" ]; then
                cat "$MOCK_DIR/graphql_$after_val.json"
            elif [ -n "$after_val" ] && [ -f "$MOCK_DIR/fail_graphql_page2" ]; then
                echo "HTTP 500: GraphQL page 2 failure" >&2
                exit 1
            elif [ -f "$MOCK_DIR/graphql.json" ]; then
                cat "$MOCK_DIR/graphql.json"
            else
                printf '%s' '{{"data":{{"repository":{{"pullRequest":{{"reviewThreads":{{"nodes":[]}}}}}}}}}}'
            fi
            exit 0
            ;;
    esac
fi
echo "unknown fake gh call: $@" >&2
exit 1
"#,
        mock_dir.display()
    );
    std::fs::write(&script, script_content).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    bin
}

pub fn run_pr(root: &Path, bin: &Path, args: &[&str]) -> (Option<i32>, String, String) {
    ensure_authorization_fixtures(root);
    normalize_view(root);
    let path = std::env::var("PATH").unwrap_or_default();
    let output = harness(root)
        .args(args)
        .env("PATH", format!("{}:{path}", bin.display()))
        .output()
        .expect("spawn do-harness");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Default merge-policy and ruleset fixtures.
///
/// `pr ready` refuses to authorize without a readable `plans/pr-merge-policy.toml`
/// and a matching server ruleset (that strictness is proven by `pr_policy`),
/// so every readiness fixture seeds a coherent pair once. A test that wants to
/// prove the missing-policy or drift behaviour overwrites these files or calls
/// the binary through its own runner instead of this helper.
pub fn ensure_authorization_fixtures(root: &Path) {
    let plans = root.join("plans");
    std::fs::create_dir_all(&plans).unwrap();
    let policy = plans.join("pr-merge-policy.toml");
    if !policy.exists() {
        std::fs::write(
            &policy,
            r#"schema_version = 1
ruleset_id = 1
name = "main"
target = "branch"
source_type = "Repository"
source = "acme/widget"
enforcement = "active"
include_refs = ["~DEFAULT_BRANCH"]
exclude_refs = []
bypass_actors = []
current_user_can_bypass = "never"

[required_status_checks]
strict_required_status_checks_policy = true
do_not_enforce_on_create = true
required = [
  { context = "pr-required", integration_id = 15368 },
  { context = "semver", integration_id = 15368 },
]

[pull_request]
required_approving_review_count = 0
required_review_thread_resolution = true
dismiss_stale_reviews_on_push = false
required_reviewers = []
require_code_owner_review = false
require_last_push_approval = false
require_extra_approval_for_unattributed_changes = false
allowed_merge_methods = ["merge", "squash", "rebase"]

[[code_scanning_tools]]
tool = "CodeQL"
security_alerts_threshold = "high_or_higher"
alerts_threshold = "errors_and_warnings"

[code_quality]
severity = "errors"

[other_rules]
deletion = true
"#,
        )
        .unwrap();
    }

    let rulesets = root.join("rulesets.json");
    if !rulesets.exists() {
        std::fs::write(&rulesets, r#"[{"id":1}]"#).unwrap();
    }
    let detail = root.join("ruleset_detail.json");
    if !detail.exists() {
        std::fs::write(
            &detail,
            r#"{
  "id": 1,
  "name": "main",
  "target": "branch",
  "enforcement": "active",
  "conditions": {"ref_name": {"include": ["~DEFAULT_BRANCH"], "exclude": []}},
  "bypass_actors": [],
  "current_user_can_bypass": "never",
  "rules": [
    {"type": "deletion"},
    {"type": "code_scanning", "parameters": {"code_scanning_tools": [{"tool": "CodeQL", "security_alerts_threshold": "high_or_higher", "alerts_threshold": "errors_and_warnings"}]}},
    {"type": "code_quality", "parameters": {"severity": "errors"}},
    {"type": "pull_request", "parameters": {"required_approving_review_count": 0, "dismiss_stale_reviews_on_push": false, "required_reviewers": [], "require_code_owner_review": false, "require_last_push_approval": false, "required_review_thread_resolution": true, "require_extra_approval_for_unattributed_changes": false, "allowed_merge_methods": ["merge", "squash", "rebase"]}},
    {"type": "required_status_checks", "parameters": {"strict_required_status_checks_policy": true, "do_not_enforce_on_create": true, "required_status_checks": [{"context": "pr-required", "integration_id": 15368}, {"context": "semver", "integration_id": 15368}]}}
  ]
}"#,
        )
        .unwrap();
    }
    let branch_rules = root.join("branch_rules.json");
    if !branch_rules.exists() {
        std::fs::write(
            &branch_rules,
            r#"[{"type":"deletion","ruleset_id":1,"ruleset_source":"acme/widget","ruleset_source_type":"Repository"},
                {"type":"pull_request","ruleset_id":1,"ruleset_source":"acme/widget","ruleset_source_type":"Repository"},
                {"type":"required_status_checks","ruleset_id":1,"ruleset_source":"acme/widget","ruleset_source_type":"Repository"}]"#,
        )
        .unwrap();
    }
}

/// Fills the PR fields merge authorization needs into a legacy `view.json`.
///
/// The fixtures predate #466 and describe only number/baseRefName/headRefOid/
/// mergeStateStatus/mergeable. Authorization additionally needs the base SHA,
/// branch names, `state`, `isDraft` and the head repository, and an API that
/// omits them is `UNKNOWN` by design. Tests that assert exactly that behaviour
/// drive the binary through their own runner and skip this normalization.
pub fn normalize_view(root: &Path) {
    let path = root.join("view.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return;
    };
    let Some(map) = value.as_object_mut() else {
        return;
    };
    // A real API always reports a 40-character commit SHA; fixtures written
    // before #466 use short placeholders, which would make ancestry unprovable.
    let raw_head = map
        .get("headRefOid")
        .and_then(|v| v.as_str())
        .unwrap_or("head")
        .to_owned();
    let head = if raw_head.len() == 40 && raw_head.chars().all(|c| c.is_ascii_hexdigit()) {
        raw_head
    } else {
        use sha2::{Digest, Sha256};
        let digest = hex::encode(Sha256::digest(raw_head.as_bytes()));
        digest[..40].to_owned()
    };
    if let Some(slot) = map.get_mut("headRefOid") {
        *slot = serde_json::json!(head);
    }
    // The compare API only proves ancestry for full 40-character SHAs, so the
    // synthesized default base is a well-formed one.
    let default_base = "1".repeat(40);
    let defaults: [(&str, serde_json::Value); 6] = [
        ("baseRefOid", serde_json::json!(default_base)),
        ("baseRefName", serde_json::json!("main")),
        ("headRefName", serde_json::json!("topic")),
        ("state", serde_json::json!("OPEN")),
        ("isDraft", serde_json::json!(false)),
        (
            "headRepository",
            serde_json::json!({"nameWithOwner": "acme/widget"}),
        ),
    ];
    let mut changed = false;
    for (key, value) in defaults {
        if !map.contains_key(key) {
            map.insert(key.to_owned(), value);
            changed = true;
        }
    }
    if !map.contains_key("labels") {
        map.insert("labels".to_owned(), serde_json::json!([]));
        changed = true;
    }
    if changed {
        std::fs::write(&path, value.to_string()).unwrap();
    }
}

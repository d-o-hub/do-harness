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

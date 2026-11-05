//! Shared fixtures for the `pr update-base` acceptance tests (#468).
//!
//! Drives the real `do-harness` binary against a fake `gh` on `PATH`, with
//! mock JSON data written in `mock_dir`. The fake understands the update-
//! branch endpoint (pinned to `--expected-head-sha`), the bounded post-update
//! poll, repository identity, authentication, and the open-PR list, so
//! eligibility, verdicts, exit codes, and the read-only guarantee are all
//! exercised without network access.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

/// Creates a fake `gh` script for update-base fixtures.
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
    # After an accepted update-branch, stage the updated refs so the poll
    # observes the new head/base without relying on view-sequence counts.
    sed_expr=""
    if [ -f "$MOCK_DIR/updated_head.txt" ]; then
        sed_expr="s/(\"headRefOid\":\")([^\"]*)/\1$(cat "$MOCK_DIR/updated_head.txt")/"
    fi
    if [ -f "$MOCK_DIR/updated_base.txt" ]; then
        if [ -n "$sed_expr" ]; then
            sed_expr="${{sed_expr}}; "
        fi
        sed_expr="${{sed_expr}}s/(\"baseRefOid\":\")([^\"]*)/\1$(cat "$MOCK_DIR/updated_base.txt")/"
    fi
    if [ -n "$sed_expr" ]; then
        sed -E "$sed_expr" "$MOCK_DIR/view.json"
    else
        cat "$MOCK_DIR/view.json"
    fi
    exit 0
fi
# gh pr update-branch ...
if [ "$1" = "pr" ] && [ "$2" = "update-branch" ]; then
    printf '%s\n' "$*" >> "$MOCK_DIR/mutations.txt"
    if [ -f "$MOCK_DIR/fail_update_branch_403" ]; then
        echo "HTTP 403: Forbidden" >&2
        exit 1
    fi
    if [ -f "$MOCK_DIR/fail_update_branch_422" ]; then
        echo "HTTP 422: Unprocessable Entity (expected head mismatch)" >&2
        exit 1
    fi
    # Stage the updated refs only after an accepted update, so a pre-update
    # `pr view` still observes the original head.
    if [ -f "$MOCK_DIR/planned_new_head.txt" ]; then
        cat "$MOCK_DIR/planned_new_head.txt" > "$MOCK_DIR/updated_head.txt"
    fi
    if [ -f "$MOCK_DIR/planned_new_base.txt" ]; then
        cat "$MOCK_DIR/planned_new_base.txt" > "$MOCK_DIR/updated_base.txt"
    fi
    printf '%s' '{{"message":"Branch updated","url":"https://github.com/acme/widget/pull/1"}}'
    exit 0
fi
# gh pr list ...
if [ "$1" = "pr" ] && [ "$2" = "list" ]; then
    if [ -f "$MOCK_DIR/pr_list.json" ]; then
        cat "$MOCK_DIR/pr_list.json"
    else
        printf '%s' '[]'
    fi
    exit 0
fi
# gh repo view ...
if [ "$1" = "repo" ] && [ "$2" = "view" ]; then
    if [ -f "$MOCK_DIR/repo.json" ]; then
        cat "$MOCK_DIR/repo.json"
    else
        printf '%s' '{{"nameWithOwner":"acme/widget","defaultBranchRef":{{"name":"main"}}}}'
    fi
    exit 0
fi
# gh api ... — the endpoint is an argument, not `$2`: `--paginate` and
# `--jq` flags may precede it.
if [ "$1" = "api" ]; then
    if [ "$2" = "user" ]; then
        if [ -f "$MOCK_DIR/user.json" ]; then
            cat "$MOCK_DIR/user.json"
        else
            printf '%s' '{{"login":"alice"}}'
        fi
        exit 0
    fi
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
        *compare*)
            if [ -f "$MOCK_DIR/compare_override.json" ]; then
                cat "$MOCK_DIR/compare_override.json"
                exit 0
            fi
            if [ -f "$MOCK_DIR/compare_diverged_for_head.txt" ]; then
                target=$(cat "$MOCK_DIR/compare_diverged_for_head.txt")
                head=$(printf '%s' "$endpoint" | sed -n 's|.*\.\.\.\([^.]*\)$|\1|p')
                if [ "$head" = "$target" ]; then
                    base=$(printf '%s' "$endpoint" | sed -n 's|.*/compare/\([^.]*\)\.\.\..*|\1|p')
                    printf '{{"status":"diverged","base_commit":{{"sha":"%s"}},"merge_base_commit":{{"sha":"3333333333333333333333333333333333333333"}}}}' "$base"
                    exit 0
                fi
            fi
            base=$(printf '%s' "$endpoint" | sed -n 's|.*/compare/\([^.]*\)\.\.\..*|\1|p')
            printf '{{"status":"ahead","base_commit":{{"sha":"%s"}},"merge_base_commit":{{"sha":"%s"}}}}' "$base" "$base"
            exit 0
            ;;
        *collaborators*|*permission*)
            if [ -f "$MOCK_DIR/permission.json" ]; then
                cat "$MOCK_DIR/permission.json"
            else
                printf '%s' '{{"permission":"write","role_name":"write"}}'
            fi
            exit 0
            ;;
        *issues*)
            if [ -f "$MOCK_DIR/issues_all.json" ]; then
                cat "$MOCK_DIR/issues_all.json"
            else
                printf '%s' '[]'
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
        *reviews*)
            if [ -f "$MOCK_DIR/reviews.json" ]; then
                cat "$MOCK_DIR/reviews.json"
            else
                printf '%s' '[]'
            fi
            exit 0
            ;;
        *rulesets/*)
            if [ -f "$MOCK_DIR/ruleset_detail.json" ]; then
                cat "$MOCK_DIR/ruleset_detail.json"
            else
                echo "HTTP 404: Not Found" >&2
                exit 1
            fi
            exit 0
            ;;
        *rulesets*)
            if [ -f "$MOCK_DIR/rulesets.json" ]; then
                cat "$MOCK_DIR/rulesets.json"
            else
                printf '%s' '[]'
            fi
            exit 0
            ;;
        graphql)
            printf '%s' '{{"data":{{"repository":{{"pullRequest":{{"reviewThreads":{{"nodes":[]}}}}}}}}}}'
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

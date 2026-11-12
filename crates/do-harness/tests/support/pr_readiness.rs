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
    # Persistent movement mode: every `pr view` alternates between two fixture
    # views so readiness can never stabilize (used by the sweep race tests).
    if [ -f "$MOCK_DIR/always_moving" ]; then
        if [ -f "$MOCK_DIR/move_toggle.txt" ]; then
            rm -f "$MOCK_DIR/move_toggle.txt"
            toggle=2
        else
            echo x > "$MOCK_DIR/move_toggle.txt"
            toggle=1
        fi
        cat "$MOCK_DIR/view_move_$toggle.json"
        exit 0
    fi
    if [ -f "$MOCK_DIR/view_seq.txt" ]; then
        count=$(cat "$MOCK_DIR/view_seq.txt")
        count=$((count + 1))
        echo "$count" > "$MOCK_DIR/view_seq.txt"
        if [ -f "$MOCK_DIR/view_$count.json" ]; then
            cat "$MOCK_DIR/view_$count.json"
            exit 0
        fi
    fi
    if [ -f "$MOCK_DIR/view_$3.json" ]; then
        cat "$MOCK_DIR/view_$3.json"
        exit 0
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
            # Post-merge main verification serves its own fixture keyed on the
            # standard merge SHA (3333…), so a test can mark the merged commit's
            # runs failing without touching the head's check fixture.
            if [ -f "$MOCK_DIR/check_runs_merge.json" ] && printf '%s' "$endpoint" | grep -q '3333333333333333333333333333333333333333'; then
                cat "$MOCK_DIR/check_runs_merge.json"
                exit 0
            fi
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
                # Default fixture: a green suite that satisfies the default
                # overlay's required contexts (`pr-required`, `semver`), so
                # tests about conversations or waivers do not have to restate
                # CI. A fixture naming only `verify` would be a *missing
                # required check* once the required set comes from the merge
                # policy, not a green suite.
                printf '%s' '[{{"total_count":3,"check_runs":[{{"id":1,"name":"verify","status":"completed","conclusion":"success","app":{{"id":15368,"slug":"github-actions"}}}},{{"id":2,"name":"pr-required","status":"completed","conclusion":"success","app":{{"id":15368,"slug":"github-actions"}}}},{{"id":3,"name":"semver","status":"completed","conclusion":"success","app":{{"id":15368,"slug":"github-actions"}}}}]}}]'
            fi
            exit 0
            ;;
        *collaborators*|*permission*)
            # Every permission read is recorded, so a test can prove a
            # classification cost zero calls (#514).
            printf '%s\n' "$endpoint" >> "$MOCK_DIR/permission_calls.txt"
            if [ -f "$MOCK_DIR/permission_fail" ]; then
                echo "HTTP 403: rate limit exceeded" >&2
                exit 1
            fi
            if [ -f "$MOCK_DIR/permission_404" ]; then
                echo "HTTP 404: Not Found" >&2
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
        repos/*)
            # The bare repository resource: the merge settings the derived
            # rules are bounded by. A plain `repos/*` arm is greedy (`*` crosses
            # `/`), so it would also swallow `repos/o/r/pulls/2/files` and the
            # other per-endpoint handlers from `gh_extra.sh` below. Only a
            # two-slash endpoint is the repository resource; anything deeper
            # falls out of this case and reaches those handlers.
            if [ "$(printf '%s' "$endpoint" | tr -cd '/' | wc -c)" -eq 2 ]; then
                if [ -f "$MOCK_DIR/fail_repo_settings" ]; then
                    echo "HTTP 403: resource not accessible by integration" >&2
                    exit 1
                fi
                if [ -f "$MOCK_DIR/repo_settings.json" ]; then
                    cat "$MOCK_DIR/repo_settings.json"
                else
                    printf '%s' '{{"allow_merge_commit":true,"allow_squash_merge":true,"allow_rebase_merge":true}}'
                fi
                exit 0
            fi
            ;;
    esac
fi
if [ -f "$MOCK_DIR/gh_extra.sh" ]; then
    . "$MOCK_DIR/gh_extra.sh"
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

pub use super::pr_readiness_fixtures::{ensure_authorization_fixtures, ensure_ruleset_fixtures};

pub fn run_pr(root: &Path, bin: &Path, args: &[&str]) -> (Option<i32>, String, String) {
    ensure_authorization_fixtures(root);
    run_pr_without_policy(root, bin, args)
}

/// Runs `pr` against the fake `gh` and deliberately leaves no
/// `plans/pr-merge-policy.toml`, which is the adopting-repository case.
///
/// Only the ruleset fixtures are seeded, so the merge decision has to come from
/// the server-derived rules rather than the overlay.
pub fn run_pr_without_policy(
    root: &Path,
    bin: &Path,
    args: &[&str],
) -> (Option<i32>, String, String) {
    std::fs::create_dir_all(root.join("plans")).unwrap();
    ensure_ruleset_fixtures(root);
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

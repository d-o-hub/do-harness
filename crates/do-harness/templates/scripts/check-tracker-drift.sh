#!/usr/bin/env bash
# check-tracker-drift.sh — compare status-document claims against the tracker.
#
# Reference `project-check` sensor: a status document (roadmap, GOAP snapshot,
# status page) that counts open pull requests or issues silently rots. This
# check reads those claims from the document, compares them with `gh pr list`
# / `gh issue list`, and reports each mismatch with its line number.
#
# Usage: check-tracker-drift.sh [--doc PATH] [--repo OWNER/NAME]
#   --doc   Status document to scan (default: plans/ROADMAP_ACTIVE.md)
#   --repo  Repository `gh` should query (default: the current checkout's)
#
# Declare it as a soft check in `do-harness.toml`:
#
#   [[sensors]]
#   name = "tracker-drift"
#   kind = "project-check"
#   argv = ["bash", "scripts/check-tracker-drift.sh", "--doc", "plans/ROADMAP_ACTIVE.md"]
#   fix = "refresh the open-PR/issue counts in the status document"
#   when-changed = ["plans/**/*.md", "scripts/check-tracker-drift.sh"]
#
# Fail-closed: a claim that cannot be checked (missing document, missing `gh`,
# unresolvable repository) is reported as a finding rather than ignored, so a
# drifted document cannot pass by making the check unable to run.
#
# `FINDINGS: <n>` reports the number of drifted or unchecked claims so
# `verify --record --bless` can pin the count.

set -euo pipefail

DOC="plans/ROADMAP_ACTIVE.md"
REPO=""

while (( $# > 0 )); do
    case "$1" in
        --doc) DOC="${2:?--doc needs a path}"; shift 2 ;;
        --doc=*) DOC="${1#*=}"; shift ;;
        --repo) REPO="${2:?--repo needs owner/name}"; shift 2 ;;
        --repo=*) REPO="${1#*=}"; shift ;;
        -h | --help)
            sed -n '2,20p' "$0"
            exit 0
            ;;
        *)
            echo "unknown argument: $1" >&2
            exit 2
            ;;
    esac
done

findings=0

if [[ ! -f "$DOC" ]]; then
    echo "drift: status document '$DOC' not found (pass --doc <path>)"
    echo "FINDINGS: 1"
    exit 1
fi

if ! command -v gh >/dev/null 2>&1; then
    echo "drift: 'gh' is required to compare '$DOC' with the tracker"
    echo "FINDINGS: 1"
    exit 1
fi

gh_args=()
[[ -n "$REPO" ]] && gh_args+=(--repo "$REPO")

count_open() {
    local kind="$1" output
    if ! output="$(gh "${gh_args[@]}" "$kind" list --state open --limit 1000 \
        --json number --jq 'length' 2>&1)"; then
        # stderr, not stdout: the caller captures stdout to read the count, so a
        # diagnostic written there is discarded together with the empty result
        # and the operator only sees a bare `FINDINGS: 1`.
        echo "drift: could not read open $kind from the tracker: $output" >&2
        return 1
    fi
    printf '%s' "$output"
}

open_prs="$(count_open pr)" || open_prs=""
open_issues="$(count_open issue)" || open_issues=""

if [[ -z "$open_prs" || -z "$open_issues" ]]; then
    echo "FINDINGS: 1"
    exit 1
fi

# Claims look like "13 open PRs", "6 open issues", "3 open pull requests".
while IFS=: read -r line_no text; do
    [[ -n "$line_no" ]] || continue
    claim="$(printf '%s' "$text" | grep -Eio '[0-9]+[[:space:]]+open[[:space:]]+(prs|pull requests|issues)' | head -1 || true)"
    [[ -n "$claim" ]] || continue
    claimed="$(printf '%s' "$claim" | grep -Eo '^[0-9]+')"
    case "$(printf '%s' "$claim" | tr '[:upper:]' '[:lower:]')" in
        *issues) actual="$open_issues" noun="open issues" ;;
        *) actual="$open_prs" noun="open PRs" ;;
    esac
    if [[ "$claimed" != "$actual" ]]; then
        echo "drift: $DOC:$line_no claims $claimed $noun, tracker reports $actual"
        findings=$((findings + 1))
    fi
done < <(grep -nEio '[0-9]+[[:space:]]+open[[:space:]]+(prs|pull requests|issues)' "$DOC" || true)

if (( findings > 0 )); then
    echo "tracker drift: $findings claim(s) in $DOC disagree with the tracker."
    echo "FINDINGS: $findings"
    exit 1
fi

echo "tracker-drift OK: $DOC agrees with the tracker ($open_prs open PRs, $open_issues open issues)."
echo "FINDINGS: 0"

#!/usr/bin/env bash
# approve-release-runs.sh — approve the held workflow runs of a release PR.
#
# Why: `auto-release.yml` opens the version-bump PR as `github-actions[bot]`, and
# GitHub queues that PR's `pull_request` workflows as `action_required` until a
# maintainer approves them. Every release therefore stalls on a UI click, and the
# release PR shows BLOCKED with no failing check.
#
# Safety: this approves only runs that all of the following hold for — the
# branch is `release/*`, the workflow is one of the release gate's, and the
# triggering actor is `github-actions[bot]`. A held run from a fork, a
# first-time contributor, or any other actor is listed and left alone, because
# approving that run is the human review the gate exists for.
#
# Usage:
#   bash scripts/approve-release-runs.sh --branch release/v0.4.7 [--dry-run]
#
# Exit codes: 0 nothing left to approve, or every eligible run approved;
# 2 usage or repository-resolution failure; 1 a run could not be approved.
set -euo pipefail

# shellcheck disable=SC1091 # resolved at runtime from this script's directory
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

#: Workflow names the release gate runs on the bump pull request.
RELEASE_WORKFLOWS=(verify semver)
#: The only actor whose held runs this script may approve.
RELEASE_ACTOR='github-actions[bot]'

branch=''
dry_run=0
while [ "$#" -gt 0 ]; do
    case "$1" in
        --branch) branch=${2:-}; shift 2 ;;
        --branch=*) branch=${1#*=}; shift ;;
        --dry-run) dry_run=1; shift ;;
        -h | --help) sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "approve-release-runs: unknown argument: $1" >&2; exit 2 ;;
    esac
done

[ -n "$branch" ] || { echo "approve-release-runs: --branch is required" >&2; exit 2; }
case "$branch" in
    release/*) ;;
    *) echo "approve-release-runs: only release/* branches are eligible (got '$branch')" >&2; exit 2 ;;
esac

command -v gh >/dev/null 2>&1 || { echo "approve-release-runs: gh is required" >&2; exit 2; }
command -v jq >/dev/null 2>&1 || { echo "approve-release-runs: jq is required" >&2; exit 2; }

repo=$(gh repo view --json nameWithOwner --jq .nameWithOwner 2>/dev/null || true)
[ -n "$repo" ] || { echo "approve-release-runs: cannot resolve the repository" >&2; exit 2; }

allowed_workflows=$(printf '%s\n' "${RELEASE_WORKFLOWS[@]}")

head_sha=$(gh api "repos/$repo/commits/$branch" --jq .sha 2>/dev/null || true)
[ -n "$head_sha" ] || { echo "approve-release-runs: cannot resolve $branch" >&2; exit 2; }

held=$(gh run list --branch "$branch" --limit 50 \
    --json databaseId,workflowName,conclusion,headSha \
    --jq '.[] | select(.conclusion == "action_required") | "\(.databaseId)\t\(.workflowName)\t\(.headSha)"')

if [ -z "$held" ]; then
    echo "approve-release-runs: no held runs on $branch"
    exit 0
fi

approved=0
skipped=0
while IFS=$'\t' read -r id name sha; do
    [ -n "$id" ] || continue
    if [ "$sha" != "$head_sha" ]; then
        echo "skipped $id ($name): superseded head ${sha:0:7}"
        skipped=$((skipped + 1))
        continue
    fi
    if ! printf '%s\n' "$allowed_workflows" | grep -qxF "$name"; then
        echo "skipped $id ($name): not a release-gate workflow"
        skipped=$((skipped + 1))
        continue
    fi
    actor=$(gh api "repos/$repo/actions/runs/$id" --jq '.triggering_actor.login' 2>/dev/null || true)
    if [ "$actor" != "$RELEASE_ACTOR" ]; then
        echo "skipped $id ($name): triggering actor '${actor:-unknown}' is not $RELEASE_ACTOR"
        skipped=$((skipped + 1))
        continue
    fi
    if [ "$dry_run" -eq 1 ]; then
        echo "would approve $id ($name) on $branch"
        continue
    fi
    if gh api -X POST "repos/$repo/actions/runs/$id/approve" >/dev/null 2>&1; then
        echo "approved $id ($name)"
        approved=$((approved + 1))
    else
        echo "approve-release-runs: failed to approve run $id" >&2
        exit 1
    fi
done <<<"$held"

if [ "$dry_run" -eq 1 ]; then
    echo "approve-release-runs: dry run, $skipped run(s) not eligible"
else
    echo "approve-release-runs: approved $approved run(s), $skipped not eligible"
fi

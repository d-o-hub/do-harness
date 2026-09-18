#!/usr/bin/env bash
# Report whether a PR introduces any effective change: merge-base..head tree delta.
#
# Usage: no-effect.sh <pr-number>
# Output (TSV): <no-effect|has-effect|unknown> <pr> <head_sha>
# Exit: 0 determined, 3 unknown (caller must not close).
set -euo pipefail

pr="${1:?usage: no-effect.sh <pr-number>}"

base=""
head=""
if ! read -r base head < <(gh pr view "$pr" --json baseRefName,headRefOid --jq '[.baseRefName, .headRefOid] | @tsv'); then
  printf 'unknown\t%s\t\n' "$pr"
  exit 3
fi
if [ -z "$base" ] || [ -z "$head" ]; then
  printf 'unknown\t%s\t\n' "$pr"
  exit 3
fi

shallow=$(git rev-parse --is-shallow-repository 2>/dev/null || printf 'false')

if [ "$shallow" != "true" ]; then
  head_fetched=""
  if git fetch --quiet origin "refs/pull/$pr/head" >/dev/null 2>&1; then
    head_fetched=$(git rev-parse FETCH_HEAD 2>/dev/null || true)
  fi
  base_tip=""
  if git fetch --quiet origin "$base" >/dev/null 2>&1; then
    base_tip=$(git rev-parse --verify --quiet "origin/$base" || true)
  fi
  if [ -n "$head_fetched" ] && [ -n "$base_tip" ]; then
    mb=$(git merge-base "$base_tip" "$head_fetched")
    if git diff --quiet "$mb" "$head_fetched" --; then
      printf 'no-effect\t%s\t%s\n' "$pr" "$head"
    else
      printf 'has-effect\t%s\t%s\n' "$pr" "$head"
    fi
    exit 0
  fi
fi

count=$(gh api "repos/{owner}/{repo}/compare/$base...$head" --jq '.files | length' 2>/dev/null || true)
if [ -z "$count" ]; then
  printf 'unknown\t%s\t%s\n' "$pr" "$head"
  exit 3
fi
if [ "$count" -eq 0 ]; then
  printf 'no-effect\t%s\t%s\n' "$pr" "$head"
else
  printf 'has-effect\t%s\t%s\n' "$pr" "$head"
fi

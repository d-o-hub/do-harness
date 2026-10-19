#!/usr/bin/env bash
# open-pr.sh — push the item branch and open (or report) its pull request.
#
# Usage: open-pr.sh (--issue N | --task ID) [--title T] [--body-file FILE] [--dry-run]
# Requires HEAD to be the item branch with commits ahead of origin/main.
# Idempotent: an existing PR for the branch is reported, never duplicated.
set -euo pipefail

base_branch="main"

fail() {
  printf 'open-pr: %s\n' "$1" >&2
  exit "${2:-1}"
}

harness_bin() {
  if [ -n "${DO_HARNESS_BIN:-}" ] && [ -x "${DO_HARNESS_BIN}" ]; then
    printf '%s' "${DO_HARNESS_BIN}"
  elif command -v do-harness >/dev/null 2>&1; then
    command -v do-harness
  else
    fail "no do-harness binary; set DO_HARNESS_BIN"
  fi
}

kind=""
id=""
title=""
body_file=""
dry_run=0
while [ $# -gt 0 ]; do
  case "$1" in
    --issue)
      kind=issue
      id="${2:?open-pr: --issue needs a number}"
      shift 2
      ;;
    --task)
      kind=task
      id="${2:?open-pr: --task needs an id}"
      shift 2
      ;;
    --title)
      title="${2:?open-pr: --title needs a value}"
      shift 2
      ;;
    --body-file)
      body_file="${2:?open-pr: --body-file needs a path}"
      shift 2
      ;;
    --dry-run)
      dry_run=1
      shift
      ;;
    *)
      fail "unknown argument: $1 (see header)" 2
      ;;
  esac
done
[ -n "$kind" ] || fail "one of --issue N or --task ID is required" 2

root="$(git rev-parse --show-toplevel 2>/dev/null)" || fail "not inside a git work tree"
cd "$root"

branch="$(git symbolic-ref --short HEAD 2>/dev/null || true)"
[ -n "$branch" ] || fail "HEAD is not on a branch"

case "$branch" in
  "$kind-$id-"*) ;;
  *) fail "branch '$branch' does not belong to $kind #$id" ;;
esac

if [ -z "$title" ]; then
  if [ "$kind" = issue ]; then
    title="$(gh issue view "$id" --json title | jq -r '.title')"
  else
    bin="$(harness_bin)"
    title="$("$bin" --root "$root" task show "$id" --format json | jq -r '.title')"
  fi
fi

if [ -n "$body_file" ]; then
  [ -f "$body_file" ] || fail "body file '$body_file' not found"
  body="$(cat "$body_file")"
elif [ "$kind" = issue ]; then
  body="$(printf 'Implements #%s.\n\nCloses #%s\n' "$id" "$id")"
else
  body="$(printf 'Implements plan task %s: %s\n' "$id" "$title")"
fi

existing="$(gh pr list --state open --head "$branch" --json number --limit 5)"
existing_number="$(printf '%s' "$existing" | jq -r '.[0].number // empty')"
if [ -n "$existing_number" ]; then
  printf 'already-exists: PR #%s for branch %s\n' "$existing_number" "$branch"
  exit 0
fi

ahead="$(git rev-list --count "origin/$base_branch..HEAD")"
[ "$ahead" -gt 0 ] || fail "no commits ahead of origin/$base_branch; nothing to open a PR for"

if [ "$dry_run" -eq 1 ]; then
  printf 'dry-run: git push -u origin %s\n' "$branch"
  printf 'dry-run: gh api -X POST repos/{owner}/{repo}/pulls -f title=%s -f head=%s -f base=%s -f body=%s\n' \
    "$title" "$branch" "$base_branch" "$body"
  exit 0
fi

git push -q -u origin HEAD

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
response="$("$script_dir/retry.sh" --attempts 5 --delay 15 -- \
  gh api -X POST "repos/{owner}/{repo}/pulls" \
    -f "title=$title" -f "head=$branch" -f "base=$base_branch" -f "body=$body")"

number="$(printf '%s' "$response" | jq -r '.number')"
url="$(printf '%s' "$response" | jq -r '.html_url // empty')"
printf 'PR #%s created: %s\n' "$number" "$url"

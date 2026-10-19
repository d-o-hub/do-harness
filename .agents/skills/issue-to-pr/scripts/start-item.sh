#!/usr/bin/env bash
# start-item.sh — cut (or resume) the branch for one queue item.
#
# Usage: start-item.sh (--issue N | --task ID) [--dry-run]
# Refuses a dirty tree, a HEAD not on the base branch, and held, covered, or
# non-pending items; --dry-run validates the same way and mutates nothing.
set -euo pipefail

base_branch="main"

fail() {
  printf 'start-item: %s\n' "$1" >&2
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

slug() {
  printf '%s' "$1" \
    | tr '[:upper:]' '[:lower:]' \
    | sed -E 's/[^a-z0-9]+/-/g; s/^-+//; s/-+$//' \
    | cut -c1-40 \
    | sed -E 's/-+$//'
}

kind=""
id=""
dry_run=0
while [ $# -gt 0 ]; do
  case "$1" in
    --issue)
      kind=issue
      id="${2:?start-item: --issue needs a number}"
      shift 2
      ;;
    --task)
      kind=task
      id="${2:?start-item: --task needs an id}"
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

[ -z "$(git status --porcelain)" ] || fail "working tree is dirty; commit or stash first"

current="$(git symbolic-ref --short HEAD 2>/dev/null || true)"
[ "$current" = "$base_branch" ] || fail "on branch '$current'; start items from $base_branch"

if [ "$kind" = issue ]; then
  item="$(gh issue view "$id" --json number,title,labels,body)"
  title="$(printf '%s' "$item" | jq -r '.title')"
  held="$(printf '%s' "$item" | jq -r \
    '[.labels[].name] | map(select(. == "hold" or . == "wip" or . == "do-not-merge" or . == "wontfix" or . == "duplicate" or . == "blocked")) | .[0] // empty')"
  [ -z "$held" ] || fail "refusing issue #$id: excluded by label $held"
  covered="$(gh pr list --state open --json number,headRefName,body --limit 200 \
    | jq -r --arg n "$id" \
      '[.[] | select((.headRefName | startswith("issue-\($n)-")) or ((.body // "") | test("(^|[^0-9])Closes #\($n)([^0-9]|$)")))] | .[0].number // empty')"
  [ -z "$covered" ] || fail "refusing issue #$id: open PR #$covered covers it"
  branch="issue-$id-$(slug "$title")"
else
  bin="$(harness_bin)"
  item="$("$bin" --root "$root" task show "$id" --format json)"
  status="$(printf '%s' "$item" | jq -r '.status')"
  [ "$status" = "pending" ] || fail "refusing task $id: status '$status' is not pending"
  title="$(printf '%s' "$item" | jq -r '.title')"
  branch="task-$id-$(slug "$title")"
fi

if git show-ref --verify --quiet "refs/heads/$branch"; then
  if [ "$dry_run" -eq 1 ]; then
    printf 'dry-run: resume branch %s\n' "$branch"
    exit 0
  fi
  git checkout -q "$branch"
  printf 'resumed: %s #%s on branch %s\n' "$kind" "$id" "$branch"
  exit 0
fi

if [ "$dry_run" -eq 1 ]; then
  base_sha="$(git rev-parse "refs/remotes/origin/$base_branch" 2>/dev/null || printf 'unknown')"
  printf 'dry-run: %s #%s -> branch %s from origin/%s@%s\n' \
    "$kind" "$id" "$branch" "$base_branch" "$base_sha"
  exit 0
fi

git fetch -q origin "$base_branch"
git checkout -q -b "$branch" "origin/$base_branch"
printf 'started: %s #%s on branch %s\n' "$kind" "$id" "$branch"

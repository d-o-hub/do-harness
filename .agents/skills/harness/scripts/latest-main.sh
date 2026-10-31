#!/usr/bin/env bash
# latest-main.sh — fetch or query latest origin/main without mutating worktree state.
#
# Usage: latest-main.sh [--root DIR] [--read-only]
#   --root DIR     target git worktree (default: $PWD)
#   --read-only    query origin/main via ls-remote without fetching or touching refs
#
# Exit codes:
#   0: origin/main resolved; prints full commit SHA to stdout
#   2: invalid argument, not a git worktree, missing origin/main, or fetch failed
set -euo pipefail

# Clear inherited Git repository-selection variables so git commands always target
# the target worktree rather than an outer or calling checkout.
unset GIT_DIR GIT_WORK_TREE GIT_IMPLICIT_WORK_TREE GIT_INDEX_FILE \
    GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES \
    GIT_CEILING_DIRECTORIES GIT_DISCOVERY_ACROSS_FILESYSTEM GIT_NAMESPACE \
    GIT_PREFIX GIT_COMMON_DIR GIT_SHALLOW_FILE GIT_GRAFT_FILE \
    GIT_REPLACE_REF_BASE GIT_NO_REPLACE_OBJECTS

fail() {
  printf 'latest-main: %s\n' "$1" >&2
  exit 2
}

command -v git >/dev/null 2>&1 || fail "git command not found"

target_root="$PWD"
read_only=0

while [ $# -gt 0 ]; do
  case "$1" in
    --root)
      [ $# -ge 2 ] || fail "missing argument for --root"
      target_root="$2"
      shift 2
      ;;
    --read-only)
      read_only=1
      shift
      ;;
    *)
      fail "unknown argument: $1"
      ;;
  esac
done

[ -d "$target_root" ] || fail "target directory does not exist: $target_root"

root="$(git -C "$target_root" rev-parse --show-toplevel 2>/dev/null)" || fail "not inside a git work tree: $target_root"
[ "$(git -C "$root" rev-parse --is-inside-work-tree 2>/dev/null)" = "true" ] || fail "target is not a git work tree: $root"

git -C "$root" remote get-url origin >/dev/null 2>&1 || fail "remote origin not configured in $root"

if [ "$read_only" -eq 1 ]; then
  raw_output="$(git -C "$root" ls-remote --exit-code --refs origin refs/heads/main 2>/dev/null)" || fail "remote ref refs/heads/main not found on origin (read-only query)"
  
  # Ensure exactly one non-empty line
  non_empty="$(printf '%s\n' "$raw_output" | sed '/^[[:space:]]*$/d')"
  line_count="$(printf '%s\n' "$non_empty" | wc -l)"
  [ "$line_count" -eq 1 ] || fail "expected exactly one ref match for refs/heads/main, got $line_count"

  col_count="$(printf '%s\n' "$non_empty" | awk '{print NF}')"
  [ "$col_count" -eq 2 ] || fail "expected exactly two columns in ref match, got $col_count"

  sha="$(printf '%s\n' "$non_empty" | awk '{print $1}')"
  ref="$(printf '%s\n' "$non_empty" | awk '{print $2}')"

  [ "$ref" = "refs/heads/main" ] || fail "unexpected ref match: $ref"
  if ! printf '%s' "$sha" | grep -Eq '^([0-9a-f]{40}|[0-9a-f]{64})$'; then
    fail "malformed object id: $sha"
  fi

  printf 'latest-main: advertised only; not fetched: %s\n' "$sha" >&2
  printf '%s\n' "$sha"
  exit 0
fi

# Normal mode: fetch origin/main into refs/remotes/origin/main
if ! git -C "$root" fetch --no-tags origin +refs/heads/main:refs/remotes/origin/main >&2; then
  fail "failed to fetch origin/main into $root"
fi

sha="$(git -C "$root" rev-parse --verify --quiet "refs/remotes/origin/main^{commit}" 2>/dev/null)" || fail "failed to resolve refs/remotes/origin/main commit object"

if ! printf '%s' "$sha" | grep -Eq '^([0-9a-f]{40}|[0-9a-f]{64})$'; then
  fail "malformed commit id: $sha"
fi

printf 'latest-main: fetched: %s\n' "$sha" >&2
printf '%s\n' "$sha"
exit 0

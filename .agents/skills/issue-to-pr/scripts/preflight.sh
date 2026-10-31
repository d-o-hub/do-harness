#!/usr/bin/env bash
# preflight.sh — refuse to start an issue-to-PR run that cannot produce
# verifiable PRs: unauthenticated gh, no git work tree, no harness binary.
set -euo pipefail

dry_run=0
while [ $# -gt 0 ]; do
  case "$1" in
    --dry-run)
      dry_run=1
      shift
      ;;
    *)
      printf 'preflight: unknown argument: %s\n' "$1" >&2
      exit 2
      ;;
  esac
done

fail() {
  printf 'preflight: %s\n' "$1" >&2
  exit 1
}

root="$(git rev-parse --show-toplevel 2>/dev/null)" || {
  printf 'preflight: not inside a git work tree\n' >&2
  exit 2
}

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
latest_main="$script_dir/../../harness/scripts/latest-main.sh"
[ -f "$latest_main" ] || {
  printf 'preflight: latest-main helper not found at %s\n' "$latest_main" >&2
  exit 2
}

if [ "$dry_run" -eq 1 ]; then
  bash "$latest_main" --root "$root" --read-only >/dev/null || exit 2
else
  bash "$latest_main" --root "$root" >/dev/null || exit 2
fi

command -v gh >/dev/null 2>&1 || fail "gh is not installed"
gh auth status >/dev/null 2>&1 || fail "gh is not authenticated"

if [ -n "${DO_HARNESS_BIN:-}" ]; then
  [ -x "${DO_HARNESS_BIN}" ] || fail "DO_HARNESS_BIN=${DO_HARNESS_BIN} is not executable"
elif ! command -v do-harness >/dev/null 2>&1; then
  fail "do-harness not found on PATH and DO_HARNESS_BIN is unset"
fi

printf 'preflight OK: gh authenticated, git work tree, harness binary present\n'

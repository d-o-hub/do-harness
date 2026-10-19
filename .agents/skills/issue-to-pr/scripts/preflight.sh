#!/usr/bin/env bash
# preflight.sh — refuse to start an issue-to-PR run that cannot produce
# verifiable PRs: unauthenticated gh, no git work tree, no harness binary.
set -euo pipefail

fail() {
  printf 'preflight: %s\n' "$1" >&2
  exit 1
}

command -v gh >/dev/null 2>&1 || fail "gh is not installed"
gh auth status >/dev/null 2>&1 || fail "gh is not authenticated"
git rev-parse --show-toplevel >/dev/null 2>&1 || fail "not inside a git work tree"

if [ -n "${DO_HARNESS_BIN:-}" ]; then
  [ -x "${DO_HARNESS_BIN}" ] || fail "DO_HARNESS_BIN=${DO_HARNESS_BIN} is not executable"
elif ! command -v do-harness >/dev/null 2>&1; then
  fail "do-harness not found on PATH and DO_HARNESS_BIN is unset"
fi

printf 'preflight OK: gh authenticated, git work tree, harness binary present\n'

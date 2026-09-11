#!/usr/bin/env bash
# Preflight checks for a pr-triage sweep.
set -euo pipefail

fail=0
ok() { printf 'ok: %s\n' "$1"; }
warn() { printf 'warn: %s\n' "$1"; }
bad() { printf 'fail: %s\n' "$1" >&2; fail=1; }

if command -v gh >/dev/null 2>&1; then
  ok "$(gh --version | head -n1)"
else
  bad "gh CLI not found"
  exit 1
fi

if command -v git >/dev/null 2>&1; then
  ok "$(git --version)"
else
  bad "git not found"
  exit 1
fi

if gh auth status >/dev/null 2>&1; then
  ok "gh authenticated"
else
  bad "gh is not authenticated (run: gh auth login)"
fi

if repo=$(gh repo view --json nameWithOwner --jq .nameWithOwner 2>/dev/null); then
  ok "repository $repo"
else
  bad "not inside a GitHub repository"
fi

if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  ok "git work tree"
else
  bad "not inside a git work tree"
fi

if [ "$(git rev-parse --is-shallow-repository 2>/dev/null)" = "true" ]; then
  warn "shallow clone: run 'git fetch --unshallow'; scripts fall back to the compare API"
else
  ok "full clone"
fi

if command -v do-harness >/dev/null 2>&1 && do-harness pr review --help >/dev/null 2>&1; then
  ok "do-harness pr review available"
else
  warn "do-harness not available; falling back to full-diff review"
fi

if [ "$fail" -ne 0 ]; then
  printf 'preflight FAILED\n' >&2
  exit 1
fi
printf 'preflight passed\n'

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

harness_bin="${DO_HARNESS_BIN:-}"
if [ -z "$harness_bin" ] && command -v do-harness >/dev/null 2>&1; then
  harness_bin="$(command -v do-harness)"
fi
if [ -n "$harness_bin" ] && "$harness_bin" pr review --help >/dev/null 2>&1; then
  ok "do-harness pr review available ($harness_bin)"
else
  warn "do-harness not available; falling back to full-diff review"
fi

# Webhook fast path (warn-only): the wait scripts work without it, polling.
if command -v node >/dev/null 2>&1; then
  ok "node $(node --version)"
else
  warn "node not found; webhook fast path unavailable"
fi

if command -v curl >/dev/null 2>&1; then
  ok "$(curl --version | head -n1)"
else
  warn "curl not found; webhook fast path unavailable"
fi

if gh webhook forward --help >/dev/null 2>&1; then
  ok "gh webhook forward available"
else
  warn "gh webhook forward unavailable (install with: gh extension install cli/gh-webhook)"
fi

if [ -n "${PR_TRIAGE_EVENTS_URL:-}" ]; then
  if command -v curl >/dev/null 2>&1 &&
    [ "$(curl -s --max-time 2 "$PR_TRIAGE_EVENTS_URL/health" 2>/dev/null || true)" = "ok" ]; then
    ok "webhook receiver reachable ($PR_TRIAGE_EVENTS_URL)"
  else
    warn "webhook receiver unreachable ($PR_TRIAGE_EVENTS_URL); waits will poll"
  fi
fi

if [ "$fail" -ne 0 ]; then
  printf 'preflight FAILED\n' >&2
  exit 1
fi
printf 'preflight passed\n'

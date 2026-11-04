#!/usr/bin/env bash
# Preflight checks for a pr-triage sweep.
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
if [ -z "$harness_bin" ]; then
  repo_root="$(git rev-parse --show-toplevel 2>/dev/null || true)"
  for candidate in "$repo_root/target/release/do-harness" \
    "$repo_root/target/release/do-harness.exe"; do
    if [ -x "$candidate" ]; then
      harness_bin="$candidate"
      break
    fi
  done
fi
# CI classification is a hard dependency: `checks.sh` and the pre-merge gate
# call `do-harness pr checks` and refuse to merge without it. Fail here, before
# any review work, instead of stopping mid-sweep at the check step.
if [ -n "$harness_bin" ] && "$harness_bin" pr checks --help >/dev/null 2>&1; then
  ok "do-harness pr checks available ($harness_bin)"
  if "$harness_bin" pr review --help >/dev/null 2>&1; then
    ok "do-harness pr review available (optional token reducer)"
  else
    warn "do-harness pr review unavailable; review falls back to the full diff"
  fi
else
  bad "do-harness pr checks not found; CI cannot be classified and the sweep must not merge (install do-harness or set DO_HARNESS_BIN)"
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

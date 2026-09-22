#!/usr/bin/env bash
# Detect (and optionally disarm) a pre-armed auto-merge request on a PR.
#
# Usage: auto-merge.sh <pr-number> [--disable]
# Output (TSV): <ARMED|NONE|DISABLED><TAB><mergeMethod|-><TAB><enabledBy|-><TAB><enabledAt|->
# Summary line: summary<TAB><ARMED|NONE|DISABLED>
# Exit: 0 nothing armed, or the request is disarmed; 1 armed (with --disable:
# disarm failed or the request survived); 2 cannot read the PR; 3 usage error.
#
# Why the sweep cares: an armed auto-merge request merges whatever head exists
# when GitHub next computes mergeability, so it lands a commit the sweep never
# validated and bypasses the --match-head-commit pin. Disarm it before
# validating, never rely on it, and halt if it cannot be disarmed.
set -euo pipefail

pr="${1:?usage: auto-merge.sh <pr-number> [--disable]}"
shift || true

disable=0
while [ "$#" -gt 0 ]; do
  case "$1" in
    --disable) disable=1 ;;
    *)
      printf 'auto-merge.sh: unknown argument: %s (see header)\n' "$1" >&2
      exit 3
      ;;
  esac
  shift
done

case "$pr" in
  '' | *[!0-9]*)
    printf 'auto-merge.sh: PR must be a number, got %s\n' "$pr" >&2
    exit 3
    ;;
esac

read_request() {
  local json
  json=$(gh pr view "$1" --json autoMergeRequest 2>/dev/null) || return 1
  printf '%s' "$json" | jq -c '.autoMergeRequest'
}

request=$(read_request "$pr") || {
  printf 'auto-merge.sh: cannot read PR %s\n' "$pr" >&2
  exit 2
}

if [ "$request" = "null" ] || [ -z "$request" ]; then
  printf 'NONE\t-\t-\t-\n'
  printf 'summary\tNONE\n'
  exit 0
fi

method=$(printf '%s' "$request" | jq -r '.mergeMethod // "-"')
actor=$(printf '%s' "$request" | jq -r '.enabledBy.login // "-"')
enabled_at=$(printf '%s' "$request" | jq -r '.enabledAt // "-"')

if [ "$disable" -eq 0 ]; then
  printf 'ARMED\t%s\t%s\t%s\n' "$method" "$actor" "$enabled_at"
  printf 'summary\tARMED\n'
  exit 1
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if ! "$script_dir/retry.sh" --attempts 5 --delay 15 -- gh pr merge "$pr" --disable-auto >/dev/null 2>&1; then
  printf 'auto-merge.sh: could not disarm auto-merge on PR %s\n' "$pr" >&2
  exit 1
fi

request=$(read_request "$pr") || {
  printf 'auto-merge.sh: cannot re-read PR %s after disarming\n' "$pr" >&2
  exit 2
}
if [ "$request" != "null" ] && [ -n "$request" ]; then
  printf 'auto-merge.sh: PR %s still has an armed auto-merge request after --disable-auto\n' "$pr" >&2
  exit 1
fi

printf 'DISABLED\t%s\t%s\t%s\n' "$method" "$actor" "$enabled_at"
printf 'summary\tDISABLED\n'
exit 0

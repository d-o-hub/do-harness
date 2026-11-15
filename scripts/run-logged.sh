#!/usr/bin/env bash
# run-logged.sh — run a command with its output captured inside the workspace.
#
# Why: a command the harness detaches runs in its own shell. A log it writes
# under /tmp is not readable from a later tool call or by another agent, while
# the same write inside the workspace is. Observed on 2026-11-15: a detached
# merge loop's /tmp log did not exist for the next call, and the identical
# write under .do-harness/logs/ did.
#
# Usage:
#   bash scripts/run-logged.sh <name> -- <command...>
#
# Writes .do-harness/logs/<name>.log (gitignored local state), prints the path
# on stderr, and exits with the command's own status. Output still reaches
# stdout, so a foreground run reads exactly as before.
set -euo pipefail

# shellcheck disable=SC1091 # resolved at runtime from this script's directory
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

usage() {
    echo "usage: run-logged.sh <name> -- <command...>" >&2
    exit 2
}

[ "$#" -ge 1 ] || usage
name=$1
shift
[ "${1:-}" = "--" ] && shift
[ "$#" -ge 1 ] || usage
# A name becomes a file stem: no separators, no traversal, no spaces.
case "$name" in
    '' | *[!A-Za-z0-9._-]*) usage ;;
esac

root="$(dh_repo_root)"
dir="$root/.do-harness/logs"
mkdir -p "$dir"
log="$dir/$name.log"
echo "run-logged: $log" >&2

set +e
"$@" 2>&1 | tee "$log"
status=${PIPESTATUS[0]}
set -e
echo "run-logged: exit $status (log $log)" >&2
exit "$status"

#!/usr/bin/env bash
# check-libsql-watch.sh — informational libSQL upstream-issue watch (not a verify sensor).
#
# The harness vendors libSQL at the 0.9.30 line (plans/libsql-distribution.md).
# Upstream released 0.10.0 on 2026-10-13; until the rebase decision lands, this
# watch reports whether the soundness/durability issues that affect the 0.9.30
# base are still open:
#   #2254  corruption on power loss
#   #2271  bundled SQLite predates the 3.51.3 WAL-reset fix (silent write loss)
#   #2272  Connection::authorizer keeps a raw pointer to the connection (heap UAF)
#   #2285  Row::get_value truncates embedded NUL text and panics on malformed UTF-8
#
# Verdicts:
#   VERDICT=OPEN     at least one tracked issue is still open (rebase still due)
#   VERDICT=CLEAR    every tracked issue is closed (the 0.9.30 base is clear)
#   VERDICT=UNKNOWN  network or parse failure (never report a state on error)
#
# Exit 0 on any verdict; exit 2 on usage errors or missing jq/curl.
# Fixture mode (hermetic, no network):
#   check-libsql-watch.sh --fixtures FILE
#   FILE is a JSON array of GitHub issue objects ({"number","state","title"}).
#
# Spike finding: the GitHub REST API returns 403 without a User-Agent header.

set -euo pipefail

USER_AGENT="do-harness-libsql-watch/1.0"
BASE_VERSION="0.9.30"
REPO="tursodatabase/libsql"
REPO_URL="https://api.github.com/repos/$REPO"
TRACKED_ISSUES=(2254 2271 2272 2285)
FIXTURES=""

while (( $# )); do
    case "$1" in
        --fixtures)
            FIXTURES="${2:-}"; shift 2 ;;
        --help|-h)
            # Print the header comment block only. A hard-coded line range
            # silently starts leaking code as soon as the header grows, so stop
            # at the first non-comment line instead.
            awk 'NR == 1 { next } /^#/ { print; next } { exit }' "${BASH_SOURCE[0]}"
            exit 0 ;;
        *)
            echo "check-libsql-watch: unknown argument: $1 (see --help)" >&2; exit 2 ;;
    esac
done

for dep in curl jq; do
    if ! command -v "$dep" >/dev/null 2>&1; then
        echo "check-libsql-watch: missing required command: $dep" >&2
        exit 2
    fi
done

fetch() {
    curl --fail --silent --show-error --max-time 20 \
        --retry 3 --retry-connrefused --retry-delay 1 \
        -A "$USER_AGENT" -H "Accept: application/vnd.github+json" "$1"
}

# The sources are printed, not just used: a watch that queries the wrong
# repository reports a state that has nothing to do with the vendored line, and
# that blindness must be visible in the log.
echo "source: repo=$REPO_URL"
echo "source: issues=${TRACKED_ISSUES[*]}"

unknown() {
    echo "issue: ${1:-unavailable}"
    echo "VERDICT=UNKNOWN"
}

# Normalize every input into one JSON array, then run a single parser over it,
# so fixture and network modes cannot drift apart.
if [[ -n "$FIXTURES" ]]; then
    if [[ ! -f "$FIXTURES" ]]; then
        echo "check-libsql-watch: fixture file not found: $FIXTURES" >&2
        exit 2
    fi
    if ! issues_raw="$(jq -c '.' "$FIXTURES" 2>/dev/null)"; then
        unknown "fixture parse failed"
        exit 0
    fi
else
    collected=()
    for number in "${TRACKED_ISSUES[@]}"; do
        if ! raw="$(fetch "$REPO_URL/issues/$number" 2>/dev/null)"; then
            unknown "fetch failed for #$number"
            exit 0
        fi
        collected+=("$raw")
    done
    if ! issues_raw="$(printf '%s\n' "${collected[@]}" | jq -c -s '.' 2>/dev/null)"; then
        unknown "response parse failed"
        exit 0
    fi
fi

issue_lines=""
if ! issue_lines="$(printf '%s' "$issues_raw" \
    | jq -r '.[] | [.number, (.state // "unknown"), (.title // "")] | @tsv' 2>/dev/null)"; then
    unknown "issue parse failed"
    exit 0
fi

# Zero parsed issues is a failed watch, not a clean bill of health: the fixture
# or the API shape changed and the tracker effectively watched nothing.
if [[ -z "$issue_lines" ]]; then
    unknown "no tracked issues parsed"
    exit 0
fi

open_issues=()
while IFS=$'\t' read -r number state title; do
    [[ -z "$number" ]] && continue
    echo "issue: #$number state=$state title=$title"
    if [[ "$state" == "open" ]]; then
        open_issues+=("$number")
    fi
done <<< "$issue_lines"

if [[ ${#open_issues[@]} -gt 0 ]]; then
    mapfile -t open_sorted < <(printf '%s\n' "${open_issues[@]}" | sort -n)
    open_named=""
    for number in "${open_sorted[@]}"; do
        open_named+="#$number "
    done
    open_named="${open_named% }"
    echo "summary: still-open issues affecting the $BASE_VERSION base: $open_named"
    echo "VERDICT=OPEN"
    exit 0
fi

echo "summary: no tracked issue is open; the $BASE_VERSION base no longer carries them"
echo "VERDICT=CLEAR"
exit 0

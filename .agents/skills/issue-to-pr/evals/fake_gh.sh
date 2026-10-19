#!/usr/bin/env bash
# Deterministic `gh` stub for the issue-to-pr walkthrough.
#
# Supports only what the skill scripts issue: auth status, issue list / issue
# view, pr list, and the REST pull-request create. Fixtures come from
# FAKE_GH_FIXTURES; created PRs append to FAKE_GH_CREATED (JSONL) so PR-list
# reads observe them; every invocation appends its argv to FAKE_GH_LOG when
# set. FAKE_GH_AUTH=fail simulates an unauthenticated gh.
set -euo pipefail

fixtures="${FAKE_GH_FIXTURES:?FAKE_GH_FIXTURES required}"
created="${FAKE_GH_CREATED:-}"
issues="$fixtures/issues.json"
prs="$fixtures/prs.json"

if [ -n "${FAKE_GH_LOG:-}" ]; then
  printf 'gh %s\n' "$*" >> "$FAKE_GH_LOG"
fi

# opt_value prints the value that follows a named flag.
opt_value() {
  local flag="$1"
  shift
  local prev=""
  local arg
  for arg in "$@"; do
    if [ "$prev" = "$flag" ]; then
      printf '%s' "$arg"
      return 0
    fi
    prev="$arg"
  done
  return 1
}

cmd="${1:-}"
shift || true

case "$cmd" in
  auth)
    sub="${1:-}"
    if [ "$sub" != "status" ]; then
      printf 'fake gh: unsupported auth subcommand %s\n' "$sub" >&2
      exit 1
    fi
    if [ "${FAKE_GH_AUTH:-ok}" = "fail" ]; then
      printf 'gh: not logged in to any GitHub hosts\n' >&2
      exit 1
    fi
    printf 'github.com\n  Logged in to github.com account eval\n'
    ;;

  issue)
    sub="${1:-}"
    shift || true
    case "$sub" in
      list)
        jq '.' "$issues"
        ;;
      view)
        n="${1:-}"
        found="$(jq -c --argjson n "$n" '[.[] | select(.number == $n)] | .[0] // empty' "$issues")"
        if [ -z "$found" ]; then
          printf 'fake gh: issue #%s not found\n' "$n" >&2
          exit 1
        fi
        printf '%s\n' "$found"
        ;;
      *)
        printf 'fake gh: unsupported issue subcommand %s\n' "$sub" >&2
        exit 1
        ;;
    esac
    ;;

  pr)
    sub="${1:-}"
    shift || true
    case "$sub" in
      list)
        head="$(opt_value --head "$@" || true)"
        extra='[]'
        if [ -n "$created" ] && [ -s "$created" ]; then
          extra="$(jq -s '.' "$created")"
        fi
        jq -n --slurpfile base "$prs" --argjson extra "$extra" --arg head "$head" '
          ($base[0] + $extra)
          | map(select($head == "" or .headRefName == $head))
          | map({
              number: .number,
              headRefName: .headRefName,
              body: (.body // ""),
              state: (.state // "OPEN")
            })
        '
        ;;
      *)
        printf 'fake gh: unsupported pr subcommand %s\n' "$sub" >&2
        exit 1
        ;;
    esac
    ;;

  api)
    [ -n "$created" ] || {
      printf 'fake gh: FAKE_GH_CREATED is required for api calls\n' >&2
      exit 1
    }
    args=("$@")
    endpoint=""
    method=""
    title=""
    head=""
    base=""
    body=""
    i=0
    while [ "$i" -lt "${#args[@]}" ]; do
      arg="${args[$i]}"
      case "$arg" in
        -X)
          i=$((i + 1))
          method="${args[$i]:-}"
          i=$((i + 1))
          ;;
        -f | -F)
          i=$((i + 1))
          pair="${args[$i]:-}"
          key="${pair%%=*}"
          value="${pair#*=}"
          case "$key" in
            title) title="$value" ;;
            head) head="$value" ;;
            base) base="$value" ;;
            body) body="$value" ;;
          esac
          i=$((i + 1))
          ;;
        -*)
          i=$((i + 1))
          ;;
        *)
          endpoint="$arg"
          i=$((i + 1))
          ;;
      esac
    done
    if [ "$method" != "POST" ] || [ "$endpoint" != "repos/{owner}/{repo}/pulls" ]; then
      printf 'fake gh: unsupported api call: %s %s\n' "$method" "$endpoint" >&2
      exit 1
    fi
    lines=0
    if [ -s "$created" ]; then
      lines="$(wc -l < "$created" | tr -d ' ')"
    fi
    number=$((101 + lines))
    jq -cn --argjson n "$number" --arg head "$head" --arg title "$title" \
      --arg body "$body" --arg base "$base" \
      '{number: $n, headRefName: $head, title: $title, body: $body, baseRefName: $base, state: "OPEN"}' \
      >> "$created"
    jq -cn --argjson n "$number" \
      '{number: $n, html_url: ("https://example.invalid/pull/" + ($n | tostring))}'
    ;;

  *)
    printf 'fake gh: unsupported command %s\n' "$cmd" >&2
    exit 1
    ;;
esac

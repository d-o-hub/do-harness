#!/usr/bin/env bash
# Deterministic `gh` stub for the pr-triage walkthrough.
#
# Covers only the commands the skill scripts issue:
#   pr list / pr view, repo view, api check-runs / status / graphql / compare.
# Fixtures come from FAKE_GH_FIXTURES; check-run mode from FAKE_GH_CHECKS.
set -euo pipefail

fixtures="${FAKE_GH_FIXTURES:?FAKE_GH_FIXTURES required}"

# jq_arg prints the value that follows --jq in the argument list.
jq_arg() {
  local prev=""
  local arg
  for arg in "$@"; do
    if [ "$prev" = "--jq" ]; then
      printf '%s' "$arg"
      return 0
    fi
    prev="$arg"
  done
  return 1
}

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

# kv_value prints the value of a `key=value` argument.
kv_value() {
  local key="$1"
  shift
  local arg
  for arg in "$@"; do
    case "$arg" in
      "$key"=*)
        printf '%s' "${arg#"$key"=}"
        return 0
        ;;
    esac
  done
  return 1
}

cmd="${1:-}"
shift || true

case "$cmd" in
  pr)
    sub="${1:-}"
    shift || true
    case "$sub" in
      list)
        expr=$(jq_arg "$@") || { echo "fake gh: pr list requires --jq" >&2; exit 1; }
        jq -r "$expr" "$fixtures/pr-list.json"
        ;;
      view)
        pr="${1:-}"
        shift || true
        fields=$(opt_value --json "$@") || fields=""
        sha=$(git rev-parse "refs/pull/$pr/head")
        case "$fields" in
          *baseRefName*) printf 'main\t%s\n' "$sha" ;;
          *headRefOid*) printf '%s\n' "$sha" ;;
          *) printf '{"number":%s,"baseRefName":"main","headRefOid":"%s"}\n' "$pr" "$sha" ;;
        esac
        ;;
      *)
        echo "fake gh: unsupported pr subcommand '$sub'" >&2
        exit 1
        ;;
    esac
    ;;

  repo)
    sub="${1:-}"
    shift || true
    if [ "$sub" != "view" ]; then
      echo "fake gh: unsupported repo subcommand '$sub'" >&2
      exit 1
    fi
    fields=$(opt_value --json "$@") || fields=""
    case "$fields" in
      *owner*) printf 'owner\n' ;;
      *name*) printf 'repo\n' ;;
      *) printf '{"owner":{"login":"owner"},"name":"repo"}\n' ;;
    esac
    ;;

  api)
    endpoint="${1:-}"
    shift || true
    expr=$(jq_arg "$@") || { echo "fake gh: api requires --jq" >&2; exit 1; }
    case "$endpoint" in
      graphql)
        query=$(kv_value query "$@") || query=""
        if printf '%s' "$query" | grep -q 'resolveReviewThread'; then
          thread_id=$(kv_value threadId "$@") || thread_id=""
          printf 'resolved=true id=%s\n' "$thread_id"
        else
          jq -r "$expr" "$fixtures/threads.json"
        fi
        ;;
      *"/check-runs"*)
        mode="${FAKE_GH_CHECKS:-pass}"
        jq -r "$expr" "$fixtures/check-runs-$mode.json"
        ;;
      *"/status"*)
        jq -r "$expr" "$fixtures/statuses.json"
        ;;
      *"/compare/"*)
        refs="${endpoint#*/compare/}"
        base="${refs%%...*}"
        head="${refs##*...}"
        mb=$(git merge-base "$base" "$head")
        git diff --name-only "$mb" "$head" | wc -l
        ;;
      *)
        echo "fake gh: unsupported endpoint '$endpoint'" >&2
        exit 1
        ;;
    esac
    ;;

  *)
    echo "fake gh: unsupported command '$cmd'" >&2
    exit 1
    ;;
esac

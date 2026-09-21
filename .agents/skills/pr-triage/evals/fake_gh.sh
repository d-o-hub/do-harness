#!/usr/bin/env bash
# Deterministic `gh` stub for the pr-triage walkthrough.
#
# Covers only the commands the skill scripts issue:
#   pr list / pr view / pr merge --disable-auto, repo view, api check-runs /
#   status / graphql / compare.
# Fixtures come from FAKE_GH_FIXTURES; check-run mode from FAKE_GH_CHECKS, and
# the auto-merge request from FAKE_GH_AUTO_MERGE (armed|none) with disarms
# recorded in FAKE_GH_AUTO_MERGE_LOG.
# FAKE_GH_CHECKS_SEQUENCE (comma list, e.g. "pending,pass") advances one mode
# per check-runs call and clamps at the last entry; its cursor lives in
# FAKE_GH_SEQ_FILE (default: a file under ${TMPDIR:-/tmp}).
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
          *autoMergeRequest*)
            mode="${FAKE_GH_AUTO_MERGE:-none}"
            log="${FAKE_GH_AUTO_MERGE_LOG:-}"
            if [ "$mode" = "armed" ] && [ -n "$log" ] && grep -qs "^disable-auto $pr$" "$log"; then
              mode=none
            fi
            if [ "$mode" = "armed" ]; then
              printf '{"autoMergeRequest":{"enabledAt":"2026-01-01T00:00:00Z","enabledBy":{"login":"owner"},"mergeMethod":"SQUASH","authorEmail":"owner@example.com","commitBody":"body","commitHeadline":"headline"}}\n'
            else
              printf '{"autoMergeRequest":null}\n'
            fi
            ;;
          *mergeCommit*) printf '%s\n' "$sha" ;;
          *baseRefName*) printf 'main\t%s\n' "$sha" ;;
          *headRefOid*) printf '%s\n' "$sha" ;;
          *) printf '{"number":%s,"baseRefName":"main","headRefOid":"%s"}\n' "$pr" "$sha" ;;
        esac
        ;;
      merge)
        pr="${1:-}"
        shift || true
        if [ "${1:-}" != "--disable-auto" ]; then
          echo "fake gh: only 'pr merge <n> --disable-auto' is supported" >&2
          exit 1
        fi
        if [ -n "${FAKE_GH_AUTO_MERGE_LOG:-}" ]; then
          printf 'disable-auto %s\n' "$pr" >> "$FAKE_GH_AUTO_MERGE_LOG"
        fi
        printf 'Disabled auto-merge for pull request #%s\n' "$pr"
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

  run)
    sub="${1:-}"
    shift || true
    if [ "$sub" != "list" ]; then
      echo "fake gh: unsupported run subcommand '$sub'" >&2
      exit 1
    fi
    expr=$(jq_arg "$@") || { echo "fake gh: run list requires --jq" >&2; exit 1; }
    mode="${FAKE_GH_RUNS:-pass}"
    if [ "$mode" = "none" ]; then
      exit 0
    fi
    jq -r "$expr" "$fixtures/runs-$mode.json"
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
        if [ -n "${FAKE_GH_CHECKS_SEQUENCE:-}" ]; then
          seq_file="${FAKE_GH_SEQ_FILE:-${TMPDIR:-/tmp}/fake-gh-checks-sequence.state}"
          step=$(cat "$seq_file" 2>/dev/null || echo 0)
          case "$step" in ''|*[!0-9]*) step=0 ;; esac
          mode=$(awk -F, -v n="$((step + 1))" '{ if (n > NF) n = NF; print $n }' \
            <<<"$FAKE_GH_CHECKS_SEQUENCE")
          if [ "$((step + 1))" -lt "$(awk -F, '{ print NF }' <<<"$FAKE_GH_CHECKS_SEQUENCE")" ]; then
            printf '%s\n' "$((step + 1))" > "$seq_file"
          fi
        fi
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

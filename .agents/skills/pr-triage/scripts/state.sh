#!/usr/bin/env bash
# Sweep state stored inside the git dir (never committed).
#
# Usage:
#   state.sh get <pr>                                   print last record (TSV) or nothing
#   state.sh set <pr> <head_sha> <base_sha> <decision>  upsert a record
#   state.sh clear <pr>                                 remove a record
#   state.sh path                                       print the state file path
set -euo pipefail

git_dir=$(git rev-parse --absolute-git-dir 2>/dev/null) || {
  printf 'not a git repository\n' >&2
  exit 3
}
dir="$git_dir/pr-triage"
file="$dir/state.tsv"

cmd="${1:?usage: state.sh <get|set|clear|path> ...}"

case "$cmd" in
  path)
    printf '%s\n' "$file"
    ;;

  get)
    pr="${2:?usage: state.sh get <pr-number>}"
    if [ -f "$file" ]; then
      awk -F'\t' -v p="$pr" '$1 == p { line=$0 } END { if (line != "") print line }' "$file"
    fi
    ;;

  set)
    pr="${2:?usage: state.sh set <pr> <head> <base> <decision>}"
    head="${3:?usage: state.sh set <pr> <head> <base> <decision>}"
    base="${4:?usage: state.sh set <pr> <head> <base> <decision>}"
    decision="${5:?usage: state.sh set <pr> <head> <base> <decision>}"
    mkdir -p "$dir"
    ts=$(date -u +%Y-%m-%dT%H:%M:%SZ)
    tmp="$file.tmp"
    if [ -f "$file" ]; then
      awk -F'\t' -v p="$pr" '$1 != p' "$file" > "$tmp"
    else
      : > "$tmp"
    fi
    printf '%s\t%s\t%s\t%s\t%s\n' "$pr" "$head" "$base" "$decision" "$ts" >> "$tmp"
    mv "$tmp" "$file"
    printf 'recorded %s %s\n' "$pr" "$decision"
    ;;

  clear)
    pr="${2:?usage: state.sh clear <pr-number>}"
    if [ -f "$file" ]; then
      tmp="$file.tmp"
      awk -F'\t' -v p="$pr" '$1 != p' "$file" > "$tmp"
      mv "$tmp" "$file"
    fi
    ;;

  *)
    printf 'usage: state.sh <get|set|clear|path> ...\n' >&2
    exit 2
    ;;
esac

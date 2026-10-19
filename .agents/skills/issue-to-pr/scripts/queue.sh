#!/usr/bin/env bash
# queue.sh — print the ordered issue-to-PR work queue.
#
# stdout: one TSV row per actionable item: kind, id, rank, title.
# stderr: one line per skipped item with its reason, plus a summary line.
# Ranking and exclusion rules: references/queue.md.
#
# The jq below deliberately avoids capture()/try and non-boolean select
# conditions: jaq and jq differ on both, and the queue must behave identically
# under either implementation.
set -euo pipefail

command -v gh >/dev/null 2>&1 || { printf 'queue.sh: gh is not installed\n' >&2; exit 2; }
command -v jq >/dev/null 2>&1 || { printf 'queue.sh: jq is not installed\n' >&2; exit 2; }

root="$(git rev-parse --show-toplevel 2>/dev/null)" || {
  printf 'queue.sh: not inside a git work tree\n' >&2
  exit 2
}
cd "$root"

sel_kind=""
sel_id=""
while [ $# -gt 0 ]; do
  case "$1" in
    --issue)
      sel_kind=issue
      sel_id="${2:?queue.sh: --issue needs a number}"
      shift 2
      ;;
    --task)
      sel_kind=task
      sel_id="${2:?queue.sh: --task needs an id}"
      shift 2
      ;;
    *)
      printf 'queue.sh: unknown argument: %s\n' "$1" >&2
      exit 2
      ;;
  esac
done

issues_json="$(gh issue list --state open --json number,title,labels,updatedAt --limit 200)"
prs_json="$(gh pr list --state open --json number,headRefName,body --limit 200)"

bin=""
if [ -n "${DO_HARNESS_BIN:-}" ] && [ -x "${DO_HARNESS_BIN}" ]; then
  bin="${DO_HARNESS_BIN}"
elif command -v do-harness >/dev/null 2>&1; then
  bin="$(command -v do-harness)"
fi

tasks_json='{"tasks":[]}'
if [ -n "$bin" ]; then
  if ! tasks_json="$("$bin" --root "$root" task list --format json 2>/dev/null)"; then
    printf 'queue.sh: task state unavailable (task list failed); continuing with issues only\n' >&2
    tasks_json='{"tasks":[]}'
  elif ! printf '%s' "$tasks_json" | jq -e . >/dev/null 2>&1; then
    printf 'queue.sh: task state unavailable (malformed task list output); continuing with issues only\n' >&2
    tasks_json='{"tasks":[]}'
  fi
else
  printf 'queue.sh: task state unavailable (no do-harness binary); continuing with issues only\n' >&2
fi

report="$(jq -n --argjson issues "$issues_json" --argjson prs "$prs_json" --argjson tasks "$tasks_json" '
  def priority_of($i):
    ([$i.labels[] | .name] | map(select(. == "P0" or . == "P1" or . == "P2" or . == "P3")) | .[0]) as $label
    | ($i.title
       | if (startswith("[P0]") or startswith("[P1]") or startswith("[P2]") or startswith("[P3]"))
         then (split("]") | .[0] | ltrimstr("["))
         else null end) as $prefix
    | ($label // $prefix);

  def rank_of($i):
    priority_of($i) as $p
    | if ($p == null) then 4 else ($p[1:2] | tonumber) end;

  def skip_reason($i):
    ([$i.labels[] | .name]
      | map(select(. == "hold" or . == "wip" or . == "do-not-merge"
                   or . == "wontfix" or . == "duplicate" or . == "blocked"))
      | .[0]) as $held
    | ([$prs[] | select(
          (.headRefName | startswith("issue-\($i.number)-"))
          or ((.body // "") | test("(^|[^0-9])Closes #\($i.number)([^0-9]|$)"))
        )] | .[0]) as $cover
    | if ($held != null) then "excluded by label \($held)"
      elif ($cover != null) then "open PR #\($cover.number) covers it"
      else null end;

  {
    rows: [
      ($issues[]
        | select(skip_reason(.) == null)
        | {kind: "issue", id: .number, rank: rank_of(.), title: .title}),
      ($tasks.tasks[]
        | select(.status == "pending")
        | {kind: "task", id: .id, rank: 5, title: .title})
    ],
    skipped: [
      ($issues[]
        | select(skip_reason(.) != null)
        | {kind: "issue", id: .number, title: .title, reason: skip_reason(.)}),
      ($tasks.tasks[]
        | select(.status != "pending")
        | {kind: "task", id: .id, title: .title, reason: "status \(.status)"})
    ]
  }
')"

if [ -n "$sel_kind" ]; then
  printf '%s' "$report" | jq -r --arg k "$sel_kind" --arg i "$sel_id" \
    '.rows[] | select(.kind == $k and ((.id | tostring) == $i)) | [.kind, .id, .rank, .title] | @tsv'
else
  printf '%s' "$report" | jq -r '.rows[] | [.kind, .id, .rank, .title] | @tsv' \
    | sort -t "$(printf '\t')" -k3,3n -k2,2n
fi

printf '%s' "$report" | jq -r '.skipped[] | "queue.sh: skipped \(.kind) #\(.id): \(.reason)"' >&2
printf 'queue.sh: %s actionable item(s), %s skipped\n' \
  "$(printf '%s' "$report" | jq '.rows | length')" \
  "$(printf '%s' "$report" | jq '.skipped | length')" >&2

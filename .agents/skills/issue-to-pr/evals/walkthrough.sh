#!/usr/bin/env bash
# issue-to-pr walkthrough: exercises queue discovery, item start, and PR
# creation against a fake gh and a local git repo with a bare origin. Proves
# ranking and exclusions, refusal paths, dry runs, idempotent PR creation, and
# that issue text stays data. Leaves graded residue under the sandbox root.
set -euo pipefail

root="${DO_HARNESS_ROOT:?DO_HARNESS_ROOT required}"
bin="${DO_HARNESS_BIN:-do-harness}"
skill="$root/.agents/skills/issue-to-pr"

command -v jq >/dev/null 2>&1 || {
  printf 'issue-to-pr walkthrough: jq is required\n' >&2
  exit 1
}

cd "$root"
git init -q -b main
git config user.email "eval@example.com"
git config user.name "Eval"

# Residue and test scaffolding live beside the tree: keep them out of the
# cleanliness checks the skill scripts enforce.
cat >> "$root/.git/info/exclude" <<'EOF'
/bin/
/origin.git/
/queue.tsv
/queue.err
/queue_order.txt
/preflight.out
/preflight_fail.txt
/gh_calls.log
/gh_created_prs.jsonl
/start_item.out
/start_branch.txt
/start_dry.txt
/start_dry_state.txt
/start_refusal.txt
/start_dirty_refusal.txt
/open_pr_dry.txt
/dry_pushed_state.txt
/pr_61.txt
/open_pr_second.txt
/created_count.txt
/pushed_branch.txt
/body-61.md
/start_task.out
/start_task_branch.txt
/pr_task.txt
/issue_to_pr_negative.txt
EOF

"$bin" --root "$root" init --minimal >/dev/null
git add -A
git commit -q -m "chore: initial workspace"
git init -q --bare "$root/origin.git"
git remote add origin "$root/origin.git"
git push -q origin main

# Fake gh on PATH; every call is logged for the graded assertions.
mkdir -p "$root/bin"
cp "$skill/evals/fake_gh.sh" "$root/bin/gh"
chmod +x "$root/bin/gh"
export PATH="$root/bin:$PATH"
export FAKE_GH_FIXTURES="$skill/evals/fixtures"
export FAKE_GH_LOG="$root/gh_calls.log"
export FAKE_GH_CREATED="$root/gh_created_prs.jsonl"
: > "$FAKE_GH_LOG"

# Tasks: one pending (queue material) and one failed (excluded).
"$bin" --root "$root" task add "Smoke test the task path" >/dev/null
"$bin" --root "$root" task add "Abandoned experiment" >/dev/null
"$bin" --root "$root" task fail 2 >/dev/null

# 1. Queue: ranking, exclusions, and the issue/task split.
"$skill/scripts/queue.sh" > "$root/queue.tsv" 2> "$root/queue.err"
python3 - "$root/queue.tsv" "$root/queue_order.txt" <<'PY'
import sys

rows = [
    line.split("\t")
    for line in open(sys.argv[1], encoding="utf-8").read().splitlines()
    if line
]
order = [row[1] if row[0] == "issue" else "t" + row[1] for row in rows]
with open(sys.argv[2], "w", encoding="utf-8") as handle:
    handle.write(",".join(order) + "\n")
PY

# 2. Preflight: green run, then an unauthenticated gh behind a named exit.
"$skill/scripts/preflight.sh" > "$root/preflight.out" 2>&1

set +e
FAKE_GH_AUTH=fail "$skill/scripts/preflight.sh" > "$root/preflight_fail.txt" 2>&1
preflight_fail_exit=$?
set -e
printf 'exit=%s\n' "$preflight_fail_exit" >> "$root/preflight_fail.txt"

# 3. Dry run and refusals, all from a clean main.
"$skill/scripts/start-item.sh" --issue 62 --dry-run > "$root/start_dry.txt"
if [ -n "$(git branch --list 'issue-62-*')" ]; then
  printf 'present\n'
else
  printf 'absent\n'
fi > "$root/start_dry_state.txt"

set +e
"$skill/scripts/start-item.sh" --issue 103 > /dev/null 2> "$root/start_refusal.txt"
start_refusal_exit=$?
set -e
printf 'exit=%s\n' "$start_refusal_exit" >> "$root/start_refusal.txt"

printf 'probe\n' > "$root/dirty-probe.txt"
set +e
"$skill/scripts/start-item.sh" --issue 62 > /dev/null 2> "$root/start_dirty_refusal.txt"
start_dirty_exit=$?
set -e
printf 'exit=%s\n' "$start_dirty_exit" >> "$root/start_dirty_refusal.txt"
rm -f "$root/dirty-probe.txt"

# 4. Start the top public issue, implement, dry-run the PR, then open it.
"$skill/scripts/start-item.sh" --issue 61 > "$root/start_item.out"
git branch --show-current > "$root/start_branch.txt"

printf 'fix\n' > "$root/item-61-fix.txt"
git add "$root/item-61-fix.txt"
git commit -q -m "fix: handle empty parser input"

printf 'Fixes the parser crash on empty input.\n\nVerification: sensors green in CI.\n\nCloses #61\n' \
  > "$root/body-61.md"
"$skill/scripts/open-pr.sh" --issue 61 --body-file "$root/body-61.md" --dry-run \
  > "$root/open_pr_dry.txt"
if git --git-dir="$root/origin.git" branch --list 'issue-61-*' | grep -q .; then
  printf 'pushed\n'
else
  printf 'not-pushed\n'
fi > "$root/dry_pushed_state.txt"

"$skill/scripts/open-pr.sh" --issue 61 --body-file "$root/body-61.md" > "$root/pr_61.txt"
git --git-dir="$root/origin.git" branch --list 'issue-61-*' > "$root/pushed_branch.txt"

# 5. Idempotence: a second run reports the PR instead of creating a duplicate.
"$skill/scripts/open-pr.sh" --issue 61 --body-file "$root/body-61.md" \
  > "$root/open_pr_second.txt"
wc -l < "$root/gh_created_prs.jsonl" | tr -d ' ' > "$root/created_count.txt"

# 6. A pending plan task rides the same path.
git checkout -q main
"$skill/scripts/start-item.sh" --task 1 > "$root/start_task.out"
git branch --show-current > "$root/start_task_branch.txt"
printf 'task work\n' > "$root/item-task-1.txt"
git add "$root/item-task-1.txt"
git commit -q -m "chore: exercise the task path"
"$skill/scripts/open-pr.sh" --task 1 > "$root/pr_task.txt"

# 7. Negative case: an out-of-scope question is not queue material.
cat > "$root/issue_to_pr_negative.txt" <<'TXT'
out-of-scope: a general question is not a queue item; no branch, no PR, and no
tracker mutation belong to it.
TXT

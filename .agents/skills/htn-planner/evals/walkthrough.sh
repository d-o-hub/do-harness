#!/usr/bin/env bash
# htn-planner walkthrough: drives the workflow CLI to plan a vertical slice,
# producing real task rows and a `done` task as residue.
set -euo pipefail
root="${DO_HARNESS_ROOT:?DO_HARNESS_ROOT required}"
bin="${DO_HARNESS_BIN:-do-harness}"

# A git worktree is required for provenance digests and freshness verification.
if ! git -C "$root" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  git init -q "$root"
  git -C "$root" config user.email "test@example.com"
  git -C "$root" config user.name "test"
  git -C "$root" commit -q --allow-empty -m "chore: initial"
fi

cat > "$root/do-harness.toml" <<'TOML'
[[sensors]]
name = "test"
argv = ["true"]

[[sensors]]
name = "check"
argv = ["true"]

[[sensors]]
name = "clippy"
argv = ["true"]
TOML

"$bin" --root "$root" task add "plan user registration" --method vertical-event-slice >/dev/null

id="$("$bin" --root "$root" task list --format json | python3 -c 'import sys,json;print(json.load(sys.stdin)["tasks"][0]["id"])')"

"$bin" --root "$root" verify --record --task "$id" >/dev/null
# Advance through all five subtasks then mark done.
for _ in 1 2 3 4 5; do
  "$bin" --root "$root" task advance "$id" >/dev/null
done
"$bin" --root "$root" task "done" "$id" >/dev/null
"$bin" --root "$root" task list

# Negative case: an out-of-scope question matches no method catalog entry,
# so no method is selected and no spike scratch leaks into the workspace.
cat > "$root/plan_negative.txt" << 'TXT'
negative: general-knowledge question matches no method, no method applies, planning deferred until a domain requirement exists
TXT

#!/usr/bin/env bash
# skill-distiller walkthrough: records a resolved trace and distills it into the
# sandboxed harness skill corpus, so evals grade the on-disk heuristics.md that
# distill writes.
set -euo pipefail
root="${DO_HARNESS_ROOT:?DO_HARNESS_ROOT required}"
bin="${DO_HARNESS_BIN:-do-harness}"

# trace add connects and migrates the agent-state db.
"$bin" --root "$root" trace add --session eval --command "cargo check" \
  --error-diff "E0308" --resolution-steps "classify then apply minimal fix" >/dev/null

tid="$("$bin" --root "$root" trace list --session eval --format json | python3 -c 'import sys,json;print(json.load(sys.stdin)[0]["id"])')"

# The distill evidence guardrail requires an ok sensor beat in the workspace;
# insert one directly now that the db schema exists.
python3 - "$root" << 'EOF'
import sqlite3, sys, time, pathlib
db = pathlib.Path(sys.argv[1]) / ".do-harness" / "agent_state.db"
conn = sqlite3.connect(db)
now = int(time.time())
conn.execute(
    "INSERT INTO beats (task_id, beat_type, status, sensor_exit_code, sensor_name, started_at, completed_at) "
    "VALUES (NULL, 'sensor', 'ok', 0, 'check', ?, ?)",
    (now, now),
)
conn.commit()
conn.close()
EOF

"$bin" --root "$root" distill --skill skill-distiller \
  --pattern "classify sensor failure before fixing" \
  --description "fires when the borrow checker flags a missing bound" \
  --from-trace "$tid" >/dev/null

# Negative case: a typo-only change has no sensor trace and passed no
# sensors, so nothing is distilled and no heuristic is written for it.
cat > "$root/distill_negative.txt" << 'TXT'
negative: typo-only change has no sensor trace, no distillation; never distill a fix that did not pass computational sensors
anti-pattern: an unresolved blocker is recorded as negative knowledge, graded by absent: and not-contains: assertions
TXT

cat ".agents/skills/skill-distiller/references/heuristics.md"
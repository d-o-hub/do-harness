#!/usr/bin/env bash
# htn-planner walkthrough: drives the workflow CLI to plan a vertical slice,
# producing real task rows and a `done` task as residue.
set -euo pipefail
root="${DO_HARNESS_ROOT:?DO_HARNESS_ROOT required}"
bin="${DO_HARNESS_BIN:-do-harness}"

"$bin" --root "$root" task add "plan user registration" --method vertical-event-slice >/dev/null

id="$("$bin" --root "$root" task list --format json | python3 -c 'import sys,json;print(json.load(sys.stdin)[0]["id"])')"

# Insert ok beats for the sensors the vertical-event-slice method gates on.
python3 - "$root" "$id" << 'EOF'
import os, sqlite3, sys, time, pathlib
root, tid = sys.argv[1], int(sys.argv[2])
db = pathlib.Path(root) / ".do-harness" / "agent_state.db"
conn = sqlite3.connect(db)
now = int(time.time())
for sensor in ("test", "test", "check", "clippy"):
    conn.execute(
        "INSERT INTO beats (task_id, beat_type, status, sensor_exit_code, sensor_name, started_at, completed_at) "
        "VALUES (?,?,?,?,?,?,?)",
        (tid, "sensor", "ok", 0, sensor, now, now),
    )
conn.commit()
conn.close()
EOF

# Advance through all five subtasks then mark done.
for _ in 1 2 3 4 5; do
  "$bin" --root "$root" task advance "$id" >/dev/null
done
"$bin" --root "$root" task done "$id" >/dev/null
"$bin" --root "$root" task list

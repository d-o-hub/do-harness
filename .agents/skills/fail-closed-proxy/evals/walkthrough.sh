#!/usr/bin/env bash
# fail-closed-proxy walkthrough: leaves hermetic decide-audit-forward residue
# in the eval sandbox. Exit 0 proves the checklist was written.
set -euo pipefail
root="${DO_HARNESS_ROOT:?DO_HARNESS_ROOT required}"
cat > "$root/proxy-checklist.md" << 'MD'
# fail-closed proxy checklist
- fail-closed: deny on invalid params, governance denial, or mediator error; never allow on error.
- order: decide -> count -> audit (best-effort) -> forward on allow only.
- statuses: deny -> 403; upstream send/read failure -> 502.
- audit: hash-chained JSONL allow/deny evidence; append failure counted, decision kept.
- metrics: GET /metrics exposes allow, deny, mediator_errors, upstream_ok, upstream_failures, audit_write_failures; counters never affect routing.
MD
test -s "$root/proxy-checklist.md"

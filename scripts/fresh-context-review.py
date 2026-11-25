#!/usr/bin/env python3
"""Fresh-context design review pilot adapter (explicit opt-in).

Assembles a frozen review packet from a pinned design document and revision,
launches one fresh reviewer process with no parent conversation, and records
a sanitized outcome. Deterministic stub mode exists for smoke tests and
self-tests; no model is launched by default and this script is never wired
into sensors or CI.
"""

from fresh_context_review.runner import main

if __name__ == "__main__":
    raise SystemExit(main())

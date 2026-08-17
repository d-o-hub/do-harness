#!/usr/bin/env bash
# harness walkthrough: exercises the fail-fast recovery path so evals grade the
# real error-signature lifecycle (record -> list -> clear) as residue.
set -euo pipefail
root="${DO_HARNESS_ROOT:?DO_HARNESS_ROOT required}"
bin="${DO_HARNESS_BIN:-do-harness}"

# A failing sensor run records a failed beat and bumps its error signature.
# In an empty sandbox `cargo test --workspace` fails, so the test sensor fails.
"$bin" --root "$root" verify --only test --record >/dev/null 2>&1 || true

"$bin" --root "$root" errors list

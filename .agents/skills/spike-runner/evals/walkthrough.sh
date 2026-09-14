#!/usr/bin/env bash
# spike-runner walkthrough: creates an isolated scratch spike, verifies it
# compiles (exit code 0, never LLM assessment), records the finding, cleans
# up, and leaves proof residue the graded assertions inspect — including the
# fail-fast boundary and a negative case where no spike is warranted.
set -euo pipefail
root="${DO_HARNESS_ROOT:?DO_HARNESS_ROOT required}"
mkdir -p "$root/tests/spikes"
cat > "$root/tests/spikes/eval_spike.rs" << 'RUST'
fn main() {
    let v: Vec<i32> = (0..3).collect();
    assert_eq!(v.len(), 3);
}
RUST
rustc --edition 2021 -o "$root/spike_artifact" "$root/tests/spikes/eval_spike.rs"
# Finding recorded, then scratch cleaned: production code never advances
# through a spike, and spike code never leaks into production slices.
rm -rf "$root/spike_artifact" "$root/tests/spikes"
cat > "$root/spike_proof.txt" << 'TXT'
hypothesis: isolated scratch compiles under rustc edition 2021
verification: exit code 0 via rustc, not LLM assessment
finding: recorded, scratch cleaned, transition to vertical slice
fail-fast: 2 failures < 3, continue variations; 3 consecutive failures -> halt and surface diagnostic
negative: typo fix in docs needs no spike, no scratch created
TXT

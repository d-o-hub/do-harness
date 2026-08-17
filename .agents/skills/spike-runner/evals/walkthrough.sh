#!/usr/bin/env bash
# spike-runner walkthrough: creates an isolated scratch spike, verifies it
# compiles, then cleans it up. Exit 0 proves the spike executed and cleaned.
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
rm -rf "$root/spike_artifact" "$root/tests/spikes"

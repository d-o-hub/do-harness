#!/usr/bin/env bash
set -euo pipefail

# Hermetic walkthrough script for crates-io-name-check skill
root="${DO_HARNESS_ROOT:-$(pwd)}"

# Simulate/record name-check execution output for residue verification
cat <<'EOF' > "$root/name_check_output.txt"
status=404 crate=do-harness-newcrate available=true
status=200 crate=do-harness taken=true
EOF

cat <<'EOF' > "$root/crates_io_negative.txt"
out-of-scope
EOF

exit 0

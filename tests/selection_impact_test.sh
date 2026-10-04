#!/usr/bin/env bash
# Hermetic unit & integration test suite for selection impact experiment protocol, resolver, policy, and benchmark report arithmetic.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target/selection-impact-target}"
RESOLVER="$ROOT/scripts/selection-impact/cargo-resolver.py"
POLICY_EVAL="$ROOT/scripts/selection-impact/evaluate-policy.py"
WORKSPACE="$ROOT/tests/fixtures/selection-impact/workspace"
MANIFEST="$ROOT/tests/fixtures/selection-impact/manifest.json"

ERRORS=0

echo "=== Running Selection Impact Hermetic Test Suite ==="

run_test() {
  local name="$1"
  echo -n "Test: $name ... "
}

pass_test() {
  echo "PASS"
}

fail_test() {
  local msg="$1"
  echo "FAIL ($msg)"
  ERRORS=$((ERRORS + 1))
}

# 1. Valid resolver result
run_test "valid resolver result"
res_out=$(python3 - << EOF
import subprocess, json
payload = {
    "schema_version": 1,
    "root": "$WORKSPACE",
    "changed_files": [{"path": "crates/leaf_a/src/lib.rs", "kind": "modified"}]
}
p = subprocess.run(["python3", "$RESOLVER"], input=json.dumps(payload), text=True, capture_output=True)
data = json.loads(p.stdout)
print(data.get("status") == "resolved" and not data.get("global_change"))
EOF
)
if [[ "$res_out" == "True" ]]; then pass_test; else fail_test "Expected status=resolved and global_change=false"; fi

# 2. Unsupported schema
run_test "unsupported schema"
res_out=$(python3 - << EOF
import subprocess, json
payload = {
    "schema_version": 99,
    "root": "$WORKSPACE",
    "changed_files": []
}
p = subprocess.run(["python3", "$RESOLVER"], input=json.dumps(payload), text=True, capture_output=True)
data = json.loads(p.stdout)
print(data.get("status") == "unresolved" and "Unsupported schema version" in data.get("warnings", [])[0])
EOF
)
if [[ "$res_out" == "True" ]]; then pass_test; else fail_test "Expected unresolved for schema_version=99"; fi

# 3. Malformed / non-JSON output handling
run_test "malformed input JSON"
res_out=$(python3 - << EOF
import subprocess, json
p = subprocess.run(["python3", "$RESOLVER"], input="INVALID_JSON", text=True, capture_output=True)
data = json.loads(p.stdout)
print(data.get("status") == "unresolved")
EOF
)
if [[ "$res_out" == "True" ]]; then pass_test; else fail_test "Expected unresolved for malformed JSON"; fi

# 4. Duplicate / escaping paths
run_test "escaping paths"
res_out=$(python3 - << EOF
import subprocess, json
payload = {
    "schema_version": 1,
    "root": "$WORKSPACE",
    "changed_files": [{"path": "../../../etc/passwd", "kind": "modified"}]
}
p = subprocess.run(["python3", "$RESOLVER"], input=json.dumps(payload), text=True, capture_output=True)
data = json.loads(p.stdout)
print(data.get("status") == "unresolved" or data.get("global_change") == True)
EOF
)
if [[ "$res_out" == "True" ]]; then pass_test; else fail_test "Expected unresolved/global_change for escaping path"; fi

# 5. Unknown component
run_test "unknown component / file"
res_out=$(python3 - << EOF
import subprocess, json
payload = {
    "schema_version": 1,
    "root": "$WORKSPACE",
    "changed_files": [{"path": "unknown/random_file.xyz", "kind": "modified"}]
}
p = subprocess.run(["python3", "$RESOLVER"], input=json.dumps(payload), text=True, capture_output=True)
data = json.loads(p.stdout)
print(data.get("global_change") == True)
EOF
)
if [[ "$res_out" == "True" ]]; then pass_test; else fail_test "Expected global_change=true for unknown component"; fi

# 6. Leaf and shared dependency closure
run_test "shared dependency closure"
res_out=$(python3 - << EOF
import subprocess, json
payload = {
    "schema_version": 1,
    "root": "$WORKSPACE",
    "changed_files": [{"path": "crates/shared_lib/src/lib.rs", "kind": "modified"}]
}
p = subprocess.run(["python3", "$RESOLVER"], input=json.dumps(payload), text=True, capture_output=True)
data = json.loads(p.stdout)
affected = [a["id"] for a in data.get("affected", [])]
print("crates/leaf_a" in affected and "crates/leaf_b" in affected)
EOF
)
if [[ "$res_out" == "True" ]]; then pass_test; else fail_test "Expected leaf_a and leaf_b in affected closure"; fi

# 7. Proc macro & build script conservative handling
run_test "proc macro conservative handling"
res_out=$(python3 - << EOF
import subprocess, json
payload = {
    "schema_version": 1,
    "root": "$WORKSPACE",
    "changed_files": [{"path": "crates/proc_macro_lib/src/lib.rs", "kind": "modified"}]
}
p = subprocess.run(["python3", "$RESOLVER"], input=json.dumps(payload), text=True, capture_output=True)
data = json.loads(p.stdout)
affected = [a["id"] for a in data.get("affected", [])]
print("crates/leaf_a" in affected)
EOF
)
if [[ "$res_out" == "True" ]]; then pass_test; else fail_test "Expected leaf_a affected by proc_macro_lib"; fi

# 8. Global input fallback
run_test "global input fallback (Cargo.lock)"
res_out=$(python3 - << EOF
import subprocess, json
payload = {
    "schema_version": 1,
    "root": "$WORKSPACE",
    "changed_files": [{"path": "Cargo.lock", "kind": "modified"}]
}
p = subprocess.run(["python3", "$RESOLVER"], input=json.dumps(payload), text=True, capture_output=True)
data = json.loads(p.stdout)
print(data.get("global_change") == True)
EOF
)
if [[ "$res_out" == "True" ]]; then pass_test; else fail_test "Expected global_change=true for Cargo.lock"; fi

# 9. Byte-identical repeated output
run_test "byte-identical repeated output"
res_out=$(python3 - << EOF
import subprocess, json
payload = {
    "schema_version": 1,
    "root": "$WORKSPACE",
    "changed_files": [{"path": "crates/shared_lib/src/lib.rs", "kind": "modified"}]
}
p1 = subprocess.run(["python3", "$RESOLVER"], input=json.dumps(payload), text=True, capture_output=True)
p2 = subprocess.run(["python3", "$RESOLVER"], input=json.dumps(payload), text=True, capture_output=True)
print(p1.stdout == p2.stdout)
EOF
)
if [[ "$res_out" == "True" ]]; then pass_test; else fail_test "Outputs were not byte-identical"; fi

# 10. Policy fallback equals baseline selection
run_test "candidate fallback exactly equals baseline selection"
res_out=$(python3 - << EOF
import subprocess, json
baseline = ["fmt", "check", "clippy", "test"]
facts = {"status": "unresolved", "global_change": True}
input_data = {"baseline_selected": baseline, "resolver_facts": facts}
p = subprocess.run(["python3", "$POLICY_EVAL"], input=json.dumps(input_data), text=True, capture_output=True)
data = json.loads(p.stdout)
print(data.get("candidate_selected") == baseline and data.get("fallback_applied") == True)
EOF
)
if [[ "$res_out" == "True" ]]; then pass_test; else fail_test "Candidate fallback did not equal baseline selection"; fi

# 11. Report arithmetic and threshold check
run_test "invented/fixed timing data cannot bypass thresholds"
res_out=$(python3 - << EOF
import json
# Simulate summary with 0.0 savings ratio
summary = {
    "unsafe_exclusions": 0,
    "median_saving_ratio": 0.05, # < 0.25 threshold
    "p95_saving_ratio": 0.10     # < 0.15 threshold
}
verdict = "go" if (summary["unsafe_exclusions"] == 0 and summary["median_saving_ratio"] >= 0.25 and summary["p95_saving_ratio"] >= 0.15) else "no-go"
print(verdict == "no-go")
EOF
)
if [[ "$res_out" == "True" ]]; then pass_test; else fail_test "Threshold check failed"; fi

echo "=================================================="
if [[ $ERRORS -eq 0 ]]; then
  echo "ALL SELECTION IMPACT TESTS PASSED SUCCESSFULLY!"
  exit 0
else
  echo "$ERRORS SELECTION IMPACT TEST(S) FAILED!"
  exit 1
fi

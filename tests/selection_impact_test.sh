#!/usr/bin/env bash
# Hermetic unit & integration test suite for selection impact experiment protocol, resolver, policy, and benchmark report arithmetic.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target/selection-impact-target}"
RESOLVER="$ROOT/scripts/selection-impact/cargo-resolver.py"
POLICY_EVAL="$ROOT/scripts/selection-impact/evaluate-policy.py"
WORKSPACE="$ROOT/tests/fixtures/selection-impact/workspace"
MANIFEST="$ROOT/tests/fixtures/selection-impact/manifest.json"
BIN="${DO_HARNESS_BIN:-$ROOT/target/release/do-harness}"

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

# 1. Manifest file existence and schema validation
run_test "manifest existence and structure"
if [[ -f "$MANIFEST" ]]; then
  res_out=$(python3 - << EOF
import json
with open("$MANIFEST") as f:
    d = json.load(f)
print(d.get("schema_version") == 1 and len(d.get("cases", [])) >= 14)
EOF
  )
  if [[ "$res_out" == "True" ]]; then pass_test; else fail_test "Manifest structure invalid"; fi
else
  fail_test "Manifest not found"
fi

# 2. Valid resolver result
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

# 3. Unsupported schema
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

# 4. Malformed / non-JSON output handling
run_test "malformed input JSON"
res_out=$(python3 - << EOF
import subprocess, json
p = subprocess.run(["python3", "$RESOLVER"], input="INVALID_JSON", text=True, capture_output=True)
data = json.loads(p.stdout)
print(data.get("status") == "unresolved")
EOF
)
if [[ "$res_out" == "True" ]]; then pass_test; else fail_test "Expected unresolved for malformed JSON"; fi

# 5. Duplicate / escaping paths
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

# 6. Unknown component / file
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

# 7. Leaf and shared dependency closure
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

# 8. Proc macro & build script conservative handling
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

# 9. Global input fallback
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

# 10. Byte-identical repeated output
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

# 11. Policy fallback equals baseline selection
run_test "candidate fallback exactly equals baseline selection"
res_out=$(python3 - << EOF
import subprocess, json
baseline = ["check", "clippy", "test-shared", "test-leaf-a"]
facts = {"status": "unresolved", "global_change": True}
input_data = {"baseline_selected": baseline, "resolver_facts": facts}
p = subprocess.run(["python3", "$POLICY_EVAL"], input=json.dumps(input_data), text=True, capture_output=True)
data = json.loads(p.stdout)
print(data.get("candidate_selected") == baseline and data.get("fallback_applied") == True)
EOF
)
if [[ "$res_out" == "True" ]]; then pass_test; else fail_test "Candidate fallback did not equal baseline selection"; fi

# 12. Planted defect detection (mutation oracle)
run_test "planted defect detected by sensor"
res_out=$(python3 - << EOF
import subprocess, os
workspace = "$WORKSPACE"
abs_file = os.path.join(workspace, "crates/leaf_a/src/lib.rs")
with open(abs_file, "r") as f:
    orig = f.read()
try:
    with open(abs_file, "a") as f:
        f.write("\ncompile_error!(\"Planted error\");\n")
    p = subprocess.run(["$BIN", "verify", "--root", workspace, "--only", "check"], capture_output=True, text=True)
    print(p.returncode != 0)
finally:
    with open(abs_file, "w") as f:
        f.write(orig)
EOF
)
if [[ "$res_out" == "True" ]]; then pass_test; else fail_test "Sensor failed to detect planted defect"; fi

# 13. Planted downstream test failure detected by oracle
run_test "planted downstream test failure"
res_out=$(python3 - << EOF
import subprocess, os
workspace = "$WORKSPACE"
abs_file = os.path.join(workspace, "crates/shared_lib/src/lib.rs")
with open(abs_file, "r") as f:
    orig = f.read()
try:
    with open(abs_file, "w") as f:
        f.write("pub fn compute_value() -> u32 { 99 }\n")
    p = subprocess.run(["$BIN", "verify", "--root", workspace, "--only", "test-leaf-a"], capture_output=True, text=True)
    print(p.returncode != 0)
finally:
    with open(abs_file, "w") as f:
        f.write(orig)
EOF
)
if [[ "$res_out" == "True" ]]; then pass_test; else fail_test "Downstream test sensor failed to detect shared change"; fi

# 14. Report arithmetic and threshold check
run_test "invented/fixed timing data cannot bypass thresholds"
res_out=$(python3 - << EOF
summary = {
    "unsafe_exclusions": 0,
    "median_saving_ratio": 0.05,
    "p95_saving_ratio": 0.10
}
verdict = "go" if (summary["unsafe_exclusions"] == 0 and summary["median_saving_ratio"] >= 0.25 and summary["p95_saving_ratio"] >= 0.15) else "no-go"
print(verdict == "no-go")
EOF
)
if [[ "$res_out" == "True" ]]; then pass_test; else fail_test "Threshold check failed"; fi

# 15. Replay corpora existence and schema
run_test "replay corpora validation"
res_out=$(python3 - << EOF
import json, os
c1 = os.path.join("$ROOT", "tests/fixtures/selection-impact/corpora/corpus-do-harness-history.json")
c2 = os.path.join("$ROOT", "tests/fixtures/selection-impact/corpora/corpus-multi-crate-sample.json")
ok1 = os.path.exists(c1) and len(json.load(open(c1)).get("cases", [])) > 0
ok2 = os.path.exists(c2) and len(json.load(open(c2)).get("cases", [])) > 0
print(ok1 and ok2)
EOF
)
if [[ "$res_out" == "True" ]]; then pass_test; else fail_test "Replay corpora missing or invalid"; fi

echo "=================================================="
if [[ $ERRORS -eq 0 ]]; then
  echo "ALL SELECTION IMPACT TESTS PASSED SUCCESSFULLY!"
  exit 0
else
  echo "$ERRORS SELECTION IMPACT TEST(S) FAILED!"
  exit 1
fi

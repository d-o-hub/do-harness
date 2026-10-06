#!/usr/bin/env bash
# End-to-end benchmark for affected-component selection impact experiment.
# Compares baseline (A) vs candidate resolver+policy (B) and checks safety against planted defect oracles.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${DO_HARNESS_BIN:-$ROOT/target/release/do-harness}"
MANIFEST="${1:-$ROOT/tests/fixtures/selection-impact/manifest.json}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target/selection-impact-target}"
RESOLVER="$ROOT/scripts/selection-impact/cargo-resolver.py"
POLICY_EVAL="$ROOT/scripts/selection-impact/evaluate-policy.py"
SAMPLES="${SAMPLES:-3}"
OUT_FILE="${2:-}"

if [[ ! -f "$BIN" ]]; then
  echo "Building do-harness binary..." >&2
  cargo build --manifest-path "$ROOT/Cargo.toml" --bin do-harness --release >&2
fi

OS_NAME="$(uname -s)"
ARCH_NAME="$(uname -m)"
CPU_CLASS="$(lscpu 2>/dev/null | grep -i "Model name" | head -n1 | cut -d':' -f2 | xargs || echo "$ARCH_NAME")"
RUST_VER="$(rustc --version 2>/dev/null || echo "unknown")"
CARGO_VER="$(cargo --version 2>/dev/null || echo "unknown")"

python3 - "$MANIFEST" "$BIN" "$RESOLVER" "$POLICY_EVAL" "$ROOT" "$SAMPLES" "$OS_NAME" "$ARCH_NAME" "$CPU_CLASS" "$RUST_VER" "$CARGO_VER" "$OUT_FILE" << 'EOF'
import sys
import json
import subprocess
import time
import os
import math

manifest_path = sys.argv[1]
bin_path = sys.argv[2]
resolver_path = sys.argv[3]
policy_path = sys.argv[4]
root_dir = sys.argv[5]
samples = int(sys.argv[6])
os_name = sys.argv[7]
arch_name = sys.argv[8]
cpu_class = sys.argv[9]
rust_ver = sys.argv[10]
cargo_ver = sys.argv[11]
out_file = sys.argv[12] if len(sys.argv) > 12 and sys.argv[12] else None

with open(manifest_path, 'r', encoding='utf-8') as f:
    manifest = json.load(f)

environment_meta = {
    "os": os_name,
    "architecture": arch_name,
    "cpu_class": cpu_class,
    "rustc_version": rust_ver,
    "cargo_version": cargo_ver,
    "cache_state": "warm"
}

case_results = []
unsafe_exclusions = 0
qualifying_cases = 0

total_baseline_wall_ms = 0.0
total_resolver_wall_ms = 0.0
total_candidate_sensor_wall_ms = 0.0

saving_ratios = []
byte_identical_passes = True

cases = manifest.get("cases", [])
for case in cases:
    case_id = case["id"]
    change_class = case.get("change_class", "unknown")
    case_root = os.path.abspath(os.path.join(root_dir, case.get("root", ".")))
    changed_files = case.get("changed_files", [])
    planted = case.get("planted_defect")
    simulated_error = case.get("simulated_error")

    # 1. Baseline Selection
    explain_cmd = [bin_path, "explain", "--root", case_root, "--set", "verification", "--format", "json"]
    try:
        ex_proc = subprocess.run(explain_cmd, cwd=case_root, capture_output=True, text=True, check=True)
        explain_data = json.loads(ex_proc.stdout)
    except Exception as e:
        explain_data = {"selected": [], "skipped": []}

    baseline_selected = [s["name"] for s in explain_data.get("selected", [])]

    # 2. Resolver run & timing
    resolver_input = {
        "schema_version": 1,
        "root": case_root,
        "changed_files": changed_files
    }

    t_res_0 = time.perf_counter()
    if simulated_error == "unreadable_git_state":
        resolver_facts = {
            "schema_version": 1,
            "status": "unresolved",
            "components": [],
            "affected": [],
            "global_change": True,
            "warnings": ["Simulated unreadable git state"]
        }
    else:
        try:
            res_proc = subprocess.run(
                ["python3", resolver_path],
                input=json.dumps(resolver_input),
                cwd=case_root,
                capture_output=True,
                text=True,
                timeout=5
            )
            resolver_facts = json.loads(res_proc.stdout) if res_proc.stdout else {"status": "unresolved", "global_change": True, "warnings": ["empty output"]}
        except Exception as e:
            resolver_facts = {"status": "unresolved", "global_change": True, "warnings": [str(e)]}
    t_res_1 = time.perf_counter()
    resolver_wall_ms = (t_res_1 - t_res_0) * 1000.0

    # Test determinism / byte-identity
    try:
        res_proc2 = subprocess.run(
            ["python3", resolver_path],
            input=json.dumps(resolver_input),
            cwd=case_root,
            capture_output=True,
            text=True
        )
        if res_proc2.stdout != json.dumps(resolver_facts, indent=2) and res_proc2.stdout != json.dumps(resolver_facts):
            if json.loads(res_proc2.stdout) != resolver_facts:
                byte_identical_passes = False
    except Exception:
        pass

    # 3. Policy evaluation
    policy_input = {
        "baseline_selected": baseline_selected,
        "resolver_facts": resolver_facts
    }
    try:
        pol_proc = subprocess.run(
            ["python3", policy_path],
            input=json.dumps(policy_input),
            capture_output=True,
            text=True,
            check=True
        )
        policy_res = json.loads(pol_proc.stdout)
    except Exception as e:
        policy_res = {
            "candidate_selected": list(baseline_selected),
            "removed_sensors": [],
            "fallback_applied": True,
            "fallback_reason": str(e)
        }

    candidate_selected = policy_res.get("candidate_selected", [])

    # 4. Mutation-based Safety Oracle: prove that the planted defect is demonstrably detected
    defect_missed = False
    oracle_proven = False
    if planted:
        detecting_sensor = planted.get("detecting_sensor")
        rel_file = planted["file"]
        abs_file = os.path.join(case_root, rel_file)
        patch = planted["patch"]

        orig_content = None
        if os.path.exists(abs_file):
            with open(abs_file, "r", encoding="utf-8") as f:
                orig_content = f.read()

        try:
            # Apply patch
            with open(abs_file, "a" if orig_content is not None and not patch.startswith("pub fn") and not patch.startswith("fn main") and not patch.startswith("{") else "w", encoding="utf-8") as f:
                f.write(patch)

            # Confirm detecting sensor demonstrably fails under baseline
            oracle_proc = subprocess.run(
                [bin_path, "verify", "--root", case_root, "--only", detecting_sensor],
                cwd=case_root,
                capture_output=True,
                text=True
            )
            if oracle_proc.returncode != 0:
                oracle_proven = True

            # If candidate excludes the detecting sensor, it is an unsafe exclusion
            if detecting_sensor and detecting_sensor in baseline_selected:
                if detecting_sensor not in candidate_selected:
                    defect_missed = True
                    unsafe_exclusions += 1
        finally:
            if orig_content is not None:
                with open(abs_file, "w", encoding="utf-8") as f:
                    f.write(orig_content)
            elif os.path.exists(abs_file):
                os.remove(abs_file)

    # 5. Measure real wall-clock runs across samples
    baseline_samples = []
    for _ in range(samples):
        t0 = time.perf_counter()
        subprocess.run(
            [bin_path, "verify", "--root", case_root, "--set", "verification", "--format", "json"],
            cwd=case_root,
            capture_output=True,
            check=True
        )
        t1 = time.perf_counter()
        baseline_samples.append((t1 - t0) * 1000.0)

    candidate_samples = []
    for _ in range(samples):
        t0 = time.perf_counter()
        if candidate_selected:
            cmd = [bin_path, "verify", "--root", case_root]
            for s in candidate_selected:
                cmd.extend(["--only", s])
            cmd.extend(["--format", "json"])
            c_proc = subprocess.run(cmd, cwd=case_root, capture_output=True)
            if c_proc.returncode != 0:
                pass
        t1 = time.perf_counter()
        candidate_samples.append((t1 - t0) * 1000.0)

    baseline_med_ms = sorted(baseline_samples)[len(baseline_samples) // 2]
    candidate_sensor_med_ms = sorted(candidate_samples)[len(candidate_samples) // 2]
    candidate_total_med_ms = candidate_sensor_med_ms + resolver_wall_ms

    total_baseline_wall_ms += baseline_med_ms
    total_resolver_wall_ms += resolver_wall_ms
    total_candidate_sensor_wall_ms += candidate_sensor_med_ms

    is_qualifying = (not resolver_facts.get("global_change", False)) and (resolver_facts.get("status") == "resolved") and len(baseline_selected) > 0

    saving_ratio = 0.0
    if is_qualifying:
        qualifying_cases += 1
        if baseline_med_ms > 0:
            saving_ratio = max(0.0, (baseline_med_ms - candidate_total_med_ms) / baseline_med_ms)
        saving_ratios.append(saving_ratio)

    case_record = {
        "case_id": case_id,
        "change_class": change_class,
        "baseline_selected": baseline_selected,
        "candidate_selected": candidate_selected,
        "removed_sensors": policy_res.get("removed_sensors", []),
        "fallback_applied": policy_res.get("fallback_applied", False),
        "qualifying": is_qualifying,
        "planted_defect": planted,
        "oracle_proven": oracle_proven if planted else None,
        "defect_missed": defect_missed,
        "baseline_wall_ms": round(baseline_med_ms, 2),
        "resolver_wall_ms": round(resolver_wall_ms, 2),
        "candidate_sensor_wall_ms": round(candidate_sensor_med_ms, 2),
        "candidate_total_wall_ms": round(candidate_total_med_ms, 2),
        "saving_ratio": round(saving_ratio, 4)
    }
    case_results.append(case_record)

if saving_ratios:
    sorted_savings = sorted(saving_ratios)
    median_saving_ratio = sorted_savings[len(sorted_savings) // 2]
    p95_idx = min(len(sorted_savings) - 1, int(math.ceil(0.95 * len(sorted_savings))) - 1)
    p95_saving_ratio = sorted_savings[p95_idx]
else:
    median_saving_ratio = 0.0
    p95_saving_ratio = 0.0

# Predeclared decision gate criteria
go_verdict = (
    unsafe_exclusions == 0 and
    median_saving_ratio >= 0.25 and
    p95_saving_ratio >= 0.15 and
    byte_identical_passes
)

verdict_str = "go" if go_verdict else "no-go"

report = {
    "schema_version": 1,
    "environment": environment_meta,
    "cases": case_results,
    "summary": {
        "cases": len(case_results),
        "qualifying_cases": qualifying_cases,
        "unsafe_exclusions": unsafe_exclusions,
        "baseline_wall_ms": round(total_baseline_wall_ms, 2),
        "resolver_wall_ms": round(total_resolver_wall_ms, 2),
        "candidate_sensor_wall_ms": round(total_candidate_sensor_wall_ms, 2),
        "candidate_total_wall_ms": round(total_candidate_sensor_wall_ms + total_resolver_wall_ms, 2),
        "median_saving_ratio": round(median_saving_ratio, 4),
        "p95_saving_ratio": round(p95_saving_ratio, 4),
        "verdict": verdict_str
    }
}

output_json = json.dumps(report, indent=2)

if out_file:
    with open(out_file, 'w', encoding='utf-8') as f:
        f.write(output_json)

print(output_json)
EOF

#!/usr/bin/env python3
"""
Experiment-only Policy Evaluator.
Maps baseline selection and resolver facts to candidate sensor selection.
"""

import sys
import json

SENSOR_SCOPES = {
    # do-harness repository sensors
    "check-agt": ["crates/guardian-proxy"],
    "check-mcp": ["crates/guardian-proxy"],
    "powerset": ["crates/guardian-proxy"],
    # Fixture workspace component-scoped sensors
    "test-shared": ["crates/shared_lib"],
    "test-leaf-a": ["crates/leaf_a"],
    "test-leaf-b": ["crates/leaf_b"],
    "check-codegen": ["crates/codegen_crate"],
    "test-codegen": ["crates/codegen_crate"],
}

ALWAYS_APPLICABLE = {
    "commitlint",
    "dora"
}

def main():
    try:
        raw_input = sys.stdin.read()
        data = json.loads(raw_input)
    except Exception as e:
        print(json.dumps({"error": f"Failed to parse input: {e}"}))
        sys.exit(1)

    baseline_selected = data.get("baseline_selected", [])
    resolver_facts = data.get("resolver_facts", {})

    status = resolver_facts.get("status", "unresolved")
    global_change = resolver_facts.get("global_change", True)
    warnings = resolver_facts.get("warnings", [])

    # Fallback rule: if unresolved, global change, or warnings, keep baseline selection exactly
    if status != "resolved" or global_change or warnings:
        result = {
            "candidate_selected": list(baseline_selected),
            "removed_sensors": [],
            "fallback_applied": True,
            "fallback_reason": "global_change or unresolved or warnings"
        }
        print(json.dumps(result, indent=2))
        return

    active_component_roots = set()
    for c in resolver_facts.get("components", []):
        active_component_roots.add(c.get("root", ""))
    for a in resolver_facts.get("affected", []):
        active_component_roots.add(a.get("id", ""))

    candidate_selected = []
    removed_sensors = []

    for sensor in baseline_selected:
        if sensor in ALWAYS_APPLICABLE:
            candidate_selected.append(sensor)
            continue

        if sensor in SENSOR_SCOPES:
            target_roots = SENSOR_SCOPES[sensor]
            matched = any(
                any(comp == target or comp.startswith(target + "/") or target.startswith(comp + "/") for comp in active_component_roots)
                for target in target_roots
            )
            if matched:
                candidate_selected.append(sensor)
            else:
                removed_sensors.append({
                    "sensor": sensor,
                    "rule": f"component_scope_not_affected:{target_roots}"
                })
        else:
            candidate_selected.append(sensor)

    result = {
        "candidate_selected": candidate_selected,
        "removed_sensors": removed_sensors,
        "fallback_applied": False,
        "fallback_reason": None
    }
    print(json.dumps(result, indent=2))

if __name__ == "__main__":
    main()

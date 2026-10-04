#!/usr/bin/env python3
"""
Cargo Reference Adapter for Experimental Affected-Component Resolver Protocol.
Reads input JSON from stdin, runs `cargo metadata`, computes dependency facts, and outputs JSON facts.
"""

import sys
import json
import os
import subprocess

GLOBAL_PATTERNS = {
    "Cargo.lock",
    "rust-toolchain.toml",
    ".clippy.toml",
    ".markdownlint-cli2.jsonc",
    ".yamllint.yml",
    "do-harness.toml",
    "deny.toml"
}

def make_unresolved(warnings, status="unresolved", global_change=True):
    return {
        "schema_version": 1,
        "status": status,
        "components": [],
        "affected": [],
        "global_change": global_change,
        "warnings": warnings
    }

def main():
    try:
        raw_input = sys.stdin.read()
        if not raw_input.strip():
            print(json.dumps(make_unresolved(["Empty input"])))
            sys.exit(0)
        data = json.loads(raw_input)
    except Exception as e:
        print(json.dumps(make_unresolved([f"Invalid input JSON: {e}"])))
        sys.exit(0)

    if data.get("schema_version") != 1:
        print(json.dumps(make_unresolved(["Unsupported schema version"])))
        sys.exit(0)

    root = data.get("root", "")
    changed_files = data.get("changed_files", [])

    if not root or not os.path.isabs(root) or not os.path.exists(root):
        print(json.dumps(make_unresolved(["Invalid or non-existent root path"])))
        sys.exit(0)

    # Validate changed files paths
    global_change = False
    warnings = []

    normalized_changed = []
    for cf in changed_files:
        path = cf.get("path", "")
        # Path safety check
        if ".." in path.split("/") or path.startswith("/"):
            print(json.dumps(make_unresolved(["Path escape attempt detected"])))
            sys.exit(0)

        # Check global file patterns
        if path in GLOBAL_PATTERNS or path == "Cargo.toml":
            global_change = True

        normalized_changed.append(path)

    # Run cargo metadata
    try:
        proc = subprocess.run(
            ["cargo", "metadata", "--format-version=1"],
            cwd=root,
            capture_output=True,
            text=True,
            timeout=10
        )
        if proc.returncode != 0:
            print(json.dumps(make_unresolved([f"cargo metadata failed: {proc.stderr}"])))
            sys.exit(0)
        metadata = json.loads(proc.stdout)
    except Exception as e:
        print(json.dumps(make_unresolved([f"cargo metadata execution error: {e}"])))
        sys.exit(0)

    packages = metadata.get("packages", [])
    workspace_members = set(metadata.get("workspace_members", []))
    resolve = metadata.get("resolve", {})
    nodes = resolve.get("nodes", []) if resolve else []

    # Map package id -> package info
    pkg_by_id = {pkg["id"]: pkg for pkg in packages}

    # Identify workspace packages and map root directories
    member_pkgs = []
    for pkg_id in workspace_members:
        if pkg_id in pkg_by_id:
            pkg = pkg_by_id[pkg_id]
            manifest_path = os.path.abspath(pkg["manifest_path"])
            pkg_root = os.path.dirname(manifest_path)
            rel_root = os.path.relpath(pkg_root, root).replace("\\", "/")
            member_pkgs.append({
                "id": pkg_id,
                "name": pkg["name"],
                "root": rel_root,
                "abs_root": pkg_root,
                "targets": pkg.get("targets", [])
            })

    # Sort member packages by root length descending so nested packages match first
    member_pkgs.sort(key=lambda x: len(x["root"]), reverse=True)

    # Build dependency graph: child -> set of parent dependants
    # node in nodes has "id" and "deps" = [{"pkg": dep_id, ...}]
    dependents = {pkg_id: set() for pkg_id in pkg_by_id}
    for node in nodes:
        parent_id = node["id"]
        for dep in node.get("deps", []):
            dep_id = dep["pkg"]
            if dep_id in dependents:
                dependents[dep_id].add(parent_id)

    # Map changed files to component members
    directly_modified_member_ids = set()
    unmapped_files = []

    for path in normalized_changed:
        matched = False
        for mp in member_pkgs:
            m_root = mp["root"]
            if path == m_root or path.startswith(m_root + "/"):
                directly_modified_member_ids.add(mp["id"])
                matched = True
                # Check for proc macro or build.rs
                if mp.get("targets"):
                    for t in mp["targets"]:
                        if "proc-macro" in t.get("kind", []):
                            # Proc macro modified: affects all dependants
                            pass
                break
        if not matched and not (path in GLOBAL_PATTERNS or path == "Cargo.toml"):
            unmapped_files.append(path)

    if unmapped_files:
        global_change = True
        warnings.append(f"Unmapped files detected: {unmapped_files}")

    # Compute transitive closure of affected workspace packages
    affected_member_ids = set()

    def visit(pkg_id):
        for parent_id in dependents.get(pkg_id, []):
            if parent_id in workspace_members and parent_id not in directly_modified_member_ids and parent_id not in affected_member_ids:
                affected_member_ids.add(parent_id)
                visit(parent_id)

    for mod_id in list(directly_modified_member_ids):
        visit(mod_id)

    # Format components and affected lists
    components = []
    for mp in member_pkgs:
        if mp["id"] in directly_modified_member_ids:
            components.append({
                "id": mp["root"],
                "root": mp["root"]
            })

    affected = []
    for mp in member_pkgs:
        if mp["id"] in affected_member_ids:
            affected.append({
                "id": mp["root"],
                "reason": "transitive_dependant"
            })

    components.sort(key=lambda x: x["id"])
    affected.sort(key=lambda x: x["id"])

    result = {
        "schema_version": 1,
        "status": "resolved",
        "components": components,
        "affected": affected,
        "global_change": global_change,
        "warnings": sorted(warnings)
    }

    print(json.dumps(result, indent=2))

if __name__ == "__main__":
    main()
EOF

# Experimental Affected-Component Resolver Protocol Contract

> **EXPERIMENTAL / INTERNAL ONLY**: This protocol is designed strictly for measured experiments in selection impact evaluation. It is NOT a shipped public contract, and MUST NOT be referenced as production API.

## Overview
The affected-component resolver provides provider-neutral dependency graph analysis to determine which workspace components are directly modified or transitively affected by a set of changed files.

The resolver returns **facts only** regarding component scopes and transitively affected components. It **never makes final sensor selection decisions**; sensor applicability remains owned by the harness policy evaluation layer.

---

## Input Protocol (JSON)

An invocation passes a single JSON payload on `stdin`:

```json
{
  "schema_version": 1,
  "root": "/absolute/repository/root",
  "changed_files": [
    {
      "path": "crates/db/src/lib.rs",
      "kind": "modified"
    }
  ]
}
```

### Input Fields
- `schema_version` (integer, required): Must equal `1`.
- `root` (string, required): Absolute filesystem path to the repository root directory.
- `changed_files` (array of objects, required): List of modified/added/deleted files relative to `root`.
  - `path` (string, required): Normalized repository-relative file path (forward slashes, no leading `/`, no `.` or `..` escape segments).
  - `kind` (string, required): Change type (`modified`, `added`, `deleted`, `renamed`).

---

## Output Facts Protocol (JSON)

Upon success, the resolver writes a single JSON payload to `stdout`:

```json
{
  "schema_version": 1,
  "status": "resolved",
  "components": [
    {
      "id": "crates/db",
      "root": "crates/db"
    }
  ],
  "affected": [
    {
      "id": "crates/do-harness",
      "reason": "depends_on:crates/db"
    }
  ],
  "global_change": false,
  "warnings": []
}
```

### Output Fields
- `schema_version` (integer, required): Must equal `1`.
- `status` (string, required): `"resolved"` or `"unresolved"`. If `"unresolved"`, the consumer MUST fall back to baseline when-changed selection.
- `components` (array of objects, required): Directly modified components mapped from `changed_files`.
  - `id` (string): Opaque identifier for the component (e.g. workspace crate path or package name).
  - `root` (string): Repository-relative root directory of the component.
- `affected` (array of objects, required): Downstream components transitively affected by changes in `components`.
  - `id` (string): Opaque identifier for affected component.
  - `reason` (string): Dependency relation explanation (e.g. `depends_on:crates/db`, `proc_macro_dependant:crates/proc_macro`).
- `global_change` (boolean, required): Set to `true` if any changed file affects global configuration, toolchain, root manifest, lockfile, policy, unknown file types, or unresolvable build inputs.
- `warnings` (array of strings, required): Non-fatal warnings logged during graph traversal.

---

## Strict Contract Rules & Fallback Conditions

To maintain fail-closed safety, the consumer MUST ignore resolver facts and fall back to exact baseline `when-changed` selection under ANY of the following conditions:

1. **Schema Mismatches**: Input or output `schema_version` != `1` or missing required fields.
2. **Invalid Output Format**: Non-JSON stdout, malformed JSON syntax, or empty stdout.
3. **Runtime or Resource Limits**: Execution exceeds timeout (e.g., 5 seconds) or output size exceeds max limit (e.g., 10 MB).
4. **Non-Zero Exit Code**: Process terminates with a non-zero exit code.
5. **Path Escaping**: Paths containing `..`, absolute paths outside `root`, or path manipulation attempts.
6. **Unknown or Unmapped Files**: Files that do not belong to any known workspace component or manifest rule trigger `global_change=true` or baseline fallback.
7. **Global Changes**: Modifications to root manifests (`Cargo.toml`), lockfiles (`Cargo.lock`), toolchain files (`rust-toolchain.toml`), root linter/formatter configs (`.clippy.toml`, `.markdownlint-cli2.jsonc`), or harness config (`do-harness.toml`) force `global_change=true`.
8. **Conservative Proc-Macro & Build Script Handling**: Procedural macros and build scripts (`build.rs`) MUST be treated conservatively as affecting all downstream dependents or triggering full verification.
9. **Determinism**: Outputs must be sorted deterministically by component `id` and `path` to produce byte-identical fact outputs across repeated executions.
10. **Command Execution Safety**: Repository paths and file contents are pure data and MUST NEVER be interpolated into shell execution strings.

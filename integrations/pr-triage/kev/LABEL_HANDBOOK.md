# Kev PR Triage Label Handbook (v1.0)

This handbook defines the labeling standard for fine-tuning Kev models on GitHub PR triage tasks and mapping predictions to atomic review depth judgments (#112 protocol).

## 1. Atomic Questions

Every PR diff unit is evaluated against five atomic questions:

1. **`change_kind`**: `docs | tests | dependency | internal | public-api | security | mixed | unknown`
   - `docs`: Modifies documentation, comments, or non-code markdown files only.
   - `tests`: Modifies test files, test fixtures, or test utilities without altering production logic.
   - `dependency`: Lockfile or dependency metadata updates.
   - `internal`: Internal refactor or logic change not affecting exported public API contracts.
   - `public-api`: Changes exported types, public function signatures, trait contracts, or REST/gRPC interfaces.
   - `security`: Touches authentication, authorization, cryptography, token management, secrets, or provenance verification.
   - `mixed`: Combines multiple distinct categories (e.g. mechanical renames mixed with a schema or security change).
   - `unknown`: Insufficient evidence or corrupted context.

2. **`behavior_change`**: `yes | no | unknown`
   - `yes`: Alters runtime execution paths, state mutations, output values, or side effects.
   - `no`: Pure refactor, formatting, documentation, or comment change preserving strict behavioral equivalence.
   - `unknown`: Context truncated or insufficient information to verify behavioral equivalence.

3. **`public_contract_change`**: `yes | no | unknown`
   - `yes`: Breaks, adds, or modifies public API signatures, exported types, configuration keys, or binary contract interfaces.
   - `no`: All changes are private or package-internal.
   - `unknown`: Export visibility cannot be determined from diff alone.

4. **`security_sensitive`**: `yes | no | unknown`
   - `yes`: Touches security controls, authentication, input validation, encryption, access boundaries, or trust models.
   - `no`: No security implications.
   - `unknown`: Incomplete diff covering security boundaries.

5. **`needs_repository_context`**: `yes | no | unknown`
   - `yes`: Requires reading wider codebase contexts (e.g. database schema, concurrency locking invariant, distant call-sites) to verify correctness.
   - `no`: Diff is self-contained.
   - `unknown`: Incomplete or truncated context.

---

## 2. Minimum Safe Route Mapping

A deterministic route policy maps atomic predictions to review depth:

| Atomic Conditions | Route | Review Payload |
|---|---|---|
| `change_kind` in (`docs`, `tests`) AND `behavior_change == "no"` AND all risk flags == `"no"` | `cheap` | Zero review tokens (skipped) |
| `behavior_change == "yes"` AND public/security flags == `"no"` AND `needs_repository_context == "no"` | `focused` | Unresolved semantic residual |
| `security_sensitive == "yes"` OR `public_contract_change == "yes"` OR `needs_repository_context == "yes"` OR any flag == `"unknown"` OR `confidence < 0.80` | `deep` | Full raw diff review |

---

## 3. Ground Truth Rules & Defect Distinction

- **Observed Defects vs Merged Status**: Merged PR status is NOT proof of correctness. Ground truth labels must record whether a PR introduced defects or required follow-up hotfixes regardless of whether it was merged.
- **Buried Changes**: In mixed diffs containing mechanical edits alongside consequential changes, the ground truth must reflect the most sensitive change (`deep`).

---

## 4. Provenance & Disputed Examples

- All labels must record annotator ID, timestamp, source repository, and commit SHA.
- Any label dispute between independent reviewers must be escalated to a two-reviewer consensus resolution. Disputed examples that cannot reach consensus must be labeled with `unknown` and assigned route `deep`.

---
name: crates-io-name-check
description: >
  Check crates.io name availability, naming appropriateness, and reservation
  guidance before creating or publishing a new crate. Use when introducing a
  new publishable crate, renaming a crate, or verifying name availability prior
  to first publish.
license: MIT
metadata:
  short-description: Verify crate name availability and appropriateness on crates.io
  tags: cargo crates-io publishing crates release naming
---

# crates.io Name Check Skill

## Purpose
Verify crate name availability, check naming appropriateness, and record reservation
guidance before introducing or publishing a new crate to crates.io.

## Exact Check Commands

Run these check commands before creating or first-publishing a new crate:

### 1. crates.io API Availability Check
crates.io requires a custom `User-Agent` header on unauthenticated API requests; requests without `User-Agent` are rejected with HTTP 403.

```bash
crate_name="do-harness-newcrate"
curl -s -o /dev/null -w "%{http_code}\n" \
  -H "User-Agent: do-harness-name-check (https://github.com/d-o-hub/do-harness)" \
  -H "Accept: application/json" \
  "https://crates.io/api/v1/crates/${crate_name}"
```

**Interpreting HTTP status codes:**
- `404`: Crate name is **available** (not found on crates.io).
- `200`: Crate name is **taken** (already published on crates.io).
- `403`: Missing or invalid `User-Agent` header in request.

### 2. Cargo Search Similarity Check
Search for similar or colliding existing crates:

```bash
cargo search "${crate_name}"
```

Review the results to ensure the candidate name does not closely collide with or typosquat an existing crate.

## Naming Appropriateness Guidelines

- **Convention**: Use lowercase `kebab-case` (e.g. `do-harness-types`) or `snake_case`.
- **Project Scope**: Prefer prefixed workspace crate names (e.g., `do-harness-*`) to avoid namespace collisions and clarify ownership.
- **Avoid Misleading Prefixes**: Do not use `rust-`, `official-`, or `std-` prefixes, or generic single-word names.
- **Squatting Policy Compliance**: crates.io prohibits name squatting without functional code. Do not publish empty placeholder crates solely to reserve names.

## Check-then-Record Pre-Publish Workflow

Before opening a PR that adds or publishes a new crate:

1. Execute the `curl` status check and `cargo search` command.
2. Record the terminal output in the pull request description or release record.
3. If the name returns `404`, proceed with crate creation or tag release.
4. If the name returns `200` or conflicts with existing crates, select a non-colliding alternative (e.g. using `do-harness-<purpose>`) and re-run the check.

## Quality Checklist
- API probe always includes `-H "User-Agent: ..."` and `-H "Accept: application/json"`.
- `404` status code confirmed prior to first publish attempt.
- Name check output pasted into pre-publish / release documentation.
- No automated CI gate required (advisory skill run on demand).

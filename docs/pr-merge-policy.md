# PR Merge Policy and Server Governance

This document describes repository PR merge policy enforcement, ruleset drift
detection, and the strict GitHub server-side boundary.

## Overview

Computational harness checks provide fast, reproducible local and CI evidence.
However, merge safety ultimately depends on server-side protection rules that
cannot be bypassed by merge automation or client misconfiguration.

The merge policy contract is declared in `plans/pr-merge-policy.toml`.
Compliance is checked via:

<!-- example: id=doctor_pr_policy_cmd classification=illustrative reason="Doctor pr-policy CLI command syntax" -->
```bash
do-harness doctor pr-policy --format json
```

Exit codes:
- `0`: PASS — server rulesets match expected configuration.
- `1`: DRIFT — server rulesets diverge from `plans/pr-merge-policy.toml`.
- `2`: UNKNOWN — permissions, API failure, or malformed policy prevent verification.

## Enforced Ruleset Requirements

Targeting the default branch (`main`), active rulesets must enforce:

1. **Review Thread Resolution:** `required_review_thread_resolution: true`.
   Every conversation thread must be explicitly addressed or resolved on GitHub.
2. **Required Status Checks:**
   - Strict status check policy: `strict_required_status_checks_policy: true`
     (head branch must be up to date with base before merging).
   - Mandatory check contexts:
     - `pr-required` (app ID `15368` / GitHub Actions) aggregating all platform builds.
     - `semver` (app ID `15368` / GitHub Actions) protecting public crate APIs.
3. **No Standing Automation Bypass:**
   - `bypass_actors: []`.
   - `current_user_can_bypass: "never"`.
   - Normal automation and owner accounts must obey branch rules.
4. **Code Quality and Security Scanning:**
   - CodeQL alerts threshold: `high_or_higher` security alerts, `errors_and_warnings`.
   - Code quality severity: `errors`.
   - Deletion protection: `deletion: true`.

## Solo-Maintainer Model

For solo-maintained repositories, `required_approving_review_count` remains `0`.
Mandatory approval count is not fabricated through bot reviews or synthetic accounts;
instead, server enforcement guarantees conversation resolution and required CI checks
pass before any merge can complete.

## Findings Policy (`[findings]`)

The `[findings]` table declares how CI findings are tracked:

- `policy = "managed-backlog"` (default): introduced and regressed findings
  block, and critical findings stay blocked until they carry a linked issue or
  an authorized disposition. Pre-existing findings outside the changed code are
  tracked with a linked issue instead of silently expanding every PR's scope.
- `policy = "strict-zero-baseline"` (opt-in): every actionable finding blocks,
  including the pre-existing backlog. Choose it when the repository wants no
  merge until the backlog is gone.
- `max_new_issues_per_run` (default 20, hard-capped at 20) bounds issue creation
  so a noisy pipeline cannot flood the tracker.

Changing a threshold never clears a finding: a critical security finding is a
blocker whatever the configured severity floor is.

Workflow-level notices that `GitHub` reports against the workflow directory
rather than a source file (a deprecated action runtime, a runner-image
migration notice) are recorded as infrastructure signals and never gate a
merge: they describe the platform, not a defect the change introduced.

## Operator Remediation

When `do-harness doctor pr-policy` reports `DRIFT`, the JSON or text output
identifies the specific `rule_type`, `field`, `expected`, `actual`, and `remediation`.

### Break-Glass Emergency Procedure

In an emergency where GitHub Actions is down or emergency fixes must be applied:
1. The repository administrator opens the GitHub repository Settings -> Rules -> Rulesets.
2. An authorized operator temporarily modifies bypass rules for a specific administrator session.
3. Once the emergency is resolved, bypass settings are reverted to `never` and verified with:
   `do-harness doctor pr-policy`.

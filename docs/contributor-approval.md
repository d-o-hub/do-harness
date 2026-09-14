# Contributor approval path

The `contributor-reputation` workflow (`.github/workflows/contributor-check.yml`)
runs the Microsoft AGT `contributor-check` action on every opened/reopened PR
and issue. It fails closed when risk is `HIGH` or unknown (`risk-threshold:
"HIGH"`), so only high-confidence spam or suspicious accounts get blocked —
`MEDIUM` passes without manual approval.

## When `screen-author` fails

1. A maintainer with write access reviews the PR (code + provenance).
2. The maintainer posts an approving PR review.
3. Anyone with write access closes and reopens the PR. Reopen re-triggers
   `pull_request_target`, re-running `screen-author`.
4. If the author is now allowlisted/known (or risk scores below `HIGH`), the
   gate goes green. If it still scores `HIGH`, repeat from step 1 — do not
   bypass the gate.

## Why close/reopen

The workflow triggers on `opened` and `reopened` only (not `synchronize`),
so the re-run path is explicit and auditable: the reopen event itself is the
record that a human re-armed the gate after review. No auto-retry, no
allowlist file to drift.

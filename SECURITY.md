# Security Policy

## Reporting a Vulnerability

Report vulnerabilities privately via GitHub's
[Security Advisories](https://github.com/d-o-hub/do-harness/security/advisories/new)
("Report a vulnerability" button), not public issues.

Include: affected version (`do-harness -V`), reproduction steps, and impact
assessment. You will receive an initial response within 7 days.

## Supported Versions

Only the latest `main` is supported; this project releases from HEAD.

## Supply-Chain Posture

- `#![forbid(unsafe_code)]` across the workspace.
- `cargo-deny` policy in `deny.toml`: license allowlist, yanked-crate deny,
  unknown registry/git source deny.
- RustSec advisory scanning enforced by the `audit` sensor
  (`scripts/check-audit.sh`, fail-closed under `CI=true`) and by
  `cargo deny check` within the `deps` sensor.
- GitHub Actions are pinned to full-length commit SHAs with least-privilege
  `permissions`.

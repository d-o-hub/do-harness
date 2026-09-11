# Security Policy

## Reporting a Vulnerability

Report vulnerabilities privately via GitHub's
[Security Advisories](https://github.com/d-o-hub/do-harness/security/advisories/new)
("Report a vulnerability" button), not public issues.

Include: affected version (`do-harness -V`), reproduction steps, and impact
assessment. No PGP key is published for this project; GitHub Security
Advisories is the encrypted reporting channel.

## Disclosure Timeline and SLA

| Stage | Target |
|---|---|
| Initial response (acknowledgement + triage start) | 7 days |
| Triage outcome + severity shared with reporter | 30 days |
| Coordinated disclosure / fix release | 90 days from report, extendable by mutual agreement |

Credit is offered in the advisory unless the reporter prefers to stay anonymous.

## Severity Rubric (CVSS v3.1)

Each report is scored with CVSS v3.1 and the vector string is shared with the
reporter:

- **Critical (9.0–10.0)** — remote code execution, audit-chain forgery without
  detection, or credential exfiltration from CI.
- **High (7.0–8.9)** — fail-open mediation, SSRF to metadata endpoints,
  privilege escalation in managed hooks.
- **Medium (4.0–6.9)** — local denial of service, evidence tampering that is
  detected only on re-verification.
- **Low (0.1–3.9)** — information disclosure of non-sensitive local state,
  defense-in-depth gaps with no practical exploit path.

## Supported Versions and MSRV

Only the latest `main` is supported; this project releases from HEAD. The
Minimum Supported Rust Version is **1.85**, enforced by the `msrv` job in
`.github/workflows/verify.yml` (`cargo check --workspace --locked`) and
declared in `Cargo.toml`. MSRV is raised at most once per Rust release cycle
and is announced in the commit that changes `rust-version`.

## Supply-Chain Posture

- `#![forbid(unsafe_code)]` across the workspace.
- `cargo-deny` policy in `deny.toml`: license allowlist, yanked-crate deny,
  unknown registry/git source deny.
- RustSec advisory scanning enforced by the `audit` sensor
  (`scripts/check-audit.sh`, fail-closed under `CI=true`) and by
  `cargo deny check` within the `deps` sensor.
- GitHub Actions are pinned to full-length commit SHAs with least-privilege
  `permissions`; `Cargo.lock` is committed and hashed into CI evidence.
- Log and artifact retention is documented in `docs/retention.md`.

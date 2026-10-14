# serde_yaml migration (retires the RUSTSEC-2024-0449 ignore)

## Decision

RUSTSEC-2024-0449 marks `unsafe-libyaml` 0.2.11 — pulled only transitively by
`serde_yaml` 0.9 — as unmaintained (upstream `dtolnay/unsafe-libyaml` is
archived). The advisory records maintenance status, not a reported
vulnerability, and the workspace touches it in two places:

- `crates/do-harness/src/skills/catalog.rs` — skill frontmatter parsing.
- `crates/guardian-proxy/src/proxy.rs` — the optional, pre-GA `agt-governance`
  policy parameters.

The advisory database added the finding on 2026-10-12; it fails both
`cargo deny check` (the `deps` sensor, `unmaintained = "transitive"`) and
`cargo audit` (the `audit` sensor). Because that flips the gate for `main` and
every open PR at once, the finding is temporarily ignored — never silenced —
while the migration lands:

- `deny.toml` `[advisories] ignore = ["RUSTSEC-2024-0449"]`
- `scripts/check-audit.sh` `--ignore RUSTSEC-2024-0449`
- decision header in `plans/invariants.json` (category `supply-chain`)

## Next action

Partially done by #308 (2026-10-13): `crates/do-harness` skill frontmatter
parsing now uses `serde_norway` 0.9.42 (one-line call-site change; `cargo tree
-i unsafe-libyaml` on default features shows no path). Remaining: `serde_yaml`
stays as a `guardian-proxy`-optional dependency because the retired
`agent-governance` 3.2.2 SDK names `serde_yaml::Value` in its
`execute_with_governance` signature, so the `deny.toml` + `scripts/check-audit.sh`
`RUSTSEC-2024-0449` ignores stay until the SDK drops `serde_yaml` or the
`chore-agt-promotion` decision removes the feature. Delete both ignore entries
and the invariants decision in the same PR that clears the last edge.

Original plan (kept for the record): replace `serde_yaml` 0.9 with a maintained
YAML crate in `[workspace.dependencies]`, migrate the two call sites, then delete
both ignore entries and the invariants decision in the same PR.

Candidates checked against crates.io on 2026-10-12 (name, max stable version,
last publish):

| Crate | Version | Published | Note |
|---|---|---|---|
| `serde_norway` | 0.9.42 | 2024-12-21 | API-compatible fork of serde_yaml 0.9 |
| `serde_yaml_ng` | 0.10.0 | 2024-05-26 | fork, `Value` API parity |
| `serde_yml` | 0.0.13 | 2026-05-27 | newest, 0.x API |

Run the `crates-io-name-check` skill before adopting one, verify licence and
MSRV against `deny.toml`/`rust-toolchain.toml`, and prefer the fork with the
smallest API delta from `serde_yaml` 0.9 so the migration stays reviewable.

## Spike outcome (2026-10-13, `crates-io-name-check` + registry metadata)

Checked each candidate's dependencies, licence, and `rust_version` via the
crates.io API before touching the tree:

- `serde_norway` 0.9.42 — **chosen**. MIT OR Apache-2.0, `rust_version` 1.71.1,
  links `unsafe-libyaml-norway` 0.2.15 (MIT, rust 1.71.1, a distinct crate the
  advisory DB does not flag) instead of the archived `unsafe-libyaml`.
  API-identical fork of serde_yaml 0.9, so the frontmatter call site migrates
  with a one-line change.
- `serde_yaml_ng` 0.10.0 — **rejected**. Still depends on `unsafe-libyaml`
  `^0.2.11` directly, so it cannot retire the advisory.
- `serde_yml` 0.0.13 — **rejected**. A deprecated shim forwarding to `noyalib`,
  whose normal dependencies include `tokio`, `rayon`, `jsonschema`,
  `validator`, and `figment`: far too heavy for frontmatter parsing.
- `serde-saphyr` 1.3.0 — **rejected for now**. Pure Rust and recently released
  (2026-09-16), but `rust_version` 1.89 exceeds the workspace MSRV 1.85 and
  its default features pull `figment`/`garde`/`validator`/`miette`.
- `agentmesh` 4.0.0 (the live AGT SDK line) still depends on `serde_yaml`
  0.9.34 as a normal dependency, so migrating the SDK would not clear the
  advisory either.

`crates/do-harness` (skill frontmatter, unconditional) now uses `serde_norway`;
`serde_yaml` stays as a `guardian-proxy`-optional dependency because the
retired `agent-governance` 3.2.2 SDK names `serde_yaml::Value` in its
`execute_with_governance` signature (`policy.rs:189`), which no fork's nominal
type can satisfy. `cargo tree -i unsafe-libyaml` on default features shows no
path; `cargo audit` (lockfile-wide) still reports RUSTSEC-2024-0449 via the
SDK's own transitive edge, so the ignore entries stay until the SDK drops
`serde_yaml` or the `agt-governance` feature is removed by the
`chore-agt-promotion` decision — full removal was never achievable by migrating
the two call sites alone.

## Executable gates

- `cargo deny check advisories` and `cargo audit --deny warnings` pass **with
  the ignore entries removed** (the migration is only done when the ignores are
  gone).
- `cargo test -p do-harness -p guardian-proxy` passes; the frontmatter and
  policy-parameter tests are the behavioural proof.
- `bash scripts/check-audit.sh` and the `deps` sensor stay green afterwards.

## References

- <https://rustsec.org/advisories/RUSTSEC-2024-0449>
- <https://github.com/dtolnay/unsafe-libyaml>

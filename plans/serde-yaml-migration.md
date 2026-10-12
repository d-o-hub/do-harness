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

Replace `serde_yaml` 0.9 with a maintained YAML crate in
`[workspace.dependencies]`, migrate the two call sites, then delete both ignore
entries and the invariants decision in the same PR.

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

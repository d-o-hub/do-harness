# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Primary users are autonomous coding agents executing work inside a git
repository: they are the runtime the harness guides (feedforward) and gates
(feedback). Human developers and CI pipelines are secondary consumers — they
adopt the harness, author its configuration, and read the verification
evidence agents produce. Confirmed with the user ("agents first").

## Product Purpose

do-harness is a compiled agent-execution harness CLI. It pairs feedforward
guides (AGENTS.md conventions, `.agents/skills/` skill packs) with feedback
sensors (computational checks that agents and CI must pass). Success means an
agent's claimed work is backed by current, non-vacuous computational
evidence — a change is complete only when its verification evidence is
current, and a sensor that ran zero checks is a failed gate, not a pass.

## Positioning

Computational sensors strictly supersede LLM self-assessment, delivered as a
repository-agnostic compiled CLI. Confirmed with the user. Neighboring
products cannot truthfully copy the combination of: native subcommands that
run on Linux, macOS, and Windows in any git repository without the adopting
checkout's scripts, Python, or a POSIX shell; evidence freshness contracts
(stale workspace or policy state is red, not green); and the client boundary
conformance model in which a passing fixture alone never proves client
mediation.

## Operating Context

- Adoption flow: install the CLI onto `PATH` in any codebase (Rust or not),
  then `do-harness init` detects a language pack; verification runs via
  `do-harness verify --set <set> [--changed] [--strict]`, with
  `do-harness status` reporting evidence currency.
- Agent workflow context: latest-main freshness before work
  (`do-harness sync`), skill suggestion (`do-harness skills suggest`),
  task workflow runtime (`task add/advance/done/fail`), fail-fast recovery
  (`errors list/clear`), skill evaluation (`do-harness eval`), metrics and
  DORA collection (`metrics`, `dora`).
- Distribution context: zero-install npx (Node 18+), prebuilt static binaries
  as GitHub Releases with SHA-256 checksums, `cargo-binstall`, and
  registry-validated source installs; Windows requires a release-zip fallback
  because npm blocks the `do-harness-win32-x64` package name.

## Capabilities and Constraints

- Language sensor packs: `rust`, `web` (viewport, a11y, console, perf,
  visual, i18n audits), `node`, `generic`.
- Persistence: local libSQL instance at `.do-harness/agent_state.db`; the
  database is local state, never committed; the versioned contract lives in
  `crates/db/migrations/*.sql`, `plans/*.json`, and `task export` snapshots.
- Hard constraints from AGENTS.md: maximum 500 LOC per source file (decompose
  nearing 450); every shipped capability must be a native `do-harness`
  subcommand usable in any repository and on any supported OS —
  repository-local `scripts/**` are development tooling, never a delivery
  surface; verification never mutates Git state.
- Evidence schema v4; development-signal contract documented in
  `plans/development-signals-epic.md`; architecture policies as executable
  invariants in `plans/invariants.json`.
- The harness's web-ui sensor pack enforces WCAG 2.2 criteria (1.4.3
  contrast, 1.4.10 reflow, 2.4.7 focus visibility, 2.5.8 target size,
  3.1.1/3.1.2 language) in adopting repositories.

## Evidence on Hand

- Repository docs: `docs/` (adoption, compliance, provenance trust model,
  releasing, retention, documentation contract), `README.md`, `AGENTS.md`
  (the feedforward guide the product itself ships).
- Real sensor implementations in `crates/` (`db`, `do-harness`,
  `guardian-proxy`, `types`), the web-ui audit library with committed clean
  and intentional-defect fixtures in `integrations/web-ui/`, and benchmarks
  in `benchmarks/`.
- Published releases with checksums (see README install section).
- Absence to preserve: no user testimonials, customer lists, or benchmark
  claims against competing products exist in the repository; future work
  must not fabricate any.

## Product Principles

1. Evidence over assertion: computational sensors strictly supersede LLM
   self-assessment; claimed completion without current evidence is failure.
2. Repository-agnostic by contract: a capability that only works in this
   checkout is a defect, not a feature.
3. Honesty at the boundary: claims carry their own verification story
   (checksums, provenance, non-vacuous gates, fixture-vs-mediation
   distinctions).
4. Proportional ceremony: the ordinary route scales to the change;
   decomposition and distillation are tools, not phases.

## Brand Commitments

- The name `do-harness` and the feedforward/feedback framing (guides plus
  sensors) are established product language used across README, docs, and
  the companion site repository.
- Voice: precise, evidence-grounded, free of buzzwords; claims are stated
  with their constraints (for example, "checksums detect corruption, not a
  compromised origin").

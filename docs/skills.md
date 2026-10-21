# Skills

Skills are the harness's feedforward guides: markdown directories an agent
loads before coding, paired with executable evals that prove the guidance
works. Sensors are the feedback half (they fire after a change); skills try
to prevent the fire. See `docs/cli.md` for the command reference
(`skills suggest`, `skills drift`, `eval`, `overlap`, `distill`, `learn`).

## Where skills live

- `.agents/skills/` — the source of truth. Every skill the harness evaluates
  lives here.
- `crates/do-harness/templates/skills/` — the scaffolded subset (`harness` +
  `skill-creator`) embedded into `do-harness init` output for adopting
  repositories. One-way sync only: templates mirror the canonical files
  byte-for-byte, enforced by the `skills` sensor
  (`scripts/check-skills-sync.sh`). Edit the canonical copy, never the
  template.
- The development-methodology skills (HTN planning, spikes, event modeling,
  distillation) describe how to build do-harness itself and are intentionally
  not scaffolded into adopting projects.

## The skill catalog

| Skill | Purpose |
|-------|---------|
| `crates-io-name-check` | Verify crates.io name availability before publishing |
| `event-modeler` | Type-safe event-sourced slices (commands, events, projections) |
| `fail-closed-proxy` | Fail-closed tool-call mediation with audit and metrics |
| `harness` | Feedforward guides + feedback sensors, self-correction and steering loops |
| `htn-planner` | Decompose objectives into task networks with precondition guards |
| `issue-to-pr` | Turn open issues and pending plan tasks into focused pull requests |
| `move-only-decomposition` | Split files at the line ceiling without behavior change |
| `nested-build-isolation` | Keep nested cargo/coverage builds from interfering |
| `npm-github-publish` | Publish npm packages via GitHub OIDC Trusted Publishers |
| `pr-triage` | End-to-end GitHub PR triage, review, and merge |
| `skill-creator` | Scaffold, validate, and evaluate new skills |
| `skill-distiller` | Compress verified traces and fixes into reusable skills |
| `spike-runner` | Isolated de-risking spikes for uncertain APIs or integrations |
| `tokio-performance` | Workload-aware async rules and review flags |

## Progressive disclosure

Never read every `SKILL.md` up front. The catalog is a metadata index first:

1. `do-harness skills suggest --query "<task>" --limit 5` ranks skills by
   frontmatter metadata only (offline, deterministic).
2. Inspect the candidate metadata (name, description, path).
3. Select the one or two skills whose contract matches the task.
4. Read only the selected `SKILL.md` plus the `references/` files it names.

## Authoring and grading

New skills go through `skill-creator`: scaffold → write `SKILL.md` with
frontmatter (`name`, `description`) → add graded `evals/evals.json` cases
(explicit, implicit, contextual, and at least one negative out-of-scope case)
→ hermetic `evals/walkthrough.sh` → `quick_validate.py` → `do-harness eval
--skill <name>` → `--bless` with an approver. CI runs `eval
--strict-fixtures` so thin datasets fail instead of passing silently.

`do-harness overlap` warns when two skills drift past the accepted similarity
baselines in `plans/invariants.json`. `skills drift` checks manifest-managed
shared skills against their pinned commit and content digest (see
`docs/cli.md`). Both are advisories; the `skills` sensor (sync) is the
gate.

## Shared safety skills and drift management

Adopting repositories can import and pin shared safety skills—such as local context
sanitization (`private-data` or `fail-closed-boundary` from `d-oit/do-context-shield`)—by
declaring them in `skills-manifest.toml` (under `.agents/`):

```toml
[[skills]]
name = "private-data"
path = ".agents/skills/private-data"
upstream = "d-oit/do-context-shield"
upstream_path = ".agents/skills/private-data"
version = "1.0.0"            # informational; never compared
commit = "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0"  # immutable 40-hex commit
content_sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855" # content digest
scope = "local_context_sanitization" # optional execution scope pattern
trigger = "pre_prompt_formatting"    # optional trigger condition
mode = "required"            # optional enforcement classification ("required" | "advisory")
required = true              # optional boolean flag
```

`do-harness skills drift` verifies local copies against these immutable pins without network
access or auto-updating. Drift is reported without silently overwriting locally customized skill
files, and moving branch references (such as `main` or `HEAD`) are rejected to ensure reproducible,
tamper-evident skill inputs.

> **Guidance vs. Runtime Integration Proof:**
> Importing or pinning a shared safety skill installs feedforward guidance (markdown rules and
> instructions) for agents and developers into `.agents/skills/`. It **does not prove or guarantee**
> that the adopting application actually routes prompts or data through the referenced security
> control at runtime (such as `do-context-shield` runtime proxies, sanitizers, or filters). End-to-end
> client integration proof requires runtime verification and testing within the adopting application.

## Auto-update (no stale docs)

`scripts/check-skills-docs.sh` keeps this page and the `README.md` Skills
section honest without loading a single skill body: it compares frontmatter
`name:` lines against the catalog table above, verifies every repo-relative
path named here exists, and checks the README pointer. After adding,
renaming, or removing a skill, run it with `--fix` (it rewrites only the
table rows; new rows arrive with an empty purpose cell for you to fill —
names are mechanical, purposes are judgment), then run it plain to confirm.
The `skills-docs` sensor runs the plain check in CI, so drift fails the
build instead of rotting silently.

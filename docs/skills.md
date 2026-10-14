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
| `harness` | Feedforward guides + feedback sensors, self-correction and steering loops |
| `skill-creator` | Scaffold, validate, and evaluate new skills |
| `skill-distiller` | Compress verified traces and fixes into reusable skills |
| `htn-planner` | Decompose objectives into task networks with precondition guards |
| `spike-runner` | Isolated de-risking spikes for uncertain APIs or integrations |
| `event-modeler` | Type-safe event-sourced slices (commands, events, projections) |
| `fail-closed-proxy` | Fail-closed tool-call mediation with audit and metrics |
| `pr-triage` | End-to-end GitHub PR triage, review, and merge |
| `crates-io-name-check` | Verify crates.io name availability before publishing |
| `npm-github-publish` | Publish npm packages via GitHub OIDC Trusted Publishers |
| `tokio-performance` | Workload-aware async rules and review flags |
| `move-only-decomposition` | Split files at the line ceiling without behavior change |
| `nested-build-isolation` | Keep nested cargo/coverage builds from interfering |

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
shared skills (`.agents/skills-manifest.toml`) against their pinned commit
and content digest. Both are advisories; the `skills` sensor (sync) is the
gate.

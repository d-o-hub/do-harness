# dsh-do-harness — DeepSeek Harness bundle

Expose [do-harness](../../README.md) development signals to DeepSeek Harness
as one typed model-facing capability and, optionally, gate completion on
current verification evidence.

The bundle contains **no verification policy**. Signal sets, change-aware
applicability, evidence generation, freshness, and selection explanations all
live in the `do-harness` CLI; this package only translates DSH lifecycle and
model calls into fixed do-harness invocations through `ctx.subprocess` and
validates the JSON it receives.

## What it provides

- **`development_signals` tool** — one capability with three actions:
  - `run`: execute a signal set (`verify --set <set> [--changed]`), returning
    `{ state, set, failed, passed, duration_ms, vacuous }`.
  - `status`: cheap evidence freshness check
    (`status --set <set>`), returning the CLI's
    `green | red | stale | missing` document. Never runs sensors.
  - `explain`: change-aware selection report
    (`explain --set <set> [--changed]`). Never runs sensors.
- **Strict completion gate** (opt-in) — on `agent/turn-stopping` the bundle
  runs only the cheap `status` operation. `green` permits the stop; `stale`,
  `red`, or `missing` steers the agent with a computational, specific reason
  and the exact next command, then lets the loop continue.

The model never passes raw CLI argv: the tool schema exposes only
`action`, `set`, and `changed`.

## Install

From a checkout of this repository (prebuilt plain JavaScript; a Git install
needs no TypeScript prepare step):

```sh
dsh plugin --profile demo add ./integrations/deepseek-harness
dsh --profile demo --dump-config   # verify the do-harness-signals layer
dsh --profile demo
```

From a Git host (pin a commit for reproducibility):

```sh
dsh plugin --profile demo add github:your-org/do-harness#<sha>
```

Remove it at any time; the repository and the `do-harness` CLI keep working
exactly as before:

```sh
dsh plugin --profile demo remove dsh-do-harness
```

No `.deepseek/` files are required. The repository keeps its normal
`AGENTS.md`, `do-harness.toml`, and `.agents/skills/`, which DSH's own
instruction and skill infrastructure consumes independently.

## Configuration

The bundle's `cordis.patch.yml` declares defaults; a profile can override the
row by restating every key it needs (patches replace the whole `config`
value):

```yaml
- id: do-harness-signals
  config:
    binary: /abs/path/to/do-harness   # default: $DO_HARNESS_BIN or `do-harness` on PATH
    workspace: /abs/path/to/repo      # default: the harness process working directory
    defaultSignalSet: feedback
    completionSignalSet: verification
    strictCompletion: true
    maxContinuations: 3
```

Binary discovery fails loudly at plugin load: if `do-harness` cannot run, the
plugin refuses to mount with an actionable message. It never compiles or
installs Rust binaries.

## Loop safety

The completion gate tracks consecutive identical outcomes per agent:

- A different state/signature resets the counter.
- After `maxContinuations` unchanged refusals, the gate disables itself for
  that agent, logs the reason, and steers one final message naming the
  underlying failure. It cannot create an infinite stop/continue loop, and it
  does not silently allow completion.
- Infrastructure or configuration errors (missing binary, unknown signal set,
  unreadable evidence) are surfaced with the underlying message and bounded
  the same way.

## Tests

The unit tests exercise the subprocess seam with scripted outcomes; the e2e
tests drive the real `do-harness` binary (no mock of the state transitions
they prove):

```sh
cargo build -p do-harness
DO_HARNESS_BIN="$PWD/target/debug/do-harness" \
  node --test integrations/deepseek-harness/test/*.test.mjs
```

E2E coverage: `green -> edit -> stale -> verify -> green`, red evidence
blocking completion, and a docs-only change selecting only the docs signal.

## Validation

Validated end-to-end against DeepSeek Harness `0.1.5-rc.1` (Node 24, pnpm 10):

- `dsh plugin --profile web add ./integrations/deepseek-harness` links the
  bundle and appends it to `dsh.profile.bundles`; `dsh --profile web
  --dump-config` shows the composed layer and profile row overrides.
- A real `dsh web --no-open` boot mounts the plugin and probes the configured
  binary through `ctx.subprocess` (verified with an argv-recording wrapper).
- A minimal real Cordis context (`@deepseek-ai/dsh-system-prompt` →
  `dsh-tools` → `dsh-subprocess-local` → this bundle) lists
  `development_signals` via `ctx.tools.schemas()` with its exact parameter
  schema, and `ctx.tools.execute()` returns the parsed `status`/`explain`
  values produced by the real CLI.
- With a missing binary, `apply` rejects and `development_signals` is not
  registered; Cordis contains the plugin error and the host keeps running.


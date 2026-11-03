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

<!-- example: id=dsh_plugin_add classification=illustrative reason="DSH plugin add command" -->
```sh
dsh plugin --profile demo add ./integrations/deepseek-harness
dsh --profile demo --dump-config   # verify the do-harness-signals layer
dsh --profile demo
```

From a Git host (pin a commit for reproducibility):

<!-- example: id=dsh_plugin_add_git classification=illustrative reason="DSH plugin add git command" -->
```sh
dsh plugin --profile demo add github:your-org/do-harness#<sha>
```

Remove it at any time; the repository and the `do-harness` CLI keep working
exactly as before:

<!-- example: id=dsh_plugin_remove classification=illustrative reason="DSH plugin remove command" -->
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

<!-- example: id=dsh_config_yaml classification=parser-check-only reason="DSH configuration YAML snippet" -->
```yaml
- id: do-harness-signals
  config:
    binary: /abs/path/to/do-harness   # default: $DO_HARNESS_BIN or `do-harness` on PATH
    workspace: /abs/path/to/repo      # default: the harness process working directory
                                      # (a relative value resolves against it)
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

Steering messages name their producer directly (`source.kind: "do-harness"`).
Session format v4 refuses the retired `{ kind: "plugin", plugin: … }` wrapper,
and on a real host that shape made every steer throw and turned the stop
boundary into an agent error. The unit suite pins the accepted shape.

## Completion gating conformance

Mounting successfully is not evidence that a client distinguishes blocked work
from successful completion, and a fake context cannot prove it. The real-host
runner drives the whole lifecycle through a version-pinned client and records
what the host actually did in the versioned
[completion-integration v1 result](../../schemas/completion-integration-v1.schema.json).

<!-- example: id=dsh_conformance_run classification=illustrative reason="real-host conformance command" -->
```sh
node integrations/deepseek-harness/conformance/run.mjs \
  --host /path/to/install/with/@deepseek-ai/dsh \
  --binary "$PWD/target/debug/do-harness" \
  --json completion-conformance.json
```

The runner:

- pins the client (`deepseek-harness` `0.1.7-rc.2`) and refuses another version
  unless `--allow-version` records an explicitly unpinned observation;
- creates a disposable Git repository with a one-sensor `do-harness.toml` and
  runs the real `do-harness` binary for every evidence state;
- replaces the model with a scripted, offline provider and drives real tool
  calls (`conformance_probe` for shape edits, `development_signals` for
  sensors), so plugin loading, the tool runtime, subprocess execution, the
  agent loop, and the stop boundary are the client's own code;
- records the nine required scenarios: `fresh-pass`, `green-then-edit`,
  `repair-and-reverify`, `failing-sensor`, `missing-or-malformed-evidence`,
  `infrastructure-error`, `retry-exhaustion`, `plugin-load-failure`, and
  `user-cancellation`.

It launches no paid model, contacts no network service, and writes only fixed
outcomes with a synthetic model identity. No prompts, tool arguments,
credentials, or child streams are recorded. Ordinary CI never runs it: it needs
a pinned client install.

Exit codes are 0 unless `--required` is passed; `--required` exits 1 for a
blocked result and 2 for an unsupported/unknown one, so a required integration
fails closed. `--negative` runs the stale-evidence scenario with
`strictCompletion: false` and asserts the suite reports a failure: a
conformance run that passes without the stop integration proves nothing.

The result is deliberately separate from the client-boundary (privacy)
conformance result: sanitization evidence says nothing about completion.

### What the host owns

The adapter interprets the client's lifecycle; it does not verify anything.
`do-harness` owns sensors, evidence, freshness, and status. The client owns
continuation, cancellation, permissions, process lifecycle, and transport.
Red, stale, missing, malformed, and infrastructure-error outcomes must remain
unverified, and only an explicitly enabled protected job may use a real model.

## Tests

The unit tests exercise the subprocess seam with scripted outcomes; the e2e
tests drive the real `do-harness` binary (no mock of the state transitions
they prove):

<!-- example: id=dsh_tests_cmd classification=illustrative reason="DSH tests execution command" -->
```sh
cargo build -p do-harness
DO_HARNESS_BIN="$PWD/target/debug/do-harness" \
  node --test integrations/deepseek-harness/test/*.test.mjs
```

E2E coverage: `green -> edit -> stale -> verify -> green`, red evidence
blocking completion, and a docs-only change selecting only the docs signal.

## Validation

Adapter-level checks, validated against DeepSeek Harness `0.1.5-rc.1`
(Node 24, pnpm 10):

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

Real-host completion conformance against DeepSeek Harness `0.1.7-rc.2`
(2026-11-03): four scenarios pass — `fresh-pass`, `repair-and-reverify`,
`plugin-load-failure`, and `user-cancellation`. Five fail with
`blocked-outcome-absent` (plus `infrastructure-error` for its own scenario):
the gate steers and the turn continues, but once the continuation budget is
spent the host closes the session as a successful completion while evidence is
stale, red, missing, or unavailable. The result is therefore
`terminal_result: blocked`, `verdict: fail`.

That is the honest state of the pinned client: it has no blocked terminal
distinction yet. Emitting one (a cancelled/aborted session instead of a
completed one) is tracked by
[#454](https://github.com/d-o-hub/do-harness/issues/454); this runner turns
green only when the client reports the blocked outcome it observes. The
adapter-level e2e tests remain fast, offline, and labeled separately from these
real-host observations.


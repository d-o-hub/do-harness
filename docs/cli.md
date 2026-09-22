# do-harness CLI Reference

`do-harness` is a unified CLI tool for agent execution harness sensors, task workflows, skill evaluation, and database maintenance.

## Usage

```bash
do-harness [GLOBAL OPTIONS] <SUBCOMMAND>
```

## Global Options

- `--root <DIR>`: Workspace root override (default: walk up from cwd).
- `--config <FILE>`: Explicit path to `do-harness.toml`.
- `-v, --verbose`: Increase verbosity level.
- `-q, --quiet`: Suppress non-error output.
- `--color <WHEN>`: Output colorization (`auto`, `always`, `never`).
- `--output <FILE>`: Default output file path.
- `--dry-run`: Dry run without applying side effects.

## Subcommands

### `version`
Print the binary's identity: name, version, commit, commit date, and whether the
working tree was dirty when it was built.

- `--format <Format>`: Output format (`text` or `json`); the JSON shape is
  `{ name, version, commit, commit_date, dirty }`.

`doctor` compares the same information against the state database to flag a
binary that predates the schema.

### `verify` (alias: `check`)
Runs computational sensors defined in `do-harness.toml`.

- `--fail-fast`: Stop execution at the first failing sensor.
- `--format <Format>`: Output format (`text` or `json`).
- `--set <SET>`: Run only the sensors in this development signal set
  (`feedback`, `verification`, `release`, or a custom `[signal-sets]` entry).
  `--only` narrows within the set and `--exclude` removes from it; a sensor
  outside the set is a usage error. JSON output carries the selected
  `"signal_set"`.
- `--changed`: Run only sensors applicable to the working-tree change. Each
  sensor's `when-changed` globs (repository-relative, `**` supported) select
  it when a changed file matches; sensors without `when-changed` always run.
  Change discovery covers staged, unstaged, deleted, renamed, and untracked
  files; when repository state cannot be determined, every sensor runs
  (fail-closed, never silently skipped).
- `--only <SENSOR>`: Run only named sensor(s). Repeatable or comma-separated.
- `--exclude <SENSOR>`: Exclude named sensor(s). Repeatable or comma-separated.
- `--jobs <N>`: Run up to `N` sensors concurrently (overrides `jobs` in
  `do-harness.toml`; default 1 = sequential). Results always report in config
  order, so text and JSON output are deterministic. `--fail-fast` cancels
  in-flight siblings and never starts a later chunk. A `jobs = 0` config is
  rejected at load.
- `--record`: Persist beats and error signatures into `.do-harness/agent_state.db`.
- `--task <ID>`: Scope recorded beats to task ID. Requires `--record`. An
  unscoped `--record` prints a global-namespace advisory; managed git hooks
  export `DO_HARNESS_HOOK=1` (a hook has no task context) and skip it.
- `--evidence <FILE>`: Write machine-readable evidence artifact JSON.
- `--strict`: Exit non-zero if any sensors were skipped or evidence checks fail. Default evidence artifact path: `.do-harness/evidence.json`.
- `--bless`: Lower or initialize blessed findings baselines from this run
  (requires `--record`; a bless never raises a baseline).
- `--approver <NAME>`: Approver identity recorded with `--bless` (defaults to
  `DO_HARNESS_APPROVER` or the git user email). Requires `--bless`.

A sensor is the execution primitive (one `argv` command). A signal set is a
development decision composed from sensors: `feedback` for the edit/fix loop,
`verification` as the mandatory pre-completion gate, `release` for
release-time checks. Sets are declared in `[signal-sets]` and refer to sensor
names; they never contain command lines. Unknown set names, dangling sensor
references, and duplicate entries fail config validation loudly. Configs
without `[signal-sets]` keep legacy behavior: `verification`/`release`
resolve to the full sensor list, `feedback` has no implicit guarantee and is
rejected, and plain `verify` is unchanged.

A `verify --set <name>` run writes its evidence artifact to
`.do-harness/evidence.<name>.json` by default, so a feedback run can never
clobber verification evidence. Runs without `--set` keep the legacy rule
(artifact only for `--evidence` or `--strict`).

Each sensor carries a gate severity. `severity = "error"` (the default) fails
the run; `severity = "warn"` makes failures advisory: a non-strict run prints
`WARN` and stays green, while `--strict` promotes the failure outside the
`feedback` set. The deprecated `allow_failure = true` alias maps to
`severity = "warn"`; setting both inconsistently is a config error.

A sensor may print a `FINDINGS: <n>` marker (last occurrence wins) to opt into
the findings ratchet. Blessed ceilings live in the committed
`plans/baselines.json` (`{"sensors": {"<name>": <max>}}`), which `verify`
reads and `verify --record --bless` only ever lowers or initializes. A count
above the baseline is a hard regression (fails even warn-severity sensors); a
non-zero count within the baseline prints `WARN` with the findings/baseline
delta. `verify --record` persists observed maxima and bless history to the
state database; `status` goes stale when the baseline file changes because its
digest is part of the policy fingerprint.

Advisory failures are recorded in evidence as `warn` (never `pass`), so the
evidence summary stays non-pass and `--strict` rejects it: softness applies to
the developer loop, never to the evidence artifact. Warn-severity sensors that
warn `3` consecutive times under `--record` are quarantined — skipped with an
advisory `WARN` verdict instead of halting the run — until a passing run or
`errors clear` resets their strikes.

Evidence schema v4 records each sensor's exact `argv` and a SHA-256 of its
captured output, the selected `signal_set`, the post-run workspace
fingerprint (content hashes of staged, unstaged, deleted, renamed, and
relevant untracked files — never mtimes, and never harness-owned
`.do-harness/` state), the policy fingerprint (raw config bytes, harness
version, set, full sensor definitions including `when-changed`, plus the
blessed findings baseline and coverage-inputs digests), and the
change-aware `skipped` sensors with reasons. Sensors that declare
`artifacts = ["glob", ...]` get every match SHA-256 digested into the sensor
record (a declared glob that matches nothing degrades the sensor to `warn`);
sensors that print a `COVERAGE: <json>` marker get the manifest aggregated
under `coverage` by sensor name. Sensors that declare
`coverage-inputs = [...]` feed those files into the policy fingerprint, so
editing the matrix definition makes evidence stale without re-running.
Artifacts chain to the previous file at the same path
(`chain_hash`/`prev_hash`) so tampering or reordering is detectable. CI
uploads only the artifact and a `Cargo.lock` hash; the local database is not
uploaded and can be pruned with `maintenance`. Schema v3 artifacts (no
artifacts/coverage) stay readable and current; pre-fingerprint (v2 or older)
artifacts are legacy (missing).

Schema v2 artifacts (no fingerprints) are still chain-linked when a v4 run
overwrites them, but `status` treats them as legacy (missing), never as
current evidence. Schema v3 artifacts parse into v4 with empty
`artifacts`/`coverage` and stay current.

#### Generic Artifact Provenance Sensor Contract (`artifact-provenance`)
A generic, stack-neutral contract for verifying release-artifact provenance without hard-coding language toolchains into the harness.
- **Sensor Name**: `artifact-provenance` (or any custom sensor name adhering to the contract).
- **Required Inputs**:
  - `ARTIFACT_PATHS`: Space- or comma-separated list/globs of target artifact file paths.
  - `EXPECTED_DIGEST`: Optional expected hex SHA-256 digest of the artifact.
  - `VERIFY_MODE`: Verification mode. `digest-only` (default) is implemented; `slsa`, `github-attestation`, `sbom`, and `strict` are reserved and fail closed until implemented, so a configured mode never reports a pass it did not verify.
- **Required Evidence Output**:
  Sensors implementing this contract output a `COVERAGE: <json>` line carrying structured JSON:
  ```json
  {
    "artifact": "path/to/artifact",
    "digest": "sha256_hex_digest",
    "status": "pass",
    "reason": null
  }
  ```
  On verification failure, `status` is set to `"fail"` and `reason` details the cause (e.g. `"digest mismatch"`, `"missing artifact"`, or `"verification mode 'sbom' is reserved and not implemented"`).

#### Rust Binary Provenance Sensor Contract (`rust-binary-provenance`)
The Rust-specific implementation of the generic artifact-provenance contract: it verifies already-built binaries with `cargo audit bin`, which reads the dependency metadata `cargo auditable` embeds at build time. Configure it in the repository that publishes auditable binaries; the harness builds nothing itself.
- **Sensor Name**: `rust-binary-provenance` (script: `scripts/check-rust-binary-provenance.sh`).
- **Required Inputs**:
  - `--artifact <PATH(S)>` or `ARTIFACT_PATHS`: Target binary path(s), space- or comma-separated.
  - `--digest <HEX_SHA256>` or `EXPECTED_DIGEST`: Optional expected hex SHA-256 digest, applied to each artifact.
  - `--strict`: Treat an unavailable `cargo-audit` as a failure instead of a skip.
  - `CARGO_AUDIT_BIN`: Optional cargo-audit binary override, invoked the way cargo invokes a subcommand (`<bin> audit bin <paths…>`).
- **Fail-closed behavior**: a missing artifact, an unavailable sha256 tool, a digest mismatch, absent auditable metadata (no `.dep-v0`/`__dep` section), an unavailable `cargo-audit` under `--strict`, and a tool failure all set `status` to `"fail"`. Metadata presence is probed from the binary itself: `cargo-audit` 0.22 exits 0 on a binary without auditable metadata after recovering a partial dependency list from panic messages, so its exit code cannot stand in for the check.
- **Required Evidence Output**:
  Outputs a `COVERAGE: <json>` line carrying structured JSON, with one entry per artifact:
  ```json
  {
    "artifact": "path/to/binary",
    "digest": "sha256_hex_digest",
    "status": "pass",
    "reason": null,
    "tool": "cargo-audit 0.22.2",
    "auditable_metadata": true,
    "findings": 0,
    "artifacts": [
      { "path": "path/to/binary", "digest": "sha256_hex_digest", "auditable_metadata": true }
    ]
  }
  ```
  `findings` counts advisory ids (`RUSTSEC-YYYY-NNNN`) so a tool or setup failure stays distinguishable from a vulnerability finding, and the `FINDINGS: <n>` marker carries the same count.

#### Coverage Sensor (`check-coverage.sh`)

`scripts/check-coverage.sh` reports two layers: a behavior inventory and the
line/branch proof from `cargo-llvm-cov nextest`.

- **Inventory (fast path).** `check-coverage.sh inventory` prints a deterministic
  table of unique compiled behavior as `unique(file, fn)` per path-based layer —
  `crates/*/src` (crate owners), `crates/*/tests` (crate integration), `src/`
  (root facade), and `tests/` (root integration). It executes no tests and needs
  only `python3`; `target/`, `.git/`, `.do-harness/`, `.agents/`, and
  `node_modules/` are skipped, and `tests/fixtures/` never counts as behavior.
- **Proof path.** `check-coverage.sh` (or `llvm-cov`) prints the same inventory,
  then runs `cargo llvm-cov nextest --lcov --output-path lcov.info` and derives
  the line percentage from the report's `LH`/`LF` totals — never from a stdout
  summary, because `--lcov` prints none. Branch records (`BRH`/`BRF`) are
  opportunistic input: the sensor passes no `--branch` (the flag is unstable and
  needs a nightly toolchain), so on the pinned stable channel the report carries
  none and the line-only verdict is the expected output; a report that does carry
  them gets the branch percentage printed beside the line one.
- **Ratchet contract.** The verdict stays numeric for `verify --record --bless`:
  `FINDINGS: <deficit>` is the line-percentage deficit against `TARGET_PCT=70`,
  and `FINDINGS: 0` above target. A branch percentage never changes that number,
  and a report with no line counts keeps the `WARN … FINDINGS: 1` contract
  instead of passing silently.
- **Tool contract.** A missing `cargo-llvm-cov`/`cargo-nextest` exits 0 with a
  `SKIP:` line locally and fails closed when `CI=true` or
  `DO_HARNESS_REQUIRE_TOOLS=1`.
- The script measures the workspace that contains it; pass `inventory <dir>` or
  `llvm-cov <dir>` to point it at another workspace.
- Test-LOC counts replace neither layer: the inventory answers *what behavior,
  where*, and the llvm-cov run is the coverage proof.

### `status`
Reports verification evidence freshness for a signal set without executing
any sensor: `green` (current passing evidence covers the set), `red`
(matching evidence contains required failures), `stale` (evidence exists but
the workspace or policy changed), or `missing` (no qualifying evidence).
Stronger evidence can satisfy a weaker set when it shares the config
fingerprint and covers every required sensor. Exit 0 on green, 1 on
red/stale/missing, 2 on usage/config errors.

- `--set <SET>`: Check evidence for this signal set (default evidence file
  `.do-harness/evidence.<set>.json`; default `.do-harness/evidence.json`
  without `--set`).
- `--evidence <FILE>`: Read evidence from this file instead.
- `--format <Format>`: Output format (`text` or `json`).

### `list` (alias: `ls`)
Lists configured sensor names, or with `--sets` the selectable development
signal-set names (declared `[signal-sets]` keys, or `verification`/`release`
for legacy configs).

- `--sets`: List signal-set names instead of sensor names.
- `--format <Format>`: Output format (`text` or `json`).

### `explain`
Explains which sensors the current change selects and why, without executing
any sensor. Reports the changed files plus every selected sensor (with its
matching pattern or constraint reason) and every skipped sensor.

- `--set <SET>`: Explain this development signal set (default: all sensors).
- `--changed`: Select by working-tree change instead of listing everything.
- `--format <Format>`: Output format (`text` or `json`).

### `init-db`
Apply pending migrations to `.do-harness/agent_state.db`, the local state store
(beats, traces, heuristics, evaluations).

- `--check`: Report pending migrations and exit non-zero when any are pending.
- `--dry-run`: Report pending migrations without applying them (exit 0).
- `-y`, `--yes`: Skip the interactive confirmation prompt.

`doctor` fails when the database outruns the binary; `init-db --check` is the
machine-readable half of that check.

### `seed`
Upsert `plans/invariants.json` into the `invariants` table — the machine-readable
half of the architecture-decision contract (invariant, rationale, sensor).

- `--prune`: Delete rows whose invariant no longer appears in
  `plans/invariants.json`.
- `--dry-run`: Report what would be written without writing.

Run `seed --prune` after editing `plans/invariants.json` so the table cannot
drift from the file.

### `init`
Scaffolds a harness workspace in the target directory and proves the
generated contract before presenting the repository as ready.

Init detects repository reality (Cargo manifest/workspace, git repository,
package markers), probes each candidate sensor's required tooling, generates
a `do-harness.toml` containing only proven signals (signal sets and hooks are
derived from the included subset), writes the portable skill subset
(`harness` plus `skill-creator`), and then runs the effective sensors once:

- `Initial verification: GREEN` — every configured sensor passed.
- `RED` — a required check failed; the command exits 1 and prints bounded
  failure output instead of claiming a verified workspace.
- `VACUOUS` — the generic pack has zero sensors; the pass is explicitly not
  evidence.

The generated `AGENTS.md` embeds a pinned installer command so an agent
landing in the repository can obtain the exact CLI version it was initialized
with.

Missing required tooling (e.g. `cargo fmt` or `cargo clippy`) is surfaced and
omitted from the generated config. Missing optional tools (`cargo-deny`,
`cargo-audit`) degrade the script-backed `deps`/`audit` sensors, which fail
open locally and are enforced in CI.

Existing files are never rewritten without `--force`, and existing
application source (including `Cargo.toml`) is never touched. The
development-methodology skills used to build do-harness itself (HTN planning,
spikes, event modeling, skill distillation) are not scaffolded into adopting
projects.

- `--language <LANG>`: Force a pack (`rust`, `generic`, `web`, `node`); default
  detects from the repository (empty/hidden-only directories bootstrap Rust).
- `--force`: Overwrite existing files.
- `--format <Format>`: Output format (`text` or `json`); JSON carries
  `detected`, `candidates`, `written`, `skipped`, and `baseline`.
- `--no-seed`: Skip seeding `plans/invariants.json`.
- `--minimal`: Skip skill scaffolding.
- `--no-gitignore`: Skip creating/modifying `.gitignore`.

### `task`
Task state inspection and workflow management.
- `task export [--output <FILE>] [--stdout] [--format <Format>]`: Export task snapshot.
- `task list [--status <STATUS>] [--method <METHOD>] [--parent <ID>] [--format <Format>]`: Print tasks and board summary.
- `task show <ID> [--format <Format>]`: Display task details.
- `task add <TITLE> [--method <METHOD>] [--parent <ID>] [--precondition <GUARD>]`: Add a new task in pending state. Method must exist in `plans/methods.json`.
- `task advance <ID>`: Advance task subtask pointer.
- `task done <ID>`: Mark task completed after all sensor gates pass.
- `task fail <ID>`: Mark task failed.
- `task remove <ID>`: Remove an orphan task row that has no workflow events. A task with history is refused, because the event log is append-only and hash-chained; use `task fail` to close it instead.
- `--dry-run` is a global flag and applies to every mutating action above (`add`, `advance`, `done`, `fail`, `remove`, `export`); each prints what it would do and writes nothing.

### `doctor`
Diagnostic health checks covering binary resolution, git hook status, and database migration skew.

- `--format <Format>`: Output format (`text` or `json`).
- `--strict`: Elevate warnings (target/ binary, missing hooks) to failures.

### `eval`
Skill structure validation and evaluation benchmark runner.

- `--skill <SKILL>`: Restrict evaluation to target skill directory.
- `--bless`: Re-baseline grader hashes and raise pass-rate floor on green run.
  Every bless appends an immutable `skill_eval_blesses` row recording the
  approver and timestamp; an approver is required (`--approver`,
  `DO_HARNESS_APPROVER`, or the git user email).
- `--approver <NAME>`: Identity recorded with `--bless`.
- `--no-lift`: Skip the without-skill baseline run (no Skill Lift measured).
- `--agent-cmd <COMMAND>`: Run this shell command once per eval case instead
  of the deterministic walkthrough (true Skill Lift). cwd is the sandbox root;
  the prompt is in `$DO_HARNESS_PROMPT` and on stdin; stdout is saved to
  `agent_stdout.txt` for assertions.
- `--agent-timeout <SECS>`: Kill an agent run after this many seconds
  (default 600).
- `--strict-fixtures`: Fail skills whose fixture has dataset-quality gaps
  (no cases, an ungraded case, or no negative out-of-scope case). CI runs
  this so thin datasets cannot silently return.
- `--list-skills`: List discovered skills under `.agents/skills`.
- `--fail-fast`: Halt evaluation on first failing skill.
- `--dry-run`: Perform dry-run evaluation without updating state.
- `--format <Format>`: Output format (`text` or `json`).

Failing runs print one `CASE-FAIL:` line per failing assertion in text mode
(case id, kind, dimension, assertion spec, and the grader reason); `--format
json` carries the same detail per skill under `cases[].assertions[]`, so a
sub-1.00 pass rate is diagnosable without re-grading by hand.

**Sandbox boundary:** walkthroughs, agent commands, and graded assertions
execute in a `tempfile` filesystem sandbox only. There is no seccomp/netns/
gVisor isolation; children run with the caller's privileges. Treat skill
walkthroughs and `--agent-cmd` commands as untrusted code and wrap the whole
command in an outer sandbox if needed.

### `overlap`
Rank skill pairs by guidance overlap and warn at or above the cosine threshold —
a Tier-2 distinctiveness advisory: accepted overlaps are recorded in
`plans/invariants.json`, and a new pair crossing the threshold signals two guides
drifting together.

- `--threshold <THRESHOLD>`: Cosine similarity at or above which a pair prints
  as `WARN` (default `0.45`).
- `--format <Format>`: Output format (`text` or `json`).

Text output is one line per pair (`skill-creator <-> skill-distiller sim=0.765
shared=[…]`), sorted by similarity; the command is advisory and exits 0.

### `distill`
Extracts a heuristic from a resolved trace into a skill.

Review distilled heuristics and code snippets against the advisory Anti-AI-Slop checklist (`.agents/skills/skill-creator/references/anti_ai_slop.md`): purpose-per-struct, real error handling, meaningful domain names, no speculative abstractions, and why-focused docs.

- `--skill <SKILL>`: Target skill directory name.
- `--pattern <PATTERN>`: Generalized heuristic pattern.
- `--description <DESC>`: When the pattern applies.
- `--from-trace <ID>`: Required source trace ID as evidence.
- `--to-fixture`: Raise skill pass-rate floor after recovery (review ticks the anti-AI-slop checklist before raising the bar).
- `--dry-run`: Perform dry run without modifying files.
- `--format <Format>`: Output format (`text` or `json`).

### `errors`
Inspect and clear fail-fast error signatures.

- `errors list [--task <ID>] [--format <Format>]`: List open error signatures.
- `errors clear [--sensor <SENSOR>] [--task <ID>] [--force] [--dry-run]`: Clear error signatures.

### `trace`
Record and query command execution traces.

- `trace add --session <SESSION> [--task <ID>] [--command <CMD>] [--error-diff <DIFF>] [--resolution-steps <STEPS>]`: Record trace.
- `trace list --session <SESSION> [--format <Format>]`: Print traces for a session.
- `trace sessions [--format <Format>]`: List distinct session identifiers.

### `hook`
Git hook management (`pre-commit`, `pre-push`, `commit-msg`).

- `hook install [--force]`: Write managed git hooks.
- `hook uninstall`: Remove managed hooks.
- `hook status [--format <Format>]`: Report hook installation status.
- `hook diff`: Show diff between installed hooks and templates.

### `metrics`
Longitudinal trends: sensor stats, strikes, and skill pass rates.

- `--format <Format>`: Output format (`text` or `json`).
- `--sensor <SENSOR>`: Filter by sensor name.
- `--skill <SKILL>`: Filter by skill name.
- `--since <UNIX_SECONDS>`: Filter metrics since a Unix timestamp in seconds.

### `loc`
Report line-of-code state against the 500-line invariant: one line per file with
`lines/500`, the band (`OK`, `WARN` at or above the 450-line decomposition
threshold, `FAIL` above the ceiling), and the code/test split when an inline
`#[cfg(test)]` module is present.

- `[PATH]...`: Files or directories to measure (default: every `.rs` under
  `crates/`).
- `--warn`: Show only files at or above the decomposition threshold.
- `--format <Format>`: Output format (`text` or `json`).

### `split`
Extract one large top-level item — or an inline `#[cfg(test)]` module — into a
sibling module, emitting the declaration and re-exports so the tree still
compiles. A file already under the ceiling is reported, not modified, and shapes
the tool cannot make compile are refused with a reason.

- `--target <NAME>`: Name the sibling module instead of deriving it from the
  item.
- `--dry-run`: Print the plan without writing files.

### `maintenance`
Prune unbounded history and compact the local state database.

- `--prune-beats <DAYS>`: Delete beats older than the cutoff, keeping at least
  `--keep-per-task` most-recent beats per task (task-less beats form their own
  partition).
- `--keep-per-task <N>`: Minimum most-recent beats retained per task when
  pruning (default 20).

`VACUUM` always runs after the optional prune. Recommended retention: run
`maintenance --prune-beats 30` on a schedule; `skill_eval_runs` history is
retained in full locally (it is small) but CI only uploads the current
`.do-harness/evidence.json` artifact, not the database.

### `compliance`
Compliance mapping to OWASP Agentic Top 10, NIST AI RMF, EU AI Act, and SOC 2.

- `--framework <FRAMEWORK>`: Filter framework (`owasp`, `nist`, `eu`, `soc`).
- `--format <Format>`: Output format (`text` or `json`).

### `audit-chain`
Verify event hash chain integrity in `.do-harness/agent_state.db`.

- `--format <Format>`: Output format (`text` or `json`).

### `dora`
Derive the four DORA deployment metrics — deployment frequency, lead time for
changes, change failure rate, and time to restore service — from git history.
Every number is **derived from persisted history and the pinned policy in
`plans/dora.json`**, never judged: the same input yields byte-identical output,
and each snapshot carries its own derivation manifest (resolved window, ref
ranges scanned, revert predicate, percentile method, incidents) so a number can
never be read without its provenance. Read-only apart from an optional snapshot
row.

- `--days <DAYS>`: Rolling window in days (overrides `window_days` in the
  policy for this run only; the file is never rewritten).
- `--format <Format>`: Output format (`text` or `json`).
- `--record`: Persist the snapshot into `.do-harness/agent_state.db`
  (`dora_snapshots`).
- `--source <git\|gh>`: `git` (default) is hermetic and offline; `gh` adds an
  opt-in GitHub enrichment object (release-run reconciliation plus PR-open →
  merge lead time) and is never used in CI, hooks, or a sensor argv.
- `--now <UNIX_SECONDS>`: Inject the measurement clock so runs are reproducible.

`policy_fingerprint` is `sha256:` + the digest of the raw `plans/dora.json`
bytes, and `plans/dora.json` is declared as the `dora` sensor's
`coverage-inputs`, so editing a threshold makes prior DORA evidence `stale`
(reason `policy_changed`) instead of silently re-scoring the same history.

Exit `0` when no threshold is breached, `1` when at least one is (the breach
count travels in the `FINDINGS:` marker for the blessed ratchet), and `2` for a
usage/config/discovery problem — including a shallow clone whose ranges cannot
be resolved. A collector that cannot read git history never reads as healthy.

### `pr no-effect`
Report whether a pull request or revision range introduces any effective change,
using the merge-base tree delta. Read-only; works in any git repository without
harness initialization.

- `<PR>`: Pull request number; base and head are resolved through `gh`.
- `--base <REV> --head <REV>`: Local mode without GitHub.
- `--format <Format>`: Output format (`text` or `json`).

Exit `0` when the verdict is determined (whether `no-effect` or `has-effect`),
`1` when it cannot be determined, and `2` for invalid arguments. An analysis
error is never reported as `no-effect`.

### `pr review`
Emit the semantic residual for a pull request or revision range: the changed
hunks (units) that evidence could not prove, with minimal context, plus the
audit list of skipped units and any untrusted `false_proven` claims. Works in
any git repository without harness initialization.

Proof skipping is driven by `.github/pr-gate.toml`, read **at the merge-base
revision only** (never the PR head):

```toml
[proof]
mechanical = ["**/Cargo.lock", "docs/generated/**"]
behavioral = ["crates/**", "src/**"]
```

A unit is moved to `skipped` when its path matches a `mechanical` glob and no
`behavioral` glob, or when the change is structural (rename-only or mode-only).
Behavioral matches always stay residual; a mechanical claim contradicted by a
behavioral rule (or by the protected policy path itself) is revoked, kept
residual, and recorded in `false_proven`. Absent, malformed, or partially
invalid policy — including invalid globs — proves nothing and adds a warning.

- `<PR>`: Pull request number; base and head are resolved through `gh`,
  falling back to `gh pr diff` when the clone cannot resolve them.
- `--base <REV> --head <REV>`: Local mode without GitHub.
- `--recompute`: Ignore the cached report and recompute from scratch.
- `--format <Format>`: Output format (`text` or `json`).

Exit `0` when a report is produced (even with a full residual), `1` when the
revisions or diff cannot be resolved, and `2` for invalid arguments. The only
write is a best-effort cache under the repository git directory; the command
never writes into the work tree.

JSON reports also carry `measurement`: `t_raw` (unified-diff bytes), `t_res`
(serialized residual payload bytes), `ratio` (`t_res / t_raw`), and `verdict`
(`reduced` when `t_res < t_raw`, otherwise `no-go`). Shared review context
cancels out; the JSON envelope counts against the residual. Consumers should
review the residual only on `reduced` and fall back to the raw diff on `no-go`.

### `skills`
Progressive-disclosure skill selection and opt-in drift checks for shared
skills. `skills suggest` ranks the skills under
`.agents/skills/*/SKILL.md` against a task description using **frontmatter
metadata only** — the catalog never reads a skill body, and ranking is offline
and deterministic.

```bash
do-harness skills suggest --query "review this pull request" --limit 5
```

- `--query <TEXT>`: Required task description.
- `--limit <N>`: Maximum candidates to return (default `5`); `0` is a usage error.
- `--format <Format>`: `text` (one `<score> <name> — <summary>` line per
  candidate) or `json`.

JSON output carries metadata only:

```json
{
  "schema_version": 1,
  "query_terms": 8,
  "catalog_size": 27,
  "candidates": [
    {
      "name": "pr-triage",
      "description": "Triage GitHub pull requests...",
      "path": ".agents/skills/pr-triage/SKILL.md",
      "score": 0.87
    }
  ],
  "warnings": []
}
```

Scoring splits the query and the skill's `name` + `metadata.short-description` +
`description` into lowercased alphanumeric tokens, computes the fraction of
query tokens present, and adds a full weight when the skill name appears as a
query token. Ties break on `(name, path)`, so repeated runs over an unchanged
catalog are byte-identical. Malformed, unreadable, and path-escaping skills
become `warnings` entries instead of failures, and duplicate names resolve to
the first path with a warning naming both.

**Optional semantic selector.** Set `DO_HARNESS_SKILL_SELECTOR` to an
executable and it is invoked with bounded candidate metadata on stdin:

```json
{"schema_version":1,"query":"…","candidates":[{"name":"…","description":"…","score":0.87}]}
```

It answers `{"schema_version":1,"selected":["…"],"confidence":0.94}`. A wrong
`schema_version`, a name outside the candidate set, duplicates, an over-long
list, a non-finite or out-of-range `confidence`, non-JSON output, a non-zero
exit, or a timeout (`DO_HARNESS_SKILL_SELECTOR_TIMEOUT`, default `10` seconds)
all fall back to the deterministic order with a warning. The selector can only
reorder candidates the deterministic stage already chose: it never sees a skill
body, cannot introduce a path, and cannot execute a skill or grant a permission.

**Cache.** Frontmatter metadata is cached at
`.do-harness/cache/skills-v1.json`, keyed by canonical `SKILL.md` path and
validated against a sha256 of the frontmatter block. A missing, corrupt,
stale-schema, or stale-scoring cache is a warning and a fresh scan, never an
error; skill bodies are never cached.

**Loading contract.** `suggest` → inspect candidate metadata → select → read
only the selected `SKILL.md` and the references it names. Defaults are five
metadata candidates and one fully loaded skill.

`crates/do-harness/src/skills/tests/` covers the algorithm and cache;
`crates/do-harness/tests/skills_suggest.rs` drives the real binary;
`scripts/skills-suggest-benchmark.sh` measures top-1/top-3/top-5 accuracy,
metadata bytes, loaded-context bytes, unnecessary-load rate, and latency across
the load-all, deterministic-only, and deterministic+selector arms.

**Drift checks (`skills drift`).** Shared skills are copied between repositories
on purpose, so their drift cannot be detected by comparing a repository against
itself. An opt-in manifest at `.agents/skills-manifest.toml` (override with
`--manifest <FILE>`) names the skills a repository manages from an upstream and
pins each one:

```toml
[[skills]]
name = "skill-creator"
path = ".agents/skills/skill-creator"
upstream = "owner/repo"
upstream_path = ".agents/skills/skill-creator"
version = "1.2.0"            # informational; never compared
commit = "<40-hex>"          # provenance anchor
content_sha256 = "<64-hex>"  # content anchor: digest of the managed tree
```

The commit pin is the full 40-hex object ID: abbreviated SHAs are rejected,
because a short prefix is ambiguous across repositories and over time.

The digest is `sha256` over one record per file — the path relative to the
managed skill directory (never the repository), a NUL byte, the file's sha256
hex, and a newline — ordered by byte-wise path, so a copy hashes alike wherever
it is checked out. Empty directories do not appear and file symlinks are
followed (the target's content is hashed). A directory symlink *inside* the
managed tree is an error because a pinned digest cannot describe it, while the
managed directory itself may be a symlink as long as its resolved target stays
under the repository root: the manifest path check is lexical, so the resolved
directory is what the check trusts. File names must be UTF-8, and a managed
directory that resolves outside the repository root is an error as well. Obtain
a pin by running the check once — the report carries the `actual` digest for
every managed tree.

```bash
do-harness skills drift                  # text: one line per managed skill
do-harness skills drift --format json    # schema_version 1 report
```

Exit codes are the verdict: `0` every managed skill matches its pin, `1` at
least one drifted or is missing (the report names the skill and its path), `2`
the manifest is absent, unreadable, or invalid (including one that lists no
skills), or a managed tree cannot be described — unreadable, containing a
non-UTF-8 file name, or resolving outside the repository root. A missing
manifest is deliberately an error rather than a vacuous pass, and the check
stays offline and check-only: it never fetches, never writes, and never
inspects a skill the manifest does not name. Adopters who
wire it in as a sensor should declare the manifest under `coverage-inputs`, so
a changed pin invalidates stale evidence instead of hiding behind it.

`crates/do-harness/src/skills/tests/drift_cases.rs` covers the digest,
validation, and status rules; `crates/do-harness/tests/skills_drift.rs` drives
the real binary's exit codes, output determinism, and unmanaged-skill
isolation.

### `completions`
Generate shell completions for `bash`, `zsh`, `fish`, `powershell`, `elvish`.

### `man`
Generate man page documentation into specified directory.

## Exit Codes

| Exit Code | Classification | Meaning |
|-----------|----------------|---------|
| `0` | Success | All requested operations and sensors succeeded. |
| `1` | Verification Failure | One or more computational sensors, evals, or audit-chain checks failed. |
| `2` | Usage / Config Error | Invalid CLI parameters, missing config/root, or database/I/O error. |

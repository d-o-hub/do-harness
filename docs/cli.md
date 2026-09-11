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
- `--record`: Persist beats and error signatures into `.do-harness/agent_state.db`.
- `--task <ID>`: Scope recorded beats to task ID. Requires `--record`.
- `--evidence <FILE>`: Write machine-readable evidence artifact JSON.
- `--strict`: Exit non-zero if any sensors were skipped or evidence checks fail. Default evidence artifact path: `.do-harness/evidence.json`.

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

A sensor configured with `allow_failure = true` keeps the local `verify` gate
green (it prints `WARN`), but its evidence verdict is still `fail` and
`--strict` rejects it: softness applies to the developer loop, never to the
evidence artifact.

Evidence schema v3 records each sensor's exact `argv` and a SHA-256 of its
captured output, the selected `signal_set`, the post-run workspace
fingerprint (content hashes of staged, unstaged, deleted, renamed, and
relevant untracked files — never mtimes, and never harness-owned
`.do-harness/` state), the policy fingerprint (raw config bytes, harness
version, set, and full sensor definitions including `when-changed`), and the
change-aware `skipped` sensors with reasons. Artifacts chain to the previous
file at the same path (`chain_hash`/`prev_hash`) so tampering or reordering
is detectable. CI uploads only the artifact and a `Cargo.lock` hash; the
local database is not uploaded and can be pruned with `maintenance`.

Schema v2 artifacts (no fingerprints) are still chain-linked when a v3 run
overwrites them, but `status` treats them as legacy (missing), never as
current evidence.

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

Missing required tooling (e.g. `cargo fmt` or `cargo clippy`) is surfaced and
omitted from the generated config. Missing optional tools (`cargo-deny`,
`cargo-audit`) degrade the script-backed `deps`/`audit` sensors, which fail
open locally and are enforced in CI.

Existing files are never rewritten without `--force`, and existing
application source (including `Cargo.toml`) is never touched. The
development-methodology skills used to build do-harness itself (HTN planning,
spikes, event modeling, skill distillation) are not scaffolded into adopting
projects.

- `--language <LANG>`: Force a pack (`rust`, `generic`); default detects from
  the repository (empty/hidden-only directories bootstrap Rust).
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
- `task advance <ID> [--dry-run]`: Advance task subtask pointer.
- `task done <ID> [--dry-run]`: Mark task completed after all sensor gates pass.
- `task fail <ID>`: Mark task failed.
- `task remove <ID>`: Remove task from state database.

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
- `--list-skills`: List discovered skills under `.agents/skills`.
- `--fail-fast`: Halt evaluation on first failing skill.
- `--dry-run`: Perform dry-run evaluation without updating state.
- `--format <Format>`: Output format (`text` or `json`).

**Sandbox boundary:** walkthroughs and graded assertions execute in a
`tempfile` filesystem sandbox only. There is no seccomp/netns/gVisor
isolation; children run with the caller's privileges. Treat skill walkthroughs
as untrusted code and wrap the whole command in an outer sandbox if needed.

### `distill`
Extracts a heuristic from a resolved trace into a skill.

- `--skill <SKILL>`: Target skill directory name.
- `--pattern <PATTERN>`: Generalized heuristic pattern.
- `--description <DESC>`: When the pattern applies.
- `--from-trace <ID>`: Required source trace ID as evidence.
- `--to-fixture`: Raise skill pass-rate floor after recovery.
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

- `--framework <FRAMEWORK>`: Filter framework (`owasp`, `nist`, `eu`, `soc2`).
- `--format <Format>`: Output format (`text` or `json`).

### `audit-chain`
Verify event hash chain integrity in `.do-harness/agent_state.db`.

- `--format <Format>`: Output format (`text` or `json`).

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

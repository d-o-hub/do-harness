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
- `--only <SENSOR>`: Run only named sensor(s). Repeatable or comma-separated.
- `--exclude <SENSOR>`: Exclude named sensor(s). Repeatable or comma-separated.
- `--record`: Persist beats and error signatures into `.do-harness/agent_state.db`.
- `--task <ID>`: Scope recorded beats to task ID. Requires `--record`.
- `--evidence <FILE>`: Write machine-readable evidence artifact JSON.
- `--strict`: Exit non-zero if any sensors were skipped or evidence checks fail. Default evidence artifact path: `.do-harness/evidence.json`.

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
- `--list-skills`: List discovered skills under `.agents/skills`.
- `--fail-fast`: Halt evaluation on first failing skill.
- `--dry-run`: Perform dry-run evaluation without updating state.
- `--format <Format>`: Output format (`text` or `json`).

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

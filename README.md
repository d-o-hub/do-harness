# do-harness

A compiled agent-execution harness CLI: feedforward guides (AGENTS.md, `.agents/skills/`) plus feedback sensors (computational checks) that agents and CI must pass.

## Install

Requirements: Rust 1.85+.

```bash
cargo build --release -p do-harness
```

The binary lands at `target/release/do-harness`.

## Commands

| Command | Description |
|---------|-------------|
| `verify` | Run all sensors (flags: `--fail-fast`, `--format text\|json`, `--only NAME` repeatable) |
| `list` | Print sensor names (`--format text\|json`) |
| `init-db` | Apply migrations to `.do-harness/agent_state.db` |
| `seed` | Upsert `plans/invariants.json` into the DB |
| `hook install [--force]` / `hook uninstall` / `hook status` | Manage `.git/hooks/pre-commit` + `pre-push` |

Global flags: `--root <path>`, `--config <path>`.

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | All sensors passed |
| 1 | A sensor failed |
| 2 | Usage/config/discovery error (e.g. not inside a do-harness workspace, missing git repo for `hook install`) |

## JSON output

`verify --format json` emits a single JSON object:

```json
{
  "ok": false,
  "root": "/path/to/workspace",
  "failed": ["clippy"],
  "sensors": [
    {"name": "fmt", "ok": true, "exit_code": 0, "duration_ms": 320},
    {"name": "clippy", "ok": false, "exit_code": 1, "duration_ms": 4100}
  ]
}
```

Failure output tails are printed to stderr so stdout stays parseable.

## Hooks

`do-harness hook install` writes both hooks into `.git/hooks/`:

- **pre-commit**: runs `verify --fail-fast --only fmt --only loc`
- **pre-push**: runs the full `verify` suite

Managed hooks carry a marker identifying them as do-harness-owned; re-running `hook install` overwrites them, and `--force` overwrites any pre-existing hook at the same path. `hook uninstall` removes only managed hooks. `hook status` reports which hooks are present and whether they are managed.

Alternative for users of the pre-commit framework: this repo ships `.pre-commit-config.yaml` with a `local` hook (`language: system`) whose entry is `./target/release/do-harness verify --fail-fast --only fmt --only loc`.

## CI

GitHub Actions: see `.github/workflows/verify.yml` (builds `do-harness`, then runs `verify --format json`).

GitLab CI:

```yaml
verify:
  image: rust:latest
  script:
    - cargo build --release -p do-harness
    - ./target/release/do-harness verify --format json
```

## Configuration

`do-harness.toml` at the workspace root configures the harness:

- `language` — the language pack (`"rust"`).
- `[hooks]` — `pre-commit` / `pre-push` lists naming the sensors each hook runs; an empty `pre-push` list means the full suite.
- `[[sensors]]` — each sensor has a `name` and an `argv` (the command to execute).

When no config is found, the CLI falls back to the built-in Rust sensor pack. The config file is also the workspace-root marker used for discovery.

## License

MIT. See `LICENSE`.

# Sensor plugins

A **sensor plugin** is a reusable sensor package: a versioned manifest that
ships a sensor definition under the repository's `plugins/` directory. The
harness discovers plugins, resolves `[[sensors]]` entries that name them, and
runs them exactly like inline sensors. Plugins let adopters share gate
definitions across repositories instead of hand-writing `argv` wrappers for
every tool (Ruff, golangci-lint, SpotBugs, ESLint, …).

Plugins are **process-based**: the manifest's `argv` is executed by the
harness with the same trust boundary and resource policy as any inline sensor
(see [sensor-execution.md](sensor-execution.md)). A plugin never loads code
into the harness process, so a plugin author may implement one in any language
that ships an executable `argv`.

## The protocol crate

The `do-harness-sensor` crate
(`crates/do-harness-sensor/`) is the single source of truth for the protocol:
the manifest schema (`PluginManifest`), the sensor definition type
(`SensorSpec`), and the schema version constant. Plugin authors can depend on
it to validate manifests against the identical types the harness parses.

## Discovery

The harness scans `<root>/plugins/**` for files named `sensor-plugin.toml`.
Every manifest is loaded and validated when configuration is read, and a
broken installation fails closed — no command silently degrades the effective
sensor set. Manifest validation rejects:

- invalid TOML or an unknown key (both at the top level and inside
  `[sensor]`);
- an unsupported `schema-version` (only version `1` is understood; a newer
  version fails rather than being guessed at);
- a reserved `plugin` key inside `[sensor]` (the harness sets it during
  resolution);
- an invalid sensor definition (empty `argv`, bad `when-changed`/`inputs`
  globs, out-of-range `max-output-bytes`, `timeout = 0` without a
  justification, conflicting severity aliases);
- two manifests declaring the same sensor name.

## Referencing a plugin

A `[[sensors]]` entry references a plugin by name:

<!-- example: id=sensor-plugin-reference classification=parser-check-only reason="Inline [[sensors]] plugin reference example" -->
```toml
[[sensors]]
name = "ruff"
plugin = "ruff"
when-changed = ["src/**/*.py"]
```

The `plugin` key must name a discovered plugin. The entry's inline fields
override the manifest's defaults; fields left unset inherit them. The resolved
sensor keeps the entry's `name` (so one plugin can be aliased under several
gate names) and is marked as a plugin sensor in `list` and `explain`.
Resolution refuses name collisions: an inline sensor claiming the same final
name as a plugin sensor is a configuration error.

## The manifest

<!-- example: id=sensor-plugin-manifest classification=parser-check-only reason="Versioned plugin manifest schema example" -->
```toml
schema-version = 1
version = "1.2.0"
description = "Ruff lint gate"

[sensor]
name = "ruff"
argv = ["ruff", "check", "."]
when-changed = ["**/*.py"]
inputs = ["**/*.py", "pyproject.toml"]
severity = "error"
```

Top-level keys:

- `schema-version` — required; must be `1`.
- `version` — required; the plugin package version (independent of the schema
  version).
- `description` — optional; what the sensor gate checks.

The `[sensor]` table is a full `[[sensors]]` entry: `name`, `argv`, and any of
the optional fields documented for inline sensors (`kind`, `retry`, `timeout`,
`timeout-justification`, `max-output-bytes`, `environment-allowlist`,
`redact-env`, `severity`, `fix`, `transient_exit_codes`, `when-changed`,
`artifacts`, `coverage-inputs`, `inputs`, `environment-inputs`). It must not
set `plugin`.

## Reference plugins
The repository ships a reference plugin for a non-Rust ecosystem at
`plugins/ruff/sensor-plugin.toml` (a Python/Ruff lint gate), and the identical
manifest is distributed on crates.io as the `do-harness-sensor-ruff` crate. It
validates the interface: the manifest parses, resolves by name, and runs through
`verify`, `list`, and `explain` like any inline sensor. Install it by copying
the manifest from the crate (or from `plugins/ruff/`) into your repository's
`plugins/ruff/sensor-plugin.toml`.

Because the plugin is a manifest, not Rust code, the crate is a distribution
vehicle: it embeds the manifest and parses it against the protocol types on
every build, and the harness test suite keeps the crate's copy and the
repository's `plugins/ruff/` copy byte-identical.

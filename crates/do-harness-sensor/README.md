# do-harness-sensor

The plugin protocol for [do-harness](https://github.com/d-o-hub/do-harness)
sensors: versioned `sensor-plugin.toml` manifests and the sensor definition
types the harness parses and executes.

Sensors are process-based commands (`argv`); a plugin is a manifest that ships
a reusable sensor definition under the repository's `plugins/` directory. See
[`src/lib.rs`](src/lib.rs) for the schema and
[`docs/sensor-plugins.md`](../../docs/sensor-plugins.md) for the discovery and
resolution contract.

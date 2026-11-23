# do-harness-sensor-ruff

The reference non-Rust sensor plugin for
[do-harness](https://github.com/d-o-hub/do-harness): a Ruff (Python) lint
gate distributed as a `sensor-plugin.toml` manifest.

The plugin is a manifest, not Rust code. Install it by copying
`sensor-plugin.toml` from this crate into your repository's
`plugins/ruff/sensor-plugin.toml`, then reference it from `[[sensors]]`:

<!-- example: id=ruff-plugin-reference classification=parser-check-only reason="Inline [[sensors]] plugin reference example" -->
```toml
[[sensors]]
name = "ruff"
plugin = "ruff"
```

See [`src/lib.rs`](src/lib.rs) for the manifest, and
[`docs/sensor-plugins.md`](../../docs/sensor-plugins.md) in the do-harness
repository for the discovery and resolution contract.

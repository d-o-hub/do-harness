//! The reference non-Rust sensor plugin for [do-harness](https://github.com/d-o-hub/do-harness): a Ruff (Python) lint gate.
//!
//! This crate is the crates.io distribution vehicle for the reference plugin
//! that validates the [`do-harness-sensor`] protocol in a non-Rust ecosystem
//! (issue #575). The plugin itself is a manifest, not Rust code: install it by
//! copying [`MANIFEST`] (or the `sensor-plugin.toml` file next to this
//! crate's `src/`) into your repository's `plugins/ruff/sensor-plugin.toml`,
//! then reference it from `[[sensors]]`:
//!
//! ```toml
//! [[sensors]]
//! name = "ruff"
//! plugin = "ruff"
//! ```
//!
//! See
//! <https://github.com/d-o-hub/do-harness/blob/main/docs/sensor-plugins.md>
//! for the discovery and resolution contract.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// The reference plugin manifest, verbatim as shipped on crates.io.
pub const MANIFEST: &str = include_str!("../sensor-plugin.toml");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_is_a_valid_plugin_manifest() {
        let manifest: do_harness_sensor::PluginManifest =
            toml::from_str(MANIFEST).expect("valid plugin manifest");
        assert_eq!(
            manifest.schema_version,
            do_harness_sensor::PLUGIN_SCHEMA_VERSION
        );
        assert_eq!(manifest.sensor.name, "ruff");
        assert!(!manifest.sensor.argv.is_empty());
        assert_eq!(manifest.sensor.argv, vec!["ruff", "check", "."]);
        assert_eq!(manifest.sensor.when_changed, vec!["**/*.py"]);
        // The harness sets `plugin` during resolution; a manifest must not.
        assert_eq!(manifest.sensor.plugin, None);
    }
}

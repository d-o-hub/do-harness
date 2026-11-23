//! The reference non-Rust sensor plugin for [do-harness](https://github.com/d-o-hub/do-harness): a Ruff (Python) lint gate.
//!
//! This crate is the crates.io distribution vehicle for the reference plugin
//! that validates the `do-harness-sensor` protocol in a non-Rust ecosystem
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

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manifest_is_a_valid_plugin_manifest() {
        let manifest: toml::Value = toml::from_str(MANIFEST).expect("valid TOML");
        let sensor = manifest.get("sensor").expect("[sensor] table");
        assert_eq!(manifest["schema-version"].as_integer(), Some(1));
        assert_eq!(sensor["name"].as_str(), Some("ruff"));
        assert_eq!(
            sensor["argv"].as_array().map(|a| {
                a.iter()
                    .map(|v| v.as_str().unwrap_or_default())
                    .collect::<Vec<_>>()
            }),
            Some(vec!["ruff", "check", "."])
        );
        assert_eq!(sensor["when-changed"].as_array().map(Vec::len), Some(1));
        // The harness sets `plugin` during resolution; a manifest must not.
        assert!(sensor.get("plugin").is_none());
    }
}

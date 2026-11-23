//! Sensor plugin discovery and resolution.
//!
//! A *sensor plugin* is a versioned manifest ([`do_harness_sensor::PluginManifest`])
//! shipped under the repository's `plugins/` directory. The harness discovers
//! manifests, validates them fail-closed, and resolves `[[sensors]]` entries
//! that reference a plugin by name (`plugin = "ruff"`) into concrete sensor
//! definitions before `verify`/`list`/`explain` see them. Plugins stay
//! process-based: the manifest's `argv` is executed exactly like an inline
//! sensor's, never loaded into the harness process.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use do_harness_sensor::{
    PLUGIN_MANIFEST_FILE, PLUGIN_SCHEMA_VERSION, PLUGINS_DIR, PluginManifest, SensorSpec,
};

use crate::config::validate_sensor;

/// A discovered plugin manifest and where it came from.
#[derive(Debug)]
struct Plugin {
    /// The parsed, validated manifest.
    manifest: PluginManifest,
    /// The `sensor-plugin.toml` file that declared it (for diagnostics).
    source: PathBuf,
}

/// Resolves plugin-referencing `[[sensors]]` entries into concrete sensor
/// definitions, in place.
///
/// Discovery reads every `sensor-plugin.toml` under `<root>/plugins` and
/// validates each manifest fail-closed: an unreadable file, invalid TOML, an
/// unsupported `schema-version`, a reserved `plugin` key, an invalid sensor
/// definition, or two manifests declaring the same sensor name are all
/// configuration errors. Each `[[sensors]]` entry with a `plugin` key must
/// name a discovered plugin; the entry's inline fields override the plugin's
/// defaults, and a resolved sensor name that collides with another sensor is
/// refused.
///
/// # Errors
///
/// Returns an error for any discovery or resolution failure; a broken plugin
/// installation never silently degrades the effective sensor set.
pub(crate) fn resolve(sensors: &mut Vec<SensorSpec>, root: &Path) -> Result<()> {
    let plugins = discover(root)?;
    let mut all_names: Vec<String> = Vec::with_capacity(sensors.len());
    let mut plugin_names: Vec<String> = Vec::new();
    let mut resolved: Vec<SensorSpec> = Vec::with_capacity(sensors.len());
    for entry in sensors.drain(..) {
        let Some(plugin_name) = entry.plugin.clone() else {
            if plugin_names.contains(&entry.name) {
                return Err(anyhow!(
                    "sensor '{}' collides with a plugin sensor of the same name",
                    entry.name
                ));
            }
            all_names.push(entry.name.clone());
            resolved.push(entry);
            continue;
        };
        let plugin = plugins.get(&plugin_name).ok_or_else(|| {
            anyhow!(
                "sensor '{}' references plugin '{plugin_name}', which is not installed under \
                 <root>/{PLUGINS_DIR}",
                entry.name
            )
        })?;
        let merged = merge(entry, &plugin_name, plugin);
        if all_names.contains(&merged.name) || plugin_names.contains(&merged.name) {
            return Err(anyhow!(
                "sensor name '{}' from plugin '{plugin_name}' collides with another sensor",
                merged.name
            ));
        }
        all_names.push(merged.name.clone());
        plugin_names.push(merged.name.clone());
        resolved.push(merged);
    }
    *sensors = resolved;
    Ok(())
}

/// Merges a `[[sensors]]` entry into its plugin's manifest definition: inline
/// fields override the manifest's defaults; unset fields inherit them.
fn merge(mut entry: SensorSpec, plugin_name: &str, plugin: &Plugin) -> SensorSpec {
    let base = &plugin.manifest.sensor;
    if entry.argv.is_empty() {
        entry.argv.clone_from(&base.argv);
    }
    if entry.kind.is_none() {
        entry.kind = base.kind;
    }
    if entry.retry.is_none() {
        entry.retry = base.retry;
    }
    if entry.timeout.is_none() {
        entry.timeout = base.timeout;
    }
    if entry.timeout_justification.is_none() {
        entry
            .timeout_justification
            .clone_from(&base.timeout_justification);
    }
    if entry.max_output_bytes.is_none() {
        entry.max_output_bytes = base.max_output_bytes;
    }
    if entry.environment_allowlist.is_none() {
        entry
            .environment_allowlist
            .clone_from(&base.environment_allowlist);
    }
    if entry.redact_env.is_empty() {
        entry.redact_env.clone_from(&base.redact_env);
    }
    if entry.severity.is_none() {
        entry.severity = base.severity;
    }
    if entry.fix.is_none() {
        entry.fix.clone_from(&base.fix);
    }
    entry.allow_failure = entry.allow_failure || base.allow_failure;
    if entry.transient_exit_codes.is_empty() {
        entry
            .transient_exit_codes
            .clone_from(&base.transient_exit_codes);
    }
    if entry.when_changed.is_empty() {
        entry.when_changed.clone_from(&base.when_changed);
    }
    if entry.artifacts.is_empty() {
        entry.artifacts.clone_from(&base.artifacts);
    }
    if entry.coverage_inputs.is_empty() {
        entry.coverage_inputs.clone_from(&base.coverage_inputs);
    }
    if entry.inputs.is_empty() {
        entry.inputs.clone_from(&base.inputs);
    }
    if entry.environment_inputs.is_empty() {
        entry
            .environment_inputs
            .clone_from(&base.environment_inputs);
    }
    entry.plugin = Some(plugin_name.to_owned());
    entry
}

/// Discovers and validates every plugin manifest under `<root>/plugins`.
///
/// # Errors
///
/// Returns an error when the plugins directory cannot be walked, a manifest
/// cannot be read or parsed, its schema version is unsupported, it sets the
/// reserved `plugin` key, its sensor definition is invalid, or two manifests
/// declare the same sensor name.
fn discover(root: &Path) -> Result<BTreeMap<String, Plugin>> {
    let plugins_dir = root.join(PLUGINS_DIR);
    if !plugins_dir.is_dir() {
        return Ok(BTreeMap::new());
    }
    let mut plugins: BTreeMap<String, Plugin> = BTreeMap::new();
    let mut pending = vec![plugins_dir];
    while let Some(directory) = pending.pop() {
        let entries = std::fs::read_dir(&directory)
            .with_context(|| format!("failed to read plugin directory {}", directory.display()))?;
        for entry in entries {
            let entry = entry.with_context(|| {
                format!("failed to read plugin directory {}", directory.display())
            })?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .with_context(|| format!("failed to inspect plugin path {}", path.display()))?;
            if file_type.is_dir() {
                pending.push(path);
                continue;
            }
            if !file_type.is_file() || path.file_name().is_none_or(|n| n != PLUGIN_MANIFEST_FILE) {
                continue;
            }
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read plugin manifest {}", path.display()))?;
            let manifest: PluginManifest = toml::from_str(&text)
                .with_context(|| format!("invalid plugin manifest {}", path.display()))?;
            if manifest.schema_version != PLUGIN_SCHEMA_VERSION {
                anyhow::bail!(
                    "plugin manifest {} declares schema version {}; this binary understands \
                     {PLUGIN_SCHEMA_VERSION}",
                    path.display(),
                    manifest.schema_version
                );
            }
            if manifest.sensor.argv.is_empty() {
                anyhow::bail!(
                    "plugin manifest {} declares an empty [sensor] argv",
                    path.display()
                );
            }
            if manifest.sensor.plugin.is_some() {
                anyhow::bail!(
                    "plugin manifest {} sets the reserved `plugin` key in [sensor]; \
                     remove it (the harness sets it during resolution)",
                    path.display()
                );
            }
            validate_sensor(&manifest.sensor).with_context(|| {
                format!(
                    "invalid sensor definition in plugin manifest {}",
                    path.display()
                )
            })?;
            let name = manifest.sensor.name.clone();
            if plugins.contains_key(&name) {
                anyhow::bail!(
                    "two plugin manifests declare the sensor name '{name}' ({}, {})",
                    plugins[&name].source.display(),
                    path.display()
                );
            }
            plugins.insert(
                name,
                Plugin {
                    manifest,
                    source: path,
                },
            );
        }
    }
    Ok(plugins)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"
schema-version = 1
version = "1.2.0"
description = "Ruff lint gate"

[sensor]
name = "ruff"
argv = ["ruff", "check", "."]
when-changed = ["**/*.py"]
inputs = ["**/*.py", "pyproject.toml"]
severity = "error"
"#;

    fn write_manifest(root: &Path, dir: &str, text: &str) -> PathBuf {
        let path = root.join(PLUGINS_DIR).join(dir).join(PLUGIN_MANIFEST_FILE);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        path
    }

    fn sensor(name: &str) -> SensorSpec {
        toml::from_str(&format!("name = \"{name}\"\nargv = [\"true\"]\n")).unwrap()
    }

    fn plugin_entry(name: &str, plugin: &str) -> SensorSpec {
        toml::from_str(&format!(
            "name = \"{name}\"\nplugin = \"{plugin}\"\nargv = []\n"
        ))
        .unwrap()
    }

    #[test]
    fn discovery_parses_nested_sensor_table() {
        let tmp = tempfile::tempdir().unwrap();
        write_manifest(tmp.path(), "ruff", MANIFEST);
        let found = discover(tmp.path()).unwrap();
        let plugin = found.get("ruff").expect("ruff plugin");
        assert_eq!(plugin.manifest.sensor.argv, vec!["ruff", "check", "."]);
    }

    #[test]
    fn discovery_rejects_unknown_manifest_keys() {
        let tmp = tempfile::tempdir().unwrap();
        write_manifest(
            tmp.path(),
            "a",
            &MANIFEST.replace("severity = \"error\"", "bogus = true\nseverity = \"error\""),
        );
        let error = discover(tmp.path()).unwrap_err();
        assert!(
            format!("{error:#}").contains("invalid plugin manifest"),
            "{error:#}"
        );
    }

    #[test]
    fn discovery_rejects_unsupported_schema_version() {
        let tmp = tempfile::tempdir().unwrap();
        write_manifest(
            tmp.path(),
            "a",
            &MANIFEST.replace("schema-version = 1", "schema-version = 2"),
        );
        let error = discover(tmp.path()).unwrap_err();
        assert!(format!("{error:#}").contains("schema version"), "{error:#}");
    }

    #[test]
    fn discovery_rejects_reserved_plugin_key_in_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        write_manifest(
            tmp.path(),
            "a",
            &MANIFEST.replace("name = \"ruff\"", "name = \"ruff\"\nplugin = \"ruff\""),
        );
        let error = discover(tmp.path()).unwrap_err();
        assert!(
            format!("{error:#}").contains("reserved `plugin` key"),
            "{error:#}"
        );
    }

    #[test]
    fn discovery_rejects_duplicate_plugin_names() {
        let tmp = tempfile::tempdir().unwrap();
        write_manifest(tmp.path(), "a", MANIFEST);
        write_manifest(tmp.path(), "b", MANIFEST);
        let error = discover(tmp.path()).unwrap_err();
        assert!(
            format!("{error:#}").contains("two plugin manifests"),
            "{error:#}"
        );
    }

    #[test]
    fn discovery_rejects_invalid_sensor_definition() {
        let tmp = tempfile::tempdir().unwrap();
        write_manifest(
            tmp.path(),
            "a",
            &MANIFEST.replace(
                "when-changed = [\"**/*.py\"]",
                "when-changed = [\"../escape.py\"]",
            ),
        );
        let error = discover(tmp.path()).unwrap_err();
        assert!(
            format!("{error:#}").contains("workspace-relative"),
            "{error:#}"
        );
    }

    #[test]
    fn resolution_merges_plugin_defaults_and_inline_overrides() {
        let tmp = tempfile::tempdir().unwrap();
        write_manifest(tmp.path(), "ruff", MANIFEST);
        let mut sensors = vec![plugin_entry("ruff", "ruff")];
        resolve(&mut sensors, tmp.path()).unwrap();
        assert_eq!(sensors.len(), 1);
        let spec = &sensors[0];
        assert_eq!(spec.argv, vec!["ruff", "check", "."]);
        assert_eq!(spec.when_changed, vec!["**/*.py"]);
        assert_eq!(spec.inputs, vec!["**/*.py", "pyproject.toml"]);
        assert_eq!(spec.severity, Some(crate::config::SensorSeverity::Error));
        assert_eq!(spec.plugin.as_deref(), Some("ruff"));

        let mut overridden = vec![
            toml::from_str::<SensorSpec>(
                "name = \"ruff\"\nplugin = \"ruff\"\nargv = [\"python3\", \"-m\", \"ruff\"]\n\
                 when-changed = [\"src/**/*.py\"]\n",
            )
            .unwrap(),
        ];
        resolve(&mut overridden, tmp.path()).unwrap();
        assert_eq!(overridden[0].argv, vec!["python3", "-m", "ruff"]);
        assert_eq!(overridden[0].when_changed, vec!["src/**/*.py"]);
        // Unset fields still inherit the manifest.
        assert_eq!(overridden[0].inputs, vec!["**/*.py", "pyproject.toml"]);
    }

    #[test]
    fn resolution_aliases_plugin_under_another_sensor_name() {
        let tmp = tempfile::tempdir().unwrap();
        write_manifest(tmp.path(), "ruff", MANIFEST);
        let mut sensors = vec![plugin_entry("python-lint", "ruff")];
        resolve(&mut sensors, tmp.path()).unwrap();
        assert_eq!(sensors[0].name, "python-lint");
        assert_eq!(sensors[0].argv, vec!["ruff", "check", "."]);
        assert_eq!(sensors[0].plugin.as_deref(), Some("ruff"));
    }

    #[test]
    fn resolution_refuses_unknown_plugin_reference() {
        let tmp = tempfile::tempdir().unwrap();
        let mut sensors = vec![plugin_entry("lint", "missing")];
        let error = resolve(&mut sensors, tmp.path()).unwrap_err();
        assert!(format!("{error:#}").contains("not installed"), "{error:#}");
    }

    #[test]
    fn resolution_refuses_inline_vs_plugin_collision() {
        let tmp = tempfile::tempdir().unwrap();
        write_manifest(tmp.path(), "ruff", MANIFEST);
        // Inline sensor first, plugin claiming the same final name later.
        let mut sensors = vec![sensor("ruff"), plugin_entry("ruff2", "ruff")];
        assert!(resolve(&mut sensors, tmp.path()).is_ok());
        // Plugin first, inline sensor claiming the resolved name later.
        let mut sensors = vec![plugin_entry("lint", "ruff"), sensor("lint")];
        let error = resolve(&mut sensors, tmp.path()).unwrap_err();
        assert!(format!("{error:#}").contains("collides"), "{error:#}");
        // Two entries resolving to the same final name.
        let mut sensors = vec![plugin_entry("lint", "ruff"), plugin_entry("lint", "ruff")];
        let error = resolve(&mut sensors, tmp.path()).unwrap_err();
        assert!(format!("{error:#}").contains("collides"), "{error:#}");
    }

    #[test]
    fn resolution_leaves_plain_sensors_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        let mut sensors = vec![sensor("fmt")];
        resolve(&mut sensors, tmp.path()).unwrap();
        assert_eq!(sensors.len(), 1);
        assert_eq!(sensors[0].name, "fmt");
        assert_eq!(sensors[0].plugin, None);
    }

    #[test]
    fn repo_reference_plugin_is_valid_and_matches_published_ruff_crate() {
        // The shipped reference plugin (issue #575) must stay parseable by the
        // harness, and the repository's copy must stay byte-identical to the
        // one distributed on crates.io by `do-harness-sensor-ruff`.
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut sensors = vec![plugin_entry("ruff", "ruff")];
        resolve(&mut sensors, &root).unwrap();
        let ruff = &sensors[0];
        assert_eq!(ruff.name, "ruff");
        assert_eq!(ruff.argv, vec!["ruff", "check", "."]);
        assert_eq!(ruff.when_changed, vec!["**/*.py"]);
        assert_eq!(ruff.plugin.as_deref(), Some("ruff"));

        let repo_manifest = std::fs::read_to_string(root.join("plugins/ruff/sensor-plugin.toml"))
            .expect("repository reference plugin manifest");
        let crate_manifest = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../do-harness-sensor-ruff/sensor-plugin.toml"),
        )
        .expect("ruff crate manifest");
        assert_eq!(
            repo_manifest, crate_manifest,
            "repository plugins/ruff and do-harness-sensor-ruff must ship the \
             same manifest"
        );
    }
}

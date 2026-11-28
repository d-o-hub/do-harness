//! Resolution of explicitly selected, installed sensor commands.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use toml::Value;

const MANIFEST: &str = "sensor-plugin.toml";
const SCHEMA_VERSION: u32 = 1;

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct Manifest {
    schema_version: u32,
    plugins: Vec<Plugin>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Plugin {
    name: String,
    argv: Vec<String>,
}

pub(super) fn resolve(root: &Path, document: &mut Value) -> Result<bool> {
    let Some(sensors) = document.get_mut("sensors").and_then(Value::as_array_mut) else {
        return Ok(false);
    };
    let selected = sensors
        .iter_mut()
        .filter_map(Value::as_table_mut)
        .filter(|sensor| sensor.contains_key("plugin"));
    let mut selected = selected.peekable();
    if selected.peek().is_none() {
        return Ok(false);
    }
    let entries = load_manifest(root)?;
    for sensor in selected {
        let name = sensor.remove("plugin").context("missing plugin name")?;
        let name = name.as_str().context("plugin must be a string")?;
        if sensor.contains_key("argv") {
            bail!("sensor plugin '{name}' and inline argv are mutually exclusive");
        }
        let argv = entries
            .get(name)
            .with_context(|| format!("sensor plugin '{name}' is not installed in {MANIFEST}"))?;
        sensor.insert("argv".to_owned(), Value::try_from(argv)?);
        let inputs = sensor
            .entry("coverage-inputs".to_owned())
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .context("coverage-inputs must be an array")?;
        if !inputs.iter().any(|value| value.as_str() == Some(MANIFEST)) {
            inputs.push(Value::String(MANIFEST.to_owned()));
        }
    }
    Ok(true)
}

fn load_manifest(root: &Path) -> Result<BTreeMap<String, Vec<String>>> {
    let path = root.join(MANIFEST);
    let metadata = std::fs::symlink_metadata(&path)
        .with_context(|| format!("cannot find installed sensor manifest {}", path.display()))?;
    if !metadata.is_file() {
        bail!(
            "sensor plugin manifest {} must be a regular file",
            path.display()
        );
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read sensor plugin manifest {}", path.display()))?;
    let manifest: Manifest = toml::from_str(&content)
        .with_context(|| format!("invalid sensor plugin manifest {}", path.display()))?;
    if manifest.schema_version != SCHEMA_VERSION {
        bail!(
            "unsupported sensor plugin schema version {}",
            manifest.schema_version
        );
    }
    let mut entries = BTreeMap::new();
    for plugin in manifest.plugins {
        if plugin.name.is_empty()
            || !plugin
                .name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
        {
            bail!("invalid sensor plugin name '{}'", plugin.name);
        }
        if plugin
            .argv
            .first()
            .is_none_or(|program| program.trim().is_empty())
        {
            bail!(
                "sensor plugin '{}' must declare a program in argv",
                plugin.name
            );
        }
        if entries.insert(plugin.name.clone(), plugin.argv).is_some() {
            bail!("duplicate sensor plugin '{}'", plugin.name);
        }
    }
    Ok(entries)
}

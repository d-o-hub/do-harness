//! HTN method catalog loading and lookup helpers.
//!
//! The catalog lives at `plans/methods.json` under the workspace root and is
//! the frozen source of truth for which method names are valid and which
//! subtasks gate on which computational sensors. When a consumer workspace has
//! not yet scaffolded `plans/methods.json`, the built-in catalog is used so
//! task workflows work everywhere.

use std::path::Path;

use anyhow::{Context, Result};
use do_harness_types::Method;
use serde::Deserialize;

/// Serializable shape of `plans/methods.json`: a top-level method list.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MethodFile {
    methods: Vec<Method>,
}

/// Embedded fallback catalog matching `plans/methods.json`.
const BUILTIN_CATALOG: &str = include_str!("../../../plans/methods.json");

/// Loads the HTN method catalog from `plans/methods.json` under `root`.
///
/// Falls back to the built-in catalog when the file is absent, so consumer
/// workspaces created before the catalog was frozen still work.
///
/// # Errors
///
/// Returns an error when the file exists but cannot be read or does not match
/// the method catalog schema.
pub fn load_methods(root: &Path) -> Result<Vec<Method>> {
    let path = root.join("plans/methods.json");
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            let file: MethodFile = serde_json::from_str(&text).with_context(|| {
                format!(
                    "invalid plans/methods.json at {}: does not match method catalog schema",
                    path.display()
                )
            })?;
            Ok(file.methods)
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            let file: MethodFile = serde_json::from_str(BUILTIN_CATALOG)
                .context("built-in method catalog is invalid")?;
            Ok(file.methods)
        }
        Err(err) => Err(err).with_context(|| format!("failed to read {}", path.display())),
    }
}

/// Returns the method with the given name, if present.
#[must_use]
pub fn find_method<'a>(methods: &'a [Method], name: &str) -> Option<&'a Method> {
    methods.iter().find(|method| method.name == name)
}

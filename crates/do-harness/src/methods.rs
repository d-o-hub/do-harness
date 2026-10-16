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
///
/// The asset is a symlink to the canonical `plans/methods.json` so the two can
/// never drift; `cargo package` dereferences it into the published crate.
const BUILTIN_CATALOG: &str = include_str!("../assets/methods.json");

/// Loads the HTN method catalog from `plans/methods.json` under `root`.
///
/// Falls back to the built-in catalog when the file is absent, so consumer
/// workspaces created before the catalog was frozen still work.
///
/// # Errors
///
/// Returns an error when the file exists but cannot be read or does not match
/// the method catalog schema.
pub async fn load_methods(root: &Path) -> Result<Vec<Method>> {
    let path = root.join("plans/methods.json");
    match tokio::fs::read_to_string(&path).await {
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// A hand-edited catalog with a mistyped top-level key fails loudly instead
    /// of silently loading zero methods.
    #[tokio::test(flavor = "current_thread")]
    async fn load_methods_rejects_unknown_top_level_keys() {
        let dir = tempfile::tempdir().unwrap();
        tokio::fs::create_dir_all(dir.path().join("plans"))
            .await
            .unwrap();
        tokio::fs::write(
            dir.path().join("plans/methods.json"),
            r#"{"methods": [], "bogus": true}"#,
        )
        .await
        .unwrap();

        let err = load_methods(dir.path()).await.unwrap_err();
        assert!(
            err.to_string()
                .contains("does not match method catalog schema"),
            "unexpected error: {err}"
        );
    }

    /// A workspace without `plans/methods.json` still plans: the embedded
    /// fallback catalog parses and carries the shipped methods.
    #[tokio::test(flavor = "current_thread")]
    async fn load_methods_falls_back_to_the_builtin_catalog() {
        let dir = tempfile::tempdir().unwrap();
        let methods = load_methods(dir.path()).await.unwrap();
        assert!(
            find_method(&methods, "vertical-event-slice").is_some(),
            "built-in catalog missed a shipped method"
        );
    }
}

//! `doctor` facts that are one check each: the configuration schema version,
//! sensor `argv` path resolution, and task-export freshness.

use std::path::Path;

use crate::argv_paths::{self, MissingArgvPath};
use crate::config;

/// The declared configuration schema version, or `None` when there is no file.
///
/// Read from the raw TOML rather than the typed config so a file this binary
/// cannot fully parse (a future schema) still reports its version instead of
/// the unrelated parse error.
pub(super) fn config_schema(root: &Path) -> Option<u32> {
    let text = std::fs::read_to_string(root.join("do-harness.toml")).ok()?;
    let value: toml::Value = toml::from_str(&text).ok()?;
    match value.get("schema-version") {
        None => Some(0),
        Some(toml::Value::Integer(version)) => u32::try_from(*version).ok(),
        Some(_) => None,
    }
}

/// the missing ones and a config-load error, if any.
pub(super) async fn check_sensor_argv(root: &Path) -> (Vec<MissingArgvPath>, Option<String>) {
    match config::load_raw(root, None).await {
        Ok((cfg, _)) => (
            argv_paths::missing_paths_for(root, cfg.effective_sensors()),
            None,
        ),
        Err(err) => (Vec::new(), Some(format!("{err:#}"))),
    }
}

/// Flags `plans/tasks.json` when it predates the newest task update.
///
/// Returns `true` when the export is missing, unreadable, invalid, or stale;
/// failures are also appended to `failures`.
pub(super) async fn check_task_export(
    root: &Path,
    latest_task_update: Option<i64>,
    failures: &mut Vec<String>,
) -> bool {
    let path = root.join("plans/tasks.json");
    if !path.exists() {
        return false;
    }
    let raw = match tokio::fs::read_to_string(&path).await {
        Ok(raw) => raw,
        Err(err) => {
            failures.push(format!("plans/tasks.json unreadable: {err}"));
            return true;
        }
    };
    match serde_json::from_str::<crate::task::TaskSnapshot>(&raw) {
        Ok(snapshot) => {
            if latest_task_update.is_some_and(|updated| updated > snapshot.exported_at) {
                failures.push(
                    "plans/tasks.json export lags the database (run: do-harness task export)"
                        .to_owned(),
                );
                true
            } else {
                false
            }
        }
        Err(err) => {
            failures.push(format!("plans/tasks.json invalid: {err}"));
            true
        }
    }
}

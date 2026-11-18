//! `doctor` facts that are one check each: hook delegation lines, sensor
//! `argv` path resolution, and task-export freshness.
//!
//! Extracted from `doctor.rs` so the report flow stays under the 500-line
//! ceiling; the split is move-only.

use std::path::Path;

use crate::argv_paths::{self, MissingArgvPath};
use crate::config;
use crate::hooks::HookOwner;

/// Renders one `[OK]`/`[WARN]`/`[FAIL]` line for a hook path.
///
/// A delegated hook is healthy: the pre-commit framework, or the hook manager
/// behind `core.hooksPath`, owns the gate and `doctor --strict` must not call it
/// missing. An absent hook that a pre-commit configuration was meant to own
/// carries the command that installs it.
pub(super) fn hook_line(
    name: &str,
    owner: HookOwner,
    shadowed: bool,
    core_hooks_path: Option<&str>,
    commitlint_missing: bool,
    pre_commit_config: bool,
) -> String {
    let configured = core_hooks_path.unwrap_or("configured");
    if shadowed && owner == HookOwner::Absent {
        return format!(
            "  [FAIL] {name} hook: installed in .git/hooks but shadowed by core.hooksPath={configured}"
        );
    }
    match owner {
        HookOwner::Managed if name == "commit-msg" && commitlint_missing => {
            format!("  [FAIL] {name} hook: installed but scripts/check-commitlint.sh is missing")
        }
        HookOwner::Managed => format!("  [OK] {name} hook: installed"),
        HookOwner::PreCommit => format!("  [OK] {name} hook: delegated to pre-commit"),
        HookOwner::HooksPath => {
            format!("  [OK] {name} hook: delegated to core.hooksPath={configured}")
        }
        HookOwner::Foreign => {
            format!("  [WARN] {name} hook: foreign hook not managed by do-harness")
        }
        HookOwner::Absent if pre_commit_config => format!(
            "  [WARN] {name} hook: absent (pre-commit config present; run: pre-commit install --hook-type {name})"
        ),
        HookOwner::Absent => format!("  [WARN] {name} hook: absent"),
    }
}

/// Resolves path-looking `argv` arguments for the effective sensors. Returns
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

/// The declared configuration schema version, or `None` when there is no file.
///
/// Read from the raw TOML rather than the typed config so a file this binary
/// cannot fully parse (a future schema) still reports its version instead of
/// the unrelated parse error.
pub(super) fn config_schema(root: &Path) -> Option<u32> {
    let text = std::fs::read_to_string(root.join("do-harness.toml")).ok()?;
    let value: toml::Value = toml::from_str(&text).ok()?;
    // Absent means a pre-versioning file this binary still reads. Treating it
    // as skew would fail `doctor` for every existing adopter; `migrate --diff`
    // is the recommendation, and a *declared* older version is the failure.
    match value.get("schema-version") {
        Some(toml::Value::Integer(version)) => u32::try_from(*version).ok(),
        _ => None,
    }
}

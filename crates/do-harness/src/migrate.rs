//! `do-harness migrate`: bring `do-harness.toml` to the current schema.
//!
//! Schema changes are not self-describing: a renamed key or a changed default
//! lands in a changelog, and adopters hand-edit configuration across minor
//! releases. Every migration here is deterministic and text-scoped, previews by
//! default, refuses to write when a conflict cannot be resolved mechanically,
//! and re-parses the result before it is accepted.

use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use crate::CliError;
use crate::config::CONFIG_SCHEMA_VERSION;
use crate::report::Format;

/// One planned line change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Change {
    /// 1-based line the change applies to (`0` means "insert before line 1").
    pub line: usize,
    /// Stable reason code.
    pub reason: &'static str,
    /// Current line, trimmed.
    pub before: String,
    /// Replacement line, trimmed.
    pub after: String,
}

/// The migration plan for one workspace.
#[derive(Debug, Clone, Serialize)]
pub struct MigrationPlan {
    /// Declared version found in the file (0 when the key is absent).
    pub from: u32,
    /// Version this binary understands.
    pub to: u32,
    /// Planned line changes, in file order.
    pub changes: Vec<Change>,
    /// Problems that cannot be resolved mechanically; a write is refused.
    pub conflicts: Vec<String>,
    /// True when the planned text is valid and conflict-free.
    pub applicable: bool,
}

impl MigrationPlan {
    /// Whether anything is planned at all.
    #[must_use]
    pub fn is_noop(&self) -> bool {
        self.changes.is_empty() && self.conflicts.is_empty()
    }
}

/// Runs `do-harness migrate`.
///
/// # Errors
///
/// Returns a usage error for a missing/unparsable/newer-than-this-binary
/// configuration, or when `--write` is asked to apply an unresolvable change.
pub async fn run(
    root: &Path,
    config_path: Option<&Path>,
    diff: bool,
    write: bool,
    format: Format,
) -> std::result::Result<(), CliError> {
    let path = config_path.map_or_else(|| root.join("do-harness.toml"), Path::to_path_buf);
    let original = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(CliError::Usage(anyhow::anyhow!(
                "no {} to migrate; run `do-harness init` first",
                path.display()
            )));
        }
        Err(error) => {
            return Err(CliError::Usage(anyhow::anyhow!(
                "failed to read {}: {error}",
                path.display()
            )));
        }
    };
    let plan = plan(&original).map_err(CliError::Usage)?;
    if write {
        if !plan.conflicts.is_empty() {
            return Err(CliError::Usage(anyhow::anyhow!(
                "cannot migrate mechanically: {}",
                plan.conflicts.join("; ")
            )));
        }
        if plan.is_noop() {
            println!(
                "{} is already at schema version {}",
                path.display(),
                plan.from
            );
            return Ok(());
        }
        let migrated = apply(&original, &plan).map_err(CliError::Usage)?;
        // Never leave a file this binary cannot parse: validate before writing
        // and restore byte for byte if anything fails afterwards.
        let parsed = crate::config::parse_str(&migrated).map_err(CliError::Usage)?;
        if parsed.schema_version != CONFIG_SCHEMA_VERSION {
            let _ = std::fs::write(&path, &original);
            return Err(CliError::Usage(anyhow::anyhow!(
                "migration produced schema version {}, expected {}",
                parsed.schema_version,
                CONFIG_SCHEMA_VERSION
            )));
        }
        std::fs::write(&path, &migrated).map_err(|error| {
            CliError::Usage(anyhow::anyhow!(
                "failed to write {}: {error}",
                path.display()
            ))
        })?;
        print_plan(&plan, format, "applied");
        // Chain into doctor: the migration is only useful if the workspace it
        // describes is healthy. A non-git workspace has no hooks to check; say
        // so instead of failing a successful migration.
        if root.join(".git").exists() {
            // Migration success does not depend on the rest of the workspace
            // being healthy, so doctor's findings are reported, not promoted
            // into a migration failure.
            if let Err(error) = crate::doctor::run(root, Format::Text, false, None, false).await {
                println!("doctor: {error:#}");
            }
        } else {
            println!("doctor skipped: {} is not a git checkout", root.display());
        }
        return Ok(());
    }
    let _ = diff;
    if plan.is_noop() {
        println!(
            "{} is already at schema version {}",
            path.display(),
            plan.from
        );
        return Ok(());
    }
    print_plan(&plan, format, "planned");
    Ok(())
}

/// Builds the plan without writing anything.
///
/// # Errors
///
/// Returns an error when the file is not valid TOML or declares a schema
/// version newer than this binary understands.
pub fn plan(text: &str) -> Result<MigrationPlan> {
    let value: toml::Value = toml::from_str(text).map_err(|error| {
        anyhow::anyhow!("do-harness.toml is not valid TOML, so nothing was migrated: {error}")
    })?;
    let from = match value.get("schema-version") {
        None => 0,
        Some(toml::Value::Integer(version)) if *version >= 0 => u32::try_from(*version)
            .map_err(|_| anyhow::anyhow!("schema-version {} is out of range", *version))?,
        Some(other) => {
            anyhow::bail!("schema-version must be a non-negative integer, found {other}");
        }
    };
    if from > CONFIG_SCHEMA_VERSION {
        anyhow::bail!(
            "do-harness.toml declares schema version {from}, but this binary understands {CONFIG_SCHEMA_VERSION}; upgrade do-harness instead of migrating"
        );
    }
    let mut plan = MigrationPlan {
        from,
        to: CONFIG_SCHEMA_VERSION,
        changes: Vec::new(),
        conflicts: Vec::new(),
        applicable: true,
    };
    if from < 1 {
        plan_v0_to_v1(text, &mut plan);
    }
    plan.applicable = plan.conflicts.is_empty();
    Ok(plan)
}

/// The v0 → v1 step: declare the version and retire the deprecated alias.
fn plan_v0_to_v1(text: &str, plan: &mut MigrationPlan) {
    let mut in_sensor = false;
    let mut sensor_has_severity = false;
    let mut sensor_start = 0;
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed == "[[sensors]]" {
            in_sensor = true;
            sensor_has_severity = false;
            sensor_start = index + 1;
            continue;
        }
        if trimmed.starts_with('[') {
            in_sensor = false;
            continue;
        }
        if in_sensor && trimmed.starts_with("severity") {
            sensor_has_severity = true;
        }
        if in_sensor && trimmed.starts_with("allow_failure") {
            let enabled = trimmed.ends_with("true");
            if enabled && sensor_has_severity {
                plan.conflicts.push(format!(
                    "line {}: sensor block starting at line {sensor_start} sets both `allow_failure` and `severity`; keep one",
                    index + 1
                ));
                continue;
            }
            let after = if enabled {
                "severity = \"warn\"".to_owned()
            } else {
                String::new()
            };
            plan.changes.push(Change {
                line: index + 1,
                reason: "allow-failure-alias",
                before: trimmed.to_owned(),
                after: after.clone(),
            });
        }
    }
    plan.changes.insert(
        0,
        Change {
            line: 0,
            reason: "schema-version",
            before: String::new(),
            after: format!("schema-version = {CONFIG_SCHEMA_VERSION}"),
        },
    );
}

/// Applies the plan to the text.
///
/// # Errors
///
/// Returns an error when a planned line no longer matches (the file changed
/// underneath the plan) so a stale plan can never write a wrong edit.
pub fn apply(text: &str, plan: &MigrationPlan) -> Result<String> {
    let mut lines: Vec<Option<String>> = text.lines().map(|line| Some(line.to_owned())).collect();
    for change in plan.changes.iter().rev() {
        if change.line == 0 {
            let insert_at = first_table_line(text);
            lines.insert(insert_at, Some(change.after.clone()));
            continue;
        }
        let index = change.line - 1;
        let current = lines
            .get(index)
            .and_then(Option::as_ref)
            .ok_or_else(|| anyhow::anyhow!("line {} disappeared from the file", change.line))?;
        if current.trim() != change.before {
            anyhow::bail!(
                "line {} changed while migrating (expected '{}', found '{}')",
                change.line,
                change.before,
                current.trim()
            );
        }
        lines[index] = if change.after.is_empty() {
            None
        } else {
            Some(change.after.clone())
        };
    }
    let mut output = String::new();
    for line in lines.into_iter().flatten() {
        output.push_str(&line);
        output.push('\n');
    }
    Ok(output)
}

/// Index (0-based) before which a top-level key may be inserted.
///
/// TOML forbids a bare key after a table header, so the version line goes
/// after the leading comment block and before the first `[table]`.
fn first_table_line(text: &str) -> usize {
    text.lines()
        .position(|line| line.trim_start().starts_with('['))
        .unwrap_or_else(|| text.lines().count())
}

fn print_plan(plan: &MigrationPlan, format: Format, verb: &str) {
    if format == Format::Json {
        if let Ok(json) = crate::report::json_pretty(plan) {
            println!("{json}");
        }
        return;
    }
    println!(
        "schema version {} -> {} ({verb}, {} change(s))",
        plan.from,
        plan.to,
        plan.changes.len()
    );
    for change in &plan.changes {
        match change.line {
            0 => println!("  + {}", change.after),
            line => {
                println!("  - line {line}: {}", change.before);
                if !change.after.is_empty() {
                    println!("  + line {line}: {}", change.after);
                }
            }
        }
    }
    for conflict in &plan.conflicts {
        println!("  ! {conflict}");
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn a_current_file_is_a_noop() {
        let plan = plan("schema-version = 1\n").unwrap();
        assert!(plan.is_noop());
        assert!(plan.applicable);
    }

    #[test]
    fn an_absent_version_is_inserted_before_the_first_table() {
        let text = "# comment\nlanguage = \"rust\"\n\n[hooks]\npre-commit = []\n";
        let plan = plan(text).unwrap();
        assert_eq!(plan.from, 0);
        assert!(plan.changes.iter().any(|change| change.line == 0
            && change.after == format!("schema-version = {CONFIG_SCHEMA_VERSION}")));
        let migrated = apply(text, &plan).unwrap();
        assert!(
            migrated.starts_with("# comment\nlanguage = \"rust\"\nschema-version = 1\n\n[hooks]")
        );
    }

    #[test]
    fn the_deprecated_alias_becomes_severity() {
        let text = "[[sensors]]\nname = \"x\"\nargv = [\"true\"]\nallow_failure = true\n";
        let plan = plan(text).unwrap();
        let migrated = apply(text, &plan).unwrap();
        assert!(migrated.contains("severity = \"warn\""), "{migrated}");
        assert!(!migrated.contains("allow_failure"), "{migrated}");
    }

    #[test]
    fn a_disabled_alias_is_removed() {
        let text = "[[sensors]]\nname = \"x\"\nargv = [\"true\"]\nallow_failure = false\n";
        let plan = plan(text).unwrap();
        let migrated = apply(text, &plan).unwrap();
        assert!(!migrated.contains("allow_failure"), "{migrated}");
        assert!(!migrated.contains("severity = \"warn\""), "{migrated}");
    }

    #[test]
    fn both_keys_in_one_block_is_a_conflict() {
        let text = "[[sensors]]\nname = \"x\"\nseverity = \"error\"\nallow_failure = true\n";
        let plan = plan(text).unwrap();
        assert!(!plan.applicable);
        assert_eq!(plan.conflicts.len(), 1);
        assert!(
            apply(text, &plan).is_ok(),
            "apply itself is still deterministic"
        );
    }

    #[test]
    fn a_newer_schema_is_refused() {
        let error = plan(&format!("schema-version = {}\n", CONFIG_SCHEMA_VERSION + 1)).unwrap_err();
        assert!(error.to_string().contains("upgrade do-harness"), "{error}");
    }

    #[test]
    fn invalid_toml_is_refused_before_any_change() {
        let error = plan("[[sensors]\n").unwrap_err();
        assert!(error.to_string().contains("not valid TOML"), "{error}");
    }

    #[test]
    fn a_stale_plan_cannot_write() {
        let text = "[[sensors]]\nname = \"x\"\nallow_failure = true\n";
        let plan = plan(text).unwrap();
        let changed = text.replace("allow_failure = true", "allow_failure = false");
        let error = apply(&changed, &plan).unwrap_err();
        assert!(
            error.to_string().contains("changed while migrating"),
            "{error}"
        );
    }
}

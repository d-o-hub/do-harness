//! `do-harness sensor new`: scaffold one `[[sensors]]` entry.
//!
//! Editing `do-harness.toml` by hand for every check is the friction that keeps
//! adopters on the empty generic pack: a typo in `argv`, a missing executable,
//! or a sensor name that is not in the signal set it claims to serve all fail
//! later, at `verify` time. This command renders the block, optionally
//! scaffolds the script it names, and then proves the result with the same
//! selection resolution `explain`/`verify` use — restoring the file byte for
//! byte when that proof fails.

use std::path::Path;

use crate::cli::NewSensorArgs;
use crate::sensors::{self, VerifyOpts};
use crate::{CliError, config};

/// Runs `sensor new`.
///
/// # Errors
///
/// Returns a usage error for an invalid name or `argv`, a missing signal set,
/// an existing sensor name, an unwritable script path, or a configuration the
/// new entry makes unparsable.
pub async fn run(
    root: &Path,
    config_path: Option<&Path>,
    args: &NewSensorArgs,
    dry_run: bool,
) -> std::result::Result<(), CliError> {
    validate_name(&args.name).map_err(CliError::Usage)?;
    if args.args.is_empty() {
        return Err(CliError::Usage(anyhow::anyhow!(
            "sensor '{}' needs at least one --arg (the program to run)",
            args.name
        )));
    }
    let path = config_path.map_or_else(|| root.join("do-harness.toml"), Path::to_path_buf);
    let original = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(CliError::Usage(anyhow::anyhow!(
                "failed to read {}: {error}",
                path.display()
            )));
        }
    };
    // The existing file must already be valid: appending to a broken config
    // would blame the new sensor for an unrelated syntax error.
    let cfg = config::load(root, config_path)
        .await
        .map_err(CliError::Usage)?;
    if cfg.sensors.iter().any(|spec| spec.name == args.name)
        || cfg.sensor_names().iter().any(|name| name == &args.name)
    {
        return Err(CliError::Usage(anyhow::anyhow!(
            "sensor '{}' already exists; choose another name",
            args.name
        )));
    }
    if let Some(set) = &args.set {
        if !cfg.signal_sets.contains_key(set) {
            return Err(CliError::Usage(anyhow::anyhow!(
                "signal set '{set}' is not declared in {}; declare it before adding a sensor to it",
                path.display()
            )));
        }
    }
    let block = render_block(args);
    let mut updated = original.clone();
    updated.push_str(&block);
    if let Some(set) = &args.set {
        updated = extend_signal_set(&updated, set, &args.name).map_err(CliError::Usage)?;
    }
    if dry_run {
        println!("# {} would gain:\n{block}", path.display());
        if let Some(set) = &args.set {
            println!("# and signal set '{set}' would include '{}'", args.name);
        }
        return Ok(());
    }
    if let Some(script) = &args.script {
        write_stub(root, script).map_err(CliError::Usage)?;
    }
    std::fs::write(&path, &updated).map_err(|error| {
        CliError::Usage(anyhow::anyhow!(
            "failed to write {}: {error}",
            path.display()
        ))
    })?;
    match prove(root, config_path, args).await {
        Ok(()) => {
            println!("sensor '{}' added to {}", args.name, path.display());
            println!(
                "next: do-harness verify --only {} (runs the new sensor once)",
                args.name
            );
            Ok(())
        }
        Err(error) => {
            // Restore the exact previous bytes: a failed proof must not leave a
            // configuration that no selection accepts.
            let _ = std::fs::write(&path, &original);
            Err(error)
        }
    }
}

/// Proves the new entry through the same selection path `verify` uses.
async fn prove(
    root: &Path,
    config_path: Option<&Path>,
    args: &NewSensorArgs,
) -> std::result::Result<(), CliError> {
    let cfg = config::load(root, config_path)
        .await
        .map_err(CliError::Usage)?;
    let opts = VerifyOpts {
        set: args.set.clone(),
        only: vec![args.name.clone()],
        ..Default::default()
    };
    let selection = sensors::resolve_selection(&cfg, root, &opts).map_err(CliError::Usage)?;
    if selection.specs.iter().all(|spec| spec.name != args.name) {
        return Err(CliError::Usage(anyhow::anyhow!(
            "sensor '{}' is not selected by the configuration that was just written",
            args.name
        )));
    }
    Ok(())
}

/// Renders the `[[sensors]]` block, quoting every value as TOML.
fn render_block(args: &NewSensorArgs) -> String {
    use std::fmt::Write as _;

    let mut block = String::from("\n[[sensors]]\n");
    let _ = writeln!(block, "name = {}", toml_string(&args.name));
    let elements: Vec<String> = args.args.iter().map(|arg| toml_string(arg)).collect();
    let _ = writeln!(block, "argv = [{}]", elements.join(", "));
    if !args.when_changed.is_empty() {
        let globs: Vec<String> = args
            .when_changed
            .iter()
            .map(|glob| toml_string(glob))
            .collect();
        let _ = writeln!(block, "when-changed = [{}]", globs.join(", "));
    }
    block
}

/// Quotes a TOML string exactly as the `toml` crate does.
fn toml_string(value: &str) -> String {
    toml::Value::String(value.to_owned()).to_string()
}

/// Adds `name` to an existing single-line `[signal-sets]` array.
///
/// The edit is deliberately line-scoped and refuses ambiguous input: a
/// multi-line array or a re-opened table cannot be rewritten safely without
/// reformatting the user's file, so the caller is told to make that edit.
fn extend_signal_set(text: &str, set: &str, name: &str) -> anyhow::Result<String> {
    let mut lines: Vec<String> = text.lines().map(ToOwned::to_owned).collect();
    let header = lines.iter().position(|line| line.trim() == "[signal-sets]");
    let Some(header) = header else {
        anyhow::bail!("no [signal-sets] table to extend");
    };
    let end = lines
        .iter()
        .enumerate()
        .skip(header + 1)
        .find(|(_, line)| line.trim_start().starts_with('['))
        .map_or(lines.len(), |(index, _)| index);
    for line in lines.iter_mut().take(end).skip(header + 1) {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix(set) else {
            continue;
        };
        let Some(rest) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        let raw_array = rest.trim();
        let Some(inner) = raw_array
            .strip_prefix('[')
            .and_then(|v| v.strip_suffix(']'))
        else {
            anyhow::bail!(
                "signal set '{set}' is not a single-line array; add '{name}' to it by hand"
            );
        };
        let inner = inner.trim();
        let array = if inner.is_empty() {
            format!("[{}]", toml_string(name))
        } else {
            format!("[{inner}, {}]", toml_string(name))
        };
        *line = format!("{set} = {array}");
        return Ok(lines.join("\n") + "\n");
    }
    anyhow::bail!("signal set '{set}' has no assignment line in [signal-sets]")
}

/// Writes the optional stub script and marks it executable on Unix.
fn write_stub(root: &Path, script: &Path) -> anyhow::Result<()> {
    let path = if script.is_absolute() {
        script.to_path_buf()
    } else {
        root.join(script)
    };
    if path.exists() {
        anyhow::bail!(
            "refusing to overwrite existing script {}; choose another --script path",
            path.display()
        );
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = "#!/usr/bin/env bash\n\
                # Sensor stub scaffolded by `do-harness sensor new`.\n\
                # Replace this body with the check; exit 0 means pass and any\n\
                # non-zero status fails the sensor.\n\
                set -eu\n\
                echo \"sensor stub: not implemented\" >&2\n\
                exit 1\n";
    std::fs::write(&path, body)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
    }
    println!("scaffolded script {}", path.display());
    Ok(())
}

/// Sensor names must survive `--only`/`--exclude` and TOML dotted keys.
fn validate_name(name: &str) -> anyhow::Result<()> {
    let valid = !name.is_empty()
        && name.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '-'
                || character == '_'
        });
    if !valid {
        anyhow::bail!(
            "invalid sensor name '{name}': use lowercase ASCII letters, digits, '-' and '_'"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn blocks_quote_argv_and_omit_empty_globs() {
        let args = NewSensorArgs {
            name: "check".to_owned(),
            args: vec!["bash".to_owned(), "scripts/check \"quoted\".sh".to_owned()],
            when_changed: vec![],
            set: None,
            script: None,
        };
        let block = render_block(&args);
        assert!(block.contains("name = \"check\""));
        assert!(block.contains(r#"argv = ["bash", "scripts/check \"quoted\".sh"]"#));
        assert!(!block.contains("when-changed"));
    }

    #[test]
    fn a_single_line_set_gains_the_name() {
        let text =
            "[signal-sets]\nverification = [\"fmt\", \"check\"]\n\n[[sensors]]\nname = \"fmt\"\n";
        let updated = extend_signal_set(text, "verification", "lint").unwrap();
        assert!(updated.contains(r#"verification = ["fmt", "check", "lint"]"#));
        assert!(updated.contains("name = \"fmt\""));
    }

    #[test]
    fn an_empty_set_gains_only_the_name() {
        let text = "[signal-sets]\nfeedback = []\n";
        let updated = extend_signal_set(text, "feedback", "lint").unwrap();
        assert!(updated.contains(r#"feedback = ["lint"]"#));
    }

    #[test]
    fn a_multi_line_set_is_refused() {
        let text = "[signal-sets]\nverification = [\n  \"fmt\",\n]\n";
        let error = extend_signal_set(text, "verification", "lint").unwrap_err();
        assert!(error.to_string().contains("single-line"), "{error}");
    }

    #[test]
    fn an_unknown_set_is_refused() {
        let text = "[signal-sets]\nfeedback = []\n";
        let error = extend_signal_set(text, "verification", "lint").unwrap_err();
        assert!(error.to_string().contains("no assignment"), "{error}");
    }

    #[test]
    fn names_reject_uppercase_and_spaces() {
        assert!(validate_name("rust-check_2").is_ok());
        assert!(validate_name("RustCheck").is_err());
        assert!(validate_name("rust check").is_err());
        assert!(validate_name("").is_err());
    }
}

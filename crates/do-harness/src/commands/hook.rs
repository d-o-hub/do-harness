//! Hook management commands: `hook install/uninstall/status/diff`.
//!
//! Split out of `commands.rs` so that file stays under the 450-line
//! decomposition threshold; the `commands` re-export keeps the call sites in
//! `main.rs` unchanged.

use std::path::Path;

use anyhow::Result;

use crate::cli::HookAction;
use crate::doctor::describe_binary;
use crate::report::Format;
use crate::{config, hooks};

/// Prints the first line where an installed managed hook diverges from its
/// template body (line-based comparison; no LCS).
fn print_first_difference(name: &str, installed: &str, expected: &str) {
    let installed_lines: Vec<&str> = installed.lines().collect();
    let expected_lines: Vec<&str> = expected.lines().collect();
    let max = installed_lines.len().max(expected_lines.len());
    for idx in 0..max {
        let actual = installed_lines.get(idx).copied();
        let template = expected_lines.get(idx).copied();
        if actual != template {
            println!("  {name}: first difference at line {}", idx + 1);
            println!("    installed: {}", actual.unwrap_or("<end of file>"));
            println!("    template:  {}", template.unwrap_or("<end of file>"));
            return;
        }
    }
    println!("  {name}: content differs only by trailing bytes");
}

/// Dispatches hook management using the configured sensor split.
#[allow(clippy::too_many_lines)]
pub async fn hook(
    root: &Path,
    config_path: Option<&Path>,
    action: HookAction,
    dry_run: bool,
) -> Result<()> {
    let git_dir = hooks::find_git_dir(root)?;
    match action {
        HookAction::Install { force } => {
            let (target, _) = hooks::get_effective_hooks_dir(&git_dir, root);
            if dry_run {
                println!(
                    "Dry run: would install managed git hooks in {}",
                    target.display()
                );
                return Ok(());
            }
            let cfg = config::load(root, config_path).await?;
            let target = hooks::install(
                &git_dir,
                root,
                &cfg.hooks.pre_commit,
                &cfg.hooks.pre_push,
                force,
            )?;
            println!("Installed managed git hooks in {}", target.display());
        }
        HookAction::Uninstall => {
            if dry_run {
                println!("Dry run: would remove managed hooks");
                return Ok(());
            }
            hooks::uninstall(&git_dir, root)?;
            println!("Removed managed hooks");
        }
        HookAction::Status { format } => {
            let status = hooks::status(&git_dir, root);
            match format {
                Format::Text => {
                    let format_hook = |installed: bool, shadowed: bool| -> String {
                        if installed {
                            "installed".to_string()
                        } else if shadowed {
                            if let Some(ref cfg) = status.core_hooks_path {
                                format!("installed (shadowed by core.hooksPath={cfg})")
                            } else {
                                "installed (shadowed)".to_string()
                            }
                        } else {
                            "absent".to_string()
                        }
                    };
                    println!(
                        "pre-commit: {}  pre-push: {}  commit-msg: {}  binary: {} ({})",
                        format_hook(status.pre_commit, status.pre_commit_shadowed),
                        format_hook(status.pre_push, status.pre_push_shadowed),
                        format_hook(status.commit_msg, status.commit_msg_shadowed),
                        describe_binary(&status.binary),
                        if status.binary.present() {
                            "present"
                        } else {
                            "missing"
                        }
                    );
                }
                Format::Json => {
                    let json = serde_json::json!({
                        "pre_commit": status.pre_commit,
                        "pre_push": status.pre_push,
                        "commit_msg": status.commit_msg,
                        "pre_commit_shadowed": status.pre_commit_shadowed,
                        "pre_push_shadowed": status.pre_push_shadowed,
                        "commit_msg_shadowed": status.commit_msg_shadowed,
                        "core_hooks_path": status.core_hooks_path,
                        "hooks_dir": status.hooks_dir.display().to_string(),
                        "binary_present": status.binary.present(),
                        "binary_source": describe_binary(&status.binary)
                    });
                    println!("{json}");
                }
            }
            if !status.binary.present() {
                anyhow::bail!(
                    "do-harness binary missing at {}; build it or set DO_HARNESS_BIN",
                    status.binary.path().display()
                );
            }
            if !status.is_all_installed() {
                if status.is_shadowed() {
                    eprintln!(
                        "hint: managed hooks are shadowed by core.hooksPath; run `do-harness hook install` to install into the configured directory or unset core.hooksPath"
                    );
                } else {
                    eprintln!("hint: run `do-harness hook install` to (re)install managed hooks");
                }
            }
        }
        HookAction::Diff => {
            // Compare installed hook content against the bodies `hook install`
            // would write now; marker presence alone cannot detect a stale or
            // hand-edited managed hook.
            let cfg = config::load(root, config_path).await?;
            let status = hooks::status(&git_dir, root);
            let expected = [
                (
                    "pre-commit",
                    crate::hook_script::script_body(&crate::hook_script::only_args(
                        &cfg.hooks.pre_commit,
                    )),
                ),
                (
                    "pre-push",
                    crate::hook_script::script_body(&crate::hook_script::only_args(
                        &cfg.hooks.pre_push,
                    )),
                ),
                ("commit-msg", crate::hook_script::commit_msg_body()),
            ];
            let mut differences: Vec<(&str, Option<String>)> = Vec::new();
            for (name, body) in &expected {
                let path = status.hooks_dir.join(name);
                match std::fs::read_to_string(&path) {
                    Ok(installed) if installed == *body => {}
                    Ok(installed) => differences.push((name, Some(installed))),
                    Err(_) => differences.push((name, None)),
                }
            }
            if differences.is_empty() {
                println!("Hooks match installed templates.");
            } else {
                println!("One or more hooks differ or are missing:");
                for (name, installed) in &differences {
                    match installed {
                        Some(installed) => {
                            let template = expected
                                .iter()
                                .find(|(expected_name, _)| expected_name == name)
                                .map(|(_, body)| body.as_str())
                                .unwrap_or_default();
                            print_first_difference(name, installed, template);
                        }
                        None => println!("  {name}: missing"),
                    }
                }
                if status.is_shadowed() {
                    eprintln!(
                        "hint: managed hooks are shadowed by core.hooksPath; run `do-harness hook install` to install into the configured directory or unset core.hooksPath"
                    );
                } else {
                    eprintln!("hint: run `do-harness hook install` to (re)install managed hooks");
                }
                anyhow::bail!("managed hooks are out of date");
            }
        }
    }
    Ok(())
}

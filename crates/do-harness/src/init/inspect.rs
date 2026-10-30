//! Non-executing repository inspection and candidate discovery (`init --inspect`) (WF-11).
//!
//! Gathers detected project facts, candidate commands, missing tools, and
//! actionable language guidance without installing packages, writing files, or
//! executing project scripts.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::{Candidate, Language};
use crate::report::Format;

/// Actionable candidate command discovered from project metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateCommand {
    /// Associated sensor/task name.
    pub name: String,
    /// Probed execution command.
    pub command: String,
    /// Metadata origin of the candidate.
    pub source: String,
}

/// Inspection report payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectReport {
    /// Resolved language pack.
    pub resolved_language: String,
    /// Discovered repository facts.
    pub detected: Vec<String>,
    /// Candidate development signals for the resolved pack.
    pub candidates: Vec<Candidate>,
    /// Discovered candidate commands from scripts/tools.
    pub candidate_commands: Vec<CandidateCommand>,
    /// Missing tools required for candidate signals.
    pub missing_tools: Vec<String>,
    /// Actionable onboarding guidance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guidance: Option<String>,
}

fn tool_on_path(tool: &str) -> bool {
    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(tool);
        #[cfg(unix)]
        {
            const EXECUTABLE_BITS: u32 = 0o111;
            use std::os::unix::fs::PermissionsExt;
            if candidate.is_file() {
                if let Ok(meta) = candidate.metadata() {
                    if meta.permissions().mode() & EXECUTABLE_BITS != 0 {
                        return true;
                    }
                }
            }
        }
        #[cfg(windows)]
        {
            if candidate.is_file() {
                return true;
            }
            for ext in ["exe", "cmd", "bat"] {
                if candidate.with_extension(ext).is_file() {
                    return true;
                }
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            if candidate.is_file() {
                return true;
            }
        }
    }
    false
}

fn has_non_hidden_entries(root: &Path) -> bool {
    fs::read_dir(root).is_ok_and(|entries| {
        entries
            .filter_map(Result::ok)
            .any(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
    })
}

/// Gathers non-executing inspection facts and candidate commands without running any tools.
#[allow(clippy::too_many_lines)]
pub fn inspect_workspace(root: &Path, requested_language: Option<Language>) -> InspectReport {
    let existing_language = if root.join("do-harness.toml").exists() {
        fs::read_to_string(root.join("do-harness.toml"))
            .ok()
            .and_then(|text| toml::from_str::<toml::Value>(&text).ok())
            .and_then(|val| {
                val.get("language")
                    .and_then(|l| l.as_str())
                    .map(std::borrow::ToOwned::to_owned)
            })
    } else {
        None
    };

    let mut detected = Vec::new();
    let has_cargo = root.join("Cargo.toml").exists();
    let has_git = root.join(".git").exists();
    let has_pkg_json = root.join("package.json").exists();
    let has_py = root.join("requirements.txt").exists()
        || root.join("pyproject.toml").exists()
        || root.join("setup.py").exists();
    let has_go = root.join("go.mod").exists();
    let has_pnpm_ws = root.join("pnpm-workspace.yaml").exists();
    let has_turbo = root.join("turbo.json").exists();

    if has_cargo {
        detected.push("Cargo.toml".to_owned());
        let is_workspace = fs::read_to_string(root.join("Cargo.toml"))
            .is_ok_and(|text| text.contains("[workspace]"));
        detected.push(if is_workspace {
            "Rust workspace".to_owned()
        } else {
            "Rust crate".to_owned()
        });
    }
    if has_git {
        detected.push("Git repository".to_owned());
    }
    if has_pkg_json {
        detected.push("package.json".to_owned());
        if has_pnpm_ws && has_turbo {
            detected.push("pnpm+turbo workspace".to_owned());
        } else if has_pnpm_ws {
            detected.push("pnpm workspace".to_owned());
        } else if has_turbo {
            detected.push("turbo workspace".to_owned());
        }
    }
    if has_py {
        detected.push("Python project markers found".to_owned());
    }
    if has_go {
        detected.push("Go project markers found (go.mod)".to_owned());
    }

    let declared = match existing_language.as_deref() {
        Some("rust") => Some(Language::Rust),
        Some("generic") => Some(Language::Generic),
        Some("web") => Some(Language::Web),
        Some("node") => Some(Language::Node),
        _ => None,
    };

    let resolved_language = match requested_language {
        Some(lang) => lang,
        None if declared.is_some() => declared.unwrap_or(Language::Rust),
        None if has_cargo => Language::Rust,
        None if has_pkg_json || has_non_hidden_entries(root) => Language::Generic,
        None => Language::Rust,
    };

    let mut candidate_commands = Vec::new();
    let mut missing_tools = Vec::new();
    let mut guidance = None;

    let pm = if root.join("pnpm-lock.yaml").exists() || has_pnpm_ws {
        "pnpm"
    } else if root.join("yarn.lock").exists() {
        "yarn"
    } else {
        "npm"
    };

    if has_pkg_json {
        let pkg_content = fs::read_to_string(root.join("package.json")).unwrap_or_default();
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&pkg_content) {
            if let Some(scripts) = val.get("scripts").and_then(|s| s.as_object()) {
                for key in ["test", "lint", "typecheck", "build"] {
                    if scripts.contains_key(key) {
                        let cmd_str = if pm == "npm" {
                            if key == "test" {
                                "npm test".to_owned()
                            } else {
                                format!("npm run {key}")
                            }
                        } else {
                            format!("{pm} {key}")
                        };
                        candidate_commands.push(CandidateCommand {
                            name: key.to_owned(),
                            command: cmd_str,
                            source: format!("package.json scripts.{key}"),
                        });
                    }
                }
            }
        }
        if !tool_on_path(pm) {
            missing_tools.push(pm.to_owned());
        }
    }

    let candidates = match resolved_language {
        Language::Rust => {
            let cargo_ok = tool_on_path("cargo");
            if !cargo_ok {
                missing_tools.push("cargo".to_owned());
            }
            let status = if cargo_ok {
                super::detect::CandidateStatus::Pass
            } else {
                super::detect::CandidateStatus::Missing
            };
            vec![
                Candidate {
                    name: "fmt".into(),
                    status,
                    included: cargo_ok,
                    detail: String::new(),
                },
                Candidate {
                    name: "check".into(),
                    status,
                    included: cargo_ok,
                    detail: String::new(),
                },
                Candidate {
                    name: "clippy".into(),
                    status,
                    included: cargo_ok,
                    detail: String::new(),
                },
                Candidate {
                    name: "test".into(),
                    status,
                    included: cargo_ok,
                    detail: String::new(),
                },
                Candidate {
                    name: "loc".into(),
                    status,
                    included: cargo_ok,
                    detail: String::new(),
                },
            ]
        }
        Language::Node => {
            let pm_ok = tool_on_path(pm);
            candidate_commands
                .iter()
                .map(|cmd| Candidate {
                    name: cmd.name.clone(),
                    status: if pm_ok {
                        super::detect::CandidateStatus::Pass
                    } else {
                        super::detect::CandidateStatus::Missing
                    },
                    included: pm_ok,
                    detail: cmd.command.clone(),
                })
                .collect()
        }
        Language::Generic | Language::Web => Vec::new(),
    };

    if resolved_language == Language::Generic && has_pkg_json {
        guidance = Some(
            "A package.json was found. Run `do-harness init --language node` to scaffold with discovered Node/JS/TS sensors, or declare custom sensors in do-harness.toml under [[sensors]].".to_owned()
        );
    } else if resolved_language == Language::Generic && (has_py || has_go) {
        guidance = Some(
            "Generic pack selected. Declare trusted custom sensor commands in do-harness.toml under [[sensors]].".to_owned()
        );
    }

    let lang_str = match resolved_language {
        Language::Rust => "rust",
        Language::Node => "node",
        Language::Web => "web",
        Language::Generic => "generic",
    };

    InspectReport {
        resolved_language: lang_str.to_owned(),
        detected,
        candidates,
        candidate_commands,
        missing_tools,
        guidance,
    }
}

/// Executes non-executing inspection and outputs report.
pub fn run_inspect(root: &Path, language: Option<Language>, format: Format) -> Result<()> {
    let report = inspect_workspace(root, language);
    match format {
        Format::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&report).context("serialize inspect report")?
            );
        }
        Format::Text => {
            println!("Repository inspection: {}", root.display());
            println!("Resolved language pack: {}", report.resolved_language);
            println!("Detected facts:");
            if report.detected.is_empty() {
                println!("  (no markers found)");
            } else {
                for finding in &report.detected {
                    println!("  * {finding}");
                }
            }
            if !report.candidate_commands.is_empty() {
                println!("Candidate commands:");
                for c in &report.candidate_commands {
                    println!("  * {:<10} {} ({})", c.name, c.command, c.source);
                }
            }
            if !report.missing_tools.is_empty() {
                println!("Missing tools: {}", report.missing_tools.join(", "));
            }
            if let Some(msg) = &report.guidance {
                println!("Guidance:\n  {msg}");
            }
        }
    }
    Ok(())
}

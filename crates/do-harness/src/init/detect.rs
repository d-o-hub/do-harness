//! Repository inspection and candidate signal probing for `do-harness init`.
//!
//! Init derives the development contract from repository reality instead of
//! template assumptions: it detects what the repository is, probes the tools
//! each candidate sensor needs, and includes only signals whose required
//! tooling actually exists. Missing optional tools (cargo-deny/cargo-audit)
//! degrade a script-backed sensor but do not remove it, because those scripts
//! fail open locally and are enforced in CI.

use std::path::Path;
use std::process::Command;

use serde::Serialize;

use super::Language;

/// Probe outcome for one candidate sensor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CandidateStatus {
    /// Required tooling is present.
    Pass,
    /// Required tooling is present but an optional tool is missing; the
    /// sensor runs and degrades according to its own policy.
    Degraded,
    /// Required tooling is missing; the sensor is omitted from the config.
    Missing,
}

/// One probed candidate development signal.
#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    /// Sensor name as it would appear in `do-harness.toml`.
    pub name: String,
    /// Probe outcome.
    pub status: CandidateStatus,
    /// Whether the generated config includes the sensor.
    pub included: bool,
    /// Human-readable probe detail (empty when straightforward).
    pub detail: String,
}

/// Detection results for one repository.
pub struct Detection {
    /// Human-readable repository facts, in report order.
    pub findings: Vec<String>,
    /// Language pack resolved from the request or the repository.
    pub language: Language,
    /// Probed candidate signals for the resolved pack.
    pub candidates: Vec<Candidate>,
}

/// Inspects `root`, resolving the language pack and probing candidates.
///
/// Priority: an explicit `requested` pack; then the language declared by an
/// existing `do-harness.toml` (`existing_language`); then repository markers
/// (a `Cargo.toml` selects the Rust pack, another non-empty project selects
/// the generic pack, and an empty or hidden-only directory is greenfield
/// Rust so `init` can materialize a minimal crate and prove a baseline).
#[must_use]
pub fn inspect(
    root: &Path,
    requested: Option<Language>,
    existing_language: Option<&str>,
) -> Detection {
    let mut findings = Vec::new();
    let has_cargo = root.join("Cargo.toml").exists();
    let has_git = root.join(".git").exists();
    let has_package_json = root.join("package.json").exists();

    if has_cargo {
        findings.push("Cargo.toml".to_owned());
        let workspace = std::fs::read_to_string(root.join("Cargo.toml"))
            .is_ok_and(|text| text.contains("[workspace]"));
        findings.push(if workspace {
            "Rust workspace".to_owned()
        } else {
            "Rust crate".to_owned()
        });
    }
    if has_git {
        findings.push("Git repository".to_owned());
    }
    if has_package_json && !has_cargo {
        findings.push("package.json (no supported pack; generic sensors)".to_owned());
    }

    let declared = match existing_language {
        Some("rust") => Some(Language::Rust),
        Some("generic") => Some(Language::Generic),
        _ => None,
    };
    let language = match requested {
        Some(language) => language,
        None if declared.is_some() => {
            let language = declared.unwrap_or(Language::Rust);
            let name = match language {
                Language::Rust => "rust",
                Language::Generic => "generic",
            };
            findings.push(format!("existing do-harness.toml selects the {name} pack"));
            language
        }
        None if has_cargo => Language::Rust,
        None if has_package_json || has_non_hidden_entries(root) => Language::Generic,
        None => {
            findings.push("empty directory (greenfield rust)".to_owned());
            Language::Rust
        }
    };
    if language == Language::Generic && !findings.iter().any(|f| f.contains("generic")) {
        findings.push("generic pack selected (zero built-in sensors)".to_owned());
    }

    let candidates = match language {
        Language::Rust => rust_candidates(),
        Language::Generic => Vec::new(),
    };
    Detection {
        findings,
        language,
        candidates,
    }
}

/// Canonical specs for `language`, filtered to sensors the probes included.
///
/// Generic has no built-in sensors, so it always returns an empty list.
#[must_use]
pub fn included_specs(
    language: Language,
    candidates: &[Candidate],
) -> Vec<crate::config::SensorSpec> {
    match language {
        Language::Rust => crate::config::rust_pack()
            .into_iter()
            .filter(|spec| {
                candidates
                    .iter()
                    .any(|candidate| candidate.name == spec.name && candidate.included)
            })
            .collect(),
        Language::Generic => Vec::new(),
    }
}

/// Probes the Rust pack's candidate sensors.
fn rust_candidates() -> Vec<Candidate> {
    let cargo = probe("cargo", &["--version"]);
    let cargo_fmt = probe("cargo", &["fmt", "--version"]);
    let cargo_clippy = probe("cargo", &["clippy", "--version"]);
    let bash = probe("bash", &["--version"]);
    let cargo_deny = probe("cargo", &["deny", "--version"]);
    let cargo_audit = probe("cargo", &["audit", "--version"]);

    vec![
        candidate("fmt", cargo_fmt, cargo_fmt, ""),
        candidate("check", cargo, cargo, ""),
        candidate("clippy", cargo_clippy, cargo_clippy, ""),
        candidate("test", cargo, cargo, ""),
        candidate("loc", bash, bash, ""),
        candidate(
            "deps",
            bash,
            bash,
            optional_detail(cargo_deny, "cargo-deny"),
        ),
        candidate(
            "audit",
            bash,
            bash,
            optional_detail(cargo_audit, "cargo-audit"),
        ),
        candidate("commitlint", bash, bash, ""),
    ]
}

/// Builds a candidate, marking it degraded when required tooling is present
/// but `detail` reports a missing optional tool.
fn candidate(name: &str, required: bool, included: bool, detail: &str) -> Candidate {
    let status = if !required {
        CandidateStatus::Missing
    } else if detail.is_empty() {
        CandidateStatus::Pass
    } else {
        CandidateStatus::Degraded
    };
    Candidate {
        name: name.to_owned(),
        status,
        included: included && required,
        detail: detail.to_owned(),
    }
}

/// Explains a missing optional tool, or returns an empty detail when present.
fn optional_detail(present: bool, tool: &str) -> &'static str {
    if present {
        ""
    } else {
        match tool {
            "cargo-deny" => {
                "cargo-deny not installed; sensor fails open locally and is enforced in CI"
            }
            _ => "cargo-audit not installed; sensor fails open locally and is enforced in CI",
        }
    }
}

/// Runs a `<program> --version`-style probe; never uses a shell.
fn probe(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Whether `root` contains any non-hidden entry (an existing project).
fn has_non_hidden_entries(root: &Path) -> bool {
    std::fs::read_dir(root).is_ok_and(|entries| {
        entries
            .filter_map(Result::ok)
            .any(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// An empty directory resolves to greenfield Rust and reports it.
    #[test]
    fn empty_directory_is_greenfield_rust() {
        let dir = tempfile::tempdir().unwrap();
        let detection = inspect(dir.path(), None, None);
        assert_eq!(detection.language, Language::Rust);
        assert!(
            detection
                .findings
                .iter()
                .any(|finding| finding.contains("empty directory"))
        );
    }

    /// A `Cargo.toml` resolves to Rust and reports the crate/workspace kind.
    #[test]
    fn cargo_manifest_selects_rust() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        let detection = inspect(dir.path(), None, None);
        assert_eq!(detection.language, Language::Rust);
        assert!(detection.findings.iter().any(|f| f == "Cargo.toml"));
        assert!(detection.findings.iter().any(|f| f == "Rust crate"));
    }

    /// A workspace manifest reports "Rust workspace".
    #[test]
    fn workspace_manifest_reports_workspace() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[workspace]\n").unwrap();
        let detection = inspect(dir.path(), None, None);
        assert!(detection.findings.iter().any(|f| f == "Rust workspace"));
    }

    /// A non-Rust project resolves to the generic pack.
    #[test]
    fn package_json_selects_generic() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}\n").unwrap();
        let detection = inspect(dir.path(), None, None);
        assert_eq!(detection.language, Language::Generic);
        assert!(detection.candidates.is_empty());
    }

    /// An explicit request wins over repository detection.
    #[test]
    fn explicit_language_wins() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        let detection = inspect(dir.path(), Some(Language::Generic), None);
        assert_eq!(detection.language, Language::Generic);
    }

    /// A declared existing-config pack outranks directory markers.
    #[test]
    fn existing_config_language_wins_over_markers() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}\n").unwrap();
        let detection = inspect(dir.path(), None, Some("rust"));
        assert_eq!(detection.language, Language::Rust);
        assert!(detection.candidates.iter().any(|c| c.name == "fmt"));
    }

    /// Included specs follow the probe decisions, not the full pack.
    #[test]
    fn included_specs_follow_probes() {
        let candidates = vec![
            candidate("fmt", true, true, ""),
            candidate("check", false, false, ""),
            candidate("clippy", true, true, "optional tool missing"),
        ];
        let specs = included_specs(Language::Rust, &candidates);
        let names: Vec<&str> = specs.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["fmt", "clippy"]);
    }
}

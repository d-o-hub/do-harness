//! Repository inspection and candidate signal probing for `do-harness init`.
//!
//! Init derives the development contract from repository reality instead of
//! template assumptions: it detects what the repository is, probes the tools
//! each candidate sensor needs, and includes only signals whose required
//! tooling actually exists. Missing optional tools (cargo-deny/cargo-audit)
//! degrade a script-backed sensor but do not remove it, because those scripts
//! fail open locally and are enforced in CI.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::Language;
use super::diagnostic::InitDiagnostic;

/// Probe outcome for one candidate sensor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Structured onboarding diagnostics (malformed manifests, unsupported
    /// Node commands, missing package managers).
    pub diagnostics: Vec<InitDiagnostic>,
    /// Node-pack sensor specs from the single probe; empty unless Node resolved.
    pub node_specs: Vec<crate::config::SensorSpec>,
}

/// Inspects `root`, resolving the language pack and probing candidates.
///
/// Priority: an explicit `requested` pack; then the language declared by an
/// existing `do-harness.toml` (`existing_language`); then repository markers
/// (a `Cargo.toml` selects the Rust pack; a `package.json` that proves a
/// supported Node command selects the Node pack; another non-empty project
/// selects the generic pack; and an empty or hidden-only directory is
/// greenfield Rust so `init` can materialize a minimal crate and prove a
/// baseline). A malformed or unsupported `package.json` stays Generic and
/// emits a structured [`InitDiagnostic`] instead of a plausible pass.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn inspect(
    root: &Path,
    requested: Option<Language>,
    existing_language: Option<&str>,
) -> Detection {
    let mut findings = Vec::new();
    let has_cargo = root.join("Cargo.toml").exists();
    let has_git = root.join(".git").exists();
    let manifest = super::node_manifest::analyze_node_manifest(root);
    let has_package_json = manifest.has_package_json;

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
    if has_package_json {
        findings.push("package.json".to_owned());
        let has_pnpm_ws = root.join("pnpm-workspace.yaml").exists();
        let has_turbo = root.join("turbo.json").exists();
        if has_pnpm_ws && has_turbo {
            findings.push("pnpm+turbo workspace".to_owned());
        } else if has_pnpm_ws {
            findings.push("pnpm workspace".to_owned());
        } else if has_turbo {
            findings.push("turbo workspace".to_owned());
        }
    }

    let declared = match existing_language {
        Some("rust") => Some(Language::Rust),
        Some("generic") => Some(Language::Generic),
        Some("web") => Some(Language::Web),
        Some("node") => Some(Language::Node),
        _ => None,
    };

    let mut diagnostics = Vec::new();
    if let Some(error) = &manifest.malformed {
        diagnostics.push(InitDiagnostic::invalid_package_json("package.json", error));
    }

    let language = match requested {
        Some(language) => language,
        None if declared.is_some() => {
            let language = declared.unwrap_or(Language::Rust);
            let name = match language {
                Language::Rust => "rust",
                Language::Generic => "generic",
                Language::Web => "web",
                Language::Node => "node",
            };
            findings.push(format!("existing do-harness.toml selects the {name} pack"));
            language
        }
        None if has_cargo => Language::Rust,
        None if has_package_json => {
            if manifest.malformed.is_none() && !manifest.supported_commands().is_empty() {
                Language::Node
            } else {
                if manifest.malformed.is_none() {
                    diagnostics.push(InitDiagnostic::no_supported_node_commands("package.json"));
                }
                Language::Generic
            }
        }
        None if has_non_hidden_entries(root) => Language::Generic,
        None => {
            findings.push("empty directory (greenfield rust)".to_owned());
            Language::Rust
        }
    };
    if language == Language::Generic && !findings.iter().any(|f| f.contains("generic")) {
        findings.push("generic pack selected (zero built-in sensors)".to_owned());
    }
    if language == Language::Web && !findings.iter().any(|f| f.contains("web")) {
        findings.push("web pack selected (web-ui audit sensors)".to_owned());
    }
    if language == Language::Node && !findings.iter().any(|f| f.contains("node")) {
        findings.push("node pack selected (JS/TS sensors)".to_owned());
    }

    let (candidates, node_specs) = match language {
        Language::Rust => (rust_candidates(), Vec::new()),
        Language::Node => {
            let probe = super::node::probe_node_full(root);
            if !probe.package_manager_present {
                diagnostics.push(InitDiagnostic::missing_package_manager(
                    &probe.package_manager,
                    "package.json",
                ));
            }
            (probe.candidates, probe.specs)
        }
        Language::Generic | Language::Web => (Vec::new(), Vec::new()),
    };
    Detection {
        findings,
        language,
        candidates,
        diagnostics,
        node_specs,
    }
}

/// Canonical specs for `language`, filtered to sensors the probes included.
///
/// Node specs come from the single probe stored on [`Detection::node_specs`];
/// this helper serves the built-in Rust pack. Generic and Web have no
/// built-in sensors, so they always return an empty list.
#[must_use]
pub fn included_specs(
    _root: &Path,
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
        Language::Generic | Language::Node | Language::Web => Vec::new(),
    }
}

/// Probes the Rust pack's candidate sensors.
fn rust_candidates() -> Vec<Candidate> {
    let cargo = probe("cargo", &["--version"]);
    let cargo_fmt = probe("cargo", &["fmt", "--version"]);
    let cargo_clippy = probe("cargo", &["clippy", "--version"]);
    let cargo_nextest = probe("cargo", &["nextest", "--version"]);
    let cargo_llvm_cov = probe("cargo", &["llvm-cov", "--version"]);
    let bash = probe("bash", &["--version"]);
    let cargo_deny = probe("cargo", &["deny", "--version"]);
    let cargo_audit = probe("cargo", &["audit", "--version"]);

    vec![
        candidate("fmt", cargo_fmt, cargo_fmt, ""),
        candidate("check", cargo, cargo, ""),
        candidate("clippy", cargo_clippy, cargo_clippy, ""),
        // The generated `test` sensor is a wrapper that prefers nextest and
        // falls back to `cargo test` with its own zero-test guard, so the
        // sensor exists wherever cargo does. nextest's absence is reported as
        // a degraded detail, not as an omitted sensor.
        candidate(
            "test",
            cargo,
            cargo,
            if cargo_nextest {
                ""
            } else {
                "cargo-nextest not installed; the test sensor falls back to cargo test"
            },
        ),
        candidate("doctest", cargo, cargo, ""),
        // Omitted rather than degraded: the script prints `SKIP:` without the
        // tool, and a skip makes `--strict` evidence non-clean, so the sensor
        // could not stay in the pack. The detail names the remedy instead of
        // leaving a bare `MISSING (omitted)`.
        candidate(
            "coverage",
            cargo_llvm_cov,
            cargo_llvm_cov,
            if cargo_llvm_cov {
                ""
            } else {
                "cargo-llvm-cov not installed; omitted because a SKIP would fail --strict (install it, then re-run init --force)"
            },
        ),
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
    // Goes through `shell::command` so Windows `.cmd` shims (npm/npx/pnpm) and
    // Git Bash are resolved the same way the generated sensors will be.
    let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
    crate::shell::command(program, &args)
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
/// `.gitignore` entries the harness needs; appended, never clobbered.
pub const GITIGNORE_ENTRIES: &str = ".do-harness/\n.agents/events/\n";
pub const GITIGNORE_RUST_ENTRIES: &str = "/target\n/lcov.info\n";

#[must_use]
pub fn gitignore_entries(language: Language) -> String {
    let mut entries = GITIGNORE_ENTRIES.to_owned();
    if language == Language::Rust {
        entries.push_str(GITIGNORE_RUST_ENTRIES);
    }
    entries
}

/// Appends the harness `.gitignore` entries when missing; creates the file
/// when it does not exist yet.
pub fn append_gitignore(
    root: &Path,
    language: Language,
    written: &mut Vec<String>,
    skipped: &mut Vec<String>,
) -> anyhow::Result<()> {
    use anyhow::Context as _;
    let entries = gitignore_entries(language);
    let path = root.join(".gitignore");
    let Ok(existing) = std::fs::read_to_string(&path) else {
        std::fs::write(&path, &entries)
            .with_context(|| format!("failed to write {}", path.display()))?;
        written.push(".gitignore".to_owned());
        return Ok(());
    };
    let existing_lines: std::collections::HashSet<&str> = existing.lines().collect();
    let missing: Vec<&str> = entries
        .lines()
        .filter(|line| !line.is_empty() && !existing_lines.contains(line))
        .collect();
    if missing.is_empty() {
        skipped.push(".gitignore".to_owned());
        return Ok(());
    }
    let mut updated = existing;
    if !updated.ends_with('\n') {
        updated.push('\n');
    }
    for entry in &missing {
        updated.push_str(entry);
        updated.push('\n');
    }
    std::fs::write(&path, updated)
        .with_context(|| format!("failed to write {}", path.display()))?;
    written.push(".gitignore".to_owned());
    Ok(())
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
                .any(|f| f.contains("empty directory"))
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

    const SUPPORTED_PKG: &str = r#"{"scripts": {"test": "node --test"}}"#;

    fn write_pkg(root: &Path, body: &str) {
        std::fs::write(root.join("package.json"), body).unwrap();
    }

    /// A package.json without supported commands stays Generic and emits a
    /// diagnostic instead of claiming a meaningful green result.
    #[test]
    fn unsupported_package_json_selects_generic_with_diagnostic() {
        let dir = tempfile::tempdir().unwrap();
        write_pkg(dir.path(), "{}\n");
        let detection = inspect(dir.path(), None, None);
        assert_eq!(detection.language, Language::Generic);
        assert_eq!(detection.candidates.len(), 0);
        assert!(
            detection
                .diagnostics
                .iter()
                .any(|d| { d.code == crate::init::diagnostic::code::NO_SUPPORTED_NODE_COMMANDS })
        );
    }

    /// A package.json proving a supported Node command selects the Node pack.
    #[test]
    fn supported_package_json_selects_node() {
        let dir = tempfile::tempdir().unwrap();
        write_pkg(dir.path(), SUPPORTED_PKG);
        let detection = inspect(dir.path(), None, None);
        assert_eq!(detection.language, Language::Node);
        // Detection owns the language resolution and the manifest-derived
        // candidates: the test signal is proven by the manifest regardless of
        // tool availability. Inclusion is the probe's call (it depends on a
        // runnable package manager) and is asserted in init_node_auto.
        assert_eq!(detection.candidates.len(), 4);
        assert!(detection.candidates.iter().any(|c| c.name == "test"));
    }

    /// A malformed package.json stays Generic and emits a diagnostic.
    #[test]
    fn malformed_package_json_selects_generic_with_diagnostic() {
        let dir = tempfile::tempdir().unwrap();
        write_pkg(dir.path(), "{oops");
        let detection = inspect(dir.path(), None, None);
        assert_eq!(detection.language, Language::Generic);
        assert!(
            detection
                .diagnostics
                .iter()
                .any(|d| { d.code == crate::init::diagnostic::code::INVALID_PACKAGE_JSON })
        );
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
        write_pkg(dir.path(), "{}\n");
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
        let dummy = std::path::Path::new(".");
        let specs = included_specs(dummy, Language::Rust, &candidates);
        let names: Vec<&str> = specs.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["fmt", "clippy"]);
    }
}

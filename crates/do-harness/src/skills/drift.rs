//! `skills drift`: opt-in provenance check for deliberately shared skill trees.
//!
//! Shared skills are copied between repositories on purpose, so drift cannot be
//! detected by comparing a repository against itself. A manifest names the
//! skills a repository manages from an upstream and pins each one to the
//! upstream commit plus the digest of the managed tree.
//!
//! Every stage is offline and deterministic, in the same spirit as the rest of
//! the `skills` surface: the check reads the local tree, hashes it, and compares
//! it with the pin. It never fetches, never writes, and never inspects a skill
//! the manifest does not name. The commit and the digest are the anchors; a
//! version string is informational and is never compared. A missing or empty
//! manifest is a usage error (exit `2`), never a green "nothing to check".

use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use super::tree::tree_digest;
use crate::CliError;
use crate::report::Format;

/// Default manifest location, relative to the repository root.
pub const MANIFEST_PATH: &str = ".agents/skills-manifest.toml";

/// Length of a SHA-256 digest in hexadecimal characters.
const SHA256_HEX_LEN: usize = 64;
/// Length of a commit pin in hexadecimal characters (a full SHA-1 object ID).
const COMMIT_HEX_LEN: usize = 40;
/// Status column width in the text report.
const STATUS_WIDTH: usize = 7;

/// One managed skill as declared in the manifest.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedSkill {
    /// Skill name, used in reports and to identify the entry.
    pub name: String,
    /// Repository-relative path of the managed skill directory.
    pub path: String,
    /// Upstream repository the pin came from (`owner/repo`).
    pub upstream: String,
    /// Path of the skill inside the upstream repository.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_path: Option<String>,
    /// Informational human version; never compared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Pinned upstream commit (a full 40-hex object ID); the provenance anchor.
    pub commit: String,
    /// Pinned digest of the managed skill tree; the content anchor.
    pub content_sha256: String,
}

/// A parsed manifest: the managed skills, in declaration order.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Managed skill entries (`[[skills]]` tables).
    #[serde(default)]
    pub skills: Vec<ManagedSkill>,
}

/// Outcome of checking one managed skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// The local tree matches the pinned digest.
    Ok,
    /// The tree exists but hashes to something else.
    Drift,
    /// The managed path is not a directory.
    Missing,
}

/// Per-skill result of a drift check.
#[derive(Debug, Clone, Serialize)]
pub struct SkillReport {
    /// Skill name from the manifest.
    pub name: String,
    /// Repository-relative managed path.
    pub path: String,
    /// Upstream repository from the manifest.
    pub upstream: String,
    /// Pinned commit from the manifest.
    pub commit: String,
    /// Pinned tree digest from the manifest.
    pub expected: String,
    /// Observed tree digest, absent when the path is missing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<String>,
    /// Check outcome.
    pub status: Status,
}

/// Result of checking every managed skill.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    /// Manifest location as reported to the caller.
    pub manifest: String,
    /// Per-skill results, ordered by path.
    pub skills: Vec<SkillReport>,
}

impl Report {
    /// Number of skills that did not match their pin.
    #[must_use]
    pub fn drifted(&self) -> usize {
        self.skills
            .iter()
            .filter(|skill| skill.status != Status::Ok)
            .count()
    }
}

/// Resolves the manifest path: an explicit override, else [`MANIFEST_PATH`].
#[must_use]
pub fn manifest_path(root: &Path, override_path: Option<&Path>) -> PathBuf {
    match override_path {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        Some(path) => root.join(path),
        None => root.join(MANIFEST_PATH),
    }
}

/// Parses and validates manifest text.
///
/// # Errors
///
/// Returns an error naming the offending entry when the TOML is malformed or an
/// entry carries an unknown key, an empty name or upstream, a path that is
/// absolute or escapes the repository, a duplicate name or path, a commit pin
/// that is not a full 40-hex object ID, or a digest that is not 64 hexadecimal
/// characters.
pub fn parse(text: &str) -> Result<Manifest> {
    let manifest: Manifest = toml::from_str(text).context("invalid skills manifest")?;
    validate(&manifest)?;
    Ok(manifest)
}

/// Rejects manifests whose entries cannot be checked deterministically.
fn validate(manifest: &Manifest) -> Result<()> {
    if manifest.skills.is_empty() {
        bail!(
            "manifest lists no managed skills; add at least one [[skills]] entry or drop the skills-drift check"
        );
    }
    let mut names = HashSet::new();
    let mut paths = HashSet::new();
    for skill in &manifest.skills {
        if skill.name.trim().is_empty() {
            bail!("a [[skills]] entry has an empty name");
        }
        if !names.insert(skill.name.as_str()) {
            bail!("duplicate managed skill name: {}", skill.name);
        }
        check_relative_path(&skill.name, "path", &skill.path)?;
        if let Some(upstream_path) = &skill.upstream_path {
            check_relative_path(&skill.name, "upstream_path", upstream_path)?;
        }
        if !paths.insert(skill.path.as_str()) {
            bail!("duplicate managed skill path: {}", skill.path);
        }
        if skill.upstream.trim().is_empty() {
            bail!("managed skill '{}' has an empty upstream", skill.name);
        }
        if !is_hex(&skill.commit, COMMIT_HEX_LEN) {
            bail!(
                "managed skill '{}' has an invalid commit pin '{}': expected a full {COMMIT_HEX_LEN}-character hexadecimal commit ID",
                skill.name,
                skill.commit
            );
        }
        if !is_hex(&skill.content_sha256, SHA256_HEX_LEN) {
            bail!(
                "managed skill '{}' has an invalid content_sha256 '{}': expected {SHA256_HEX_LEN} hexadecimal characters",
                skill.name,
                skill.content_sha256
            );
        }
    }
    Ok(())
}

/// Requires a repository-relative path built from normal components only.
fn check_relative_path(name: &str, field: &str, value: &str) -> Result<()> {
    let path = Path::new(value);
    let normalized = !value.trim().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
    if normalized {
        return Ok(());
    }
    bail!(
        "managed skill '{name}' has an invalid {field} '{value}': use a repository-relative path without '..'"
    )
}

/// Whether `value` is exactly `len` hexadecimal characters.
fn is_hex(value: &str, len: usize) -> bool {
    value.len() == len && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Checks every managed skill against its pin.
///
/// The manifest path check is lexical, so every managed directory is resolved
/// and required to stay under the canonical repository root: a symlinked
/// top-level path must not make the digest describe content outside the
/// repository.
///
/// # Errors
///
/// Returns an error when the root or a managed tree cannot be resolved or
/// hashed, and when a managed path resolves outside the repository root.
pub fn evaluate(root: &Path, manifest: &Manifest) -> Result<Report> {
    let root = root
        .canonicalize()
        .with_context(|| format!("resolving repository root {}", root.display()))?;
    let mut skills = Vec::with_capacity(manifest.skills.len());
    for skill in &manifest.skills {
        let directory = root.join(&skill.path);
        let (status, actual) = if directory.is_dir() {
            let directory = directory
                .canonicalize()
                .with_context(|| format!("resolving managed skill '{}'", skill.name))?;
            ensure_within_root(&root, &directory, skill)?;
            let digest = tree_digest(&directory)
                .with_context(|| format!("hashing managed skill '{}'", skill.name))?;
            let status = if digest.eq_ignore_ascii_case(&skill.content_sha256) {
                Status::Ok
            } else {
                Status::Drift
            };
            (status, Some(digest))
        } else {
            (Status::Missing, None)
        };
        skills.push(SkillReport {
            name: skill.name.clone(),
            path: skill.path.clone(),
            upstream: skill.upstream.clone(),
            commit: skill.commit.clone(),
            expected: skill.content_sha256.to_ascii_lowercase(),
            actual,
            status,
        });
    }
    skills.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(Report {
        manifest: String::new(),
        skills,
    })
}

/// Rejects a resolved managed directory that sits outside the repository root.
///
/// The manifest's path check is lexical, so `is_dir()` would happily follow a
/// symlinked top-level path and hash content the repository does not own.
fn ensure_within_root(root: &Path, directory: &Path, skill: &ManagedSkill) -> Result<()> {
    if directory.starts_with(root) {
        return Ok(());
    }
    bail!(
        "managed skill '{}' resolves outside the repository root: '{}' -> {}; pin a real directory inside the root",
        skill.name,
        skill.path,
        directory.display()
    )
}

/// Runs `do-harness skills drift`.
///
/// Exit codes are the verdict: `0` when every managed skill matches its pin,
/// `1` when at least one skill drifted or is missing, and `2` when there is no
/// usable manifest or a managed tree cannot be read. A missing manifest is a
/// usage error rather than a vacuous pass: the command only exists to check
/// pins, so "nothing to check" must not read as green.
///
/// # Errors
///
/// Returns [`CliError::Usage`] when the manifest is absent, unreadable, or
/// invalid (including one that lists no skills), when a managed tree cannot be
/// hashed, and when a managed path resolves outside the repository root;
/// returns [`CliError::Verify`] when a managed skill no longer matches its
/// pinned digest.
pub fn run(root: &Path, override_path: Option<&Path>, format: Format) -> Result<(), CliError> {
    let path = manifest_path(root, override_path);
    let display = display_path(root, &path);
    if !path.is_file() {
        return Err(CliError::Usage(anyhow::anyhow!(
            "no skills manifest at {display}; add [[skills]] entries with pinned commit and content_sha256, or drop the skills-drift check"
        )));
    }

    let text = fs::read_to_string(&path)
        .with_context(|| format!("reading manifest {}", path.display()))
        .map_err(CliError::Usage)?;
    let manifest = parse(&text).map_err(CliError::Usage)?;
    let mut report = evaluate(root, &manifest).map_err(CliError::Usage)?;
    report.manifest = display;
    emit(&report, format);

    let drifted = report.drifted();
    if drifted == 0 {
        return Ok(());
    }
    Err(CliError::Verify(anyhow::anyhow!(
        "{drifted} of {} managed skill(s) drifted from their pinned digest",
        report.skills.len()
    )))
}

/// Repository-relative rendering of a path when it sits under the root.
///
/// Separators are normalized to `/` so the report is identical on every
/// platform and matches the manifest's own forward-slashed paths.
fn display_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Prints the report in the requested format.
fn emit(report: &Report, format: Format) {
    match format {
        Format::Json => println!(
            "{}",
            serde_json::json!({
                "schema_version": 1,
                "manifest": report.manifest,
                "managed": report.skills.len(),
                "drifted": report.drifted(),
                "skills": report.skills,
            })
        ),
        Format::Text => {
            for skill in &report.skills {
                let label = match skill.status {
                    Status::Ok => "OK".to_owned(),
                    Status::Drift => "DRIFT".to_owned(),
                    Status::Missing => "MISSING".to_owned(),
                };
                let detail = match &skill.actual {
                    Some(actual) => format!("expected={} actual={actual}", skill.expected),
                    None => format!("expected={}", skill.expected),
                };
                println!(
                    "{label:<STATUS_WIDTH$} {} {} {detail}",
                    skill.name, skill.path
                );
            }
            println!(
                "skills drift: {} managed, {} drifted",
                report.skills.len(),
                report.drifted()
            );
        }
    }
}

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

use super::cache::sha256_hex;
use crate::CliError;
use crate::report::Format;

/// Default manifest location, relative to the repository root.
pub const MANIFEST_PATH: &str = ".agents/skills-manifest.toml";

/// Length of a SHA-256 digest in hexadecimal characters.
const SHA256_HEX_LEN: usize = 64;
/// Shortest accepted commit pin (a full SHA-1 is 40 characters).
const COMMIT_MIN_LEN: usize = 7;
/// Longest accepted commit pin.
const COMMIT_MAX_LEN: usize = 40;
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
    /// Pinned upstream commit (hex); the provenance anchor.
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
/// absolute or escapes the repository, a duplicate name or path, or a pin that
/// is not hexadecimal.
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
        if !is_hex(&skill.commit, COMMIT_MIN_LEN, COMMIT_MAX_LEN) {
            bail!(
                "managed skill '{}' has an invalid commit pin '{}': expected {COMMIT_MIN_LEN}-{COMMIT_MAX_LEN} hexadecimal characters",
                skill.name,
                skill.commit
            );
        }
        if !is_hex(&skill.content_sha256, SHA256_HEX_LEN, SHA256_HEX_LEN) {
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

/// Whether `value` is hexadecimal within the inclusive length bounds.
fn is_hex(value: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Digest of a directory tree: `sha256` over `<path>\0<file-sha256>\n` lines.
///
/// Paths are forward-slashed, sorted by byte order, and framed so no
/// concatenation is ambiguous; empty directories do not appear. File symlinks
/// are followed (the target's content is hashed), a symlinked directory is an
/// error because its contents are not well-defined for a pinned tree, and other
/// file types are rejected rather than silently skipped.
///
/// # Errors
///
/// Returns an error when the tree cannot be read or contains an entry the digest
/// cannot describe (a symlinked directory, a socket, or a fifo).
pub fn tree_digest(dir: &Path) -> Result<String> {
    let mut files = Vec::new();
    collect_files(dir, "", &mut files)?;
    files.sort_by(|left, right| left.0.cmp(&right.0));

    let mut canonical = String::new();
    for (relative, absolute) in files {
        let bytes = fs::read(&absolute).with_context(|| format!("reading {relative}"))?;
        canonical.push_str(&relative);
        canonical.push('\0');
        canonical.push_str(&sha256_hex(&bytes));
        canonical.push('\n');
    }
    Ok(sha256_hex(canonical.as_bytes()))
}

/// Collects `(relative path, absolute path)` for every file under `dir`.
fn collect_files(dir: &Path, prefix: &str, out: &mut Vec<(String, PathBuf)>) -> Result<()> {
    let entries = fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))?;
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let relative = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_files(&entry.path(), &relative, out)?;
        } else if file_type.is_file() {
            out.push((relative, entry.path()));
        } else if file_type.is_symlink() {
            if fs::metadata(entry.path())?.is_dir() {
                bail!(
                    "managed skill tree contains a symlinked directory: {relative}; pin real files so the digest stays well-defined"
                );
            }
            out.push((relative, entry.path()));
        } else {
            bail!("managed skill tree contains an unsupported file type: {relative}");
        }
    }
    Ok(())
}

/// Checks every managed skill against its pin.
///
/// # Errors
///
/// Returns an error when a managed tree cannot be hashed.
pub fn evaluate(root: &Path, manifest: &Manifest) -> Result<Report> {
    let mut skills = Vec::with_capacity(manifest.skills.len());
    for skill in &manifest.skills {
        let directory = root.join(&skill.path);
        let (status, actual) = if directory.is_dir() {
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
/// invalid (including one that lists no skills), and when a managed tree cannot
/// be hashed; returns [`CliError::Verify`] when a managed skill no longer
/// matches its pinned digest.
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

//! Skill catalog: frontmatter-only metadata for progressive disclosure.
//!
//! The catalog reads each `SKILL.md` frontmatter block and nothing else, so
//! building it never loads a full skill body. Catalog content is repository
//! data and therefore untrusted: malformed, unreadable, escaping, and
//! duplicate skills become warnings and are excluded rather than fatal, and no
//! frontmatter value is ever interpreted as a command.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::cache::{self, Cache};

/// Repository-relative directory whose immediate children are skills.
pub const SKILL_ROOT: &str = ".agents/skills";

/// Upper bound on the frontmatter block read from a `SKILL.md`.
///
/// The structure gate caps `description` at 1024 characters, so a block beyond
/// this bound is malformed by construction rather than merely large; bounding
/// the read is what keeps catalog construction from loading skill bodies.
const MAX_FRONTMATTER_BYTES: usize = 64 * 1024;

/// Selection metadata for one cataloged skill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillMetadata {
    /// Frontmatter `name`.
    pub name: String,
    /// Frontmatter `description`.
    pub description: String,
    /// Frontmatter `metadata.short-description`, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short_description: Option<String>,
    /// Repository-relative, forward-slashed path of the skill's `SKILL.md`.
    pub path: String,
    /// Canonical path of that `SKILL.md`; the catalog identity.
    #[serde(skip)]
    pub canonical_path: PathBuf,
}

/// Cataloged skills plus non-fatal diagnostics.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    /// Deduplicated skills, ordered by name.
    pub skills: Vec<SkillMetadata>,
    /// Diagnostics for skipped, malformed, or duplicate skills.
    pub warnings: Vec<String>,
}

/// Scans `<root>/.agents/skills/*/SKILL.md` into a deduplicated catalog.
///
/// A missing skill root is not an error: it yields an empty catalog, so
/// `skills suggest` on a repository with no skills still succeeds.
///
/// # Errors
///
/// Returns an error only when the skill root exists but cannot be listed.
pub fn scan(root: &Path) -> Result<Catalog> {
    let skills_root = root.join(SKILL_ROOT);
    let mut warnings = Vec::new();
    if !skills_root.is_dir() {
        return Ok(Catalog::default());
    }
    let entries = fs::read_dir(&skills_root)
        .map_err(|err| anyhow::anyhow!("cannot read {}: {err}", skills_root.display()))?;
    let canonical_root = fs::canonicalize(&skills_root).unwrap_or_else(|_| skills_root.clone());
    let mut cache = Cache::load(root, &mut warnings);

    let mut found: Vec<SkillMetadata> = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let skill_md = dir.join("SKILL.md");
        if !skill_md.is_file() {
            continue;
        }
        if let Some(metadata) =
            read_skill(root, &canonical_root, &skill_md, &mut cache, &mut warnings)
        {
            found.push(metadata);
        }
    }

    // Deterministic identity: never depend on `read_dir` order. The first entry
    // per name wins; every later one is reported with both paths.
    found.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.path.cmp(&b.path)));
    let mut skills: Vec<SkillMetadata> = Vec::with_capacity(found.len());
    for candidate in found {
        match skills.iter().find(|kept| kept.name == candidate.name) {
            Some(kept) => warnings.push(format!(
                "duplicate skill name '{}': keeping {} and ignoring {}",
                candidate.name, kept.path, candidate.path
            )),
            None => skills.push(candidate),
        }
    }

    cache.store(root, &skills, &mut warnings);
    Ok(Catalog { skills, warnings })
}

/// Reads one `SKILL.md` into metadata, or records a warning and skips it.
fn read_skill(
    root: &Path,
    canonical_root: &Path,
    skill_md: &Path,
    cache: &mut Cache,
    warnings: &mut Vec<String>,
) -> Option<SkillMetadata> {
    let canonical = match fs::canonicalize(skill_md) {
        Ok(path) => path,
        Err(err) => {
            warnings.push(format!("cannot resolve {}: {err}", skill_md.display()));
            return None;
        }
    };
    // Containment check on the *canonical* path, so a symlinked alias pointing
    // outside the skill root is rejected rather than followed.
    if !canonical.starts_with(canonical_root) {
        warnings.push(format!(
            "skill {} resolves outside {}; ignored",
            skill_md.display(),
            canonical_root.display()
        ));
        return None;
    }
    let rel = relative_path(root, skill_md);
    let block = match read_frontmatter_block(skill_md) {
        Ok(Some(block)) => block,
        Ok(None) => {
            warnings.push(format!(
                "{rel}: missing or unterminated --- frontmatter block; ignored"
            ));
            return None;
        }
        Err(err) => {
            warnings.push(format!("cannot read {rel}: {err}"));
            return None;
        }
    };

    // The cache is validated against the frontmatter bytes actually on disk, so
    // a stale or corrupted entry can never change what the catalog reports: any
    // mismatch falls through to a fresh parse of this skill.
    let frontmatter_sha256 = cache::sha256_hex(block.as_bytes());
    if let Some(cached) = cache.lookup(&canonical, &frontmatter_sha256) {
        return Some(SkillMetadata {
            name: cached.name.clone(),
            description: cached.description.clone(),
            short_description: cached.short_description.clone(),
            path: rel,
            canonical_path: canonical,
        });
    }

    let parsed: Frontmatter = match serde_yaml::from_str(&block) {
        Ok(parsed) => parsed,
        Err(err) => {
            warnings.push(format!("{rel}: malformed frontmatter: {err}"));
            return None;
        }
    };
    let (Some(name), Some(description)) = (parsed.name, parsed.description) else {
        warnings.push(format!(
            "{rel}: frontmatter needs both name and description; ignored"
        ));
        return None;
    };
    let short_description = parsed.metadata.short_description;
    cache.remember(
        &canonical,
        &name,
        &description,
        short_description.clone(),
        &frontmatter_sha256,
    );
    Some(SkillMetadata {
        name,
        description,
        short_description,
        path: rel,
        canonical_path: canonical,
    })
}

/// Reads the frontmatter block of `path` without loading the skill body.
///
/// Returns `None` when the file does not open with `---\n` or the closing
/// `---` line is absent within [`MAX_FRONTMATTER_BYTES`].
fn read_frontmatter_block(path: &Path) -> std::io::Result<Option<String>> {
    let mut file = fs::File::open(path)?;
    let mut buf = vec![0_u8; MAX_FRONTMATTER_BYTES];
    let mut filled = 0;
    loop {
        let read = file.read(&mut buf[filled..])?;
        if read == 0 {
            break;
        }
        filled += read;
        if filled == MAX_FRONTMATTER_BYTES {
            break;
        }
    }
    buf.truncate(filled);
    let Ok(text) = String::from_utf8(buf) else {
        return Ok(None);
    };
    Ok(frontmatter(&text).map(str::to_owned))
}

/// Extracts the YAML frontmatter block: the text after a leading `---` line up
/// to the next line that is exactly `---`.
///
/// Anything else — a missing opening marker, an unterminated block — is
/// malformed and returns `None`.
#[must_use]
pub fn frontmatter(text: &str) -> Option<&str> {
    let rest = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))?;
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end_matches(['\n', '\r']) == "---" {
            return Some(&rest[..offset]);
        }
        offset += line.len();
    }
    None
}

/// Repository-relative, forward-slashed path of `path`.
fn relative_path(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.to_string_lossy().replace('\\', "/")
}

/// Frontmatter fields the selector uses; every other key is ignored.
#[derive(Debug, Default, Deserialize)]
struct Frontmatter {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    metadata: MetadataBlock,
}

/// `metadata:` sub-block; unknown keys (`version`, `tags`, ...) are ignored.
#[derive(Debug, Default, Deserialize)]
struct MetadataBlock {
    #[serde(default, rename = "short-description")]
    short_description: Option<String>,
}

//! Writes distilled heuristics into a skill's corpus on disk.
//!
//! Two idempotent operations keep the skill's instructions referencing the
//! accumulated distillations:
//!   * append a bullet to `<skill>/references/heuristics.md`
//!   * ensure a `## Guides` pointer in `<skill>/SKILL.md` links to that file.

use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::Path;

use anyhow::{Context, Result};

const GUIDES_HEADER: &str = "## Guides\n";
const POINTER_LINE: &str = "See references/heuristics.md for distilled heuristics.\n";
const HEURISTICS_HEADER: &str = "# Heuristics\n";

/// Returns the `references/heuristics.md` path for a skill under `root`.
fn heuristics_path(root: &Path, skill: &str) -> std::path::PathBuf {
    root.join(".agents")
        .join("skills")
        .join(skill)
        .join("references")
        .join("heuristics.md")
}

/// Appends a bullet for `pattern` to the skill's `references/heuristics.md`,
/// creating the file plus a header when it does not exist yet.
///
/// The bullet is written in append mode (`O_APPEND`) so concurrent distills
/// never clobber each other and a crash mid-write cannot truncate previously
/// distilled content. A missing file is seeded with the header; any other read
/// error (permissions, wrong file type) propagates instead of being treated as
/// "absent" and overwriting the file.
///
/// # Errors
///
/// Returns an error when the file cannot be created or appended to, or when
/// its existence cannot be determined.
pub fn append_heuristic(
    root: &Path,
    skill: &str,
    pattern: &str,
    description: Option<&str>,
    trace_id: i64,
) -> Result<()> {
    let path = heuristics_path(root, skill);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let bullet = format!(
        "- **{pattern}**: {} (from trace {trace_id})\n",
        description.unwrap_or("no description")
    );
    match fs::read_to_string(&path) {
        Ok(existing) => {
            let mut file = OpenOptions::new()
                .append(true)
                .open(&path)
                .with_context(|| format!("failed to open {}", path.display()))?;
            if existing.is_empty() {
                write!(file, "{HEURISTICS_HEADER}")
                    .with_context(|| format!("failed to append header to {}", path.display()))?;
            }
            write!(file, "{bullet}")
                .with_context(|| format!("failed to append bullet to {}", path.display()))?;
        }
        Err(err) if err.kind() == ErrorKind::NotFound => {
            fs::write(&path, format!("{HEURISTICS_HEADER}{bullet}"))?;
        }
        Err(err) => {
            return Err(err).context(format!("failed to read {}", path.display()));
        }
    }
    Ok(())
}

/// Ensures `SKILL.md` carries a `## Guides` pointer to `references/heuristics.md`,
/// adding one once — near the top but after any YAML frontmatter — rather than
/// duplicating it on subsequent runs.
///
/// # Errors
///
/// Returns an error when the skill file cannot be read or written.
pub fn ensure_skill_pointer(root: &Path, skill: &str) -> Result<()> {
    let skill_md = root
        .join(".agents")
        .join("skills")
        .join(skill)
        .join("SKILL.md");
    let original = fs::read_to_string(&skill_md)?;
    if original.contains(POINTER_LINE) {
        return Ok(());
    }
    let insertion = format!("{GUIDES_HEADER}{POINTER_LINE}");
    let updated = match frontmatter_end(&original) {
        Some(after) => {
            let mut out = String::with_capacity(original.len() + insertion.len());
            out.push_str(&original[..after]);
            out.push_str(&insertion);
            out.push_str(&original[after..]);
            out
        }
        None => format!("{insertion}{original}"),
    };
    // Write to a sibling temp file and rename so a crash mid-write cannot
    // leave a truncated SKILL.md; rename is atomic on POSIX.
    let tmp = skill_md.with_extension("md.tmp");
    fs::write(&tmp, updated).with_context(|| format!("failed to write {}", tmp.display()))?;
    fs::rename(&tmp, &skill_md)
        .with_context(|| format!("failed to replace {}", skill_md.display()))?;
    Ok(())
}

/// Returns the byte offset just past a leading `---`...`---\n` frontmatter
/// block (consuming the newline that closes it), or `None` when the file does
/// not start with one.
fn frontmatter_end(content: &str) -> Option<usize> {
    let remainder = content.strip_prefix("---\n")?;
    let closing = remainder.find("\n---")?;
    Some(4 + closing + 5)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::fs;

    use super::*;

    const FRONTMATTER_MD: &str =
        "---\nname: harness\ndescription: A skill.\nlicense: MIT\n---\n\n# Harness Skill\n";

    #[test]
    fn frontmatter_end_skips_the_closing_delimiter() {
        assert_eq!(
            frontmatter_end(FRONTMATTER_MD),
            Some(FRONTMATTER_MD.find("\n---").unwrap() + 5)
        );
        assert!(frontmatter_end("# No frontmatter\n").is_none());
    }

    #[test]
    fn pointer_lands_on_its_own_line_after_frontmatter() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join(".agents/skills/harness");
        fs::create_dir_all(&skill_dir).unwrap();
        let path = skill_dir.join("SKILL.md");
        fs::write(&path, FRONTMATTER_MD).unwrap();

        ensure_skill_pointer(dir.path(), "harness").unwrap();

        let body = fs::read_to_string(&path).unwrap();
        assert!(body.contains("---\n## Guides\n"));
        assert!(!body.contains("---## Guides")); // newline not glued to frontmatter
        assert!(
            body.contains(
                "See references/heuristics.md for distilled heuristics.\n\n# Harness Skill"
            )
        );
    }

    #[test]
    fn pointer_is_not_duplicated_on_second_run() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join(".agents/skills/harness");
        fs::create_dir_all(&skill_dir).unwrap();
        let path = skill_dir.join("SKILL.md");
        fs::write(&path, FRONTMATTER_MD).unwrap();

        ensure_skill_pointer(dir.path(), "harness").unwrap();
        ensure_skill_pointer(dir.path(), "harness").unwrap();

        let body = fs::read_to_string(&path).unwrap();
        assert_eq!(body.matches(POINTER_LINE).count(), 1);
    }

    #[test]
    fn append_seeds_header_only_when_creating() {
        let dir = tempfile::tempdir().unwrap();
        append_heuristic(dir.path(), "harness", "p1", Some("d1"), 7).unwrap();
        let path = dir
            .path()
            .join(".agents/skills/harness/references/heuristics.md");
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "# Heuristics\n- **p1**: d1 (from trace 7)\n"
        );
    }

    /// Appends never rewrite existing content: a second distill adds a line
    /// and keeps the first bullet intact, and no temp files linger.
    #[test]
    fn append_preserves_earlier_bullets() {
        let dir = tempfile::tempdir().unwrap();
        append_heuristic(dir.path(), "harness", "p1", None, 7).unwrap();
        append_heuristic(dir.path(), "harness", "p2", Some("d2"), 8).unwrap();
        let path = dir
            .path()
            .join(".agents/skills/harness/references/heuristics.md");
        let body = fs::read_to_string(&path).unwrap();
        assert!(body.contains("- **p1**: no description (from trace 7)\n"));
        assert!(body.contains("- **p2**: d2 (from trace 8)\n"));
        assert_eq!(body.matches("# Heuristics\n").count(), 1);
        let references = path.parent().unwrap();
        assert_eq!(fs::read_dir(references).unwrap().count(), 1);
    }

    /// An empty existing file gets the header before the first bullet.
    #[test]
    fn append_to_empty_existing_file_seeds_header() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir
            .path()
            .join(".agents/skills/harness/references/heuristics.md");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "").unwrap();
        append_heuristic(dir.path(), "harness", "p1", None, 3).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "# Heuristics\n- **p1**: no description (from trace 3)\n"
        );
    }

    /// The SKILL.md pointer write leaves no `.md.tmp` sibling behind.
    #[test]
    fn pointer_write_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join(".agents/skills/harness");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), FRONTMATTER_MD).unwrap();

        ensure_skill_pointer(dir.path(), "harness").unwrap();

        assert_eq!(fs::read_dir(&skill_dir).unwrap().count(), 1);
    }
}

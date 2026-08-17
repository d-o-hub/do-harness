//! Writes distilled heuristics into a skill's corpus on disk.
//!
//! Two idempotent operations keep the skill's instructions referencing the
//! accumulated distillations:
//!   * append a bullet to `<skill>/references/heuristics.md`
//!   * ensure a `## Guides` pointer in `<skill>/SKILL.md` links to that file.

use std::fs;
use std::path::Path;

use anyhow::Result;

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
/// # Errors
///
/// Returns an error when the file cannot be created or appended to.
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
    let existing = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(_) => HEURISTICS_HEADER.to_string(),
    };
    fs::write(&path, format!("{existing}{bullet}"))?;
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
    fs::write(&skill_md, updated)?;
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
    #![allow(clippy::unwrap_used)]

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
}

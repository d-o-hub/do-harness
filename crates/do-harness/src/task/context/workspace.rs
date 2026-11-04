//! Workspace context, skill ranking, and safe text clipping.

use std::path::Path;

use super::{
    Bounded, ChangedPath, MAX_CHANGED_PATHS, MAX_FIELD_CHARS, MAX_SKILLS, SkillItem, Workspace,
};

/// Bounded branch/workspace context.
pub(super) fn workspace_view(root: &Path) -> Workspace {
    let changes = crate::changes::discover(root);
    let mut files: Vec<&crate::changes::ChangedFile> = changes.files.iter().collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let total = files.len();
    let items = files
        .into_iter()
        .take(MAX_CHANGED_PATHS)
        .map(|file| ChangedPath {
            path: clean(&file.path, MAX_FIELD_CHARS),
            kind: change_kind(&file.kind),
        })
        .collect();
    Workspace {
        git_revision: crate::provenance::git_revision(root),
        branch: crate::changes::current_branch(root),
        scope: "workspace",
        changed: Bounded::new(items, total),
        discovery_failed: changes.discovery_failed,
        discovery_error: changes.discovery_error,
    }
}

fn change_kind(kind: &crate::changes::ChangeKind) -> &'static str {
    match kind {
        crate::changes::ChangeKind::Added => "added",
        crate::changes::ChangeKind::Modified => "modified",
        crate::changes::ChangeKind::Deleted => "deleted",
        crate::changes::ChangeKind::Renamed { .. } => "renamed",
        crate::changes::ChangeKind::Untracked => "untracked",
    }
}

/// Ranks skill metadata; never reads a skill body.
pub(super) fn ranked_skills(
    root: &Path,
    query: &str,
) -> (Bounded<SkillItem>, Bounded<String>, Option<anyhow::Error>) {
    let catalog = match crate::skills::catalog::scan_read_only(root) {
        Ok(catalog) => catalog,
        Err(err) => {
            return (
                Bounded::new(Vec::new(), 0),
                Bounded::new(Vec::new(), 0),
                Some(err),
            );
        }
    };
    let ranked = crate::skills::suggest::rank(&catalog.skills, query, MAX_SKILLS);
    let total = catalog.skills.len();
    let items: Vec<SkillItem> = ranked
        .into_iter()
        .map(|candidate| SkillItem {
            name: candidate.name,
            path: candidate.path,
            score: candidate.score,
            short_description: candidate.short_description,
        })
        .collect();
    let paths: Vec<String> = items.iter().map(|skill| skill.path.clone()).collect();
    let paths_len = paths.len();
    (
        Bounded::new(items, total),
        Bounded::new(paths, paths_len),
        None,
    )
}

/// Clips untrusted text and removes control characters so a stored newline or
/// escape sequence cannot reshape the text view.
pub(super) fn clean(text: &str, max_chars: usize) -> String {
    let mut out = String::with_capacity(text.len().min(max_chars));
    for ch in text.chars().take(max_chars) {
        out.push(if ch.is_control() { ' ' } else { ch });
    }
    out.trim().to_owned()
}

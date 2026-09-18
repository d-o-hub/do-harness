//! Catalog, ranking, and cache cases.
//!
//! Fixtures (`write_skill`, `write_named`, `rank`) come from the parent module.

use super::*;

#[test]
fn exact_name_match_ranks_first() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_named(root, "pr-triage", "pr-triage", "Triage pull requests.");
    write_named(root, "harness", "harness", "Map sensors and guides.");

    let ranked = rank(root, "please run pr-triage on the queue", 5);
    assert_eq!(ranked[0].name, "pr-triage");
}

#[test]
fn description_terms_outrank_an_unrelated_skill() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_named(
        root,
        "wheel-builder",
        "wheel-builder",
        "Compile binary wheels for python packaging.",
    );
    write_named(
        root,
        "reviewer",
        "reviewer",
        "Review code changes and report defects.",
    );

    let ranked = rank(root, "review code changes", 5);
    assert_eq!(ranked[0].name, "reviewer");
    assert!(ranked[0].score > ranked[1].score);
}

#[test]
fn limit_excludes_lower_scoring_candidates() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_named(root, "alpha", "alpha", "alpha topic");
    write_named(root, "beta", "beta", "beta topic");
    write_named(root, "gamma", "gamma", "gamma topic");

    assert_eq!(rank(root, "topic", 2).len(), 2);
    assert!(rank(root, "topic", 0).is_empty());
}

#[test]
fn ties_break_deterministically_on_name_then_path() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    // Identical metadata: only the paths differ, so the tie must break on them.
    write_named(root, "z-b", "same", "identical");
    write_named(root, "z-a", "same", "identical");

    let catalog = catalog::scan(root).unwrap();
    // Duplicate names collapse by the documented rule: first path wins.
    assert_eq!(catalog.skills.len(), 1);
    assert_eq!(catalog.skills[0].path, ".agents/skills/z-a/SKILL.md");
}

#[test]
fn malformed_skill_warns_without_panicking() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_named(root, "good", "good", "A good skill.");
    write_skill(root, "broken", "name: [unclosed");
    write_skill(root, "no-description", "name: only-name");
    // No frontmatter block at all.
    let bare = root.join(SKILL_ROOT).join("bare");
    fs::create_dir_all(&bare).unwrap();
    fs::write(bare.join("SKILL.md"), "# no frontmatter\n").unwrap();

    let catalog = catalog::scan(root).unwrap();
    assert_eq!(catalog.skills.len(), 1);
    assert_eq!(catalog.skills[0].name, "good");
    assert_eq!(catalog.warnings.len(), 3);
}

#[test]
fn duplicate_names_keep_one_entry_and_warn() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_named(root, "a-first", "shared", "First copy.");
    write_named(root, "b-second", "shared", "Second copy.");

    let catalog = catalog::scan(root).unwrap();
    assert_eq!(catalog.skills.len(), 1);
    assert_eq!(catalog.skills[0].path, ".agents/skills/a-first/SKILL.md");
    assert_eq!(catalog.warnings.len(), 1);
    assert!(catalog.warnings[0].contains("duplicate skill name"));
    assert!(catalog.warnings[0].contains("b-second"));
}

#[test]
fn short_description_and_unknown_keys_are_tolerated() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_skill(
        root,
        "described",
        "name: described\ndescription: Long description here.\nmetadata:\n  short-description: Short one.\n  version: \"2.0\"\n  tags: x y z\nlicense: Apache-2.0\n",
    );

    let catalog = catalog::scan(root).unwrap();
    assert_eq!(
        catalog.skills[0].short_description.as_deref(),
        Some("Short one.")
    );
}

#[cfg(unix)]
#[test]
fn symlinked_alias_collapses_to_one_entry() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_named(root, "real", "real", "The real skill.");
    let alias = root.join(SKILL_ROOT).join("alias");
    std::os::unix::fs::symlink(root.join(SKILL_ROOT).join("real"), &alias).unwrap();

    let catalog = catalog::scan(root).unwrap();
    // Both directory entries resolve to the same SKILL.md, so the second is a
    // duplicate of the first rather than a second logical skill.
    assert_eq!(catalog.skills.len(), 1);
    assert_eq!(catalog.warnings.len(), 1);
}

#[cfg(unix)]
#[test]
fn path_escaping_skill_root_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let outside = tempfile::tempdir().unwrap();
    fs::write(
        outside.path().join("SKILL.md"),
        "---\nname: escaped\ndescription: Outside the root.\n---\n",
    )
    .unwrap();
    let skills = root.join(SKILL_ROOT);
    fs::create_dir_all(&skills).unwrap();
    std::os::unix::fs::symlink(outside.path(), skills.join("escape")).unwrap();

    let catalog = catalog::scan(root).unwrap();
    assert!(catalog.skills.is_empty());
    assert!(
        catalog.warnings[0].contains("outside"),
        "{:?}",
        catalog.warnings
    );
}

#[test]
fn missing_skill_root_yields_an_empty_catalog() {
    let temp = tempfile::tempdir().unwrap();
    let catalog = catalog::scan(temp.path()).unwrap();
    assert!(catalog.skills.is_empty());
    assert!(catalog.warnings.is_empty());
}

#[test]
fn tokenizer_drops_single_characters_and_duplicates() {
    let tokens = suggest::tokenize("Review  the PR! review, a b");
    assert_eq!(tokens, vec!["review", "the", "pr"]);
}

#[test]
fn empty_query_scores_zero_without_a_name_hit() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_named(root, "anything", "anything", "Some description.");
    let ranked = rank(root, "   ", 5);
    assert!(ranked[0].score.abs() < f64::EPSILON);
}

#[test]
fn metadata_never_leaks_a_skill_body() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_named(root, "leaky", "leaky", "Describe it.");
    let catalog = catalog::scan(root).unwrap();
    let json = serde_json::to_string(&catalog.skills).unwrap();
    assert!(!json.contains("secret body text"));
}

#[test]
fn cache_is_reused_while_frontmatter_is_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_named(root, "cached", "cached", "A cached skill.");
    let cache_path = root.join(cache::CACHE_PATH);

    catalog::scan(root).unwrap();
    assert!(cache_path.is_file(), "first scan writes the cache");
    let first = fs::read_to_string(&cache_path).unwrap();

    catalog::scan(root).unwrap();
    assert_eq!(first, fs::read_to_string(&cache_path).unwrap());
    assert!(!first.contains("secret body text"));
}

#[test]
fn cache_invalidates_when_frontmatter_changes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_named(root, "changing", "changing", "First description.");
    assert_eq!(
        catalog::scan(root).unwrap().skills[0].description,
        "First description."
    );

    write_named(root, "changing", "changing", "Second description.");
    assert_eq!(
        catalog::scan(root).unwrap().skills[0].description,
        "Second description."
    );
}

#[test]
fn corrupt_cache_falls_back_to_a_fresh_scan() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_named(root, "survivor", "survivor", "Still found.");
    let cache_path = root.join(cache::CACHE_PATH);
    fs::create_dir_all(cache_path.parent().unwrap()).unwrap();
    fs::write(&cache_path, "{ not json at all").unwrap();

    let catalog = catalog::scan(root).unwrap();
    assert_eq!(catalog.skills.len(), 1);
    assert_eq!(catalog.skills[0].name, "survivor");
}

#[test]
fn stale_cache_schema_is_ignored() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_named(root, "versioned", "versioned", "Version check.");
    let cache_path = root.join(cache::CACHE_PATH);
    fs::create_dir_all(cache_path.parent().unwrap()).unwrap();
    fs::write(
        &cache_path,
        r#"{"schema_version":99,"scoring_version":1,"entries":{}}"#,
    )
    .unwrap();

    let catalog = catalog::scan(root).unwrap();
    assert_eq!(catalog.skills.len(), 1);
    assert!(
        catalog.warnings.iter().any(|w| w.contains("stale-schema")),
        "{:?}",
        catalog.warnings
    );
}

#[test]
fn cache_keeps_unchanged_skills_when_one_changes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_named(root, "stable", "stable", "Never edited.");
    write_named(root, "changing", "changing", "Before.");
    catalog::scan(root).unwrap();

    // Editing one skill must not drop the other from the cache, or it would
    // re-parse on every subsequent scan.
    write_named(root, "changing", "changing", "After.");
    catalog::scan(root).unwrap();

    let text = fs::read_to_string(root.join(cache::CACHE_PATH)).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
    let entries = parsed["entries"].as_object().unwrap();
    assert_eq!(entries.len(), 2, "{text}");
    assert!(text.contains("Never edited."));
    assert!(text.contains("After."));
}

#[test]
fn repeated_scans_are_byte_identical() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_named(root, "one", "one", "First skill about testing.");
    write_named(root, "two", "two", "Second skill about reviews.");

    let first = serde_json::to_string(&rank(root, "testing reviews", 5)).unwrap();
    let second = serde_json::to_string(&rank(root, "testing reviews", 5)).unwrap();
    assert_eq!(first, second);
}

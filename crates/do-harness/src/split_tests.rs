//! Unit tests for `do-harness split` (`split.rs` and `split/scan.rs`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::names::{raise_visibility, snake_case, validate_ident};
use super::scan::{ItemKind, parse_kind, scan_top_items};
use super::*;
use std::fmt::Write;
use std::path::PathBuf;

/// A scratch workspace with a `crates/` tree, returning (root, tempdir guard).
fn workspace() -> (PathBuf, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join("crates/app/src")).unwrap();
    (root, dir)
}

/// Writes `text` as `crates/app/src/<name>` and returns its path.
fn write_source(root: &Path, name: &str, text: &str) -> PathBuf {
    let path = root.join("crates/app/src").join(name);
    std::fs::write(&path, text).unwrap();
    path
}

/// A source file that is over the ceiling: `preamble` production items plus a
/// trailing inline test module of `tests` test functions.
fn over_limit(preamble_items: usize, body_lines: usize, tests: usize) -> String {
    let mut text = String::from("//! Fixture module.\n\n");
    for item in 0..preamble_items {
        writeln!(text, "fn item_{item}() {{").unwrap();
        for line in 0..body_lines {
            writeln!(text, "    let _v{line} = {line};").unwrap();
        }
        text.push_str("}\n\n");
    }
    text.push_str("#[cfg(test)]\nmod tests {\n    use super::*;\n\n");
    for test in 0..tests {
        writeln!(
            text,
            "    #[test]\n    fn t_{test}() {{\n        assert!(true);\n    }}\n"
        )
        .unwrap();
    }
    text.push_str("}\n");
    text
}

// --- scanning -------------------------------------------------------------

#[test]
fn parse_kind_reads_modifiers_and_nested_gracefully() {
    assert_eq!(parse_kind("fn a() {}"), Some(ItemKind::Fn));
    assert_eq!(parse_kind("pub(crate) fn a() {}"), Some(ItemKind::Fn));
    assert_eq!(parse_kind("pub async fn a() {}"), Some(ItemKind::Fn));
    assert_eq!(parse_kind("impl Foo {"), Some(ItemKind::Impl));
    assert_eq!(parse_kind("pub struct Foo;"), Some(ItemKind::Struct));
    assert_eq!(parse_kind("macro_rules! m {"), Some(ItemKind::MacroRules));
    assert_eq!(
        parse_kind("    fn nested() {}"),
        None,
        "nested is not top level"
    );
    assert_eq!(
        parse_kind("pub use crate::x;"),
        None,
        "use items are not movable"
    );
    assert_eq!(parse_kind("let x = 1;"), None);
}

#[test]
fn scan_finds_spans_including_attributes() {
    let text = "\
//! header

/// Doc for alpha.
#[derive(Debug)]
pub struct Alpha {
    pub x: u32,
}

pub fn beta() {
    fn nested() {}
}

const GAMMA: u32 = 1;
";
    let lines: Vec<&str> = text.lines().collect();
    let items = scan_top_items(&lines).unwrap();
    let spans: Vec<(usize, usize, ItemKind)> = items
        .iter()
        .map(|item| (item.start, item.end, item.kind))
        .collect();
    assert_eq!(
        spans,
        vec![
            (2, 6, ItemKind::Struct),
            (8, 10, ItemKind::Fn),
            (12, 12, ItemKind::Const),
        ],
        "doc attribute belongs to the struct; nested fn belongs to beta"
    );
}

#[test]
fn scan_ignores_braces_inside_strings_and_comments() {
    let text = "\
fn noisy() {
    let s = \"} not a close {\";
    let r = r#\"} still not\"#;
    let c = '}';
    // } trailing comment
    /* } block comment */
}

fn next() {}
";
    let lines: Vec<&str> = text.lines().collect();
    let items = scan_top_items(&lines).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!((items[0].start, items[0].end), (0, 6));
    assert_eq!((items[1].start, items[1].end), (8, 8));
}

#[test]
fn unbalanced_braces_are_reported() {
    let lines: Vec<&str> = vec!["fn broken() {", "    let _ = 1;"];
    assert!(
        scan_top_items(&lines)
            .unwrap_err()
            .to_string()
            .contains("unbalanced braces in the item starting at line 1")
    );
}

// --- helpers --------------------------------------------------------------

#[test]
fn snake_case_splits_camel_and_acronyms() {
    assert_eq!(snake_case("renderEvalsJson"), "render_evals_json");
    assert_eq!(snake_case("RESOLVE_BIN"), "resolve_bin");
    assert_eq!(snake_case("HttpServer"), "http_server");
    assert_eq!(snake_case("already_snake"), "already_snake");
}

#[test]
fn visibility_is_raised_only_when_private() {
    assert_eq!(
        raise_visibility("fn a() {}", ItemKind::Fn),
        "pub(crate) fn a() {}"
    );
    assert_eq!(
        raise_visibility("pub fn a() {}", ItemKind::Fn),
        "pub fn a() {}"
    );
    assert_eq!(
        raise_visibility("pub(crate) fn a() {}", ItemKind::Fn),
        "pub(crate) fn a() {}"
    );
    assert_eq!(raise_visibility("impl Foo {", ItemKind::Impl), "impl Foo {");
}

#[test]
fn dedent_uses_the_smallest_indent() {
    let lines = vec!["        let a = 1;", "", "    let b = 2;"];
    assert_eq!(
        names::dedent(&lines),
        vec!["    let a = 1;", "", "let b = 2;"]
    );
}

#[test]
fn module_names_are_validated() {
    assert_eq!(
        validate_ident("render_evals_json").unwrap(),
        "render_evals_json"
    );
    assert!(validate_ident("mod").is_err(), "keyword");
    assert!(validate_ident("2fast").is_err(), "leading digit");
    assert!(validate_ident("has-dash").is_err(), "non-identifier");
    assert!(validate_ident("").is_err(), "empty");
}

// --- planning and applying ------------------------------------------------

#[test]
fn test_module_is_extracted_with_a_path_attribute() {
    let (root, _guard) = workspace();
    let source = over_limit(1, 200, 160);
    let path = write_source(&root, "big.rs", &source);
    assert!(source.lines().count() > 500);

    let lines: Vec<&str> = source.lines().collect();
    let plan = build_plan(&root, &path, &lines, None).unwrap();
    assert_eq!(plan.kind, SplitKind::Tests);
    assert_eq!(plan.target_file, "crates/app/src/big_tests.rs");
    assert_eq!(plan.what, "mod tests");
    assert!(
        plan.result_lines <= 500,
        "plan must bring the file under the ceiling: {}",
        plan.result_lines
    );
    assert_eq!(
        plan.declaration,
        vec!["#[cfg(test)]", "#[path = \"big_tests.rs\"]", "mod tests;"]
    );
    assert!(
        plan.target_text.contains("use super::*;"),
        "parent scope preserved"
    );
    assert!(plan.target_text.contains("fn t_159()"));

    run(&root, &path, false, None).unwrap();

    let rewritten = std::fs::read_to_string(&path).unwrap();
    assert!(rewritten.lines().count() <= 500);
    assert!(rewritten.contains("#[path = \"big_tests.rs\"]"));
    assert!(!rewritten.contains("fn t_159"), "tests moved out");
    let moved = std::fs::read_to_string(root.join("crates/app/src/big_tests.rs")).unwrap();
    assert!(moved.contains("fn t_159"));
    assert!(moved.contains("//! Tests for `crates/app/src/big.rs`"));
}

#[test]
fn test_extraction_refuses_when_it_would_not_fix_the_ceiling() {
    let (root, _guard) = workspace();
    let source = over_limit(3, 200, 3);
    let path = write_source(&root, "big.rs", &source);
    assert!(source.lines().count() > 500);
    let lines: Vec<&str> = source.lines().collect();

    // Tests alone cannot bring it under the ceiling, so the item path runs and
    // the trailing test module is protected from being the moved item.
    let plan = build_plan(&root, &path, &lines, None).unwrap();
    assert_eq!(plan.kind, SplitKind::Item);
    assert!(plan.result_lines <= 500);
}

#[test]
fn largest_item_is_extracted_and_re_exported() {
    let (root, _guard) = workspace();
    let mut source = String::from("//! Fixture module.\n\n");
    source.push_str("pub fn small() {}\n\n");
    source.push_str("pub(crate) fn load_config() {\n");
    for line in 0..300 {
        writeln!(source, "    let _v{line} = {line};").unwrap();
    }
    source.push_str("}\n\n");
    // Filler keeps the file over the ceiling without any single item exceeding it.
    for filler in 0..30 {
        writeln!(source, "/// Filler {filler}.\npub fn filler_{filler}() {{").unwrap();
        for line in 0..6 {
            writeln!(source, "    let _f{line} = {line};").unwrap();
        }
        source.push_str("}\n\n");
    }
    let path = write_source(&root, "cfg.rs", &source);
    assert!(source.lines().count() > 500);

    let lines: Vec<&str> = source.lines().collect();
    let plan = build_plan(&root, &path, &lines, None).unwrap();
    assert_eq!(plan.kind, SplitKind::Item);
    assert_eq!(plan.target_file, "crates/app/src/load_config.rs");
    assert_eq!(plan.what, "pub(crate) fn load_config() {");
    assert_eq!(
        plan.declaration,
        vec![
            "#[path = \"load_config.rs\"]",
            "mod load_config;",
            "pub(crate) use load_config::load_config;",
        ]
    );

    run(&root, &path, false, None).unwrap();
    let rewritten = std::fs::read_to_string(&path).unwrap();
    assert!(rewritten.lines().count() <= 500);
    assert!(rewritten.contains("#[path = \"load_config.rs\"]"));
    assert!(rewritten.contains("pub(crate) use load_config::load_config;"));
    assert!(!rewritten.contains("fn load_config()"), "moved out");
    assert!(
        rewritten.contains("pub fn filler_29()"),
        "filler stayed behind"
    );
    let moved = std::fs::read_to_string(root.join("crates/app/src/load_config.rs")).unwrap();
    assert!(moved.starts_with("//! `load_config` split out of `crates/app/src/cfg.rs`"));
    assert!(moved.contains("pub(crate) fn load_config() {"));
}

#[test]
fn private_item_gains_visibility_when_moved() {
    let (root, _guard) = workspace();
    let mut source = String::new();
    source.push_str("fn private_helper() {\n");
    for line in 0..290 {
        writeln!(source, "    let _v{line} = {line};").unwrap();
    }
    source.push_str("}\n\n");
    for filler in 0..30 {
        writeln!(source, "fn filler_{filler}() {{").unwrap();
        for line in 0..6 {
            writeln!(source, "    let _f{line} = {line};").unwrap();
        }
        source.push_str("}\n\n");
    }
    let path = write_source(&root, "helpers.rs", &source);
    assert!(source.lines().count() > 500);

    let lines: Vec<&str> = source.lines().collect();
    let plan = build_plan(&root, &path, &lines, None).unwrap();
    assert_eq!(
        plan.declaration.last().unwrap(),
        "pub(crate) use private_helper::private_helper;"
    );
    run(&root, &path, false, None).unwrap();
    let moved = std::fs::read_to_string(root.join("crates/app/src/private_helper.rs")).unwrap();
    assert!(
        moved.contains("pub(crate) fn private_helper() {"),
        "moved item must stay reachable from the parent module"
    );
}

#[test]
fn target_flag_names_the_sibling_module() {
    let (root, _guard) = workspace();
    let source = over_limit(1, 200, 170);
    let path = write_source(&root, "big.rs", &source);
    let lines: Vec<&str> = source.lines().collect();

    let plan = build_plan(&root, &path, &lines, Some("fixtures")).unwrap();
    assert_eq!(plan.target_file, "crates/app/src/fixtures.rs");
    assert_eq!(plan.declaration.last().unwrap(), "mod fixtures;");
}

#[test]
fn unsupported_shapes_are_refused_with_a_reason() {
    let (root, _guard) = workspace();
    let mut source = String::from("macro_rules! m { () => {}; }\n");
    source.push_str(&"fn filler() {}\n".repeat(520));
    let path = write_source(&root, "macros.rs", &source);
    let err = run(&root, &path, true, None).unwrap_err().to_string();
    assert!(err.contains("macro_rules!"), "{err}");

    let mut cfg_source = String::from("#[cfg(feature = \"x\")]\nfn gated() {}\n");
    cfg_source.push_str(&"fn filler() {}\n".repeat(520));
    let cfg_path = write_source(&root, "gated.rs", &cfg_source);
    let err = run(&root, &cfg_path, true, None).unwrap_err().to_string();
    assert!(err.contains("conditional compilation"), "{err}");
}

#[test]
fn files_outside_crates_and_under_the_ceiling_are_handled() {
    let (root, _guard) = workspace();
    let outside = root.join("elsewhere.rs");
    std::fs::write(&outside, "fn a() {}\n").unwrap();
    let err = run(&root, &outside, true, None).unwrap_err().to_string();
    assert!(err.contains("only operates on files under crates"), "{err}");

    let small = write_source(&root, "small.rs", "fn a() {}\n");
    run(&root, &small, false, None).unwrap();
    assert!(
        std::fs::read_to_string(&small)
            .unwrap()
            .contains("fn a() {}")
    );
}

#[test]
fn dry_run_writes_nothing() {
    let (root, _guard) = workspace();
    let source = over_limit(1, 200, 160);
    let path = write_source(&root, "big.rs", &source);
    run(&root, &path, true, None).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
    assert!(!root.join("crates/app/src/big_tests.rs").exists());
}

#[test]
fn existing_target_file_is_not_overwritten() {
    let (root, _guard) = workspace();
    let source = over_limit(1, 200, 160);
    let path = write_source(&root, "big.rs", &source);
    write_source(&root, "big_tests.rs", "//! existing\n");
    let err = run(&root, &path, false, None).unwrap_err().to_string();
    assert!(err.contains("already exists"), "{err}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
    assert_eq!(
        std::fs::read_to_string(root.join("crates/app/src/big_tests.rs")).unwrap(),
        "//! existing\n"
    );
}

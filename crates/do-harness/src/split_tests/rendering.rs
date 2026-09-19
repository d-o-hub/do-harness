//! Render-shape and regression tests for `do-harness split`.
//!
//! Split out of the parent test module when it crossed the ceiling. These cases
//! assert the *shape* of the generated sibling file — token interiors, inner
//! attributes, glob injection, and refusal to overwrite — as opposed to the
//! planning logic the parent covers.

use super::*;

/// A moved test module must not have the interior of a multi-line token
/// re-indented: that silently rewrites the literal's value. Regression for the
/// bug where `dedent` stripped indentation line by line.
#[test]
fn extraction_preserves_multiline_token_interiors() {
    let (root, _guard) = workspace();
    let mut source = String::from("//! Fixture module.\n\nfn anchor() {}\n\n");
    source.push_str("#[cfg(test)]\nmod tests {\n    use super::*;\n\n");
    source.push_str("    #[test]\n    fn plain_literal() {\n");
    source.push_str("        let expected = \"alpha\n        beta\";\n");
    source.push_str("        assert_eq!(expected, \"alpha\\n        beta\");\n    }\n\n");
    source.push_str("    #[test]\n    fn raw_literal() {\n");
    source.push_str("        let sql = r#\"SELECT\n        * FROM t\"#;\n");
    source.push_str("        assert!(sql.contains(\"\\n        *\"));\n    }\n\n");
    source.push_str("    #[test]\n    fn block_comment() {\n");
    source.push_str("        /* leading\n           interior */\n");
    source.push_str("        assert_eq!(1, 1);\n    }\n");
    // Filler pushes the file over the ceiling with the tests still removable.
    for filler in 0..120 {
        writeln!(
            source,
            "    #[test]\n    fn filler_{filler}() {{\n        assert!(true);\n    }}\n"
        )
        .unwrap();
    }
    source.push_str("}\n");
    let path = write_source(&root, "tokens.rs", &source);
    assert!(source.lines().count() > 500);

    let lines: Vec<&str> = source.lines().collect();
    let plan = build_plan(&root, &path, &lines, None).unwrap();
    assert_eq!(plan.kind, SplitKind::Tests);

    let moved: Vec<&str> = plan.target_text.lines().collect();
    assert!(
        moved.contains(&"        beta\";"),
        "interior line of a plain string literal must keep its whitespace"
    );
    assert!(
        moved.contains(&"        * FROM t\"#;"),
        "interior line of a raw string literal must keep its whitespace"
    );
    assert!(
        moved.contains(&"           interior */"),
        "interior line of a block comment must keep its whitespace"
    );
    assert!(
        moved.contains(&"    assert_eq!(expected, \"alpha\\n        beta\");"),
        "code lines outside tokens are still dedented to module level"
    );
}

/// An inner attribute must stay ahead of any injected `use`: inner attributes
/// are only legal before the module's items, so emitting the glob first would
/// produce a file that does not parse. Regression for the bug found in review.
#[test]
fn extraction_keeps_inner_attributes_first() {
    let (root, _guard) = workspace();
    let mut source = String::from("//! Fixture module.\n\nfn anchor() {}\n\n");
    // No `use super::*;`, so the glob is injected.
    source.push_str("#[cfg(test)]\nmod tests {\n    #![allow(unused)]\n\n");
    for filler in 0..120 {
        writeln!(
            source,
            "    #[test]\n    fn filler_{filler}() {{\n        assert!(true);\n    }}\n"
        )
        .unwrap();
    }
    source.push_str("}\n");
    let path = write_source(&root, "inner.rs", &source);

    let lines: Vec<&str> = source.lines().collect();
    let plan = build_plan(&root, &path, &lines, None).unwrap();
    assert_eq!(plan.kind, SplitKind::Tests);

    let rendered: Vec<&str> = plan.target_text.lines().collect();
    let attr = rendered
        .iter()
        .position(|line| *line == "#![allow(unused)]")
        .expect("inner attribute survives the move");
    let glob = rendered
        .iter()
        .position(|line| *line == "use super::*;")
        .expect("glob is injected when the body has none");
    assert!(
        attr < glob,
        "inner attribute must precede the injected glob: {rendered:#?}"
    );
}

/// When the moved body already globs the parent, no second glob is injected.
#[test]
fn extraction_does_not_duplicate_an_existing_glob() {
    let (root, _guard) = workspace();
    let mut source = String::from("//! Fixture module.\n\nfn anchor() {}\n\n");
    source.push_str("#[cfg(test)]\nmod tests {\n    #![allow(unused)]\n\n    use super::*;\n\n");
    for filler in 0..120 {
        writeln!(
            source,
            "    #[test]\n    fn filler_{filler}() {{\n        assert!(true);\n    }}\n"
        )
        .unwrap();
    }
    source.push_str("}\n");
    let path = write_source(&root, "existing_glob.rs", &source);

    let lines: Vec<&str> = source.lines().collect();
    let plan = build_plan(&root, &path, &lines, None).unwrap();
    assert_eq!(
        plan.target_text.matches("use super::*;").count(),
        1,
        "exactly one glob: {:#?}",
        plan.target_text
    );
    let rendered: Vec<&str> = plan.target_text.lines().collect();
    assert_eq!(rendered[1], "#![allow(unused)]", "attribute stays first");
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
